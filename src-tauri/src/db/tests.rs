use super::*;
use std::collections::BTreeSet;

fn temp_dir() -> tempfile::TempDir {
    tempfile::tempdir_in(crate::test_support::test_temp_root()).unwrap()
}

/// Database files in `dir`, without their `-wal`/`-shm` companions.
fn database_files(dir: &Path) -> BTreeSet<String> {
    std::fs::read_dir(dir)
        .unwrap()
        .flatten()
        .map(|entry| entry.file_name().to_string_lossy().into_owned())
        .filter(|name| {
            [".sqlite3", ".sqlite", ".db"]
                .iter()
                .any(|extension| name.ends_with(extension))
        })
        .collect()
}

// Catches: a store still opening its own file, or under another name or extension.
#[test]
fn a_fresh_start_creates_exactly_the_declared_files() {
    let dir = temp_dir();
    let _guard = crate::config::set_config_dir_override(dir.path().to_path_buf());
    legacy::remove_retired_files(dir.path());
    crate::stories::StoryStore::open().unwrap();
    crate::workflows::WorkflowStore::open().unwrap();
    crate::progress::ProgressStore::open().unwrap();
    crate::automations::store::RunStore::open().unwrap();
    crate::workflows::RunStore::open().unwrap();
    crate::mail_subscriptions::MailSubscriptions::default().action(
        dir.path(),
        "peer",
        &serde_json::json!({"action": "subscribe", "subscription_id": "s"}),
    );
    crate::tunnels::audit::AuditLog::open(&dir.path().join(TUNNEL_AUDIT_DB))
        .unwrap()
        .insert(
            "t",
            crate::tunnels::audit::EventKind::Started,
            serde_json::json!({}),
        )
        .unwrap();
    let declared = [
        TUIC_DB,
        PROGRESS_DB,
        AUTOMATION_RUNS_DB,
        WORKFLOW_RUNS_DB,
        MAIL_DB,
        TUNNEL_AUDIT_DB,
    ];
    let expected: BTreeSet<String> = declared.iter().map(|name| name.to_string()).collect();
    assert_eq!(database_files(dir.path()), expected);
}

// Catches: an old store, its WAL or a legacy backup surviving the first start,
// or a log written by this build being deleted on the next start.
#[test]
fn the_first_start_removes_every_old_file_and_later_starts_keep_new_logs() {
    let dir = temp_dir();
    let old = [
        "stories.sqlite3",
        "stories.sqlite3-wal",
        "workflows.sqlite3-shm",
        "managed-launches.sqlite",
        "mcp_usage.sqlite3",
        "mail-subscriptions.db",
        "mail-subscriptions.db-wal",
        "tunnel_audit.db",
        "progress.sqlite3",
        "automation_runs.sqlite3",
        "workflow_runs.sqlite3-wal",
        "automation_runs.sqlite3.legacy-workflows.bak",
        "automations.json.legacy-workflows.bak",
    ];
    for name in old {
        std::fs::write(dir.path().join(name), b"old").unwrap();
    }
    std::fs::write(dir.path().join("config.json"), b"{}").unwrap();
    std::fs::create_dir_all(dir.path().join("ai-sessions")).unwrap();
    std::fs::write(dir.path().join("ai-sessions/s.json"), b"{}").unwrap();

    let mut removed = legacy::remove_retired_files(dir.path());
    removed.sort();
    let mut expected: Vec<String> = old.iter().map(|name| name.to_string()).collect();
    expected.push("ai-sessions/".into());
    expected.sort();
    assert_eq!(removed, expected);
    assert!(dir.path().join("config.json").exists());
    assert!(!dir.path().join("ai-sessions").exists());
    assert!(dir.path().join(TUIC_DB).exists());

    open(&dir.path().join(PROGRESS_DB)).unwrap();
    assert_eq!(
        legacy::remove_retired_files(dir.path()),
        Vec::<String>::new()
    );
    assert!(dir.path().join(PROGRESS_DB).exists());
}

// Catches: two stores sharing tuic.sqlite3 failing with SQLITE_BUSY instead of
// waiting for each other within the busy timeout.
#[test]
fn two_stores_write_tuic_sqlite3_concurrently_without_busy_errors() {
    let dir = temp_dir();
    let path = dir.path().join(TUIC_DB);
    let stories = crate::stories::StoryStore::open_at(&path).unwrap();
    let launches_path = path.clone();
    let launches = std::thread::spawn(move || {
        for index in 0..200 {
            let mut conn = open_with(&launches_path, &TUIC_SCHEMA).unwrap();
            let tx = conn
                .transaction_with_behavior(TransactionBehavior::Immediate)
                .unwrap();
            tx.execute(
                "INSERT INTO launches VALUES (?1, '{}', 0, 0)",
                [format!("key-{index}")],
            )
            .unwrap();
            tx.commit().unwrap();
        }
    });
    for index in 0..200 {
        stories
            .create_plan(crate::stories::NewPlan {
                project: "/p".into(),
                title: format!("plan {index}"),
                source: "test".into(),
            })
            .unwrap();
    }
    launches.join().unwrap();
    let conn = open(&path).unwrap();
    let count = |table: &str| -> i64 {
        conn.query_row(&format!("SELECT count(*) FROM {table}"), [], |row| {
            row.get(0)
        })
        .unwrap()
    };
    assert_eq!((count("plans"), count("launches")), (200, 200));
}

// Catches: a build silently writing into a database laid out by a newer one.
#[test]
fn a_newer_schema_version_is_refused_and_its_data_kept() {
    let dir = temp_dir();
    let path = dir.path().join("future.sqlite3");
    {
        let conn = Connection::open(&path).unwrap();
        conn.execute_batch("CREATE TABLE future(x); INSERT INTO future VALUES (1);")
            .unwrap();
        conn.pragma_update(None, "user_version", 2).unwrap();
    }
    const ONE: Schema = Schema {
        name: "test",
        migrations: &[&["CREATE TABLE t(x)"]],
    };
    let error = open_with(&path, &ONE).unwrap_err();
    assert!(error.contains("schema version 2"), "{error}");
    let kept: i64 = Connection::open(&path)
        .unwrap()
        .query_row("SELECT count(*) FROM future", [], |row| row.get(0))
        .unwrap();
    assert_eq!(kept, 1);
}

// Catches: a migration rerun on a migrated file, or a later migration skipped.
#[test]
fn migrations_run_in_order_once() {
    let dir = temp_dir();
    let path = dir.path().join("m.sqlite3");
    const FIRST: Schema = Schema {
        name: "test",
        migrations: &[&["CREATE TABLE a(x)"]],
    };
    const SECOND: Schema = Schema {
        name: "test",
        migrations: &[&["CREATE TABLE a(x)"], &["CREATE TABLE b(x)"]],
    };
    open_with(&path, &FIRST).unwrap();
    // A second process would prepare again: simulate it with a fresh file key.
    let mut conn = open(&path).unwrap();
    migrate(&mut conn, &SECOND).unwrap();
    migrate(&mut conn, &SECOND).unwrap();
    assert_eq!(user_version(&conn).unwrap(), 2);
}
