//! File rules: an old unowned entry goes, a live, current or fresh one stays.
//! Files are written now, so a cutoff one day ahead makes them "old" and a
//! cutoff in the past makes them "fresh".

use super::*;

fn temp_dir() -> tempfile::TempDir {
    tempfile::tempdir_in(crate::test_support::test_temp_root()).unwrap()
}

fn write(root: &Path, relative: &str) {
    let path = root.join(relative);
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::write(path, b"x").unwrap();
}

fn names(dir: &Path) -> Vec<String> {
    let mut names: Vec<String> = std::fs::read_dir(dir)
        .unwrap()
        .flatten()
        .map(|entry| entry.file_name().to_string_lossy().into_owned())
        .collect();
    names.sort();
    names
}

fn old() -> i64 {
    now_ms() + DAY_MS
}

fn fresh() -> i64 {
    now_ms() - 30 * DAY_MS
}

// Catches: a cleanup bug (or a crafted name) deleting outside the data dir.
#[test]
fn remove_refuses_paths_outside_the_data_dir() {
    let outer = temp_dir();
    let root = outer.path().join("config");
    write(&root, "inside/file");
    write(outer.path(), "outside");
    for path in [
        outer.path().join("outside"),
        root.join("../outside"),
        root.clone(),
    ] {
        assert!(files::remove(&root, &path).is_err(), "{}", path.display());
    }
    #[cfg(unix)]
    {
        std::os::unix::fs::symlink(outer.path(), root.join("escape")).unwrap();
        assert!(files::remove(&root, &root.join("escape/outside")).is_err());
        // The link itself is inside: deleting it leaves its target alone.
        files::remove(&root, &root.join("escape")).unwrap();
    }
    assert!(outer.path().join("outside").exists());
    files::remove(&root, &root.join("inside")).unwrap();
    assert!(!root.join("inside").exists());
}

// Catches: deleting the capability record or wake marker of a live or
// suspended session (a non-UUID identity is stored under a hashed name), or
// of a recently closed one.
#[test]
fn session_files_keep_live_sessions_and_fresh_files() {
    let dir = temp_dir();
    let live_uuid = "11111111-1111-1111-1111-111111111111";
    let dead_uuid = "22222222-2222-2222-2222-222222222222";
    let hashed = crate::agent_capabilities::file_stem("client/id");
    for sub in [files::AGENT_CAPABILITIES, files::BG_WAKES] {
        for stem in [live_uuid, dead_uuid, &hashed] {
            write(dir.path(), &format!("{sub}/{stem}.json"));
        }
    }
    let live = HashSet::from([live_uuid.to_string(), hashed.clone()]);
    for sub in [files::AGENT_CAPABILITIES, files::BG_WAKES] {
        assert_eq!(files::session_files(dir.path(), sub, fresh(), &live), 0);
        assert_eq!(files::session_files(dir.path(), sub, old(), &live), 1);
        assert_eq!(files::session_files(dir.path(), sub, old(), &live), 0);
        let mut kept = vec![format!("{live_uuid}.json"), format!("{hashed}.json")];
        kept.sort();
        assert_eq!(names(&dir.path().join(sub)), kept);
    }
}

// Catches: captures kept forever, or deleted inside the window.
#[test]
fn captures_go_after_the_window() {
    let dir = temp_dir();
    write(dir.path(), "captures/s.tcap");
    write(dir.path(), "captures/frame.json");
    assert_eq!(files::captures(dir.path(), fresh()), 0);
    assert_eq!(files::captures(dir.path(), old()), 2);
    assert_eq!(names(&dir.path().join("captures")), Vec::<String>::new());
}

// Catches: deleting the revision agent configs point at, or a recently
// installed one.
#[test]
fn bridge_keeps_the_current_and_recent_revisions() {
    let dir = temp_dir();
    for revision in ["aa", "bb", "cc"] {
        write(dir.path(), &format!("mcp-bridge/{revision}/tuic-bridge"));
    }
    assert_eq!(files::bridge_revisions(dir.path(), fresh(), Some("bb")), 0);
    assert_eq!(files::bridge_revisions(dir.path(), old(), Some("bb")), 2);
    assert_eq!(names(&dir.path().join("mcp-bridge")), ["bb"]);
}

// Catches: the startup run (before restored tabs respawn) deleting the
// capability record a restored root tab needs.
#[test]
fn the_startup_run_leaves_session_files_alone() {
    let dir = temp_dir();
    write(dir.path(), "agent-capabilities/x.json");
    write(dir.path(), "bg-wakes/x.json");
    let startup = InUse::default();
    let removed = run(
        dir.path(),
        &RetentionConfig::default(),
        old() + 30 * DAY_MS,
        &startup,
    );
    assert!(
        removed
            .iter()
            .all(|(store, _)| !matches!(*store, "agent_capabilities" | "bg_wakes"))
    );
    assert!(dir.path().join("agent-capabilities/x.json").exists());
    let daily = InUse {
        sessions: Some(HashSet::new()),
        bridge_revision: None,
    };
    run(
        dir.path(),
        &RetentionConfig::default(),
        old() + 30 * DAY_MS,
        &daily,
    );
    assert!(!dir.path().join("agent-capabilities/x.json").exists());
    assert!(!dir.path().join("bg-wakes/x.json").exists());
}

// Catches: loading a restored identity's record without refreshing it, so the
// cleanup deletes it while the tab is still in use and the root loses operator.
#[test]
fn loading_a_capability_record_refreshes_its_age() {
    let dir = temp_dir();
    let identity = "33333333-3333-3333-3333-333333333333";
    crate::agent_capabilities::save_root(dir.path(), identity).unwrap();
    let path = dir
        .path()
        .join(format!("agent-capabilities/{identity}.json"));
    let long_ago = UNIX_EPOCH + std::time::Duration::from_secs(1_000_000);
    std::fs::File::options()
        .write(true)
        .open(&path)
        .unwrap()
        .set_modified(long_ago)
        .unwrap();
    assert!(crate::agent_capabilities::load(dir.path(), identity).is_none());
    assert!(files::modified_ms(&path) > fresh());
}
