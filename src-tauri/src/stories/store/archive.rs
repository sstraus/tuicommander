use super::*;
use crate::stories::ArchiveEvent;

impl StoryStore {
    pub(crate) fn require_unarchived_plan(&self, id: &str) -> Result<Plan, String> {
        let plan = self.get_plan(id)?;
        if plan.archived {
            return Err("plan is archived; restore it first".into());
        }
        Ok(plan)
    }

    pub(crate) fn require_unarchived_story(&self, id: &str) -> Result<Story, String> {
        let story = self.get_story(id)?;
        if story.archived {
            return Err("story is archived; restore it first".into());
        }
        self.require_unarchived_plan(&story.plan_id)?;
        Ok(story)
    }

    pub fn set_story_archived(
        &self,
        id: &str,
        expected_revision: i64,
        archived: bool,
        actor: StoryTransitionActor,
    ) -> Result<Story, String> {
        let mut conn = self.connect()?;
        let tx = immediate(&mut conn)?;
        let mut story = read_story(&tx, id)?;
        check_revision(&story, expected_revision)?;
        if story.claim_session.is_some() {
            return Err("claimed story cannot be archived or restored".into());
        }
        if story.archived == archived {
            return Err("story already has the requested archive state".into());
        }
        if archived && story.status != StoryStatus::Done {
            let dependents: Vec<_> = read_plan_stories(&tx, &story.plan_id)?
                .into_iter()
                .filter(|candidate| {
                    !matches!(candidate.status, StoryStatus::Done | StoryStatus::WontFix)
                        && candidate.dependencies.contains(&story.id)
                })
                .map(|candidate| candidate.id)
                .collect();
            if !dependents.is_empty() {
                return Err(format!(
                    "unresolved prerequisite cannot be archived; open dependents: {}",
                    dependents.join(", ")
                ));
            }
        }
        story.archived = archived;
        save_story(&tx, &mut story, expected_revision)?;
        record_archive(&tx, id, story.revision, archived, &actor)?;
        tx.commit()
            .map_err(|e| format!("commit story archive: {e}"))?;
        Ok(story)
    }

    pub fn set_plan_archived(
        &self,
        id: &str,
        expected_revision: i64,
        archived: bool,
        actor: StoryTransitionActor,
    ) -> Result<Plan, String> {
        let mut conn = self.connect()?;
        let tx = immediate(&mut conn)?;
        let (revision, current): (i64, bool) = tx
            .query_row(
                "SELECT revision,archived FROM plans WHERE id=?1",
                [id],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .map_err(|e| format!("read plan archive state: {e}"))?;
        if revision != expected_revision {
            return Err(format!(
                "stale plan revision: expected {expected_revision}, current {revision}"
            ));
        }
        if current == archived {
            return Err("plan already has the requested archive state".into());
        }
        if read_plan_stories(&tx, id)?
            .iter()
            .any(|story| story.claim_session.is_some())
        {
            return Err("plan contains a claimed story".into());
        }
        let revision = revision.checked_add(1).ok_or("plan revision exhausted")?;
        tx.execute(
            "UPDATE plans SET revision=?1,archived=?2 WHERE id=?3 AND revision=?4",
            params![revision, archived, id, expected_revision],
        )
        .map_err(|e| format!("save plan archive: {e}"))?;
        record_archive(&tx, id, revision, archived, &actor)?;
        tx.commit()
            .map_err(|e| format!("commit plan archive: {e}"))?;
        // Read only after releasing this store's connection mutex.
        drop(conn);
        self.get_plan(id)
    }

    pub fn archive_history(&self, id: &str) -> Result<Vec<ArchiveEvent>, String> {
        let conn = self.connect()?;
        let mut stmt = conn.prepare(
            "SELECT revision,archived,actor_json FROM archive_history WHERE entity_id=?1 ORDER BY revision"
        ).map_err(|e| format!("prepare archive history: {e}"))?;
        stmt.query_map([id], |row| {
            Ok((
                row.get::<_, i64>(0)?,
                row.get::<_, bool>(1)?,
                row.get::<_, String>(2)?,
            ))
        })
        .map_err(|e| format!("read archive history: {e}"))?
        .map(|row| {
            let (revision, archived, actor) =
                row.map_err(|e| format!("read archive event: {e}"))?;
            Ok(ArchiveEvent {
                revision,
                archived,
                actor: serde_json::from_str(&actor)
                    .map_err(|e| format!("decode archive actor: {e}"))?,
            })
        })
        .collect()
    }
}

fn record_archive(
    tx: &rusqlite::Transaction<'_>,
    id: &str,
    revision: i64,
    archived: bool,
    actor: &StoryTransitionActor,
) -> Result<(), String> {
    tx.execute(
        "INSERT INTO archive_history(entity_id,revision,archived,actor_json) VALUES (?1,?2,?3,?4)",
        params![
            id,
            revision,
            archived,
            serde_json::to_string(actor).map_err(|e| format!("encode archive actor: {e}"))?
        ],
    )
    .map_err(|e| format!("record archive history: {e}"))?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn prerequisite_fixture(store: &StoryStore) -> (Plan, Story, Story) {
        let plan = store
            .create_plan(NewPlan {
                project: "/project".into(),
                title: "Prerequisites".into(),
                source: "plan.md".into(),
            })
            .unwrap();
        let create = |title: &str| {
            store
                .create_story(NewStory {
                    plan_id: plan.id.clone(),
                    title: title.into(),
                    criteria: vec!["Verified".into()],
                    priority: 2,
                    story_type: None,
                    origin: StoryOrigin::Native,
                    file_scope: vec![],
                })
                .unwrap()
        };
        let prerequisite = create("Prerequisite");
        let dependent = create("Dependent");
        (plan, prerequisite, dependent)
    }

    /// Catches: an open dependent being blocked by an archived Unknown prerequisite.
    #[test]
    fn archiving_unresolved_prerequisite_does_not_hide_blocker() {
        for cancelled in [false, true] {
            let dir = tempfile::tempdir_in(crate::test_support::test_temp_root()).unwrap();
            let store = StoryStore::open_at(&dir.path().join("stories.sqlite3")).unwrap();
            let (plan, mut prerequisite, dependent) = prerequisite_fixture(&store);
            let dependent = store
                .add_dependency(&dependent.id, &prerequisite.id, dependent.revision)
                .unwrap();
            if cancelled {
                prerequisite = store
                    .transition(
                        &prerequisite.id,
                        prerequisite.revision,
                        StoryCommand::WontFix,
                    )
                    .unwrap();
            }
            for archived_plan in [false, true] {
                if archived_plan {
                    store
                        .set_plan_archived(
                            &plan.id,
                            plan.revision,
                            true,
                            StoryTransitionActor::Human,
                        )
                        .unwrap();
                }
                let error = store
                    .set_story_archived(
                        &prerequisite.id,
                        prerequisite.revision,
                        true,
                        StoryTransitionActor::Human,
                    )
                    .expect_err("unresolved prerequisite must remain visible");
                assert!(error.contains(&dependent.id), "{error}");
                assert_eq!(store.get_story(&prerequisite.id).unwrap(), prerequisite);
                assert_eq!(store.archive_history(&prerequisite.id).unwrap(), vec![]);
                let view = store.plan_view(&plan.id).unwrap();
                assert!(view.visible_story_ids.contains(&prerequisite.id));
                assert_eq!(
                    store.get_story(&dependent.id).unwrap().status,
                    StoryStatus::Backlog
                );
            }
        }
    }

    /// Catches: the unresolved-prerequisite guard refusing completed work or closed dependents.
    #[test]
    fn archive_prerequisite_allows_done_work_and_closed_dependents() {
        for done in [false, true] {
            let dir = tempfile::tempdir_in(crate::test_support::test_temp_root()).unwrap();
            let store = StoryStore::open_at(&dir.path().join("stories.sqlite3")).unwrap();
            let (plan, mut prerequisite, dependent) = prerequisite_fixture(&store);
            let dependent = store
                .add_dependency(&dependent.id, &prerequisite.id, dependent.revision)
                .unwrap();
            if done {
                for command in [
                    StoryCommand::StartManual,
                    StoryCommand::CheckCriterion(0),
                    StoryCommand::SubmitReview,
                    StoryCommand::Approve,
                ] {
                    prerequisite = store
                        .transition(&prerequisite.id, prerequisite.revision, command)
                        .unwrap();
                }
                assert_eq!(
                    store.get_story(&dependent.id).unwrap().status,
                    StoryStatus::Ready
                );
            } else {
                store
                    .transition(&dependent.id, dependent.revision, StoryCommand::WontFix)
                    .unwrap();
            }
            let archived = store
                .set_story_archived(
                    &prerequisite.id,
                    prerequisite.revision,
                    true,
                    StoryTransitionActor::Human,
                )
                .unwrap();
            assert_eq!(archived.status, prerequisite.status);
            assert!(archived.archived);
            assert!(
                !store
                    .plan_view(&plan.id)
                    .unwrap()
                    .visible_story_ids
                    .contains(&prerequisite.id)
            );
            assert!(
                store
                    .plan_view_archived(&plan.id, true)
                    .unwrap()
                    .visible_story_ids
                    .contains(&prerequisite.id)
            );
            if done {
                assert_eq!(
                    store.get_story(&dependent.id).unwrap().status,
                    StoryStatus::Ready
                );
            }
            let restored = store
                .set_story_archived(
                    &prerequisite.id,
                    archived.revision,
                    false,
                    StoryTransitionActor::Human,
                )
                .unwrap();
            assert!(!restored.archived);
        }
    }

    /// Catches: adding a dependency after archival recreating the hidden-blocker state through either editor.
    #[test]
    fn archive_prerequisite_requires_restore_before_adding_unresolved_edges() {
        for patch in [false, true] {
            let dir = tempfile::tempdir_in(crate::test_support::test_temp_root()).unwrap();
            let store = StoryStore::open_at(&dir.path().join("stories.sqlite3")).unwrap();
            let (_, prerequisite, dependent) = prerequisite_fixture(&store);
            let archived = store
                .set_story_archived(
                    &prerequisite.id,
                    prerequisite.revision,
                    true,
                    StoryTransitionActor::Human,
                )
                .unwrap();
            let result = if patch {
                store.update_story(
                    &dependent.id,
                    dependent.revision,
                    StoryPatch {
                        dependencies: Some(vec![prerequisite.id.clone()]),
                        ..Default::default()
                    },
                )
            } else {
                store.add_dependency(&dependent.id, &prerequisite.id, dependent.revision)
            };
            assert!(result.unwrap_err().contains("restore"));
            assert_eq!(store.get_story(&dependent.id).unwrap(), dependent);
            let restored = store
                .set_story_archived(
                    &prerequisite.id,
                    archived.revision,
                    false,
                    StoryTransitionActor::Human,
                )
                .unwrap();
            let blocked = store
                .add_dependency(&dependent.id, &restored.id, dependent.revision)
                .unwrap();
            assert_eq!(blocked.status, StoryStatus::Backlog);
        }
    }

    /// Catches: schema v4 documents without the new flag failing to decode or being rewritten during migration.
    #[test]
    fn archive_migration_defaults_legacy_records_without_rewriting_documents() {
        let dir = tempfile::tempdir_in(crate::test_support::test_temp_root()).unwrap();
        let db = dir.path().join("stories.sqlite3");
        let store = StoryStore::open_at(&db).unwrap();
        let plan = store
            .create_plan(NewPlan {
                project: "/project".into(),
                title: "Legacy".into(),
                source: "plan.md".into(),
            })
            .unwrap();
        let story = store
            .create_story(NewStory {
                plan_id: plan.id.clone(),
                title: "Existing".into(),
                criteria: vec!["Preserved".into()],
                priority: 2,
                story_type: None,
                origin: StoryOrigin::Native,
                file_scope: vec![],
            })
            .unwrap();
        drop(store);
        let conn = Connection::open(&db).unwrap();
        let mut document = serde_json::to_value(&story).unwrap();
        document.as_object_mut().unwrap().remove("archived");
        let document = serde_json::to_string(&document).unwrap();
        conn.execute(
            "UPDATE stories SET document=?1 WHERE id=?2",
            params![document, story.id],
        )
        .unwrap();
        conn.execute_batch("ALTER TABLE plans DROP COLUMN archived; DROP TABLE archive_history; PRAGMA user_version=4;").unwrap();
        drop(conn);
        let store = StoryStore::open_at(&db).unwrap();
        assert_eq!(store.get_story(&story.id).unwrap(), story);
        assert_eq!(store.get_plan(&plan.id).unwrap(), plan);
        let stored: String = store
            .connect()
            .unwrap()
            .query_row(
                "SELECT document FROM stories WHERE id=?1",
                [&story.id],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(stored, document);
        let archived = store
            .set_story_archived(&story.id, story.revision, true, StoryTransitionActor::Human)
            .unwrap();
        assert!(archived.archived);
        assert_eq!(store.archive_history(&story.id).unwrap().len(), 1);
    }
}
