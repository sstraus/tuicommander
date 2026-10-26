use super::{
    CRITERIA_INCOMPLETE, NewPlan, NewStory, PLAN_DOCUMENT_NOT_FOUND, Plan, PlanSource, PlanState,
    PlanView, Story, StoryCommand, StoryListFilter, StoryListPage, StoryPatch, StoryStatus,
    StoryStore, StorySummary, StoryTransition,
};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

/// One call of the story tool. Field names are snake_case here, camelCase inside `input`.
#[derive(Clone, Debug, Deserialize, Serialize, JsonSchema)]
#[serde(tag = "action", rename_all = "snake_case", deny_unknown_fields)]
pub enum StoryAction {
    /// Add an Idea in the envelope project. Source defaults to the bound session.
    AddIdea {
        text: String,
        source: Option<String>,
    },
    /// List project and global Ideas, matching the panel filter.
    ListIdeas,
    /// Change text without replacing metadata or image assets; project-owned ids only.
    UpdateIdea {
        idea_id: String,
        text: String,
    },
    /// Create a plan record for a plan file (`source` is its repo-relative path).
    CreatePlan {
        title: String,
        source: String,
    },
    /// Atomically correct plan metadata using its current stored revision.
    UpdatePlan {
        plan_id: String,
        expected_revision: i64,
        patch: super::PlanPatch,
    },
    /// List the plans of the calling session's project.
    ListPlans {
        #[serde(default)]
        archived: bool,
    },
    /// List the plan files found in the project that can become plans.
    ListPlanSources,
    /// Create the plan for a plan file, deriving its title; returns the existing plan if there is one.
    AddPlanSource {
        /// Repo-relative path from list_plan_sources.
        source: String,
    },
    GetPlan {
        plan_id: String,
    },
    PlanState {
        plan_id: String,
    },
    PlanView {
        plan_id: String,
    },
    /// Counts over the active plans of the project: plans, in_progress stories, done and total stories.
    Summary,
    /// Read the complete plan with Rust-filtered list ids and Kanban columns.
    FilteredPlanView {
        plan_id: String,
        filter: super::StoryFilter,
    },
    CreateStory {
        input: NewStory,
    },
    /// Amend an unclaimed backlog/ready story atomically at its current revision.
    UpdateStory {
        story_id: String,
        expected_revision: i64,
        patch: StoryPatch,
    },
    /// Optional conjunctive filters; limit opts into bounded id-order keyset pages.
    ListStories {
        plan_id: String,
        state: Option<StoryStatus>,
        #[schemars(range(min = 1, max = 3))]
        priority: Option<u8>,
        story_type: Option<super::StoryType>,
        #[serde(default)]
        archived: bool,
        #[schemars(range(min = 1, max = 100))]
        limit: Option<u16>,
        cursor: Option<String>,
    },
    /// Append commentary without changing the subject revision or status.
    AddNote {
        story_id: String,
        expected_revision: i64,
        /// Nonblank text, at most 16000 characters.
        text: String,
        /// At most 16 distinct persisted workflow receipts in the same project and plan.
        #[serde(default)]
        receipts: Vec<super::StoryReceiptReference>,
    },
    /// Read immutable notes in append order.
    ListNotes {
        story_id: String,
    },
    ArchiveStory {
        story_id: String,
        expected_revision: i64,
    },
    UnarchiveStory {
        story_id: String,
        expected_revision: i64,
    },
    ArchivePlan {
        plan_id: String,
        expected_revision: i64,
    },
    UnarchivePlan {
        plan_id: String,
        expected_revision: i64,
    },
    StoryArchiveHistory {
        story_id: String,
    },
    PlanArchiveHistory {
        plan_id: String,
    },
    GetStory {
        story_id: String,
    },
    TransitionHistory {
        story_id: String,
    },
    /// Make `story_id` depend on `dependency_id` (same plan, story in backlog or ready). If the
    /// dependency is not done, a ready story moves to backlog and cannot be claimed until it is.
    AddDependency {
        story_id: String,
        dependency_id: String,
        /// The story's current `revision`.
        expected_revision: i64,
    },
    /// Removes a cancelled (wont_fix) dependency.
    RemoveDependency {
        story_id: String,
        dependency_id: String,
        /// The story's current `revision`.
        expected_revision: i64,
    },
    /// Claim a ready story for the calling live session; moves it to in_progress.
    Claim {
        story_id: String,
        /// The story's current `revision`.
        expected_revision: i64,
    },
    /// Apply a status command and record the calling actor without restricting its authority.
    Transition {
        story_id: String,
        /// The story's current `revision`.
        expected_revision: i64,
        command: StoryCommand,
    },
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(tag = "type", content = "value", rename_all = "snake_case")]
pub enum StoryReply {
    Idea(serde_json::Value),
    Ideas(Vec<serde_json::Value>),
    Plan(Plan),
    Plans(Vec<Plan>),
    PlanSources(Vec<PlanSource>),
    PlanState(PlanState),
    PlanView(PlanView),
    Summary(StorySummary),
    Story(Story),
    Transitions(Vec<StoryTransition>),
    Note(super::StoryNote),
    Notes(Vec<super::StoryNote>),
    ArchiveHistory(Vec<super::ArchiveEvent>),
    // This reply already contains its type tag so the cursor is beside, not inside, value.
    #[serde(untagged)]
    Stories(StoryListPage),
}

/// True for an error the user can fix, including a plan source still used by a coordinator.
pub fn is_user_error(message: &str) -> bool {
    [
        "invalid idea",
        PLAN_DOCUMENT_NOT_FOUND,
        super::PLAN_SOURCE_IN_USE,
        CRITERIA_INCOMPLETE,
        super::STORY_LIST_QUERY_INVALID,
        super::STORY_NOTE_INVALID,
    ]
    .iter()
    .any(|prefix| message.starts_with(prefix))
}

/// All transports use this boundary, so an identifier alone never grants cross-project access.
#[cfg(test)]
pub fn story_action(
    project: &str,
    action: StoryAction,
    actor_session: Option<&str>,
) -> Result<StoryReply, String> {
    story_action_with_source(project, action, actor_session, false)
}

fn story_action_with_source(
    project: &str,
    action: StoryAction,
    actor_session: Option<&str>,
    unauthenticated_http: bool,
) -> Result<StoryReply, String> {
    if !crate::fs::is_absolute_on_any_platform(project) {
        return Err("project must be an absolute path".into());
    }
    let project = crate::progress::resolve_owning_project(Some(project))?
        .to_string_lossy()
        .to_string();
    let store = StoryStore::open()?;
    let plan_in_project = |plan_id: &str| -> Result<Plan, String> {
        let plan = store.get_plan(plan_id)?;
        if plan.project != project {
            return Err("plan does not belong to project".into());
        }
        Ok(plan)
    };
    let story_in_project = |story_id: &str| -> Result<Story, String> {
        let story = store.get_story(story_id)?;
        plan_in_project(&story.plan_id)?;
        Ok(story)
    };
    match action {
        StoryAction::AddIdea { text, source } => Ok(StoryReply::Idea(crate::config::add_idea(
            &project,
            &text,
            source.as_deref().or(actor_session).unwrap_or("local-api"),
        )?)),
        StoryAction::ListIdeas => Ok(StoryReply::Ideas(crate::config::list_ideas(&project)?)),
        StoryAction::UpdateIdea { idea_id, text } => Ok(StoryReply::Idea(
            crate::config::update_idea(&project, &idea_id, &text)?,
        )),
        StoryAction::CreatePlan { title, source } => {
            Ok(StoryReply::Plan(store.create_plan(NewPlan {
                project: project.clone(),
                title,
                source,
            })?))
        }
        StoryAction::UpdatePlan {
            plan_id,
            expected_revision,
            patch,
        } => {
            plan_in_project(&plan_id)?;
            Ok(StoryReply::Plan(store.update_plan(
                &plan_id,
                expected_revision,
                patch,
            )?))
        }
        StoryAction::ListPlans { archived } => Ok(StoryReply::Plans(
            store.list_plans_archived(&project, archived)?,
        )),
        StoryAction::ListPlanSources => Ok(StoryReply::PlanSources(
            super::sources::list_plan_sources(std::path::Path::new(&project))?,
        )),
        StoryAction::AddPlanSource { source } => {
            if let Some(plan) = store
                .list_plans(&project)?
                .into_iter()
                .chain(store.list_plans_archived(&project, true)?)
                .find(|plan| plan.source == source)
            {
                return Ok(StoryReply::Plan(plan));
            }
            let title = super::sources::title_for_source(std::path::Path::new(&project), &source)?;
            Ok(StoryReply::Plan(store.create_plan(NewPlan {
                project: project.clone(),
                title,
                source,
            })?))
        }
        StoryAction::GetPlan { plan_id } => Ok(StoryReply::Plan(plan_in_project(&plan_id)?)),
        StoryAction::PlanState { plan_id } => {
            plan_in_project(&plan_id)?;
            Ok(StoryReply::PlanState(store.plan_state(&plan_id)?))
        }
        StoryAction::FilteredPlanView { plan_id, filter } => {
            plan_in_project(&plan_id)?;
            Ok(StoryReply::PlanView(
                store.filtered_plan_view(&plan_id, &filter)?,
            ))
        }
        StoryAction::PlanView { plan_id } => {
            plan_in_project(&plan_id)?;
            Ok(StoryReply::PlanView(store.plan_view(&plan_id)?))
        }
        StoryAction::Summary => {
            let mut summary = StorySummary::default();
            for plan in store.list_plans(&project)? {
                let view = store.plan_view(&plan.id)?;
                if view.state != PlanState::Active {
                    continue;
                }
                summary.active_plans += 1;
                for read in &view.stories {
                    if read.abandoned {
                        continue;
                    }
                    match read.story.status {
                        StoryStatus::WontFix => continue,
                        StoryStatus::InProgress => summary.in_progress += 1,
                        StoryStatus::Qa => summary.qa += 1,
                        StoryStatus::Done => summary.done += 1,
                        _ => {}
                    }
                    summary.total += 1;
                }
            }
            Ok(StoryReply::Summary(summary))
        }
        StoryAction::CreateStory { input } => {
            plan_in_project(&input.plan_id)?;
            Ok(StoryReply::Story(store.create_story(input)?))
        }
        StoryAction::UpdateStory {
            story_id,
            expected_revision,
            patch,
        } => {
            story_in_project(&story_id)?;
            Ok(StoryReply::Story(store.update_story(
                &story_id,
                expected_revision,
                patch,
            )?))
        }
        StoryAction::ListStories {
            plan_id,
            state,
            priority,
            story_type,
            archived,
            limit,
            cursor,
        } => {
            plan_in_project(&plan_id)?;
            Ok(StoryReply::Stories(store.list_stories_filtered(
                &plan_id,
                &StoryListFilter {
                    archived,
                    state,
                    priority,
                    story_type,
                },
                limit,
                cursor.as_deref(),
            )?))
        }
        StoryAction::AddNote {
            story_id,
            expected_revision,
            text,
            receipts,
        } => {
            story_in_project(&story_id)?;
            let actor = match actor_session {
                Some(session_id) => super::StoryTransitionActor::ManagedSession {
                    session_id: session_id.into(),
                },
                None if unauthenticated_http => super::StoryTransitionActor::LocalApi,
                None => super::StoryTransitionActor::Human,
            };
            Ok(StoryReply::Note(store.add_note(
                &story_id,
                expected_revision,
                &text,
                actor,
                receipts,
            )?))
        }
        StoryAction::ListNotes { story_id } => {
            story_in_project(&story_id)?;
            Ok(StoryReply::Notes(store.list_notes(&story_id)?))
        }
        StoryAction::ArchiveStory {
            ref story_id,
            expected_revision,
        }
        | StoryAction::UnarchiveStory {
            ref story_id,
            expected_revision,
        } => {
            story_in_project(story_id)?;
            let archived = matches!(action, StoryAction::ArchiveStory { .. });
            Ok(StoryReply::Story(store.set_story_archived(
                story_id,
                expected_revision,
                archived,
                archive_actor(actor_session, unauthenticated_http),
            )?))
        }
        StoryAction::ArchivePlan {
            ref plan_id,
            expected_revision,
        }
        | StoryAction::UnarchivePlan {
            ref plan_id,
            expected_revision,
        } => {
            plan_in_project(plan_id)?;
            let archived = matches!(action, StoryAction::ArchivePlan { .. });
            Ok(StoryReply::Plan(store.set_plan_archived(
                plan_id,
                expected_revision,
                archived,
                archive_actor(actor_session, unauthenticated_http),
            )?))
        }
        StoryAction::StoryArchiveHistory { story_id } => {
            story_in_project(&story_id)?;
            Ok(StoryReply::ArchiveHistory(
                store.archive_history(&story_id)?,
            ))
        }
        StoryAction::PlanArchiveHistory { plan_id } => {
            plan_in_project(&plan_id)?;
            Ok(StoryReply::ArchiveHistory(store.archive_history(&plan_id)?))
        }
        StoryAction::GetStory { story_id } => Ok(StoryReply::Story(story_in_project(&story_id)?)),
        StoryAction::TransitionHistory { story_id } => {
            story_in_project(&story_id)?;
            Ok(StoryReply::Transitions(
                store.transition_history(&story_id)?,
            ))
        }
        StoryAction::AddDependency {
            story_id,
            dependency_id,
            expected_revision,
        } => {
            story_in_project(&story_id)?;
            story_in_project(&dependency_id)?;
            Ok(StoryReply::Story(store.add_dependency(
                &story_id,
                &dependency_id,
                expected_revision,
            )?))
        }
        StoryAction::RemoveDependency {
            story_id,
            dependency_id,
            expected_revision,
        } => {
            story_in_project(&story_id)?;
            story_in_project(&dependency_id)?;
            Ok(StoryReply::Story(store.remove_dependency(
                &story_id,
                &dependency_id,
                expected_revision,
                actor_session,
            )?))
        }
        StoryAction::Claim {
            story_id,
            expected_revision,
        } => {
            story_in_project(&story_id)?;
            let session = actor_session.ok_or("claim requires a live session")?;
            Ok(StoryReply::Story(store.claim(
                &story_id,
                session,
                expected_revision,
            )?))
        }
        StoryAction::Transition {
            story_id,
            expected_revision,
            command,
        } => {
            story_in_project(&story_id)?;
            let changed = if unauthenticated_http && actor_session.is_none() {
                store.transition_from_local_api(&story_id, expected_revision, command)?
            } else {
                store.transition_for_actor(&story_id, expected_revision, command, actor_session)?
            };
            Ok(StoryReply::Story(changed))
        }
    }
}

/// A claim is bound to a live PTY in the same registered project.
pub fn story_action_for_session(
    state: &crate::AppState,
    project: &str,
    action: StoryAction,
    session_id: Option<&str>,
) -> Result<StoryReply, String> {
    story_action_for_session_with_source(state, project, action, session_id, false)
}

pub(crate) fn story_action_for_http_authenticated(
    state: &crate::AppState,
    project: &str,
    action: StoryAction,
    session_id: Option<&str>,
    authenticated_user: bool,
) -> Result<StoryReply, String> {
    story_action_for_session_with_source(state, project, action, session_id, !authenticated_user)
}

fn story_action_for_session_with_source(
    state: &crate::AppState,
    project: &str,
    action: StoryAction,
    session_id: Option<&str>,
    unauthenticated_http: bool,
) -> Result<StoryReply, String> {
    if let Some(session) = session_id {
        let session_project = crate::progress::project_for_session(state, session)
            .ok_or("session has no registered project")?;
        let session_owner = crate::progress::resolve_owning_project(Some(&session_project))?;
        let requested_owner = crate::progress::resolve_owning_project(Some(project))?;
        if session_owner != requested_owner {
            return Err("session does not belong to project".into());
        }
    }
    if matches!(action, StoryAction::Claim { .. }) && session_id.is_none() {
        return Err("claim requires a live session".into());
    }
    let changes_ideas = matches!(
        action,
        StoryAction::AddIdea { .. } | StoryAction::UpdateIdea { .. }
    );
    let reply = story_action_with_source(project, action, session_id, unauthenticated_http)?;
    if changes_ideas {
        state.notify_ideas_changed();
    }
    Ok(reply)
}

fn archive_actor(session: Option<&str>, unauthenticated_http: bool) -> super::StoryTransitionActor {
    match session {
        Some(id) => super::StoryTransitionActor::ManagedSession {
            session_id: id.into(),
        },
        None if unauthenticated_http => super::StoryTransitionActor::LocalApi,
        None => super::StoryTransitionActor::Human,
    }
}

#[cfg(test)]
mod tests {
    use super::super::StoryOrigin;
    use super::*;

    /// Catches: the missing update_plan action prevents correcting either stored field.
    #[test]
    fn update_plan_changes_title_and_source() {
        let config = tempfile::tempdir_in(crate::test_support::test_temp_root()).unwrap();
        let _guard = crate::config::set_config_dir_override(config.path().into());
        let repo = tempfile::tempdir_in(crate::test_support::test_temp_root()).unwrap();
        let project = repo.path().to_str().unwrap();
        let StoryReply::Plan(plan) = story_action(
            project,
            StoryAction::CreatePlan {
                title: "Original".into(),
                source: "original.md".into(),
            },
            None,
        )
        .unwrap() else {
            panic!("plan")
        };
        let original = serde_json::to_value(&plan).unwrap();
        let action = serde_json::from_value(serde_json::json!({
            "action": "update_plan", "plan_id": plan.id,
            "expected_revision": original["revision"].as_i64().unwrap_or(1),
            "patch": {"title": "Corrected", "source": "moved.md"}
        }))
        .expect("update_plan must be a supported action");
        let StoryReply::Plan(updated) = story_action(project, action, None).unwrap() else {
            panic!("plan")
        };
        assert_eq!(updated.id, plan.id);
        assert_eq!(updated.title, "Corrected");
        assert_eq!(updated.source, "moved.md");
        let wire = serde_json::to_value(&updated).unwrap();
        assert_eq!(wire["revision"], 2);
        assert_eq!(
            StoryStore::open().unwrap().get_plan(&plan.id).unwrap(),
            updated
        );
    }

    /// Catches: an identifier permitting an update to another project's plan.
    #[test]
    fn update_plan_rejects_cross_project_writes() {
        let config = tempfile::tempdir_in(crate::test_support::test_temp_root()).unwrap();
        let _guard = crate::config::set_config_dir_override(config.path().into());
        let repo = tempfile::tempdir_in(crate::test_support::test_temp_root()).unwrap();
        let other = tempfile::tempdir_in(crate::test_support::test_temp_root()).unwrap();
        let project = repo.path().to_str().unwrap();
        let StoryReply::Plan(plan) = story_action(
            project,
            StoryAction::CreatePlan {
                title: "Owned".into(),
                source: "plan.md".into(),
            },
            None,
        )
        .unwrap() else {
            panic!("plan")
        };
        let result = story_action(
            other.path().to_str().unwrap(),
            StoryAction::UpdatePlan {
                plan_id: plan.id.clone(),
                expected_revision: plan.revision,
                patch: super::super::PlanPatch {
                    title: Some("Wrong".into()),
                    source: None,
                },
            },
            None,
        );
        assert!(result.unwrap_err().contains("does not belong to project"));
        assert_eq!(
            StoryStore::open().unwrap().get_plan(&plan.id).unwrap(),
            plan
        );
    }

    /// Catches: archive that only sets a flag the default list ignores.
    #[test]
    fn archived_story_is_hidden_from_default_list() {
        let config = tempfile::tempdir_in(crate::test_support::test_temp_root()).unwrap();
        let _guard = crate::config::set_config_dir_override(config.path().to_path_buf());
        let repo = tempfile::tempdir_in(crate::test_support::test_temp_root()).unwrap();
        let project = repo.path().to_str().unwrap();
        let (story, sibling) = two_stories(project);
        let action = serde_json::from_value(serde_json::json!({
            "action":"archive_story", "story_id":story.id,
            "expected_revision":story.revision
        }))
        .expect("archive action must be supported");
        story_action(project, action, None).expect("archive");
        let action = serde_json::from_value(serde_json::json!({
            "action":"list_stories", "plan_id":story.plan_id
        }))
        .unwrap();
        let StoryReply::Stories(page) = story_action(project, action, None).unwrap() else {
            panic!("stories");
        };
        assert_eq!(
            page.value.iter().map(|s| &s.id).collect::<Vec<_>>(),
            vec![&sibling.id]
        );
    }

    /// Catches: restore losing record contents, stale cleanup overwriting edits, or archive history disappearing on reopen.
    #[test]
    fn archive_round_trip_checks_revisions_and_preserves_history_and_contents() {
        let config = tempfile::tempdir_in(crate::test_support::test_temp_root()).unwrap();
        let _guard = crate::config::set_config_dir_override(config.path().to_path_buf());
        let repo = tempfile::tempdir_in(crate::test_support::test_temp_root()).unwrap();
        let project = repo.path().to_str().unwrap();
        let (story, _) = two_stories(project);
        let call = |value| story_action(project, serde_json::from_value(value).unwrap(), None);
        let archived = story_of(call(serde_json::json!({"action":"archive_story","story_id":story.id,"expected_revision":story.revision})).unwrap());
        assert!(archived.archived);
        assert_eq!(archived.revision, story.revision + 1);
        assert_eq!(archived.status, story.status);
        // Catches: metadata amendments silently rewriting an archived story or parent.
        assert!(call(serde_json::json!({"action":"update_story","story_id":story.id,"expected_revision":archived.revision,"patch":{"title":"Changed while archived"}})).unwrap_err().contains("archived"));
        assert!(call(serde_json::json!({"action":"unarchive_story","story_id":story.id,"expected_revision":story.revision})).unwrap_err().contains("stale"));
        let StoryReply::Stories(page) = call(serde_json::json!({"action":"list_stories","plan_id":story.plan_id,"archived":true,"limit":1})).unwrap() else { panic!("stories"); };
        assert_eq!(page.value, vec![archived.clone()]);
        let StoryReply::PlanView(view) = call(serde_json::json!({"action":"filtered_plan_view","plan_id":story.plan_id,"filter":{"archived":true}})).unwrap() else { panic!("view"); };
        assert_eq!(view.visible_story_ids, vec![story.id.clone()]);
        assert_eq!(view.stories[0].story, archived);
        assert_eq!(view.state, PlanState::Active);

        let restored = story_of(call(serde_json::json!({"action":"unarchive_story","story_id":story.id,"expected_revision":archived.revision})).unwrap());
        let mut expected = story.clone();
        expected.revision += 2;
        assert_eq!(restored, expected);
        let StoryReply::ArchiveHistory(events) =
            call(serde_json::json!({"action":"story_archive_history","story_id":story.id}))
                .unwrap()
        else {
            panic!("history");
        };
        assert_eq!(
            events
                .iter()
                .map(|e| (e.revision, e.archived))
                .collect::<Vec<_>>(),
            vec![(2, true), (3, false)]
        );
        assert!(
            events
                .iter()
                .all(|e| e.actor == super::super::StoryTransitionActor::Human)
        );
        let store = StoryStore::open().unwrap();
        let plan = store.get_plan(&story.plan_id).unwrap();
        let archived_plan = store
            .set_plan_archived(
                &plan.id,
                plan.revision,
                true,
                super::super::StoryTransitionActor::LocalApi,
            )
            .unwrap();
        assert_eq!(store.list_plans(project).unwrap(), vec![]);
        assert_eq!(
            store.list_plans_archived(project, true).unwrap(),
            vec![archived_plan.clone()]
        );
        assert!(
            store
                .set_plan_archived(
                    &plan.id,
                    plan.revision,
                    false,
                    super::super::StoryTransitionActor::Human
                )
                .unwrap_err()
                .contains("stale")
        );
        assert!(
            store
                .claim(&story.id, "session", restored.revision)
                .unwrap_err()
                .contains("archived")
        );
        assert!(call(serde_json::json!({"action":"update_story","story_id":story.id,"expected_revision":restored.revision,"patch":{"title":"Changed under archived plan"}})).unwrap_err().contains("archived"));
        let restored_plan = store
            .set_plan_archived(
                &plan.id,
                archived_plan.revision,
                false,
                super::super::StoryTransitionActor::Human,
            )
            .unwrap();
        assert_eq!(restored_plan.revision, plan.revision + 2);
        assert!(!restored_plan.archived);
        assert_eq!(store.get_story(&story.id).unwrap(), restored);
        assert_eq!(store.archive_history(&plan.id).unwrap().len(), 2);
    }

    /// Catches: continuing a cursor from active records into a different archive query.
    #[test]
    fn archive_filter_is_bound_to_story_list_cursor() {
        let config = tempfile::tempdir_in(crate::test_support::test_temp_root()).unwrap();
        let _guard = crate::config::set_config_dir_override(config.path().to_path_buf());
        let repo = tempfile::tempdir_in(crate::test_support::test_temp_root()).unwrap();
        let (story, _) = two_stories(repo.path().to_str().unwrap());
        let store = StoryStore::open().unwrap();
        let page = store
            .list_stories_filtered(&story.plan_id, &StoryListFilter::default(), Some(1), None)
            .unwrap();
        let filter = StoryListFilter {
            archived: true,
            ..Default::default()
        };
        assert!(
            store
                .list_stories_filtered(
                    &story.plan_id,
                    &filter,
                    Some(1),
                    page.next_cursor.as_deref()
                )
                .unwrap_err()
                .contains("does not match")
        );
    }

    /// Catches: cleanup releasing a live claim, or parent cleanup bypassing child ownership.
    #[test]
    fn archive_refuses_claims_and_archived_stories_cannot_restart() {
        let config = tempfile::tempdir_in(crate::test_support::test_temp_root()).unwrap();
        let _guard = crate::config::set_config_dir_override(config.path().to_path_buf());
        let repo = tempfile::tempdir_in(crate::test_support::test_temp_root()).unwrap();
        let (story, sibling) = two_stories(repo.path().to_str().unwrap());
        let store = StoryStore::open().unwrap();
        let claimed = store.claim(&story.id, "owner", story.revision).unwrap();
        let actor = super::super::StoryTransitionActor::Human;
        assert!(
            store
                .set_story_archived(&story.id, claimed.revision, true, actor.clone())
                .unwrap_err()
                .contains("claimed")
        );
        let plan = store.get_plan(&story.plan_id).unwrap();
        assert!(
            store
                .set_plan_archived(&plan.id, plan.revision, true, actor.clone())
                .unwrap_err()
                .contains("claimed")
        );
        assert_eq!(store.get_story(&story.id).unwrap(), claimed);
        assert_eq!(store.archive_history(&story.id).unwrap(), vec![]);
        let archived = store
            .set_story_archived(&sibling.id, sibling.revision, true, actor)
            .unwrap();
        assert!(
            store
                .claim(&sibling.id, "other", archived.revision)
                .unwrap_err()
                .contains("archived")
        );
        assert!(
            store
                .transition(&sibling.id, archived.revision, StoryCommand::StartManual)
                .unwrap_err()
                .contains("archived")
        );
        assert!(
            !store
                .plan_view(&plan.id)
                .unwrap()
                .visible_story_ids
                .contains(&sibling.id)
        );
    }

    /// Catches: archive and history actions accessing another project's record by id.
    #[test]
    fn archive_actions_preserve_project_boundaries() {
        let config = tempfile::tempdir_in(crate::test_support::test_temp_root()).unwrap();
        let _guard = crate::config::set_config_dir_override(config.path().to_path_buf());
        let repo = tempfile::tempdir_in(crate::test_support::test_temp_root()).unwrap();
        let other = tempfile::tempdir_in(crate::test_support::test_temp_root()).unwrap();
        let (story, _) = two_stories(repo.path().to_str().unwrap());
        for action in [
            "archive_story",
            "unarchive_story",
            "archive_plan",
            "unarchive_plan",
            "story_archive_history",
            "plan_archive_history",
        ] {
            let mut value = serde_json::json!({"action":action});
            if matches!(
                action,
                "archive_story" | "unarchive_story" | "story_archive_history"
            ) {
                value["story_id"] = story.id.clone().into();
            } else {
                value["plan_id"] = story.plan_id.clone().into();
            }
            if !action.ends_with("history") {
                value["expected_revision"] = 1.into();
            }
            let error = story_action(
                other.path().to_str().unwrap(),
                serde_json::from_value(value).unwrap(),
                None,
            )
            .unwrap_err();
            assert!(error.contains("does not belong"), "{action}: {error}");
        }
    }

    #[test]
    fn plan_sources_follow_repo_files_and_document_titles() {
        let config = tempfile::tempdir().expect("config");
        let _guard = crate::config::set_config_dir_override(config.path().to_path_buf());
        let repo = tempfile::tempdir().expect("project");
        std::fs::create_dir_all(repo.path().join("plans/archive")).expect("plan dirs");
        std::fs::create_dir_all(repo.path().join(".claude/plans")).expect("claude plans");
        std::fs::write(
            repo.path().join("plans/first.md"),
            "---\ntitle: Front title\n---\n# Other title\n",
        )
        .expect("frontmatter plan");
        std::fs::write(
            repo.path().join(".claude/plans/second.md"),
            "# Heading title\n",
        )
        .expect("heading plan");
        std::fs::write(repo.path().join("plans/archive/old.md"), "# Archived\n")
            .expect("archived plan");
        let project = repo.path().to_str().expect("project path");
        let list = || {
            let action = serde_json::from_value(serde_json::json!({"action":"list_plan_sources"}))
                .expect("list plan sources action");
            serde_json::to_value(story_action(project, action, None).expect("plan sources"))
                .expect("serialize sources")
        };
        let first = list();
        assert_eq!(first["type"], "plan_sources");
        assert_eq!(first["value"].as_array().expect("sources").len(), 2);
        assert!(
            first["value"]
                .as_array()
                .expect("sources")
                .iter()
                .any(|item| item["source"] == "plans/first.md" && item["title"] == "Front title")
        );
        assert!(
            first["value"]
                .as_array()
                .expect("sources")
                .iter()
                .any(|item| item["source"] == ".claude/plans/second.md"
                    && item["title"] == "Heading title")
        );

        std::fs::write(repo.path().join("plans/new.md"), "# Created by an agent\n")
            .expect("new plan");
        assert!(
            list()["value"]
                .as_array()
                .expect("refreshed sources")
                .iter()
                .any(|item| item["source"] == "plans/new.md"
                    && item["title"] == "Created by an agent")
        );
    }

    #[test]
    fn adding_a_plan_source_derives_its_title_and_reuses_its_record() {
        let config = tempfile::tempdir().expect("config");
        let _guard = crate::config::set_config_dir_override(config.path().to_path_buf());
        let repo = tempfile::tempdir().expect("project");
        std::fs::create_dir(repo.path().join("plans")).expect("plans dir");
        std::fs::write(repo.path().join("plans/feature.md"), "# Document title\n").expect("plan");
        let project = repo.path().to_str().expect("project path");
        let action = || {
            serde_json::from_value(serde_json::json!({
                "action":"add_plan_source", "source":"plans/feature.md"
            }))
            .expect("add plan source action")
        };
        let StoryReply::Plan(first) = story_action(project, action(), None).expect("first add")
        else {
            panic!("expected plan");
        };
        assert_eq!(first.title, "Document title");
        let StoryReply::Plan(second) = story_action(project, action(), None).expect("second add")
        else {
            panic!("expected plan");
        };
        assert_eq!(first.id, second.id);
    }

    #[test]
    fn story_actions_reject_unknown_fields() {
        assert!(
            serde_json::from_value::<StoryAction>(serde_json::json!({
                "action": "get_story", "story_id": "s", "project": "/other"
            }))
            .is_err()
        );
    }

    #[test]
    fn project_scopes_all_plan_and_story_reads() {
        let config = tempfile::tempdir().expect("config");
        let _guard = crate::config::set_config_dir_override(config.path().to_path_buf());
        let first = tempfile::tempdir().expect("first project");
        let second = tempfile::tempdir().expect("second project");
        let first_path = first.path().to_str().expect("first path");
        let second_path = second.path().to_str().expect("second path");

        let StoryReply::Plan(plan) = story_action(
            first_path,
            StoryAction::CreatePlan {
                title: "Plan".into(),
                source: "plans/feature.md".into(),
            },
            None,
        )
        .expect("create plan") else {
            panic!("expected plan");
        };
        assert!(matches!(
            story_action(
                &format!("{first_path}/."),
                StoryAction::GetPlan {
                    plan_id: plan.id.clone()
                },
                None,
            ),
            Ok(StoryReply::Plan(_))
        ));
        assert!(
            story_action(
                second_path,
                StoryAction::GetPlan {
                    plan_id: plan.id.clone()
                },
                None
            )
            .is_err()
        );
        assert!(
            matches!(story_action(second_path, StoryAction::ListPlans { archived: false }, None).expect("list"), StoryReply::Plans(plans) if plans.is_empty())
        );

        let StoryReply::Story(story) = story_action(
            first_path,
            StoryAction::CreateStory {
                input: NewStory {
                    story_type: None,
                    plan_id: plan.id.clone(),
                    title: "Work".into(),
                    criteria: vec!["Done".into()],
                    priority: 1,
                    origin: StoryOrigin::Native,
                    file_scope: vec![],
                },
            },
            None,
        )
        .expect("create story") else {
            panic!("expected story");
        };
        assert!(
            story_action(
                second_path,
                StoryAction::GetStory { story_id: story.id },
                None
            )
            .is_err()
        );
    }

    fn story_of(reply: StoryReply) -> Story {
        let StoryReply::Story(story) = reply else {
            panic!("expected story");
        };
        story
    }

    /// Two stories in one plan; the second is ready until it gains a dependency.
    fn two_stories(project: &str) -> (Story, Story) {
        let StoryReply::Plan(plan) = story_action(
            project,
            StoryAction::CreatePlan {
                title: "Plan".into(),
                source: "plans/feature.md".into(),
            },
            None,
        )
        .expect("plan") else {
            panic!("expected plan");
        };
        let create = |title: &str| {
            story_of(
                story_action(
                    project,
                    StoryAction::CreateStory {
                        input: NewStory {
                            story_type: None,
                            plan_id: plan.id.clone(),
                            title: title.into(),
                            criteria: vec!["Done".into()],
                            priority: 1,
                            origin: StoryOrigin::Native,
                            file_scope: vec![],
                        },
                    },
                    None,
                )
                .expect("story"),
            )
        };
        (create("First"), create("Second"))
    }

    /// Catches: a missing update arm in StoryAction across the shared transport boundary.
    #[test]
    fn update_story_changes_priority_on_shared_service() {
        let config = tempfile::tempdir_in(crate::test_support::test_temp_root()).unwrap();
        let _guard = crate::config::set_config_dir_override(config.path().to_path_buf());
        let repo = tempfile::tempdir_in(crate::test_support::test_temp_root()).unwrap();
        let project = repo.path().to_str().unwrap();
        let (story, _) = two_stories(project);
        assert_eq!(story.priority, 1);
        let action = serde_json::from_value(serde_json::json!({
            "action": "update_story",
            "story_id": story.id,
            "expected_revision": story.revision,
            "patch": {"priority": 3}
        }))
        .expect("update_story is a supported shared action");
        let updated = story_of(story_action(project, action, None).expect("update priority"));
        assert_eq!(updated.priority, 3);
        let persisted = story_of(
            story_action(project, StoryAction::GetStory { story_id: story.id }, None)
                .expect("read persisted story"),
        );
        assert_eq!(persisted.priority, 3);
        assert_eq!(persisted.revision, story.revision + 1);
    }

    /// Catches: accepting a typo as an omitted field, including nested origin fields.
    #[test]
    fn update_story_rejects_unknown_patch_and_origin_fields() {
        for patch in [
            serde_json::json!({"prioirty": 3}),
            serde_json::json!({"origin": {"type": "native", "step": "ignored"}}),
        ] {
            assert!(serde_json::from_value::<StoryAction>(serde_json::json!({
                "action": "update_story", "story_id": "s", "expected_revision": 1, "patch": patch
            })).is_err());
        }
    }

    /// Catches: stale or invalid patches persisting an earlier valid field before refusal.
    #[test]
    fn update_story_rolls_back_stale_and_invalid_dependency_patches() {
        let config = tempfile::tempdir_in(crate::test_support::test_temp_root()).unwrap();
        let _guard = crate::config::set_config_dir_override(config.path().to_path_buf());
        let repo = tempfile::tempdir_in(crate::test_support::test_temp_root()).unwrap();
        let project = repo.path().to_str().unwrap();
        let (story, dependency) = two_stories(project);
        let (foreign, _) = two_stories(project);
        let store = StoryStore::open().unwrap();
        for (revision, patch, message) in [
            (
                story.revision - 1,
                serde_json::json!({"priority": 3}),
                "stale",
            ),
            (
                story.revision,
                serde_json::json!({"priority": 3, "dependencies": [foreign.id]}),
                "one plan",
            ),
            (
                story.revision,
                serde_json::json!({"priority": 3, "dependencies": [story.id]}),
                "cycle",
            ),
            (
                story.revision,
                serde_json::json!({"priority": 3, "dependencies": [dependency.id, dependency.id]}),
                "duplicate",
            ),
            (
                story.revision,
                serde_json::json!({"priority": 3, "criteria": []}),
                "criteria",
            ),
            (
                story.revision,
                serde_json::json!({"priority": 0, "title": "Changed"}),
                "priority",
            ),
            (
                story.revision,
                serde_json::json!({"priority": 3, "fileScope": ["../outside"]}),
                "relative",
            ),
        ] {
            let patch = serde_json::from_value(patch).unwrap();
            let error = store.update_story(&story.id, revision, patch).unwrap_err();
            assert!(error.contains(message), "{error}");
            assert_eq!(store.get_story(&story.id).unwrap(), story);
        }
    }

    /// Catches: concurrent handles both accepting one revision and losing an amendment.
    #[test]
    fn update_story_allows_only_one_writer_at_the_expected_revision() {
        let config = tempfile::tempdir_in(crate::test_support::test_temp_root()).unwrap();
        let _guard = crate::config::set_config_dir_override(config.path().to_path_buf());
        let repo = tempfile::tempdir_in(crate::test_support::test_temp_root()).unwrap();
        let (story, _) = two_stories(repo.path().to_str().unwrap());
        let first = StoryStore::open().unwrap();
        let second = StoryStore::open().unwrap();
        let results = std::thread::scope(|scope| {
            let a = scope.spawn(|| {
                first.update_story(
                    &story.id,
                    story.revision,
                    StoryPatch {
                        priority: Some(2),
                        ..Default::default()
                    },
                )
            });
            let b = scope.spawn(|| {
                second.update_story(
                    &story.id,
                    story.revision,
                    StoryPatch {
                        priority: Some(3),
                        ..Default::default()
                    },
                )
            });
            [a.join().unwrap(), b.join().unwrap()]
        });
        assert_eq!(results.iter().filter(|result| result.is_ok()).count(), 1);
        assert!(
            results
                .iter()
                .filter_map(|result| result.as_ref().err())
                .all(|error| error.contains("stale") || error.contains("preflight")),
            "{results:?}"
        );
        let winner = results.into_iter().find_map(Result::ok).unwrap();
        assert_eq!(first.get_story(&story.id).unwrap(), winner);
        assert_eq!(winner.revision, story.revision + 1);
    }

    /// Catches: metadata edits losing checks, reordered requirements receiving another check,
    /// and changed/new or ambiguous duplicate requirements inheriting old proof.
    #[test]
    fn update_story_preserves_only_unchanged_unambiguous_criterion_checks() {
        let config = tempfile::tempdir_in(crate::test_support::test_temp_root()).unwrap();
        let _guard = crate::config::set_config_dir_override(config.path().to_path_buf());
        let repo = tempfile::tempdir_in(crate::test_support::test_temp_root()).unwrap();
        let project = repo.path().to_str().unwrap();
        let (story, _) = two_stories(project);
        let store = StoryStore::open().unwrap();
        let patch = |value| serde_json::from_value::<StoryPatch>(value).unwrap();
        let mut story = store
            .update_story(
                &story.id,
                story.revision,
                patch(serde_json::json!({
                    "criteria": ["First", "Second", "Duplicate", "Duplicate"]
                })),
            )
            .unwrap();
        for command in [
            StoryCommand::StartManual,
            StoryCommand::CheckCriterion(0),
            StoryCommand::CheckCriterion(1),
            StoryCommand::CheckCriterion(2),
            StoryCommand::CheckCriterion(3),
            StoryCommand::SubmitReview,
            StoryCommand::RejectReview,
        ] {
            story = store
                .transition(&story.id, story.revision, command)
                .unwrap();
        }
        assert_eq!(story.status, StoryStatus::Ready);
        let priority = store
            .update_story(
                &story.id,
                story.revision,
                patch(serde_json::json!({"priority": 3})),
            )
            .unwrap();
        assert_eq!(priority.checked, vec![true; 4]);
        let edited = store
            .update_story(
                &priority.id,
                priority.revision,
                patch(serde_json::json!({
                    "criteria": ["Second", "Changed first", "New", "Duplicate"]
                })),
            )
            .unwrap();
        assert_eq!(edited.checked, vec![true, false, false, false]);
        let duplicated = store
            .update_story(
                &edited.id,
                edited.revision,
                patch(serde_json::json!({
                    "criteria": ["Second", "Second"]
                })),
            )
            .unwrap();
        assert_eq!(duplicated.checked, vec![false, false]);
    }

    /// Catches: dependency amendment leaving readiness stale or allowing a transitive cycle.
    #[test]
    fn update_story_reconciles_added_and_removed_dependencies_atomically() {
        let config = tempfile::tempdir_in(crate::test_support::test_temp_root()).unwrap();
        let _guard = crate::config::set_config_dir_override(config.path().to_path_buf());
        let repo = tempfile::tempdir_in(crate::test_support::test_temp_root()).unwrap();
        let (first, second) = two_stories(repo.path().to_str().unwrap());
        let store = StoryStore::open().unwrap();
        let patch = |value| serde_json::from_value::<StoryPatch>(value).unwrap();
        let blocked = store
            .update_story(
                &second.id,
                second.revision,
                patch(serde_json::json!({
                    "dependencies": [first.id]
                })),
            )
            .unwrap();
        assert_eq!(blocked.status, StoryStatus::Backlog);
        let error = store
            .update_story(
                &first.id,
                first.revision,
                patch(serde_json::json!({
                    "title": "Not persisted", "dependencies": [second.id]
                })),
            )
            .unwrap_err();
        assert!(error.contains("cycle"), "{error}");
        assert_eq!(store.get_story(&first.id).unwrap(), first);
        let ready = store
            .update_story(
                &blocked.id,
                blocked.revision,
                patch(serde_json::json!({
                    "dependencies": []
                })),
            )
            .unwrap();
        assert_eq!(ready.status, StoryStatus::Ready);
        assert_eq!(ready.dependencies, Vec::<String>::new());
    }

    /// Catches: amendments bypassing claim/lifecycle ownership and mutating terminal records.
    #[test]
    fn update_story_refuses_claimed_active_and_terminal_stories() {
        let config = tempfile::tempdir_in(crate::test_support::test_temp_root()).unwrap();
        let _guard = crate::config::set_config_dir_override(config.path().to_path_buf());
        let repo = tempfile::tempdir_in(crate::test_support::test_temp_root()).unwrap();
        let project = repo.path().to_str().unwrap();
        let store = StoryStore::open().unwrap();
        let (claimed, _) = two_stories(project);
        let claimed = store
            .claim(&claimed.id, "session", claimed.revision)
            .unwrap();
        let (mut manual, _) = two_stories(project);
        assert!(
            store
                .update_story(&claimed.id, claimed.revision, StoryPatch::default())
                .unwrap_err()
                .contains("claimed")
        );
        assert_eq!(store.get_story(&claimed.id).unwrap(), claimed);

        for command in [
            StoryCommand::StartManual,
            StoryCommand::CheckCriterion(0),
            StoryCommand::SubmitReview,
            StoryCommand::Approve,
        ] {
            manual = store
                .transition(&manual.id, manual.revision, command)
                .unwrap();
            let error = store
                .update_story(&manual.id, manual.revision, StoryPatch::default())
                .unwrap_err();
            assert!(error.contains("only backlog or ready"), "{error}");
            assert_eq!(store.get_story(&manual.id).unwrap(), manual);
        }
        let (cancelled, _) = two_stories(project);
        let cancelled = store
            .transition(&cancelled.id, cancelled.revision, StoryCommand::WontFix)
            .unwrap();
        assert!(
            store
                .update_story(&cancelled.id, cancelled.revision, StoryPatch::default())
                .is_err()
        );
    }

    /// Catches: the generic amendment dropping story type, clearing it on omission, or hiding it from filters.
    #[test]
    fn update_story_type_is_nullable_and_updates_the_shared_filter_projection() {
        use super::super::{StoryFilter, StoryType};
        let config = tempfile::tempdir_in(crate::test_support::test_temp_root()).unwrap();
        let _guard = crate::config::set_config_dir_override(config.path().to_path_buf());
        let repo = tempfile::tempdir_in(crate::test_support::test_temp_root()).unwrap();
        let project = repo.path().to_str().unwrap();
        let (story, _) = two_stories(project);
        let update = |revision, patch| {
            story_of(story_action(project, serde_json::from_value(serde_json::json!({
            "action": "update_story", "story_id": story.id, "expected_revision": revision, "patch": patch
        })).unwrap(), None).unwrap())
        };
        let typed = update(story.revision, serde_json::json!({"storyType": "fix"}));
        assert_eq!(typed.story_type, Some(StoryType::Fix));
        let store = StoryStore::open().unwrap();
        let filtered = store
            .filtered_plan_view(
                &story.plan_id,
                &StoryFilter {
                    story_type: Some(StoryType::Fix),
                    ..Default::default()
                },
            )
            .unwrap();
        assert_eq!(filtered.visible_story_ids, vec![story.id.clone()]);
        let unchanged = update(typed.revision, serde_json::json!({"priority": 3}));
        assert_eq!(unchanged.story_type, Some(StoryType::Fix));
        let untyped = update(unchanged.revision, serde_json::json!({"storyType": null}));
        assert_eq!(untyped.story_type, None);
        let filtered = store
            .filtered_plan_view(
                &story.plan_id,
                &StoryFilter {
                    story_type: Some(StoryType::Fix),
                    ..Default::default()
                },
            )
            .unwrap();
        assert_eq!(filtered.visible_story_ids, Vec::<String>::new());
    }

    /// Catches: old documents failing to decode or omitted optional fields clearing metadata.
    #[test]
    fn update_story_optional_metadata_defaults_and_null_clearing() {
        let config = tempfile::tempdir_in(crate::test_support::test_temp_root()).unwrap();
        let _guard = crate::config::set_config_dir_override(config.path().to_path_buf());
        let repo = tempfile::tempdir_in(crate::test_support::test_temp_root()).unwrap();
        let (story, _) = two_stories(repo.path().to_str().unwrap());
        let mut legacy = serde_json::to_value(&story).unwrap();
        legacy.as_object_mut().unwrap().remove("problem");
        legacy.as_object_mut().unwrap().remove("risk");
        assert_eq!(serde_json::from_value::<Story>(legacy).unwrap(), story);
        let store = StoryStore::open().unwrap();
        let patch = |value| serde_json::from_value::<StoryPatch>(value).unwrap();
        let metadata = store
            .update_story(
                &story.id,
                story.revision,
                patch(serde_json::json!({
                    "title": "Amended", "fileScope": ["src/service.rs"],
                    "origin": {"type": "plan_step", "step": "Step 2"},
                    "problem": "Missing amendment", "risk": "high"
                })),
            )
            .unwrap();
        assert_eq!(metadata.title, "Amended");
        assert_eq!(metadata.file_scope, vec!["src/service.rs"]);
        assert_eq!(
            metadata.origin,
            StoryOrigin::PlanStep {
                step: "Step 2".into()
            }
        );
        assert_eq!(metadata.problem.as_deref(), Some("Missing amendment"));
        assert_eq!(metadata.risk, Some(super::super::StoryRisk::High));
        let unchanged = store
            .update_story(
                &metadata.id,
                metadata.revision,
                patch(serde_json::json!({"priority": 2})),
            )
            .unwrap();
        assert_eq!(unchanged.problem, metadata.problem);
        assert_eq!(unchanged.risk, metadata.risk);
        let cleared = store
            .update_story(
                &unchanged.id,
                unchanged.revision,
                patch(serde_json::json!({"problem": null, "risk": null})),
            )
            .unwrap();
        assert_eq!(cleared.problem, None);
        assert_eq!(cleared.risk, None);
    }

    /// Catches: counting stories of finished plans, counting won't-fix stories in the total, and
    /// counting stories abandoned behind a cancelled dependency, and leaking another project's plans into the summary.
    #[test]
    fn summary_counts_only_active_plans_of_the_project() {
        let config = tempfile::tempdir().expect("config");
        let _guard = crate::config::set_config_dir_override(config.path().to_path_buf());
        let repo = tempfile::tempdir().expect("project");
        let other = tempfile::tempdir().expect("other project");
        let project = repo.path().to_str().expect("project path");
        let other_path = other.path().to_str().expect("other path");
        let transition = |project: &str, story: &Story, command: StoryCommand| {
            story_of(
                story_action(
                    project,
                    StoryAction::Transition {
                        story_id: story.id.clone(),
                        expected_revision: story.revision,
                        command,
                    },
                    None,
                )
                .expect("transition"),
            )
        };

        // Active plan: one in progress, one won't fix, one ready.
        let (first, second) = two_stories(project);
        let third = {
            let StoryReply::Plans(plans) =
                story_action(project, StoryAction::ListPlans { archived: false }, None).unwrap()
            else {
                panic!("expected plans");
            };
            story_of(
                story_action(
                    project,
                    StoryAction::CreateStory {
                        input: NewStory {
                            story_type: None,
                            plan_id: plans[0].id.clone(),
                            title: "Third".into(),
                            criteria: vec!["Done".into()],
                            priority: 1,
                            origin: StoryOrigin::Native,
                            file_scope: vec![],
                        },
                    },
                    None,
                )
                .unwrap(),
            )
        };
        transition(project, &first, StoryCommand::StartManual);
        // `third` depends on `second`, which is then cancelled: `third` is abandoned.
        let third = story_of(
            story_action(
                project,
                StoryAction::AddDependency {
                    story_id: third.id.clone(),
                    dependency_id: second.id.clone(),
                    expected_revision: third.revision,
                },
                None,
            )
            .unwrap(),
        );
        assert_eq!(third.status, StoryStatus::Backlog);
        transition(project, &second, StoryCommand::WontFix);

        // Finished plan: its done story must not count.
        let StoryReply::Plan(done_plan) = story_action(
            project,
            StoryAction::CreatePlan {
                title: "Old".into(),
                source: "plans/old.md".into(),
            },
            None,
        )
        .unwrap() else {
            panic!("expected plan");
        };
        let old = story_of(
            story_action(
                project,
                StoryAction::CreateStory {
                    input: NewStory {
                        story_type: None,
                        plan_id: done_plan.id,
                        title: "Old".into(),
                        criteria: vec!["Done".into()],
                        priority: 1,
                        origin: StoryOrigin::Native,
                        file_scope: vec![],
                    },
                },
                None,
            )
            .unwrap(),
        );
        let old = transition(project, &old, StoryCommand::StartManual);
        let old = transition(project, &old, StoryCommand::CheckCriterion(0));
        let old = transition(project, &old, StoryCommand::SubmitReview);
        assert_eq!(
            transition(project, &old, StoryCommand::Approve).status,
            StoryStatus::Done
        );

        // Another project with its own active plan.
        two_stories(other_path);

        let StoryReply::Summary(summary) =
            story_action(project, StoryAction::Summary, None).unwrap()
        else {
            panic!("expected summary");
        };
        assert_eq!(
            summary,
            StorySummary {
                active_plans: 1,
                in_progress: 1,
                qa: 0,
                done: 0,
                total: 1
            }
        );
        let StoryReply::Summary(empty) = story_action(
            tempfile::tempdir().unwrap().path().to_str().unwrap(),
            StoryAction::Summary,
            None,
        )
        .unwrap() else {
            panic!("expected summary");
        };
        assert_eq!(empty, StorySummary::default());
    }

    /// Catches: add_dependency silently moving a ready story to backlog, and a claim refusal
    /// ("story is not ready") that leaves the agent to guess which dependency blocks it.
    #[test]
    fn claim_refusal_names_the_unmet_dependency_after_add_dependency_demotes_the_story() {
        let config = tempfile::tempdir().expect("config");
        let _guard = crate::config::set_config_dir_override(config.path().to_path_buf());
        let repo = tempfile::tempdir().expect("project");
        let project = repo.path().to_str().expect("project path");
        let (first, second) = two_stories(project);
        assert_eq!(second.status, super::super::StoryStatus::Ready);

        let demoted = story_of(
            story_action(
                project,
                StoryAction::AddDependency {
                    story_id: second.id.clone(),
                    dependency_id: first.id.clone(),
                    expected_revision: second.revision,
                },
                Some("pty-1"),
            )
            .expect("add dependency"),
        );
        assert_eq!(demoted.status, super::super::StoryStatus::Backlog);

        let error = story_action(
            project,
            StoryAction::Claim {
                story_id: second.id,
                expected_revision: demoted.revision,
            },
            Some("pty-1"),
        )
        .expect_err("claim of a backlog story is refused");
        assert!(error.contains(&first.id), "{error}");
        assert!(error.contains("backlog"), "{error}");
    }

    /// Catches: managed callers being blocked from administrative actions or misattributed as Human.
    #[test]
    fn managed_administrative_actions_record_the_caller() {
        let config = tempfile::tempdir_in(crate::test_support::test_temp_root()).expect("config");
        let _guard = crate::config::set_config_dir_override(config.path().to_path_buf());
        let repo = tempfile::tempdir_in(crate::test_support::test_temp_root()).expect("project");
        let project = repo.path().to_str().expect("project path");
        let (mut first, _) = two_stories(project);
        for command in [
            StoryCommand::Block,
            StoryCommand::Unblock,
            StoryCommand::StartManual,
            StoryCommand::CheckCriterion(0),
            StoryCommand::SubmitReview,
            StoryCommand::RejectReview,
            StoryCommand::WontFix,
        ] {
            first = story_of(
                story_action(
                    project,
                    StoryAction::Transition {
                        story_id: first.id.clone(),
                        expected_revision: first.revision,
                        command,
                    },
                    Some("pty-1"),
                )
                .expect("managed action"),
            );
        }
        assert_eq!(first.status, super::super::StoryStatus::WontFix);
        let store = StoryStore::open().expect("store");
        assert!(
            store
                .transition_history(&first.id)
                .expect("history")
                .iter()
                .all(|entry| entry.actor
                    == super::super::StoryTransitionActor::ManagedSession {
                        session_id: "pty-1".into()
                    })
        );
    }

    /// Catches: a schema that stays `{"type":"object"}` (fields learned one error at a time),
    /// and root-relative `$ref`s that no longer resolve once the schema is nested under `input`.
    #[test]
    fn published_schema_describes_create_story_without_references() {
        let generator = schemars::generate::SchemaSettings::draft2020_12()
            .with(|settings| settings.inline_subschemas = true)
            .into_generator();
        let schema = serde_json::to_value(generator.into_root_schema_for::<StoryAction>())
            .expect("story action schema");
        let text = schema.to_string();
        assert!(
            !text.contains("$ref"),
            "sub-schemas must be inlined: {text}"
        );
        let create = schema["oneOf"]
            .as_array()
            .expect("one schema per action")
            .iter()
            .find(|variant| variant["properties"]["action"]["const"] == "create_story")
            .expect("create_story is published");
        let update = schema["oneOf"]
            .as_array()
            .unwrap()
            .iter()
            .find(|variant| variant["properties"]["action"]["const"] == "update_story")
            .expect("update_story is published");
        let patch = &update["properties"]["patch"];
        assert_eq!(patch["additionalProperties"], false);
        for field in [
            "title",
            "priority",
            "fileScope",
            "storyType",
            "origin",
            "problem",
            "risk",
            "criteria",
            "dependencies",
        ] {
            assert!(patch["properties"][field].is_object(), "{field}: {patch}");
        }
        let input = &create["properties"]["input"];
        assert_eq!(input["properties"]["priority"]["minimum"], 1);
        assert_eq!(input["properties"]["priority"]["maximum"], 3);
        assert!(input["properties"]["planId"].is_object(), "{input}");
        assert!(input["properties"]["fileScope"].is_object(), "{input}");
        assert!(
            input["properties"]["origin"]
                .to_string()
                .contains("plan_step"),
            "the origin tag shape must be published: {input}"
        );
    }
    /// Catches: treating authenticated or managed transitions as anonymous LocalApi actions.
    #[test]
    fn transition_source_preserves_actor_in_persisted_history() {
        use super::super::StoryTransitionActor;
        let config = tempfile::tempdir_in(crate::test_support::test_temp_root()).unwrap();
        let _guard = crate::config::set_config_dir_override(config.path().to_path_buf());
        let repo = tempfile::tempdir_in(crate::test_support::test_temp_root()).unwrap();
        let project = repo.path().to_str().unwrap();
        for (anonymous, session, expected) in [
            (false, None, StoryTransitionActor::Human),
            (true, None, StoryTransitionActor::LocalApi),
            (
                false,
                Some("managed"),
                StoryTransitionActor::ManagedSession {
                    session_id: "managed".into(),
                },
            ),
            (
                true,
                Some("managed"),
                StoryTransitionActor::ManagedSession {
                    session_id: "managed".into(),
                },
            ),
        ] {
            let (story, _) = two_stories(project);
            story_action_with_source(
                project,
                StoryAction::Transition {
                    story_id: story.id.clone(),
                    expected_revision: story.revision,
                    command: StoryCommand::Block,
                },
                session,
                anonymous,
            )
            .unwrap();
            let history = StoryStore::open()
                .unwrap()
                .transition_history(&story.id)
                .unwrap();
            assert_eq!(history.last().unwrap().actor, expected);
        }
    }
    /// Catches: the shared MCP/HTTP/CLI boundary dropping manual commands or hiding QA in summaries.
    #[test]
    fn qa_actions_roundtrip_and_summary_separates_manual_work() {
        let config = tempfile::tempdir_in(crate::test_support::test_temp_root()).unwrap();
        let _guard = crate::config::set_config_dir_override(config.path().into());
        let project = tempfile::tempdir_in(crate::test_support::test_temp_root()).unwrap();
        let project = project.path().to_str().unwrap();
        let StoryReply::Plan(plan) = story_action(
            project,
            StoryAction::CreatePlan {
                title: "QA".into(),
                source: "plan.md".into(),
            },
            None,
        )
        .unwrap() else {
            panic!("plan")
        };
        let StoryReply::Story(mut story) = story_action(
            project,
            StoryAction::CreateStory {
                input: NewStory {
                    story_type: None,
                    plan_id: plan.id.clone(),
                    title: "Manual work".into(),
                    criteria: vec!["Development".into(), "Device".into()],
                    priority: 1,
                    origin: super::super::StoryOrigin::Native,
                    file_scope: vec![],
                },
            },
            None,
        )
        .unwrap() else {
            panic!("story")
        };
        for command in [
            serde_json::json!({"set_criterion_manual":{"index":1,"manual":true}}),
            serde_json::json!("start_manual"),
            serde_json::json!({"check_criterion":0}),
        ] {
            let action: StoryAction = serde_json::from_value(serde_json::json!({
                "action":"transition", "story_id":story.id,
                "expected_revision":story.revision, "command":command
            }))
            .unwrap();
            let StoryReply::Story(updated) =
                story_action(project, action, Some("managed")).unwrap()
            else {
                panic!("story")
            };
            story = updated;
        }
        let list: StoryAction = serde_json::from_value(serde_json::json!({
            "action": "list_stories", "plan_id": plan.id, "state": "qa", "limit": 1
        }))
        .unwrap();
        let wire = serde_json::to_value(story_action(project, list, None).unwrap()).unwrap();
        assert_eq!(wire["value"][0]["status"], "qa");
        let wire = serde_json::to_value(story_action(project, StoryAction::Summary, None).unwrap())
            .unwrap();
        assert_eq!(wire["value"]["qa"], 1);
        assert_eq!(wire["value"]["inProgress"], 0);
        assert_eq!(wire["value"]["done"], 0);
        assert_eq!(wire["value"]["activePlans"], 1);
    }

    // Catches misattributing anonymous HTTP notes as Human or overriding a managed author.
    #[test]
    fn story_note_source_preserves_transport_actor_provenance() {
        use super::super::StoryTransitionActor;
        let config = tempfile::tempdir_in(tuic_test_support::test_temp_root()).unwrap();
        let _guard = crate::config::set_config_dir_override(config.path().to_path_buf());
        let repo = tempfile::tempdir_in(tuic_test_support::test_temp_root()).unwrap();
        let project = repo.path().to_str().unwrap();
        let (story, _) = two_stories(project);
        for (anonymous, session, expected) in [
            (false, None, StoryTransitionActor::Human),
            (true, None, StoryTransitionActor::LocalApi),
            (
                false,
                Some("managed"),
                StoryTransitionActor::ManagedSession {
                    session_id: "managed".into(),
                },
            ),
            (
                true,
                Some("managed"),
                StoryTransitionActor::ManagedSession {
                    session_id: "managed".into(),
                },
            ),
        ] {
            let StoryReply::Note(note) = story_action_with_source(
                project,
                StoryAction::AddNote {
                    story_id: story.id.clone(),
                    expected_revision: story.revision,
                    text: "Observed".into(),
                    receipts: vec![],
                },
                session,
                anonymous,
            )
            .unwrap() else {
                panic!("note reply")
            };
            assert_eq!(note.actor, expected);
            assert_eq!(
                StoryStore::open()
                    .unwrap()
                    .list_notes(&story.id)
                    .unwrap()
                    .last()
                    .unwrap(),
                &note
            );
        }
    }
}
