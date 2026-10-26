use super::*;

impl StoryStore {
    /// Apply a patch under one revision check and one story transaction.
    pub fn update_story(
        &self,
        story_id: &str,
        expected_revision: i64,
        patch: StoryPatch,
    ) -> Result<Story, String> {
        let plan_id = self.get_story(story_id)?.plan_id;
        let project = self.get_plan(&plan_id)?.project;
        let preflight = DependencyPreflight::prepare(self, &plan_id)?;
        // Same lock order as workflow start: run reservations, then story records.
        let _reservation = crate::workflows::guard_story_amendment(
            &self.db_path.with_file_name("workflow_runs.sqlite3"),
            &project,
            story_id,
        )?;
        let mut conn = self.connect()?;
        let tx = immediate(&mut conn)?;
        preflight.validate(&tx)?;
        let mut story = read_story(&tx, story_id)?;
        check_revision(&story, expected_revision)?;
        require_active_story(&tx, &story)?;
        if story.claim_session.is_some() {
            return Err("claimed stories cannot be amended".into());
        }
        if !matches!(story.status, StoryStatus::Backlog | StoryStatus::Ready) {
            return Err(format!(
                "only backlog or ready stories can be amended (status: {})",
                story.status.as_str()
            ));
        }
        if let Some(title) = patch.title {
            story.title = title.trim().into();
        }
        if let Some(priority) = patch.priority {
            story.priority = priority;
        }
        if let Some(scope) = patch.file_scope {
            story.file_scope = scope;
        }
        if let Some(story_type) = patch.story_type {
            story.story_type = story_type;
        }
        if let Some(origin) = patch.origin {
            story.origin = origin;
        }
        if let Some(problem) = patch.problem {
            if let Some(text) = &problem {
                validate_text("problem", text, 20_000)?;
            }
            story.problem = problem;
        }
        if let Some(risk) = patch.risk {
            story.risk = risk;
        }
        if let Some(criteria) = patch.criteria {
            validate_criteria(&criteria)?;
            // Text is the existing requirement identity. Ambiguous duplicates cannot carry proof.
            story.checked = criteria
                .iter()
                .map(|text| {
                    if criteria.iter().filter(|other| *other == text).count() != 1 {
                        return false;
                    }
                    let mut matches = story
                        .criteria
                        .iter()
                        .enumerate()
                        .filter(|(_, old)| *old == text);
                    match (matches.next(), matches.next()) {
                        (Some((index, _)), None) => {
                            story.checked.get(index).copied().unwrap_or(false)
                        }
                        _ => false,
                    }
                })
                .collect();
            story.criteria = criteria;
        }
        validate_new_story(&NewStory {
            plan_id: story.plan_id.clone(),
            title: story.title.clone(),
            criteria: story.criteria.clone(),
            priority: story.priority,
            story_type: story.story_type,
            origin: story.origin.clone(),
            file_scope: story.file_scope.clone(),
        })?;
        if let Some(dependencies) = patch.dependencies {
            let mut unique = HashSet::new();
            let mut seen = HashSet::new();
            for id in &dependencies {
                if !unique.insert(id) {
                    return Err(format!("duplicate dependency: {id}"));
                }
                let dependency = read_story(&tx, id)?;
                if dependency.plan_id != story.plan_id {
                    return Err("dependencies must belong to one plan".into());
                }
                let mut stack = vec![id.clone()];
                while let Some(next) = stack.pop() {
                    if next == story.id {
                        return Err(format!("dependency cycle through {id}"));
                    }
                    if seen.insert(next.clone()) {
                        stack.extend(read_story(&tx, &next)?.dependencies);
                    }
                }
            }
            story.dependencies = dependencies;
        }
        story.status = if dependencies_integrated(&tx, &story, &preflight)? {
            StoryStatus::Ready
        } else {
            StoryStatus::Backlog
        };
        save_story(&tx, &mut story, expected_revision)?;
        tx.commit()
            .map_err(|error| format!("commit story amendment: {error}"))?;
        Ok(story)
    }
}
