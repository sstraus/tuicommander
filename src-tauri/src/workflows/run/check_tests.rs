//! Pinned check receipts and owned process trees of native verification.
use super::check::*;

#[test]
fn tuic_check_receipt_binds_a_clean_commit_and_reports_failures() {
    use std::process::Command;
    let repo = tempfile::tempdir().expect("repo");
    for args in [
        vec!["init", "-q"],
        vec!["config", "user.name", "Workflow Test"],
        vec!["config", "user.email", "workflow@example.test"],
    ] {
        assert!(
            Command::new("git")
                .args(args)
                .current_dir(repo.path())
                .status()
                .expect("git setup")
                .success()
        );
    }
    std::fs::write(repo.path().join("file.txt"), "first\n").expect("file");
    assert!(
        Command::new("git")
            .args(["add", "file.txt"])
            .current_dir(repo.path())
            .status()
            .expect("add")
            .success()
    );
    assert!(
        Command::new("git")
            .args(["commit", "-qm", "initial"])
            .current_dir(repo.path())
            .status()
            .expect("commit")
            .success()
    );
    let check = crate::workflows::CheckDefinition {
        id: "head".into(),
        argv: vec!["git".into(), "rev-parse".into(), "HEAD".into()],
        timeout_secs: 10,
    };
    let receipt =
        execute_run_check(&check, repo.path(), repo.path(), "receipt-test", false).expect("check");
    assert_eq!(receipt.exit_code, 0);
    assert_eq!(receipt.argv, check.argv);
    assert_eq!(receipt.commit.len(), 40);
    assert_eq!(receipt.tree.len(), 40);
    let failed = crate::workflows::CheckDefinition {
        id: "bad".into(),
        argv: vec!["git".into(), "rev-parse".into(), "missing-ref".into()],
        timeout_secs: 10,
    };
    assert_ne!(
        execute_run_check(&failed, repo.path(), repo.path(), "receipt-test", false)
            .expect("failed check receipt")
            .exit_code,
        0
    );
    std::fs::write(repo.path().join("file.txt"), "changed\n").expect("edit");
    assert!(
        execute_run_check(&check, repo.path(), repo.path(), "receipt-test", false)
            .expect_err("dirty worktree")
            .contains("clean")
    );
}

#[cfg(unix)]
#[test]
fn timed_out_pinned_check_stops_its_descendants() {
    assert_check_tree_stops(false);
}

#[cfg(unix)]
#[test]
fn cancelled_pinned_check_stops_descendants_and_fences_late_workers() {
    // catches: cancellation stops the run projection but leaves checks alive,
    // or a worker finishes its Git setup and spawns after cancellation.
    assert_check_tree_stops(true);
}

#[cfg(unix)]
fn assert_check_tree_stops(cancel: bool) {
    use std::process::Command;
    let repo = tempfile::tempdir().expect("repo");
    let pid_dir = tempfile::tempdir().expect("pid dir");
    let pid_file = pid_dir.path().join("worker.pid");
    for args in [
        vec!["init", "-q"],
        vec!["config", "user.name", "Workflow Test"],
        vec!["config", "user.email", "workflow@example.test"],
        vec!["commit", "-q", "--allow-empty", "-m", "initial"],
    ] {
        assert!(
            Command::new("git")
                .args(args)
                .current_dir(repo.path())
                .status()
                .expect("git setup")
                .success()
        );
    }
    // A runner that forks a long-lived worker and waits for it, like a
    // test runner with a child pool; killing only the shell leaves the worker.
    let check = crate::workflows::CheckDefinition {
        id: "pool".into(),
        argv: vec![
            "sh".into(),
            "-c".into(),
            format!("sleep 60 & echo $! > {}; wait", pid_file.display()),
        ],
        timeout_secs: if cancel { 60 } else { 1 },
    };
    if cancel {
        let repo_path = repo.path().to_owned();
        let db_path = pid_dir.path().join("runs.sqlite3");
        let worker_check = check.clone();
        let worker_db = db_path.clone();
        let worker = std::thread::spawn(move || {
            execute_run_check(
                &worker_check,
                &repo_path,
                &worker_db,
                "cancelled-run",
                false,
            )
        });
        // Setup is not the behavior deadline; nextest bounds a stuck setup.
        while !pid_file.exists() {
            std::thread::sleep(std::time::Duration::from_millis(10));
        }
        cancel_checks(&db_path, "cancelled-run");
        assert!(
            worker
                .join()
                .expect("check worker")
                .unwrap_err()
                .contains("cancelled")
        );
        assert!(
            execute_run_check(&check, repo.path(), &db_path, "cancelled-run", false)
                .unwrap_err()
                .contains("cancelled")
        );
    } else {
        let receipt = execute_run_check(&check, repo.path(), repo.path(), "receipt-test", false)
            .expect("timed out receipt");
        assert_eq!(receipt.exit_code, -1);
    }
    let worker: libc::pid_t = std::fs::read_to_string(&pid_file)
        .expect("worker pid")
        .trim()
        .parse()
        .expect("pid");
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
    // SAFETY: signal 0 only probes for existence.
    while unsafe { libc::kill(worker, 0) } == 0 {
        assert!(
            std::time::Instant::now() < deadline,
            "descendant {worker} outlived the timed-out check"
        );
        std::thread::sleep(std::time::Duration::from_millis(20));
    }
}

// Catches: a rerun starting while the original attempt's process group survived a TUIC
// kill (the group has its own id), or an unreadable or stale record blocking a rerun
// forever. A live group refuses; a gone group and an absent record both prove it gone.
#[cfg(unix)]
#[test]
fn original_process_gone_refuses_a_surviving_group_and_accepts_a_gone_one() {
    use std::os::unix::process::CommandExt;
    let dir = tempfile::tempdir_in(crate::test_support::test_temp_root()).unwrap();
    let db = dir.path().join("runs.sqlite3");
    assert_eq!(original_process_gone(&db, "run:script:abc"), Ok(()));
    let mut child = std::process::Command::new("sleep")
        .arg("30")
        .process_group(0)
        .spawn()
        .unwrap();
    let record = dir.path().join("runs.processes").join("run_script_abc");
    std::fs::create_dir_all(record.parent().unwrap()).unwrap();
    std::fs::write(&record, child.id().to_string()).unwrap();
    let refused = original_process_gone(&db, "run:script:abc").unwrap_err();
    assert!(refused.contains("may still be running"), "{refused}");
    assert!(record.exists(), "a refused proof keeps the record");
    child.kill().unwrap();
    child.wait().unwrap();
    assert_eq!(original_process_gone(&db, "run:script:abc"), Ok(()));
    assert!(!record.exists(), "a proven-gone group drops its record");
    std::fs::write(&record, "not a pid").unwrap();
    assert!(original_process_gone(&db, "run:script:abc").is_err());
}
