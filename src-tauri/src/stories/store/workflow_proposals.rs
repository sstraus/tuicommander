//! The plan identity and its exact creation payload commit together.
use super::*;

impl StoryStore {
    pub fn create_plan_once(&self, run: &str, key: &str, input: NewPlan) -> Result<Plan, String> {
        validate_text("workflow run id", run, 128)?;
        validate_text("plan proposal key", key, 128)?;
        let payload = serde_json::to_string(&input).map_err(|e| e.to_string())?;
        let plan = build_plan(input)?;
        let mut conn = self.connect()?;
        let tx = immediate(&mut conn)?;
        if let Some(id) = plan_proposal(&tx, run, key, &payload)? {
            tx.commit().map_err(|e| e.to_string())?;
            drop(conn);
            return self.get_plan(&id);
        }
        tx.execute(
            "INSERT INTO plans(id,project,title,source) VALUES (?1,?2,?3,?4)",
            params![plan.id, plan.project, plan.title, plan.source],
        )
        .map_err(|e| e.to_string())?;
        tx.execute("INSERT INTO workflow_plan_proposals(run_id,proposal_key,plan_id,payload) VALUES (?1,?2,?3,?4)", params![run, key, plan.id, payload]).map_err(|e| e.to_string())?;
        tx.commit().map_err(|e| e.to_string())?;
        Ok(plan)
    }

    /// No create path is reachable from observation, including an absent receipt.
    pub fn existing_plan_for_proposal(
        &self,
        run: &str,
        key: &str,
        input: &NewPlan,
    ) -> Result<Option<Plan>, String> {
        validate_text("workflow run id", run, 128)?;
        validate_text("plan proposal key", key, 128)?;
        let payload = serde_json::to_string(input).map_err(|e| e.to_string())?;
        let id = plan_proposal(&*self.connect()?, run, key, &payload)?;
        id.map(|id| self.get_plan(&id)).transpose()
    }
}

fn plan_proposal(
    conn: &Connection,
    run: &str,
    key: &str,
    payload: &str,
) -> Result<Option<String>, String> {
    let prior: Option<(String, String)> = conn.query_row("SELECT plan_id,payload FROM workflow_plan_proposals WHERE run_id=?1 AND proposal_key=?2", params![run, key], |r| Ok((r.get(0)?, r.get(1)?))).optional().map_err(|e| e.to_string())?;
    match prior {
        Some((id, original)) if original == payload => Ok(Some(id)),
        Some(_) => Err("plan proposal key was reused with a different payload".into()),
        None => Ok(None),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // Catches: lost-response recovery creates a second plan, or changed input
    // borrows an earlier receipt. Reopening proves persistence, not just caching.
    #[test]
    fn issue_plan_once_retains_exact_identity_and_observation_never_creates() {
        let dir = tempfile::tempdir_in(crate::test_support::test_temp_root()).unwrap();
        let db = dir.path().join("stories.sqlite3");
        let store = StoryStore::open_at(&db).unwrap();
        let input = NewPlan {
            project: dir.path().to_string_lossy().into_owned(),
            title: "Issue plan".into(),
            source: "Issue run source".into(),
        };
        assert!(
            store
                .existing_plan_for_proposal("run", "issue", &input)
                .unwrap()
                .is_none()
        );
        assert_eq!(
            store.list_plans(&input.project).unwrap(),
            [] as [crate::stories::Plan; 0]
        );
        let plan = store
            .create_plan_once("run", "issue", input.clone())
            .unwrap();
        drop(store);
        let store = StoryStore::open_at(&db).unwrap();
        assert_eq!(
            store
                .create_plan_once("run", "issue", input.clone())
                .unwrap()
                .id,
            plan.id
        );
        assert_eq!(
            store
                .existing_plan_for_proposal("run", "issue", &input)
                .unwrap()
                .unwrap()
                .id,
            plan.id
        );
        let mut changed = input.clone();
        changed.source.push_str(" changed");
        assert!(
            store
                .create_plan_once("run", "issue", changed.clone())
                .is_err()
        );
        assert!(
            store
                .existing_plan_for_proposal("run", "issue", &changed)
                .is_err()
        );
        assert_eq!(store.list_plans(&input.project).unwrap().len(), 1);
    }

    // Catches: issue problem text is dropped, or changing it reuses a proposal.
    #[test]
    fn issue_story_once_preserves_problem_and_refuses_changed_problem() {
        let dir = tempfile::tempdir_in(crate::test_support::test_temp_root()).unwrap();
        let store = StoryStore::open_at(&dir.path().join("stories.sqlite3")).unwrap();
        let plan = store
            .create_plan(NewPlan {
                project: dir.path().to_string_lossy().into_owned(),
                title: "Issue".into(),
                source: "source".into(),
            })
            .unwrap();
        let input = NewStory {
            plan_id: plan.id.clone(),
            title: "Resolve issue".into(),
            criteria: vec!["Resolve the source issue".into()],
            priority: 2,
            story_type: None,
            origin: StoryOrigin::GitHubIssue { number: 3 },
            file_scope: vec![],
        };
        let problem = "Title\n\nExact body\n\nhttps://github.com/owner/repo/issues/3";
        assert!(
            store
                .existing_story_with_problem_for_proposal("run", "issue", &input, Some(problem))
                .unwrap()
                .is_none()
        );
        let story = store
            .create_story_with_problem_once("run", "issue", input.clone(), Some(problem.into()))
            .unwrap();
        assert_eq!(story.problem.as_deref(), Some(problem));
        assert_eq!(
            store
                .create_story_with_problem_once("run", "issue", input.clone(), Some(problem.into()))
                .unwrap()
                .id,
            story.id
        );
        assert!(
            store
                .create_story_with_problem_once(
                    "run",
                    "issue",
                    input.clone(),
                    Some("changed".into())
                )
                .is_err()
        );
        assert!(
            store
                .existing_story_with_problem_for_proposal("run", "issue", &input, None)
                .is_err()
        );
        assert_eq!(store.list_stories(&plan.id).unwrap().len(), 1);
        let observed = store
            .existing_story_with_problem_for_proposal("run", "issue", &input, Some(problem))
            .unwrap()
            .unwrap();
        assert_eq!(observed.problem.as_deref(), Some(problem));
    }
}
