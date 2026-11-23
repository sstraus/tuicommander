//! Each rule deletes an expired closed record, keeps an expired open one and a
//! fresh closed one, and a second run removes nothing.

use super::*;
use rusqlite::params;

const NOW: i64 = 400 * DAY_MS;
const CUTOFF: i64 = NOW - 30 * DAY_MS;
const OLD: i64 = CUTOFF - 1;

fn temp_dir() -> tempfile::TempDir {
    tempfile::tempdir_in(crate::test_support::test_temp_root()).unwrap()
}

fn ids(path: &Path, sql: &str) -> Vec<String> {
    let conn = crate::db::open(path).unwrap();
    let mut stmt = conn.prepare(sql).unwrap();
    stmt.query_map([], |row| row.get::<_, String>(0))
        .unwrap()
        .collect::<rusqlite::Result<_>>()
        .unwrap()
}

/// Apply `rule` twice: the second pass must find nothing left to delete.
fn apply_twice(rule: Rule, path: &Path) -> usize {
    let removed = rule(path, CUTOFF).unwrap();
    assert_eq!(rule(path, CUTOFF).unwrap(), 0, "the rule is not idempotent");
    removed
}

// Catches: deleting the open question of a terminal, keeping answered history
// forever, or leaving reply rows of deleted entries behind.
#[test]
fn progress_keeps_open_questions_and_fresh_entries() {
    let dir = temp_dir();
    let _guard = crate::config::set_config_dir_override(dir.path().to_path_buf());
    crate::progress::ProgressStore::open().unwrap();
    let path = dir.path().join(crate::db::PROGRESS_DB);
    {
        let conn = crate::db::open(&path).unwrap();
        for (id, created, kind, pty) in [
            (1, OLD, "blocked", "open"),
            (2, OLD, "done", "closed"),
            (3, OLD, "blocked", "answered"),
            (4, NOW, "intent", "answered"),
            (5, NOW, "done", "fresh"),
        ] {
            conn.execute(
                "INSERT INTO entries (id, project, created_at_ms, kind, text, pty_id)
                 VALUES (?1, '/p', ?2, ?3, 'x', ?4)",
                params![id, created, kind, pty],
            )
            .unwrap();
        }
        conn.execute_batch(
            "INSERT INTO reply_context VALUES (3, NULL, 1, 1);
             INSERT INTO progress_replies VALUES ('/p', 'r', 3, '{}', 'sent');",
        )
        .unwrap();
    }
    assert_eq!(apply_twice(progress, &path), 4);
    assert_eq!(
        ids(&path, "SELECT CAST(id AS TEXT) FROM entries ORDER BY id"),
        ["1", "4", "5"]
    );
    assert_eq!(
        ids(&path, "SELECT CAST(entry_id AS TEXT) FROM reply_context"),
        Vec::<String>::new()
    );
}

// Catches: pruning an open run, or a finished run inside the retention window.
#[test]
fn automation_runs_delete_only_finished_runs_past_the_window() {
    let dir = temp_dir();
    let path = dir.path().join(crate::db::AUTOMATION_RUNS_DB);
    crate::automations::store::RunStore::open_at(&path).unwrap();
    {
        let conn = crate::db::open(&path).unwrap();
        for (id, status, finished) in [
            ("old-final", "completed", Some(OLD)),
            ("old-open", "running", None),
            ("fresh-final", "failed", Some(NOW)),
        ] {
            conn.execute(
                "INSERT INTO automation_runs VALUES (?1, 'a', NULL, ?2, 0, ?3, '{}')",
                params![id, status, finished],
            )
            .unwrap();
        }
        conn.execute("INSERT INTO automation_cursors VALUES ('a', 7)", [])
            .unwrap();
    }
    assert_eq!(apply_twice(automation_runs, &path), 1);
    assert_eq!(
        ids(&path, "SELECT id FROM automation_runs ORDER BY id"),
        ["fresh-final", "old-open"]
    );
    assert_eq!(
        ids(&path, "SELECT automation_id FROM automation_cursors"),
        ["a"]
    );
}

// Catches: deleting mail a subscriber has not acknowledged yet, or a live peer's
// recent mail.
#[test]
fn mail_deletes_only_dead_recipients() {
    let dir = temp_dir();
    let subscriptions = crate::mail_subscriptions::MailSubscriptions::default();
    subscriptions.action(
        dir.path(),
        "setup",
        &serde_json::json!({"action": "subscribe", "subscription_id": "s"}),
    );
    let path = dir.path().join(crate::db::MAIL_DB);
    {
        let conn = crate::db::open(&path).unwrap();
        for (recipient, timestamp, ack) in
            [("dead", OLD, 1), ("pending", OLD, 0), ("fresh", NOW, 1)]
        {
            conn.execute("INSERT INTO mail_heads VALUES (?1, 1)", [recipient])
                .unwrap();
            conn.execute(
                "INSERT INTO mail_events VALUES (?1, 1, 'm', 'mail', json_object('timestamp', ?2))",
                params![recipient, timestamp],
            )
            .unwrap();
            conn.execute(
                "INSERT INTO mail_subscriptions (recipient, id, filter, ack) VALUES (?1, 's', 'all', ?2)",
                params![recipient, ack],
            )
            .unwrap();
        }
    }
    assert_eq!(apply_twice(mail, &path), 3);
    assert_eq!(
        ids(&path, "SELECT recipient FROM mail_heads ORDER BY recipient"),
        ["fresh", "pending"]
    );
    // The subscription made through the API has no head yet and is untouched.
    assert_eq!(
        ids(
            &path,
            "SELECT recipient FROM mail_subscriptions ORDER BY recipient"
        ),
        ["fresh", "pending", "setup"]
    );
}

// Catches: comparing the stored ISO timestamps against another format.
#[test]
fn tunnel_audit_deletes_events_past_the_window() {
    let dir = temp_dir();
    let path = dir.path().join(crate::db::TUNNEL_AUDIT_DB);
    let log = crate::tunnels::audit::AuditLog::open(&path).unwrap();
    log.insert(
        "fresh",
        crate::tunnels::audit::EventKind::Started,
        serde_json::json!({}),
    )
    .unwrap();
    let old = chrono::DateTime::from_timestamp_millis(OLD)
        .unwrap()
        .format("%Y-%m-%dT%H:%M:%S%.3fZ")
        .to_string();
    crate::db::open(&path)
        .unwrap()
        .execute(
            "INSERT INTO tunnel_events (timestamp, tunnel_id, kind) VALUES (?1, 'old', 'started')",
            [old],
        )
        .unwrap();
    // The fresh row was written now, far after the test's NOW: run against the clock.
    let cutoff = now_ms() - 30 * DAY_MS;
    assert_eq!(tunnel_audit(&path, cutoff).unwrap(), 1);
    assert_eq!(tunnel_audit(&path, cutoff).unwrap(), 0);
    assert_eq!(ids(&path, "SELECT tunnel_id FROM tunnel_events"), ["fresh"]);
}

// Catches: an open launch (no terminal stage yet) losing its idempotency record,
// or MCP usage days inside the window being dropped.
#[test]
fn launches_and_mcp_usage_keep_open_and_fresh_rows() {
    let dir = temp_dir();
    let path = dir.path().join(crate::db::TUIC_DB);
    {
        let conn = crate::db::open_with(&path, &crate::db::TUIC_SCHEMA).unwrap();
        for (key, stage, updated) in [
            ("old-created", "created", OLD),
            ("old-blocked", "blocked", OLD),
            ("old-reserved", "reserved", OLD),
            ("fresh-created", "created", NOW),
        ] {
            conn.execute(
                "INSERT INTO launches VALUES (?1, json_object('stage', ?2), 0, ?3)",
                params![key, stage, updated],
            )
            .unwrap();
        }
        let day = |ms: i64| crate::mcp_http::mcp_usage_rollup::day_of(ms as u64);
        for day in [day(OLD - DAY_MS), day(CUTOFF), day(NOW)] {
            conn.execute(
                "INSERT INTO mcp_usage_daily VALUES (?1,'c','r','t','a',1,0,0,'',0,0)",
                [day],
            )
            .unwrap();
        }
    }
    assert_eq!(apply_twice(launches, &path), 2);
    assert_eq!(
        ids(&path, "SELECT key FROM launches ORDER BY key"),
        ["fresh-created", "old-reserved"]
    );
    assert_eq!(apply_twice(mcp_usage, &path), 1);
    assert_eq!(ids(&path, "SELECT day FROM mcp_usage_daily").len(), 2);
}

// Catches: a panel visit inside the window being forgotten, which would make
// the next visit report no new comments.
#[test]
fn work_item_visits_keep_visits_inside_the_window() {
    let dir = temp_dir();
    let path = dir.path().join(crate::db::TUIC_DB);
    for (url, visited) in [("old", OLD), ("edge", CUTOFF), ("fresh", NOW)] {
        crate::work_item_visits::visit(&path, url, 1, visited).unwrap();
    }
    assert_eq!(apply_twice(work_item_visits, &path), 1);
    assert_eq!(
        ids(
            &path,
            "SELECT item_url FROM work_item_visits ORDER BY item_url"
        ),
        ["edge", "fresh"]
    );
}

// Catches: removing a running instance or an instance still inside the
// retention window; a non-agent-test name is no longer special.
#[test]
fn stale_instances_are_removed_and_live_ones_kept() {
    let dir = temp_dir();
    let instances = dir.path().join("instances");
    for name in ["agent-test-dead", "agent-test-live", "my-instance"] {
        std::fs::create_dir_all(instances.join(name)).unwrap();
        std::fs::write(instances.join(name).join("config.json"), b"{}").unwrap();
    }
    let lock = std::fs::File::create(instances.join("agent-test-live/instance.lock")).unwrap();
    lock.try_lock().unwrap();

    assert_eq!(instances::remove_stale_instances(dir.path(), 0), 0);
    let future = now_ms() + DAY_MS;
    assert_eq!(instances::remove_stale_instances(dir.path(), future), 2);
    assert_eq!(instances::remove_stale_instances(dir.path(), future), 0);
    assert!(!instances.join("agent-test-dead").exists());
    assert!(!instances.join("my-instance").exists());
    assert!(instances.join("agent-test-live").exists());
    assert!(
        dir.path().exists(),
        "the daily instance is the data dir itself"
    );
}

fn disabled() -> RetentionConfig {
    RetentionConfig {
        progress_days: 0,
        automation_run_days: 0,
        mail_days: 0,
        tunnel_audit_days: 0,
        mcp_usage_days: 0,
        launch_days: 0,
        work_item_visit_days: 0,
        instance_days: 0,
        session_file_days: 0,
        capture_days: 0,
        bridge_revision_days: 0,
    }
}

// Catches: the job creating a store that does not exist yet, or a disabled rule
// (0 days) deleting anything.
#[test]
fn the_job_skips_missing_files_and_disabled_rules() {
    let dir = temp_dir();
    let in_use = InUse {
        sessions: Some(HashSet::new()),
        bridge_revision: None,
    };
    assert_eq!(
        run(dir.path(), &disabled(), NOW, &in_use),
        Vec::<(&str, usize)>::new()
    );
    let removed = run(dir.path(), &RetentionConfig::default(), NOW, &in_use);
    let stores: Vec<_> = removed
        .iter()
        .map(|(store, count)| (*store, *count))
        .collect();
    assert_eq!(
        stores,
        [
            ("instances", 0),
            ("captures", 0),
            ("mcp_bridge", 0),
            ("agent_capabilities", 0),
            ("bg_wakes", 0)
        ]
    );
    assert_eq!(std::fs::read_dir(dir.path()).unwrap().count(), 0);
}
