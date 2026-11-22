use super::*;

// Catches: closing one session removes sibling session files or legacy repository files.
#[test]
fn attachment_cleanup_removes_only_ended_session_and_startup_orphans() {
    let data = tempfile::tempdir_in(crate::test_support::test_temp_root()).unwrap();
    let ended = prepare_session_directory(data.path(), "pty", "ended").unwrap();
    let live = prepare_session_directory(data.path(), "pty", "live").unwrap();
    std::fs::write(ended.join("fresh.txt"), b"ended").unwrap();
    std::fs::write(live.join("fresh.txt"), b"live").unwrap();
    let foreign = data.path().join("attachments/user-files");
    std::fs::create_dir(&foreign).unwrap();
    std::fs::write(foreign.join("keep.txt"), b"user").unwrap();
    cleanup_session(data.path(), "pty", "ended");
    assert!(!ended.exists());
    assert_eq!(std::fs::read(live.join("fresh.txt")).unwrap(), b"live");
    // Record a real process that has exited: startup must reclaim a crashed owner,
    // while the critic regression below protects a still-live owner.
    let mut child = std::process::Command::new(std::env::current_exe().unwrap())
        .arg("--list")
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .spawn()
        .unwrap();
    let dead_pid = child.id();
    assert!(child.wait().unwrap().success());
    std::fs::write(live.with_file_name(".owner-pty-live"), dead_pid.to_string()).unwrap();
    let unknown = data.path().join("attachments/pty-unknown");
    std::fs::create_dir(&unknown).unwrap();
    std::fs::write(unknown.join("keep.txt"), b"unknown owner").unwrap();
    let invalid = data.path().join("attachments/pty-invalid");
    std::fs::create_dir(&invalid).unwrap();
    std::fs::write(invalid.with_file_name(".owner-pty-invalid"), "0").unwrap();
    cleanup_orphans(data.path());
    assert!(unknown.join("keep.txt").exists());
    assert!(invalid.exists());
    assert!(!live.exists());
    assert_eq!(std::fs::read(foreign.join("keep.txt")).unwrap(), b"user");
    let resumed = prepare_session_directory(data.path(), "pty", "resumed").unwrap();
    assert_eq!(std::fs::read_dir(resumed).unwrap().count(), 0);
}

// Catches: a crafted target id escapes the app-owned attachment root on any host.
#[test]
fn attachment_directory_rejects_cross_platform_traversal() {
    let data = tempfile::tempdir_in(crate::test_support::test_temp_root()).unwrap();
    for id in [
        "",
        "..",
        "../repo",
        "/etc/passwd",
        "C:\\repo",
        "\\\\server\\share",
        "x/y",
    ] {
        assert!(
            prepare_session_directory(data.path(), "pty", id).is_err(),
            "{id}"
        );
        cleanup_session(data.path(), "pty", id);
    }
    assert!(!data.path().join("attachments").exists());
}

// Catches: prepare or startup cleanup follows a symlink and writes/deletes user data.
#[cfg(unix)]
#[test]
fn attachment_symlinks_never_reach_external_files() {
    let data = tempfile::tempdir_in(crate::test_support::test_temp_root()).unwrap();
    let external = tempfile::tempdir_in(crate::test_support::test_temp_root()).unwrap();
    std::fs::write(external.path().join("keep.txt"), b"outside").unwrap();
    std::os::unix::fs::symlink(external.path(), data.path().join("attachments")).unwrap();
    assert!(prepare_session_directory(data.path(), "pty", "escape").is_err());
    cleanup_orphans(data.path());
    assert_eq!(
        std::fs::read(external.path().join("keep.txt")).unwrap(),
        b"outside"
    );
    std::fs::remove_file(data.path().join("attachments")).unwrap();
    std::fs::create_dir(data.path().join("attachments")).unwrap();
    std::os::unix::fs::symlink(external.path(), data.path().join("attachments/pty-escape"))
        .unwrap();
    assert!(prepare_session_directory(data.path(), "pty", "escape").is_err());
    cleanup_session(data.path(), "pty", "escape");
    cleanup_orphans(data.path());
    assert_eq!(
        std::fs::read(external.path().join("keep.txt")).unwrap(),
        b"outside"
    );
}
// Catches: starting a second TUIC instance deletes an active first instance's upload.
#[cfg(unix)]
#[tokio::test]
async fn second_instance_startup_preserves_live_session_uploads() {
    let first = std::sync::Arc::new(crate::state::tests_support::make_test_app_state());
    crate::state::tests_support::insert_dummy_session(&first, "live-first-instance");
    let receipt = super::upload(
        first.clone(),
        super::UploadQuery {
            kind: "pty".into(),
            id: "live-first-instance".into(),
            session: None,
            name: "image.png".into(),
        },
        axum::http::HeaderMap::new(),
        axum::body::Body::from("live attachment bytes"),
    )
    .await
    .unwrap();

    let second = crate::AppState::new(
        first.data_dir.clone(),
        first.worktrees_dir.clone(),
        crate::config::AppConfig::default(),
        first.log_buffer.clone(),
    );
    let bytes = std::fs::read(&receipt.path);
    crate::pty::cleanup_session("live-first-instance", &first);
    drop(second);
    assert_eq!(
        bytes.expect("second instance startup deleted a live session's attachment"),
        b"live attachment bytes"
    );
}
