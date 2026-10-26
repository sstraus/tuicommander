use super::*;

fn strings(args: &[&str]) -> Vec<String> {
    args.iter().map(|arg| (*arg).to_string()).collect()
}

/// A config dir holding only defaults, so the default text is what launches get.
fn config_dir() -> (tempfile::TempDir, impl Drop) {
    let dir = tempfile::TempDir::new().unwrap();
    assert!(
        dir.path()
            .starts_with(crate::test_support::test_temp_root())
    );
    let guard = tuic_core::config_dir::set_override(dir.path().to_path_buf());
    write_files(dir.path()).unwrap();
    (dir, guard)
}

fn path_of(dir: &Path, agent: &str) -> String {
    file_path(dir, agent).to_string_lossy().into_owned()
}

fn text_of(dir: &Path, agent: &str) -> String {
    std::fs::read_to_string(file_path(dir, agent)).unwrap()
}

/// Catches: a resumed Claude launched without the instructions, or the flag
/// added to a subcommand or a headless run.
#[test]
fn claude_composition() {
    let (dir, _guard) = config_dir();
    let dir = dir.path();
    let file = path_of(dir, "claude");
    for typed in [
        vec![],
        strings(&["--model", "opus"]),
        strings(&["--resume", "x"]),
        strings(&["--continue"]),
    ] {
        let mut expected = typed.clone();
        expected.extend(strings(&["--append-system-prompt-file", &file]));
        assert_eq!(args("claude", typed.clone(), dir), expected, "{typed:?}");
    }
    // Never after a bare --, where the flag would become prompt text.
    assert_eq!(
        args("claude", strings(&["--model", "opus", "--", "text"]), dir),
        strings(&[
            "--model",
            "opus",
            "--append-system-prompt-file",
            &file,
            "--",
            "text"
        ])
    );
    for unchanged in [
        strings(&["mcp", "list"]),
        strings(&["-p", "hi"]),
        strings(&["--print", "hi"]),
        strings(&["--append-system-prompt", "mine"]),
        strings(&["--append-system-prompt-file=/mine.md"]),
        strings(&["--system-prompt", "mine"]),
    ] {
        assert_eq!(args("claude", unchanged.clone(), dir), unchanged);
    }
    assert!(text_of(dir, "claude").contains("intent: <desc> (<title>)"));
}

/// Catches: raw markdown passed to -c and parsed as TOML, or codex exec output
/// polluted by markers.
#[test]
fn codex_composition() {
    let (dir, _guard) = config_dir();
    let dir = dir.path();
    let fresh = args("codex", strings(&["--model", "o3"]), dir);
    assert_eq!(fresh[0], "-c");
    assert_eq!(&fresh[2..], ["--model", "o3"]);
    // Codex parses the -c value as TOML; the multi-line text must survive it.
    let parsed: toml::Table = toml::from_str(&fresh[1]).unwrap();
    let app = crate::config::AppConfig::default();
    let agents = crate::config::AgentsConfig::default();
    assert_eq!(
        parsed["developer_instructions"].as_str(),
        Some(effective_text(&app, &agents, "codex").as_str())
    );
    let resume = args("codex", strings(&["resume", "--last"]), dir);
    assert_eq!(&resume[2..], ["resume", "--last"]);
    assert_eq!(resume[1], fresh[1]);
    for unchanged in [
        strings(&["exec", "task"]),
        strings(&["e", "task"]),
        strings(&["-c", "developer_instructions=\"mine\""]),
        strings(&["-cdeveloper_instructions=\"mine\""]),
        strings(&["--config=developer_instructions=\"mine\""]),
    ] {
        assert_eq!(args("codex", unchanged.clone(), dir), unchanged);
    }
}

/// Catches: the rules flag on a single-turn run or a duplicate --rules.
#[test]
fn grok_composition() {
    let (dir, _guard) = config_dir();
    let dir = dir.path();
    let text = text_of(dir, "grok");
    for typed in [vec![], strings(&["--continue"])] {
        let mut expected = vec![format!("--rules={text}")];
        expected.extend(typed.clone());
        assert_eq!(args("grok", typed, dir), expected);
    }
    for unchanged in [
        strings(&["-p", "hi"]),
        strings(&["--single", "hi"]),
        strings(&["--rules", "mine"]),
    ] {
        assert_eq!(args("grok", unchanged.clone(), dir), unchanged);
    }
}

/// Catches: the instructions env missing on resume or set on opencode run.
#[test]
fn opencode_composition() {
    let (dir, _guard) = config_dir();
    let dir = dir.path();
    let expected = Some((
        "OPENCODE_CONFIG_CONTENT".to_string(),
        serde_json::json!({"instructions": [path_of(dir, "opencode")]}).to_string(),
    ));
    for typed in [vec![], strings(&["-c"]), strings(&["--session", "s1"])] {
        assert_eq!(
            args("opencode", typed.clone(), dir),
            typed,
            "opencode takes no flag"
        );
        assert_eq!(env("opencode", &typed, dir), expected, "{typed:?}");
    }
    assert_eq!(env("opencode", &strings(&["run", "task"]), dir), None);
    assert_eq!(env("claude", &[], dir), None);
}

/// Catches: --system placed before the session subcommand or added to goose run.
#[test]
fn goose_composition() {
    let (dir, _guard) = config_dir();
    let dir = dir.path();
    let text = text_of(dir, "goose");
    assert_eq!(
        args("goose", strings(&["session"]), dir),
        ["session".to_string(), format!("--system={text}")]
    );
    let resumed = args(
        "goose",
        strings(&["session", "--resume", "--name", "n"]),
        dir,
    );
    assert_eq!(
        resumed[..2],
        ["session".to_string(), format!("--system={text}")]
    );
    assert_eq!(&resumed[2..], ["--resume", "--name", "n"]);
    for unchanged in [
        strings(&["run", "-t", "task"]),
        strings(&["session", "--system", "mine"]),
        vec![],
    ] {
        assert_eq!(args("goose", unchanged.clone(), dir), unchanged);
    }
}

/// Catches: the instructions missing on resume or added to a print run.
#[test]
fn pi_composition() {
    let (dir, _guard) = config_dir();
    let dir = dir.path();
    let file = path_of(dir, "pi");
    for typed in [vec![], strings(&["--continue"]), strings(&["--resume"])] {
        let mut expected = strings(&["--append-system-prompt", &file]);
        expected.extend(typed.clone());
        assert_eq!(args("pi", typed, dir), expected);
    }
    for unchanged in [
        strings(&["-p", "hi"]),
        strings(&["install", "x"]),
        strings(&["--append-system-prompt", "mine"]),
    ] {
        assert_eq!(args("pi", unchanged.clone(), dir), unchanged);
    }
}

/// Catches: --append-system after -- (becomes prompt text) or on a non-run verb.
#[test]
fn ego_composition() {
    let (dir, _guard) = config_dir();
    let dir = dir.path();
    let flag = format!("@{}", path_of(dir, "ego"));
    assert_eq!(
        args("ego", vec![], dir),
        ["--append-system".to_string(), flag.clone()]
    );
    assert_eq!(
        args("ego", strings(&["resume", "id"]), dir),
        strings(&["resume", "--append-system", &flag, "id"])
    );
    assert_eq!(
        args("ego", strings(&["--", "--append-system"]), dir),
        strings(&["--append-system", &flag, "--", "--append-system"])
    );
    for unchanged in [
        strings(&["run", "task"]),
        strings(&["doctor"]),
        strings(&["--append-system", "mine"]),
    ] {
        assert_eq!(args("ego", unchanged.clone(), dir), unchanged);
    }
}

/// Catches: a per-agent override ignored, an empty text still injected, or the
/// default demanding a marker that the agent's toggle disabled.
#[test]
fn effective_text_follows_overrides_and_toggles() {
    let app = crate::config::AppConfig::default();
    let mut agents = crate::config::AgentsConfig::default();
    let default = effective_text(&app, &agents, "claude");
    assert!(
        REQUIRED_MARKERS
            .iter()
            .all(|marker| default.contains(marker))
    );
    assert!(default.contains(&format!(
        "TUICommander v{} is connected.",
        env!("CARGO_PKG_VERSION")
    )));
    assert!(!default.contains("{VER}"));

    agents.agents.insert(
        "codex".into(),
        crate::config::AgentSettings {
            suggest_followups: Some(false),
            ..Default::default()
        },
    );
    let codex = effective_text(&app, &agents, "codex");
    assert!(
        !codex.contains("suggest:"),
        "toggle off must drop the suggest rule"
    );
    assert!(codex.contains("intent:"));

    agents.launch_instructions = Some("global v{VER}".into());
    agents.agents.insert(
        "grok".into(),
        crate::config::AgentSettings {
            launch_instructions: Some("grok only".into()),
            ..Default::default()
        },
    );
    assert!(effective_text(&app, &agents, "grok").ends_with("grok only"));
    assert!(effective_text(&app, &agents, "grok").contains("global v"));
    assert!(
        effective_text(&app, &agents, "claude")
            .ends_with(&format!("global v{}", env!("CARGO_PKG_VERSION")))
    );

    // Explicit off disables modules and both additions for that agent only.
    agents
        .agents
        .get_mut("grok")
        .unwrap()
        .launch_instructions_disabled = true;
    let dir = tempfile::TempDir::new().unwrap();
    write_files_with(dir.path(), &app, &agents).unwrap();
    assert_eq!(
        std::fs::read_to_string(file_path(dir.path(), "grok")).unwrap(),
        ""
    );
    assert_eq!(
        std::fs::read_to_string(file_path(dir.path(), "pi")).unwrap(),
        effective_text(&app, &agents, "pi")
    );
}

/// Catches: a custom text that starts with a bullet (`- intent: ...`) passed as
/// a separate argv value, which grok and goose parse as an unknown flag and
/// refuse to start.
#[test]
fn text_flags_keep_a_dash_led_text_attached() {
    let (dir, _guard) = config_dir();
    let base = crate::config::AgentsConfig::default();
    let mut edited = base.clone();
    edited.launch_instructions = Some("- intent: first".into());
    crate::config::save_agents_config(base, edited).unwrap();
    for (agent, typed) in [
        ("grok", strings(&["--continue"])),
        ("goose", strings(&["session"])),
    ] {
        let launched = args(agent, typed, dir.path());
        assert!(
            launched.len() > 1,
            "{agent}: the instructions were not injected: {launched:?}"
        );
        assert!(
            launched.iter().any(|arg| arg.ends_with("- intent: first")
                && arg.starts_with("--")
                && arg.contains("=")),
            "{agent}: a dash-led value must be glued to its flag: {launched:?}"
        );
    }
}

/// Catches: a Settings edit that launches keep ignoring until TUIC restarts.
#[test]
fn saving_agents_config_rewrites_the_launch_files() {
    let (dir, _guard) = config_dir();
    let base = crate::config::AgentsConfig::default();
    let mut edited = base.clone();
    edited.launch_instructions = Some("edited".into());
    crate::config::save_agents_config(base, edited).unwrap();
    assert!(text_of(dir.path(), "claude").ends_with("edited"));
    let parsed: toml::Table = toml::from_str(&text_of(dir.path(), "codex")).unwrap();
    assert!(
        parsed["developer_instructions"]
            .as_str()
            .unwrap()
            .ends_with("edited")
    );
}

/// Catches: global additions replacing generated modules or agent additions replacing global ones.
#[test]
fn generated_modules_append_both_user_layers_and_obey_off() {
    let app = AppConfig::default();
    let mut agents = AgentsConfig {
        launch_instructions: Some("GLOBAL ADDITION".into()),
        ..Default::default()
    };
    agents
        .agents
        .entry("ego".into())
        .or_default()
        .launch_instructions = Some("AGENT ADDITION".into());
    let root = effective_text(&app, &agents, "ego");
    assert!(root.contains("## Working with the user"));
    assert!(root.ends_with("GLOBAL ADDITION\n\nAGENT ADDITION"));
    assert!(!root.contains("## Child agents"));
    let child = effective_text_for(&app, &agents, "ego", true);
    assert!(child.contains("## Child agents"));
    assert!(!child.contains("suggest:"));
    agents.launch_modules.insert("waiting".into(), false);
    agents
        .agents
        .get_mut("ego")
        .unwrap()
        .launch_modules
        .insert("markers".into(), false);
    let text = effective_text(&app, &agents, "ego");
    assert!(!text.contains("## Waiting and long work"));
    assert!(!text.contains("## TUIC output markers"));
    assert!(text.contains("## Process safety"));
    assert!(text.contains("except through `make agent-test`, which isolates port, config folder, binary name, shortcuts and microphone."));
    agents.launch_instructions_disabled = true;
    assert_eq!(effective_text_for(&app, &agents, "ego", true), "");
}

/// Catches: saved default copies appended twice, custom additions lost, or migration rerun on new empty additions.
#[test]
fn migration_drops_only_exact_legacy_defaults_once() {
    for intent in [false, true] {
        for suggest in [false, true] {
            let mut agents = AgentsConfig {
                launch_instructions: Some(marker_text(intent, suggest).replace("{VER}", "1.7.99")),
                ..Default::default()
            };
            agents
                .agents
                .entry("ego".into())
                .or_default()
                .launch_instructions = Some("CUSTOM".into());
            assert!(migrate_additions(&mut agents));
            assert_eq!(agents.launch_instructions, None);
            assert_eq!(
                agents.agents["ego"].launch_instructions.as_deref(),
                Some("CUSTOM")
            );
            agents.launch_instructions = Some(String::new());
            assert!(!migrate_additions(&mut agents));
            assert!(!agents.launch_instructions_disabled);
        }
    }
    let mut agents = AgentsConfig {
        launch_instructions: Some(String::new()),
        ..Default::default()
    };
    migrate_additions(&mut agents);
    assert!(agents.launch_instructions_disabled);
}

/// Catches: a child launch overwriting root files or delivering parent-only markers instead of child scope.
#[test]
fn child_launch_files_are_isolated_and_codex_receives_child_text() {
    let (dir, _guard) = config_dir();
    let root_before = text_of(dir.path(), "claude");
    let child = args_for("claude", vec![], dir.path(), true);
    assert!(child[1].contains("instructions-claude-child.md"));
    let text = std::fs::read_to_string(&child[1]).unwrap();
    assert!(text.contains("## Child agents"));
    assert!(!text.contains("suggest:"));
    assert_eq!(text_of(dir.path(), "claude"), root_before);
    let codex = args_for("codex", vec![], dir.path(), true);
    let parsed: toml::Table = toml::from_str(&codex[1]).unwrap();
    assert!(
        parsed["developer_instructions"]
            .as_str()
            .unwrap()
            .contains("## Child agents")
    );
}

/// Catches: generated user guidance demanding progress after the user disabled its journal.
#[test]
fn user_module_respects_progress_flags_without_losing_other_guidance() {
    let mut app = AppConfig::default();
    let mut agents = AgentsConfig::default();
    assert!(effective_text(&app, &agents, "ego").contains("progress type=done"));
    agents
        .agents
        .entry("ego".into())
        .or_default()
        .progress_tracking = Some(false);
    let disabled = effective_text(&app, &agents, "ego");
    assert!(!disabled.contains("progress type=done"));
    assert!(disabled.contains("ui action=tab"));
    app.progress_tracking = false;
    assert!(!effective_text(&app, &agents, "claude").contains("progress type=done"));
}

/// Catches: agents launched without the MDKB search rule while mdkb is installed.
#[test]
fn mdkb_rule_is_in_every_agent_text_when_installed() {
    let (app, agents) = (AppConfig::default(), AgentsConfig::default());
    for agent in AGENTS {
        for child in [false, true] {
            let text = effective_text_with(&app, &agents, agent, child, true);
            assert!(text.contains(MDKB_RULE), "{agent} child={child}");
        }
    }
    let text = effective_text_with(&app, &agents, "claude", false, true);
    assert!(text.contains("Use MDKB for all code searches, including exact strings, symbols, and surrounding context. Prefer targeted retrieval over reading entire files. Grep, rg, and Bash-based searches are fallback tools only when MDKB cannot cover the target. Do not attempt alternative search commands when a search is blocked."));
}

/// Catches: the rule promising a tool that is not installed, or surviving a disabled launch prompt.
#[test]
fn mdkb_rule_is_absent_when_not_installed_or_launch_text_is_disabled() {
    let (app, mut agents) = (AppConfig::default(), AgentsConfig::default());
    assert!(!effective_text_with(&app, &agents, "claude", false, false).contains("MDKB"));
    agents.launch_instructions_disabled = true;
    assert_eq!(
        effective_text_with(&app, &agents, "claude", false, true),
        ""
    );
}

/// Catches: detection that misses an mdkb executable in a PATH directory, or accepts a directory named mdkb.
#[test]
fn mdkb_detection_finds_a_file_not_a_directory() {
    let dir = tempfile::TempDir::new().unwrap();
    let name = if cfg!(windows) { "mdkb.exe" } else { "mdkb" };
    let dirs = [dir.path().to_path_buf()];
    assert!(!mdkb_in(&dirs));
    std::fs::create_dir(dir.path().join(name)).unwrap();
    assert!(!mdkb_in(&dirs));
    std::fs::remove_dir(dir.path().join(name)).unwrap();
    std::fs::write(dir.path().join(name), b"").unwrap();
    assert!(mdkb_in(&dirs));
}
