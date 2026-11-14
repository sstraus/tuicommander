//! Launch-scoped native status integration for agent CLIs.

use crate::agent_hook::{HookTransport, SENTINEL, claude_hook_map_on};
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
    crate::agent_hook::hooks_supported_on_host()
        && crate::config::load_agents_config()
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

/// The Claude status line relay, next to `claude.json` (spike S5).
const CLAUDE_STATUS_LINE_RELAY: &str = "claude-statusline.sh";

fn claude_document(transport: &HookTransport, dir: &Path) -> Value {
    let mut hooks = Map::new();
    for (event, matcher, command) in claude_hook_map_on(transport) {
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
    let mut document = serde_json::json!({"hooks": hooks, "_tuic": SENTINEL});
    if *transport == HookTransport::Posix {
        // Only `type` and `command`: Claude merges `statusLine` field by field,
        // so the user's `padding` and `refreshInterval` still apply (S5).
        let relay = dir.join(CLAUDE_STATUS_LINE_RELAY);
        document["statusLine"] = serde_json::json!({
            "type": "command",
            "command": format!("sh {}", shell_quote(&relay.to_string_lossy())),
        });
    }
    // DEFERRED (2026-11-12) — Windows relay through tuic.exe agent-state; needs a
    // Windows Claude Code status line observation first. Without it the user
    // status line stays unchanged and the quota uses the active read.
    document
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
    __f=$(printf '\033]7770;state=idle\033\\')
    {{ {post}; }} >/dev/null 2>&1 || true
    ;;
esac
{chain}exit 0
"#,
        post = crate::agent_hook::post_frames(crate::agent_hook::TTY_FRAMES),
    )
}

/// Codex `notify` argv. POSIX runs the generated script, which chains the
/// user's own notify. Windows runs `tuic agent-state codex-notify -- <user
/// notify argv>`; Codex appends the payload, and the CLI chains the user's
/// command with it (#2632-97ef).
fn codex_notify_argv(transport: &HookTransport, config_dir: &Path) -> Vec<String> {
    match transport {
        HookTransport::Posix => vec![
            config_dir
                .join("agent-hooks/codex-notify.sh")
                .to_string_lossy()
                .into_owned(),
        ],
        HookTransport::Cli(cli) => [cli.as_str(), "agent-state", "codex-notify", "--"]
            .into_iter()
            .map(str::to_owned)
            .chain(codex_user_notify())
            .collect(),
    }
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
    regenerate_launch_assets_on(config_dir, HookTransport::host().as_ref())
}

/// `transport` is `None` on a host where generated hooks cannot run.
fn regenerate_launch_assets_on(
    config_dir: &Path,
    transport: Option<&HookTransport>,
) -> Result<(), String> {
    let Some(transport) = transport else {
        // Instruction documents are portable; these external hook commands are not.
        return crate::launch_instructions::write_files(config_dir);
    };
    let dir = config_dir.join("agent-hooks");
    std::fs::create_dir_all(&dir).map_err(|e| format!("create {}: {e}", dir.display()))?;
    let claude =
        serde_json::to_vec_pretty(&claude_document(transport, &dir)).map_err(|e| e.to_string())?;
    crate::config::persist_atomic(&dir.join("claude.json"), &claude)?;
    if *transport != HookTransport::Posix {
        // The Windows Codex notify is the CLI itself (`codex_notify_argv`).
        return crate::launch_instructions::write_files(config_dir);
    }
    crate::config::persist_atomic_with_mode(
        &dir.join(CLAUDE_STATUS_LINE_RELAY),
        crate::provider_quota::status_line::posix_script().as_bytes(),
        0o700,
    )?;
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

/// Workspace trust and the TUICommander MCP server for an ego child that TUIC
/// launches. Without trust the child stops at ego's trust question; without the
/// server it cannot mail its parent. A caller's own trust or `--mcp` choice wins.
/// The injected server comes with `--mcp-only` (ego 0.4.11): an enabled
/// configured TUIC server of the owner would publish the same tool names and
/// reach the owner's app, not the one that spawned the child (3020-5de1).
/// Like `--read-only-dir`, only the ego binary itself gets the options.
pub(crate) fn managed_ego_args(
    binary_path: &str,
    args: Vec<String>,
    trust: bool,
    mcp_server_file: Option<&Path>,
) -> Vec<String> {
    let binary_name = binary_path
        .rsplit(['/', '\\'])
        .next()
        .unwrap_or(binary_path);
    if binary_name.strip_suffix(".exe").unwrap_or(binary_name) != "ego"
        || args
            .first()
            .is_some_and(|first| EGO_NON_RUN_COMMANDS.contains(&first.as_str()))
    {
        return args;
    }
    let options = args.iter().take_while(|arg| *arg != "--");
    let mut has_trust = false;
    let mut has_mcp = false;
    for arg in options {
        has_trust |= arg == "--trust-workspace" || arg == "--trust-development-programs";
        has_mcp |= arg == "--mcp" || arg.starts_with("--mcp=");
    }
    let mut added = Vec::new();
    if trust && !has_trust {
        added.push("--trust-workspace".to_string());
    }
    if let Some(file) = mcp_server_file.filter(|_| !has_mcp) {
        added.push("--mcp".to_string());
        added.push(format!("@{}", file.display()));
        added.push("--mcp-only".to_string());
    }
    let insertion = usize::from(
        args.first()
            .is_some_and(|arg| ["run", "resume"].contains(&arg.as_str())),
    );
    let mut result = args;
    result.splice(insertion..insertion, added);
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

/// Apply Claude status hooks and the shared screen/permission policies to
/// commands typed into an existing terminal. Callers own shell quoting.
pub(crate) fn build_agent_launch_args(
    agent_type: &str,
    binary_path: &str,
    args: &[String],
) -> Vec<String> {
    if agent_type == "claude" {
        if is_claude_subcommand(agent_type, binary_path, args) {
            return args.to_vec();
        }
        return augment_launch(agent_type, binary_path, args, &crate::config::config_dir());
    }
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
    tokio::task::spawn_blocking(move || {
        crate::session_resume::prepare_shell_launch_args(&agent_type, &binary_path, &args)
    })
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
    augment_args_on(
        enabled,
        agent_type,
        args,
        config_dir,
        HookTransport::host().as_ref(),
    )
}

/// `transport` is `None` on a host where generated hooks cannot run.
fn augment_args_on(
    enabled: bool,
    agent_type: &str,
    args: &[String],
    config_dir: &Path,
    transport: Option<&HookTransport>,
) -> Vec<String> {
    let mut result = args.to_vec();
    let Some(transport) = transport.filter(|_| enabled) else {
        return result;
    };
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
            result.push(format!(
                "notify={}",
                serde_json::to_string(&codex_notify_argv(transport, config_dir))
                    .expect("argv serializes")
            ));
        }
        _ => {}
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;

    // Catches: restoring a discovered absolute Claude binary bypasses the shell
    // function and launches without the state-hook settings (#2858-2b1c).
    #[test]
    fn restored_claude_launch_args_include_state_hooks() {
        let dir = tempfile::TempDir::new_in(crate::test_support::test_temp_root()).unwrap();
        let config_dir = dir.path().join("config with spaces");
        let _guard = tuic_core::config_dir::set_override(config_dir.clone());
        let binary = dir.path().join("claude").to_string_lossy().into_owned();
        let args = vec!["--resume".into(), "recorded-session".into()];
        let actual = build_agent_launch_args("claude", &binary, &args);
        let mut expected = args.clone();
        if crate::agent_hook::hooks_supported_on_host() {
            expected.extend([
                "--settings".into(),
                config_dir
                    .join("agent-hooks/claude.json")
                    .to_string_lossy()
                    .into_owned(),
            ]);
        }
        assert_eq!(actual, expected, "absolute restore lost state hooks");
        assert_eq!(
            actual,
            augment_launch("claude", &binary, &args, &config_dir)
        );
        let shell_args = vec![
            "--resume".into(),
            crate::prompt::shell_quote("recorded-session"),
            "--model".into(),
            "\"$CLAUDE_MODEL\"".into(),
        ];
        let mut expected_shell = shell_args.clone();
        expected_shell.extend(
            expected[args.len()..]
                .iter()
                .map(|arg| crate::prompt::shell_quote(arg)),
        );
        assert_eq!(
            crate::session_resume::prepare_shell_launch_args("claude", &binary, &shell_args),
            expected_shell,
            "terminal preparation must preserve original shell words and quote new settings"
        );
        let custom = vec![
            crate::prompt::shell_quote("--settings"),
            crate::prompt::shell_quote("custom settings.json"),
        ];
        assert_eq!(
            crate::session_resume::prepare_shell_launch_args("claude", &binary, &custom),
            custom,
            "persisted quoted settings must not acquire another settings option"
        );
        for args in [
            vec!["--settings".into(), "custom settings.json".into()],
            vec!["--settings=custom.json".into()],
            vec!["--bare".into()],
            vec!["mcp".into(), "list".into()],
        ] {
            assert_eq!(build_agent_launch_args("claude", &binary, &args), args);
        }
        crate::config::save_agents_config(
            crate::config::AgentsConfig::default(),
            crate::config::AgentsConfig {
                agents: [(
                    "claude".into(),
                    crate::config::AgentSettings {
                        native_status_signals: Some(false),
                        ..Default::default()
                    },
                )]
                .into(),
                ..Default::default()
            },
        )
        .unwrap();
        assert_eq!(build_agent_launch_args("claude", &binary, &args), args);
    }

    // Catches: native Windows receives POSIX settings/notify hooks even with a saved override.
    #[test]
    fn windows_launch_does_not_inject_posix_hook() {
        let dir = tempfile::TempDir::new_in(crate::test_support::test_temp_root()).unwrap();
        for agent in ["claude", "codex"] {
            for enabled in [false, true] {
                for args in [
                    vec![],
                    vec!["--resume".into(), "session".into()],
                    vec!["--settings=custom.json".into()],
                    vec!["-cnotify=user".into()],
                ] {
                    assert_eq!(
                        augment_args_on(enabled, agent, &args, dir.path(), None),
                        args,
                        "Windows must preserve explicit argv without TUIC hooks: {agent}/{enabled}"
                    );
                }
            }
        }
    }

    // Catches: boot rewrites previously installed POSIX assets on Windows.
    #[test]
    fn windows_boot_preserves_existing_posix_assets() {
        let dir = tempfile::TempDir::new_in(crate::test_support::test_temp_root()).unwrap();
        regenerate_launch_assets_on(dir.path(), None).unwrap();
        let hooks = dir.path().join("agent-hooks");
        assert!(!hooks.join("claude.json").exists());
        assert!(!hooks.join("codex-notify.sh").exists());
        assert!(hooks.join("instructions-ego.md").exists());
        for name in ["claude.json", "codex-notify.sh"] {
            std::fs::write(hooks.join(name), b"user-owned").unwrap();
        }
        regenerate_launch_assets_on(dir.path(), None).unwrap();
        for name in ["claude.json", "codex-notify.sh"] {
            assert_eq!(std::fs::read(hooks.join(name)).unwrap(), b"user-owned");
        }
    }

    // Catches: a Windows host with the bundled CLI still gets no status hooks,
    // gets a POSIX Codex notify script it cannot run, or a Claude settings
    // file whose commands are `sh` (#2632-97ef).
    #[test]
    fn windows_launch_uses_the_cli_transport() {
        let dir = tempfile::TempDir::new_in(crate::test_support::test_temp_root()).unwrap();
        let cli = r"C:\Program Files\TUICommander\tuic.exe";
        let t = HookTransport::windows_cli(Path::new(cli)).unwrap();
        regenerate_launch_assets_on(dir.path(), Some(&t)).unwrap();
        let hooks = dir.path().join("agent-hooks");
        assert!(!hooks.join("codex-notify.sh").exists());
        let claude: Value =
            serde_json::from_slice(&std::fs::read(hooks.join("claude.json")).unwrap()).unwrap();
        let commands: Vec<&str> = claude["hooks"]
            .as_object()
            .unwrap()
            .values()
            .flat_map(|groups| groups.as_array().unwrap())
            .map(|group| group["hooks"][0]["command"].as_str().unwrap())
            .collect();
        assert_eq!(commands.len(), crate::agent_hook::claude_hook_map().len());
        for command in commands {
            assert!(
                command.starts_with(&format!("\"{cli}\" agent-state ")),
                "{command}"
            );
        }

        let claude = augment_args_on(true, "claude", &[], dir.path(), Some(&t));
        assert_eq!(claude[0], "--settings");
        let codex = augment_args_on(true, "codex", &[], dir.path(), Some(&t));
        assert_eq!(codex[0], "-c");
        let notify: Vec<String> =
            serde_json::from_str(codex[1].strip_prefix("notify=").unwrap()).unwrap();
        assert_eq!(notify[..4], [cli, "agent-state", "codex-notify", "--"]);
        assert_eq!(
            augment_args_on(false, "codex", &[], dir.path(), Some(&t)),
            Vec::<String>::new(),
            "the saved opt-out still wins"
        );
    }

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
            assert_eq!(
                actual.iter().any(|arg| arg == "--settings"),
                crate::agent_hook::hooks_supported_on_host(),
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
            if crate::agent_hook::hooks_supported_on_host() {
                expected.extend([
                    "--settings".into(),
                    dir.path()
                        .join("agent-hooks/claude.json")
                        .to_string_lossy()
                        .into_owned(),
                ]);
            }
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

    /// Catches: an ego child that stops at the workspace trust question or has no
    /// TUICommander MCP server (2881-4e8c), a second server added next to the
    /// caller's own `--mcp` choice, and options placed before the `run` verb.
    /// Also catches (3020-5de1): the owner's enabled configured TUIC server is
    /// attached next to the invocation server, so the child exits on duplicate
    /// tool names or, selected by name, mails the daily app instead of its own.
    #[test]
    fn managed_ego_child_gets_workspace_trust_and_the_bridge_server() {
        // ego 0.4.11, `ego run --help`, recorded 2026-11-13.
        let help = include_str!("../tests/fixtures/agent-help/ego-run-2026-11-13.txt");
        assert!(
            help.contains("--trust-workspace")
                && help.contains("--mcp <NAME|@FILE>")
                && help.contains("--mcp-only")
        );
        let file = Path::new("/attachments/ego-tuic-mcp.json");
        let args = |list: &[&str]| list.iter().map(|arg| arg.to_string()).collect::<Vec<_>>();

        assert_eq!(
            managed_ego_args("/bin/ego", args(&["run", "task"]), true, Some(file)),
            args(&[
                "run",
                "--trust-workspace",
                "--mcp",
                "@/attachments/ego-tuic-mcp.json",
                "--mcp-only",
                "task"
            ])
        );
        // No bridge binary: no invocation server, so the configured servers stay.
        assert_eq!(
            managed_ego_args("/bin/ego", args(&["run", "task"]), true, None),
            args(&["run", "--trust-workspace", "task"])
        );
        assert_eq!(
            managed_ego_args("/bin/ego", args(&["--mcp=tuic", "task"]), false, Some(file)),
            args(&["--mcp=tuic", "task"])
        );
        assert_eq!(
            managed_ego_args(
                "/bin/ego",
                args(&["--mcp", "tuic", "--trust-workspace", "task"]),
                true,
                Some(file)
            ),
            args(&["--mcp", "tuic", "--trust-workspace", "task"])
        );
        // skip_trust_dialog = false keeps ego's question; a wrapper takes no ego options.
        assert_eq!(
            managed_ego_args("/bin/ego", args(&["task"]), false, None),
            args(&["task"])
        );
        assert_eq!(
            managed_ego_args("/bin/sh", args(&["-c", "ego"]), true, Some(file)),
            args(&["-c", "ego"])
        );
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
        let dir = tempfile::TempDir::new_in(crate::test_support::test_temp_root()).unwrap();
        regenerate_launch_assets_on(dir.path(), Some(&HookTransport::Posix)).unwrap();
        let claude = std::fs::read_to_string(dir.path().join("agent-hooks/claude.json")).unwrap();
        let codex =
            std::fs::read_to_string(dir.path().join("agent-hooks/codex-notify.sh")).unwrap();
        assert!(claude.contains(SENTINEL));
        assert!(claude.contains("UserPromptSubmit"));
        assert!(codex.contains("agent-turn-complete"));
        assert!(codex.contains("7770;state=idle"));
    }

    // Catches: launch-scoped settings omit permission/failure hooks or subscribe to ordinary idle notices.
    #[test]
    fn claude_launch_document_installs_scoped_permission_events() {
        let document = claude_document(&HookTransport::Posix, Path::new("/config/agent-hooks"));
        let hooks = &document["hooks"];
        for event in ["PermissionRequest", "PostToolUseFailure", "StopFailure"] {
            assert!(!hooks[event].as_array().unwrap().is_empty(), "{event}");
        }
        assert_eq!(hooks["Notification"].as_array().unwrap().len(), 1);
        assert_eq!(hooks["Notification"][0]["matcher"], "permission_prompt");
        assert_eq!(hooks["PostToolUse"].as_array().unwrap().len(), 2);
        let dir = tempfile::tempdir_in(crate::test_support::test_temp_root()).unwrap();
        let path = dir.path().join("settings.json");
        let user = serde_json::json!({"hooks":{"Notification":[{"matcher":"idle_prompt","hooks":[{"type":"command","command":"user-owned"}]}]}});
        std::fs::write(&path, serde_json::to_vec(&user).unwrap()).unwrap();
        crate::agent_hook_commands::apply_at("claude", &path, true).unwrap();
        let installed: Value = serde_json::from_slice(&std::fs::read(&path).unwrap()).unwrap();
        assert_eq!(
            installed["hooks"]["Notification"][0],
            user["hooks"]["Notification"][0]
        );
        assert_eq!(
            installed["hooks"]["PermissionRequest"],
            hooks["PermissionRequest"]
        );
        assert_eq!(
            installed["hooks"]["PostToolUseFailure"],
            hooks["PostToolUseFailure"]
        );
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
        let tuic = claude_document(&HookTransport::Posix, Path::new("/config/agent-hooks"));

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
        regenerate_launch_assets_on(dir.path(), Some(&HookTransport::Posix)).unwrap();
        let stop = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
        let writers: Vec<_> = (0..4)
            .map(|_| {
                let root = dir.path().to_path_buf();
                let stop = stop.clone();
                std::thread::spawn(move || {
                    while !stop.load(std::sync::atomic::Ordering::Relaxed) {
                        regenerate_launch_assets_on(&root, Some(&HookTransport::Posix)).unwrap();
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
