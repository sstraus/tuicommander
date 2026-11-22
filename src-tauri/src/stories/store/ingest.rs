use super::*;
use crate::mdkb_client::ExternalDocument;
use std::collections::BTreeMap;
use std::fmt::Write;

/// An ingest revision is independent of the story revision: notes deliberately
/// preserve workflow proof, but must still replace the indexed document.
#[derive(Debug)]
pub(crate) struct IngestRecord {
    pub revision: i64,
    pub document: Option<ExternalDocument>,
}

#[derive(Debug)]
pub(crate) struct IngestVersion {
    pub id: String,
    pub revision: i64,
}

/// Ingest ledger and its triggers. The triggers share the mutation's
/// commit/rollback boundary, including writes from workflow callers.
pub(crate) const SCHEMA: &str = "CREATE TABLE IF NOT EXISTS story_ingest (
            id TEXT PRIMARY KEY,
            project TEXT NOT NULL,
            revision INTEGER NOT NULL CHECK(typeof(revision)='integer' AND revision>0),
            updated_at TEXT NOT NULL,
            deleted INTEGER NOT NULL DEFAULT 0
        );
        CREATE TRIGGER IF NOT EXISTS story_ingest_insert AFTER INSERT ON stories BEGIN
          INSERT INTO story_ingest(id,project,revision,updated_at)
            SELECT NEW.id,p.project,NEW.revision,strftime('%Y-%m-%dT%H:%M:%fZ','now')
            FROM plans p WHERE p.id=NEW.plan_id
            ON CONFLICT(id) DO UPDATE SET revision=story_ingest.revision+1,
              deleted=0,updated_at=excluded.updated_at;
        END;
        CREATE TRIGGER IF NOT EXISTS story_ingest_update AFTER UPDATE ON stories BEGIN
          UPDATE story_ingest SET revision=revision+1,
            updated_at=strftime('%Y-%m-%dT%H:%M:%fZ','now') WHERE id=NEW.id;
        END;
        CREATE TRIGGER IF NOT EXISTS story_ingest_delete AFTER DELETE ON stories BEGIN
          UPDATE story_ingest SET revision=revision+1,deleted=1,
            updated_at=strftime('%Y-%m-%dT%H:%M:%fZ','now') WHERE id=OLD.id;
        END;
        CREATE TRIGGER IF NOT EXISTS story_ingest_note AFTER INSERT ON story_notes BEGIN
          UPDATE story_ingest SET revision=revision+1,
            updated_at=strftime('%Y-%m-%dT%H:%M:%fZ','now') WHERE id=NEW.story_id;
        END;";

impl StoryStore {
    /// Lightweight inventory, including tombstones. Bodies are read one at a time.
    pub(crate) fn ingest_ids(&self) -> Result<Vec<(String, Vec<IngestVersion>)>, String> {
        let conn = self.connect()?;
        let mut stmt = conn
            .prepare("SELECT project,id,revision FROM story_ingest ORDER BY project,id")
            .map_err(|error| format!("prepare story ingest ids: {error}"))?;
        let rows = stmt
            .query_map([], |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    IngestVersion {
                        id: row.get(1)?,
                        revision: row.get(2)?,
                    },
                ))
            })
            .map_err(|error| format!("list story ingest ids: {error}"))?;
        let mut roots: BTreeMap<String, Vec<IngestVersion>> = BTreeMap::new();
        for row in rows {
            let (root, id) = row.map_err(|error| format!("read story ingest id: {error}"))?;
            roots.entry(root).or_default().push(id);
        }
        Ok(roots.into_iter().collect())
    }

    /// One SQLite read snapshot pairs the revision with its story and notes.
    /// No lock or read transaction survives a network call.
    pub(crate) fn ingest_document(&self, id: &str) -> Result<IngestRecord, String> {
        let mut conn = self.connect()?;
        let tx = conn
            .transaction()
            .map_err(|error| format!("begin ingest snapshot: {error}"))?;
        let (revision, updated_at, deleted): (i64, String, bool) = tx
            .query_row(
                "SELECT revision,updated_at,deleted FROM story_ingest WHERE id=?1",
                [id],
                |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
            )
            .map_err(|error| format!("read story ingest revision: {error}"))?;
        if deleted {
            return Ok(IngestRecord {
                revision,
                document: None,
            });
        }
        let story = read_story(&tx, id)?;
        let mut body = format!(
            "# {}\n\nStatus: {}\nStory revision: {}\nUpdated: {}\n\n",
            story.title,
            story.status.as_str(),
            story.revision,
            updated_at
        );
        if let Some(problem) = &story.problem {
            writeln!(body, "## Problem\n\n{problem}\n").expect("write string");
        }
        body.push_str("## Acceptance criteria\n\n");
        for (index, criterion) in story.criteria.iter().enumerate() {
            let mark = if story.checked.get(index).copied().unwrap_or(false) {
                'x'
            } else {
                ' '
            };
            writeln!(body, "- [{mark}] {criterion}").expect("write string");
        }
        body.push_str("\n## Notes / worklog\n");
        let mut stmt = tx
            .prepare("SELECT document FROM story_notes WHERE story_id=?1 ORDER BY rowid")
            .map_err(|error| format!("prepare ingest notes: {error}"))?;
        let notes = stmt
            .query_map([id], |row| row.get::<_, String>(0))
            .map_err(|error| format!("list ingest notes: {error}"))?;
        for note in notes {
            let note: super::super::StoryNote =
                serde_json::from_str(&note.map_err(|error| format!("read ingest note: {error}"))?)
                    .map_err(|error| format!("decode ingest note: {error}"))?;
            writeln!(body, "\n### {}\n\n{}", note.created_at, note.text).expect("write string");
        }
        let metadata = serde_json::json!({
            "status": story.status.as_str(), "type": story.story_type,
            "updated_at": updated_at, "revision": revision,
            "story_revision": story.revision, "archived": story.archived,
            "plan_id": story.plan_id,
        });
        Ok(IngestRecord {
            revision,
            document: Some(ExternalDocument {
                title: story.title,
                body,
                metadata,
            }),
        })
    }
}
