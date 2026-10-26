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
        let plan_id = self.get_story(id)?.plan_id;
        let plan = self.get_plan(&plan_id)?;
        let _run_guard = crate::workflows::guard_native_archive(
            &self.db_path.with_file_name("workflow_runs.sqlite3"),
            &plan.project,
            &plan_id,
            Some(id),
        )?;
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
        let plan = self.get_plan(id)?;
        let _run_guard = crate::workflows::guard_native_archive(
            &self.db_path.with_file_name("workflow_runs.sqlite3"),
            &plan.project,
            id,
            None,
        )?;
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

    /// True only when every revision after acceptance is a durable archive event.
    /// The primary key makes the count an exact contiguous-range check, not a heuristic.
    pub(crate) fn archive_only_since(
        &self,
        id: &str,
        accepted: i64,
        current: i64,
    ) -> Result<bool, String> {
        if accepted <= 0 || current < accepted {
            return Ok(false);
        }
        let conn = self.connect()?;
        let count: i64 = conn.query_row(
            "SELECT COUNT(*) FROM archive_history WHERE entity_id=?1 AND revision>?2 AND revision<=?3",
            params![id, accepted, current], |row| row.get(0),
        ).map_err(|e| format!("read archive revision continuity: {e}"))?;
        Ok(count == current - accepted)
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

    /// Catches: archive history bridging an ordinary work revision and certifying changed work.
    #[test]
    fn archive_revision_continuity_rejects_work_changes_and_future_receipts() {
        let dir = tempfile::tempdir_in(crate::test_support::test_temp_root()).unwrap();
        let store = StoryStore::open_at(&dir.path().join("stories.sqlite3")).unwrap();
        let plan = store
            .create_plan(NewPlan {
                project: "/project".into(),
                title: "Continuity".into(),
                source: "plan.md".into(),
            })
            .unwrap();
        let initial = store
            .create_story(NewStory {
                plan_id: plan.id,
                title: "Work".into(),
                criteria: vec!["Done".into()],
                priority: 2,
                story_type: None,
                origin: StoryOrigin::Native,
                file_scope: vec![],
            })
            .unwrap();
        let actor = StoryTransitionActor::Human;
        let archived = store
            .set_story_archived(&initial.id, initial.revision, true, actor.clone())
            .unwrap();
        let restored = store
            .set_story_archived(&initial.id, archived.revision, false, actor.clone())
            .unwrap();
        assert!(
            store
                .archive_only_since(&initial.id, initial.revision, restored.revision)
                .unwrap()
        );
        assert!(
            !store
                .archive_only_since(&initial.id, restored.revision + 1, restored.revision)
                .unwrap()
        );
        let changed = store
            .transition(&initial.id, restored.revision, StoryCommand::StartManual)
            .unwrap();
        let archived = store
            .set_story_archived(&initial.id, changed.revision, true, actor.clone())
            .unwrap();
        let restored = store
            .set_story_archived(&initial.id, archived.revision, false, actor)
            .unwrap();
        assert!(
            !store
                .archive_only_since(&initial.id, initial.revision, restored.revision)
                .unwrap()
        );
        assert!(
            store
                .archive_only_since(&initial.id, changed.revision, restored.revision)
                .unwrap()
        );
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
