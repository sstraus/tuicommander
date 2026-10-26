#[test]
fn worktree_remove_success_omits_absent_warning() {
    let clean = worktree_remove_success_response(None, "ancestry");
    assert_eq!(clean["ok"], true);
    assert_eq!(clean["removal_rule"], "ancestry");
    assert!(clean.get("branch_delete_warning").is_none());

    let warned =
        worktree_remove_success_response(Some("branch retained".to_string()), "patch_equivalence");
    assert_eq!(warned["branch_delete_warning"], "branch retained");
}

#[tokio::test]
async fn mcp_branch_integration_queries_include_branches_without_worktrees_1295() {
    let (_temp, repo, _) = branch_delete_fixture();
    let git = |args: &[&str]| crate::git_cli::git_cmd(&repo).args(args).run().unwrap();
    git(&["checkout", "-b", "query-1295"]);
    branch_delete_commit(&repo, "one.txt", "one\n");
    branch_delete_commit(&repo, "two.txt", "two\n");
    git(&["checkout", "main"]);
    git(&["merge", "--squash", "query-1295"]);
    git(&["commit", "-m", "release\n\none.txt\ntwo.txt"]);
    let state = test_state();
    let query = handle_repo(
        &state,
        &serde_json::json!({
            "action":"branch_integration", "path":repo, "branch":"query-1295"
        }),
        false,
    )
    .await;
    assert_eq!(query["integrated"], true, "{query}");
    assert_eq!(query["proof"], "squash_message", "{query}");
    assert_eq!(query["archive_required"], false, "{query}");
    let list = handle_repo(
        &state,
        &serde_json::json!({
            "action":"branch_integrations", "path":repo
        }),
        false,
    )
    .await;
    assert!(
        list.as_array().unwrap().iter().any(|entry| entry == &query),
        "{list}"
    );
    let deleted = handle_repo(
        &state,
        &serde_json::json!({
            "action":"branch_delete", "path":repo, "branch":"query-1295"
        }),
        false,
    )
    .await;
    assert_eq!(deleted["proof"], query["proof"], "{deleted}");
    assert!(
        crate::git_cli::git_cmd(&repo)
            .args(["rev-parse", "--verify", "refs/heads/query-1295"])
            .run_silent()
            .is_none()
    );
}

#[tokio::test]
async fn mcp_branch_delete_accepts_integrated_refs_despite_stale_upstream_or_squash() {
    let (_temp, repo, base) = branch_delete_fixture();
    let git = |args: &[&str]| crate::git_cli::git_cmd(&repo).args(args).run().unwrap();
    git(&["checkout", "-b", "stale-upstream"]);
    branch_delete_commit(&repo, "merged.txt", "merged on integration\n");
    git(&["checkout", "integration"]);
    git(&["merge", "--no-ff", "stale-upstream", "--no-edit"]);
    git(&["update-ref", "refs/remotes/origin/stale-upstream", &base]);
    git(&["config", "branch.stale-upstream.remote", "origin"]);
    git(&[
        "config",
        "branch.stale-upstream.merge",
        "refs/heads/stale-upstream",
    ]);
    let upstream_tip = crate::git_cli::git_cmd(&repo)
        .args(["rev-parse", "refs/remotes/origin/stale-upstream"])
        .run()
        .unwrap()
        .stdout;
    assert_eq!(upstream_tip.trim(), base, "upstream must be stale");

    git(&["checkout", "-b", "squashed"]);
    branch_delete_commit(&repo, "squash.txt", "same final patch\n");
    git(&["checkout", "integration"]);
    branch_delete_commit(&repo, "squash.txt", "same final patch\n");
    // Distinct messages guarantee different SHAs even when Git records both
    // commits in the same second with identical parents and trees.
    git(&[
        "commit",
        "--amend",
        "-m",
        "integration copy of squash patch",
    ]);
    // Deletion proof is against the default branch, not the checkout.
    git(&["branch", "-f", "main", "integration"]);
    assert!(
        crate::git_cli::git_cmd(&repo)
            .args(["merge-base", "--is-ancestor", "squashed", "integration"])
            .run_silent()
            .is_none(),
        "the fixture must require patch proof rather than ancestry"
    );

    let state = test_state();
    let path = repo.to_string_lossy();
    let ancestor = handle_repo(
        &state,
        &serde_json::json!({"action":"branch_delete","path":path,"branch":"stale-upstream"}),
        false,
    )
    .await;
    let squash = handle_repo(
        &state,
        &serde_json::json!({"action":"branch_delete","path":path,"branch":"squashed"}),
        false,
    )
    .await;

    assert_eq!(ancestor["ok"], true, "{ancestor}");
    assert_eq!(ancestor["proof"], "ancestry", "{ancestor}");
    assert_eq!(squash["ok"], true, "{squash}");
    assert_eq!(squash["proof"], "patch_equivalence", "{squash}");
    for branch in ["stale-upstream", "squashed"] {
        assert!(
            crate::git_cli::git_cmd(&repo)
                .args(["show-ref", "--verify", &format!("refs/heads/{branch}")])
                .run_silent()
                .is_none(),
            "local {branch} must be gone"
        );
    }
    assert!(
        crate::git_cli::git_cmd(&repo)
            .args(["show-ref", "--verify", "refs/remotes/origin/stale-upstream"])
            .run()
            .is_ok(),
        "the remote-tracking ref must remain untouched"
    );
}

#[tokio::test]
async fn branch_delete_accepts_archived_tip() {
    let (_temp, repo, _base) = branch_delete_fixture();
    let git = |args: &[&str]| crate::git_cli::git_cmd(&repo).args(args).run().unwrap();
    git(&["checkout", "-b", "feat/superseded"]);
    branch_delete_commit(&repo, "superseded.txt", "only on this branch\n");
    git(&["branch", "archive-src"]);
    git(&["update-ref", "refs/archive/feat/superseded", "archive-src"]);
    git(&["branch", "-D", "archive-src"]);
    git(&["checkout", "integration"]);

    let response = handle_repo(
            &test_state(),
            &serde_json::json!({"action":"branch_delete","path":repo.to_string_lossy(),"branch":"feat/superseded"}),
            false,
        )
        .await;

    assert_eq!(response["ok"], true, "{response}");
    assert_eq!(response["proof"], "archived", "{response}");
    assert_eq!(
        response["archive_ref"], "refs/archive/feat/superseded",
        "{response}"
    );
    assert!(!branch_exists(&repo, "refs/heads/feat/superseded"));
    assert!(branch_exists(&repo, "refs/archive/feat/superseded"));
}

// Catches reporting the occupied primary archive ref instead of the
// tip-suffixed ref: restoring from the response must recover the deleted tip.
#[tokio::test]
async fn branch_delete_reports_the_suffixed_archive_holding_the_tip_1489() {
    let (_temp, repo, base) = branch_delete_fixture();
    assert!(
        repo.as_path()
            .canonicalize()
            .unwrap()
            .starts_with(tuic_test_support::test_temp_root().canonicalize().unwrap())
    );
    let git = |args: &[&str]| crate::git_cli::git_cmd(&repo).args(args).run().unwrap();
    git(&["checkout", "-b", "reused"]);
    branch_delete_commit(&repo, "unique.txt", "unmerged work\n");
    let tip = git(&["rev-parse", "HEAD"]).stdout.trim().to_owned();
    let primary = "refs/archive/reused";
    let suffixed = format!("{primary}-{}", &tip[..7]);
    git(&["update-ref", primary, &base]);
    git(&["update-ref", &suffixed, &tip]);
    git(&["checkout", "integration"]);
    let response = handle_repo(
            &test_state(),
            &serde_json::json!({"action":"branch_delete","path":repo.to_string_lossy(),"branch":"reused"}),
            false,
        ).await;
    assert_eq!(response["ok"], true, "{response}");
    assert_eq!(response["proof"], "archived");
    assert_eq!(response["archive_ref"], suffixed);
    assert_eq!(git(&["rev-parse", primary]).stdout.trim(), base);
    assert_eq!(
        git(&["rev-parse", response["archive_ref"].as_str().unwrap()])
            .stdout
            .trim(),
        tip
    );
    assert!(!branch_exists(&repo, "refs/heads/reused"));
}

#[tokio::test]
async fn branch_delete_refuses_stale_archive() {
    let (_temp, repo, _base) = branch_delete_fixture();
    let git = |args: &[&str]| crate::git_cli::git_cmd(&repo).args(args).run().unwrap();
    git(&["checkout", "-b", "moved"]);
    branch_delete_commit(&repo, "first.txt", "archived\n");
    git(&["update-ref", "refs/archive/moved", "moved"]);
    branch_delete_commit(&repo, "second.txt", "committed after archiving\n");
    git(&["checkout", "integration"]);

    let response = handle_repo(
            &test_state(),
            &serde_json::json!({"action":"branch_delete","path":repo.to_string_lossy(),"branch":"moved"}),
            false,
        )
        .await;

    assert!(
        response["error"]
            .as_str()
            .is_some_and(|e| e.contains("unmerged")),
        "{response}"
    );
    assert!(branch_exists(&repo, "refs/heads/moved"));
}

#[tokio::test]
async fn branch_delete_refuses_checked_out_or_default() {
    let (temp, repo, _base) = branch_delete_fixture();
    let git = |args: &[&str]| crate::git_cli::git_cmd(&repo).args(args).run().unwrap();
    git(&["checkout", "-b", "wt-branch"]);
    branch_delete_commit(&repo, "wt.txt", "unique\n");
    git(&["checkout", "integration"]);
    // The default branch carries a commit that only it holds, archived at its exact tip.
    git(&["checkout", "main"]);
    branch_delete_commit(&repo, "main-only.txt", "default only\n");
    git(&["checkout", "integration"]);
    let linked = temp.path().join("linked");
    git(&["worktree", "add", linked.to_str().unwrap(), "wt-branch"]);
    for branch in ["wt-branch", "main", "integration"] {
        git(&["update-ref", &format!("refs/archive/{branch}"), branch]);
    }

    let state = test_state();
    let path = repo.to_string_lossy();
    for (branch, reason) in [
        ("wt-branch", "checked out"),
        ("main", "default"),
        ("integration", "current"),
    ] {
        let response = handle_repo(
            &state,
            &serde_json::json!({"action":"branch_delete","path":path,"branch":branch}),
            false,
        )
        .await;
        assert!(
            response["error"]
                .as_str()
                .is_some_and(|e| e.to_lowercase().contains(reason)),
            "{branch}: {response}"
        );
        assert!(branch_exists(&repo, &format!("refs/heads/{branch}")));
    }
}

#[tokio::test]
async fn mcp_branch_delete_refuses_unique_checked_out_current_and_missing_branches() {
    let (temp, repo, _base) = branch_delete_fixture();
    let git = |args: &[&str]| crate::git_cli::git_cmd(&repo).args(args).run().unwrap();
    git(&["checkout", "-b", "unique"]);
    branch_delete_commit(&repo, "unique.txt", "not integrated\n");
    git(&["checkout", "integration"]);
    git(&["branch", "checked-out"]);
    let linked = temp.path().join("linked");
    git(&["worktree", "add", linked.to_str().unwrap(), "checked-out"]);

    let state = test_state();
    let path = repo.to_string_lossy();
    let cases = [
        ("unique", "unmerged"),
        ("checked-out", "checked out"),
        ("integration", "current"),
        ("main", "default"),
        ("missing", "does not exist"),
        ("../HEAD", "invalid"),
    ];
    let mut failures = Vec::new();
    for (branch, reason) in cases {
        let response = handle_repo(
            &state,
            &serde_json::json!({"action":"branch_delete","path":path,"branch":branch}),
            false,
        )
        .await;
        if !response["error"]
            .as_str()
            .is_some_and(|error| error.to_lowercase().contains(reason))
        {
            failures.push(format!("{branch}: expected {reason:?}, got {response}"));
        }
        assert!(
            crate::git_cli::git_cmd(&repo)
                .args(["show-ref", "--verify", &format!("refs/heads/{branch}")])
                .run_silent()
                .is_some()
                || matches!(branch, "missing" | "../HEAD"),
            "ref {branch} must be retained"
        );
    }
    assert!(failures.is_empty(), "{}", failures.join("; "));
    assert!(linked.exists(), "a checked-out worktree must remain");
}

#[tokio::test]
async fn mcp_worktree_remove_defaults_to_non_force_and_keeps_dirty_work() {
    let config = tempfile::tempdir().unwrap();
    let _guard = crate::config::set_config_dir_override(config.path().to_path_buf());
    let temp = tempfile::tempdir().unwrap();
    let repo = temp.path().join("repo");
    std::fs::create_dir(&repo).unwrap();
    crate::git_cli::git_cmd(&repo).args(["init"]).run().unwrap();
    crate::git_cli::git_cmd(&repo)
        .args(["config", "user.email", "test@test.com"])
        .run()
        .unwrap();
    crate::git_cli::git_cmd(&repo)
        .args(["config", "user.name", "Test"])
        .run()
        .unwrap();
    std::fs::write(repo.join("README.md"), "base\n").unwrap();
    crate::git_cli::git_cmd(&repo)
        .args(["add", "."])
        .run()
        .unwrap();
    crate::git_cli::git_cmd(&repo)
        .args(["commit", "-m", "base"])
        .run()
        .unwrap();
    crate::git_cli::git_cmd(&repo)
        .args(["branch", "-M", "main"])
        .run()
        .unwrap();
    let worktree = temp.path().join("feature");
    crate::git_cli::git_cmd(&repo)
        .args([
            "worktree",
            "add",
            "-b",
            "feature",
            &worktree.to_string_lossy(),
        ])
        .run()
        .unwrap();
    let dirty = worktree.join("untracked.txt");
    std::fs::write(&dirty, "keep this\n").unwrap();
    let state = test_state();
    let repo_path = repo.to_string_lossy().into_owned();

    let refused = handle_worktree(
        &state,
        &serde_json::json!({"action": "worktree_remove", "path": &repo_path, "branch": "feature"}),
        false,
    )
    .await;
    assert!(
        refused["error"].as_str().unwrap().contains("uncommitted"),
        "{refused}"
    );
    assert!(dirty.exists());
    assert!(
        crate::git_cli::git_cmd(&repo)
            .args(["show-ref", "--verify", "refs/heads/feature"])
            .run()
            .is_ok()
    );

    crate::git_cli::git_cmd(&worktree)
        .args(["add", "untracked.txt"])
        .run()
        .unwrap();
    crate::git_cli::git_cmd(&worktree)
        .args(["commit", "-m", "unique change"])
        .run()
        .unwrap();
    let missing_confirmation = handle_worktree(
            &state,
            &serde_json::json!({"action": "worktree_remove", "path": &repo_path, "branch": "feature", "force": true, "delete_branch": true}),
            false,
        )
        .await;
    assert!(
        missing_confirmation["error"]
            .as_str()
            .is_some_and(|error| error.contains("fingerprint")),
        "{missing_confirmation}"
    );
    assert!(worktree.exists());
    let lifecycle = handle_worktree(
            &state,
            &serde_json::json!({"action": "worktree_lifecycle", "path": &repo_path, "branch": "feature"}),
            false,
        )
        .await;
    let fingerprint = lifecycle["dirty_fingerprint"]
        .as_str()
        .expect("MCP lifecycle fingerprint");
    assert!(lifecycle["submodule_unpushed_commits"].is_array());
    let removed = handle_worktree(
            &state,
            &serde_json::json!({"action": "worktree_remove", "path": &repo_path, "branch": "feature", "force": true, "delete_branch": true, "expected_fingerprint": fingerprint}),
            false,
        )
        .await;
    assert_eq!(removed["ok"], true, "{removed}");
    assert!(
        removed["branch_delete_warning"]
            .as_str()
            .is_some_and(|s| s.contains("unmerged")),
        "{removed}"
    );
    assert!(!worktree.exists());
    assert!(
        crate::git_cli::git_cmd(&repo)
            .args(["show-ref", "--verify", "refs/heads/feature"])
            .run()
            .is_ok()
    );

    let second = temp.path().join("force-default");
    crate::git_cli::git_cmd(&repo)
        .args([
            "worktree",
            "add",
            "-b",
            "force-default",
            &second.to_string_lossy(),
        ])
        .run()
        .unwrap();
    std::fs::write(second.join("untracked.txt"), "discardable\n").unwrap();
    let second_lifecycle = handle_worktree(
            &state,
            &serde_json::json!({"action": "worktree_lifecycle", "path": &repo_path, "branch": "force-default"}),
            false,
        )
        .await;
    let second_fingerprint = second_lifecycle["dirty_fingerprint"].as_str().unwrap();
    let force_without_delete = handle_worktree(
            &state,
            &serde_json::json!({"action": "worktree_remove", "path": &repo_path, "branch": "force-default", "force": true, "expected_fingerprint": second_fingerprint}),
            false,
        )
        .await;
    assert_eq!(force_without_delete["ok"], true, "{force_without_delete}");
    assert!(!second.exists());
    assert!(
        crate::git_cli::git_cmd(&repo)
            .args(["show-ref", "--verify", "refs/heads/force-default"])
            .run()
            .is_ok(),
        "force alone does not authorize branch deletion"
    );
}

#[tokio::test]
async fn github_dispatch_reports_missing_and_unknown_actions() {
    let state = test_state();

    let missing = handle_github(&state, &serde_json::json!({})).await;
    assert!(missing["error"].as_str().unwrap().contains("action"));

    let unknown = handle_github(&state, &serde_json::json!({"action": "explode"})).await;
    let error = unknown["error"].as_str().unwrap();
    assert!(error.contains("Unknown action 'explode'"));
    assert!(
        error.contains("status"),
        "available actions omitted status: {error}"
    );
}

/// Catches: a cold listing waits on GitHub network proof behind the CLI's 3s deadline.
/// A loopback HTTP proxy rejects any attempted connection, without contacting GitHub.
#[tokio::test]
async fn native_mcp_worktree_list_never_queries_uncached_github_proofs() {
    use std::sync::atomic::{AtomicUsize, Ordering};
    use tokio::io::{AsyncReadExt, AsyncWriteExt};

    let config = tempfile::tempdir().unwrap();
    let _guard = crate::config::set_config_dir_override(config.path().to_path_buf());
    let (_temp, repo, _) = branch_delete_fixture();
    crate::git_cli::git_cmd(&repo)
        .args(["checkout", "main"])
        .run()
        .unwrap();
    let checkout = _temp.path().join("cold-proof");
    crate::git_cli::git_cmd(&repo)
        .args([
            "worktree",
            "add",
            "-b",
            "cold-proof",
            checkout.to_str().unwrap(),
        ])
        .run()
        .unwrap();
    branch_delete_commit(&checkout, "own.txt", "unmerged work\n");
    crate::git_cli::git_cmd(&repo)
        .args([
            "remote",
            "add",
            "origin",
            "https://github.com/fixture/never-contact.git",
        ])
        .run()
        .unwrap();

    let proxy = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let proxy_url = format!("http://{}", proxy.local_addr().unwrap());
    let requests = Arc::new(AtomicUsize::new(0));
    let observed = Arc::clone(&requests);
    let proxy_task = tokio::spawn(async move {
        while let Ok((mut socket, _)) = proxy.accept().await {
            let mut bytes = [0; 4096];
            let bytes_read = socket.read(&mut bytes).await.unwrap();
            if bytes_read == 0 {
                continue;
            }
            observed.fetch_add(1, Ordering::Relaxed);
            socket
                .write_all(
                    b"HTTP/1.1 502 Bad Gateway\r\nContent-Length: 0\r\nConnection: close\r\n\r\n",
                )
                .await
                .unwrap();
        }
    });
    let mut state = test_state();
    Arc::get_mut(&mut state).unwrap().http_client = reqwest::Client::builder()
        .proxy(reqwest::Proxy::all(proxy_url).unwrap())
        .build()
        .unwrap();
    // Synthetic cached token prevents any external credential lookup. The proxy
    // rejects CONNECT before TLS, so the token never leaves this process.
    *state.github.token.write() = Some("synthetic-fixture-token".into());
    let response = handle_worktree(
        &state,
        &serde_json::json!({"action":"worktree_list", "path":repo}),
        false,
    )
    .await;
    proxy_task.abort();
    let _ = proxy_task.await;
    assert_eq!(
        requests.load(Ordering::Relaxed),
        0,
        "cold listing attempted GitHub network I/O"
    );
    assert_eq!(
        response["cold-proof"]["lifecycle_status"]["commit_status"], "unmerged",
        "{response}"
    );
    assert_eq!(
        response["cold-proof"]["lifecycle_status"]["dirty_files"], 0,
        "{response}"
    );
    assert!(
        response["cold-proof"]["lifecycle_status"]["dirty_fingerprint"].is_string(),
        "{response}"
    );
}

/// Catches repeated inventory/integration subprocesses for unchanged linked
/// worktrees on the listing path; fresh dirt and fingerprints must still move.
#[tokio::test]
async fn native_mcp_worktree_list_reuses_merge_proofs_for_forty_worktrees() {
    // DEFERRED (2026-10-25) — Windows repeat listing takes 13.621s with the fork budget satisfied; profile per-checkout reads there, not a larger response deadline.
    let (_temp, repo, _) = branch_delete_fixture();
    crate::git_cli::git_cmd(&repo)
        .args(["checkout", "main"])
        .run()
        .unwrap();
    let mut checkouts = Vec::new();
    for index in 0..39 {
        let checkout = _temp.path().join(format!("work-{index}"));
        crate::git_cli::git_cmd(&repo)
            .args([
                "worktree",
                "add",
                "-b",
                &format!("work-{index}"),
                checkout.to_str().unwrap(),
            ])
            .run()
            .unwrap();
        checkouts.push(checkout);
    }
    let state = test_state();
    let args = serde_json::json!({"action": "worktree_list", "path": repo});
    let first = handle_worktree(&state, &args, false).await;
    assert_eq!(first.as_object().unwrap().len(), 40, "{first}");
    let fingerprint = first["work-0"]["lifecycle_status"]["dirty_fingerprint"]
        .as_str()
        .unwrap()
        .to_owned();
    std::fs::write(checkouts[0].join("untracked.txt"), "fresh dirt\n").unwrap();

    super::super::mcp_transport_ancillary::WORKTREE_LIST_TIMINGS.remove(repo.to_str().unwrap());
    let before = tuic_git::git_cli::git_cmd_forks(&repo);
    let started = std::time::Instant::now();
    let second = handle_worktree(&state, &args, false).await;
    let elapsed = started.elapsed();
    let forks = tuic_git::git_cli::git_cmd_forks(&repo) - before;
    assert_eq!(second.as_object().unwrap().len(), 40, "{second}");
    assert_eq!(second["work-0"]["lifecycle_status"]["dirty_files"], 1);
    assert_eq!(
        second["work-0"]["lifecycle_status"]["removal_safety"],
        "requires_force"
    );
    assert_ne!(
        second["work-0"]["lifecycle_status"]["dirty_fingerprint"],
        fingerprint
    );
    assert_eq!(second["work-1"]["lifecycle_status"]["dirty_files"], 0);
    for workspace in second.as_object().unwrap().values() {
        assert!(workspace["lifecycle_status"]["dirty_fingerprint"].is_string());
        assert_eq!(
            workspace["lifecycle_status"]["submodule_unpushed_commits"],
            serde_json::json!([])
        );
    }
    // One inventory plus the main checkout's fresh status/fingerprint reads.
    // No sibling branch may re-run its inventory or unchanged merge proof.
    assert!(
        forks <= 6,
        "40-worktree repeat listing built {forks} base-repo git commands in {elapsed:?}"
    );
    assert!(
        elapsed < std::time::Duration::from_secs(3),
        "40-worktree repeat listing exceeded the CLI response window: {elapsed:?}; per-checkout timings={:#?}",
        super::super::mcp_transport_ancillary::WORKTREE_LIST_TIMINGS
            .get(repo.to_str().unwrap())
            .map(|entry| entry.value().clone())
    );

    // Changed refs must invalidate monitoring proofs, unlike a plain file save.
    branch_delete_commit(&checkouts[1], "feature.txt", "own commit\n");
    let changed = handle_worktree(&state, &args, false).await;
    assert_eq!(
        changed["work-1"]["lifecycle_status"]["commit_status"],
        "unmerged"
    );
    assert_eq!(changed["work-0"]["lifecycle_status"]["dirty_files"], 1);
}

/// worktree_list must share the sidebar and removal classifier rather than
/// recomputing merge state a second way; listing may reuse monitoring proofs.
#[tokio::test]
async fn native_mcp_worktree_list_reports_lifecycle_status() {
    let config = tempfile::tempdir().unwrap();
    let _guard = crate::config::set_config_dir_override(config.path().to_path_buf());
    let temp = tempfile::tempdir().unwrap();
    let repo = temp.path().join("repo");
    std::fs::create_dir(&repo).unwrap();
    crate::git_cli::git_cmd(&repo).args(["init"]).run().unwrap();
    crate::git_cli::git_cmd(&repo)
        .args(["config", "user.email", "test@test.com"])
        .run()
        .unwrap();
    crate::git_cli::git_cmd(&repo)
        .args(["config", "user.name", "Test"])
        .run()
        .unwrap();
    std::fs::write(repo.join("README.md"), "base\n").unwrap();
    crate::git_cli::git_cmd(&repo)
        .args(["add", "."])
        .run()
        .unwrap();
    crate::git_cli::git_cmd(&repo)
        .args(["commit", "-m", "base"])
        .run()
        .unwrap();
    crate::git_cli::git_cmd(&repo)
        .args(["branch", "-M", "main"])
        .run()
        .unwrap();
    let worktree = temp.path().join("feature");
    crate::git_cli::git_cmd(&repo)
        .args([
            "worktree",
            "add",
            "-b",
            "feature",
            &worktree.to_string_lossy(),
        ])
        .run()
        .unwrap();
    std::fs::write(worktree.join("feature.txt"), "work\n").unwrap();
    crate::git_cli::git_cmd(&worktree)
        .args(["add", "."])
        .run()
        .unwrap();
    crate::git_cli::git_cmd(&worktree)
        .args(["commit", "-m", "feature work"])
        .run()
        .unwrap();
    // Fast-forwards main to feature's tip, so the worktree is merged and clean.
    crate::git_cli::git_cmd(&repo)
        .args(["merge", "feature"])
        .run()
        .unwrap();

    let state = test_state();
    let repo_path = repo.to_string_lossy().into_owned();
    let response = handle_worktree(
        &state,
        &serde_json::json!({"action": "worktree_list", "path": &repo_path}),
        false,
    )
    .await;

    let lifecycle = &response["feature"]["lifecycle_status"];
    assert_eq!(
        lifecycle["commit_status"].as_str(),
        Some("merged"),
        "a fast-forward-merged, clean worktree must report its merged commit status, reusing \
             the same inspect_workspace_lifecycle the sidebar uses: {response}"
    );
    assert_eq!(
        lifecycle["dirty_files"].as_i64(),
        Some(0),
        "a clean worktree must report zero dirty files: {response}"
    );
    assert_eq!(
        lifecycle["removal_safety"].as_str(),
        Some("safe"),
        "a merged, clean worktree must be safe to remove: {response}"
    );
}

/// Adversarial complement to `native_mcp_worktree_list_reports_lifecycle_status`:
/// an unmerged branch with an uncommitted, dirty file must not be reported as
/// merged/safe. Catches a fan-out bug that mismatches lifecycle results back
/// onto the wrong workspace id, or a default that silently reads as "safe".
#[tokio::test]
async fn native_mcp_worktree_list_reports_unmerged_dirty_worktree_as_unsafe() {
    let config = tempfile::tempdir().unwrap();
    let _guard = crate::config::set_config_dir_override(config.path().to_path_buf());
    let temp = tempfile::tempdir().unwrap();
    let repo = temp.path().join("repo");
    std::fs::create_dir(&repo).unwrap();
    crate::git_cli::git_cmd(&repo).args(["init"]).run().unwrap();
    crate::git_cli::git_cmd(&repo)
        .args(["config", "user.email", "test@test.com"])
        .run()
        .unwrap();
    crate::git_cli::git_cmd(&repo)
        .args(["config", "user.name", "Test"])
        .run()
        .unwrap();
    std::fs::write(repo.join("README.md"), "base\n").unwrap();
    crate::git_cli::git_cmd(&repo)
        .args(["add", "."])
        .run()
        .unwrap();
    crate::git_cli::git_cmd(&repo)
        .args(["commit", "-m", "base"])
        .run()
        .unwrap();
    crate::git_cli::git_cmd(&repo)
        .args(["branch", "-M", "main"])
        .run()
        .unwrap();
    let worktree = temp.path().join("feature");
    crate::git_cli::git_cmd(&repo)
        .args([
            "worktree",
            "add",
            "-b",
            "feature",
            &worktree.to_string_lossy(),
        ])
        .run()
        .unwrap();
    std::fs::write(worktree.join("feature.txt"), "work\n").unwrap();
    crate::git_cli::git_cmd(&worktree)
        .args(["add", "."])
        .run()
        .unwrap();
    crate::git_cli::git_cmd(&worktree)
        .args(["commit", "-m", "feature work"])
        .run()
        .unwrap();
    // main never merges feature: feature stays unmerged. An untracked file
    // keeps the worktree dirty.
    std::fs::write(worktree.join("untracked.txt"), "not committed\n").unwrap();

    let state = test_state();
    let repo_path = repo.to_string_lossy().into_owned();
    let response = handle_worktree(
        &state,
        &serde_json::json!({"action": "worktree_list", "path": &repo_path}),
        false,
    )
    .await;

    let lifecycle = &response["feature"]["lifecycle_status"];
    assert_eq!(
        lifecycle["commit_status"].as_str(),
        Some("unmerged"),
        "an unmerged branch must not be reported merged: {response}"
    );
    assert_eq!(
        lifecycle["dirty_files"].as_i64(),
        Some(1),
        "the untracked file must be counted dirty: {response}"
    );
    assert_eq!(
        lifecycle["removal_safety"].as_str(),
        Some("requires_force"),
        "a dirty, unmerged worktree must never report safe: {response}"
    );

    // main itself is merged (it is the default branch) and has no dirty
    // files: its lifecycle must not have picked up feature's verdict.
    let main_lifecycle = &response["main"]["lifecycle_status"];
    assert_eq!(main_lifecycle["dirty_files"].as_i64(), Some(0));
    assert_ne!(
        main_lifecycle["commit_status"].as_str(),
        Some("unmerged"),
        "main's own lifecycle must not be feature's: {response}"
    );
}

/// `repo` still routes the worktree actions after the LEGACY action remap
/// was dropped — `handle_worktree` now reads the merged action names
/// straight off `args` instead of a rewritten clone.
#[tokio::test]
async fn repo_dispatch_still_routes_worktree_actions() {
    let state = test_state();

    for action in ["worktree_list", "worktree_create", "worktree_remove"] {
        let response = handle_repo(&state, &serde_json::json!({"action": action}), false).await;
        let error = response["error"].as_str().unwrap();
        assert!(
            !error.contains("Unknown action"),
            "repo must dispatch '{action}' to the worktree handler: {error}"
        );
        assert!(
            error.contains("path"),
            "{action} must reach the path check in handle_worktree: {error}"
        );
    }
}

// The parent module's `test_state` was already this function, byte for byte:
// the same helper, the same two overrides — all native tools enabled, and the
// tool search index built so `search_tools`/`get_tool_schema` work without the
// background updater. Both were hand-copied `AppState` literals before
// #678-9a75; one of them is enough.
use super::super::tests::test_state;

#[tokio::test]
async fn remote_mcp_update_requires_confirmation_fields() {
    let state = test_state();
    let response = handle_remote_update(
        &state,
        &serde_json::json!({
            "action": "update", "connection_id": "machine-1"
        }),
    )
    .await;
    assert_eq!(
        response["error"],
        "remote update requires confirmed_sessions"
    );

    let response = handle_remote_update(
        &state,
        &serde_json::json!({
            "action": "update", "connection_id": "machine-1", "confirmed_sessions": 3
        }),
    )
    .await;
    assert_eq!(response["error"], "remote update requires expected_sha256");
}

#[test]
fn story_tool_refuses_an_unbound_caller() {
    let state = test_state();
    let result = handle_story(
        &state,
        &serde_json::json!({"input": {"action": "list_plans"}}),
        None,
    );
    assert_eq!(
        result["error"],
        "story requires a bound live managed session"
    );
}

// Catches: the MCP workflow envelope dropping the typed amendment or caller project.
#[cfg(unix)]
#[tokio::test]
async fn update_story_mcp_persists_priority_for_bound_caller() {
    let config = tempfile::tempdir_in(crate::test_support::test_temp_root()).unwrap();
    let _guard = crate::config::set_config_dir_override(config.path().to_path_buf());
    let project = tempfile::tempdir_in(crate::test_support::test_temp_root()).unwrap();
    let state = test_state();
    state
        .mcp
        .to_session
        .insert("update-story-mcp".into(), TEST_UUID_A.into());
    insert_managed_test_session(
        &state,
        "update-story-pty",
        &project.path().to_string_lossy(),
    );
    state.bind_live_pty(TEST_UUID_A, "update-story-pty");
    crate::repo_watcher::start_watching(&project.path().to_string_lossy(), &state).unwrap();
    let stories = crate::stories::StoryStore::open().unwrap();
    let plan = stories
        .create_plan(crate::stories::NewPlan {
            project: project
                .path()
                .canonicalize()
                .unwrap()
                .to_string_lossy()
                .into(),
            title: "Plan".into(),
            source: "feature.md".into(),
        })
        .unwrap();
    let story = stories
        .create_story(crate::stories::NewStory {
            plan_id: plan.id,
            title: "Story".into(),
            criteria: vec!["Done".into()],
            priority: 1,
            story_type: None,
            origin: crate::stories::StoryOrigin::Native,
            file_scope: vec![],
        })
        .unwrap();
    let result = handle_mcp_tool_call(
        &state,
        "127.0.0.1:12345".parse().unwrap(),
        "workflow",
        &serde_json::json!({"action": "story", "input": {
            "action": "update_story", "story_id": story.id,
            "expected_revision": story.revision, "patch": {"priority": 3}
        }}),
        Some("update-story-mcp"),
    )
    .await;
    assert_eq!(result["Ok"]["value"]["priority"], 3, "{result}");
    assert_eq!(stories.get_story(&story.id).unwrap().priority, 3);
}

#[cfg(unix)]
#[test]
fn story_tool_lists_plans_for_a_bound_caller() {
    #[cfg(not(feature = "desktop"))]
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .expect("test Tokio runtime");
    #[cfg(not(feature = "desktop"))]
    let _runtime_guard = runtime.enter();
    let config = tempfile::tempdir().expect("config directory");
    let _config_guard = crate::config::set_config_dir_override(config.path().to_path_buf());
    let project = tempfile::tempdir().expect("project directory");
    let state = test_state();
    let mcp_sid = "story-mcp";
    let tuic = TEST_UUID_A;
    state.mcp.to_session.insert(mcp_sid.into(), tuic.into());
    insert_managed_test_session(&state, "pty-story", &project.path().to_string_lossy());
    state.bind_live_pty(tuic, "pty-story");
    crate::repo_watcher::start_watching(&project.path().to_string_lossy(), &state)
        .expect("watch project");
    let created = handle_story(
        &state,
        &serde_json::json!({"input": {"action": "create_plan", "title": "Bound plan", "source": "plan.md"}}),
        Some(mcp_sid),
    );
    assert_eq!(created["Ok"]["type"], "plan", "unexpected result {created}");
    crate::stories::StoryStore::open()
        .expect("store")
        .create_plan(crate::stories::NewPlan {
            project: "/another/project".into(),
            title: "Foreign plan".into(),
            source: "plan.md".into(),
        })
        .expect("foreign plan");
    let result = handle_story(
        &state,
        &serde_json::json!({"input": {"action": "list_plans"}}),
        Some(mcp_sid),
    );
    assert_eq!(result["Ok"]["type"], "plans", "unexpected result {result}");
    let titles: Vec<_> = result["Ok"]["value"]
        .as_array()
        .expect("plan list")
        .iter()
        .map(|plan| plan["title"].as_str().expect("title"))
        .collect();
    assert_eq!(
        titles,
        ["Bound plan"],
        "the caller's tab project scopes the list"
    );
}

#[test]
fn workflow_tools_refuse_an_unbound_caller() {
    let state = test_state();
    let create = handle_workflow_story_create(&state, &serde_json::json!({"input": {}}), None);
    assert_eq!(
        create["error"],
        "workflow story creation requires a bound live managed session"
    );
    let report = handle_workflow_report(&state, &serde_json::json!({"input": {}}), None);
    assert_eq!(
        report["error"],
        "workflow report requires a bound live managed session"
    );
    let launch = handle_workflow_launch(
        &state,
        "127.0.0.1:0".parse().unwrap(),
        &serde_json::json!({"input": {
            "runId": "run",
            "attemptId": "attempt",
            "worktreePath": "/tmp/worktree",
            "agentType": "claude"
        }}),
        None,
    );
    assert_eq!(
        launch["error"],
        "workflow launch requires a bound live managed session"
    );
}

#[test]
fn workflow_coordinator_wake_buffers_only_a_durable_event_cursor() {
    let state = test_state();
    assert!(!queue_workflow_coordinator_wake(
        &state,
        TEST_UUID_A,
        "worker-pty",
        "run-1",
        "story-1",
        6,
    ));
    register_peer(&state, TEST_UUID_A, "coordinator", "mcp-coordinator");
    assert!(queue_workflow_coordinator_wake(
        &state,
        TEST_UUID_A,
        "worker-pty",
        "run-1",
        "story-1",
        7,
    ));
    let inbox = state.agent_inbox.get(TEST_UUID_A).unwrap();
    assert_eq!(inbox.len(), 1);
    let message = &inbox[0];
    assert_eq!(message.from_tuic_session, "worker-pty");
    assert_eq!(
        serde_json::from_str::<serde_json::Value>(&message.content).unwrap(),
        serde_json::json!({"type": "workflow_event", "runId": "run-1", "storyId": "story-1", "sequence": 7})
    );
}

#[tokio::test]
async fn progress_persists_before_emitting_and_each_report_is_its_own_entry() {
    let config = tempfile::tempdir().unwrap();
    let _config_guard = crate::config::set_config_dir_override(config.path().to_path_buf());
    let project = tempfile::tempdir().unwrap();
    let state = test_state();
    let mut events = state.event_bus.subscribe();
    let input = crate::progress::ProgressReportInput {
        kind: crate::progress::ProgressKind::Done,
        text: "Transport parity is verified.".to_string(),
        step: Some("Progress".to_string()),
    };

    let first = report_progress(
        &state,
        Some(&project.path().to_string_lossy()),
        input.clone(),
        Some("worker".to_string()),
        None,
        None,
        None,
    )
    .unwrap();

    let owner = project.path().canonicalize().unwrap();
    let owner = owner.to_string_lossy().to_string();
    let stored = crate::progress::ProgressStore::open()
        .unwrap()
        .list(&owner, &Default::default())
        .unwrap();
    assert_eq!(
        stored.entries.len(),
        1,
        "the entry must be durable before push"
    );
    assert_eq!(stored.entries[0].id, first.id);
    assert_eq!(stored.entries[0].agent_name.as_deref(), Some("worker"));

    let emitted = events.try_recv().expect("a recorded report must emit");
    match emitted {
        crate::state::AppEvent::ProgressRecorded { repo_path, payload } => {
            assert_eq!(repo_path, owner);
            assert_eq!(payload["entry"]["id"], first.id);
            assert_eq!(payload["entry"]["text"], "Transport parity is verified.");
        }
        other => panic!("expected ProgressRecorded, got {other:?}"),
    }

    // The journal is append-only and holds no dedup: the same agent
    // reporting the same step twice did the work twice, and the reader
    // decides what that means.
    let second = report_progress(
        &state,
        Some(&project.path().to_string_lossy()),
        input,
        Some("worker".to_string()),
        None,
        None,
        None,
    )
    .unwrap();
    assert_ne!(second.id, first.id);
    assert!(
        matches!(
            events.try_recv(),
            Ok(crate::state::AppEvent::ProgressRecorded { .. })
        ),
        "the second entry emits in its own right"
    );
}

#[cfg(unix)]
#[tokio::test]
async fn blocked_progress_emits_the_question_for_its_managed_session() {
    let config = tempfile::tempdir().unwrap();
    let _guard = crate::config::set_config_dir_override(config.path().to_path_buf());
    let project = tempfile::tempdir().unwrap();
    let state = test_state();
    insert_managed_test_session(&state, "asking-pty", &project.path().to_string_lossy());
    let mut events = state.event_bus.subscribe();

    report_progress(
        &state,
        Some(&project.path().to_string_lossy()),
        crate::progress::ProgressReportInput {
            kind: crate::progress::ProgressKind::Blocked,
            text: "Should I deploy now?".to_string(),
            step: None,
        },
        Some("worker".to_string()),
        Some("codex"),
        Some("asking-pty"),
        None,
    )
    .unwrap();

    assert!(matches!(
        events.try_recv(),
        Ok(crate::state::AppEvent::ProgressRecorded { .. })
    ));
    match events.try_recv() {
        Ok(crate::state::AppEvent::PtyParsed { session_id, parsed }) => {
            assert_eq!(session_id, "asking-pty");
            assert_eq!(parsed["type"], "question");
            assert_eq!(parsed["prompt_text"], "Should I deploy now?");
            assert_eq!(parsed["confident"], true);
        }
        other => panic!("expected explicit question after committed report, got {other:?}"),
    }
}

#[cfg(unix)]
fn report_for_pty(
    state: &Arc<AppState>,
    project: &str,
    kind: crate::progress::ProgressKind,
    text: &str,
    pty_id: &str,
) {
    report_progress(
        state,
        Some(project),
        crate::progress::ProgressReportInput {
            kind,
            text: text.to_string(),
            step: None,
        },
        Some("worker".to_string()),
        Some("codex"),
        Some(pty_id),
        None,
    )
    .unwrap();
}

// Catches: a hand-off journalled for the asking terminal (it spawned a
// child, so it is working) leaving its own blocked badge latched.
#[cfg(unix)]
#[tokio::test]
async fn blocked_progress_badge_clears_when_the_same_pty_delegates() {
    let config = tempfile::tempdir().unwrap();
    let _guard = crate::config::set_config_dir_override(config.path().to_path_buf());
    let project = tempfile::tempdir().unwrap();
    let project = project.path().to_string_lossy().to_string();
    let state = progress_badge_state(&project, &["codex-pty"]);

    report_for_pty(
        &state,
        &project,
        crate::progress::ProgressKind::Blocked,
        "Should I deploy now?",
        "codex-pty",
    );
    emit_progress_entry(
        &state,
        crate::progress::ProgressEntry {
            id: 99,
            project: project.clone(),
            pty_id: Some("codex-pty".to_string()),
            created_at_ms: 0,
            kind: crate::progress::ProgressKind::Delegated,
            text: "Review the parser".to_string(),
            step: None,
            agent_name: None,
            target_pty_id: Some("child-pty".to_string()),
            target_name: None,
        },
    );
    let row = settled_session(&state, "codex-pty").await;
    assert!(
        !row.awaiting_input,
        "delegating must supersede the blocked badge"
    );
}

#[cfg(unix)]
#[tokio::test]
async fn empty_blocked_progress_is_rejected_without_a_question_event() {
    let config = tempfile::tempdir().unwrap();
    let _guard = crate::config::set_config_dir_override(config.path().to_path_buf());
    let project = tempfile::tempdir().unwrap();
    let state = test_state();
    insert_managed_test_session(&state, "asking-pty", &project.path().to_string_lossy());
    let mut events = state.event_bus.subscribe();
    let result = report_progress(
        &state,
        Some(&project.path().to_string_lossy()),
        crate::progress::ProgressReportInput {
            kind: crate::progress::ProgressKind::Blocked,
            text: "   ".to_string(),
            step: None,
        },
        None,
        Some("codex"),
        Some("asking-pty"),
        None,
    );
    assert_eq!(result.unwrap_err(), "text must not be empty");
    assert!(events.try_recv().is_err());
}

#[tokio::test]
async fn blocked_acp_progress_reaches_the_conversation_without_a_pty_event() {
    let config = tempfile::tempdir().unwrap();
    let _guard = crate::config::set_config_dir_override(config.path().to_path_buf());
    let project = tempfile::tempdir().unwrap();
    let state = test_state();
    let mut events = state.event_bus.subscribe();
    let session_id = "01932d5e-0000-7000-8000-0000000000aa";

    report_progress(
        &state,
        Some(&project.path().to_string_lossy()),
        crate::progress::ProgressReportInput {
            kind: crate::progress::ProgressKind::Blocked,
            text: "Need approval for the next step".to_string(),
            step: None,
        },
        Some("ego".to_string()),
        None,
        None,
        Some(session_id),
    )
    .unwrap();

    match events.try_recv() {
        Ok(crate::state::AppEvent::ProgressRecorded { payload, .. }) => {
            assert_eq!(payload["acpSessionId"], session_id);
            assert_eq!(payload["entry"]["type"], "blocked");
            assert!(payload["entry"].get("ptyId").is_none());
        }
        other => panic!("expected conversation-scoped progress push, got {other:?}"),
    }
    assert!(events.try_recv().is_err(), "an ACP peer has no PTY to wake");
}

#[tokio::test]
async fn progress_mcp_attributes_the_registered_peer_and_its_project() {
    use crate::state::PeerAgent;

    let config = tempfile::tempdir().unwrap();
    let _config_guard = crate::config::set_config_dir_override(config.path().to_path_buf());
    let project = tempfile::tempdir().unwrap();
    let state = test_state();
    let mcp_sid = "progress-mcp".to_string();
    let tuic = "00000000-0000-0000-0000-000000000750".to_string();
    state.mcp.to_session.insert(mcp_sid.clone(), tuic.clone());
    state.peer_agents.insert(
        tuic.clone(),
        PeerAgent {
            tuic_session: tuic.clone(),
            mcp_session_id: mcp_sid.clone(),
            name: "progress-worker".to_string(),
            project: Some(project.path().to_string_lossy().to_string()),
            registered_at: 0,
        },
    );
    #[cfg(unix)]
    {
        insert_managed_test_session(&state, "pty-progress", &project.path().to_string_lossy());
        state.bind_live_pty(&tuic, "pty-progress");
    }

    let receipt = handle_progress(
        &state,
        &serde_json::json!({"type":"done", "text":"Delivery is complete."}),
        Some(&mcp_sid),
    )
    .await;
    assert!(receipt["id"].is_i64(), "unexpected receipt {receipt}");
    let owner = project.path().canonicalize().unwrap();
    let stored = crate::progress::ProgressStore::open()
        .unwrap()
        .list(&owner.to_string_lossy(), &Default::default())
        .unwrap();
    assert_eq!(
        stored.entries[0].agent_name.as_deref(),
        Some("progress-worker")
    );
    assert_eq!(stored.entries[0].project, owner.to_string_lossy());
    #[cfg(unix)]
    assert_eq!(stored.entries[0].pty_id.as_deref(), Some("pty-progress"));
}

/// The only reportable kinds are `done` and `blocked`. `intent` belongs to
/// the host: an agent that claims one is refused rather than believed.
#[tokio::test]
async fn an_agent_cannot_report_an_intent() {
    let state = test_state();
    let receipt = handle_progress(
        &state,
        &serde_json::json!({"type":"intent", "text":"I will pretend to plan."}),
        None,
    )
    .await;
    let error = receipt["error"].as_str().unwrap_or_default();
    assert!(
        error.contains("'done' or 'blocked'") && error.contains("intent:"),
        "the refusal must name both the allowed kinds and who writes intent: {receipt}"
    );
}

/// `sound` is what an unattended agent uses to reach a user who is not
/// watching, so every accepted form must resolve to a real notification
/// sound: silence is silence, `true` still means "match the level", and a
/// name (notably `attention`) overrides it.
#[test]
fn toast_sound_resolves_to_a_notification_sound_or_silence() {
    use serde_json::json;
    for (value, level, expected) in [
        (json!(null), "info", None),
        (json!(false), "error", None),
        (json!(true), "info", Some("info")),
        (json!(true), "warn", Some("warning")),
        (json!(true), "error", Some("error")),
        (json!("attention"), "info", Some("attention")),
        (json!("question"), "error", Some("question")),
    ] {
        assert_eq!(
            resolve_toast_sound(&value, level).expect("valid sound"),
            expected.map(str::to_string),
            "sound={value} level={level}"
        );
    }
}

/// A typo must not silently produce a silent toast — the agent would believe
/// it had rung a bell that never rang.
#[test]
fn toast_sound_rejects_unknown_names() {
    let err = resolve_toast_sound(&serde_json::json!("buzzer"), "info")
        .expect_err("unknown sound must be rejected");
    let message = err["error"].as_str().expect("error message");
    assert!(
        message.contains("attention"),
        "lists the valid names: {message}"
    );
    assert!(resolve_toast_sound(&serde_json::json!(3), "info").is_err());
}

#[test]
fn ui_tab_emits_event() {
    let state = test_state();
    let mut rx = state.event_bus.subscribe();

    let result = handle_ui(
        &state,
        &serde_json::json!({
            "action": "tab",
            "id": "test-panel",
            "title": "Test",
            "html": "<p>hello</p>"
        }),
        None,
    );
    assert_eq!(result["ok"], true);
    assert_eq!(result["id"], "test-panel");

    let event = rx.try_recv().expect("Expected UiTab event");
    match event {
        crate::state::AppEvent::UiTab {
            id,
            title,
            html,
            url,
            pinned,
            focus,
            origin_repo_path,
            ..
        } => {
            assert_eq!(id, "test-panel");
            assert_eq!(title, "Test");
            assert_eq!(html, "<p>hello</p>");
            assert!(url.is_none(), "url should be None for html tab");
            assert!(!pinned, "pinned should default to false");
            assert!(!focus, "focus should default to false");
            assert!(
                origin_repo_path.is_none(),
                "origin_repo_path should be None when no mcp_session"
            );
        }
        other => panic!("Expected UiTab, got {:?}", other),
    }
}

#[test]
fn ui_tab_includes_origin_repo_path_from_peer_agent() {
    use crate::state::PeerAgent;
    let state = test_state();
    let mcp_sid = "mcp-xyz".to_string();
    let tuic = "00000000-0000-0000-0000-000000000001".to_string();
    // Register an MCP→tuic mapping and a peer agent with a project path.
    state.mcp.to_session.insert(mcp_sid.clone(), tuic.clone());
    state.peer_agents.insert(
        tuic.clone(),
        PeerAgent {
            tuic_session: tuic.clone(),
            mcp_session_id: mcp_sid.clone(),
            name: "wiz".to_string(),
            project: Some("/Gits/personal/alpha".to_string()),
            registered_at: 0,
        },
    );

    let mut rx = state.event_bus.subscribe();
    let result = handle_ui(
        &state,
        &serde_json::json!({
            "action": "tab",
            "id": "mcf",
            "title": "MCF",
            "html": "<p/>"
        }),
        Some(&mcp_sid),
    );
    assert_eq!(result["ok"], true);

    let event = rx.try_recv().expect("Expected UiTab event");
    match event {
        crate::state::AppEvent::UiTab {
            origin_repo_path, ..
        } => {
            assert_eq!(
                origin_repo_path.as_deref(),
                Some("/Gits/personal/alpha"),
                "caller's repo path must be propagated so the tab lands in the right repo"
            );
        }
        other => panic!("Expected UiTab, got {:?}", other),
    }
}

#[test]
#[ignore = "requires real PTY (openpty) — fails in sandboxed CI; covered by integration tests"]
fn ui_tab_falls_back_to_pty_cwd_when_no_peer_agent() {
    use crate::state::PtySession;
    use portable_pty::{PtySize, native_pty_system};

    let state = test_state();
    let mcp_sid = "mcp-no-peer".to_string();
    let tuic = "00000000-0000-0000-0000-000000000002".to_string();
    state.mcp.to_session.insert(mcp_sid.clone(), tuic.clone());

    // Spawn a minimal PTY session with cwd set so we can exercise the fallback.
    let pty_system = native_pty_system();
    let pair = pty_system
        .openpty(PtySize {
            rows: 24,
            cols: 80,
            pixel_width: 0,
            pixel_height: 0,
        })
        .expect("openpty");
    let mut cmd = portable_pty::CommandBuilder::new("true");
    cmd.cwd("/tmp");
    let child = pair.slave.spawn_command(cmd).expect("spawn");
    let writer = pair.master.take_writer().expect("writer");
    state.session_maps.sessions.insert(
        tuic.clone(),
        parking_lot::Mutex::new(PtySession {
            launch_receipt: None,
            writer: Arc::new(parking_lot::Mutex::new(writer)),
            master: pair.master,
            _child: child,
            paused: std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false)),
            worktree: None,
            initial_cwd: Some("/Gits/personal/beta".to_string()),
            cwd: Some("/Gits/personal/beta".to_string()),
            display_name: None,
            display_name_is_custom: false,
            display_name_from_spawn: false,
            is_remote: false,
            shell: "true".to_string(),
        }),
    );

    let mut rx = state.event_bus.subscribe();
    handle_ui(
        &state,
        &serde_json::json!({
            "action": "tab",
            "id": "beta-tab",
            "title": "Beta",
            "html": "<p/>"
        }),
        Some(&mcp_sid),
    );

    let event = rx.try_recv().expect("Expected UiTab event");
    match event {
        crate::state::AppEvent::UiTab {
            origin_repo_path, ..
        } => {
            assert_eq!(origin_repo_path.as_deref(), Some("/Gits/personal/beta"));
        }
        other => panic!("Expected UiTab, got {:?}", other),
    }
}

/// A hand-opened tab registers under its `$TUIC_SESSION`, while its PTY is
/// keyed by the UUID `create_pty` minted. The resolver must bridge the two:
/// looking `sessions` up under the peer id finds nothing, which is how
/// `progress` came to refuse every report from a tab the user opened
/// himself with `project_required` — peer `tu-5`, cwd
/// `/Users/stefano.straus/Gits/personal/tuicommander`, observed 2026-09-20.
#[cfg(unix)]
#[test]
fn origin_repo_path_resolves_a_pty_keyed_differently_from_the_peer() {
    let state = test_state();
    let mcp_sid = "mcp-hand-opened-tab";
    let tuic = "91136da9-2b9c-4c7f-95a1-d615454ba760";
    let pty_key = "1c052428-3a90-4e6c-8c6b-e7e311c546f3";

    crate::state::tests_support::insert_dummy_session(&state, pty_key);
    crate::state::tests_support::set_session_cwd(&state, pty_key, "/Gits/personal/delta");
    state
        .session_maps
        .live_pty_by_tuic_session
        .insert(tuic.to_string(), pty_key.to_string());
    state
        .mcp
        .to_session
        .insert(mcp_sid.to_string(), tuic.to_string());

    assert_eq!(
        resolve_mcp_origin_repo_path(&state, Some(mcp_sid)).as_deref(),
        Some("/Gits/personal/delta"),
        "the peer's live PTY holds the cwd under its own key — resolve it, do not assume the keys match"
    );
}

/// Hands-free binds the PTY the user picked, keyed by the UUID
/// `create_pty` minted; the MCP connection names the caller by its
/// `$TUIC_SESSION`. Compared as they arrive, a hand-opened tab never
/// matched its own binding and `voice` answered "Speech is bound to
/// another session" to the very terminal it was armed for — tab
/// `a171e425…`, PTY `2015957f…`, observed 2026-09-23.
#[cfg(unix)]
#[test]
fn origin_pty_resolves_a_hand_opened_tab_to_its_own_pty() {
    let state = test_state();
    let mcp_sid = "mcp-hand-opened-voice-tab";
    let tuic = "a171e425-ddbd-47bf-804a-a7f3f7ffd474";
    let pty_key = "2015957f-705a-49ee-a344-329f6ee68641";

    crate::state::tests_support::insert_dummy_session(&state, pty_key);
    state
        .session_maps
        .live_pty_by_tuic_session
        .insert(tuic.to_string(), pty_key.to_string());
    state
        .mcp
        .to_session
        .insert(mcp_sid.to_string(), tuic.to_string());

    assert_eq!(
        resolve_mcp_origin_pty(&state, Some(mcp_sid)).as_deref(),
        Some(pty_key),
        "the binding holds the PTY key, so the caller must be named by it too"
    );
}

#[test]
fn origin_pty_is_none_for_a_connection_with_no_live_terminal() {
    let state = test_state();
    state
        .mcp
        .to_session
        .insert("mcp-orphan".to_string(), "no-such-tab".to_string());

    assert_eq!(resolve_mcp_origin_pty(&state, Some("mcp-orphan")), None);
    assert_eq!(resolve_mcp_origin_pty(&state, None), None);
}

#[test]
fn ui_tab_falls_back_to_mcp_session_repo_path() {
    let state = test_state();
    let mcp_sid = "mcp-no-peer-no-pty".to_string();
    state.mcp.sessions.insert(
        mcp_sid.clone(),
        crate::state::McpSessionMeta {
            prompt_instructions: None,
            last_activity: std::time::Instant::now(),
            is_claude_code: true,
            requires_meta_tools: false,
            has_sse_stream: false,
            sse_generation: 0,
            repo_path: Some("/Gits/personal/gamma".to_string()),
        },
    );

    let mut rx = state.event_bus.subscribe();
    let result = handle_ui(
        &state,
        &serde_json::json!({
            "action": "tab",
            "id": "gamma-tab",
            "title": "Gamma",
            "html": "<p/>"
        }),
        Some(&mcp_sid),
    );
    assert_eq!(result["ok"], true);

    let event = rx.try_recv().expect("Expected UiTab event");
    match event {
        crate::state::AppEvent::UiTab {
            origin_repo_path, ..
        } => {
            assert_eq!(
                origin_repo_path.as_deref(),
                Some("/Gits/personal/gamma"),
                "should fall back to mcp_sessions repo_path when no peer agent or PTY session"
            );
        }
        other => panic!("Expected UiTab, got {:?}", other),
    }
}

// Catches: desktop receives opener metadata but the browser/mobile SSE payload drops it.
#[test]
fn ui_tab_carries_opener_identity_on_both_transports() {
    let state = test_state();
    state.mcp.to_session.insert("mcp-b".into(), "tuic-b".into());
    let mut rx = state.event_bus.subscribe();
    let result = handle_ui(
        &state,
        &serde_json::json!({
            "action": "tab", "id": "review", "title": "Review", "url": "tuic://open/review.md"
        }),
        Some("mcp-b"),
    );
    assert_eq!(result["ok"], true);
    let event = rx.try_recv().expect("ui-tab event");
    assert_eq!(
        serde_json::to_value(&event).unwrap()["payload"]["opener_tuic_session"],
        "tuic-b"
    );
    assert_eq!(
        super::super::sse_routes::event_payload_for_test(&event)["opener_tuic_session"],
        "tuic-b"
    );
}

#[test]
fn ui_tab_requires_fields() {
    let state = test_state();
    let r = handle_ui(&state, &serde_json::json!({"action": "tab"}), None);
    assert!(r["error"].as_str().unwrap().contains("'id'"));

    let r = handle_ui(
        &state,
        &serde_json::json!({"action": "tab", "id": "x"}),
        None,
    );
    assert!(r["error"].as_str().unwrap().contains("'title'"));

    // Requires either html or url — url is accepted as alternative to html
    let r = handle_ui(
        &state,
        &serde_json::json!({"action": "tab", "id": "x", "title": "t"}),
        None,
    );
    assert!(r["error"].as_str().unwrap().contains("'html' or 'url'"));

    // url alone is accepted
    let r = handle_ui(
        &state,
        &serde_json::json!({"action": "tab", "id": "x", "title": "t", "url": "http://localhost/"}),
        None,
    );
    assert_eq!(r["ok"], true);

    // Both html and url is rejected
    let r = handle_ui(
        &state,
        &serde_json::json!({"action": "tab", "id": "x", "title": "t", "html": "<p/>", "url": "http://localhost/"}),
        None,
    );
    assert!(r["error"].as_str().unwrap().contains("not both"));
}

#[test]
fn ui_tab_focus_true_is_respected() {
    let state = test_state();
    let mut rx = state.event_bus.subscribe();

    handle_ui(
        &state,
        &serde_json::json!({"action": "tab", "id": "fg", "title": "Foreground", "html": "<p/>", "focus": true}),
        None,
    );

    match rx.try_recv().expect("Expected UiTab event") {
        crate::state::AppEvent::UiTab { focus, .. } => {
            assert!(focus, "explicit focus=true should navigate");
        }
        other => panic!("Expected UiTab, got {:?}", other),
    }
}

#[test]
fn ui_tab_focus_false() {
    let state = test_state();
    let mut rx = state.event_bus.subscribe();

    handle_ui(
        &state,
        &serde_json::json!({
            "action": "tab",
            "id": "bg",
            "title": "Background",
            "html": "<p/>",
            "focus": false
        }),
        None,
    );

    let event = rx.try_recv().expect("Expected UiTab event");
    match event {
        crate::state::AppEvent::UiTab { focus, .. } => {
            assert!(!focus, "focus=false should be respected");
        }
        other => panic!("Expected UiTab, got {:?}", other),
    }
}

#[test]
fn ui_tab_pinned_false() {
    let state = test_state();
    let mut rx = state.event_bus.subscribe();

    handle_ui(
        &state,
        &serde_json::json!({
            "action": "tab",
            "id": "unpinned",
            "title": "T",
            "html": "<p/>",
            "pinned": false
        }),
        None,
    );

    let event = rx.try_recv().expect("Expected UiTab event");
    match event {
        crate::state::AppEvent::UiTab { pinned, .. } => {
            assert!(!pinned);
        }
        other => panic!("Expected UiTab, got {:?}", other),
    }
}

// -------- HTML tab lifecycle tests (story 1176-b88b) --------

#[test]
fn ui_tab_warns_when_session_already_has_terminal() {
    use crate::state::VtLogBuffer;
    let state = test_state();
    // Simulate an active session by inserting into vt_log_buffers
    state.grid.vt_log_buffers.insert(
        "sess-active".to_string(),
        parking_lot::Mutex::new(VtLogBuffer::new(24, 220, 500)),
    );

    // Calling ui(tab) with session_id = active session should warn, not create tab
    let r = handle_ui(
        &state,
        &serde_json::json!({
            "action": "tab",
            "id": "status-tab",
            "title": "Status",
            "html": "<p>status</p>",
            "session_id": "sess-active"
        }),
        None,
    );
    assert!(
        r.get("warning").and_then(|v| v.as_str()).is_some(),
        "should return warning when session_id has an active terminal"
    );
    assert_eq!(
        r["ok"],
        serde_json::json!(false),
        "should not create tab when session already has terminal"
    );
}

#[test]
fn ui_tab_no_warning_without_session_id() {
    let state = test_state();
    // No session_id → normal tab creation, no warning
    let r = handle_ui(
        &state,
        &serde_json::json!({
            "action": "tab",
            "id": "standalone-tab",
            "title": "My Tab",
            "html": "<p>hello</p>"
        }),
        None,
    );
    assert_eq!(r["ok"], serde_json::json!(true));
    assert!(r.get("warning").is_none());
}

#[test]
fn ui_tab_no_warning_for_unknown_session_id() {
    let state = test_state();
    // session_id refers to a session that doesn't exist → no warning, tab created normally
    let r = handle_ui(
        &state,
        &serde_json::json!({
            "action": "tab",
            "id": "status-tab",
            "title": "Status",
            "html": "<p>hi</p>",
            "session_id": "nonexistent-session"
        }),
        None,
    );
    assert_eq!(
        r["ok"],
        serde_json::json!(true),
        "nonexistent session_id should not block tab creation"
    );
}

#[test]
fn ui_tab_registers_creator_and_clears_on_session_close() {
    use crate::state::VtLogBuffer;
    let state = test_state();
    register_peer(
        &state,
        "550e8400-e29b-41d4-a716-446655440b02",
        "orchestrator",
        "mcp-orch",
    );
    // Map mcp_session_id → tuic_session
    state.mcp.to_session.insert(
        "mcp-orch".to_string(),
        "550e8400-e29b-41d4-a716-446655440b02".to_string(),
    );

    // Create HTML tab as orchestrator
    let r = handle_ui(
        &state,
        &serde_json::json!({
            "action": "tab",
            "id": "orch-status",
            "title": "Orchestrator",
            "html": "<p>running</p>"
        }),
        Some("mcp-orch"),
    );
    assert_eq!(r["ok"], serde_json::json!(true));

    // session_html_tabs should have the tab registered under the creator's session
    let tabs = state
        .session_maps
        .session_html_tabs
        .get("550e8400-e29b-41d4-a716-446655440b02");
    assert!(
        tabs.is_some(),
        "tab should be registered under creator session"
    );
    assert!(tabs.unwrap().contains(&"orch-status".to_string()));

    // Insert vt_log_buffers so close succeeds
    state.grid.vt_log_buffers.insert(
        "550e8400-e29b-41d4-a716-446655440b02".to_string(),
        parking_lot::Mutex::new(VtLogBuffer::new(24, 220, 500)),
    );
    // Close the session — should clear its html tabs
    handle_session(
        &state,
        &serde_json::json!({"action": "close", "session_id": "550e8400-e29b-41d4-a716-446655440b02"}),
        None,
    );

    assert!(
        state
            .session_maps
            .session_html_tabs
            .get("550e8400-e29b-41d4-a716-446655440b02")
            .is_none(),
        "session_html_tabs should be cleared after session close"
    );
}

/// A tab id is a dedup key — `ui action=tab` with an id it already used
/// UPDATES that tab, it does not open a second one. Registering the id again
/// therefore has to be a no-op, and an agent that opens tabs all day must
/// not grow the close list without bound: this vector lives until the
/// session exits, and every entry is replayed into `close-html-tabs`.
#[test]
fn ui_tab_registration_dedupes_and_caps_per_session() {
    let tuic = "550e8400-e29b-41d4-a716-446655440b03";
    let state = test_state();
    register_peer(&state, tuic, "capper", "mcp-cap");
    state
        .mcp
        .to_session
        .insert("mcp-cap".to_string(), tuic.to_string());

    let open = |id: String| {
        handle_ui(
            &state,
            &serde_json::json!({
                "action": "tab",
                "id": id,
                "title": "T",
                "html": "<p>x</p>"
            }),
            Some("mcp-cap"),
        )
    };

    for _ in 0..5 {
        assert_eq!(open("same".to_string())["ok"], serde_json::json!(true));
    }
    assert_eq!(
        state
            .session_maps
            .session_html_tabs
            .get(tuic)
            .unwrap()
            .len(),
        1,
        "re-opening the same tab id must register it once"
    );

    for i in 0..SESSION_HTML_TAB_LIMIT + 4 {
        open(format!("tab-{i}"));
    }
    let tabs = state.session_maps.session_html_tabs.get(tuic).unwrap();
    assert_eq!(
        tabs.len(),
        SESSION_HTML_TAB_LIMIT,
        "registration must be capped"
    );
    assert!(
        tabs.contains(&format!("tab-{}", SESSION_HTML_TAB_LIMIT + 3)),
        "the newest tab must survive eviction"
    );
    assert!(
        !tabs.contains(&"same".to_string()),
        "eviction must drop the oldest registration first"
    );
}

// --- ui(action=confirm): answerable from any client, not just the desktop ---

/// Story 760: `REQUEST_TIMEOUT` (`mcp_http/mod.rs`) wraps the whole router,
/// including this handler, through `with_server_limits`. Both constants used
/// to read 300s — a tie the scheduler could resolve either way — so the
/// outer layer could win and hand the caller a bare 408 instead of this
/// handler's own `{confirmed:false, reason:...}` body. Asserting the
/// ordering directly means a future edit that reintroduces a tie, or drops
/// REQUEST_TIMEOUT below CONFIRM_TIMEOUT, fails the build rather than only
/// production.
#[test]
fn request_timeout_beats_every_in_handler_deadline() {
    assert!(
        crate::mcp_http::REQUEST_TIMEOUT > CONFIRM_TIMEOUT,
        "the router's outer timeout must never race the confirm handler's own \
             deadline: REQUEST_TIMEOUT={:?}, CONFIRM_TIMEOUT={:?}",
        crate::mcp_http::REQUEST_TIMEOUT,
        CONFIRM_TIMEOUT,
    );
    assert!(
        crate::mcp_http::REQUEST_TIMEOUT.as_millis() as u64 > WAIT_EFFECTIVE_MAX_MS,
        "the router's outer timeout must also clear the longest session/agent \
             wait this server actually runs: REQUEST_TIMEOUT={:?}, WAIT_EFFECTIVE_MAX_MS={}ms",
        crate::mcp_http::REQUEST_TIMEOUT,
        WAIT_EFFECTIVE_MAX_MS,
    );
    // `/acp/one-shot` is the third, and the one whose deadline is a SUM.
    // `TURN_TIMEOUT` bounds the turn only; the ego launch and `initialize`
    // happen before it and are bounded by `INITIALIZE_TIMEOUT`. Comparing
    // the turn alone is how the handler came to be able to overshoot: at
    // 300s it already equalled the router, and every millisecond of launch
    // pushed the total past it, so a slow turn answered a bare 408 instead
    // of "ego did not finish the turn within Ns" (#804-2ec8).
    let one_shot = crate::acp::INITIALIZE_TIMEOUT + crate::acp::oneshot::TURN_TIMEOUT;
    assert!(
        crate::mcp_http::REQUEST_TIMEOUT > one_shot,
        "the router's outer timeout must clear the whole unattended turn, launch \
             included: REQUEST_TIMEOUT={:?}, INITIALIZE_TIMEOUT + TURN_TIMEOUT={:?}",
        crate::mcp_http::REQUEST_TIMEOUT,
        one_shot,
    );
}

/// Wait for `handle_confirm` to publish its request, and return the id.
///
/// The handler registers the pending entry before it awaits, so polling the
/// registry is enough — no need to reach into the event bus for the id.
async fn await_pending_confirm(state: &Arc<AppState>) -> String {
    for _ in 0..200 {
        if let Some(entry) = state.confirm_responses.iter().next() {
            return entry.key().clone();
        }
        tokio::task::yield_now().await;
    }
    panic!("confirm request was never registered");
}

#[tokio::test]
async fn confirm_returns_the_answer_a_client_gave() {
    let state = test_state();
    let addr = "127.0.0.1:0".parse().unwrap();

    let answering = state.clone();
    tokio::spawn(async move {
        let id = await_pending_confirm(&answering).await;
        crate::mcp_http::resolve_mcp_confirm(&answering, &id, true);
    });

    let result = handle_confirm(
        &state,
        addr,
        &serde_json::json!({"title": "Delete branch?", "message": "git branch -D wip"}),
        None,
    )
    .await;

    assert_eq!(result["confirmed"], true);
    assert!(
        result.get("reason").is_none(),
        "a real answer must not be reported as a timeout"
    );
}

#[tokio::test]
async fn confirm_reaches_every_client_over_the_event_bus() {
    let state = test_state();
    let addr = "127.0.0.1:0".parse().unwrap();
    // Subscribing before the call is what a remote SSE client does; without
    // this bus hop a phone would never learn an agent is waiting on it.
    let mut rx = state.event_bus.subscribe();

    let answering = state.clone();
    tokio::spawn(async move {
        let id = await_pending_confirm(&answering).await;
        crate::mcp_http::resolve_mcp_confirm(&answering, &id, false);
    });

    let _ = handle_confirm(
        &state,
        addr,
        &serde_json::json!({"title": "Force push?", "message": "to main"}),
        None,
    )
    .await;

    let mut saw_request = None;
    let mut saw_resolution = false;
    while let Ok(event) = rx.try_recv() {
        match event {
            crate::state::AppEvent::McpConfirm {
                request_id,
                title,
                message,
                ..
            } => {
                assert_eq!(title, "Force push?");
                assert_eq!(message, "to main");
                saw_request = Some(request_id);
            }
            crate::state::AppEvent::McpConfirmResolved {
                request_id,
                confirmed,
            } => {
                assert_eq!(Some(&request_id), saw_request.as_ref());
                assert!(!confirmed);
                saw_resolution = true;
            }
            _ => {}
        }
    }
    assert!(saw_request.is_some(), "no request reached the bus");
    assert!(
        saw_resolution,
        "clients that did not answer are never told to dismiss the dialog"
    );
}

#[tokio::test(start_paused = true)]
async fn confirm_gives_up_when_nobody_answers() {
    let state = test_state();
    let addr = "127.0.0.1:0".parse().unwrap();

    let result = handle_confirm(
        &state,
        addr,
        &serde_json::json!({"title": "Drop the table?"}),
        None,
    )
    .await;

    // Silence is not consent: a destructive op nobody approved must read as
    // refused, and say why so the caller can tell it from a real "no".
    assert_eq!(result["confirmed"], false);
    assert!(
        result["reason"]
            .as_str()
            .is_some_and(|r| r.contains("no answer")),
        "timeout must be distinguishable from a refusal: {result}"
    );
    assert!(
        state.confirm_responses.is_empty(),
        "an expired request must not leak its registry entry"
    );
}

#[tokio::test]
async fn answering_a_confirm_twice_resolves_it_once() {
    let state = test_state();
    let addr = "127.0.0.1:0".parse().unwrap();
    let mut rx = state.event_bus.subscribe();

    let answering = state.clone();
    tokio::spawn(async move {
        let id = await_pending_confirm(&answering).await;
        // Every client races to answer the same request. The loser must be a
        // no-op, not an error and not a second resolution.
        crate::mcp_http::resolve_mcp_confirm(&answering, &id, true);
        crate::mcp_http::resolve_mcp_confirm(&answering, &id, false);
    });

    let result = handle_confirm(
        &state,
        addr,
        &serde_json::json!({"title": "Proceed?"}),
        None,
    )
    .await;

    assert_eq!(result["confirmed"], true);
    let resolutions = std::iter::from_fn(|| rx.try_recv().ok())
        .filter(|e| matches!(e, crate::state::AppEvent::McpConfirmResolved { .. }))
        .count();
    assert_eq!(resolutions, 1, "the losing answer must not re-broadcast");
}

#[tokio::test]
async fn confirm_refuses_a_non_loopback_caller() {
    let state = test_state();
    let addr = "192.168.1.50:9876".parse().unwrap();

    let result = handle_confirm(
        &state,
        addr,
        &serde_json::json!({"title": "Proceed?"}),
        None,
    )
    .await;

    assert!(
        result["error"]
            .as_str()
            .is_some_and(|e| e.contains("localhost"))
    );
    assert!(state.confirm_responses.is_empty());
}

#[tokio::test]
async fn confirm_requires_a_title() {
    let state = test_state();
    let addr = "127.0.0.1:0".parse().unwrap();

    let result = handle_confirm(
        &state,
        addr,
        &serde_json::json!({"message": "no title"}),
        None,
    )
    .await;

    assert!(
        result["error"]
            .as_str()
            .is_some_and(|e| e.contains("title"))
    );
}

#[cfg(feature = "desktop")]
#[tokio::test]
async fn ui_screenshot_times_out_without_frontend() {
    let state = test_state();
    // No frontend listener, so the screenshot request will time out.
    // Override timeout to 1s to keep the test fast.
    let r = handle_screenshot(
        &state,
        loopback_addr(),
        &serde_json::json!({ "id": "nonexistent-panel" }),
    )
    .await;
    let err = r["error"].as_str().expect("should return error");
    assert!(
        err.contains("timed out") || err.contains("not available"),
        "Expected timeout or not-available error, got: {err}"
    );
}

#[cfg(feature = "desktop")]
#[tokio::test]
async fn ui_screenshot_channel_delivers_result() {
    let state = test_state();
    let panel_id = "test-panel";

    // Simulate: spawn a task that waits for a screenshot_responses entry
    // and delivers fake base64 data.
    let state2 = state.clone();
    let deliver = tokio::spawn(async move {
        // Poll for the channel to appear
        for _ in 0..50 {
            tokio::time::sleep(std::time::Duration::from_millis(20)).await;
            if let Some((_, sender)) = state2.screenshot_responses.remove("") {
                // Won't match — we need the actual request_id.
                state2.screenshot_responses.insert(String::new(), sender);
            }
            // Check all entries
            let keys: Vec<_> = state2
                .screenshot_responses
                .iter()
                .map(|e| e.key().clone())
                .collect();
            for key in keys {
                if let Some((_, sender)) = state2.screenshot_responses.remove(&key) {
                    // 1x1 white WebP (minimal valid WebP)
                    let fake_b64 =
                        base64::engine::general_purpose::STANDARD.encode(b"\x00\x00\x00\x00");
                    let _ = sender.send(Some(fake_b64));
                    return;
                }
            }
        }
    });

    let r = handle_screenshot(
        &state,
        loopback_addr(),
        &serde_json::json!({ "id": panel_id }),
    )
    .await;
    deliver.await.unwrap();

    // Should succeed (write file) or at least not be a timeout
    if let Some(err) = r["error"].as_str() {
        assert!(
            !err.contains("timed out"),
            "Should not have timed out with a responding channel, got: {err}"
        );
    } else {
        assert!(
            r["ok"].as_bool().unwrap_or(false),
            "Expected ok: true, got: {r}"
        );
        assert!(
            r["path"].as_str().is_some(),
            "Expected path in result, got: {r}"
        );
    }
}
// Catches: an MCP write reaching the provider before the user opts in.
#[tokio::test]
async fn work_item_act_disabled_changes_nothing() {
    let state = test_state();
    let mut events = state.event_bus.subscribe();
    let response = handle_repo(&state, &serde_json::json!({
        "action": "work_item_act", "reference": {"providerId":"demo","accountId":"local","id":"1"},
        "operation": {"kind":"comment","value":"must not be sent"}
    }), false).await;
    assert_eq!(
        response["error"],
        "Work-item writes are disabled; enable mcp_work_item_writes to opt in"
    );
    assert!(
        events.try_recv().is_err(),
        "disabled writes must not dispatch to any provider"
    );
}

// Catches: provider metadata carrying an account credential into the model payload.
#[tokio::test]
async fn work_item_rejects_credentials_in_provider_payload() {
    use tower::ServiceExt;
    let state = test_state();
    let mut events = state.event_bus.subscribe();
    let args = serde_json::json!({"action":"work_item", "reference":{"providerId":"demo","accountId":"local","id":"1"}});
    let call = handle_repo(&state, &args, false);
    tokio::pin!(call);
    let response = tokio::select! {
        response = &mut call => response,
        event = events.recv() => {
            let event = event.unwrap();
            let payload = super::super::sse_routes::event_payload_for_test(&event);
            let router = super::super::build_router(state.clone(), false, true);
            let post = |body: serde_json::Value| {
                axum::http::Request::builder().method("POST").uri("/work-items/provider-bridge")
                    .header("content-type", "application/json")
                    .extension(axum::extract::ConnectInfo("127.0.0.1:12345".parse::<std::net::SocketAddr>().unwrap()))
                    .body(axum::body::Body::from(body.to_string())).unwrap()
            };
            let claimed = router.clone().oneshot(post(serde_json::json!({"requestId":payload["request_id"],"action":"claim"}))).await.unwrap();
            assert_eq!(claimed.status(), axum::http::StatusCode::OK);
            let mut detail = serde_json::from_str::<serde_json::Value>(include_str!("../work_items/demo.json")).unwrap()["items"][0].clone();
            detail["token"] = "account-secret-sentinel".into();
            let reply = router.oneshot(post(serde_json::json!({"requestId":payload["request_id"],"action":"complete","value":detail}))).await.unwrap();
            assert_eq!(reply.status(), axum::http::StatusCode::OK);
            call.await
        }
    };
    assert_eq!(response["error"], "Invalid work-item provider response");
    assert!(!response.to_string().contains("account-secret-sentinel"));
}

// Catches: proposal_create routed to ordinary create_story, losing retry deduplication.
#[cfg(unix)]
#[tokio::test]
async fn workflow_proposal_create_retries_create_one_story() {
    use crate::stories::{NewPlan, NewStory, StoryOrigin, StoryStore};
    use crate::workflows::{
        AgentBinding, EffectKind, RunCommand, RunLimits, RunStore, WorkflowKind, WorkflowStore,
    };
    let config = tempfile::tempdir().unwrap();
    let _guard = crate::config::set_config_dir_override(config.path().to_path_buf());
    let project = tempfile::tempdir().unwrap();
    let path = project
        .path()
        .canonicalize()
        .unwrap()
        .to_string_lossy()
        .to_string();
    let state = test_state();
    state
        .mcp
        .to_session
        .insert("proposal-mcp".into(), TEST_UUID_A.into());
    insert_managed_test_session(&state, "proposal-pty", &path);
    state.bind_live_pty(TEST_UUID_A, "proposal-pty");
    crate::repo_watcher::start_watching(&path, &state).unwrap();
    let stories = StoryStore::open().unwrap();
    let plan = stories
        .create_plan(NewPlan {
            project: path.clone(),
            title: "Proposals".into(),
            source: "plan.md".into(),
        })
        .unwrap();
    let template = WorkflowStore::open()
        .unwrap()
        .seed_templates(&path)
        .unwrap()
        .into_iter()
        .find(|d| d.kind == WorkflowKind::Plan)
        .unwrap();
    let runs = RunStore::open().unwrap();
    let run = runs
        .start_plan(&path, &plan.id, &template.id, 1, RunLimits::default())
        .unwrap();
    let started = runs
        .command(
            &run.id,
            "coordinate",
            RunCommand::StartPlanAgent {
                node_id: "coordinate".into(),
            },
        )
        .unwrap();
    let attempt = started.snapshot.attempts.last().unwrap();
    let reserved = runs
        .command(
            &run.id,
            "spawn",
            RunCommand::ReserveEffect {
                key: format!("spawn:{}", attempt.id),
                kind: EffectKind::SpawnAgent,
            },
        )
        .unwrap();
    runs.bind_agent(
        &run.id,
        &attempt.id,
        AgentBinding {
            session_id: "proposal-pty".into(),
            task_id: None,
            effect_id: reserved.snapshot.effects.last().unwrap().id.clone(),
            prompt_contract_version: 1,
            prompt_sha256: "a".repeat(64),
            audit_preview: "coordinate".into(),
        },
    )
    .unwrap();
    let story = NewStory {
        story_type: None,
        plan_id: plan.id.clone(),
        title: "One proposal".into(),
        criteria: vec!["Delivered".into()],
        priority: 2,
        origin: StoryOrigin::PlanStep { step: "one".into() },
        file_scope: vec![],
    };
    let args = serde_json::json!({"action":"proposal_create", "input":{"runId":run.id,"proposalKey":"one","story":story}});
    let addr = "127.0.0.1:12345".parse().unwrap();
    let first = handle_mcp_tool_call(&state, addr, "workflow", &args, Some("proposal-mcp")).await;
    assert!(
        first["story"]["id"].is_string(),
        "proposal must be created: {first}"
    );
    let retry = handle_mcp_tool_call(&state, addr, "workflow", &args, Some("proposal-mcp")).await;
    assert_eq!(retry["story"]["id"], first["story"]["id"], "{retry}");
    assert_eq!(stories.list_stories(&plan.id).unwrap().len(), 1);
    let mut conflict = args.clone();
    conflict["input"]["story"]["title"] = serde_json::json!("Different proposal");
    let rejected =
        handle_mcp_tool_call(&state, addr, "workflow", &conflict, Some("proposal-mcp")).await;
    assert!(
        rejected["error"].is_string(),
        "conflicting key reuse must fail: {rejected}"
    );
    state
        .mcp
        .to_session
        .insert("other-mcp".into(), TEST_UUID_B.into());
    insert_managed_test_session(&state, "other-pty", &path);
    state.bind_live_pty(TEST_UUID_B, "other-pty");
    let rejected = handle_mcp_tool_call(&state, addr, "workflow", &args, Some("other-mcp")).await;
    assert!(
        rejected["error"].as_str().unwrap().contains("coordinator"),
        "{rejected}"
    );
    assert_eq!(stories.list_stories(&plan.id).unwrap().len(), 1);
}

// Catches: an optional-field bag accepts malformed or cross-action inputs before owner effects.
#[tokio::test]
async fn workflow_typed_actions_reject_invalid_inputs_before_effects() {
    let state = test_state();
    let addr = "127.0.0.1:12345".parse().unwrap();
    for args in [
        serde_json::json!({"action":"story","input":{"action":"create_story"}}),
        serde_json::json!({"action":"run","input":{"action":"get","run_id":4}}),
        serde_json::json!({"action":"proposal_create","input":{"runId":"run","proposalKey":"key","story":{}}}),
        serde_json::json!({"action":"report","input":{"runId":"run","outcome":"invented"}}),
        serde_json::json!({"action":"launch","input":{"runId":"run","attemptId":"attempt","worktreePath":"/repo","agentType":false}}),
        serde_json::json!({"action":"story","input":{"action":"list_plans"},"runId":"extra"}),
        serde_json::json!({"action":"automation","input":{"action":"list"}}),
    ] {
        let result = handle_mcp_tool_call(&state, addr, "workflow", &args, None).await;
        assert_eq!(result["code"], "invalid_arguments", "{args}: {result}");
        assert_eq!(result["effect"], "not_started", "{args}: {result}");
    }
    // A valid nested action reaches its existing owner, rather than a different service.
    for (action, input, error) in [
        (
            "story",
            serde_json::json!({"action":"list_plans"}),
            "story requires a bound live managed session",
        ),
        (
            "run",
            serde_json::json!({"action":"get","run_id":"missing"}),
            "workflow_run requires a bound live managed session",
        ),
        (
            "report",
            serde_json::json!({"contractVersion":1,"runId":"run","storyId":"story","storyRevision":1,"attemptId":"attempt","generation":1,"outcome":"completed","summary":"Done","criterionResults":[],"evidence":[]}),
            "workflow report requires a bound live managed session",
        ),
        (
            "launch",
            serde_json::json!({"runId":"run","attemptId":"attempt","worktreePath":"/repo","agentType":"claude"}),
            "workflow launch requires a bound live managed session",
        ),
    ] {
        let result = handle_mcp_tool_call(
            &state,
            addr,
            "workflow",
            &serde_json::json!({"action":action,"input":input}),
            None,
        )
        .await;
        assert_eq!(result["error"], error, "{result}");
    }
}

// Catches: the grouped entry bypasses an old member denial, or workflow denial disables the scheduler.
#[tokio::test]
async fn workflow_denials_preserve_members_and_leave_automations_independent() {
    let config = tempfile::tempdir().unwrap();
    let _guard = crate::config::set_config_dir_override(config.path().to_path_buf());
    let state = test_state();
    let addr = "127.0.0.1:12345".parse().unwrap();
    for (action, member) in [
        ("story", "story"),
        ("run", "workflow_run"),
        ("proposal_create", "workflow_story_create"),
        ("report", "workflow_report"),
        ("launch", "workflow_launch"),
    ] {
        state.config.write().mcp_capabilities =
            crate::mcp_capabilities::McpCapabilities::from_legacy(&[member.into()]);
        let result = handle_mcp_tool_call(
            &state,
            addr,
            "workflow",
            &serde_json::json!({"action":action,"input":{}}),
            None,
        )
        .await;
        assert_eq!(
            result["error"], "Tool 'workflow' is disabled by configuration",
            "{member}: {result}"
        );
        let tools = filtered_native_tools(&state);
        let workflow = tools
            .iter()
            .find(|tool| tool["name"] == "workflow")
            .unwrap();
        assert!(
            !workflow["inputSchema"]["properties"]["action"]["enum"]
                .as_array()
                .unwrap()
                .iter()
                .any(|advertised| advertised == action)
        );
    }
    state.config.write().mcp_capabilities.denied = vec!["workflow".into()];
    let tools = filtered_native_tools(&state);
    assert!(!tools.iter().any(|tool| tool["name"] == "workflow"));
    assert!(tools.iter().any(|tool| tool["name"] == "automations"));
    let listed = handle_mcp_tool_call(
        &state,
        addr,
        "automations",
        &serde_json::json!({"action":"list"}),
        None,
    )
    .await;
    assert!(listed.is_array(), "scheduler remains callable: {listed}");
    let denied = handle_mcp_tool_call(
        &state,
        addr,
        "workflow",
        &serde_json::json!({"action":"story","input":{"action":"list_plans"}}),
        None,
    )
    .await;
    assert_eq!(
        denied["error"],
        "Tool 'workflow' is disabled by configuration"
    );
}

fn register_git_repo(root: &std::path::Path, name: &str) -> String {
    let dir = root.join(name);
    std::fs::create_dir(&dir).unwrap();
    crate::git_cli::git_cmd(&dir).args(["init"]).run().unwrap();
    let path = dir.canonicalize().unwrap();
    let path = tuic_core::path_spelling::portable_spelling(&path.to_string_lossy());
    crate::config::register_repository(&crate::git::get_repo_info_impl(&path)).unwrap();
    path
}

// Catches: repo status ignores `path` and answers an unregistered path with the full repository list.
#[tokio::test]
async fn repo_status_with_unregistered_path_names_the_path() {
    let root = tempfile::tempdir().unwrap();
    let _config = crate::config::set_config_dir_override(root.path().join("config"));
    register_git_repo(root.path(), "known");
    let bogus = root
        .path()
        .join("not-registered")
        .to_string_lossy()
        .into_owned();

    let result = handle_github(
        &test_state(),
        &serde_json::json!({"action": "status", "path": bogus}),
    )
    .await;

    let error = result["error"]
        .as_str()
        .unwrap_or_else(|| panic!("expected error: {result}"));
    assert!(error.contains(&bogus), "error must name the path: {error}");
}

// Catches: a registered `path` still returns every repository instead of that repository only.
#[tokio::test]
async fn repo_status_with_registered_path_returns_that_repo_only() {
    let root = tempfile::tempdir().unwrap();
    let _config = crate::config::set_config_dir_override(root.path().join("config"));
    let first = register_git_repo(root.path(), "first");
    register_git_repo(root.path(), "second");

    let result = handle_github(
        &test_state(),
        &serde_json::json!({"action": "status", "path": first}),
    )
    .await;

    let rows = result
        .as_array()
        .unwrap_or_else(|| panic!("expected list: {result}"));
    assert_eq!(rows.len(), 1, "{result}");
    assert_eq!(rows[0]["path"], first);
}

// Catches: omitting `path` stops returning the cross-repo aggregate.
#[tokio::test]
async fn repo_status_without_path_returns_every_repo() {
    let root = tempfile::tempdir().unwrap();
    let _config = crate::config::set_config_dir_override(root.path().join("config"));
    register_git_repo(root.path(), "first");
    register_git_repo(root.path(), "second");

    let result = handle_github(&test_state(), &serde_json::json!({"action": "status"})).await;

    assert_eq!(result.as_array().unwrap().len(), 2, "{result}");
}

// Catches: exact string comparison rejects a registered repo given with a trailing slash or via a symlink.
#[cfg(unix)]
#[tokio::test]
async fn repo_status_matches_registered_path_through_slash_and_symlink() {
    let root = tempfile::tempdir().unwrap();
    let _config = crate::config::set_config_dir_override(root.path().join("config"));
    let first = register_git_repo(root.path(), "first");
    register_git_repo(root.path(), "second");
    let link = root.path().join("link");
    std::os::unix::fs::symlink(&first, &link).unwrap();

    for spelling in [format!("{first}/"), link.to_string_lossy().into_owned()] {
        let result = handle_github(
            &test_state(),
            &serde_json::json!({"action": "status", "path": spelling}),
        )
        .await;
        let rows = result
            .as_array()
            .unwrap_or_else(|| panic!("{spelling}: {result}"));
        assert_eq!(rows.len(), 1, "{spelling}: {result}");
        assert_eq!(rows[0]["path"], first, "{spelling}");
    }
}
