// Catches: repo add reports success without persisting a usable workspace, or
// a repeated add destroys terminal state already owned by the repository.
#[tokio::test]
async fn repo_add_persists_workspace_and_preserves_repeat_registration() {
    let config = tempfile::tempdir_in(crate::test_support::test_temp_root()).unwrap();
    let _guard = crate::config::set_config_dir_override(config.path().into());
    let project = tempfile::tempdir_in(crate::test_support::test_temp_root()).unwrap();
    let path = project
        .path()
        .canonicalize()
        .unwrap()
        .to_string_lossy()
        .into_owned();
    let state = test_state();
    let mut events = state.event_bus.subscribe();
    let args = serde_json::json!({"action":"add", "path":path});
    let result = handle_repo(&state, &args, false).await;
    assert!(result.get("error").is_none(), "{result}");
    let mut saved = crate::config::load_repositories();
    assert_eq!(saved["activeRepoPath"], path);
    assert_eq!(saved["repoOrder"], serde_json::json!([path]));
    assert_eq!(saved["repos"][&path]["isGitRepo"], false);
    assert_eq!(saved["repos"][&path]["activeWorkspaceId"], "shell");
    assert!(matches!(
        events.try_recv().unwrap(),
        crate::state::AppEvent::RepositoriesChanged
    ));
    assert_eq!(
        saved["repos"][&path]["workspaces"]["shell"]["worktreePath"],
        path
    );
    saved["repos"][&path]["workspaces"]["shell"]["terminals"] =
        serde_json::json!(["live-terminal"]);
    saved["groups"] = serde_json::json!({"personal":{"id":"personal","name":"Personal"}});
    crate::config::replace_repositories_for_test(saved.clone()).unwrap();
    let result = handle_repo(&state, &args, false).await;
    assert!(result.get("error").is_none(), "{result}");
    assert_eq!(crate::config::load_repositories(), saved);
    crate::repo_watcher::stop_watching(&path, &state);
}

// Catches: registration defaults unreadable/corrupt config to an empty object
// and overwrites unrelated repositories rather than preserving recovery data.
#[tokio::test]
async fn repo_add_preserves_corrupt_configuration_on_error() {
    let config = tempfile::tempdir_in(crate::test_support::test_temp_root()).unwrap();
    let _guard = crate::config::set_config_dir_override(config.path().into());
    let project = tempfile::tempdir_in(crate::test_support::test_temp_root()).unwrap();
    let file = config.path().join("repositories.json");
    std::fs::write(&file, "{recover this file").unwrap();
    let result = handle_repo(
        &test_state(),
        &serde_json::json!({
            "action":"add", "path":project.path().to_string_lossy()
        }),
        false,
    )
    .await;
    assert!(result["error"].is_string(), "{result}");
    assert!(
        !file.exists(),
        "strict config loading quarantines corrupt files"
    );
    let backups: Vec<_> = std::fs::read_dir(config.path())
        .unwrap()
        .map(|entry| entry.unwrap().path())
        .filter(|path| {
            path.file_name()
                .unwrap()
                .to_string_lossy()
                .starts_with("repositories.corrupt-")
        })
        .collect();
    assert_eq!(backups.len(), 1);
    assert_eq!(
        std::fs::read_to_string(&backups[0]).unwrap(),
        "{recover this file"
    );
}

// Catches: registration uses a fabricated default branch, or treats a missing
// directory/file as a valid plain repository and persists an unusable row.
#[tokio::test]
async fn repo_add_uses_real_git_head_and_invalid_paths_do_not_change_config() {
    let config = tempfile::tempdir_in(crate::test_support::test_temp_root()).unwrap();
    let _guard = crate::config::set_config_dir_override(config.path().into());
    let project = tempfile::tempdir_in(crate::test_support::test_temp_root()).unwrap();
    crate::git_cli::git_cmd(project.path())
        .args(["init", "-b", "feature-registration"])
        .run()
        .unwrap();
    let path = project
        .path()
        .canonicalize()
        .unwrap()
        .to_string_lossy()
        .into_owned();
    let state = test_state();
    let result = handle_repo(
        &state,
        &serde_json::json!({"action":"add", "path":path}),
        false,
    )
    .await;
    assert_eq!(result["ok"], true, "{result}");
    assert_eq!(result["branch"], "feature-registration");
    let saved = crate::config::load_repositories();
    assert_eq!(
        saved["repos"][&path]["activeWorkspaceId"],
        "feature-registration"
    );
    assert_eq!(saved["repos"][&path]["isGitRepo"], true);
    let file = project.path().join("file.txt");
    std::fs::write(&file, "content").unwrap();
    for invalid in [
        file.to_string_lossy().into_owned(),
        format!("{path}/missing"),
    ] {
        let result = handle_repo(
            &state,
            &serde_json::json!({"action":"add", "path":invalid}),
            false,
        )
        .await;
        assert!(result["error"].is_string(), "{result}");
        assert_eq!(crate::config::load_repositories(), saved);
    }
    crate::repo_watcher::stop_watching(&path, &state);
}

// Catches: ui tab silently accepts a deep link as iframe content and does
// nothing while telling the caller that registration succeeded.
#[test]
fn ui_tab_rejects_repository_deep_link_instead_of_silent_success() {
    let result = handle_ui(
        &test_state(),
        &serde_json::json!({
            "action":"tab", "id":"register", "title":"Register",
            "url":"tuic://open-repo?path=/repo"
        }),
        None,
    );
    assert!(
        result["error"]
            .as_str()
            .is_some_and(|error| error.contains("repo") && error.contains("add")),
        "{result}"
    );
    // Native content links remain valid; an overbroad tuic-scheme guard would
    // break file tabs while fixing repository registration.
    for url in ["tuic://open//repo/readme.md", "tuic://edit//repo/main.rs"] {
        let result = handle_ui(
            &test_state(),
            &serde_json::json!({
                "action":"tab", "id":"content", "title":"Content", "url":url
            }),
            None,
        );
        assert_eq!(result["ok"], true, "{result}");
    }
}
