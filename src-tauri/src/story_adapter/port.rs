//! The external adapter behind the `StoryBackend` port.
//!
//! Records carry an opaque `version`; the port speaks `i64` revisions. The
//! revision of a record is a hash of its version, so a write first compares the
//! caller's revision with the current record, then sends that record's version
//! as `expected_version`, and the adapter makes the final atomic check.
//!
//! Execution state (claims, review/QA sub-states, receipts, history) is not
//! content: it lives in TUIC's ledger and is laid over the adapter's records
//! (`execution.rs`).

use super::{AdapterError, ApplyOp, ExternalBackend, ExternalStory, NewExternalStory};
use crate::stories::{
    ArchiveEvent, NewPlan, NewStory, Plan, PlanPatch, PlanState, PlanView, ProjectStoryView, Story,
    StoryBackend, StoryCapabilities, StoryCommand, StoryFilter, StoryListFilter, StoryListPage,
    StoryNote, StoryOperations, StoryOrigin, StoryPatch, StoryReceiptReference, StoryStatus,
    StoryTransitionActor,
};
use sha2::{Digest, Sha256};
use std::borrow::Cow;
use std::collections::HashSet;
use std::path::PathBuf;
use std::sync::Arc;

#[path = "execution.rs"]
mod execution;

pub(crate) struct ExternalStories {
    backend: Arc<ExternalBackend>,
    project: PathBuf,
}

impl ExternalStories {
    pub(crate) fn new(backend: Arc<ExternalBackend>, project: PathBuf) -> Self {
        Self { backend, project }
    }

    fn plan(&self, plan: super::ExternalPlan) -> Plan {
        Plan {
            revision: revision(&plan.version),
            archived: false,
            id: plan.id,
            project: self.project.to_string_lossy().into_owned(),
            title: plan.title,
            source: String::new(),
        }
    }

    fn current(&self, id: &str, expected_revision: i64) -> Result<ExternalStory, String> {
        Ok(self.read_current(id, expected_revision)?.raw)
    }

    fn overlay_one(&self, raw: ExternalStory) -> Result<Story, String> {
        let ledger = self.ledger()?;
        Ok(self
            .overlay_all(vec![raw], &ledger)?
            .pop()
            .expect("one story in, one out"))
    }

    /// Applies `ops` in order, each against the version the previous one returned.
    /// Not atomic across ops: a later refusal leaves the earlier ones applied.
    // DEFERRED (2026-11-12) — legacy aggregate adapters apply operations sequentially,
    // not atomically. Preserve that contract until the protocol supplies a bulk transaction;
    // restricted adapters reject multi-op saves before reaching this path.
    fn apply_all(&self, mut story: ExternalStory, ops: &[ApplyOp]) -> Result<Story, String> {
        for op in ops {
            story = self
                .backend
                .apply(&story.id, &story.version, op)
                .map_err(text)?;
        }
        self.overlay_one(story)
    }

    fn all_stories(&self, plan_id: Option<&str>) -> Result<Vec<Story>, String> {
        Ok(self.stories_with_outside(plan_id)?.0)
    }

    /// The TUIC views of a plan's stories and the ids completed outside TUIC.
    fn stories_with_outside(
        &self,
        plan_id: Option<&str>,
    ) -> Result<(Vec<Story>, HashSet<String>), String> {
        let raws = self.backend.list_stories(plan_id).map_err(text)?;
        let ledger = self.ledger()?;
        let outside = raws
            .iter()
            .filter(|raw| ledger.completed_outside(raw))
            .map(|raw| raw.id.clone())
            .collect();
        Ok((self.overlay_all(raws, &ledger)?, outside))
    }

    fn plan_view_of(&self, plan_id: &str, archived: bool) -> Result<PlanView, String> {
        let (stories, outside) = self.stories_with_outside(Some(plan_id))?;
        let mut view = view(&stories, archived)?;
        for read in &mut view.stories {
            read.completed_outside = outside.contains(&read.story.id);
        }
        Ok(view)
    }
}

/// What a running adapter declared, in the shape the UI and MCP read.
pub(crate) fn external_capabilities(info: &super::AdapterInfo) -> StoryCapabilities {
    StoryCapabilities {
        backend: Cow::Owned(format!("external:{}", info.adapter.id)),
        operations: StoryOperations {
            create_plan: false,
            update_plan: false,
            archive_plan: false,
            create_story: info.supports("create_story"),
            update_story: info.supports("apply") && info.supports("set_fields"),
            story_edits: crate::stories::backend::StoryEdits {
                title: info.supports("apply")
                    && (info.supports("set_fields") || info.supports("set_title")),
                priority: info.supports("apply")
                    && (info.supports("set_fields") || info.supports("set_priority")),
                problem: info.supports("apply") && info.supports("set_fields"),
                criteria_add: info.supports("apply") && info.supports("add_criterion"),
                single_change: !info.supports("set_fields"),
            },
            archive_story: false,
            dependencies: info.supports("apply")
                && info.supports("add_dependency")
                && info.supports("remove_dependency"),
            notes: info.supports("append_note"),
            transitions: info.supports("apply")
                && info.supports("set_status")
                && info.supports("check_criterion"),
        },
    }
}

fn revision(version: &str) -> i64 {
    let digest = Sha256::digest(version.as_bytes());
    let mut bytes = [0u8; 8];
    bytes.copy_from_slice(&digest[..8]);
    (u64::from_be_bytes(bytes) >> 1) as i64
}

fn text(error: AdapterError) -> String {
    match error {
        AdapterError::Conflict => "stale story revision: the story changed in its backend".into(),
        other => other.to_string(),
    }
}

fn unsupported(what: &str) -> String {
    AdapterError::Unsupported(what.into()).to_string()
}

fn status_of(status: &str) -> Result<StoryStatus, String> {
    Ok(match status {
        "pending" => StoryStatus::Backlog,
        "ready" => StoryStatus::Ready,
        "in_progress" => StoryStatus::InProgress,
        "blocked" => StoryStatus::Blocked,
        "complete" => StoryStatus::Done,
        other => return Err(format!("story backend error: unknown status {other}")),
    })
}

fn view(stories: &[Story], archived: bool) -> Result<PlanView, String> {
    crate::stories::assemble_plan_view(stories, archived, |story| {
        Ok(matches!(
            story.status,
            StoryStatus::Done | StoryStatus::WontFix
        ))
    })
}

impl StoryBackend for ExternalStories {
    fn create_plan(&self, _input: NewPlan) -> Result<Plan, String> {
        Err(unsupported("create_plan"))
    }
    fn get_plan(&self, id: &str) -> Result<Plan, String> {
        let plans = self.backend.list_plans().map_err(text)?;
        let plan = plans
            .into_iter()
            .find(|plan| plan.id == id)
            .ok_or_else(|| format!("plan not found: {id}"))?;
        Ok(self.plan(plan))
    }
    fn require_unarchived_plan(&self, id: &str) -> Result<Plan, String> {
        self.get_plan(id)
    }
    fn list_plans(&self, _project: &str) -> Result<Vec<Plan>, String> {
        let plans = self.backend.list_plans().map_err(text)?;
        Ok(plans.into_iter().map(|plan| self.plan(plan)).collect())
    }
    fn list_plans_archived(&self, project: &str, archived: bool) -> Result<Vec<Plan>, String> {
        if archived {
            return Ok(Vec::new());
        }
        self.list_plans(project)
    }
    fn update_plan(&self, _id: &str, _rev: i64, _patch: PlanPatch) -> Result<Plan, String> {
        Err(unsupported("update_plan"))
    }
    fn plan_state(&self, plan_id: &str) -> Result<PlanState, String> {
        Ok(self.plan_view(plan_id)?.state)
    }
    fn plan_view(&self, plan_id: &str) -> Result<PlanView, String> {
        self.get_plan(plan_id)?;
        self.plan_view_of(plan_id, false)
    }
    fn filtered_plan_view(&self, plan_id: &str, filter: &StoryFilter) -> Result<PlanView, String> {
        crate::stories::filters::validate(filter)?;
        self.get_plan(plan_id)?;
        let mut view = self.plan_view_of(plan_id, filter.archived)?;
        (view.visible_story_ids, view.columns) =
            crate::stories::filters::project(&view.stories, filter);
        Ok(view)
    }
    fn project_story_view(
        &self,
        _project: &str,
        archived_plans: bool,
        filter: &StoryFilter,
    ) -> Result<ProjectStoryView, String> {
        crate::stories::filters::validate(filter)?;
        if archived_plans {
            return Ok(ProjectStoryView {
                stories: Vec::new(),
                visible_story_ids: Vec::new(),
                columns: crate::stories::filters::project(&[], filter).1,
            });
        }
        let mut stories = Vec::new();
        for plan in self.backend.list_plans().map_err(text)? {
            stories.extend(self.plan_view_of(&plan.id, filter.archived)?.stories);
        }
        let (visible_story_ids, columns) = crate::stories::filters::project(&stories, filter);
        Ok(ProjectStoryView {
            stories,
            visible_story_ids,
            columns,
        })
    }
    fn create_story_with_problem(
        &self,
        input: NewStory,
        problem: Option<String>,
    ) -> Result<Story, String> {
        if input.story_type.is_some() {
            return Err(unsupported("story_type"));
        }
        if input.origin != StoryOrigin::Native {
            return Err(unsupported("origin"));
        }
        if !input.file_scope.is_empty() {
            return Err(unsupported("file_scope"));
        }
        self.overlay_one(
            self.backend
                .create_story(&NewExternalStory {
                    plan_id: Some(input.plan_id),
                    title: input.title,
                    problem: problem.unwrap_or_default(),
                    priority: Some(input.priority),
                    criteria: input.criteria,
                })
                .map_err(text)?,
        )
    }
    fn get_story(&self, id: &str) -> Result<Story, String> {
        self.overlay_one(self.backend.get_story(id).map_err(text)?)
    }
    fn require_unarchived_story(&self, id: &str) -> Result<Story, String> {
        self.get_story(id)
    }
    fn list_stories(&self, plan_id: &str) -> Result<Vec<Story>, String> {
        self.get_plan(plan_id)?;
        self.all_stories(Some(plan_id))
    }
    fn list_stories_filtered(
        &self,
        plan_id: &str,
        filter: &StoryListFilter,
        limit: Option<u16>,
        cursor: Option<&str>,
    ) -> Result<StoryListPage, String> {
        let mut matching: Vec<Story> = self
            .list_stories(plan_id)?
            .into_iter()
            .filter(|story| {
                !filter.archived
                    && filter.state.is_none_or(|state| story.status == state)
                    && filter.priority.is_none_or(|p| story.priority == p)
                    && filter
                        .story_type
                        .is_none_or(|t| story.story_type == Some(t))
            })
            .filter(|story| cursor.is_none_or(|after| story.id.as_str() > after))
            .collect();
        matching.sort_by(|a, b| a.id.cmp(&b.id));
        let next_cursor = match limit.map(usize::from) {
            Some(limit) if matching.len() > limit => {
                matching.truncate(limit);
                matching.last().map(|story| story.id.clone())
            }
            _ => None,
        };
        Ok(StoryListPage {
            value: matching,
            next_cursor,
        })
    }
    fn update_story(
        &self,
        story_id: &str,
        expected_revision: i64,
        patch: StoryPatch,
    ) -> Result<Story, String> {
        if patch.file_scope.is_some() {
            return Err(unsupported("file_scope"));
        }
        if patch.origin.is_some() {
            return Err(unsupported("origin"));
        }
        if patch.story_type.is_some() {
            return Err(unsupported("story_type"));
        }
        if patch.risk.is_some() {
            return Err(unsupported("risk"));
        }
        let info = self.backend.info().map_err(text)?;
        let caps = external_capabilities(&info).operations.story_edits;
        // Refuse unsupported fields before reading content, and all restricted
        // multi-op saves before applying anything. Never silently split a save.
        if patch.title.is_some() && !caps.title {
            return Err(unsupported("title"));
        }
        if patch.priority.is_some() && !caps.priority {
            return Err(unsupported("priority"));
        }
        if patch.problem.is_some() && !caps.problem {
            return Err(unsupported("problem"));
        }
        if patch.criteria.is_some() && !caps.criteria_add {
            return Err(unsupported("add_criterion"));
        }
        if caps.single_change {
            let fields = usize::from(patch.title.is_some())
                + usize::from(patch.priority.is_some())
                + usize::from(patch.problem.is_some())
                + usize::from(patch.criteria.is_some());
            if patch.dependencies.is_some() || fields != 1 {
                return Err(unsupported(
                    "backend saves exactly one supported change at a time",
                ));
            }
        }
        let story = self.current(story_id, expected_revision)?;
        let mut ops = Vec::new();
        if patch.title.is_some() || patch.problem.is_some() || patch.priority.is_some() {
            ops.push(ApplyOp::SetFields {
                title: patch.title,
                problem: patch.problem.map(Option::unwrap_or_default),
                priority: patch.priority,
            });
        }
        if let Some(criteria) = patch.criteria {
            let current: Vec<&str> = story.criteria.iter().map(|c| c.text.as_str()).collect();
            let keeps_prefix = criteria.len() >= current.len()
                && criteria.iter().zip(&current).all(|(new, old)| new == old);
            if !keeps_prefix {
                return Err(unsupported("editing or removing criteria"));
            }
            if caps.single_change && criteria.len() != current.len() + 1 {
                return Err(unsupported(
                    "backend saves exactly one appended criterion at a time",
                ));
            }
            ops.extend(
                criteria[current.len()..]
                    .iter()
                    .map(|text| ApplyOp::AddCriterion { text: text.clone() }),
            );
        }
        if let Some(dependencies) = patch.dependencies {
            ops.extend(
                story
                    .dependencies
                    .iter()
                    .filter(|id| !dependencies.contains(id))
                    .map(|id| ApplyOp::RemoveDependency { id: id.clone() }),
            );
            ops.extend(
                dependencies
                    .iter()
                    .filter(|id| !story.dependencies.contains(id))
                    .map(|id| ApplyOp::AddDependency { id: id.clone() }),
            );
        }
        if caps.single_change && ops.len() != 1 {
            return Err(unsupported(
                "backend saves exactly one supported change at a time",
            ));
        }
        self.apply_all(story, &ops)
    }
    fn add_dependency(
        &self,
        story_id: &str,
        dependency_id: &str,
        expected_revision: i64,
    ) -> Result<Story, String> {
        let story = self.current(story_id, expected_revision)?;
        let op = ApplyOp::AddDependency {
            id: dependency_id.into(),
        };
        self.apply_all(story, &[op])
    }
    fn remove_dependency(
        &self,
        story_id: &str,
        dependency_id: &str,
        expected_revision: i64,
        _actor_session: Option<&str>,
    ) -> Result<Story, String> {
        let story = self.current(story_id, expected_revision)?;
        let op = ApplyOp::RemoveDependency {
            id: dependency_id.into(),
        };
        self.apply_all(story, &[op])
    }
    fn add_note(
        &self,
        story_id: &str,
        expected_revision: i64,
        text_value: &str,
        actor: StoryTransitionActor,
        receipts: Vec<StoryReceiptReference>,
    ) -> Result<StoryNote, String> {
        if !receipts.is_empty() {
            return Err(unsupported("note receipts"));
        }
        self.current(story_id, expected_revision)?;
        let story = self
            .backend
            .append_note(story_id, text_value)
            .map_err(text)?;
        let index = story.notes.len().saturating_sub(1);
        let mut note = notes_of(&story)
            .into_iter()
            .nth(index)
            .ok_or("story backend error: the note is missing after append")?;
        note.actor = actor;
        Ok(note)
    }
    fn list_notes(&self, story_id: &str) -> Result<Vec<StoryNote>, String> {
        Ok(notes_of(&self.backend.get_story(story_id).map_err(text)?))
    }
    fn set_story_archived(
        &self,
        _id: &str,
        _rev: i64,
        _archived: bool,
        _actor: StoryTransitionActor,
    ) -> Result<Story, String> {
        Err(unsupported("archive_story"))
    }
    fn set_plan_archived(
        &self,
        _id: &str,
        _rev: i64,
        _archived: bool,
        _actor: StoryTransitionActor,
    ) -> Result<Plan, String> {
        Err(unsupported("archive_plan"))
    }
    fn archive_history(&self, _id: &str) -> Result<Vec<ArchiveEvent>, String> {
        Ok(Vec::new())
    }
    fn transition_for_actor(
        &self,
        story_id: &str,
        expected_revision: i64,
        command: StoryCommand,
        actor_session: Option<&str>,
    ) -> Result<Story, String> {
        let actor = actor_session.map_or(StoryTransitionActor::Human, |session| {
            StoryTransitionActor::ManagedSession {
                session_id: session.into(),
            }
        });
        self.transition(story_id, expected_revision, command, actor)
    }
    fn reopen_for_workflow(
        &self,
        story_id: &str,
        revision: i64,
        run_id: &str,
    ) -> Result<Story, String> {
        let actor = StoryTransitionActor::Workflow {
            run_id: run_id.into(),
        };
        self.transition(story_id, revision, StoryCommand::Reopen, actor)
    }
    /// The adapter creates the story, then the ledger row records the proposal and
    /// the origin and file scope the protocol cannot hold. A failure between the two
    /// leaves one unreferenced adapter story and is reported; the run marks the
    /// effect failed and the key is not retried.
    fn create_story_once(
        &self,
        run_id: &str,
        proposal_key: &str,
        input: NewStory,
    ) -> Result<Story, String> {
        crate::stories::validate_workflow_proposal(&input)?;
        if input.story_type.is_some() {
            return Err(unsupported("story_type"));
        }
        if let Some(existing) = self.existing_story_for_proposal(run_id, proposal_key, &input)? {
            return Ok(existing);
        }
        let ledger = self.ledger()?;
        let raw = self
            .backend
            .create_story(&NewExternalStory {
                plan_id: Some(input.plan_id.clone()),
                title: input.title.clone(),
                problem: String::new(),
                priority: Some(input.priority),
                criteria: input.criteria.clone(),
            })
            .map_err(text)?;
        ledger.store().record_external_proposal(
            ledger.adapter_id(),
            run_id,
            proposal_key,
            &raw.id,
            &input,
        )?;
        self.get_story(&raw.id)
    }
    fn existing_story_for_proposal(
        &self,
        run_id: &str,
        proposal_key: &str,
        input: &NewStory,
    ) -> Result<Option<Story>, String> {
        let ledger = self.ledger()?;
        ledger
            .store()
            .external_proposal(ledger.adapter_id(), run_id, proposal_key, input)?
            .map(|id| self.get_story(&id))
            .transpose()
    }
}

/// The protocol records no note author; notes read back as local API notes.
fn notes_of(story: &ExternalStory) -> Vec<StoryNote> {
    let revision = revision(&story.version);
    story
        .notes
        .iter()
        .enumerate()
        .map(|(index, note)| StoryNote {
            id: format!("{}:{index}", story.id),
            story_id: story.id.clone(),
            story_revision: revision,
            text: note.text.clone(),
            actor: StoryTransitionActor::LocalApi,
            created_at: chrono::DateTime::parse_from_rfc3339(&note.at)
                .map(|at| at.timestamp_millis())
                .unwrap_or(0),
            receipts: Vec::new(),
        })
        .collect()
}
