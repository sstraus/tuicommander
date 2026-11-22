//! The one cleanup job: deletes closed or dead records past their retention
//! period, at startup and once a day. Open records are never touched. Rules
//! and their reasons: `docs/backend/persistence.md`.

use crate::config::RetentionConfig;
use rusqlite::{Connection, params};
use std::collections::HashSet;
use std::path::Path;
use std::sync::Arc;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

pub(super) mod files;
mod instances;

const DAY_MS: i64 = 86_400_000;
const INTERVAL: Duration = Duration::from_secs(24 * 60 * 60);

pub(crate) fn now_ms() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis()
        .min(i64::MAX as u128) as i64
}

/// What the file rules must not delete: the identities of live and suspended
/// sessions, and the bridge revision agents are pointed at.
#[derive(Default)]
pub(crate) struct InUse {
    /// `None` on the startup run: restored tabs have not respawned yet, so
    /// session files are only swept by the daily runs.
    pub sessions: Option<HashSet<String>>,
    pub bridge_revision: Option<String>,
}

/// Hold this instance's liveness lock, then run the cleanup now and every day.
pub(crate) fn start(state: &Arc<crate::AppState>) {
    let dir = state.data_dir.clone();
    instances::hold_instance_lock(&dir);
    let state = Arc::downgrade(state);
    let spawned = std::thread::Builder::new()
        .name("db-retention".into())
        .spawn(move || {
            let mut startup = true;
            loop {
                let Some(state) = state.upgrade() else { return };
                let in_use = InUse {
                    sessions: if startup { None } else { live_sessions(&state) },
                    bridge_revision: crate::agent_mcp::current_bridge_revision(),
                };
                drop(state);
                let config = crate::config::load_app_config().retention;
                run(&dir, &config, now_ms(), &in_use);
                startup = false;
                std::thread::sleep(INTERVAL);
            }
        });
    if let Err(error) = spawned {
        tracing::error!(source = "db", %error, "Cannot start the database cleanup");
    }
}

/// File names (without `.json`) of every live or suspended session, both as
/// PTY key and as identity.
fn live_sessions(state: &crate::AppState) -> Option<HashSet<String>> {
    let mut ids: HashSet<String> = state
        .agent_capabilities
        .iter()
        .map(|entry| entry.key().clone())
        .chain(
            state
                .session_maps
                .sessions
                .iter()
                .map(|entry| entry.key().clone()),
        )
        .chain(
            state
                .session_maps
                .live_pty_by_tuic_session
                .iter()
                .map(|entry| entry.key().clone()),
        )
        .collect();
    match crate::suspended_sessions::list() {
        Ok(records) => {
            for (identity, record) in records {
                ids.extend([identity, record.tuic_session, record.session_id]);
            }
        }
        // Without the list nothing is known to be dead: skip the session files.
        Err(error) => {
            tracing::warn!(source = "db", %error, "Cleanup cannot read suspended sessions");
            return None;
        }
    }
    let stems: Vec<String> = ids
        .iter()
        .map(|id| crate::agent_capabilities::file_stem(id))
        .collect();
    ids.extend(stems);
    Some(ids)
}

/// One rule: rows of `store` deleted for records closed before `cutoff_ms`.
type Rule = fn(&Path, i64) -> Result<usize, String>;

fn cutoff(now_ms: i64, days: u32) -> i64 {
    now_ms.saturating_sub(i64::from(days) * DAY_MS)
}

/// Run every rule once. Returns `(store, removed)` and logs one line per store.
pub(crate) fn run(
    dir: &Path,
    config: &RetentionConfig,
    now_ms: i64,
    in_use: &InUse,
) -> Vec<(&'static str, usize)> {
    let rules: [(&'static str, &'static str, u32, Rule); 6] = [
        (
            "progress",
            super::PROGRESS_DB,
            config.progress_days,
            progress,
        ),
        (
            "automation_runs",
            super::AUTOMATION_RUNS_DB,
            config.automation_run_days,
            automation_runs,
        ),
        ("mail", super::MAIL_DB, config.mail_days, mail),
        (
            "tunnel_audit",
            super::TUNNEL_AUDIT_DB,
            config.tunnel_audit_days,
            tunnel_audit,
        ),
        (
            "mcp_usage",
            super::TUIC_DB,
            config.mcp_usage_days,
            mcp_usage,
        ),
        ("launches", super::TUIC_DB, config.launch_days, launches),
    ];
    let mut removed = Vec::new();
    for (store, file, days, rule) in rules {
        let path = dir.join(file);
        if days == 0 || !path.exists() {
            continue;
        }
        match rule(&path, cutoff(now_ms, days))
            .and_then(|count| compact(&path, count).map(|()| count))
        {
            Ok(count) => removed.push((store, count)),
            Err(error) => tracing::warn!(source = "db", store, %error, "Cleanup failed"),
        }
    }
    let mut file_rule = |store: &'static str, days: u32, rule: &dyn Fn(i64) -> usize| {
        if days > 0 {
            removed.push((store, rule(cutoff(now_ms, days))));
        }
    };
    file_rule("instances", config.instance_days, &|cut| {
        instances::remove_stale_instances(dir, cut)
    });
    file_rule("captures", config.capture_days, &|cut| {
        files::captures(dir, cut)
    });
    file_rule("mcp_bridge", config.bridge_revision_days, &|cut| {
        files::bridge_revisions(dir, cut, in_use.bridge_revision.as_deref())
    });
    if let Some(live) = &in_use.sessions {
        for (store, sub) in [
            ("agent_capabilities", files::AGENT_CAPABILITIES),
            ("bg_wakes", files::BG_WAKES),
        ] {
            file_rule(store, config.session_file_days, &|cut| {
                files::session_files(dir, sub, cut, live)
            });
        }
    }
    for (store, count) in &removed {
        tracing::info!(
            source = "db",
            store,
            removed = count,
            "Cleanup removed {count} from {store}"
        );
    }
    removed
}

/// Give freed pages back to the file system after a delete.
fn compact(path: &Path, removed: usize) -> Result<(), String> {
    if removed == 0 {
        return Ok(());
    }
    let conn = super::open(path)?;
    conn.execute_batch("PRAGMA incremental_vacuum;")
        .map_err(|e| format!("incremental vacuum: {e}"))?;
    conn.query_row("PRAGMA wal_checkpoint(TRUNCATE)", [], |_| Ok(()))
        .map_err(|e| format!("checkpoint: {e}"))
}

fn sql(e: rusqlite::Error) -> String {
    e.to_string()
}

fn in_transaction(
    path: &Path,
    work: impl FnOnce(&Connection) -> rusqlite::Result<usize>,
) -> Result<usize, String> {
    let mut conn = super::open(path)?;
    let tx = conn.transaction().map_err(sql)?;
    let count = work(&tx).map_err(sql)?;
    tx.commit().map_err(sql)?;
    Ok(count)
}

/// Entries older than the cutoff, except a blocked entry that is still the
/// newest of its terminal: that question is open. Replies, contexts and views
/// of deleted entries go with them.
/// A question whose answer is still pending is open, like the newest `blocked`.
/// Opened with the journal schema first: the cleanup can run before the store
/// migrated a journal older than `discussion_turns`.
fn progress(path: &Path, cutoff: i64) -> Result<usize, String> {
    drop(super::open_with(path, &crate::progress::JOURNAL_SCHEMA)?);
    in_transaction(path, |tx| {
        let entries = tx.execute(
            "DELETE FROM entries WHERE created_at_ms < ?1
               AND NOT (kind = 'blocked' AND NOT EXISTS (
                 SELECT 1 FROM entries later WHERE later.project = entries.project
                   AND later.pty_id IS entries.pty_id AND later.id > entries.id))
               AND NOT (kind = 'question' AND EXISTS (
                 SELECT 1 FROM discussion_turns t
                  WHERE t.question_entry_id = entries.id AND t.finished_ms IS NULL))",
            [cutoff],
        )?;
        let orphans = tx.execute(
            "DELETE FROM reply_context WHERE entry_id NOT IN (SELECT id FROM entries)",
            [],
        )? + tx.execute(
            "DELETE FROM discussion_turns WHERE question_entry_id NOT IN (SELECT id FROM entries)",
            [],
        )? + tx.execute(
            "DELETE FROM progress_replies WHERE entry_id NOT IN (SELECT id FROM entries)",
            [],
        )?;
        let views = tx.execute("DELETE FROM pty_views WHERE last_viewed_ms < ?1", [cutoff])?
            + tx.execute(
                "DELETE FROM project_views WHERE last_viewed_ms < ?1
                   AND project NOT IN (SELECT project FROM entries)",
                [cutoff],
            )?;
        Ok(entries + orphans + views)
    })
}

/// Finished runs; their deliveries cascade. Schedule cursors stay, so a
/// consumed occurrence never runs again.
fn automation_runs(path: &Path, cutoff: i64) -> Result<usize, String> {
    let store = crate::automations::store::RunStore::open_at(path)?;
    let days = 1;
    let now = cutoff.saturating_add(DAY_MS);
    let mut total = 0;
    loop {
        let count = store.prune(now, days)?;
        total += count;
        if count == 0 {
            return Ok(total);
        }
    }
}

/// Every row of a dead recipient: no mail since the cutoff and nothing left
/// unacknowledged by any of its subscriptions.
fn mail(path: &Path, cutoff: i64) -> Result<usize, String> {
    in_transaction(path, |tx| {
        let dead: Vec<String> = tx
            .prepare(
                "SELECT h.recipient FROM mail_heads h
                 WHERE COALESCE((SELECT MAX(json_extract(e.body, '$.timestamp'))
                                 FROM mail_events e WHERE e.recipient = h.recipient), 0) < ?1
                   AND NOT EXISTS (SELECT 1 FROM mail_subscriptions s
                                   WHERE s.recipient = h.recipient AND s.ack < h.head)",
            )?
            .query_map([cutoff], |row| row.get(0))?
            .collect::<rusqlite::Result<_>>()?;
        let mut count = 0;
        for recipient in &dead {
            for table in ["mail_events", "mail_subscriptions", "mail_heads"] {
                count += tx.execute(
                    &format!("DELETE FROM {table} WHERE recipient = ?1"),
                    params![recipient],
                )?;
            }
        }
        Ok(count)
    })
}

fn tunnel_audit(path: &Path, cutoff: i64) -> Result<usize, String> {
    let cutoff = chrono::DateTime::from_timestamp_millis(cutoff)
        .unwrap_or_default()
        .format("%Y-%m-%dT%H:%M:%S%.3fZ")
        .to_string();
    in_transaction(path, |tx| {
        tx.execute("DELETE FROM tunnel_events WHERE timestamp < ?1", [cutoff])
    })
}

fn mcp_usage(path: &Path, cutoff: i64) -> Result<usize, String> {
    let day = crate::mcp_http::mcp_usage_rollup::day_of(cutoff.max(0) as u64);
    in_transaction(path, |tx| {
        tx.execute("DELETE FROM mcp_usage_daily WHERE day < ?1", [day])
    })
}

/// A launch is closed once it reached `created` or `blocked`.
fn launches(path: &Path, cutoff: i64) -> Result<usize, String> {
    in_transaction(path, |tx| {
        tx.execute(
            "DELETE FROM launches WHERE updated_ms < ?1
               AND json_extract(record, '$.stage') IN ('created', 'blocked')",
            [cutoff],
        )
    })
}

#[cfg(test)]
mod files_tests;
#[cfg(test)]
mod tests;
