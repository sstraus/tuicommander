use super::model::{
    DEFAULT_PAGE_LIMIT, MAX_PAGE_LIMIT, NewProgressEntry, ProgressDeleteReceipt, ProgressEntry,
    ProgressKind, ProgressList, ProgressListInput, ProgressViewedReceipt,
};
use rusqlite::{Connection, OptionalExtension, TransactionBehavior, params};
use std::path::PathBuf;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

const BUSY_TIMEOUT: Duration = Duration::from_secs(5);

/// The one journal, for every project.
///
/// It lives in the app's config directory and never inside a repository:
/// nothing Progress writes belongs in the user's source tree. `project` is a
/// column, so closing a session, deleting a temporary workspace or parking a
/// project cannot take a journal with it.
const STORE_FILE: &str = "progress.sqlite3";

/// How many terminals the list names for the filter: the most recently active.
/// Unbounded, the page grew with the journal (1704 ids was 67 KB beside 8 entries).
const PTY_IDS_LIMIT: i64 = 32;

#[derive(Clone, Debug)]
pub struct ProgressStore {
    db_path: PathBuf,
}

impl ProgressStore {
    /// Open (and create on first use) the shared journal.
    ///
    /// Connections are deliberately not retained: separate TUIC processes write
    /// this same file, and SQLite is the serialization boundary.
    pub fn open() -> Result<Self, String> {
        let dir = crate::config::config_dir();
        std::fs::create_dir_all(&dir).map_err(|error| {
            format!(
                "progress_store_unavailable: cannot create '{}': {error}",
                dir.display()
            )
        })?;
        let store = Self {
            db_path: dir.join(STORE_FILE),
        };
        store.connect()?;
        Ok(store)
    }

    /// Test-support accessor: the repository-watcher test needs the file the
    /// store writes, to prove it is not under any repository.
    #[cfg(test)]
    pub fn database_path(&self) -> &std::path::Path {
        &self.db_path
    }

    fn connect(&self) -> Result<Connection, String> {
        let mut conn = Connection::open(&self.db_path).map_err(|error| {
            format!(
                "progress_store_unavailable: cannot open '{}': {error}",
                self.db_path.display()
            )
        })?;
        conn.busy_timeout(BUSY_TIMEOUT)
            .map_err(db_error("set progress busy timeout"))?;
        // WAL survives the timeout: several TUIC processes do write this file,
        // and a reader must not block the writer that is recording an entry.
        conn.pragma_update(None, "journal_mode", "WAL")
            .map_err(db_error("enable WAL on the progress store"))?;
        conn.execute_batch(
            // AUTOINCREMENT, not a bare rowid. A plain `INTEGER PRIMARY KEY`
            // hands out `max(rowid) + 1`, so deleting the newest entry and
            // writing the next one reuses its id — and a dialog still holding
            // the old id would then delete an entry it never showed.
            "CREATE TABLE IF NOT EXISTS entries (
               id            INTEGER PRIMARY KEY AUTOINCREMENT,
               project       TEXT NOT NULL,
               created_at_ms INTEGER NOT NULL,
               kind          TEXT NOT NULL CHECK (kind IN ('done','blocked','intent','delegated','message')),
               text          TEXT NOT NULL,
               step          TEXT,
               agent_name    TEXT,
               pty_id        TEXT,
               target_pty_id TEXT,
               target_name   TEXT
             );
             CREATE INDEX IF NOT EXISTS entries_by_project ON entries (project, id DESC);
             CREATE TABLE IF NOT EXISTS project_views (
               project        TEXT PRIMARY KEY,
               last_viewed_ms INTEGER NOT NULL
             );
             CREATE TABLE IF NOT EXISTS pty_views (
               project        TEXT NOT NULL,
               pty_id         TEXT NOT NULL,
               last_viewed_ms INTEGER NOT NULL,
               PRIMARY KEY (project, pty_id)
             );",
        )
        .map_err(db_error("prepare the progress schema"))?;
        if !Self::has_pty_column(&conn)? {
            // Serialize upgrades across processes, then inspect again: another
            // instance may have added the column while we waited for the lock.
            let tx = conn
                .transaction_with_behavior(TransactionBehavior::Immediate)
                .map_err(db_error("begin the progress schema migration"))?;
            if !Self::has_pty_column(&tx)? {
                tx.execute("ALTER TABLE entries ADD COLUMN pty_id TEXT", [])
                    .map_err(db_error("add terminal identity to the progress schema"))?;
            }
            tx.commit()
                .map_err(db_error("commit the progress schema migration"))?;
        }
        if !Self::has_hand_off_schema(&conn)? {
            let tx = conn
                .transaction_with_behavior(TransactionBehavior::Immediate)
                .map_err(db_error("begin the hand-off schema migration"))?;
            if !Self::has_hand_off_schema(&tx)? {
                Self::rebuild_for_hand_offs(&tx)?;
            }
            tx.commit()
                .map_err(db_error("commit the hand-off schema migration"))?;
        }
        // After the migrations: a rebuilt table has lost its indexes.
        conn.execute_batch(
            "CREATE INDEX IF NOT EXISTS entries_by_project ON entries (project, id DESC);
             CREATE INDEX IF NOT EXISTS entries_by_pty ON entries (project, pty_id, id DESC);",
        )
        .map_err(db_error("index terminal progress"))?;
        Ok(conn)
    }

    /// Whether `entries` accepts the hand-off kinds and carries their target.
    /// The CHECK constraint is part of the table's SQL, so the SQL is the only
    /// place to read it from.
    fn has_hand_off_schema(conn: &Connection) -> Result<bool, String> {
        let sql: String = conn
            .query_row(
                "SELECT sql FROM sqlite_master WHERE type = 'table' AND name = 'entries'",
                [],
                |row| row.get(0),
            )
            .map_err(db_error("inspect the progress schema"))?;
        Ok(sql.contains("'delegated'") && sql.contains("target_pty_id"))
    }

    /// SQLite cannot alter a CHECK constraint, so a journal from before the
    /// hand-off kinds is copied into a new table. Ids are copied verbatim and
    /// the AUTOINCREMENT high-water mark is carried over: a dialog may still
    /// hold the id of a deleted newest entry, and reissuing it would let that
    /// dialog delete an entry it never showed.
    ///
    /// The earliest journals used a bare `INTEGER PRIMARY KEY`, and such a file
    /// has no `sqlite_sequence` table at all. The mark is therefore the larger
    /// of the recorded sequence, when there is one, and the highest id present.
    fn rebuild_for_hand_offs(tx: &rusqlite::Transaction<'_>) -> Result<(), String> {
        let has_sequence: bool = tx
            .query_row(
                "SELECT EXISTS (SELECT 1 FROM sqlite_master
                                 WHERE type = 'table' AND name = 'sqlite_sequence')",
                [],
                |row| row.get(0),
            )
            .map_err(db_error("read the progress id high-water mark"))?;
        let sequence: Option<i64> = if has_sequence {
            tx.query_row(
                "SELECT seq FROM sqlite_sequence WHERE name = 'entries'",
                [],
                |row| row.get(0),
            )
            .optional()
            .map_err(db_error("read the progress id high-water mark"))?
        } else {
            None
        };
        let max_id: Option<i64> = tx
            .query_row("SELECT MAX(id) FROM entries", [], |row| row.get(0))
            .map_err(db_error("read the progress id high-water mark"))?;
        let high_water = sequence.max(max_id);
        tx.execute_batch(
            "CREATE TABLE entries_hand_offs (
               id            INTEGER PRIMARY KEY AUTOINCREMENT,
               project       TEXT NOT NULL,
               created_at_ms INTEGER NOT NULL,
               kind          TEXT NOT NULL CHECK (kind IN ('done','blocked','intent','delegated','message')),
               text          TEXT NOT NULL,
               step          TEXT,
               agent_name    TEXT,
               pty_id        TEXT,
               target_pty_id TEXT,
               target_name   TEXT
             );
             INSERT INTO entries_hand_offs (id, project, created_at_ms, kind, text, step, agent_name, pty_id)
               SELECT id, project, created_at_ms, kind, text, step, agent_name, pty_id FROM entries;
             DROP TABLE entries;
             ALTER TABLE entries_hand_offs RENAME TO entries;",
        )
        .map_err(db_error("rebuild the progress table for hand-offs"))?;
        if let Some(seq) = high_water {
            let updated = tx
                .execute(
                    "UPDATE sqlite_sequence SET seq = MAX(seq, ?1) WHERE name = 'entries'",
                    params![seq],
                )
                .map_err(db_error("carry the progress id high-water mark"))?;
            if updated == 0 {
                tx.execute(
                    "INSERT INTO sqlite_sequence (name, seq) VALUES ('entries', ?1)",
                    params![seq],
                )
                .map_err(db_error("carry the progress id high-water mark"))?;
            }
        }
        Ok(())
    }

    fn has_pty_column(conn: &Connection) -> Result<bool, String> {
        Ok(conn
            .prepare("PRAGMA table_info(entries)")
            .map_err(db_error("inspect the progress schema"))?
            .query_map([], |row| row.get::<_, String>(1))
            .map_err(db_error("inspect the progress schema"))?
            .collect::<Result<Vec<_>, _>>()
            .map_err(db_error("inspect the progress schema"))?
            .iter()
            .any(|column| column == "pty_id"))
    }

    /// Append one entry. There is nothing to reconcile: the table is
    /// append-only and the rowid is identity, order and cursor in one.
    #[cfg(test)]
    pub(crate) fn record(
        &self,
        project: &str,
        entry: &NewProgressEntry,
    ) -> Result<ProgressEntry, String> {
        self.record_for_pty(project, entry, None)
    }

    #[cfg(test)]
    pub(crate) fn record_for_pty(
        &self,
        project: &str,
        entry: &NewProgressEntry,
        pty_id: Option<&str>,
    ) -> Result<ProgressEntry, String> {
        self.record_hand_off(project, entry, pty_id, None, None)
    }

    /// Append one entry for a PTY, plus — for a `delegated` or `message`
    /// entry — the terminal it points at and that terminal's name at the time.
    pub(crate) fn record_hand_off(
        &self,
        project: &str,
        entry: &NewProgressEntry,
        pty_id: Option<&str>,
        target_pty_id: Option<&str>,
        target_name: Option<&str>,
    ) -> Result<ProgressEntry, String> {
        let mut entry = entry.clone();
        entry.text = crate::redaction::redact_secrets(&entry.text);
        if let Some(step) = entry.step.as_mut() {
            *step = crate::redaction::redact_secrets(step);
        }
        entry.validate()?;
        let target_name = target_name.and_then(super::model::bounded_name);
        let mut conn = self.connect()?;
        let tx = conn
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .map_err(db_error("begin progress transaction"))?;
        let created_at_ms = now_ms();
        let text = entry.trimmed_text();
        let step = entry.trimmed_step();
        let agent_name = entry.trimmed_agent_name();
        // A repeat of the project's newest intent is that intent, not a new
        // row: the changed-row parser hands the same `intent:` line back on
        // every repaint it survives, and one intent landed 17 times in 8 s.
        // Agent reports are deliberately NOT collapsed — an agent that reported
        // the same step twice did the work twice, and only the reader can say
        // what that means (docs/user-guide/project-progress.md).
        if entry.kind == ProgressKind::Intent
            && let Some(newest) = Self::newest(&tx, project, pty_id)?
            && newest.kind == ProgressKind::Intent
            && newest.text == text
            && newest.agent_name == agent_name
        {
            return Ok(newest);
        }
        tx.execute(
            "INSERT INTO entries (project, created_at_ms, kind, text, step, agent_name, pty_id,
                                  target_pty_id, target_name)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
            params![
                project,
                i64_from_u64(created_at_ms),
                entry.kind.as_str(),
                &text,
                &step,
                &agent_name,
                pty_id,
                target_pty_id,
                &target_name,
            ],
        )
        .map_err(db_error("insert progress entry"))?;
        let id = tx.last_insert_rowid();
        tx.commit().map_err(db_error("commit progress entry"))?;
        Ok(ProgressEntry {
            id,
            project: project.to_string(),
            pty_id: pty_id.map(str::to_string),
            created_at_ms,
            kind: entry.kind,
            text,
            step,
            agent_name,
            target_pty_id: target_pty_id.map(str::to_string),
            target_name,
        })
    }

    /// The project's newest entry, read inside the caller's transaction so the
    /// repeat check in `record` sees every row a concurrent writer has landed.
    ///
    /// Hand-offs are skipped: an orchestrator writes `intent:` and then spawns
    /// or sends, and the next repaint of that same intent must still read as a
    /// repeat rather than land a duplicate after every hand-off.
    fn newest(
        tx: &rusqlite::Transaction<'_>,
        project: &str,
        pty_id: Option<&str>,
    ) -> Result<Option<ProgressEntry>, String> {
        let row = tx
            .query_row(
                "SELECT id, created_at_ms, kind, text, step, agent_name
                   FROM entries
                  WHERE project = ?1 AND pty_id IS ?2
                    AND kind NOT IN ('delegated', 'message')
                  ORDER BY id DESC
                  LIMIT 1",
                params![project, pty_id],
                |row| {
                    Ok((
                        row.get::<_, i64>(0)?,
                        row.get::<_, i64>(1)?,
                        row.get::<_, String>(2)?,
                        row.get::<_, String>(3)?,
                        row.get::<_, Option<String>>(4)?,
                        row.get::<_, Option<String>>(5)?,
                    ))
                },
            )
            .optional()
            .map_err(db_error("read the newest progress entry"))?;
        row.map(|(id, created_at_ms, kind, text, step, agent_name)| {
            Ok(ProgressEntry {
                id,
                project: project.to_string(),
                pty_id: pty_id.map(str::to_string),
                created_at_ms: u64_from_i64(created_at_ms),
                kind: ProgressKind::parse(&kind)?,
                text,
                step,
                agent_name,
                target_pty_id: None,
                target_name: None,
            })
        })
        .transpose()
    }

    pub fn list(&self, project: &str, input: &ProgressListInput) -> Result<ProgressList, String> {
        self.list_limited(
            project,
            input,
            input
                .limit
                .unwrap_or(DEFAULT_PAGE_LIMIT)
                .clamp(1, MAX_PAGE_LIMIT),
        )
    }

    /// Projects with journal entries, ordered by their most recent entry.
    pub fn recent_projects(&self) -> Result<Vec<String>, String> {
        let conn = self.connect()?;
        let mut statement = conn
            .prepare("SELECT project FROM entries GROUP BY project ORDER BY MAX(id) DESC")
            .map_err(db_error("prepare recent progress projects"))?;
        statement
            .query_map([], |row| row.get::<_, String>(0))
            .map_err(db_error("read recent progress projects"))?
            .collect::<Result<Vec<_>, _>>()
            .map_err(db_error("read a progress project"))
    }

    /// `list` with an explicit row cap. The Flow reads one row past
    /// `LIST_LIMIT` so it can tell a journal of exactly that many entries from
    /// one that was cut.
    pub(crate) fn list_limited(
        &self,
        project: &str,
        input: &ProgressListInput,
        limit: usize,
    ) -> Result<ProgressList, String> {
        let conn = self.connect()?;
        let blocked_only = input.blocked_only.unwrap_or(false);
        let known_kinds = ProgressKind::ALL
            .map(|kind| format!("'{}'", kind.as_str()))
            .join(",");
        let total: i64 = conn
            .query_row(
                &format!(
                    "SELECT COUNT(*) FROM entries
                  WHERE project = ?1 AND (?2 = 0 OR kind = 'blocked')
                    AND (?3 IS NULL OR pty_id = ?3)
                    AND kind IN ({known_kinds})"
                ),
                params![project, i64::from(blocked_only), input.pty_id],
                |row| row.get(0),
            )
            .map_err(db_error("count the progress list"))?;
        let total =
            usize::try_from(total).map_err(|error| format!("invalid progress count: {error}"))?;
        let mut statement = conn
            .prepare(&format!(
                "SELECT id, created_at_ms, kind, text, step, agent_name, pty_id,
                        target_pty_id, target_name
                   FROM entries
                  WHERE project = ?1 AND (?2 = 0 OR kind = 'blocked')
                    AND (?3 IS NULL OR pty_id = ?3)
                    AND kind IN ({known_kinds})
                    AND (?4 IS NULL OR id < ?4)
                  ORDER BY id DESC
                  LIMIT ?5"
            ))
            .map_err(db_error("prepare the progress list"))?;
        let entries = statement
            .query_map(
                params![
                    project,
                    i64::from(blocked_only),
                    input.pty_id,
                    input.cursor,
                    (limit + 1) as i64
                ],
                |row| {
                    Ok((
                        row.get::<_, i64>(0)?,
                        row.get::<_, i64>(1)?,
                        row.get::<_, String>(2)?,
                        row.get::<_, String>(3)?,
                        row.get::<_, Option<String>>(4)?,
                        row.get::<_, Option<String>>(5)?,
                        row.get::<_, Option<String>>(6)?,
                        row.get::<_, Option<String>>(7)?,
                        row.get::<_, Option<String>>(8)?,
                    ))
                },
            )
            .map_err(db_error("read the progress list"))?
            .collect::<Result<Vec<_>, _>>()
            .map_err(db_error("read a progress entry"))?;
        let mut entries = entries
            .into_iter()
            .map(
                |(
                    id,
                    created_at_ms,
                    kind,
                    text,
                    step,
                    agent_name,
                    pty_id,
                    target_pty_id,
                    target_name,
                )|
                 -> ProgressEntry {
                    let kind =
                        ProgressKind::parse(&kind).expect("SQL selected a known progress kind");
                    ProgressEntry {
                        id,
                        project: project.to_string(),
                        pty_id,
                        created_at_ms: u64_from_i64(created_at_ms),
                        kind,
                        text,
                        step,
                        agent_name,
                        target_pty_id,
                        target_name,
                    }
                },
            )
            .collect::<Vec<_>>();
        let has_more = entries.len() > limit;
        entries.truncate(limit);
        let next_cursor = has_more.then(|| entries.last().expect("nonempty page").id);
        let pty_ids = conn
            .prepare("SELECT pty_id FROM (SELECT pty_id FROM entries WHERE project = ?1 AND pty_id IS NOT NULL GROUP BY pty_id ORDER BY MAX(id) DESC LIMIT ?2) ORDER BY pty_id")
            .map_err(db_error("prepare the terminal list"))?
            .query_map(params![project, PTY_IDS_LIMIT], |row| row.get::<_, String>(0))
            .map_err(db_error("read the terminal list"))?
            .collect::<Result<Vec<_>, _>>()
            .map_err(db_error("read a terminal identity"))?;

        Ok(ProgressList {
            project: project.to_string(),
            entries,
            total,
            next_cursor,
            pty_ids,
            last_viewed_ms: self.last_viewed_ms(&conn, project, input.pty_id.as_deref())?,
        })
    }

    pub fn delete(&self, project: &str, ids: &[i64]) -> Result<ProgressDeleteReceipt, String> {
        if ids.is_empty() {
            return Ok(ProgressDeleteReceipt { deleted: 0 });
        }
        let mut conn = self.connect()?;
        let tx = conn
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .map_err(db_error("begin progress delete"))?;
        let mut deleted = 0usize;
        {
            // Scoped by project as well as by id: an id is unique in the shared
            // table, so without this one project could delete another's entry
            // by guessing a rowid.
            let mut statement = tx
                .prepare("DELETE FROM entries WHERE project = ?1 AND id = ?2")
                .map_err(db_error("prepare the progress delete"))?;
            for id in ids {
                deleted += statement
                    .execute(params![project, id])
                    .map_err(db_error("delete a progress entry"))?;
            }
        }
        tx.commit().map_err(db_error("commit progress delete"))?;
        Ok(ProgressDeleteReceipt { deleted })
    }

    pub fn mark_viewed_for_pty(
        &self,
        project: &str,
        pty_id: Option<&str>,
    ) -> Result<ProgressViewedReceipt, String> {
        let last_viewed_ms = now_ms();
        let conn = self.connect()?;
        if let Some(pty_id) = pty_id {
            conn.execute(
                "INSERT INTO pty_views (project, pty_id, last_viewed_ms) VALUES (?1, ?2, ?3)
                 ON CONFLICT(project, pty_id) DO UPDATE SET last_viewed_ms = excluded.last_viewed_ms",
                params![project, pty_id, i64_from_u64(last_viewed_ms)],
            )
            .map_err(db_error("record the terminal last-viewed time"))?;
        } else {
            conn.execute(
                "INSERT INTO project_views (project, last_viewed_ms) VALUES (?1, ?2)
                 ON CONFLICT(project) DO UPDATE SET last_viewed_ms = excluded.last_viewed_ms",
                params![project, i64_from_u64(last_viewed_ms)],
            )
            .map_err(db_error("record the progress last-viewed time"))?;
        }
        Ok(ProgressViewedReceipt { last_viewed_ms })
    }

    fn last_viewed_ms(
        &self,
        conn: &Connection,
        project: &str,
        pty_id: Option<&str>,
    ) -> Result<Option<u64>, String> {
        let value = if let Some(pty_id) = pty_id {
            conn.query_row(
                "SELECT last_viewed_ms FROM pty_views WHERE project = ?1 AND pty_id = ?2",
                params![project, pty_id],
                |row| row.get::<_, i64>(0),
            )
        } else {
            conn.query_row(
                "SELECT last_viewed_ms FROM project_views WHERE project = ?1",
                params![project],
                |row| row.get::<_, i64>(0),
            )
        };
        value
            .optional()
            .map_err(db_error("read the progress last-viewed time"))
            .map(|value| value.map(u64_from_i64))
    }
}

fn db_error(what: &'static str) -> impl Fn(rusqlite::Error) -> String {
    move |error| format!("progress_store_unavailable: cannot {what}: {error}")
}

fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|value| value.as_millis() as u64)
        .unwrap_or_default()
}

/// SQLite binds integers as `i64`. Every timestamp this store writes is a
/// millisecond epoch, so the saturating cast is unreachable in practice and
/// exists so a clock far in the future cannot wrap into a negative rowid-order
/// timestamp.
fn i64_from_u64(value: u64) -> i64 {
    i64::try_from(value).unwrap_or(i64::MAX)
}

fn u64_from_i64(value: i64) -> u64 {
    u64::try_from(value).unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::progress::model::ProgressKind;

    fn entry(kind: ProgressKind, text: &str) -> NewProgressEntry {
        NewProgressEntry {
            kind,
            text: text.to_string(),
            step: None,
            agent_name: None,
        }
    }

    /// The guard holds the process-wide config-directory lock, so it must be
    /// bound for the whole test — `let (_guard, …)`, never `let (_, …)`.
    fn isolated_store() -> (impl Drop, ProgressStore, tempfile::TempDir) {
        let dir = tempfile::tempdir().unwrap();
        let guard = crate::config::set_config_dir_override(dir.path().to_path_buf());
        let store = ProgressStore::open().unwrap();
        (guard, store, dir)
    }

    #[test]
    fn the_database_lives_in_the_config_directory_and_nowhere_else() {
        let (_guard, store, dir) = isolated_store();
        assert_eq!(store.database_path(), dir.path().join("progress.sqlite3"));
    }

    #[test]
    fn opening_an_existing_journal_adds_nullable_pty_identity_without_losing_history() {
        let dir = tempfile::tempdir().unwrap();
        let _guard = crate::config::set_config_dir_override(dir.path().to_path_buf());
        let conn = Connection::open(dir.path().join("progress.sqlite3")).unwrap();
        conn.execute_batch(
            "CREATE TABLE entries (
               id INTEGER PRIMARY KEY AUTOINCREMENT, project TEXT NOT NULL,
               created_at_ms INTEGER NOT NULL, kind TEXT NOT NULL,
               text TEXT NOT NULL, step TEXT, agent_name TEXT
             );
             INSERT INTO entries (project, created_at_ms, kind, text)
             VALUES ('/repo', 1, 'done', 'Older work');",
        )
        .unwrap();
        drop(conn);

        let store = ProgressStore::open().unwrap();
        let legacy = store.list("/repo", &ProgressListInput::default()).unwrap();
        assert_eq!(legacy.entries[0].text, "Older work");
        let conn = Connection::open(store.database_path()).unwrap();
        let pty_id: Option<String> = conn
            .query_row("SELECT pty_id FROM entries WHERE id = 1", [], |row| {
                row.get(0)
            })
            .unwrap();
        assert_eq!(pty_id, None);
    }

    /// A journal written before the hand-off kinds has a CHECK constraint that
    /// refuses them, and SQLite cannot alter a CHECK. Opening it must rebuild
    /// the table without losing a row, an id, or the id high-water mark — a
    /// deleted newest id must still never be handed out again.
    #[test]
    fn opening_a_pre_hand_off_journal_rebuilds_it_and_keeps_every_id() {
        let dir = tempfile::tempdir().unwrap();
        let _guard = crate::config::set_config_dir_override(dir.path().to_path_buf());
        let conn = Connection::open(dir.path().join("progress.sqlite3")).unwrap();
        conn.execute_batch(
            "CREATE TABLE entries (
               id INTEGER PRIMARY KEY AUTOINCREMENT, project TEXT NOT NULL,
               created_at_ms INTEGER NOT NULL,
               kind TEXT NOT NULL CHECK (kind IN ('done','blocked','intent')),
               text TEXT NOT NULL, step TEXT, agent_name TEXT, pty_id TEXT
             );
             INSERT INTO entries (project, created_at_ms, kind, text, pty_id)
             VALUES ('/repo', 1, 'done', 'kept', 'pty-a'),
                    ('/repo', 2, 'intent', 'deleted newest', 'pty-a');
             DELETE FROM entries WHERE id = 2;",
        )
        .unwrap();
        drop(conn);

        let store = ProgressStore::open().unwrap();
        let kept = store.list("/repo", &ProgressListInput::default()).unwrap();
        assert_eq!(kept.entries.len(), 1);
        assert_eq!(kept.entries[0].id, 1);
        assert_eq!(kept.entries[0].pty_id.as_deref(), Some("pty-a"));

        let hand_off = store
            .record_hand_off(
                "/repo",
                &entry(ProgressKind::Delegated, "Review the parser"),
                Some("pty-a"),
                Some("pty-b"),
                Some("reviewer"),
            )
            .unwrap();
        assert_eq!(hand_off.id, 3, "id 2 was deleted and must not come back");
        let listed = store.list("/repo", &ProgressListInput::default()).unwrap();
        assert_eq!(listed.entries[0].kind, ProgressKind::Delegated);
        assert_eq!(listed.entries[0].target_pty_id.as_deref(), Some("pty-b"));
        assert_eq!(listed.entries[0].target_name.as_deref(), Some("reviewer"));

        // Opening again finds the new schema and leaves it alone.
        let again = ProgressStore::open().unwrap();
        assert_eq!(
            again
                .list("/repo", &ProgressListInput::default())
                .unwrap()
                .entries
                .len(),
            2
        );
    }

    #[test]
    fn entries_come_back_newest_first_with_their_fields_intact() {
        let (_guard, store, _dir) = isolated_store();
        let first = store
            .record("/p", &entry(ProgressKind::Done, "parser shipped"))
            .unwrap();
        let second = store
            .record(
                "/p",
                &NewProgressEntry {
                    kind: ProgressKind::Blocked,
                    text: "needs an API key".to_string(),
                    step: Some("Step 3".to_string()),
                    agent_name: Some("claude".to_string()),
                },
            )
            .unwrap();
        assert!(second.id > first.id, "the rowid is the order");

        let listed = store.list("/p", &ProgressListInput::default()).unwrap();
        let ids: Vec<_> = listed.entries.iter().map(|e| e.id).collect();
        assert_eq!(ids, vec![second.id, first.id]);
        assert_eq!(listed.entries[0].step.as_deref(), Some("Step 3"));
        assert_eq!(listed.entries[0].agent_name.as_deref(), Some("claude"));
        assert_eq!(listed.entries[0].kind, ProgressKind::Blocked);
        assert_eq!(listed.last_viewed_ms, None);
    }

    /// A changed-row repaint hands the parser the same `intent:` line again and
    /// again, so a repeat of the newest intent is that intent, not a new row.
    /// An agent's own report is never collapsed: reporting the same step twice
    /// means the work was done twice, and the reader decides what that means.
    #[test]
    fn a_repeat_of_the_newest_intent_is_not_recorded_twice_but_a_report_is() {
        let (_guard, store, _dir) = isolated_store();
        let first = store
            .record("/p", &entry(ProgressKind::Intent, "fixing the tag"))
            .unwrap();
        let again = store
            .record("/p", &entry(ProgressKind::Intent, "fixing the tag"))
            .unwrap();
        assert_eq!(again.id, first.id, "a repeat returns the existing row");

        // The same text as a different kind is news, and so is the same text
        // once something else has happened in between.
        store
            .record("/p", &entry(ProgressKind::Done, "fixing the tag"))
            .unwrap();
        let later = store
            .record("/p", &entry(ProgressKind::Intent, "fixing the tag"))
            .unwrap();
        assert_ne!(later.id, first.id);

        // Another project is not "in between" for this one.
        store
            .record("/other", &entry(ProgressKind::Intent, "fixing the tag"))
            .unwrap();
        let list = store.list("/p", &ProgressListInput::default()).unwrap();
        assert_eq!(list.entries.len(), 3);

        // The same report twice in a row is two reports.
        let done = store
            .record("/p", &entry(ProgressKind::Done, "parser shipped"))
            .unwrap();
        let done_again = store
            .record("/p", &entry(ProgressKind::Done, "parser shipped"))
            .unwrap();
        assert_ne!(done_again.id, done.id, "agent reports are never collapsed");
    }

    #[test]
    fn the_blocked_filter_is_the_only_one() {
        let (_guard, store, _dir) = isolated_store();
        store
            .record("/p", &entry(ProgressKind::Done, "done"))
            .unwrap();
        store
            .record("/p", &entry(ProgressKind::Intent, "setting out"))
            .unwrap();
        let blocked = store
            .record("/p", &entry(ProgressKind::Blocked, "stuck"))
            .unwrap();

        let filtered = store
            .list(
                "/p",
                &ProgressListInput {
                    blocked_only: Some(true),
                    pty_id: None,
                    ..Default::default()
                },
            )
            .unwrap();
        assert_eq!(
            filtered.entries.iter().map(|e| e.id).collect::<Vec<_>>(),
            vec![blocked.id]
        );
        assert_eq!(
            store
                .list("/p", &ProgressListInput::default())
                .unwrap()
                .entries
                .len(),
            3
        );
    }

    #[test]
    fn terminal_filter_separates_parallel_ptys_and_keeps_legacy_entries_in_the_project() {
        let (_guard, store, _dir) = isolated_store();
        let old = store
            .record("/p", &entry(ProgressKind::Done, "older work"))
            .unwrap();
        let first = store
            .record_for_pty(
                "/p",
                &entry(ProgressKind::Intent, "same task"),
                Some("pty-a"),
            )
            .unwrap();
        let second = store
            .record_for_pty(
                "/p",
                &entry(ProgressKind::Intent, "same task"),
                Some("pty-b"),
            )
            .unwrap();
        assert_ne!(
            first.id, second.id,
            "one PTY must not deduplicate another's intent"
        );
        assert_eq!(first.pty_id.as_deref(), Some("pty-a"));

        let only_a = store
            .list(
                "/p",
                &ProgressListInput {
                    pty_id: Some("pty-a".to_string()),
                    ..Default::default()
                },
            )
            .unwrap();
        assert_eq!(
            only_a.entries.iter().map(|row| row.id).collect::<Vec<_>>(),
            vec![first.id]
        );
        assert_eq!(only_a.pty_ids, vec!["pty-a", "pty-b"]);

        let aggregate = store.list("/p", &Default::default()).unwrap();
        assert_eq!(
            aggregate
                .entries
                .iter()
                .map(|row| row.id)
                .collect::<Vec<_>>(),
            vec![second.id, first.id, old.id]
        );
        assert_eq!(aggregate.entries[2].pty_id, None);
        assert_eq!(
            store.list("/other", &Default::default()).unwrap().pty_ids,
            [] as [std::string::String; 0]
        );
    }

    #[test]
    fn progress_list_pty_ids_stay_bounded_with_many_ptys() {
        let (_guard, store, _dir) = isolated_store();
        // This tests list bounds, not per-entry durability. Seed the real database
        // in one transaction instead of reopening, syncing and checkpointing it
        // for each of the 1,704 terminals on the shared build machine.
        let mut conn = store.connect().unwrap();
        let tx = conn.transaction().unwrap();
        {
            let mut insert = tx
                .prepare(
                    "INSERT INTO entries (project, created_at_ms, kind, text, pty_id)
                     VALUES ('/p', 1, 'done', ?1, ?2)",
                )
                .unwrap();
            for index in 0..1704 {
                insert
                    .execute(params![
                        format!("event {index}"),
                        format!("{index:08x}-0000-4000-8000-000000000000"),
                    ])
                    .unwrap();
            }
        }
        tx.commit().unwrap();
        drop(conn);
        let page = store.list("/p", &ProgressListInput::default()).unwrap();
        assert_eq!(page.total, 1704);
        assert_eq!(page.pty_ids.len(), PTY_IDS_LIMIT as usize);
        let wire = serde_json::to_string(&page).unwrap();
        assert!(
            wire.len() < 16384,
            "the default page serialized to {} B",
            wire.len()
        );
        // The filter keeps the newest terminals with history: the last one written is listed.
        assert!(
            page.pty_ids
                .contains(&format!("{:08x}-0000-4000-8000-000000000000", 1703))
        );
        // The oldest terminal's entries stay reachable through the aggregate list; only its filter slot is dropped.
        assert!(
            !page
                .pty_ids
                .contains(&"00000000-0000-4000-8000-000000000000".to_string())
        );
    }

    #[test]
    fn progress_default_page_bounds_history_and_reports_remaining_entries() {
        let (_guard, store, _dir) = isolated_store();
        for index in 0..75 {
            store
                .record("/p", &entry(ProgressKind::Done, &format!("event {index}")))
                .unwrap();
        }
        let page = store.list("/p", &ProgressListInput::default()).unwrap();
        let wire = serde_json::to_value(&page).unwrap();
        assert_eq!(wire["total"], 75);
        assert_eq!(page.entries.len(), 8, "default response must be bounded");
        assert!(
            wire["nextCursor"].is_number(),
            "remaining history needs a cursor"
        );
        assert_eq!(page.entries.first().unwrap().text, "event 74");
    }

    #[test]
    fn progress_cursor_reaches_each_entry_once_in_newest_first_order() {
        let (_guard, store, _dir) = isolated_store();
        for index in 0..7 {
            store
                .record("/p", &entry(ProgressKind::Done, &format!("event {index}")))
                .unwrap();
        }
        let mut cursor: Option<i64> = None;
        let mut texts = Vec::new();
        loop {
            let mut input = serde_json::json!({"limit": 3});
            if let Some(value) = cursor {
                input["cursor"] = serde_json::json!(value);
            }
            let page = store
                .list("/p", &serde_json::from_value(input).unwrap())
                .unwrap();
            let wire = serde_json::to_value(&page).unwrap();
            assert_eq!(wire["total"], 7);
            texts.extend(page.entries.into_iter().map(|entry| entry.text));
            cursor = wire["nextCursor"].as_i64();
            if cursor.is_none() {
                break;
            }
        }
        assert_eq!(
            texts,
            (0..7)
                .rev()
                .map(|i| format!("event {i}"))
                .collect::<Vec<_>>()
        );
    }

    #[test]
    fn progress_filters_before_paging_and_clamps_oversized_limit() {
        let (_guard, store, _dir) = isolated_store();
        for index in 0..12 {
            let kind = if index % 2 == 0 {
                ProgressKind::Blocked
            } else {
                ProgressKind::Done
            };
            let pty = if index % 3 == 0 { "a" } else { "b" };
            store
                .record_for_pty("/p", &entry(kind, &format!("event {index}")), Some(pty))
                .unwrap();
        }
        let input = serde_json::from_value(
            serde_json::json!({"blockedOnly": true, "ptyId": "b", "limit": 2}),
        )
        .unwrap();
        let page = store.list("/p", &input).unwrap();
        let wire = serde_json::to_value(&page).unwrap();
        assert_eq!(wire["total"], 4);
        assert_eq!(
            page.entries
                .iter()
                .map(|entry| entry.text.as_str())
                .collect::<Vec<_>>(),
            vec!["event 10", "event 8"]
        );
        assert!(wire["nextCursor"].is_number());

        for index in 12..122 {
            store
                .record("/p", &entry(ProgressKind::Done, &format!("event {index}")))
                .unwrap();
        }
        let input = serde_json::from_value(serde_json::json!({"limit": 1000000})).unwrap();
        let page = store.list("/p", &input).unwrap();
        assert_eq!(serde_json::to_value(&page).unwrap()["total"], 122);
        assert_eq!(page.entries.len(), 100);
        assert!(serde_json::to_value(&page).unwrap()["nextCursor"].is_number());

        let input = serde_json::from_value(serde_json::json!({"limit": 0})).unwrap();
        assert_eq!(store.list("/p", &input).unwrap().entries.len(), 1);
    }

    #[test]
    fn viewing_one_pty_does_not_mark_another_pty_or_the_repo_aggregate_as_seen() {
        let (_guard, store, _dir) = isolated_store();
        store
            .record_for_pty("/p", &entry(ProgressKind::Done, "A"), Some("pty-a"))
            .unwrap();
        store
            .record_for_pty("/p", &entry(ProgressKind::Done, "B"), Some("pty-b"))
            .unwrap();

        store.mark_viewed_for_pty("/p", Some("pty-a")).unwrap();
        let select = |pty: Option<&str>| {
            store
                .list(
                    "/p",
                    &ProgressListInput {
                        pty_id: pty.map(str::to_string),
                        ..Default::default()
                    },
                )
                .unwrap()
                .last_viewed_ms
        };
        assert!(select(Some("pty-a")).is_some());
        assert_eq!(select(Some("pty-b")), None);
        assert_eq!(select(None), None);

        store.mark_viewed_for_pty("/p", None).unwrap();
        assert!(select(None).is_some());
        assert_eq!(select(Some("pty-b")), None);
    }

    /// Two projects share one database. Nothing may leak across the column,
    /// and that includes a delete aimed at another project's rowid.
    #[test]
    fn projects_are_isolated_inside_the_one_database() {
        let (_guard, store, _dir) = isolated_store();
        let mine = store
            .record("/mine", &entry(ProgressKind::Done, "mine"))
            .unwrap();
        let theirs = store
            .record("/theirs", &entry(ProgressKind::Done, "theirs"))
            .unwrap();

        let listed = store.list("/mine", &ProgressListInput::default()).unwrap();
        assert_eq!(
            listed.entries.iter().map(|e| e.id).collect::<Vec<_>>(),
            vec![mine.id]
        );

        assert_eq!(store.delete("/mine", &[theirs.id]).unwrap().deleted, 0);
        assert_eq!(
            store
                .list("/theirs", &ProgressListInput::default())
                .unwrap()
                .entries
                .len(),
            1,
            "another project's delete must not reach this entry"
        );

        store.mark_viewed_for_pty("/mine", None).unwrap();
        assert!(
            store
                .list("/theirs", &ProgressListInput::default())
                .unwrap()
                .last_viewed_ms
                .is_none(),
            "last-viewed is per project"
        );
    }

    #[test]
    fn delete_removes_only_the_named_ids_and_reports_the_count() {
        let (_guard, store, _dir) = isolated_store();
        let first = store
            .record("/p", &entry(ProgressKind::Done, "one"))
            .unwrap();
        let second = store
            .record("/p", &entry(ProgressKind::Done, "two"))
            .unwrap();

        assert_eq!(store.delete("/p", &[]).unwrap().deleted, 0);
        // A missing id is not an error: two clients deleting the same entry is
        // an ordinary race, not a failure the user can act on.
        assert_eq!(
            store
                .delete("/p", &[first.id, first.id + 9_999])
                .unwrap()
                .deleted,
            1
        );
        assert_eq!(
            store
                .list("/p", &ProgressListInput::default())
                .unwrap()
                .entries
                .iter()
                .map(|e| e.id)
                .collect::<Vec<_>>(),
            vec![second.id]
        );
    }

    /// The rowid never repeats after a delete, which is what lets the dialog
    /// use it as identity for a delete button and as the sort key at once.
    #[test]
    fn a_rowid_is_not_reused_after_a_delete() {
        let (_guard, store, _dir) = isolated_store();
        let first = store
            .record("/p", &entry(ProgressKind::Done, "one"))
            .unwrap();
        store.delete("/p", &[first.id]).unwrap();
        let second = store
            .record("/p", &entry(ProgressKind::Done, "two"))
            .unwrap();
        assert!(second.id > first.id);
    }

    #[test]
    fn marking_viewed_moves_the_divider_forward() {
        let (_guard, store, _dir) = isolated_store();
        store
            .record("/p", &entry(ProgressKind::Done, "one"))
            .unwrap();
        let first = store
            .mark_viewed_for_pty("/p", None)
            .unwrap()
            .last_viewed_ms;
        assert_eq!(
            store
                .list("/p", &ProgressListInput::default())
                .unwrap()
                .last_viewed_ms,
            Some(first)
        );
        std::thread::sleep(Duration::from_millis(2));
        let second = store
            .mark_viewed_for_pty("/p", None)
            .unwrap()
            .last_viewed_ms;
        assert!(second >= first, "the timestamp is not allowed to go back");
    }

    #[test]
    fn an_invalid_entry_is_refused_before_it_reaches_the_table() {
        let (_guard, store, _dir) = isolated_store();
        assert!(
            store
                .record("/p", &entry(ProgressKind::Done, "   "))
                .unwrap_err()
                .contains("text must not be empty")
        );
        assert_eq!(
            store
                .list("/p", &ProgressListInput::default())
                .unwrap()
                .entries
                .len(),
            0
        );
    }

    /// Several processes write this file. A second connection opened while the
    /// first is mid-transaction must wait on the busy timeout and then commit,
    /// not fail — and two entries written in the same millisecond must both
    /// survive with distinct ids.
    #[test]
    fn concurrent_writers_both_land() {
        let (_guard, store, _dir) = isolated_store();
        let other = ProgressStore {
            db_path: store.database_path().to_path_buf(),
        };
        let a = store
            .record("/p", &entry(ProgressKind::Done, "from a"))
            .unwrap();
        let b = other
            .record("/p", &entry(ProgressKind::Done, "from b"))
            .unwrap();
        assert_ne!(a.id, b.id);
        assert_eq!(
            store
                .list("/p", &ProgressListInput::default())
                .unwrap()
                .entries
                .len(),
            2
        );
    }

    /// Restarting TUIC does not delete the journal: a fresh store over the same
    /// directory reads what the previous one wrote.
    #[test]
    fn the_journal_survives_a_restart() {
        let dir = tempfile::tempdir().unwrap();
        let _guard = crate::config::set_config_dir_override(dir.path().to_path_buf());
        ProgressStore::open()
            .unwrap()
            .record("/p", &entry(ProgressKind::Done, "before the restart"))
            .unwrap();
        let listed = ProgressStore::open()
            .unwrap()
            .list("/p", &ProgressListInput::default())
            .unwrap();
        assert_eq!(listed.entries.len(), 1);
        assert_eq!(listed.entries[0].text, "before the restart");
    }

    #[test]
    fn an_unopenable_store_reports_a_concrete_error() {
        let dir = tempfile::tempdir().unwrap();
        // A directory where the database file belongs: SQLite cannot open it,
        // and the caller must be told which path failed rather than getting an
        // empty list that reads as "nothing happened yet".
        std::fs::create_dir(dir.path().join(STORE_FILE)).unwrap();
        let _guard = crate::config::set_config_dir_override(dir.path().to_path_buf());
        let error = ProgressStore::open().unwrap_err();
        assert!(
            error.starts_with("progress_store_unavailable:"),
            "unexpected error: {error}"
        );
        assert!(error.contains(STORE_FILE), "unexpected error: {error}");
    }

    /// An orchestrator writes `intent:` and then spawns or sends. The next
    /// repaint of that same intent is still a repeat: a hand-off row between
    /// them must not turn every repaint after a hand-off into a new intent.
    #[test]
    fn a_hand_off_does_not_break_a_run_of_the_same_intent() {
        let (_guard, store, _dir) = isolated_store();
        let intent = entry(ProgressKind::Intent, "splitting the parser");
        let first = store.record_for_pty("/p", &intent, Some("lead")).unwrap();
        for kind in [ProgressKind::Delegated, ProgressKind::Message] {
            store
                .record_hand_off(
                    "/p",
                    &entry(kind, "do the lexer"),
                    Some("lead"),
                    Some("w1"),
                    None,
                )
                .unwrap();
            let again = store.record_for_pty("/p", &intent, Some("lead")).unwrap();
            assert_eq!(
                again.id, first.id,
                "after a {kind:?} the intent is a repeat"
            );
        }
        let list = store.list("/p", &ProgressListInput::default()).unwrap();
        let intents = list
            .entries
            .iter()
            .filter(|e| e.kind == ProgressKind::Intent)
            .count();
        assert_eq!(intents, 1);
    }

    /// Debug and release builds share this journal. A kind written by a newer
    /// build must cost that one row in an older reader, never the whole list.
    #[test]
    fn a_kind_this_build_does_not_know_is_skipped_not_fatal() {
        let (_guard, store, _dir) = isolated_store();
        store
            .record("/p", &entry(ProgressKind::Done, "known"))
            .unwrap();
        let conn = Connection::open(store.database_path()).unwrap();
        conn.execute_batch(
            "PRAGMA ignore_check_constraints = ON;
             INSERT INTO entries (project, created_at_ms, kind, text)
             VALUES ('/p', 2, 'from_the_future', 'unknown');",
        )
        .unwrap();
        drop(conn);

        let list = store.list("/p", &ProgressListInput::default()).unwrap();
        let texts: Vec<&str> = list.entries.iter().map(|e| e.text.as_str()).collect();
        assert_eq!(texts, vec!["known"]);
    }

    #[test]
    fn progress_paging_skips_future_kinds_without_losing_known_history() {
        let (_guard, store, _dir) = isolated_store();
        for index in 0..12 {
            store
                .record("/p", &entry(ProgressKind::Done, &format!("known {index}")))
                .unwrap();
        }
        let conn = Connection::open(store.database_path()).unwrap();
        conn.execute_batch(
            "PRAGMA ignore_check_constraints = ON;
             INSERT INTO entries (project, created_at_ms, kind, text)
             VALUES ('/p', 20, 'from_the_future', 'unknown one'),
                    ('/p', 21, 'from_the_future', 'unknown two');",
        )
        .unwrap();
        drop(conn);

        let first = store.list("/p", &ProgressListInput::default()).unwrap();
        assert_eq!(first.total, 12);
        assert_eq!(first.entries.len(), 8);
        let older = store
            .list(
                "/p",
                &ProgressListInput {
                    cursor: first.next_cursor,
                    ..Default::default()
                },
            )
            .unwrap();
        assert_eq!(
            older
                .entries
                .iter()
                .map(|entry| entry.text.as_str())
                .collect::<Vec<_>>(),
            vec!["known 3", "known 2", "known 1", "known 0"]
        );
        assert_eq!(older.next_cursor, None);
    }

    /// The first journals were created with a bare `INTEGER PRIMARY KEY` (the
    /// schema parked in b3f470df), so the file has no `sqlite_sequence` table
    /// at all. The hand-off rebuild read that table unconditionally and failed
    /// with `no such table: sqlite_sequence`, which made every progress read
    /// and write fail. The rebuild must open such a journal, keep every row
    /// and id, and continue numbering after the highest id.
    #[test]
    fn opening_a_journal_without_autoincrement_rebuilds_it_and_keeps_every_id() {
        let dir = tempfile::tempdir().unwrap();
        let _guard = crate::config::set_config_dir_override(dir.path().to_path_buf());
        let conn = Connection::open(dir.path().join("progress.sqlite3")).unwrap();
        conn.execute_batch(
            "CREATE TABLE entries (
               id            INTEGER PRIMARY KEY,
               project       TEXT NOT NULL,
               created_at_ms INTEGER NOT NULL,
               kind          TEXT NOT NULL CHECK (kind IN ('done','blocked','intent')),
               text          TEXT NOT NULL,
               step          TEXT,
               agent_name    TEXT
             );
             CREATE INDEX entries_by_project ON entries (project, id DESC);
             CREATE TABLE project_views (
               project        TEXT PRIMARY KEY,
               last_viewed_ms INTEGER NOT NULL DEFAULT 0
             );
             ALTER TABLE entries ADD COLUMN pty_id TEXT;
             INSERT INTO entries (id, project, created_at_ms, kind, text, pty_id)
             VALUES (1, '/repo', 1, 'done', 'first', 'pty-a'),
                    (7, '/repo', 2, 'intent', 'seventh', 'pty-a');",
        )
        .unwrap();
        let has_sequence: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM sqlite_master WHERE name = 'sqlite_sequence'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(has_sequence, 0, "the legacy journal has no sqlite_sequence");
        drop(conn);

        let store = ProgressStore::open().unwrap();
        let kept = store.list("/repo", &ProgressListInput::default()).unwrap();
        let ids: Vec<i64> = kept.entries.iter().map(|e| e.id).collect();
        assert_eq!(ids, vec![7, 1]);

        let next = store
            .record_hand_off(
                "/repo",
                &entry(ProgressKind::Delegated, "Review the parser"),
                Some("pty-a"),
                Some("pty-b"),
                None,
            )
            .unwrap();
        assert_eq!(next.id, 8, "numbering continues after the highest id");
    }
}
