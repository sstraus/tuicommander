use super::*;
use crate::stories::{STORY_NOTE_INVALID, StoryNote, StoryReceiptReference};

impl StoryStore {
    /// Append commentary against a checked subject, without invalidating workflow proof.
    pub fn add_note(
        &self,
        story_id: &str,
        expected_revision: i64,
        text: &str,
        actor: StoryTransitionActor,
        receipts: Vec<StoryReceiptReference>,
    ) -> Result<StoryNote, String> {
        validate_text("text", text, 16_000).map_err(invalid)?;
        if receipts.len() > 16 {
            return Err(invalid("at most 16 receipt references are allowed"));
        }
        let mut distinct = HashSet::new();
        for receipt in &receipts {
            validate_text("receipt run id", &receipt.run_id, 128).map_err(invalid)?;
            validate_text("receipt command id", &receipt.command_id, 200).map_err(invalid)?;
            if !distinct.insert(receipt) {
                return Err(invalid("duplicate receipt reference"));
            }
        }
        // Workflow receipts and run ownership are immutable. Probe outside the story write
        // transaction, so adding evidence never acquires the two stores in reverse order.
        let subject = self.get_story(story_id)?;
        let plan = self.get_plan(&subject.plan_id)?;
        if !receipts.is_empty() {
            let run_db = self
                .db_path
                .parent()
                .ok_or("story store has no parent directory")?
                .join("workflow_runs.sqlite3");
            if !run_db.is_file() {
                return Err(invalid("workflow receipt not found"));
            }
            let runs = crate::workflows::RunStore::open_at(&run_db)?;
            for receipt in &receipts {
                if !runs.receipt_belongs_to_plan(
                    &receipt.run_id,
                    &receipt.command_id,
                    &plan.project,
                    &plan.id,
                )? {
                    return Err(invalid(
                        "workflow receipt does not exist in the story project and plan",
                    ));
                }
            }
        }
        let mut conn = self.connect()?;
        let tx = immediate(&mut conn)?;
        let story = read_story(&tx, story_id)?;
        check_revision(&story, expected_revision).map_err(invalid)?;
        let note = StoryNote {
            id: Uuid::now_v7().to_string(),
            story_id: story.id,
            story_revision: story.revision,
            text: text.into(),
            actor,
            created_at: chrono::Utc::now().timestamp_millis(),
            receipts,
        };
        let document =
            serde_json::to_string(&note).map_err(|e| format!("encode story note: {e}"))?;
        tx.execute(
            "INSERT INTO story_notes(id,story_id,document) VALUES (?1,?2,?3)",
            params![note.id, note.story_id, document],
        )
        .map_err(|e| format!("append story note: {e}"))?;
        tx.commit().map_err(|e| format!("commit story note: {e}"))?;
        Ok(note)
    }

    pub fn list_notes(&self, story_id: &str) -> Result<Vec<StoryNote>, String> {
        let conn = self.connect()?;
        read_story(&conn, story_id)?;
        let mut stmt = conn
            .prepare("SELECT document FROM story_notes WHERE story_id=?1 ORDER BY rowid")
            .map_err(|e| format!("prepare story notes: {e}"))?;
        stmt.query_map([story_id], |row| row.get::<_, String>(0))
            .map_err(|e| format!("list story notes: {e}"))?
            .map(|row| {
                let document = row.map_err(|e| format!("read story note: {e}"))?;
                serde_json::from_str(&document).map_err(|e| format!("decode story note: {e}"))
            })
            .collect()
    }
}

fn invalid(message: impl std::fmt::Display) -> String {
    format!("{STORY_NOTE_INVALID}{message}")
}
