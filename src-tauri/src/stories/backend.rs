//! The story backend port: story content behind one interface, so a second backend can exist.
//!
//! The port owns content: plans, stories, criteria text, dependencies, notes and archive
//! flags. Execution state (claims, transitions with their review/QA sub-states, transition
//! history, receipts, idempotency keys) stays in TUIC's own SQLite ledger, reached through
//! [`execution_ledger`], whatever backend holds the content. The port's transition
//! operations are what the workflow engine calls: each adapter writes its content and
//! the ledger together, so a run reads and moves stories through its project's backend.

use super::{
    ArchiveEvent, NewPlan, NewStory, Plan, PlanPatch, PlanState, PlanView, ProjectStoryView, Story,
    StoryCommand, StoryFilter, StoryListFilter, StoryListPage, StoryNote, StoryPatch,
    StoryReceiptReference, StoryStore, StoryTransitionActor,
};
#[cfg(test)]
use serde::Deserialize;
use serde::Serialize;
use std::borrow::Cow;

pub trait StoryBackend: Send + Sync {
    fn create_plan(&self, input: NewPlan) -> Result<Plan, String>;
    /// Atomic native issue preparation; unsupported adapters refuse before writing.
    fn create_plan_once(&self, _run: &str, _key: &str, _input: NewPlan) -> Result<Plan, String> {
        Err("story backend does not support exact plan creation".into())
    }
    fn existing_plan_for_proposal(
        &self,
        _run: &str,
        _key: &str,
        _input: &NewPlan,
    ) -> Result<Option<Plan>, String> {
        Err("story backend does not support exact plan observation".into())
    }
    fn create_story_with_problem_once(
        &self,
        run: &str,
        key: &str,
        input: NewStory,
        problem: Option<String>,
    ) -> Result<Story, String> {
        if problem.is_some() {
            return Err("story backend does not support exact problem-preserving creation".into());
        }
        self.create_story_once(run, key, input)
    }
    fn existing_story_with_problem_for_proposal(
        &self,
        run: &str,
        key: &str,
        input: &NewStory,
        problem: Option<&str>,
    ) -> Result<Option<Story>, String> {
        if problem.is_some() {
            return Err(
                "story backend does not support exact problem-preserving observation".into(),
            );
        }
        self.existing_story_for_proposal(run, key, input)
    }
    fn get_plan(&self, id: &str) -> Result<Plan, String>;
    fn require_unarchived_plan(&self, id: &str) -> Result<Plan, String>;
    fn list_plans(&self, project: &str) -> Result<Vec<Plan>, String>;
    fn list_plans_archived(&self, project: &str, archived: bool) -> Result<Vec<Plan>, String>;
    fn update_plan(
        &self,
        id: &str,
        expected_revision: i64,
        patch: PlanPatch,
    ) -> Result<Plan, String>;
    fn plan_state(&self, plan_id: &str) -> Result<PlanState, String>;
    fn plan_view(&self, plan_id: &str) -> Result<PlanView, String>;
    fn filtered_plan_view(&self, plan_id: &str, filter: &StoryFilter) -> Result<PlanView, String>;
    fn project_story_view(
        &self,
        project: &str,
        archived_plans: bool,
        filter: &StoryFilter,
    ) -> Result<ProjectStoryView, String>;
    fn create_story_with_problem(
        &self,
        input: NewStory,
        problem: Option<String>,
    ) -> Result<Story, String>;
    fn get_story(&self, id: &str) -> Result<Story, String>;
    fn require_unarchived_story(&self, id: &str) -> Result<Story, String>;
    fn list_stories(&self, plan_id: &str) -> Result<Vec<Story>, String>;
    fn list_stories_filtered(
        &self,
        plan_id: &str,
        filter: &StoryListFilter,
        limit: Option<u16>,
        cursor: Option<&str>,
    ) -> Result<StoryListPage, String>;
    fn update_story(
        &self,
        story_id: &str,
        expected_revision: i64,
        patch: StoryPatch,
    ) -> Result<Story, String>;
    fn add_dependency(
        &self,
        story_id: &str,
        dependency_id: &str,
        expected_revision: i64,
    ) -> Result<Story, String>;
    fn remove_dependency(
        &self,
        story_id: &str,
        dependency_id: &str,
        expected_revision: i64,
        actor_session: Option<&str>,
    ) -> Result<Story, String>;
    fn add_note(
        &self,
        story_id: &str,
        expected_revision: i64,
        text: &str,
        actor: StoryTransitionActor,
        receipts: Vec<StoryReceiptReference>,
    ) -> Result<StoryNote, String>;
    fn list_notes(&self, story_id: &str) -> Result<Vec<StoryNote>, String>;
    fn set_story_archived(
        &self,
        id: &str,
        expected_revision: i64,
        archived: bool,
        actor: StoryTransitionActor,
    ) -> Result<Story, String>;
    fn set_plan_archived(
        &self,
        id: &str,
        expected_revision: i64,
        archived: bool,
        actor: StoryTransitionActor,
    ) -> Result<Plan, String>;
    fn archive_history(&self, id: &str) -> Result<Vec<ArchiveEvent>, String>;
    /// A transition by a managed session, or by a person when `actor_session` is `None`.
    fn transition_for_actor(
        &self,
        story_id: &str,
        expected_revision: i64,
        command: StoryCommand,
        actor_session: Option<&str>,
    ) -> Result<Story, String>;
    /// A batch critic FIX reopens the owning Done story.
    fn reopen_for_workflow(
        &self,
        story_id: &str,
        revision: i64,
        run_id: &str,
    ) -> Result<Story, String>;
    /// A coordinator's story proposal: one story per (run, key). A retry returns the
    /// first story; the same key with another payload is rejected.
    fn create_story_once(
        &self,
        run_id: &str,
        proposal_key: &str,
        input: NewStory,
    ) -> Result<Story, String>;
    /// The story that a proposal already created, without creating one.
    fn existing_story_for_proposal(
        &self,
        run_id: &str,
        proposal_key: &str,
        input: &NewStory,
    ) -> Result<Option<Story>, String>;
}

/// What the selected backend supports, so the UI and MCP can disable the rest.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct StoryCapabilities {
    /// `native`, or `external:<adapter id>`.
    pub backend: Cow<'static, str>,
    pub operations: StoryOperations,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[cfg_attr(test, derive(Deserialize))]
pub struct StoryOperations {
    pub create_plan: bool,
    pub update_plan: bool,
    pub archive_plan: bool,
    pub create_story: bool,
    pub update_story: bool,
    #[serde(default)]
    pub story_edits: StoryEdits,
    pub archive_story: bool,
    pub dependencies: bool,
    pub notes: bool,
    pub transitions: bool,
}

/// Additive field-level edit support. Missing flags do not grant external writes.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize)]
#[cfg_attr(test, derive(Deserialize))]
#[serde(default)]
pub struct StoryEdits {
    pub title: bool,
    pub priority: bool,
    pub problem: bool,
    pub criteria_add: bool,
    pub single_change: bool,
}

/// The native SQLite adapter supports every operation. Static: answering it opens no database.
pub const NATIVE_CAPABILITIES: StoryCapabilities = StoryCapabilities {
    backend: Cow::Borrowed("native"),
    operations: StoryOperations {
        create_plan: true,
        update_plan: true,
        archive_plan: true,
        create_story: true,
        update_story: true,
        story_edits: StoryEdits {
            title: true,
            priority: true,
            problem: true,
            criteria_add: true,
            single_change: false,
        },
        archive_story: true,
        dependencies: true,
        notes: true,
        transitions: true,
    },
};

/// The native store, for code that has no project. A workflow run has one: it
/// reads and moves stories through [`story_backend_for`] its project.
pub fn story_backend() -> Result<Box<dyn StoryBackend>, String> {
    Ok(Box::new(StoryStore::open()?))
}

/// The backend the settings bind to a canonical project path. An external
/// backend that cannot start answers `unavailable`; there is no fallback to SQLite.
pub fn story_backend_for(project: &str) -> Result<Box<dyn StoryBackend>, String> {
    let project = std::path::PathBuf::from(project);
    match crate::story_adapter::binding::external_for(&project)? {
        Some(external) => Ok(Box::new(crate::story_adapter::ExternalStories::new(
            external, project,
        ))),
        None => story_backend(),
    }
}

/// What the project's backend supports. An external adapter is started to read
/// its `initialize` answer; an adapter that is down is an error, not "native".
pub fn story_capabilities_for(project: &str) -> Result<StoryCapabilities, String> {
    let project = crate::progress::resolve_owning_project(Some(project))?;
    match crate::story_adapter::binding::external_for(&project)? {
        Some(external) => Ok(crate::story_adapter::external_capabilities(
            &external.info().map_err(|error| error.to_string())?,
        )),
        None => Ok(NATIVE_CAPABILITIES),
    }
}

/// TUIC-owned execution state for stories, always in the native SQLite store.
pub fn execution_ledger() -> Result<StoryStore, String> {
    StoryStore::open()
}

impl StoryBackend for StoryStore {
    fn create_plan_once(&self, run: &str, key: &str, input: NewPlan) -> Result<Plan, String> {
        StoryStore::create_plan_once(self, run, key, input)
    }
    fn existing_plan_for_proposal(
        &self,
        run: &str,
        key: &str,
        input: &NewPlan,
    ) -> Result<Option<Plan>, String> {
        StoryStore::existing_plan_for_proposal(self, run, key, input)
    }
    fn create_story_with_problem_once(
        &self,
        run: &str,
        key: &str,
        input: NewStory,
        problem: Option<String>,
    ) -> Result<Story, String> {
        StoryStore::create_story_with_problem_once(self, run, key, input, problem)
    }
    fn existing_story_with_problem_for_proposal(
        &self,
        run: &str,
        key: &str,
        input: &NewStory,
        problem: Option<&str>,
    ) -> Result<Option<Story>, String> {
        StoryStore::existing_story_with_problem_for_proposal(self, run, key, input, problem)
    }

    fn create_plan(&self, input: NewPlan) -> Result<Plan, String> {
        StoryStore::create_plan(self, input)
    }
    fn get_plan(&self, id: &str) -> Result<Plan, String> {
        StoryStore::get_plan(self, id)
    }
    fn require_unarchived_plan(&self, id: &str) -> Result<Plan, String> {
        StoryStore::require_unarchived_plan(self, id)
    }
    fn list_plans(&self, project: &str) -> Result<Vec<Plan>, String> {
        StoryStore::list_plans(self, project)
    }
    fn list_plans_archived(&self, project: &str, archived: bool) -> Result<Vec<Plan>, String> {
        StoryStore::list_plans_archived(self, project, archived)
    }
    fn update_plan(
        &self,
        id: &str,
        expected_revision: i64,
        patch: PlanPatch,
    ) -> Result<Plan, String> {
        StoryStore::update_plan(self, id, expected_revision, patch)
    }
    fn plan_state(&self, plan_id: &str) -> Result<PlanState, String> {
        StoryStore::plan_state(self, plan_id)
    }
    fn plan_view(&self, plan_id: &str) -> Result<PlanView, String> {
        StoryStore::plan_view(self, plan_id)
    }
    fn filtered_plan_view(&self, plan_id: &str, filter: &StoryFilter) -> Result<PlanView, String> {
        StoryStore::filtered_plan_view(self, plan_id, filter)
    }
    fn project_story_view(
        &self,
        project: &str,
        archived_plans: bool,
        filter: &StoryFilter,
    ) -> Result<ProjectStoryView, String> {
        StoryStore::project_story_view(self, project, archived_plans, filter)
    }
    fn create_story_with_problem(
        &self,
        input: NewStory,
        problem: Option<String>,
    ) -> Result<Story, String> {
        StoryStore::create_story_with_problem(self, input, problem)
    }
    fn get_story(&self, id: &str) -> Result<Story, String> {
        StoryStore::get_story(self, id)
    }
    fn require_unarchived_story(&self, id: &str) -> Result<Story, String> {
        StoryStore::require_unarchived_story(self, id)
    }
    fn list_stories(&self, plan_id: &str) -> Result<Vec<Story>, String> {
        StoryStore::list_stories(self, plan_id)
    }
    fn list_stories_filtered(
        &self,
        plan_id: &str,
        filter: &StoryListFilter,
        limit: Option<u16>,
        cursor: Option<&str>,
    ) -> Result<StoryListPage, String> {
        StoryStore::list_stories_filtered(self, plan_id, filter, limit, cursor)
    }
    fn update_story(
        &self,
        story_id: &str,
        expected_revision: i64,
        patch: StoryPatch,
    ) -> Result<Story, String> {
        StoryStore::update_story(self, story_id, expected_revision, patch)
    }
    fn add_dependency(
        &self,
        story_id: &str,
        dependency_id: &str,
        expected_revision: i64,
    ) -> Result<Story, String> {
        StoryStore::add_dependency(self, story_id, dependency_id, expected_revision)
    }
    fn remove_dependency(
        &self,
        story_id: &str,
        dependency_id: &str,
        expected_revision: i64,
        actor_session: Option<&str>,
    ) -> Result<Story, String> {
        StoryStore::remove_dependency(
            self,
            story_id,
            dependency_id,
            expected_revision,
            actor_session,
        )
    }
    fn add_note(
        &self,
        story_id: &str,
        expected_revision: i64,
        text: &str,
        actor: StoryTransitionActor,
        receipts: Vec<StoryReceiptReference>,
    ) -> Result<StoryNote, String> {
        StoryStore::add_note(self, story_id, expected_revision, text, actor, receipts)
    }
    fn list_notes(&self, story_id: &str) -> Result<Vec<StoryNote>, String> {
        StoryStore::list_notes(self, story_id)
    }
    fn set_story_archived(
        &self,
        id: &str,
        expected_revision: i64,
        archived: bool,
        actor: StoryTransitionActor,
    ) -> Result<Story, String> {
        StoryStore::set_story_archived(self, id, expected_revision, archived, actor)
    }
    fn set_plan_archived(
        &self,
        id: &str,
        expected_revision: i64,
        archived: bool,
        actor: StoryTransitionActor,
    ) -> Result<Plan, String> {
        StoryStore::set_plan_archived(self, id, expected_revision, archived, actor)
    }
    fn archive_history(&self, id: &str) -> Result<Vec<ArchiveEvent>, String> {
        StoryStore::archive_history(self, id)
    }
    fn transition_for_actor(
        &self,
        story_id: &str,
        expected_revision: i64,
        command: StoryCommand,
        actor_session: Option<&str>,
    ) -> Result<Story, String> {
        StoryStore::transition_for_actor(self, story_id, expected_revision, command, actor_session)
    }
    fn reopen_for_workflow(
        &self,
        story_id: &str,
        revision: i64,
        run_id: &str,
    ) -> Result<Story, String> {
        StoryStore::reopen_for_workflow(self, story_id, revision, run_id)
    }
    fn create_story_once(
        &self,
        run_id: &str,
        proposal_key: &str,
        input: NewStory,
    ) -> Result<Story, String> {
        StoryStore::create_story_once(self, run_id, proposal_key, input)
    }
    fn existing_story_for_proposal(
        &self,
        run_id: &str,
        proposal_key: &str,
        input: &NewStory,
    ) -> Result<Option<Story>, String> {
        StoryStore::existing_story_for_proposal(self, run_id, proposal_key, input)
    }
}

#[cfg(test)]
#[path = "backend_tests.rs"]
mod tests;
