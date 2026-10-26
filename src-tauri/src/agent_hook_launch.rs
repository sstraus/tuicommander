//! Launch-scoped native status integration for agent CLIs.

use crate::agent_hook::{SENTINEL, claude_hook_map};
use serde_json::{Map, Value};
use std::path::Path;

/// Recorded installed Claude help supplies advertised verbs when a probe is unavailable.
pub(crate) const RECORDED_CLAUDE_HELP: &str =
    include_str!("../tests/fixtures/agent-help/claude-2026-10-04.txt");

fn claude_command_names(help: &str) -> impl Iterator<Item = &str> {
    help.lines()
        .skip_while(|line| line.trim() != "Commands:")
        .skip(1)
        .filter(|line| line.starts_with("  ") && !line.starts_with("   "))
        .filter_map(|line| line.split_whitespace().next())
}

pub(crate) fn claude_help_is_usable(help: &str) -> bool {
    claude_command_names(help).next().is_some()
}

pub(crate) fn enabled(agent_type: &str) -> bool {
    crate::config::load_agents_config()
        .agents
        .get(agent_type)
        .and_then(|settings| settings.native_status_signals)
        .unwrap_or(true)
}

pub(crate) static SCREEN_POLICIES: &[(&str, &str, Option<&str>)] = &[
    ("codex", "--no-alt-screen", Some("exec")),
    ("grok", "--no-alt-screen", None),
    ("opencode", "--mini", Some("run")),
];

pub(crate) fn screen_policy(agent_type: &str) -> Option<(&'static str, Option<&'static str>)> {
    SCREEN_POLICIES
        .iter()
        .find(|(agent, _, _)| *agent == agent_type)
        .map(|(_, flag, skip)| (*flag, *skip))
}

pub(crate) fn prevents_alt_screen_in(
    config: &crate::config::AgentsConfig,
    agent_type: &str,
) -> bool {
    config
        .agents
        .get(agent_type)
        .and_then(|settings| settings.prevent_alt_screen)
        .unwrap_or(true)
}

pub(crate) fn prevents_alt_screen(agent_type: &str) -> bool {
    prevents_alt_screen_in(&crate::config::load_agents_config(), agent_type)
}

fn claude_document() -> Value {
    let mut hooks = Map::new();
    for (event, matcher, command) in claude_hook_map() {
        let group = serde_json::json!({
            "matcher": matcher,
            "hooks": [{"type": "command", "command": command}],
        });
        hooks
            .entry(event.to_string())
            .or_insert_with(|| Value::Array(Vec::new()))
            .as_array_mut()
            .expect("new hook event is an array")
            .push(group);
    }
    serde_json::json!({"hooks": hooks, "_tuic": SENTINEL})
}

fn shell_quote(value: &str) -> String {
    format!("'{}'", value.replace('\'', "'\\''"))
}

fn codex_user_notify() -> Vec<String> {
    let Some(home) = dirs::home_dir() else {
        return Vec::new();
    };
    let Ok(text) = std::fs::read_to_string(home.join(".codex/config.toml")) else {
        return Vec::new();
    };
    let Ok(value) = text.parse::<toml::Value>() else {
        return Vec::new();
    };
    value
        .get("notify")
        .and_then(toml::Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(toml::Value::as_str)
        .map(str::to_owned)
        .collect()
}

fn codex_script(user_notify: &[String]) -> String {
    let chain = if user_notify.is_empty() {
        String::new()
    } else {
        let command = user_notify
            .iter()
            .map(|part| shell_quote(part))
            .collect::<Vec<_>>()
            .join(" ");
        format!("exec {command} \"$payload\"\n")
    };
    format!(
        r#"#!/bin/sh
# {SENTINEL}
payload=${{1:-}}
case "$payload" in
  *'"type":"agent-turn-complete"'*|*'"type": "agent-turn-complete"'*)
    tty=$(ps -o tty= -p "$PPID" 2>/dev/null | tr -d '[:space:]')
    case "$tty" in *[0-9]*) tty="/dev/${{tty#/dev/}}";; *) tty=/dev/tty;; esac
    printf '\033]7770;state=idle\033\\' > "$tty" 2>/dev/null || true
    ;;
esac
{chain}exit 0
"#
    )
}

/// Boot-time entry shared by the desktop and both headless binaries: a
/// `claude` spawn points at `agent-hooks/claude.json`, so every process that can
/// spawn agents must write it before serving. A failure is logged, not fatal.
pub(crate) fn regenerate_launch_assets_at_boot(config_dir: &Path) {
    if let Err(error) = regenerate_launch_assets(config_dir) {
        tracing::error!(
            source = "agent_hooks",
            "Failed to generate launch-scoped agent status assets: {error}"
        );
    }
}

pub(crate) fn regenerate_launch_assets(config_dir: &Path) -> Result<(), String> {
    let dir = config_dir.join("agent-hooks");
    std::fs::create_dir_all(&dir).map_err(|e| format!("create {}: {e}", dir.display()))?;
    let claude = serde_json::to_vec_pretty(&claude_document()).map_err(|e| e.to_string())?;
    crate::config::persist_atomic(&dir.join("claude.json"), &claude)?;
    let codex = codex_script(&codex_user_notify());
    let codex_path = dir.join("codex-notify.sh");
    crate::config::persist_atomic_with_mode(&codex_path, codex.as_bytes(), 0o700)?;
    crate::launch_instructions::write_files(config_dir)
}

/// Administrative and ACP verbs from the recorded installed ego help.
pub(crate) const EGO_NON_RUN_COMMANDS: &[&str] = &[
    "fork",
    "compact",
    "export",
    "ls",
    "usage",
    "models",
    "hooks",
    "doctor",
    "acp",
    "a2a",
    "auth",
    "mcp",
    "config",
    "rules",
    "mcp-server",
];

/// Translate persisted ego choices once for terminal launches and MCP spawns.
/// Configured choices replace raw overrides; `--` and its positional text are preserved.
fn ego_permission_args(config: &crate::config::AgentsConfig, args: &[String]) -> Vec<String> {
    let Some(settings) = config.agents.get("ego") else {
        return args.to_vec();
    };
    let options = [
        ("--mode", settings.ego_mode.map(|value| value.as_str())),
        (
            "--sandbox",
            settings.ego_sandbox.map(|value| value.as_str()),
        ),
    ];
    if options.iter().all(|(_, value)| value.is_none()) {
        return args.to_vec();
    }
    if args
        .first()
        .is_some_and(|first| EGO_NON_RUN_COMMANDS.contains(&first.as_str()))
    {
        return args.to_vec();
    }
    let mut result = Vec::new();
    let mut index = 0;
    while index < args.len() {
        let arg = &args[index];
        if arg == "--" {
            result.extend_from_slice(&args[index..]);
            break;
        }
        if options
            .iter()
            .any(|(flag, value)| value.is_some() && arg == flag)
        {
            index += 1;
            if args.get(index).is_some_and(|value| !value.starts_with('-')) {
                index += 1;
            }
            continue;
        }
        if options
            .iter()
            .any(|(flag, value)| value.is_some() && arg.starts_with(&format!("{flag}=")))
            || (settings.ego_mode.is_some() && ["--plan", "--yolo"].contains(&arg.as_str()))
        {
            index += 1;
            continue;
        }
        result.push(arg.clone());
        index += 1;
    }
    let insertion = usize::from(
        result
            .first()
            .is_some_and(|arg| ["run", "resume"].contains(&arg.as_str())),
    );
    result.splice(
        insertion..insertion,
        options
            .into_iter()
            .filter_map(|(flag, value)| value.map(|value| [flag.to_string(), value.to_string()]))
            .flatten(),
    );
    result
}

/// The permission-skipping option a launch of `args` would carry, after configured ego choices apply.
pub(crate) fn permission_bypass_flag(
    config: &crate::config::AgentsConfig,
    agent_type: &str,
    args: &[String],
) -> Option<String> {
    let effective = if agent_type == "ego" {
        ego_permission_args(config, args)
    } else {
        args.to_vec()
    };
    let mut rest = effective.iter();
    while let Some(arg) = rest.next() {
        if arg == "--" {
            break;
        }
        if arg.starts_with("--dangerously-") || arg == "--yolo" || arg == "--mode=yolo" {
            return Some(arg.clone());
        }
        for (flag, value) in [
            ("--mode", "yolo"),
            ("--permission-mode", "bypassPermissions"),
            ("--sandbox", "danger-full-access"),
            ("-s", "danger-full-access"),
        ] {
            if arg == &format!("{flag}={value}")
                || (arg == flag && rest.clone().next().is_some_and(|next| next == value))
            {
                return Some(format!("{flag} {value}"));
            }
        }
    }
    None
}

/// Structured launches: status integration, then the marker instructions.
pub(crate) fn launch_args(
    agent_type: &str,
    binary_path: &str,
    args: &[String],
    config_dir: &Path,
    attachment_dir: &Path,
) -> Vec<String> {
    launch_args_for(
        agent_type,
        binary_path,
        args,
        config_dir,
        attachment_dir,
        false,
    )
}

pub(crate) fn launch_args_for(
    agent_type: &str,
    binary_path: &str,
    args: &[String],
    config_dir: &Path,
    attachment_dir: &Path,
    child: bool,
) -> Vec<String> {
    if is_claude_subcommand(agent_type, binary_path, args) {
        return args.to_vec();
    }
    let result = augment_launch(agent_type, binary_path, args, config_dir);
    let mut result = if child {
        crate::launch_instructions::args_for(agent_type, result, config_dir, true)
    } else {
        crate::launch_instructions::args(agent_type, result, config_dir)
    };
    let flag = match agent_type {
        "claude" => Some("--add-dir"),
        "gemini" => Some("--include-directories"),
        "ego" => Some("--read-only-dir"),
        _ => None,
    };
    // A custom binary may be a shell or opaque wrapper, not the harness.
    // Never pass harness grant options to it. Name-based harness hops inherit
    // TUIC_ATTACHMENTS_DIR and use the PATH shim; absolute hops bypass that shim
    // and must arrange their own read access (documented custom-launch limit).
    let binary_name = binary_path
        .rsplit(['/', '\\'])
        .next()
        .unwrap_or(binary_path);
    let direct_harness = binary_name.strip_suffix(".exe").unwrap_or(binary_name) == agent_type;
    if let Some(flag) = flag.filter(|_| direct_harness) {
        // Ego's run/resume options belong after the subcommand; a root option
        // before it makes the CLI parse the verb as its positional prompt.
        let insertion = usize::from(
            agent_type == "ego"
                && result
                    .first()
                    .is_some_and(|arg| ["run", "resume"].contains(&arg.as_str())),
        );
        result.splice(
            insertion..insertion,
            [format!("{flag}={}", attachment_dir.display())],
        );
    }
    result
}

/// A Claude verb (`mcp list`, `auth status`) takes no launch flags. Probes the
/// installed help only when the first argument could be one.
fn is_claude_subcommand(agent_type: &str, binary_path: &str, args: &[String]) -> bool {
    agent_type == "claude"
        && args.first().is_some_and(|arg| !arg.starts_with('-'))
        && claude_is_subcommand(
            args,
            &crate::agent::cli_help(binary_path).unwrap_or_default(),
        )
}

/// The status integration alone, as the tests pin it.
#[cfg(test)]
pub(crate) fn augment_args(
    agent_type: &str,
    binary_path: &str,
    args: &[String],
    config_dir: &Path,
) -> Vec<String> {
    if enabled(agent_type) && is_claude_subcommand(agent_type, binary_path, args) {
        return args.to_vec();
    }
    augment_launch(agent_type, binary_path, args, config_dir)
}

/// `augment_args` after the Claude verb check.
fn augment_launch(
    agent_type: &str,
    binary_path: &str,
    args: &[String],
    config_dir: &Path,
) -> Vec<String> {
    if agent_type == "ego" {
        return ego_permission_args(&crate::config::load_agents_config(), args);
    }
    let enabled = enabled(agent_type);
    let result = augment_args_when(enabled, agent_type, args, config_dir);
    if screen_flag_candidate(agent_type, args).is_none() {
        return result;
    }
    add_screen_flag(
        agent_type,
        args,
        result,
        crate::agent::supports_no_alt_screen(agent_type, binary_path),
    )
}

/// Use the same screen policy for commands typed into an existing terminal.
/// The frontend owns shell quoting, while this function owns the agent-specific
/// flag and its capability check.
pub(crate) fn build_agent_launch_args(
    agent_type: &str,
    binary_path: &str,
    args: &[String],
) -> Vec<String> {
    if agent_type == "ego" {
        return ego_permission_args(&crate::config::load_agents_config(), args);
    }
    if screen_flag_candidate(agent_type, args).is_none() {
        return args.to_vec();
    }
    add_screen_flag(
        agent_type,
        args,
        args.to_vec(),
        crate::agent::supports_no_alt_screen(agent_type, binary_path),
    )
}

#[cfg_attr(feature = "desktop", tauri::command)]
pub(crate) async fn prepare_agent_launch_args(
    agent_type: String,
    binary_path: String,
    args: Vec<String>,
) -> Vec<String> {
    let fallback = args.clone();
    tokio::task::spawn_blocking(move || build_agent_launch_args(&agent_type, &binary_path, &args))
        .await
        .unwrap_or(fallback)
}

fn screen_flag_candidate(agent_type: &str, args: &[String]) -> Option<&'static str> {
    if !prevents_alt_screen(agent_type) {
        return None;
    }
    let (flag, excluded_first_arg) = screen_policy(agent_type)?;
    if excluded_first_arg.is_some_and(|excluded| args.first().is_some_and(|arg| arg == excluded))
        || args.iter().any(|arg| arg == flag)
    {
        return None;
    }
    Some(flag)
}

fn add_screen_flag(
    agent_type: &str,
    args: &[String],
    mut result: Vec<String>,
    supported: bool,
) -> Vec<String> {
    if let Some(screen_flag) = screen_flag_candidate(agent_type, args)
        && supported
    {
        result.insert(0, screen_flag.to_string());
    }
    result
}

/// Installed help owns advertised verbs. On probe failure, retain the verbs
/// verified in the recorded help. The reported remote-control refusal proves
/// that exact hidden command also rejects root launch settings.
pub(crate) fn claude_is_subcommand(args: &[String], help: &str) -> bool {
    let Some(first) = args
        .first()
        .filter(|arg| !arg.is_empty() && !arg.starts_with('-'))
    else {
        return false;
    };
    let help = if !claude_help_is_usable(help) {
        RECORDED_CLAUDE_HELP
    } else {
        help
    };
    first == "remote-control"
        || claude_command_names(help).any(|names| names.split('|').any(|name| name == first))
}

fn augment_args_when(
    enabled: bool,
    agent_type: &str,
    args: &[String],
    config_dir: &Path,
) -> Vec<String> {
    let mut result = args.to_vec();
    if !enabled {
        return result;
    }
    match agent_type {
        "claude"
            if !claude_is_subcommand(args, "")
                && !args.iter().any(|arg| {
                    arg == "--settings" || arg.starts_with("--settings=") || arg == "--bare"
                }) =>
        {
            result.push("--settings".into());
            result.push(
                config_dir
                    .join("agent-hooks/claude.json")
                    .to_string_lossy()
                    .into_owned(),
            );
        }
        "codex"
            if !args
                .windows(2)
                .any(|pair| pair[0] == "-c" && pair[1].starts_with("notify="))
                && !args.iter().any(|arg| {
                    arg.starts_with("-cnotify=") || arg.starts_with("--config=notify=")
                }) =>
        {
            result.push("-c".into());
            let path = config_dir.join("agent-hooks/codex-notify.sh");
            result.push(format!(
                "notify=[{}]",
                serde_json::to_string(&path.to_string_lossy()).expect("path serializes")
            ));
        }
        _ => {}
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Catches: a successful probe publishes unusable stdout/stderr as Claude help.
    #[test]
    fn successful_probe_without_command_rows_is_unavailable() {
        let dir = tempfile::TempDir::new().unwrap();
        assert!(
            dir.path()
                .starts_with(crate::test_support::test_temp_root())
        );
        let truncated = RECORDED_CLAUDE_HELP.split("Commands:").next().unwrap();
        for (case, help) in [
            ("whitespace", " \n\t"),
            ("truncated", truncated),
            ("header-only", "Commands:\n  \n"),
            ("recorded", RECORDED_CLAUDE_HELP),
        ] {
            let help_path = dir.path().join(format!("{case}.txt"));
            std::fs::write(&help_path, help).unwrap();
            for (stream, redirect) in [("stdout", ""), ("stderr", " >&2")] {
                let binary = crate::test_support::fake_ssh_script(
                    &format!("claude-help-{case}-{stream}"),
                    &format!(
                        "cat {}{redirect}",
                        shell_quote(&help_path.to_string_lossy())
                    ),
                    &format!("type \"{}\"{redirect}", help_path.display()),
                );
                let actual = crate::agent::cli_help(&binary.to_string_lossy());
                assert_eq!(actual.is_some(), case == "recorded", "{case} on {stream}");
            }
        }
    }

    /// Catches: a hyphen in an ordinary prompt disables native status hooks.
    #[test]
    fn prompt_with_hyphen_retains_native_settings() {
        let dir = tempfile::TempDir::new().unwrap();
        let _guard = tuic_core::config_dir::set_override(dir.path().to_path_buf());
        let binary = dir.path().join("missing-claude");
        for prompt in ["fix-bug", "Explain the remote-control failure"] {
            let args = vec![prompt.to_string()];
            let actual = augment_args("claude", &binary.to_string_lossy(), &args, dir.path());
            assert!(
                actual.iter().any(|arg| arg == "--settings"),
                "prompt {prompt:?} lost native status settings: {actual:?}"
            );
        }
    }

    /// Catches: a failed help probe adds launch settings to the mcp subcommand.
    #[test]
    fn unavailable_help_does_not_corrupt_subcommand_arguments() {
        let dir = tempfile::TempDir::new().unwrap();
        let _guard = tuic_core::config_dir::set_override(dir.path().to_path_buf());
        let binary = dir.path().join("missing-claude");
        let args = vec!["mcp".into(), "list".into()];
        assert_eq!(
            augment_args("claude", &binary.to_string_lossy(), &args, dir.path()),
            args,
            "a failed help probe must not add settings to a subcommand"
        );
    }

    /// Catches: failed help injects root settings into the recorded auth subcommand.
    #[test]
    fn unavailable_help_preserves_recorded_auth_subcommand() {
        let dir = tempfile::TempDir::new().unwrap();
        assert!(
            dir.path()
                .starts_with(crate::test_support::test_temp_root())
        );
        let _guard = tuic_core::config_dir::set_override(dir.path().to_path_buf());
        let binary = dir.path().join("missing-claude");
        let args = vec!["auth".into(), "status".into()];
        assert_eq!(
            augment_args("claude", &binary.to_string_lossy(), &args, dir.path()),
            args,
            "recorded auth command must retain argv when help is unavailable"
        );
    }

    /// Catches: launch-scoped settings corrupt a verb or its alias, while a
    /// positional prompt gets misclassified as a command.
    #[test]
    fn claude_subcommands_use_captured_help_and_preserve_prompt_launches() {
        let dir = tempfile::TempDir::new().unwrap();
        let _guard = tuic_core::config_dir::set_override(dir.path().to_path_buf());
        let help = include_str!("../tests/fixtures/agent-help/claude-2026-10-04.txt");
        let help_path = dir.path().join("claude-help.txt");
        std::fs::write(&help_path, help).unwrap();
        let script = crate::test_support::fake_ssh_script(
            "claude-recorded-help-launch",
            &format!("cat {}", shell_quote(&help_path.to_string_lossy())),
            &format!("type \"{}\"", help_path.display()),
        );
        let binary = script.to_string_lossy();
        for verb in [
            "auth",
            "doctor",
            "mcp",
            "plugin",
            "plugins",
            "update",
            "upgrade",
            "remote-control",
        ] {
            let args = vec![verb.into()];
            assert_eq!(
                augment_args("claude", &binary, &args, dir.path()),
                args,
                "{verb}"
            );
        }
        for args in [
            vec![],
            vec!["prompt".into()],
            vec!["fix-bug".into()],
            vec!["--resume".into(), "x".into()],
        ] {
            let mut expected = args.clone();
            expected.extend([
                "--settings".into(),
                dir.path()
                    .join("agent-hooks/claude.json")
                    .to_string_lossy()
                    .into_owned(),
            ]);
            assert_eq!(augment_args("claude", &binary, &args, dir.path()), expected);
        }
        let args = vec!["remote-control".into(), "--resume".into(), "x".into()];
        assert_eq!(augment_args("claude", &binary, &args, dir.path()), args);
    }

    #[cfg(unix)]
    #[test]
    fn screen_probe_is_skipped_when_its_answer_cannot_change_launch_args() {
        let script = crate::test_support::fake_ssh_script(
            "screen-lazy-probe",
            "printf x >> \"${0%/*}/screen-lazy-probe.invoked\"; printf '%s\\n' '--no-alt-screen'",
            "echo --no-alt-screen",
        );
        let marker = script.with_file_name("screen-lazy-probe.invoked");
        let _ = std::fs::remove_file(&marker);
        let binary = script.to_string_lossy();
        for (agent, args) in [
            ("codex", vec!["--no-alt-screen".into()]),
            ("codex", vec!["exec".into(), "echo".into()]),
        ] {
            let _ = augment_args(agent, &binary, &args, Path::new("/unused"));
            assert!(!marker.exists(), "{agent} {args:?} needlessly ran --help");
        }
    }

    /// Catches: UI spelling diverges from installed ego, or unset injects a default.
    #[test]
    fn ego_permission_values_match_recorded_help_without_implicit_defaults() {
        use crate::config::{AgentSettings, AgentsConfig, EgoPermissionMode, EgoSandbox};
        let dir = tempfile::TempDir::new_in(crate::test_support::test_temp_root()).unwrap();
        let _guard = tuic_core::config_dir::set_override(dir.path().to_path_buf());
        let help = include_str!("../tests/fixtures/agent-help/ego-run-2026-10-06.txt");
        let resume_help = include_str!("../tests/fixtures/agent-help/ego-resume-2026-10-06.txt");
        for (key, flag, advertised, values) in [
            (
                "ego_mode",
                "--mode",
                "Permission mode: ",
                vec!["plan", "default", "edits", "auto", "yolo"],
            ),
            (
                "ego_sandbox",
                "--sandbox",
                "Filesystem confinement: ",
                vec!["ro", "workspace"],
            ),
        ] {
            for value in values {
                for recorded in [help, resume_help] {
                    assert!(recorded.contains(flag));
                    let options = recorded
                        .lines()
                        .find_map(|line| line.trim().strip_prefix(advertised))
                        .unwrap();
                    assert!(options.split('|').any(|option| option == value));
                }
                let settings: AgentSettings =
                    serde_json::from_value(serde_json::json!({key: value})).unwrap();
                let config = AgentsConfig {
                    agents: [("ego".into(), settings)].into(),
                    ..Default::default()
                };
                crate::config::save_agents_config(crate::config::load_agents_config(), config)
                    .unwrap();
                assert_eq!(
                    build_agent_launch_args("ego", "ego", &["run".into(), "task".into()]),
                    ["run", flag, value, "task"]
                );
            }
        }
        let mut config = AgentsConfig::default();
        config.agents.insert("ego".into(), AgentSettings::default());
        let args = vec!["run".into(), "--mode".into(), "auto".into(), "task".into()];
        crate::config::save_agents_config(crate::config::load_agents_config(), config.clone())
            .unwrap();
        assert_eq!(build_agent_launch_args("ego", "ego", &args), args);
        config.agents.get_mut("ego").unwrap().ego_mode = Some(EgoPermissionMode::Plan);
        config.agents.get_mut("ego").unwrap().ego_sandbox = Some(EgoSandbox::Workspace);
        crate::config::save_agents_config(crate::config::load_agents_config(), config).unwrap();
        assert_eq!(
            build_agent_launch_args(
                "ego",
                "ego",
                &[
                    "run".into(),
                    "--yolo".into(),
                    "--mode=auto".into(),
                    "--sandbox".into(),
                    "off".into(),
                    "--".into(),
                    "--mode=yolo".into()
                ]
            ),
            [
                "run",
                "--mode",
                "plan",
                "--sandbox",
                "workspace",
                "--",
                "--mode=yolo"
            ]
        );
        // Catches: a valueless override consumes another option or prompt text.
        for (args, suffix) in [
            (
                vec!["run", "--mode", "--sandbox", "off", "--", "--mode=yolo"],
                vec!["--", "--mode=yolo"],
            ),
            (
                vec!["run", "--mode", "--verbose", "task"],
                vec!["--verbose", "task"],
            ),
            (vec!["run", "--sandbox"], vec![]),
        ] {
            let args = args.into_iter().map(String::from).collect::<Vec<_>>();
            let expected = [
                vec!["run", "--mode", "plan", "--sandbox", "workspace"],
                suffix,
            ]
            .concat();
            assert_eq!(build_agent_launch_args("ego", "ego", &args), expected);
            assert_eq!(augment_args("ego", "ego", &args, dir.path()), expected);
        }
        for command in [
            "fork",
            "compact",
            "export",
            "ls",
            "usage",
            "models",
            "hooks",
            "doctor",
            "acp",
            "a2a",
            "auth",
            "mcp",
            "config",
            "rules",
            "mcp-server",
        ] {
            let recorded = include_str!("../tests/fixtures/agent-help/ego-2026-10-06.txt");
            assert!(
                recorded
                    .lines()
                    .any(|line| line.trim_start().starts_with(command))
            );
            assert_eq!(
                build_agent_launch_args("ego", "ego", &[command.into()]),
                [command]
            );
        }
        // Catches: an invalid enum discards the valid sibling permission field.
        for (document, expected) in [
            (
                serde_json::json!({"ego_mode": "invalid", "ego_sandbox": "ro"}),
                vec!["run", "--sandbox", "ro"],
            ),
            (
                serde_json::json!({"ego_mode": "edits", "ego_sandbox": "off"}),
                vec!["run", "--mode", "edits"],
            ),
            (
                serde_json::json!({"ego_mode": 42, "ego_sandbox": {}}),
                vec!["run"],
            ),
        ] {
            let settings: AgentSettings = serde_json::from_value(document).unwrap();
            let config = AgentsConfig {
                agents: [("ego".into(), settings)].into(),
                ..Default::default()
            };
            crate::config::save_agents_config(crate::config::load_agents_config(), config).unwrap();
            assert_eq!(
                build_agent_launch_args("ego", "ego", &["run".into()]),
                expected
            );
        }
    }

    /// Catches: either terminal or MCP spawn bypasses persisted ego permissions.
    #[test]
    fn ego_permissions_reach_terminal_and_mcp_argument_builders() {
        use crate::config::{AgentSettings, AgentsConfig, EgoPermissionMode, EgoSandbox};
        let dir = tempfile::TempDir::new_in(crate::test_support::test_temp_root()).unwrap();
        let _guard = tuic_core::config_dir::set_override(dir.path().to_path_buf());
        let config = AgentsConfig {
            agents: [(
                "ego".into(),
                AgentSettings {
                    ego_mode: Some(EgoPermissionMode::Edits),
                    ego_sandbox: Some(EgoSandbox::Ro),
                    ..Default::default()
                },
            )]
            .into(),
            ..Default::default()
        };
        crate::config::save_agents_config(AgentsConfig::default(), config).unwrap();
        for args in [
            vec![],
            vec!["run".into(), "task".into()],
            vec!["resume".into(), "session".into()],
        ] {
            let terminal = build_agent_launch_args("ego", "ego", &args);
            let spawn = augment_args("ego", "ego", &args, dir.path());
            assert_eq!(terminal, spawn);
            assert!(
                terminal
                    .windows(4)
                    .any(|flags| flags == ["--mode", "edits", "--sandbox", "ro"])
            );
        }
        assert_eq!(
            build_agent_launch_args("git", "git", &[]),
            Vec::<String>::new()
        );
    }

    #[cfg(unix)]
    #[test]
    fn terminal_launch_args_share_the_structured_spawn_screen_policy() {
        let script = crate::test_support::fake_ssh_script(
            "screen-terminal-launch-args",
            "printf '%s\\n' '--no-alt-screen --mini'",
            "echo --no-alt-screen --mini",
        );
        let binary = script.to_string_lossy();
        assert_eq!(
            build_agent_launch_args("codex", &binary, &["resume".into()]),
            ["--no-alt-screen", "resume"]
        );
        assert_eq!(
            build_agent_launch_args("codex", &binary, &["exec".into()]),
            ["exec"]
        );
        assert_eq!(
            build_agent_launch_args("opencode", &binary, &["run".into()]),
            ["run"]
        );
        assert_eq!(
            build_agent_launch_args("codex", &binary, &["--no-alt-screen".into()]),
            ["--no-alt-screen"]
        );
        assert_eq!(
            build_agent_launch_args("grok", &binary, &["--fullscreen".into()]),
            ["--no-alt-screen", "--fullscreen"]
        );
        assert_eq!(
            build_agent_launch_args("gemini", &binary, &[]),
            Vec::<String>::new()
        );

        let older = crate::test_support::fake_ssh_script(
            "screen-terminal-older-codex",
            "printf '%s\\n' 'old help'",
            "echo old help",
        );
        assert_eq!(
            build_agent_launch_args("codex", &older.to_string_lossy(), &["resume".into()]),
            ["resume"]
        );
    }

    #[cfg(unix)]
    #[test]
    fn per_agent_screen_setting_disables_the_structured_flag() {
        let dir = tempfile::TempDir::new().unwrap();
        let _guard = tuic_core::config_dir::set_override(dir.path().to_path_buf());
        let mut config = crate::config::AgentsConfig::default();
        config.agents.insert(
            "codex".into(),
            crate::config::AgentSettings {
                prevent_alt_screen: Some(false),
                ..Default::default()
            },
        );
        crate::config::save_agents_config(crate::config::AgentsConfig::default(), config).unwrap();
        let binary = crate::test_support::fake_ssh_script(
            "screen-setting-codex",
            "printf '%s\\n' '--no-alt-screen'",
            "echo --no-alt-screen",
        );
        assert_eq!(
            build_agent_launch_args("codex", &binary.to_string_lossy(), &["resume".into()]),
            ["resume"]
        );
    }

    #[test]
    fn actual_agent_argument_builder_defaults_to_native_scrollback() {
        for (agent, flag) in [
            ("codex", "--no-alt-screen"),
            ("grok", "--no-alt-screen"),
            ("opencode", "--mini"),
        ] {
            let script = crate::test_support::fake_ssh_script(
                &format!("screen-help-{agent}"),
                &format!("printf '%s\\n' '{flag}'"),
                &format!("echo {flag}"),
            );
            let binary = script.to_string_lossy();
            let args = vec!["resume".to_string()];
            let built = augment_args(agent, &binary, &args, Path::new("/unused"));
            assert_eq!(built.first().map(String::as_str), Some(flag), "{agent}");
            assert_eq!(built.get(1).map(String::as_str), Some("resume"), "{agent}");
        }
    }

    #[test]
    fn screen_flags_are_placed_before_subcommands_only_when_supported() {
        let args = vec!["resume".to_string(), "abc".to_string()];
        assert_eq!(
            add_screen_flag("codex", &args, args.clone(), true),
            ["--no-alt-screen", "resume", "abc"]
        );
        assert_eq!(add_screen_flag("codex", &args, args.clone(), false), args);
        let args = vec!["--model".to_string(), "fast".to_string()];
        assert_eq!(
            add_screen_flag("grok", &args, args.clone(), true),
            ["--no-alt-screen", "--model", "fast"]
        );
        assert_eq!(
            add_screen_flag("opencode", &args, args.clone(), true),
            ["--mini", "--model", "fast"]
        );
    }

    #[test]
    fn screen_flags_respect_existing_flags_and_subcommands() {
        let args = vec!["--no-alt-screen".to_string()];
        assert_eq!(add_screen_flag("codex", &args, args.clone(), true), args);
        let args = vec!["run".to_string(), "task".to_string()];
        assert_eq!(add_screen_flag("opencode", &args, args.clone(), true), args);
    }

    /// Gated with its only caller, which is unix-only because the hooks it
    /// builds are POSIX shell. Without the gate this is dead code on Windows,
    /// and the Windows job warns about it where nothing reads the warning.
    #[cfg(unix)]
    fn hook_commands(document: &Value, event: &str) -> Vec<String> {
        document["hooks"][event]
            .as_array()
            .expect("event hook groups")
            .iter()
            .flat_map(|group| group["hooks"].as_array().expect("hooks in group").iter())
            .map(|hook| hook["command"].as_str().expect("command hook").to_string())
            .collect()
    }

    #[test]
    fn explicit_agent_flags_are_not_overridden() {
        let root = Path::new("/config");
        assert_eq!(
            augment_args_when(true, "claude", &["--bare".into()], root),
            vec!["--bare"]
        );
        assert_eq!(
            augment_args_when(true, "claude", &["--settings=x".into()], root),
            vec!["--settings=x"]
        );
        assert_eq!(
            augment_args_when(
                true,
                "codex",
                &["-c".into(), "notify=['mine']".into()],
                root
            ),
            vec!["-c", "notify=['mine']"]
        );
    }
    #[test]
    fn generated_assets_have_protocol_and_ownership_markers() {
        let dir = tempfile::TempDir::new().unwrap();
        regenerate_launch_assets(dir.path()).unwrap();
        let claude = std::fs::read_to_string(dir.path().join("agent-hooks/claude.json")).unwrap();
        let codex =
            std::fs::read_to_string(dir.path().join("agent-hooks/codex-notify.sh")).unwrap();
        assert!(claude.contains(SENTINEL));
        assert!(claude.contains("UserPromptSubmit"));
        assert!(codex.contains("agent-turn-complete"));
        assert!(codex.contains("7770;state=idle"));
    }

    #[test]
    fn setting_off_keeps_claude_and_codex_argv_byte_for_byte() {
        let root = Path::new("/config");
        for agent in ["claude", "codex"] {
            let original = vec!["--model".to_string(), "test".to_string()];
            assert_eq!(augment_args_when(false, agent, &original, root), original);
        }
    }

    /// Unix only because it runs the hook commands, and the TUIC one is a POSIX
    /// shell script that reads the controlling tty through `ps -o tty=`. The
    /// additivity it proves is Claude's, not the platform's.
    ///
    /// DEFERRED (2026-09-15) — `hook_command` and `codex_script` are generated
    /// on every platform, so a Windows install writes agent hooks that no shell
    /// there can run and the agent-state badge never leaves its initial value.
    /// Giving Windows its own hook shape is a feature, not part of making the
    /// suite pass; raised with Boss rather than guessed at here.
    #[cfg(unix)]
    #[test]
    fn claude_settings_hooks_are_additive_with_same_event_global_and_project_hooks() {
        let dir = tempfile::TempDir::new().unwrap();
        let global_marker = dir.path().join("global-ran");
        let project_marker = dir.path().join("project-ran");
        let global = serde_json::json!({
            "hooks": {"UserPromptSubmit": [{"hooks": [{
                "type": "command",
                "command": format!("printf global > {}", shell_quote(&global_marker.to_string_lossy()))
            }]}]}
        });
        let project = serde_json::json!({
            "hooks": {"UserPromptSubmit": [{"hooks": [{
                "type": "command",
                "command": format!("printf project > {}", shell_quote(&project_marker.to_string_lossy()))
            }]}]}
        });
        let tuic = claude_document();

        // Claude documents `--settings` as an additional settings source. Model
        // that source loading directly: same-event arrays concatenate rather
        // than one source replacing another.
        let commands = [&global, &project, &tuic]
            .into_iter()
            .flat_map(|source| hook_commands(source, "UserPromptSubmit"))
            .collect::<Vec<_>>();
        assert_eq!(
            commands.len(),
            3,
            "all same-event sources must remain loaded"
        );
        assert!(commands[2].contains("7770;state=prompt"));

        for command in commands {
            let status = std::process::Command::new("/bin/sh")
                .arg("-c")
                .arg(command)
                .status()
                .unwrap();
            assert!(
                status.success(),
                "every loaded same-event hook must execute"
            );
        }
        assert_eq!(std::fs::read_to_string(global_marker).unwrap(), "global");
        assert_eq!(std::fs::read_to_string(project_marker).unwrap(), "project");
    }
}

#[cfg(test)]
mod critic_1302_tests {
    use super::*;

    /// Catches: two boots on one config dir (desktop + `tuic-remote`) tear
    /// `claude.json`, so a `claude --settings` spawn reads half a document.
    #[test]
    fn concurrent_boots_never_expose_a_torn_claude_json() {
        let dir = tempfile::TempDir::new().unwrap();
        let path = dir.path().join("agent-hooks/claude.json");
        regenerate_launch_assets_at_boot(dir.path());
        let stop = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
        let writers: Vec<_> = (0..4)
            .map(|_| {
                let root = dir.path().to_path_buf();
                let stop = stop.clone();
                std::thread::spawn(move || {
                    while !stop.load(std::sync::atomic::Ordering::Relaxed) {
                        regenerate_launch_assets_at_boot(&root);
                    }
                })
            })
            .collect();
        for _ in 0..300 {
            let bytes = std::fs::read(&path).expect("claude.json always present");
            serde_json::from_slice::<serde_json::Value>(&bytes).expect("never torn");
        }
        stop.store(true, std::sync::atomic::Ordering::Relaxed);
        for writer in writers {
            writer.join().unwrap();
        }
    }

    /// Catches: `codex-notify.sh` is renamed into place as 0600 and chmod'ed
    /// afterwards, so a codex spawn racing a second boot finds it not executable.
    #[cfg(unix)]
    #[test]
    fn codex_notify_script_is_never_visible_without_the_execute_bit() {
        use std::os::unix::fs::PermissionsExt;
        let dir = tempfile::TempDir::new().unwrap();
        let path = dir.path().join("agent-hooks/codex-notify.sh");
        regenerate_launch_assets_at_boot(dir.path());
        let stop = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
        let writer = {
            let root = dir.path().to_path_buf();
            let stop = stop.clone();
            std::thread::spawn(move || {
                while !stop.load(std::sync::atomic::Ordering::Relaxed) {
                    regenerate_launch_assets_at_boot(&root);
                }
            })
        };
        let mut bad = 0;
        for _ in 0..2000 {
            let mode = std::fs::metadata(&path).unwrap().permissions().mode();
            if mode & 0o100 == 0 {
                bad += 1;
            }
        }
        stop.store(true, std::sync::atomic::Ordering::Relaxed);
        writer.join().unwrap();
        assert_eq!(bad, 0, "observed the script without its execute bit {bad}x");
    }
}

#[cfg(test)]
#[path = "agent_hook_launch_critic1400_tests.rs"]
mod critic1400_tests;

#[cfg(test)]
mod attachment_grant_tests {
    // Catches: a confined ego launch has no read-only upload grant.
    #[test]
    fn attachment_launch_grants_ego_read_only_directory() {
        let root = tempfile::tempdir_in(crate::test_support::test_temp_root()).unwrap();
        let directory = root.path().join("attachments/pty-ego");
        let args = super::launch_args(
            "ego",
            "ego",
            &["--sandbox=workspace".into()],
            root.path(),
            &directory,
        );
        assert!(args.contains(&format!("--read-only-dir={}", directory.display())));
        assert!(args.contains(&"--sandbox=workspace".into()));
    }

    // Catches: a prepended grant makes ego parse run/resume as the root prompt, rejecting the real prompt.
    #[test]
    fn attachment_ego_grant_stays_after_subcommand() {
        let root = tempfile::tempdir_in(crate::test_support::test_temp_root()).unwrap();
        let directory = root.path().join("attachments/pty-ego");
        for verb in ["run", "resume"] {
            let args = super::launch_args(
                "ego",
                "ego",
                &[verb.into(), "--sandbox=ro".into(), "task".into()],
                root.path(),
                &directory,
            );
            assert_eq!(args[0], verb, "the ego subcommand must precede its options");
            assert_eq!(args[1], format!("--read-only-dir={}", directory.display()));
        }
    }

    // Catches: Claude/Gemini launch without the directory grant, or codex/grok get an unknown flag.
    #[test]
    fn attachment_launch_grants_only_harnesses_that_need_them() {
        let root = tempfile::tempdir_in(crate::test_support::test_temp_root()).unwrap();
        for (agent, flag) in [
            ("claude", Some("--add-dir")),
            ("gemini", Some("--include-directories")),
            ("codex", None),
            ("grok", None),
            ("ego", Some("--read-only-dir")),
        ] {
            let args = super::launch_args(
                agent,
                agent,
                &[],
                root.path(),
                &root.path().join("attachments/pty-granted"),
            );
            if let Some(flag) = flag {
                assert!(
                    args.iter().any(|arg| arg
                        == &format!(
                            "{flag}={}",
                            root.path().join("attachments/pty-granted").display()
                        )),
                    "missing grant for {agent}"
                );
            } else {
                assert!(!args.iter().any(|arg| arg.starts_with("--add-dir")
                    || arg.starts_with("--include-directories")
                    || arg.starts_with("--set=roots=")));
            }
        }
    }
}
