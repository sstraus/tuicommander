//! The GitHub PR review template end to end on the neutral engine: production
//! poll normalization, trigger admission, the real GitHub operation logic over
//! an in-memory GitHub, and deterministic model results.
use super::neutral_effects::{EffectObservation, EffectOutcome, EffectPreparation, NeutralEffect};
use super::neutral_execution::{NeutralResult, NeutralStatus};
use super::neutral_github_review::{self as github, Operation};
use super::neutral_runtime::{NeutralHost, NeutralInvocation, ObservationFuture};
use super::runtime::{RuntimeOwner, run_actor};
use crate::github_pr_api::PrApi;
use crate::workflows::catalog::{
    ActivityDescriptor, ActivityInstance, HandlerRef, WorkflowCatalog,
};
use crate::workflows::github_trigger::GithubResource;
use crate::workflows::neutral_admission::{HostAdmission, HostContext, HostRegistry};
use crate::workflows::neutral_publication::{NeutralPublication, PublicationRef};
use crate::workflows::trigger::*;
use crate::workflows::{WorkflowStore, github_event::from_rest};
use serde_json::{Value, json};
use std::collections::{BTreeMap, BTreeSet};
use std::sync::Arc;

const REPO: &str = "octo-org/example";

/// In-memory GitHub REST surface for one PR. Reviews behave as GitHub does:
/// pinned to commit_id, listed, and readable by id.
#[derive(Default)]
struct FakeGitHub {
    state: parking_lot::Mutex<PrState>,
}
#[derive(Default)]
struct PrState {
    head: String,
    base: String,
    /// Simulates a push that lands between the snapshot and the review.
    head_after_compare: Option<String>,
    reviews: Vec<Value>,
}
impl FakeGitHub {
    fn handle(&self, method: &str, endpoint: &str, body: Value) -> Result<Value, String> {
        let mut pr = self.state.lock();
        match (method, endpoint) {
            ("GET", "pulls/1347") => Ok(json!({
                "number": 1347, "state": "open", "title": "Found a bug", "body": "Fix it",
                "head": {"sha": pr.head}, "base": {"sha": pr.base},
            })),
            ("GET", e) if e.starts_with("compare/") => {
                if let Some(next) = pr.head_after_compare.take() {
                    pr.head = next;
                }
                Ok(
                    json!({"files": [{"filename": "src/lib.rs", "status": "modified", "patch": "@@ -1 +1 @@\n-a\n+b"}]}),
                )
            }
            ("POST", "pulls/1347/reviews") => {
                let state = match body["event"].as_str() {
                    Some("COMMENT") => "COMMENTED",
                    _ => return Err("unexpected review event".into()),
                };
                let id = pr.reviews.len() as u64 + 1;
                let review = json!({
                    "id": id, "commit_id": body["commit_id"], "body": body["body"],
                    "state": state, "html_url": format!("https://github.com/{REPO}/pull/1347#review-{id}"),
                });
                pr.reviews.push(review.clone());
                Ok(review)
            }
            ("GET", e) if e.starts_with("pulls/1347/reviews/") => pr
                .reviews
                .iter()
                .find(|r| e.ends_with(&format!("/{}", r["id"])))
                .cloned()
                .ok_or_else(|| "review not found".into()),
            _ => Err(format!("unexpected GitHub call {method} {endpoint}")),
        }
    }
    fn reviewed_heads(&self) -> Vec<String> {
        self.state
            .lock()
            .reviews
            .iter()
            .map(|r| r["commit_id"].as_str().unwrap().to_owned())
            .collect()
    }
}
impl PrApi for FakeGitHub {
    fn graphql(
        &self,
        _: &str,
        _: Value,
    ) -> impl std::future::Future<Output = Result<Value, String>> + Send {
        std::future::ready(Err("GraphQL is not part of this workflow".into()))
    }
    fn rest(
        &self,
        method: &str,
        endpoint: &str,
        body: Value,
    ) -> impl std::future::Future<Output = Result<Value, String>> + Send {
        std::future::ready(self.handle(method, endpoint, body))
    }
}

struct Context;
impl HostContext for Context {}

/// GitHub activities run the production operation code; model activities
/// return a fixed verdict so the graph, not a model, is under test.
struct ReviewHost {
    github: Arc<FakeGitHub>,
    /// The production adapter against real GitHub, for the sandbox check.
    live: Option<github::GitHubHost>,
    repo: String,
    handlers: Vec<HandlerRef>,
}
impl HostAdmission for ReviewHost {
    fn publication(
        &self,
        _: &dyn HostContext,
        _: &NeutralPublication,
        _: &ActivityDescriptor,
        _: &ActivityInstance,
    ) -> Result<(), String> {
        Ok(())
    }
    fn dispatch(
        &self,
        _: &dyn HostContext,
        _: &NeutralPublication,
        _: &ActivityDescriptor,
        _: &ActivityInstance,
        _: &BTreeMap<String, Value>,
        _: Option<&BTreeMap<String, Value>>,
    ) -> Result<(), String> {
        Ok(())
    }
}
impl NeutralHost for ReviewHost {
    fn context(&self, _: &NeutralInvocation) -> Result<Box<dyn HostContext>, String> {
        Ok(Box::new(Context))
    }
    fn registry(&self) -> HostRegistry<'_> {
        let mut registry = HostRegistry::default();
        for handler in &self.handlers {
            registry
                .handlers
                .insert(handler.clone(), self as &dyn HostAdmission);
        }
        registry
    }
    fn prepare(&self, invocation: &NeutralInvocation) -> Result<EffectPreparation, String> {
        if invocation.descriptor.id.namespace == "github" {
            return github::prepare(invocation);
        }
        Ok(EffectPreparation {
            mutation: false,
            resources: BTreeSet::new(),
            approval: None,
            model_tools: None,
        })
    }
    fn invoke_effect(&self, effect: NeutralEffect) -> ObservationFuture {
        if let Some(live) = &self.live
            && effect.invocation.descriptor.id.namespace == "github"
        {
            return live.invoke_effect(effect);
        }
        let api = self.github.clone();
        let repo = self.repo.clone();
        Box::pin(async move {
            let outputs = match effect.invocation.descriptor.id.namespace.as_str() {
                "github" => return github::execute(api.as_ref(), &repo, &effect).await,
                "inbox" => json!({"approved": true}),
                _ => {
                    let request = &effect.invocation.instance.config["request"];
                    if request["output_schema"]["fields"].get("passed").is_some() {
                        json!({"result": {"passed": true, "summary": "claims hold"}})
                    } else {
                        let text = effect.invocation.inputs["text"].as_str().unwrap_or("");
                        assert!(
                            text.contains("Changed files"),
                            "review must receive the PR text"
                        );
                        json!({"result": {"body": "No defects found."}})
                    }
                }
            };
            Ok(EffectObservation {
                stopped: true,
                outcome: EffectOutcome::Completed {
                    result: NeutralResult::Completed {
                        outcome: "complete".into(),
                        outputs: serde_json::from_value(outputs).unwrap(),
                    },
                },
            })
        })
    }
}

struct Fixture {
    _dir: tempfile::TempDir,
    owner: Arc<RuntimeOwner>,
    workflows: WorkflowStore,
    catalog: WorkflowCatalog,
    handlers: BTreeSet<HandlerRef>,
    github: Arc<FakeGitHub>,
    project: String,
    repo: String,
    raw: parking_lot::Mutex<Value>,
}

fn fixture() -> Fixture {
    fixture_for("/project", REPO, None)
}

fn fixture_for(project: &str, repo: &str, live: Option<github::GitHubHost>) -> Fixture {
    let dir = tempfile::tempdir_in(crate::test_support::test_temp_root()).unwrap();
    let owner = RuntimeOwner::acquire(&dir.path().join("runs.sqlite3")).unwrap();
    let workflows = WorkflowStore::open_at(&dir.path().join("definitions.sqlite3")).unwrap();
    let runtime = crate::acp::EgoAcpConfig {
        executable: std::env::current_exe().unwrap(),
        profile: "review".into(),
        master_prompt: String::new(),
        ai_chat: false,
    };
    let mut catalog = WorkflowCatalog::new();
    for descriptor in [
        super::native_model::descriptor(&runtime).unwrap(),
        super::native_approval::descriptor(),
        github::descriptor(Operation::Snapshot),
        github::descriptor(Operation::Review),
    ] {
        catalog.register(descriptor).unwrap();
    }
    let handlers: BTreeSet<_> = catalog.descriptors().map(|d| d.handler.clone()).collect();
    let github = Arc::new(FakeGitHub::default());
    owner.neutral_host(Arc::new(ReviewHost {
        github: github.clone(),
        live,
        repo: repo.into(),
        handlers: handlers.iter().cloned().collect(),
    }));
    let mut definition = crate::workflows::neutral_api::templates::templates()
        .unwrap()
        .into_iter()
        .find(|t| t.key == "github-pr-review")
        .expect("built-in PR review template")
        .definition;
    let checked =
        crate::workflows::neutral_api::validation::validate(&definition, &catalog, &handlers);
    assert!(checked.valid, "{:?}", checked.diagnostics);
    // What the user sets in the Designer before publishing.
    for instance in &mut definition.instances {
        if let Some(request) = instance.config.get_mut("request") {
            request["profile"] = json!(runtime.profile);
            request["model"] = json!("test-model");
            request["effort"] = json!("low");
        }
    }
    workflows
        .save_neutral_publication(project, "pr-review", 0, definition, &catalog, &handlers)
        .unwrap();
    let draft = TriggerBindingDraft {
        id: "pr-review-binding".into(),
        publication: PublicationRef {
            project: project.into(),
            id: "pr-review".into(),
            revision: 1,
        },
        source: TriggerSource::Github,
        schedule: None,
        kind: "pull_request".into(),
        resource_prefix: format!("github:{repo}/"),
        events: ["opened", "reopened", "updated"]
            .into_iter()
            .map(str::to_owned)
            .collect(),
        filters: BTreeMap::new(),
        ignored_actors: BTreeSet::new(),
        mapping: TriggerMapping {
            inputs: ["repository", "number", "head", "base"]
                .into_iter()
                .map(|name| (name.to_owned(), name.to_owned()))
                .collect(),
            subject: BTreeMap::new(),
        },
        limits: super::neutral::NeutralLimits {
            max_activations: 50,
            max_concurrency: 2,
            max_duration_ms: 600_000,
        },
    };
    let saved = workflows
        .save_neutral_trigger_binding(project, 0, draft, &catalog, &handlers)
        .unwrap();
    let mut sample = super::neutral_trigger_fixture::pr_event("1", 'a');
    sample.resource = format!("github:{repo}/pull_request/1347");
    sample.values.insert("repository".into(), json!(repo));
    let overlaps = workflows
        .preview_neutral_trigger_mapping(project, &saved.draft, &sample, &catalog, &handlers)
        .unwrap()
        .overlaps;
    workflows
        .enable_neutral_trigger_binding(
            project,
            &saved.draft.id,
            saved.revision,
            true,
            &overlaps,
            &catalog,
            &handlers,
        )
        .unwrap();
    let f = Fixture {
        _dir: dir,
        owner,
        workflows,
        catalog,
        handlers,
        github,
        project: project.into(),
        repo: repo.into(),
        raw: parking_lot::Mutex::new(
            serde_json::from_str(include_str!("../fixtures/github-pr-rest.json")).unwrap(),
        ),
    };
    // The activation cut: a complete empty poll before the PR is opened.
    assert_eq!(f.poll(None), [] as [TriggerAdmission; 0]);
    f
}

impl Fixture {
    /// One production poll cycle. `push` moves the PR to a new head first.
    fn poll(&self, push: Option<char>) -> Vec<TriggerAdmission> {
        let snapshots = match push {
            None if self.github.state.lock().head.is_empty() => Vec::new(),
            push => {
                let mut raw = self.raw.lock();
                if let Some(head) = push {
                    raw["head"]["sha"] = json!(head.to_string().repeat(40));
                }
                let mut pr = self.github.state.lock();
                pr.head = raw["head"]["sha"].as_str().unwrap().into();
                pr.base = raw["base"]["sha"].as_str().unwrap().into();
                vec![from_rest(REPO, GithubResource::PullRequest, &raw).unwrap()]
            }
        };
        self.admit(snapshots)
    }
    fn admit(
        &self,
        snapshots: Vec<crate::workflows::github_trigger::GithubSnapshot>,
    ) -> Vec<TriggerAdmission> {
        self.owner
            .admit_github_batch(
                &self.workflows,
                &self.catalog,
                &self.handlers,
                &self.project,
                &self.repo,
                GithubResource::PullRequest,
                snapshots,
            )
            .unwrap()
    }
    /// GitHub advances updated_at on every review; the next poll sees that.
    fn touch(&self) {
        let mut raw = self.raw.lock();
        let next = chrono::DateTime::parse_from_rfc3339(raw["updated_at"].as_str().unwrap())
            .unwrap()
            + chrono::Duration::seconds(1);
        raw["updated_at"] = json!(next.to_rfc3339());
    }
    async fn drive(&self, run: &str) -> NeutralStatus {
        run_actor(
            Arc::downgrade(&self.owner),
            run.to_owned(),
            Arc::new(tokio::sync::Notify::new()),
        )
        .await;
        self.owner.store.neutral_execution(run).unwrap().status
    }
    fn runs(&self) -> Vec<String> {
        self.owner.store.neutral_recovery_run_ids().unwrap()
    }
}

/// Run ids of retained admissions; a new generation would carry a new run.
fn only_existing(admissions: &[TriggerAdmission]) -> Vec<String> {
    admissions
        .iter()
        .map(|a| a.run_id.clone().expect("retained admission has its run"))
        .collect()
}

fn started(admissions: &[TriggerAdmission]) -> String {
    assert_eq!(admissions.len(), 1, "{admissions:?}");
    admissions[0]
        .run_id
        .clone()
        .expect("admission started a run")
}

// Catches: duplicate reviews (and the built-in template drifting from the
// installed catalog, so it cannot be published) — every update of an active PR starting its own
// review run, a superseded run still publishing on its old head, or one head
// receiving two reviews.
#[tokio::test]
async fn updates_of_an_active_pr_follow_overlap_policy_and_publish_one_review_per_head() {
    let f = fixture();
    let first = started(&f.poll(Some('a')));
    f.touch();
    let second = started(&f.poll(Some('b')));
    f.touch();
    let latest = started(&f.poll(Some('c')));
    assert_eq!(
        f.runs().len(),
        3,
        "each update supersedes the active run instead of running beside it"
    );
    for superseded in [&first, &second] {
        assert_eq!(f.drive(superseded).await, NeutralStatus::Cancelled);
    }
    assert_eq!(
        f.drive(&latest).await,
        NeutralStatus::Completed {
            outcome: "published".into()
        }
    );
    assert_eq!(f.github.reviewed_heads(), vec!["c".repeat(40)]);
    // A repeated poll of the same head is the same generation: it reports the
    // retained admission of the run that already reviewed it, never a new run.
    assert_eq!(only_existing(&f.poll(None)), vec![latest.clone()]);
    assert_eq!(f.runs().len(), 3);
    assert_eq!(f.github.reviewed_heads().len(), 1);
}

// Catches: self-triggered loops — the review the run publishes changes the PR
// (updated_at, review list), and that change starting another review run.
#[tokio::test]
async fn the_published_review_does_not_start_a_new_run() {
    let f = fixture();
    let run = started(&f.poll(Some('a')));
    assert_eq!(
        f.drive(&run).await,
        NeutralStatus::Completed {
            outcome: "published".into()
        }
    );
    assert_eq!(f.github.reviewed_heads(), vec!["a".repeat(40)]);
    f.touch();
    assert_eq!(
        only_existing(&f.poll(None)),
        vec![run.clone()],
        "the own review must not admit the PR again"
    );
    assert_eq!(f.runs(), vec![run]);
    assert_eq!(f.github.reviewed_heads().len(), 1);
}

// Catches: a newer head receiving an old review — the head moves after the
// snapshot was reviewed, and the run still posts the captured review on it.
#[tokio::test]
async fn a_head_pushed_during_review_supersedes_the_publication() {
    let f = fixture();
    f.github.state.lock().head_after_compare = Some("d".repeat(40));
    let run = started(&f.poll(Some('a')));
    assert_eq!(
        f.drive(&run).await,
        NeutralStatus::Completed {
            outcome: "superseded".into()
        }
    );
    assert_eq!(f.github.reviewed_heads(), [] as [String; 0]);
}

// Catches: the approval that a failed verification opens showing only a fixed
// question, so the operator decides to publish without the verification
// summary that caused the question.
#[test]
fn failed_verification_approval_shows_the_verification_summary() {
    use crate::workflows::data_binding::{DataState, Datum, resolve_bindings};
    let definition = crate::workflows::neutral_api::templates::templates()
        .unwrap()
        .into_iter()
        .find(|t| t.key == "github-pr-review")
        .expect("built-in PR review template")
        .definition;
    let summary = "median([1,2,3]) returns 1.5: the parity check is inverted";
    let verify = BTreeMap::from([(
        "result".to_owned(),
        Datum {
            state: DataState::Present(json!({"passed": false, "summary": summary})),
            provenance: BTreeSet::new(),
        },
    )]);
    let descriptor = super::native_approval::descriptor();
    let inputs = resolve_bindings(
        &definition.input_bindings[&4],
        &BTreeMap::new(),
        &BTreeMap::from([(2, verify)]),
        &descriptor.inputs,
        &BTreeMap::new(),
    )
    .unwrap()
    .values;
    let native = super::native_test_support::Fixture::new();
    let run = native.start("workspace.pin", json!({}));
    let invocation = NeutralInvocation {
        fence: run.attempt.unwrap().fence,
        publication: run.publication,
        descriptor,
        instance: definition.instances[4].clone(),
        inputs,
        subject: None,
    };
    let title = native
        .host
        .prepare(&invocation)
        .unwrap()
        .approval
        .expect("the operator question")
        .title;
    assert!(
        title.contains("Publish the review anyway?") && title.contains(summary),
        "{title}"
    );
}

async fn reviews(api: &crate::github_pr_api::RepoApi<'_>, number: i64) -> Vec<Value> {
    api.rest(
        "GET",
        &format!("pulls/{number}/reviews?per_page=100"),
        Value::Null,
    )
    .await
    .unwrap()
    .as_array()
    .unwrap()
    .clone()
}

// Catches: the production GitHub adapter failing against the real service — wrong
// account or repository, a review not pinned to the exact head, a second review
// for one head, or the own review re-admitting the PR. Model results stay fixed.
#[tokio::test]
#[ignore = "requires explicit sandbox authority and command-only GITHUB_E2E_TOKEN"]
async fn github_sandbox_pr_gets_one_review_from_the_neutral_engine() {
    use crate::github_pr_api::RepoApi;
    let token = std::env::var("GITHUB_E2E_TOKEN").expect("authorized GITHUB_E2E_TOKEN required");
    assert_ne!(token, "");
    let config = tempfile::tempdir_in(crate::test_support::test_temp_root()).unwrap();
    let _guard = crate::config::set_config_dir_override(config.path().into());
    let native = super::native_test_support::Fixture::new();
    *native.state.github.token.write() = Some(token);
    let project = native.context.workspace_path().to_str().unwrap().to_owned();
    let repo = "sstraus/tuic-e2e-sandbox";
    super::check::git_output(
        native.context.workspace_path(),
        &[
            "remote",
            "add",
            "origin",
            &format!("https://github.com/{repo}.git"),
        ],
    )
    .unwrap();
    let api = RepoApi::resolve(&native.state, &project).await.unwrap();
    let open = api
        .rest("GET", "pulls?state=open&per_page=100", Value::Null)
        .await
        .unwrap();
    let number = open
        .as_array()
        .and_then(|prs| prs.first())
        .expect("sandbox requires one open PR")["number"]
        .as_i64()
        .unwrap();
    let before = reviews(&api, number).await.len();
    let f = fixture_for(
        &project,
        repo,
        Some(github::GitHubHost {
            state: native.state.clone(),
            authority: Arc::new(|| Ok(())),
        }),
    );
    let snapshot = crate::workflows::github_event::read_snapshot(
        &api,
        repo,
        GithubResource::PullRequest,
        number,
    )
    .await
    .unwrap();
    let head = snapshot.head.clone().unwrap();
    let run = started(&f.admit(vec![snapshot]));
    assert_eq!(
        f.drive(&run).await,
        NeutralStatus::Completed {
            outcome: "published".into()
        }
    );
    let after = reviews(&api, number).await;
    assert_eq!(after.len(), before + 1, "exactly one review from the run");
    let review = after.last().unwrap();
    assert_eq!(review["commit_id"], json!(head));
    assert!(
        review["body"]
            .as_str()
            .unwrap()
            .contains("<!-- tuic-effect:")
    );
    let again = crate::workflows::github_event::read_snapshot(
        &api,
        repo,
        GithubResource::PullRequest,
        number,
    )
    .await
    .unwrap();
    assert_eq!(only_existing(&f.admit(vec![again])), vec![run.clone()]);
    assert_eq!(f.runs(), vec![run]);
    assert_eq!(reviews(&api, number).await.len(), before + 1);
}
