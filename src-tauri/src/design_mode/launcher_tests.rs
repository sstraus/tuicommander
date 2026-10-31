use super::*;
use std::os::unix::fs::PermissionsExt;

// Catches: first launch omits --load-extension or replaces a discovered server with setup.
#[tokio::test]
async fn first_run_page_is_useful_without_a_server_and_escapes_paths() {
    const ROLE: &str = "TUIC_DESIGN_LAUNCH_TEST";
    if let Some(root) = std::env::var_os(ROLE) {
        let root = PathBuf::from(root);
        let _config = crate::config::set_config_dir_override(root.join("<script>\"&"));
        let state = Arc::new(crate::state::tests_support::make_test_app_state());
        *state.bound_socket_path.write() = root.join("socket");
        let bridge = root.join(".cargo/bin/tuic-bridge");
        assert_eq!(crate::agent_mcp::locate_bridge_binary(), Some(bridge));
        let profile = super::super::browser::profile_dir(&root.join("repo")).join("chrome");
        if std::env::var_os("TUIC_DESIGN_RUNNING").is_some() {
            std::fs::create_dir_all(&profile).unwrap();
            std::os::unix::fs::symlink(
                format!("test-host-{}", std::process::id()),
                profile.join("SingletonLock"),
            )
            .unwrap();
            let error = launch(
                &state,
                &root.join("repo"),
                "launching-agent",
                Some("chrome"),
                Some("http://localhost:5173"),
            )
            .await
            .unwrap_err();
            assert!(
                error.contains("Close the Design Mode browser window"),
                "{error}"
            );
            assert!(!root.join("browser-args").exists());
            std::fs::remove_file(profile.join("SingletonLock")).unwrap();
            return;
        }
        if let Ok(url) = std::env::var("TUIC_DESIGN_DISCOVERED") {
            let base = crate::config::load_repo_settings();
            let mut settings = base.clone();
            settings.repos.insert(
                root.join("repo")
                    .canonicalize()
                    .unwrap()
                    .to_string_lossy()
                    .into_owned(),
                crate::config::RepoSettingsEntry {
                    dev_server_url: Some(url),
                    ..Default::default()
                },
            );
            crate::config::save_repo_settings(base, settings).unwrap();
        }
        let acknowledge = (std::env::var_os("TUIC_DESIGN_SILENT").is_none())
            .then(|| acknowledge_test_launch(state.clone(), root.join("repo")));
        let response = launch(
            &state,
            &root.join("repo"),
            "launching-agent",
            Some("chrome"),
            std::env::var("TUIC_DESIGN_LAUNCH_URL").ok().as_deref(),
        )
        .await;
        if std::env::var_os("TUIC_DESIGN_SILENT").is_some() {
            assert!(response.unwrap_err().contains("browser_not_ready"));
            assert_eq!(
                super::super::legacy::statuses(&state).await[0]["status"],
                "stopped"
            );
            return;
        }
        acknowledge.unwrap().await.unwrap();
        let response = response.unwrap();
        let target = response["url"].as_str().unwrap();
        if let Ok(requested) = std::env::var("TUIC_DESIGN_LAUNCH_URL")
            .or_else(|_| std::env::var("TUIC_DESIGN_DISCOVERED"))
        {
            // Catches: onboarding/discovery replacing an explicit URL on first start.
            assert_eq!(target, requested);
            assert_eq!(response["sessionId"], "launching-agent");
            assert_eq!(response["status"], "armed");
        } else {
            let path = url::Url::parse(target).unwrap().to_file_path().unwrap();
            let html = std::fs::read_to_string(path).unwrap();
            assert!(target.starts_with("file:"));
            assert!(!html.contains("Load unpacked"));
            assert!(html.contains("No single dev server was found"));
            assert!(!html.contains("about:blank"));
        }
        let captured = root.join("browser-args");
        while !captured.exists() {
            tokio::task::yield_now().await;
        }
        let args = std::fs::read_to_string(captured).unwrap();
        assert_eq!(args.lines().last(), Some(target));
        assert!(args.contains("--user-data-dir="));
        let profile = super::super::browser::profile_dir(&root.join("repo")).join("chrome");
        assert!(
            args.lines().any(|arg| arg
                == format!(
                    "--load-extension={}",
                    profile.join("tuic-design-extension").display()
                )),
            "browser did not load the extension: {args}"
        );
        return;
    }
    // Environment-based browser/sidecar discovery is isolated from other libtest threads.
    let dir = tempfile::tempdir_in(crate::test_support::test_temp_root()).unwrap();
    let root = dir.path();
    std::fs::create_dir_all(root.join(".cargo/bin")).unwrap();
    std::fs::create_dir(root.join("repo")).unwrap();
    super::super::discovery::tests::git(&root.join("repo"), &["init"]);
    let bridge = root.join(".cargo/bin/tuic-bridge");
    std::fs::write(
        &bridge,
        "#!/bin/sh\nmkdir -p \"$2/tuic-design-extension\"\n",
    )
    .unwrap();
    let browser = root.join("chrome-test");
    std::fs::write(&browser, "#!/bin/sh\nprintf '%s\\n' \"$@\" > \"$TUIC_DESIGN_LAUNCH_TEST/browser-args.pending\"\nmv \"$TUIC_DESIGN_LAUNCH_TEST/browser-args.pending\" \"$TUIC_DESIGN_LAUNCH_TEST/browser-args\"\n").unwrap();
    for path in [&bridge, &browser] {
        std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o755)).unwrap();
    }
    for mode in ["setup", "explicit", "discovered", "running", "silent"] {
        let _ = std::fs::remove_file(root.join("browser-args"));
        let mut command = tokio::process::Command::new(std::env::current_exe().unwrap());
        match mode {
            "silent" => {
                command.env("TUIC_DESIGN_SILENT", "1");
            }
            "explicit" => {
                command.env(
                    "TUIC_DESIGN_LAUNCH_URL",
                    "http://localhost:5173/page?x=1#element",
                );
            }
            "discovered" => {
                command.env("TUIC_DESIGN_DISCOVERED", "http://localhost:5173/discovered");
            }
            "running" => {
                command.env("TUIC_DESIGN_RUNNING", "1");
            }
            _ => {}
        }
        let output = command
        .args(["--exact", "design_mode::launcher::tests::first_run_page_is_useful_without_a_server_and_escapes_paths", "--nocapture"])
        .env(ROLE, root).env("HOME", root).env("TUIC_DESIGN_CHROME", &browser)
        .output().await.unwrap();
        assert!(
            output.status.success(),
            "{}\n{}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
    }
}

// Catches: silently launching branded Chrome even though it ignores extension loading.
#[test]
fn branded_chrome_returns_actionable_error_before_launch() {
    for executable in [
        "/Applications/Google Chrome.app/Contents/MacOS/Google Chrome",
        "/usr/bin/google-chrome-stable",
        "/opt/google/chrome/chrome",
        "chrome.exe",
    ] {
        let error = ensure_autoload_supported(Path::new(executable)).unwrap_err();
        assert!(error.contains("Choose Microsoft Edge or Chromium"));
    }
    for executable in [
        "/Applications/Microsoft Edge.app/Contents/MacOS/Microsoft Edge",
        "/usr/bin/chromium",
        "Google Chrome for Testing",
    ] {
        assert!(ensure_autoload_supported(Path::new(executable)).is_ok());
    }
}
