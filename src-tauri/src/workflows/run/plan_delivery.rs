//! Plan and story delivery, built only from neutral catalog nodes: a bounded
//! correction loop (implement, verify, review) and one delivery commit.
//!
//! Every step is an ordinary saved script operation or subflow, so the
//! Designer edits the published definition like any other neutral workflow.
use crate::workflows::WorkflowStore;
use crate::workflows::catalog::{
    ActivityDescriptor, DescriptorId, HandlerRef, ReusableTarget, WorkflowCatalog, content_digest,
};
use crate::workflows::command::Invocation;
use crate::workflows::neutral_admission::{HostContext, HostRegistry, NeutralAdmissionService};
use crate::workflows::neutral_api::model::{EditorMetadata, NeutralDraft};
use crate::workflows::neutral_publication::{
    NeutralDefinition, NeutralPublication, PublicationPin, PublicationRef,
};
use serde_json::{Value, json};

/// Draft name; the installer finds an existing installation by it.
pub(crate) const NAME: &str = "Plan and story delivery";
/// Every run of this workflow carries `<prefix><story id>:<caller key>` as its
/// request identity, so a story lists and resumes its own runs.
pub(crate) const RUN_PREFIX: &str = "story-delivery:";
const NAMESPACE: &str = "delivery";
const ATTEMPT: &str = "delivery-attempt";
/// Attempts before the loop is exhausted; the Designer can change it.
const MAX_ATTEMPTS: u32 = 3;

const EGO_WORKSPACE: &[&str] = &["ego", "run", "--sandbox", "workspace", "-"];
const EGO_READ_ONLY: &[&str] = &["ego", "run", "--sandbox", "ro", "-"];
// Whitespace errors only. The project check command is set in the Designer.
const GIT_CHECK: &[&str] = &["git", "diff", "--check", "HEAD"];

/// The external commands each step runs in the workspace. The prompt goes to
/// stdin of the implementer and the reviewer.
#[derive(Clone, Debug)]
pub(crate) struct Commands {
    pub implement: Vec<String>,
    pub verify: Vec<String>,
    pub review: Vec<String>,
}

impl Default for Commands {
    fn default() -> Self {
        let owned = |argv: &[&str]| argv.iter().map(|s| (*s).to_owned()).collect();
        Self {
            implement: owned(EGO_WORKSPACE),
            verify: owned(GIT_CHECK),
            review: owned(EGO_READ_ONLY),
        }
    }
}

const PRELUDE: &str = r#"import hashlib,json,os,subprocess,sys
q=json.load(sys.stdin)
c=q['config'];i=q['inputs']
def done(**data):
    print(json.dumps(dict(version=1,identity=q['identity'],outcome='ok',data=data,artifacts={})))
def tail(t):
    return t[-6000:]
def story():
    s=i['story']
    lines=['Story '+str(s.get('id',''))+': '+str(s.get('title',''))]
    if s.get('problem'): lines+=['','Problem:',str(s['problem'])]
    if s.get('criteria'): lines+=['','Acceptance criteria:']+['- '+str(x) for x in s['criteria']]
    return '\n'.join(lines)
def agent(prompt):
    r=subprocess.run(c['command'],input=prompt,capture_output=True,text=True)
    sys.stderr.write(tail(r.stderr))
    if r.returncode: sys.exit(r.returncode)
    return r.stdout
def git(*a):
    return subprocess.run(['git',*a],capture_output=True,text=True)
def snapshot():
    # Every path git reports as changed or untracked, with its content digest.
    out=subprocess.run(['git','status','--porcelain=v1','-z','-uall','--no-renames'],capture_output=True).stdout
    state={}
    for e in out.decode('utf-8','surrogateescape').split('\0'):
        if len(e)<4: continue
        try:
            with open(e[3:],'rb') as f: h=hashlib.sha256(f.read()).hexdigest()
        except OSError: h='missing'
        state[e[3:]]=e[:2]+h
    return state
"#;

const IMPLEMENT: &str = r#"p='Implement this story in the current repository. Edit files only; do not commit.\n\n'+story()
if i['findings']: p+='\n\nThe previous attempt was rejected. Correct these findings:\n'+json.dumps(i['findings'],indent=2)
before=snapshot()
out=agent(p)
after=snapshot()
new={x for x in set(before)|set(after) if before.get(x)!=after.get(x)}
done(summary=tail(out),changed=sorted(set(i['changed'])|new))
"#;

const VERIFY: &str = r#"r=subprocess.run(c['command'],stdin=subprocess.DEVNULL,capture_output=True,text=True)
done(passed=r.returncode==0,report=tail(r.stdout+r.stderr))
"#;

const REVIEW: &str = r#"v=i['verification']
p=('Review the uncommitted change against the story. Reply APPROVE on the first line only when it satisfies every criterion; otherwise reply FIX on the first line, then the findings.\n\n'+story()
  +'\n\nVerification passed: '+str(v['passed'])+'\n'+v['report']
  +'\n\nChanged files:\n'+git('status','--porcelain').stdout+'\nDiff:\n'+tail(git('diff','HEAD').stdout))
out=agent(p).strip()
first=out.splitlines()[0].strip().upper() if out else ''
done(approved=first.startswith('APPROVE'),findings=tail(out))
"#;

const DELIVER: &str = r#"s=i['story']
# Only paths an attempt changed; files that were there before the run stay out.
current=snapshot()
paths=[x for x in i['changed'] if x in current]
os.environ['GIT_LITERAL_PATHSPECS']='1'
if paths: git('add','-A','--',*paths)
if git('diff','--cached','--quiet').returncode==0:
    sys.stderr.write('nothing to deliver'); sys.exit(1)
r=git('commit','-q','-m',str(s.get('title') or 'Story delivery')+' (#'+str(s['id'])+')')
if r.returncode:
    sys.stderr.write(r.stderr); sys.exit(1)
done(commit=git('rev-parse','HEAD').stdout.strip())
"#;

fn id(name: &str) -> DescriptorId {
    DescriptorId {
        namespace: NAMESPACE.into(),
        name: name.into(),
        version: 1,
    }
}

fn field(kind: &str) -> Value {
    json!({"value_type": kind, "required": true, "default": null})
}

fn workspace() -> Value {
    json!({"required": true, "fields": {"fields": {"workspace": field("workspace_reference")}}})
}

struct Step {
    name: &'static str,
    purpose: &'static str,
    body: &'static str,
    inputs: Value,
    outputs: Value,
    command: Option<&'static [&'static str]>,
    /// Present for a step that writes the workspace: the manual recovery note.
    mutation: Option<&'static str>,
    timeout_secs: u32,
}

fn script(project: &str, step: &Step) -> Result<ActivityDescriptor, String> {
    let mut config = serde_json::Map::new();
    if let Some(command) = step.command {
        config.insert(
            "command".into(),
            json!({"value_type": "array", "required": true, "default": command}),
        );
    }
    let contract: ActivityDescriptor = serde_json::from_value(json!({
        "id": id(step.name), "purpose": step.purpose,
        "handler": {"name": "host.script", "version": 1}, "target": null,
        "config": {"fields": config}, "inputs": {"fields": step.inputs},
        "subject": workspace(), "outputs": {"fields": step.outputs},
        "outcomes": ["ok"], "required_capabilities": [], "execution_policies": ["local"],
    }))
    .map_err(|e| e.to_string())?;
    let policy = serde_json::from_value(json!({
        "version": 1, "timeout_secs": step.timeout_secs, "input_bytes": 262_144,
        "stdout_bytes": 65_536, "stderr_bytes": 65_536, "artifact_limit": 0,
        "resources": if step.mutation.is_some() {
            json!([format!("plan-delivery:workspace:{project}")])
        } else {
            json!([])
        },
        "environment_refs": {}, "secret_refs": {}, "mutation": step.mutation.is_some(),
        "retry": "never",
        "reconciliation": match step.mutation {
            Some(procedure) => json!({"type": "manual", "procedure": procedure}),
            None => json!({"type": "read_only"}),
        },
        "output": {"type": "structured"},
    }))
    .map_err(|e| e.to_string())?;
    super::neutral_script::script_descriptor(
        contract,
        Invocation::Shell {
            program: if cfg!(windows) { "python" } else { "python3" }.into(),
            script: format!("{PRELUDE}{}", step.body),
            version: 1,
        },
        &policy,
    )
}

/// The four saved script operations of the workflow.
pub(crate) fn operations(project: &str) -> Result<Vec<ActivityDescriptor>, String> {
    [
        Step {
            name: "implement",
            purpose: "Implement the story in the workspace with an agent command",
            body: IMPLEMENT,
            inputs: json!({
                "story": field("object"), "findings": field("object"), "changed": field("array"),
            }),
            outputs: json!({"summary": field("string"), "changed": field("array")}),
            command: Some(EGO_WORKSPACE),
            mutation: Some("Inspect the workspace diff; the implementer may have changed files"),
            timeout_secs: 3600,
        },
        Step {
            name: "verify",
            purpose: "Run the project verification command; a nonzero exit fails the gate",
            body: VERIFY,
            inputs: json!({}),
            outputs: json!({"passed": field("boolean"), "report": field("string")}),
            command: Some(GIT_CHECK),
            mutation: None,
            timeout_secs: 1800,
        },
        Step {
            name: "review",
            purpose: "Review the uncommitted change against the story with an agent command",
            body: REVIEW,
            inputs: json!({"story": field("object"), "verification": field("object")}),
            outputs: json!({"approved": field("boolean"), "findings": field("string")}),
            command: Some(EGO_READ_ONLY),
            mutation: None,
            timeout_secs: 1800,
        },
        Step {
            name: "deliver",
            purpose: "Commit the paths the attempts changed, once",
            body: DELIVER,
            inputs: json!({"story": field("object"), "changed": field("array")}),
            outputs: json!({"commit": field("string")}),
            command: None,
            mutation: Some("Read git log for the story delivery commit before any new run"),
            timeout_secs: 120,
        },
    ]
    .iter()
    .map(|step| script(project, step))
    .collect()
}

fn activity(name: &str, command: Option<&[String]>) -> Value {
    json!({
        "descriptor": id(name),
        "config": command.map_or(json!({}), |c| json!({"command": c})),
        "execution_policy": "local",
    })
}

fn output(activity: usize, field: &str) -> Value {
    json!({"source": "activity_output", "activity": activity, "field": field})
}

fn input(field: &str) -> Value {
    json!({"source": "workflow_input", "field": field})
}

/// One attempt: implement, verify, then review the uncommitted change.
pub(crate) fn attempt(commands: &Commands) -> Result<NeutralDefinition, String> {
    serde_json::from_value(json!({
        "inputs": {"fields": {
            "story": field("object"), "findings": field("object"), "changed": field("array"),
        }},
        "subject": workspace(),
        "outputs": {"fields": {
            "passed": field("boolean"), "approved": field("boolean"), "findings": field("object"),
            "changed": field("array"),
        }},
        "outcomes": ["attempted"],
        "instances": [
            activity("implement", Some(&commands.implement)),
            activity("verify", Some(&commands.verify)),
            activity("review", Some(&commands.review)),
        ],
        "input_bindings": {
            "0": {
                "story": input("story"), "findings": input("findings"), "changed": input("changed"),
            },
            "1": {},
            "2": {"story": input("story"), "verification": {"source": "object", "fields": {
                "passed": output(1, "passed"), "report": output(1, "report"),
            }}},
        },
        "output_bindings": {
            "passed": output(1, "passed"),
            "approved": output(2, "approved"),
            "changed": output(0, "changed"),
            "findings": {"source": "object", "fields": {
                "verification": output(1, "report"), "review": output(2, "findings"),
            }},
        },
        "conditions": {}, "collections": {}, "loops": {}, "forks": {}, "subflows": [],
        "control": {
            "nodes": {
                "start": {"type": "start"},
                "implement": {"type": "activity", "activity": 0},
                "verify": {"type": "activity", "activity": 1},
                "review": {"type": "activity", "activity": 2},
                "end": {"type": "end", "outcome": "attempted"},
            },
            "edges": {
                "start": {"next": "implement"},
                "implement": {"ok": "verify"},
                "verify": {"ok": "review"},
                "review": {"ok": "end"},
            },
        },
    }))
    .map_err(|e| e.to_string())
}

/// Repeat the attempt until verification passed and the review approved; only
/// then deliver. Any other loop result ends blocked without a commit.
pub(crate) fn root() -> Result<NeutralDefinition, String> {
    // The loop repeats while the verdict is one of these rejected pairs. A
    // missing value is Unknown, which routes to blocked, never to deliver.
    let rejected: Vec<Value> = [(false, false), (false, true), (true, false)]
        .into_iter()
        .map(|(passed, approved)| json!({"passed": passed, "approved": approved}))
        .collect();
    serde_json::from_value(json!({
        "inputs": {"fields": {"story": field("object")}},
        "subject": workspace(),
        "outputs": {"fields": {}},
        "outcomes": ["delivered", "blocked"],
        "instances": [activity(ATTEMPT, None), activity("deliver", None)],
        "input_bindings": {
            "0": {"story": input("story")},
            "1": {
                "story": input("story"),
                "changed": {"source": "scope_output", "scope": "correction", "field": "changed"},
            },
        },
        "output_bindings": {},
        "loops": {"correction": {
            "body": 0,
            "max_iterations": MAX_ATTEMPTS,
            "carried": {"fields": {"findings": field("object"), "changed": field("array")}},
            "initial": {
                "findings": {"source": "constant", "value": {}},
                "changed": {"source": "constant", "value": []},
            },
            "result": {"findings": output(0, "findings"), "changed": output(0, "changed")},
        }},
        "conditions": {"needs_correction": {
            "operator": "contains",
            "collection": {"source": "constant", "value": rejected},
            "value": {"source": "object", "fields": {
                "passed": output(0, "passed"), "approved": output(0, "approved"),
            }},
        }},
        "collections": {}, "forks": {}, "subflows": [],
        "control": {
            "nodes": {
                "start": {"type": "start"},
                "correction": {"type": "loop", "scope": "correction", "condition": "needs_correction"},
                "deliver": {"type": "activity", "activity": 1},
                "delivered": {"type": "end", "outcome": "delivered"},
                "blocked": {"type": "end", "outcome": "blocked"},
            },
            "edges": {
                "start": {"next": "correction"},
                "correction": {
                    "complete": "deliver", "exhausted": "blocked",
                    "unknown": "blocked", "failed": "blocked",
                },
                "deliver": {"ok": "delivered"},
            },
        },
    }))
    .map_err(|e| e.to_string())
}

fn subflow(child: &NeutralPublication) -> Result<ActivityDescriptor, String> {
    let definition = &child.definition;
    Ok(ActivityDescriptor {
        id: id(ATTEMPT),
        purpose: "One delivery attempt: implement, verify and review".into(),
        handler: HandlerRef {
            name: "host.subflow".into(),
            version: 1,
        },
        target: Some(ReusableTarget::Subflow {
            publication: PublicationPin {
                reference: PublicationRef {
                    project: child.project.clone(),
                    id: child.id.clone(),
                    revision: child.revision,
                },
                sha256: content_digest(child)?,
            },
        }),
        config: Default::default(),
        inputs: definition.inputs.clone(),
        subject: definition.subject.clone(),
        outputs: definition.outputs.clone(),
        outcomes: definition.outcomes.clone(),
        required_capabilities: Default::default(),
        execution_policies: ["local".to_owned()].into(),
    })
}

/// Save the operations, publish the attempt subflow, then create and publish
/// the editable root draft. An existing published installation is returned
/// unchanged, so Designer edits are never overwritten.
pub(crate) fn install(
    store: &WorkflowStore,
    project: &str,
    installed: &WorkflowCatalog,
    registry: &HostRegistry<'_>,
    context: &dyn HostContext,
    commands: &Commands,
) -> Result<NeutralDraft, String> {
    let existing = store
        .list_neutral_drafts(project)?
        .into_iter()
        .find(|draft| draft.name == NAME);
    if let Some(draft) = &existing
        && draft.latest_published_revision > 0
    {
        return Ok(draft.clone());
    }
    let catalog = |store: &WorkflowStore| store.build_project_catalog(project, installed, registry);
    for operation in operations(project)? {
        let current = catalog(store)?;
        if current.get(&operation.id).is_none() {
            store.save_reusable_operation(project, 0, operation, &current, registry)?;
        }
    }
    let current = catalog(store)?;
    if current.get(&id(ATTEMPT)).is_none() {
        let child = NeutralAdmissionService {
            store,
            catalog: &current,
            registry,
        }
        .publish(Some(context), project, ATTEMPT, 0, attempt(commands)?)?;
        store.save_reusable_operation(project, 0, subflow(&child)?, &current, registry)?;
    }
    let draft = match existing {
        Some(draft) => draft,
        None => store.create_neutral_draft(
            project,
            NAME.into(),
            root()?,
            EditorMetadata {
                labels: [
                    ("correction", "Correction loop: implement, verify, review"),
                    ("deliver", "Deliver one commit"),
                ]
                .into_iter()
                .map(|(k, v)| (k.to_owned(), v.to_owned()))
                .collect(),
                presets: Default::default(),
            },
        )?,
    };
    let current = catalog(store)?;
    NeutralAdmissionService {
        store,
        catalog: &current,
        registry,
    }
    .publish_draft(Some(context), &draft)?;
    store.get_neutral_draft(project, &draft.id)
}

/// The story snapshot a run receives: what the implementer and the reviewer read.
pub(crate) fn story_input(project: &str, story: &str) -> Result<Value, String> {
    let backend = crate::stories::story_backend_for(project)?;
    let story = backend.require_unarchived_story(story)?;
    let plan = backend.require_unarchived_plan(&story.plan_id)?;
    if plan.project != project {
        return Err("story belongs to another project".into());
    }
    Ok(json!({
        "id": story.id, "title": story.title, "problem": story.problem.unwrap_or_default(),
        "criteria": story.criteria, "plan": plan.id, "plan_title": plan.title,
    }))
}
