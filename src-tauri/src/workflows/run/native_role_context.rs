//! Read-only role material is retained native data, never model-supplied authority.
use super::native_agent::Source;
use super::native_host::NativeBinding;
use super::neutral_effects::{EffectKey, EffectObservation, EffectOutcome, NeutralEffect};
use super::neutral_execution::{NeutralResult, NeutralStatus};
use crate::stories::{Plan, Story, StoryBackend};
use serde::{Deserialize, Serialize};
use serde_json::Value;

pub(super) const HANDLER: &str = "native.agent.context.read";
const MAX_PART_BYTES: usize = 262_144;
const MAX_TOTAL_BYTES: usize = 786_432;

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub(super) enum Kind {
    Story,
    Plan,
    Batch,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct Request {
    pub source: Source,
    pub kind: Kind,
    /// Exact commit, not a revision expression. Batch uses this run's first artifact pin.
    pub base: Option<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct Snapshot {
    pub effect: EffectKey,
    pub source: Source,
    pub kind: Kind,
    pub base: String,
    pub plan: Plan,
    /// Markdown bytes from plan.source in source.tree, not the mutable filesystem.
    pub plan_document: Option<String>,
    pub story: Option<Story>,
    /// Host-derived review target; requirements is the exact criteria digest.
    pub review_subject: Option<super::evidence::ReviewSubject>,
    pub review_criteria: Option<crate::workflows::correction::CorrectionPolicy>,
    pub members: Vec<Story>,
    pub diff: String,
}

impl Snapshot {
    pub(super) fn validate(&self, source: &Source) -> Result<(), String> {
        if self.source != *source
            || self.members.len() > 128
            || self.members.iter().any(|s| s.plan_id != self.plan.id)
        {
            return Err("role context source or member bound differs".into());
        }
        let subject_matches = match self.kind {
            Kind::Story => {
                self.story
                    .as_ref()
                    .is_some_and(|s| s.id == source.origin && s.plan_id == self.plan.id)
                    && self.members.is_empty()
            }
            Kind::Plan => {
                self.plan.id == source.origin
                    && self.story.is_none()
                    && self.plan_document.is_some()
            }
            Kind::Batch => {
                self.plan.id == source.origin && self.story.is_none() && !self.members.is_empty()
            }
        };
        let review_matches = match (&self.kind, &self.review_subject, &self.review_criteria) {
            (Kind::Story, Some(subject), Some(criteria)) => {
                subject == &story_review_subject(source, criteria)?
            }
            (Kind::Plan | Kind::Batch, None, None) => true,
            _ => false,
        };
        if !subject_matches || !review_matches {
            return Err("role context subject differs from its source kind".into());
        }
        for part in [
            serde_json::to_vec(&self.plan),
            serde_json::to_vec(&self.plan_document),
            serde_json::to_vec(&self.story),
            serde_json::to_vec(&self.review_subject),
            serde_json::to_vec(&self.review_criteria),
            serde_json::to_vec(&self.members),
            serde_json::to_vec(&self.diff),
        ] {
            if part.map_err(|e| e.to_string())?.len() > MAX_PART_BYTES {
                return Err("role context part exceeds 262144 bytes".into());
            }
        }
        if serde_json::to_vec(self).map_err(|e| e.to_string())?.len() > MAX_TOTAL_BYTES {
            return Err("role context exceeds 786432 bytes".into());
        }
        Ok(())
    }
}

fn story_review_criteria(
    context: &super::native_operations::NativeContext,
) -> crate::workflows::correction::CorrectionPolicy {
    crate::workflows::correction::CorrectionPolicy {
        criteria: context
            .verification_check_names()
            .iter()
            .map(|name| format!("check:{name}"))
            .chain(["independent_review".into()])
            .collect(),
    }
}

fn story_review_subject(
    source: &Source,
    criteria: &crate::workflows::correction::CorrectionPolicy,
) -> Result<super::evidence::ReviewSubject, String> {
    Ok(super::evidence::ReviewSubject::Story {
        id: source.origin.clone(),
        commit: source.commit.clone(),
        tree: source.tree.clone(),
        requirements: crate::workflows::catalog::content_digest(criteria)?,
    })
}

fn receipt(effect: &NeutralEffect) -> Option<&Value> {
    match &effect.observation {
        Some(EffectObservation {
            stopped: true,
            outcome:
                EffectOutcome::Completed {
                    result: NeutralResult::Completed { outputs, .. },
                },
        }) => outputs.get("receipt"),
        _ => None,
    }
}

pub(super) fn read(
    bound: &NativeBinding,
    key: &EffectKey,
    backend: &dyn StoryBackend,
    request: &Request,
) -> Result<Snapshot, String> {
    bound
        .context
        .require_artifact(&request.source.commit, &request.source.tree)?;
    let run = bound.store.neutral_execution(&key.fence.run)?;
    let original = run.effect(key)?;
    if run.status != NeutralStatus::Running
        || !original.dispatched
        || original.cancel_started
        || original.observation.is_some()
        || original.invocation.descriptor.handler.name != HANDLER
        || original.invocation.inputs.get("request")
            != Some(&serde_json::to_value(request).map_err(|e| e.to_string())?)
    {
        return Err("role context read does not own its original active request".into());
    }
    let (story, plan) = match request.kind {
        Kind::Story => {
            let story = backend.require_unarchived_story(&request.source.origin)?;
            let plan = backend.require_unarchived_plan(&story.plan_id)?;
            (Some(story), plan)
        }
        Kind::Plan | Kind::Batch => (
            None,
            backend.require_unarchived_plan(&request.source.origin)?,
        ),
    };
    if plan.project != bound.context.workspace_path().to_string_lossy() {
        return Err("role context is outside this run's project".into());
    }
    let members = match request.kind {
        Kind::Story => Vec::new(),
        Kind::Plan => backend.list_stories(&plan.id)?,
        Kind::Batch => {
            let pin = run
                .effects
                .values()
                .filter(|e| e.invocation.descriptor.handler.name == super::native_batch::PIN)
                .filter_map(|e| receipt(e).map(|r| (e, r)))
                .filter(|(_, r)| {
                    r["plan"] == plan.id
                        && r["commit"] == request.source.commit
                        && r["tree"] == request.source.tree
                })
                .max_by_key(|(e, _)| (e.key.fence.activation, e.key.fence.attempt))
                .ok_or("role context has no owned settled batch pin")?
                .1;
            pin["members"]
                .as_array()
                .ok_or("batch pin members absent")?
                .iter()
                .map(|m| {
                    let story = backend.require_unarchived_story(
                        m["story"].as_str().ok_or("batch member id absent")?,
                    )?;
                    if story.plan_id != plan.id {
                        return Err("batch member has a foreign plan".into());
                    }
                    Ok(story)
                })
                .collect::<Result<Vec<_>, String>>()?
        }
    };
    let base = match &request.base {
        Some(base) => base.clone(),
        None => run
            .effects
            .values()
            .filter(|e| e.invocation.descriptor.handler.name == "native.artifact.pin")
            .filter_map(|e| {
                receipt(e)
                    .and_then(|r| r["commit"].as_str())
                    .map(|c| (e, c))
            })
            .min_by_key(|(e, _)| (e.key.fence.activation, e.key.fence.attempt))
            .ok_or("role context has no original artifact baseline")?
            .1
            .to_owned(),
    };
    let path = bound.context.workspace_path();
    if super::check::git_output(
        path,
        &["rev-parse", "--verify", &format!("{base}^{{commit}}")],
    )? != base
    {
        return Err("role context base is not an exact commit".into());
    }
    super::check::git_output(
        path,
        &["merge-base", "--is-ancestor", &base, &request.source.commit],
    )?;
    let diff = read_git_bounded(
        path,
        &[
            "diff",
            "--no-ext-diff",
            "--no-textconv",
            &base,
            &request.source.tree,
            "--",
        ],
    )?;
    let diff = match diff {
        Some(diff)
            if serde_json::to_vec(&diff).map_err(|e| e.to_string())?.len() <= MAX_PART_BYTES =>
        {
            diff
        }
        _ => format!(
            "[TRUNCATED: full diff exceeds the role context byte limit; diff stat follows]\n{}",
            read_git(
                path,
                &[
                    "diff",
                    "--no-ext-diff",
                    "--no-textconv",
                    "--stat=120,80,100",
                    &base,
                    &request.source.tree,
                    "--"
                ]
            )?
        ),
    };
    let plan_document = if matches!(request.kind, Kind::Plan) {
        plan_document(path, &request.source.tree, &plan.source)?
    } else {
        None
    };
    if matches!(request.kind, Kind::Plan) && plan_document.is_none() {
        return Err("plan coordinator requires its document in the pinned tree".into());
    }
    let review_criteria =
        matches!(request.kind, Kind::Story).then(|| story_review_criteria(&bound.context));
    let review_subject = review_criteria
        .as_ref()
        .map(|criteria| story_review_subject(&request.source, criteria))
        .transpose()?;
    let snapshot = Snapshot {
        effect: key.clone(),
        source: request.source.clone(),
        kind: request.kind.clone(),
        base,
        plan,
        plan_document,
        story,
        review_subject,
        review_criteria,
        members,
        diff,
    };
    snapshot.validate(&request.source)?;
    bound
        .context
        .require_artifact(&request.source.commit, &request.source.tree)?;
    Ok(snapshot)
}

fn plan_document(
    path: &std::path::Path,
    tree: &str,
    source: &str,
) -> Result<Option<String>, String> {
    if crate::fs::is_absolute_on_any_platform(source)
        || source.split(['/', '\\']).any(|p| p == "..")
    {
        return Ok(None);
    }
    let entry = super::check::git_output(
        path,
        &[
            "ls-tree",
            "-z",
            "--full-tree",
            tree,
            "--",
            &format!(":(literal){source}"),
        ],
    )?;
    if entry.is_empty() {
        return Ok(None);
    }
    if !(entry.starts_with("100644 blob ") || entry.starts_with("100755 blob "))
        || entry.split('\0').filter(|s| !s.is_empty()).count() != 1
    {
        return Err("role plan document must be one pinned regular file".into());
    }
    let object = format!("{tree}:{source}");
    let bytes: usize = super::check::git_output(path, &["cat-file", "-s", &object])?
        .parse()
        .map_err(|_| "plan document size is invalid")?;
    if bytes > MAX_PART_BYTES {
        return Err("role context part exceeds 262144 bytes".into());
    }
    read_git(path, &["cat-file", "blob", &object]).map(Some)
}

/// Bound allocation before collecting subprocess output, and retain exact bytes.
fn read_git(path: &std::path::Path, args: &[&str]) -> Result<String, String> {
    read_git_bounded(path, args)?.ok_or_else(|| "role context part exceeds 262144 bytes".into())
}

fn read_git_bounded(path: &std::path::Path, args: &[&str]) -> Result<Option<String>, String> {
    use std::io::Read;
    use std::process::{Command, Stdio};
    let mut child = Command::new("git")
        .args(args)
        .current_dir(path)
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .map_err(|e| e.to_string())?;
    let mut bytes = Vec::new();
    let read = child
        .stdout
        .take()
        .ok_or("role context Git stdout absent")?
        .take(MAX_PART_BYTES as u64 + 1)
        .read_to_end(&mut bytes);
    if read.is_err() || bytes.len() > MAX_PART_BYTES {
        let _ = child.kill();
        let _ = child.wait();
        read.map_err(|e| format!("read pinned role context: {e}"))?;
        return Ok(None);
    }
    if !child.wait().map_err(|e| e.to_string())?.success() {
        return Err("read pinned role context from Git failed".into());
    }
    String::from_utf8(bytes)
        .map(Some)
        .map_err(|_| "role context Git material is not UTF-8".into())
}
