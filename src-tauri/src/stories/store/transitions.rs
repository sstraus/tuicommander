use super::*;
use std::collections::HashSet;

fn is_manual(criterion: &str) -> bool {
    criterion.trim_start().starts_with("[MANUAL]")
}

pub(super) fn only_manual_pending(story: &Story) -> bool {
    story.checked.iter().any(|checked| !checked)
        && story
            .criteria
            .iter()
            .zip(&story.checked)
            .all(|(criterion, checked)| *checked || is_manual(criterion))
}

fn incomplete_criteria(story: &Story) -> String {
    let open: Vec<String> = story
        .checked
        .iter()
        .enumerate()
        .filter(|(_, checked)| !**checked)
        .map(|(index, _)| index.to_string())
        .collect();
    format!("{CRITERIA_INCOMPLETE}{}", open.join(", "))
}

fn wrong_status(story: &Story, required: &str) -> String {
    format!(
        "story must be {required} for this command (status: {})",
        story.status.as_str()
    )
}

/// Shared evaluator for preview and commit. It changes only the supplied snapshot.
fn apply_command(
    conn: &Connection,
    story: &mut Story,
    command: StoryCommand,
    preflight: &DependencyPreflight,
) -> Result<(), String> {
    match command {
        StoryCommand::StartManual => {
            if story.status != StoryStatus::Ready {
                return Err(wrong_status(story, "ready"));
            }
            if !dependencies_integrated(conn, story, preflight)? {
                return Err("story dependency lacks a current integration receipt".into());
            }
            story.status = StoryStatus::InProgress;
        }
        StoryCommand::CheckCriterion(index) | StoryCommand::UncheckCriterion(index) => {
            if !matches!(story.status, StoryStatus::InProgress | StoryStatus::Qa) {
                return Err(wrong_status(story, "in_progress or qa"));
            }
            let checked = story.checked.get_mut(index).ok_or_else(|| {
                format!(
                    "criterion index {index} out of range: story has {} criteria",
                    story.criteria.len()
                )
            })?;
            *checked = matches!(command, StoryCommand::CheckCriterion(_));
        }
        StoryCommand::SetCriterionManual { index, manual } => {
            if matches!(story.status, StoryStatus::Done | StoryStatus::WontFix) {
                return Err(wrong_status(story, "unfinished"));
            }
            let criterion = story
                .criteria
                .get_mut(index)
                .ok_or_else(|| format!("criterion index {index} out of range"))?;
            if manual && !is_manual(criterion) {
                let marked = format!("[MANUAL] {}", criterion.trim_start());
                validate_text("criterion", &marked, 2000)?;
                *criterion = marked;
            } else if !manual && is_manual(criterion) {
                *criterion = criterion
                    .trim_start()
                    .strip_prefix("[MANUAL]")
                    .expect("manual prefix")
                    .trim_start()
                    .into();
                validate_text("criterion", criterion, 2000)?;
            }
        }
        StoryCommand::SubmitQa => {
            if !matches!(story.status, StoryStatus::InProgress | StoryStatus::Review) {
                return Err(wrong_status(story, "in_progress or review"));
            }
            if !story.criteria.iter().any(|criterion| is_manual(criterion))
                || story
                    .criteria
                    .iter()
                    .zip(&story.checked)
                    .any(|(criterion, checked)| !checked && !is_manual(criterion))
            {
                return Err("QA requires manual criteria and no open non-manual criteria".into());
            }
            story.status = StoryStatus::Qa;
        }
        StoryCommand::RejectQa => {
            if story.status != StoryStatus::Qa {
                return Err(wrong_status(story, "qa"));
            }
            story.status = StoryStatus::InProgress;
        }
        StoryCommand::SubmitReview => {
            if story.status != StoryStatus::InProgress {
                return Err(wrong_status(story, "in_progress"));
            }
            if !story.checked.iter().all(|checked| *checked) && !only_manual_pending(story) {
                return Err(incomplete_criteria(story));
            }
            story.status = StoryStatus::Review;
        }
        StoryCommand::Approve => {
            if !matches!(story.status, StoryStatus::Review | StoryStatus::Qa) {
                return Err(wrong_status(story, "review or qa"));
            }
            if !story.checked.iter().all(|checked| *checked) {
                return Err(incomplete_criteria(story));
            }
            story.status = StoryStatus::Done;
            story.claim_session = None;
        }
        StoryCommand::RejectReview => {
            if story.status != StoryStatus::Review {
                return Err(wrong_status(story, "review"));
            }
            story.status = if story.claim_session.is_some() {
                StoryStatus::InProgress
            } else {
                StoryStatus::Ready
            };
        }
        StoryCommand::Block => {
            if !matches!(
                story.status,
                StoryStatus::Ready
                    | StoryStatus::InProgress
                    | StoryStatus::Review
                    | StoryStatus::Qa
            ) {
                return Err(format!(
                    "story can be blocked only from ready, in_progress, review or qa (status: {})",
                    story.status.as_str()
                ));
            }
            story.status = StoryStatus::Blocked;
            story.claim_session = None;
        }
        StoryCommand::Unblock => {
            if story.status != StoryStatus::Blocked {
                return Err(wrong_status(story, "blocked"));
            }
            story.status = if dependencies_integrated(conn, story, preflight)? {
                StoryStatus::Ready
            } else {
                StoryStatus::Backlog
            };
        }
        StoryCommand::WontFix => {
            if story.status == StoryStatus::Done {
                return Err("completed story cannot be discarded".into());
            }
            if story.status == StoryStatus::WontFix {
                return Err("story is already cancelled".into());
            }
            story.status = StoryStatus::WontFix;
            story.claim_session = None;
        }
    }
    if matches!(
        command,
        StoryCommand::StartManual
            | StoryCommand::CheckCriterion(_)
            | StoryCommand::UncheckCriterion(_)
            | StoryCommand::SetCriterionManual { .. }
            | StoryCommand::SubmitReview
    ) {
        if matches!(story.status, StoryStatus::InProgress | StoryStatus::Review)
            && only_manual_pending(story)
        {
            story.status = StoryStatus::Qa;
        } else if story.status == StoryStatus::Qa
            && story
                .criteria
                .iter()
                .zip(&story.checked)
                .any(|(criterion, checked)| !checked && !is_manual(criterion))
        {
            story.status = StoryStatus::InProgress;
        }
    }
    if story.status == StoryStatus::Qa {
        story.claim_session = None;
    }
    Ok(())
}

impl StoryStore {
    fn check_transition_actor(
        &self,
        story_id: &str,
        command: &StoryCommand,
        actor: &StoryTransitionActor,
    ) -> Result<(), String> {
        let run_path = self.db_path.with_file_name("workflow_runs.sqlite3");
        if *command == StoryCommand::Approve
            && run_path.exists()
            && let StoryTransitionActor::ManagedSession { session_id } = actor
            && crate::workflows::RunStore::open_at(&run_path)?
                .is_story_implementer(story_id, session_id)?
        {
            return Err("a story cannot be approved by its workflow implementer".into());
        }
        Ok(())
    }

    /// Preview actual destinations without writes, using the commit evaluator.
    pub fn available_transitions(
        &self,
        story_id: &str,
        expected_revision: i64,
        actor: StoryTransitionActor,
    ) -> Result<super::super::AvailableTransitions, String> {
        let plan = self.require_unarchived_story(story_id)?.plan_id;
        let preflight = DependencyPreflight::prepare(self, &plan)?;
        let project = self.get_plan(&plan)?.project;
        let can_approve = self
            .check_transition_actor(story_id, &StoryCommand::Approve, &actor)
            .is_ok();
        // Keep an accepted reservation guard alive through the snapshot read.
        let start_guard = crate::workflows::guard_manual_story_start(&project, story_id);
        let can_start = start_guard.is_ok();
        let mut conn = self.connect()?;
        let tx = conn
            .transaction()
            .map_err(|error| format!("read transition snapshot: {error}"))?;
        preflight.validate(&tx)?;
        let story = read_story(&tx, story_id)?;
        check_revision(&story, expected_revision)?;
        require_active_story(&tx, &story)?;
        let mut moves = Vec::new();
        for command in [
            StoryCommand::StartManual,
            StoryCommand::SubmitReview,
            StoryCommand::SubmitQa,
            StoryCommand::Approve,
            StoryCommand::RejectReview,
            StoryCommand::RejectQa,
            StoryCommand::Block,
            StoryCommand::Unblock,
            StoryCommand::WontFix,
        ] {
            if (command == StoryCommand::StartManual && !can_start)
                || (command == StoryCommand::Approve && !can_approve)
            {
                continue;
            }
            let mut candidate = story.clone();
            if apply_command(&tx, &mut candidate, command.clone(), &preflight).is_ok()
                && candidate.status != story.status
                && !moves
                    .iter()
                    .any(|item: &super::super::StoryPhaseMove| item.status == candidate.status)
            {
                moves.push(super::super::StoryPhaseMove {
                    status: candidate.status,
                    command,
                });
            }
        }
        Ok(super::super::AvailableTransitions {
            revision: story.revision,
            moves,
        })
    }

    /// Rebuild dependent readiness from durable integration receipts. Safe to
    /// repeat after a crash between the run event and the story projection.
    pub fn reconcile_integrated_dependencies(&self, plan_id: &str) -> Result<(), String> {
        let preflight_plan = plan_id.to_owned();
        let preflight = DependencyPreflight::prepare(self, &preflight_plan)?;
        let mut conn = self.connect()?;
        let tx = immediate(&mut conn)?;
        preflight.validate(&tx)?;
        reconcile_ready(&tx, plan_id, &preflight)?;
        tx.commit()
            .map_err(|error| format!("commit dependency release: {error}"))
    }

    pub fn transition_history(&self, story_id: &str) -> Result<Vec<StoryTransition>, String> {
        let conn = self.connect()?;
        read_story(&conn, story_id)?;
        let mut stmt = conn.prepare(
            "SELECT revision,command_json,actor_json FROM story_transitions WHERE story_id=?1 ORDER BY revision"
        ).map_err(|e| format!("prepare story transition history: {e}"))?;
        let rows = stmt
            .query_map([story_id], |row| {
                Ok((
                    row.get::<_, i64>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, String>(2)?,
                ))
            })
            .map_err(|e| format!("read story transition history: {e}"))?;
        rows.map(|row| {
            let (revision, command_json, actor_json) =
                row.map_err(|e| format!("read story transition: {e}"))?;
            Ok(StoryTransition {
                story_id: story_id.into(),
                revision,
                command: serde_json::from_str(&command_json)
                    .map_err(|e| format!("decode story command: {e}"))?,
                actor: serde_json::from_str(&actor_json)
                    .map_err(|e| format!("decode story actor: {e}"))?,
            })
        })
        .collect()
    }

    pub fn add_dependency(
        &self,
        story_id: &str,
        dependency_id: &str,
        expected_revision: i64,
    ) -> Result<Story, String> {
        let preflight_plan = self.require_unarchived_story(story_id)?.plan_id;
        let preflight = DependencyPreflight::prepare(self, &preflight_plan)?;
        let mut conn = self.connect()?;
        let tx = immediate(&mut conn)?;
        preflight.validate(&tx)?;
        let mut story = read_story(&tx, story_id)?;
        let dependency = read_story(&tx, dependency_id)?;
        check_revision(&story, expected_revision)?;
        require_active_story(&tx, &story)?;
        if story.plan_id != dependency.plan_id {
            return Err("dependencies must belong to one plan".into());
        }
        if !matches!(story.status, StoryStatus::Backlog | StoryStatus::Ready) {
            return Err(format!(
                "dependencies can be edited only on backlog or ready stories (status: {})",
                story.status.as_str()
            ));
        }
        require_actionable_dependency(&dependency)?;
        if story.dependencies.iter().any(|id| id == dependency_id) {
            return Err(format!(
                "dependency already exists: {dependency_id} is already a dependency of {story_id}"
            ));
        }
        let mut seen = HashSet::new();
        let mut stack = vec![dependency_id.to_string()];
        while let Some(id) = stack.pop() {
            if id == story_id {
                return Err(format!(
                    "dependency cycle: {dependency_id} already depends on {story_id}"
                ));
            }
            if seen.insert(id.clone()) {
                stack.extend(read_story(&tx, &id)?.dependencies);
            }
        }
        story.dependencies.push(dependency_id.into());
        if !dependencies_integrated(&tx, &story, &preflight)? {
            // A ready story with an unfinished or unintegrated dependency is no longer claimable.
            story.status = StoryStatus::Backlog;
        }
        save_story(&tx, &mut story, expected_revision)?;
        tx.commit().map_err(|e| format!("commit dependency: {e}"))?;
        Ok(story)
    }

    pub fn remove_dependency(
        &self,
        story_id: &str,
        dependency_id: &str,
        expected_revision: i64,
        _actor_session: Option<&str>,
    ) -> Result<Story, String> {
        let preflight_plan = self.require_unarchived_story(story_id)?.plan_id;
        let preflight = DependencyPreflight::prepare(self, &preflight_plan)?;
        let mut conn = self.connect()?;
        let tx = immediate(&mut conn)?;
        preflight.validate(&tx)?;
        let mut story = read_story(&tx, story_id)?;
        check_revision(&story, expected_revision)?;
        require_active_story(&tx, &story)?;
        if story.status != StoryStatus::Backlog {
            return Err(format!(
                "dependencies can be removed only from backlog stories (status: {})",
                story.status.as_str()
            ));
        }
        if !story.dependencies.iter().any(|id| id == dependency_id) {
            return Err(format!(
                "dependency does not exist on story: {dependency_id} is not listed in its dependencies"
            ));
        }
        let dependency = read_story(&tx, dependency_id)?;
        if dependency.plan_id != story.plan_id || dependency.status != StoryStatus::WontFix {
            return Err(format!(
                "only cancelled (wont_fix) dependencies in the same plan can be removed; {dependency_id} is {}",
                if dependency.plan_id != story.plan_id {
                    "in another plan"
                } else {
                    dependency.status.as_str()
                }
            ));
        }
        story.dependencies.retain(|id| id != dependency_id);
        if dependencies_integrated(&tx, &story, &preflight)? {
            story.status = StoryStatus::Ready;
        }
        save_story(&tx, &mut story, expected_revision)?;
        tx.commit()
            .map_err(|e| format!("commit dependency removal: {e}"))?;
        Ok(story)
    }

    #[cfg(test)]
    pub fn transition(
        &self,
        story_id: &str,
        expected_revision: i64,
        command: StoryCommand,
    ) -> Result<Story, String> {
        self.transition_for_actor(story_id, expected_revision, command, None)
    }

    pub fn transition_for_actor(
        &self,
        story_id: &str,
        expected_revision: i64,
        command: StoryCommand,
        actor_session: Option<&str>,
    ) -> Result<Story, String> {
        let actor = actor_session.map_or(StoryTransitionActor::Human, |session_id| {
            StoryTransitionActor::ManagedSession {
                session_id: session_id.into(),
            }
        });
        self.transition_as(story_id, expected_revision, command, actor, false)
    }

    pub(crate) fn transition_from_local_api(
        &self,
        story_id: &str,
        expected_revision: i64,
        command: StoryCommand,
    ) -> Result<Story, String> {
        self.transition_as(
            story_id,
            expected_revision,
            command,
            StoryTransitionActor::LocalApi,
            false,
        )
    }

    /// Only the owning workflow may begin its reserved story without a manual claim.
    pub(crate) fn begin_workflow_story(
        &self,
        story_id: &str,
        revision: i64,
        session: &str,
    ) -> Result<Story, String> {
        self.transition_as(
            story_id,
            revision,
            StoryCommand::StartManual,
            StoryTransitionActor::ManagedSession {
                session_id: session.into(),
            },
            true,
        )
    }

    fn transition_as(
        &self,
        story_id: &str,
        expected_revision: i64,
        command: StoryCommand,
        actor: StoryTransitionActor,
        workflow_start: bool,
    ) -> Result<Story, String> {
        self.check_transition_actor(story_id, &command, &actor)?;
        let preflight_plan = self.require_unarchived_story(story_id)?.plan_id;
        let preflight = DependencyPreflight::prepare(self, &preflight_plan)?;
        let _reservation_guard = if !workflow_start && matches!(command, StoryCommand::StartManual)
        {
            let project = self.get_plan(&preflight_plan)?.project;
            crate::workflows::guard_manual_story_start(&project, story_id)?
        } else {
            None
        };
        let mut conn = self.connect()?;
        let tx = immediate(&mut conn)?;
        preflight.validate(&tx)?;
        let mut story = read_story(&tx, story_id)?;
        check_revision(&story, expected_revision)?;
        require_active_story(&tx, &story)?;
        apply_command(&tx, &mut story, command.clone(), &preflight)?;
        save_story(&tx, &mut story, expected_revision)?;
        tx.execute(
            "INSERT INTO story_transitions(story_id,revision,command_json,actor_json) VALUES (?1,?2,?3,?4)",
            rusqlite::params![
                story.id, story.revision,
                serde_json::to_string(&command).map_err(|e| format!("encode story command: {e}"))?,
                serde_json::to_string(&actor)
                    .map_err(|e| format!("encode story actor: {e}"))?,
            ],
        ).map_err(|e| format!("record story transition: {e}"))?;
        if story.status == StoryStatus::Done {
            reconcile_ready(&tx, &story.plan_id, &preflight)?;
        }
        tx.commit()
            .map_err(|e| format!("commit story transition: {e}"))?;
        Ok(story)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn store_with_story(dir: &std::path::Path, criteria: usize) -> (StoryStore, Story) {
        let store = StoryStore::open_at(&dir.join("stories.sqlite3")).expect("store");
        let plan = store
            .create_plan(NewPlan {
                project: "/project".into(),
                title: "Transitions".into(),
                source: "plan.md".into(),
            })
            .expect("plan");
        let story = store
            .create_story(NewStory {
                story_type: None,
                plan_id: plan.id,
                title: "Story".into(),
                criteria: (0..criteria).map(|n| format!("Criterion {n}")).collect(),
                priority: 1,
                origin: StoryOrigin::Native,
                file_scope: vec![],
            })
            .expect("story");
        (store, story)
    }

    /// Catches: a phase preview writes a revision/history or offers an illegal target.
    #[test]
    fn phase_moves_preview_is_read_only_and_matches_committed_destinations() {
        let dir = tempfile::tempdir_in(crate::test_support::test_temp_root()).expect("dir");
        let _guard = crate::config::set_config_dir_override(dir.path().into());
        let (store, story) = store_with_story(dir.path(), 1);
        let choices = store
            .available_transitions(&story.id, story.revision, StoryTransitionActor::Human)
            .expect("preview");
        assert_eq!(choices.revision, story.revision);
        assert_eq!(
            choices
                .moves
                .iter()
                .map(|item| item.status)
                .collect::<Vec<_>>(),
            vec![
                StoryStatus::InProgress,
                StoryStatus::Blocked,
                StoryStatus::WontFix
            ]
        );
        assert_eq!(store.get_story(&story.id).unwrap(), story);
        assert_eq!(
            store.transition_history(&story.id).unwrap(),
            Vec::<StoryTransition>::new()
        );
        let moved = store
            .transition(
                &story.id,
                choices.revision,
                choices.moves[0].command.clone(),
            )
            .unwrap();
        assert_eq!(moved.status, choices.moves[0].status);
        assert_eq!(moved.revision, story.revision + 1);
        assert!(
            store
                .available_transitions(&story.id, story.revision, StoryTransitionActor::Human)
                .unwrap_err()
                .contains("stale")
        );
        assert_eq!(store.get_story(&story.id).unwrap(), moved);
    }

    /// Catches: a status-only UI mapping offers development when manual-only work actually enters QA.
    #[test]
    fn phase_moves_preview_uses_automatic_qa_and_checked_criteria() {
        let dir = tempfile::tempdir_in(crate::test_support::test_temp_root()).expect("dir");
        let _guard = crate::config::set_config_dir_override(dir.path().into());
        let (store, story) = store_with_story(dir.path(), 1);
        let marked = store
            .transition(
                &story.id,
                story.revision,
                StoryCommand::SetCriterionManual {
                    index: 0,
                    manual: true,
                },
            )
            .unwrap();
        let choices = store
            .available_transitions(&story.id, marked.revision, StoryTransitionActor::Human)
            .unwrap();
        let start = choices
            .moves
            .iter()
            .find(|item| item.command == StoryCommand::StartManual)
            .unwrap();
        assert_eq!(start.status, StoryStatus::Qa);
        let qa = store
            .transition(&story.id, marked.revision, start.command.clone())
            .unwrap();
        let choices = store
            .available_transitions(&story.id, qa.revision, StoryTransitionActor::Human)
            .unwrap();
        assert!(
            !choices
                .moves
                .iter()
                .any(|item| item.status == StoryStatus::Done)
        );
        let checked = store
            .transition(&story.id, qa.revision, StoryCommand::CheckCriterion(0))
            .unwrap();
        let choices = store
            .available_transitions(&story.id, checked.revision, StoryTransitionActor::Human)
            .unwrap();
        assert!(
            choices
                .moves
                .iter()
                .any(|item| item.command == StoryCommand::Approve
                    && item.status == StoryStatus::Done)
        );
    }

    /// Catches: archived records still advertising executable phase moves.
    #[test]
    fn phase_moves_preview_refuses_archived_stories() {
        let dir = tempfile::tempdir_in(crate::test_support::test_temp_root()).expect("dir");
        let _guard = crate::config::set_config_dir_override(dir.path().into());
        let (store, story) = store_with_story(dir.path(), 1);
        let archived = store
            .set_story_archived(&story.id, story.revision, true, StoryTransitionActor::Human)
            .unwrap();
        assert!(
            store
                .available_transitions(&story.id, archived.revision, StoryTransitionActor::Human)
                .unwrap_err()
                .contains("archived")
        );
        assert_eq!(store.get_story(&story.id).unwrap(), archived);
    }

    /// Catches: the last automated check leaving manual-only work in progress.
    #[test]
    fn last_non_manual_criterion_moves_story_to_qa() {
        let dir = tempfile::tempdir_in(crate::test_support::test_temp_root()).expect("dir");
        let (store, story) = store_with_story(dir.path(), 1);
        let plan_id = story.plan_id;
        let story = store
            .create_story(NewStory {
                story_type: None,
                plan_id,
                title: "Manual verification".into(),
                criteria: vec![
                    "Automated behavior".into(),
                    "[MANUAL] Check hardware".into(),
                ],
                priority: 1,
                origin: StoryOrigin::Native,
                file_scope: vec![],
            })
            .expect("story");
        let started = store
            .transition(&story.id, story.revision, StoryCommand::StartManual)
            .expect("start");
        let checked = store
            .transition(&story.id, started.revision, StoryCommand::CheckCriterion(0))
            .expect("check development");
        assert_eq!(serde_json::to_value(&checked).unwrap()["status"], "qa");
        assert_eq!(checked.checked, vec![true, false]);
        assert_eq!(store.get_story(&story.id).unwrap(), checked);
        assert_eq!(checked.revision, started.revision + 1);
    }

    /// Catches: wrong_status returning an empty or placeholder string (mutants
    /// `String::new()` and `"xyzzy"` at the format! call), which would leave an agent
    /// refused with no hint of the status it needs.
    #[test]
    fn wrong_status_names_required_and_current_status() {
        let dir = tempfile::tempdir().expect("dir");
        let (store, story) = store_with_story(dir.path(), 1);

        let err = store
            .transition(&story.id, story.revision, StoryCommand::Approve)
            .expect_err("approve needs review");
        assert_eq!(
            err,
            "story must be review or qa for this command (status: ready)"
        );
        let err = store
            .transition(&story.id, story.revision, StoryCommand::CheckCriterion(0))
            .expect_err("check needs in_progress");
        assert_eq!(
            err,
            "story must be in_progress or qa for this command (status: ready)"
        );
    }

    /// Catches: actor provenance being used to refuse another caller's valid criterion update.
    #[test]
    fn criterion_commands_use_status_and_revision_not_actor_authority() {
        let dir = tempfile::tempdir().expect("dir");
        let (store, story) = store_with_story(dir.path(), 1);
        let err = store
            .transition_for_actor(
                &story.id,
                story.revision,
                StoryCommand::CheckCriterion(0),
                Some("impl"),
            )
            .expect_err("not in progress");
        assert!(err.contains("in_progress"), "{err}");
        let claimed = store
            .claim(&story.id, "impl", story.revision)
            .expect("claim");
        let checked = store
            .transition_for_actor(
                &story.id,
                claimed.revision,
                StoryCommand::CheckCriterion(0),
                Some("other"),
            )
            .expect("caller checks");
        assert_eq!(checked.checked, vec![true]);
        let unchecked = store
            .transition_for_actor(
                &story.id,
                checked.revision,
                StoryCommand::UncheckCriterion(0),
                Some("impl"),
            )
            .expect("holder unchecks");
        assert_eq!(unchecked.checked, vec![false]);
        let checked = store
            .transition_for_actor(
                &story.id,
                unchecked.revision,
                StoryCommand::CheckCriterion(0),
                Some("impl"),
            )
            .expect("holder checks again");
        let review = store
            .transition_for_actor(
                &story.id,
                checked.revision,
                StoryCommand::SubmitReview,
                Some("impl"),
            )
            .expect("holder submits");
        assert_eq!(review.status, StoryStatus::Review);
    }

    /// Catches: deleting the `!` in the unchecked-criteria filter, which would report the
    /// checked indexes as the ones still open.
    #[test]
    fn pending_criteria_lists_only_unchecked_indices() {
        let dir = tempfile::tempdir().expect("dir");
        let (store, story) = store_with_story(dir.path(), 3);
        let started = store
            .transition(&story.id, story.revision, StoryCommand::StartManual)
            .expect("start");
        let s = store
            .transition(&story.id, started.revision, StoryCommand::CheckCriterion(0))
            .expect("check 0");
        let s = store
            .transition(&story.id, s.revision, StoryCommand::CheckCriterion(2))
            .expect("check 2");

        let err = store
            .transition(&story.id, s.revision, StoryCommand::SubmitReview)
            .expect_err("criterion 1 is open");
        assert_eq!(err, "criteria are incomplete: unchecked indexes 1");
    }

    /// Catches: `!=` flipped to `==` on the RejectReview (needs Review) and Unblock (needs
    /// Blocked) guards: either command would refuse its only valid status and run anywhere else.
    #[test]
    fn reject_review_outside_review_is_refused_and_unblock_outside_blocked_is_refused() {
        let dir = tempfile::tempdir().expect("dir");
        let (store, story) = store_with_story(dir.path(), 1);

        let err = store
            .transition(&story.id, story.revision, StoryCommand::RejectReview)
            .expect_err("ready story has no review to reject");
        assert_eq!(err, "story must be review for this command (status: ready)");
        let err = store
            .transition(&story.id, story.revision, StoryCommand::Unblock)
            .expect_err("ready story is not blocked");
        assert_eq!(
            err,
            "story must be blocked for this command (status: ready)"
        );

        let started = store
            .transition(&story.id, story.revision, StoryCommand::StartManual)
            .expect("start");
        let checked = store
            .transition(&story.id, started.revision, StoryCommand::CheckCriterion(0))
            .expect("check");
        let review = store
            .transition(&story.id, checked.revision, StoryCommand::SubmitReview)
            .expect("review");
        let rejected = store
            .transition(&story.id, review.revision, StoryCommand::RejectReview)
            .expect("review is rejectable");
        assert_eq!(rejected.status, StoryStatus::Ready);

        let blocked = store
            .transition(&story.id, rejected.revision, StoryCommand::Block)
            .expect("block");
        let unblocked = store
            .transition(&story.id, blocked.revision, StoryCommand::Unblock)
            .expect("blocked story unblocks");
        assert_eq!(unblocked.status, StoryStatus::Ready);
    }
}
