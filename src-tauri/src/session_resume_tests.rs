use super::*;

// Catches: profile-root verification being lost when command construction moves to Rust.
#[test]
fn resume_verifies_the_launch_profile_and_preserves_quoted_environment() {
    let root = tempfile::tempdir_in(crate::test_support::test_temp_root()).unwrap();
    let _config = crate::config::set_config_dir_override(root.path().join("config with spaces"));
    let profile = root.path().join("profile with spaces");
    let id = "550e8400-e29b-41d4-a716-446655440aaa";
    let cwd = root.path().to_str().unwrap();
    let project = crate::agent_session::claude_project_dir_path(cwd, profile.to_str()).unwrap();
    std::fs::create_dir_all(&project).unwrap();
    std::fs::write(project.join(format!("{id}.jsonl")), b"").unwrap();
    let request = ResumeRequest {
        agent_type: "claude".into(),
        cwd: Some(cwd.into()),
        agent_session_id: Some(id.into()),
        tuic_session: Some("different-peer".into()),
        launch_command: Some(format!(
            "CLAUDE_CONFIG_DIR={} claude --model opus",
            crate::prompt::shell_quote(profile.to_str().unwrap())
        )),
    };
    let result = resolve(&request);
    assert!(result.verified);
    let (env, command) = shell_launch(result.cmd.as_deref().unwrap());
    assert_eq!(env["CLAUDE_CONFIG_DIR"], profile.to_str().unwrap());
    assert_eq!(
        command,
        format!(
            "claude --resume {} --model opus{}",
            crate::prompt::shell_quote(id),
            expected_claude_settings(&root.path().join("config with spaces"))
        )
    );
    std::fs::remove_file(project.join(format!("{id}.jsonl"))).unwrap();
    assert!(resolve(&request).cmd.is_none());
}

// Catches: an old Start Work brief being submitted again when a tab resumes.
#[test]
fn resume_drops_the_seed_brief_and_retains_run_arguments() {
    let root = tempfile::tempdir_in(crate::test_support::test_temp_root()).unwrap();
    let _config = crate::config::set_config_dir_override(root.path().into());
    let request = ResumeRequest { agent_type: "claude".into(), launch_command: Some("c2 --model opus -- '<untrusted-content source=\"work item\">\nfix the bug\n</untrusted-content>'".into()), ..Default::default() };
    assert_eq!(
        resolve(&request).cmd.unwrap(),
        format!(
            "c2 --continue --model opus{}",
            expected_claude_settings(root.path())
        )
    );
}

// Catches: an unknown/non-resumable agent being turned into a destructive fake resume.
#[test]
fn resume_refuses_non_resumable_agents_and_keeps_static_fallback_explicit() {
    let root = tempfile::tempdir_in(crate::test_support::test_temp_root()).unwrap();
    let _config = crate::config::set_config_dir_override(root.path().into());
    for agent in ["droid", "shell", "unknown"] {
        assert!(
            resolve(&ResumeRequest {
                agent_type: agent.into(),
                ..Default::default()
            })
            .cmd
            .is_none()
        );
    }
    let fallback = resolve(&ResumeRequest {
        agent_type: "claude".into(),
        ..Default::default()
    });
    assert!(!fallback.verified);
    assert_eq!(
        fallback.cmd.unwrap(),
        format!("claude --continue{}", expected_claude_settings(root.path()))
    );
}

// Catches: migration dropping supported agents' static and id-based resume flags.
#[test]
fn resume_preserves_agent_selectors_and_custom_binaries() {
    let root = tempfile::tempdir_in(crate::test_support::test_temp_root()).unwrap();
    let _config = crate::config::set_config_dir_override(root.path().into());
    for (agent, expected) in [
        ("gemini", "--resume"),
        ("codex", "resume --last"),
        ("ego", "resume"),
        ("cursor", "resume"),
        ("amp", "threads continue"),
        ("opencode", "-c"),
        ("pi", "--continue"),
        ("grok", "--continue"),
    ] {
        let result = resolve(&ResumeRequest {
            agent_type: agent.into(),
            launch_command: Some("wrapper --model custom".into()),
            ..Default::default()
        });
        let command = result.cmd.unwrap();
        assert!(command.contains(expected), "{agent}: {command}");
        assert!(command.starts_with("wrapper ") && command.ends_with("--model custom"));
    }
    let quoted_id = crate::prompt::shell_quote("recorded-id");
    for (agent, selector) in [
        ("claude", "--resume"),
        ("gemini", "--resume"),
        ("codex", "resume"),
        ("ego", "resume"),
    ] {
        let expected = format!("{selector} {quoted_id}");
        let result = resolve(&ResumeRequest {
            agent_type: agent.into(),
            agent_session_id: Some("recorded-id".into()),
            ..Default::default()
        });
        assert!(result.cmd.unwrap().contains(&expected));
        assert!(!result.verified, "no cwd must never claim verified storage");
    }
}

// Catches: decoding every backslash as an escape corrupts quoted profile paths.
#[test]
fn shell_launch_preserves_backslashes_in_double_quoted_profile_paths() {
    let (env, command) =
        shell_launch(r#"CLAUDE_CONFIG_DIR="D:\profiles\private profile" claude --model opus"#);
    assert_eq!(env["CLAUDE_CONFIG_DIR"], r"D:\profiles\private profile");
    assert_eq!(command, "claude --model opus");
}

// Catches: command/profile decoding drifting from the launch writer's host quoting.
#[test]
fn shell_launch_round_trips_native_quoted_profile_values() {
    for profile in [
        r"C:\profiles\private profile",
        r"\\server\share\profile\",
        "a'b\"c^&|<>%",
    ] {
        let input = format!(
            "CLAUDE_CONFIG_DIR={} claude --model opus",
            crate::prompt::shell_quote(profile)
        );
        let (env, command) = shell_launch(&input);
        assert_eq!(env["CLAUDE_CONFIG_DIR"], profile, "{input}");
        assert_eq!(command, "claude --model opus");
    }
}

// Independent literal contract: the appended settings path remains one shell word.
fn expected_claude_settings(config: &std::path::Path) -> String {
    if crate::agent_hook::hooks_supported_on_host() {
        format!(
            " {} {}",
            crate::prompt::shell_quote("--settings"),
            crate::prompt::shell_quote(&config.join("agent-hooks/claude.json").to_string_lossy())
        )
    } else {
        String::new()
    }
}
