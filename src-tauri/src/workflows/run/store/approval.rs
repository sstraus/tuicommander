use super::*;

impl RunStore {
    pub(in crate::workflows::run) fn execute_plan_check(
        &self,
        run_id: &str,
        check: &CheckDefinition,
    ) -> Result<CheckReceipt, String> {
        let run = self.snapshot(run_id)?;
        if run.status != RunStatus::Running {
            return Err("plan is not running".into());
        }
        execute_run_check(check, Path::new(&run.project), &self.db_path, run_id, false)
    }

    pub(in crate::workflows::run) fn begin_graph_story(
        &self,
        run: &RunSnapshot,
        story_id: &str,
        session: &str,
    ) -> Result<StoryStatus, String> {
        let stories = StoryStore::open()?;
        let mut story = stories.get_story(story_id)?;
        if !matches!(story.status, StoryStatus::Ready | StoryStatus::Review) {
            return Ok(story.status);
        }
        let mut conn = self.connect()?;
        let tx = conn
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .map_err(|e| format!("begin workflow story: {e}"))?;
        if read_snapshot(&tx, &run.id)? != *run || run.status != RunStatus::Running {
            return Err("workflow moved before story start".into());
        }
        if story.status == StoryStatus::Review {
            story = stories.transition_for_actor(
                story_id,
                story.revision,
                crate::stories::StoryCommand::RejectReview,
                Some(session),
            )?;
        }
        let started = stories.begin_workflow_story(story_id, story.revision, session)?;
        tx.commit()
            .map_err(|e| format!("commit workflow story start: {e}"))?;
        Ok(started.status)
    }

    /// The writer fence prevents cancellation winning between approval preflight and
    /// the native transition. A lost response recovers from native actor history.
    pub(in crate::workflows::run) fn approve_graph_story(
        &self,
        run: &RunSnapshot,
        graph: &super::super::graph::GraphExecution,
        reviewer: &str,
    ) -> Result<(), String> {
        let stories = StoryStore::open()?;
        let story = stories.get_story(&graph.target_id)?;
        let mut conn = self.connect()?;
        let tx = conn
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .map_err(|e| format!("begin workflow approval: {e}"))?;
        if read_snapshot(&tx, &run.id)? != *run || run.status != RunStatus::Running {
            return Err("workflow moved before independent approval".into());
        }
        if story.status == StoryStatus::Review {
            stories.transition_for_actor(
                &story.id,
                story.revision,
                crate::stories::StoryCommand::Approve,
                Some(reviewer),
            )?;
        } else if story.status != StoryStatus::Done {
            return Err("independent approval requires review status".into());
        }
        tx.commit()
            .map_err(|e| format!("commit workflow approval fence: {e}"))?;
        self.command(
            &run.id,
            &format!("daemon:accept:{}:{}", graph.id, story.revision),
            RunCommand::AcceptStory { story_id: story.id },
        )?;
        Ok(())
    }
}
