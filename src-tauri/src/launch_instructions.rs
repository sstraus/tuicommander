//! Launch-time TUIC output-marker instructions (story 1810-c28c).
//!
//! The MCP `instructions` field reaches an agent once per session at best (Claude
//! does not refresh it on reconnect, Codex never surfaces it). Every agent below
//! also takes extra system instructions at launch, and the marker matrix of
//! 2026-10-12 proved that text survives compact, clear and resume. So TUIC writes
//! the effective text to one file per agent and passes it through the agent's own
//! flag on structured launches (`agent_hook_launch::launch_args`).
//!
//! Agents typed in a TUIC shell (also through an alias or a wrapper script that
//! runs the agent by name) get the same text from the PATH shim in
//! `shell_integration`, or from `OPENCODE_CONFIG_CONTENT` for OpenCode, the only
//! agent with a native env var that appends instructions (verified live,
//! 2026-10-12). Launch-capable agents do not receive a duplicate MCP playbook;
//! absolute-path launches that bypass their shim must supply instructions explicitly.
use crate::config::{AgentsConfig, AppConfig};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU8, Ordering};

/// Agents with a verified launch-time instruction channel (installed `--help`,
/// 2026-10-12: claude 2.1.286, codex 0.162.1, grok 1.0.50, opencode 1.18.30,
/// goose 1.49.0, pi 0.84.2, ego 0.1.0).
pub(crate) const AGENTS: [&str; 7] = ["claude", "codex", "grok", "opencode", "goose", "pi", "ego"];

/// Tokens the TUIC parser anchors on; exposed with the preview metadata.
pub(crate) const REQUIRED_MARKERS: [&str; 3] = ["intent:", "suggest:", "💬 "];

const HEADER: &str = "## TUIC output markers\n\n\
Required even under concision rules; omission breaks the UI. Each marker is the first text on its own line, outside quotes and code fences. Only 💬 may also start the text of a list item.\n\n";
const CONNECTION: &str = "- Connection: once per MCP connection or reconnect, your first message starts with `TUICommander v{VER} is connected.`";
const CONNECTION_INTENT: &str = " and the next line is the `intent:` line";
const INTENT: &str = "- `intent: <desc> (<title>)` — the first text of your turn, before your first tool call, when the user sends a new request, changes direction, or you resume after an interruption or compaction. On any other turn, do not emit it. `<desc>`: the work in progress, present tense. `<title>`: ≤3 words, spaces not hyphens, the last parenthesis on the line.\n  Example: `intent: Fixing the mobile tab bar height (Mobile tab bar)`\n";
const SUGGEST: &str = "- `suggest: [ A | B | C ]` — once, as the last line of the turn that completes the task or asks the user to choose. Never while work continues. EXACTLY 3 items separated by ` | `, each ≤40 chars.\n";
const ANSWER: &str = "- `💬 ` — prefix every sentence that directly answers the user's question with `💬 ` at row start or at the start of a list item’s text (marker + space). Exclude status, tool and hook/task notices.\n";

/// The shipped text. A marker turned off for the agent is left out, so the
/// launch text never demands what the MCP instructions withhold.
fn marker_text(intent: bool, suggest: bool) -> String {
    let mut out = String::from(HEADER);
    out.push_str(CONNECTION);
    if intent {
        out.push_str(CONNECTION_INTENT);
    }
    out.push_str(".\n");
    if intent {
        out.push_str(INTENT);
    }
    if suggest {
        out.push_str(SUGGEST);
    }
    out.push_str(ANSWER);
    out
}

/// `global AND (per_agent ?? true)` — the one rule for both instruction surfaces.
pub(crate) fn marker_flags(
    app: &AppConfig,
    agents: &AgentsConfig,
    agent: Option<&str>,
) -> (bool, bool) {
    let settings = agent.and_then(|agent| agents.agents.get(agent));
    (
        app.intent_tab_title && settings.and_then(|s| s.intent_tab_title).unwrap_or(true),
        app.suggest_followups && settings.and_then(|s| s.suggest_followups).unwrap_or(true),
    )
}

/// Boss text (2026-10-22), verbatim. Present only when mdkb is installed.
pub(crate) const MDKB_RULE: &str = "## Code search\nUse MDKB for all code searches, including exact strings, symbols, and surrounding context. Prefer targeted retrieval over reading entire files. Grep, rg, and Bash-based searches are fallback tools only when MDKB cannot cover the target. Do not attempt alternative search commands when a search is blocked.\n";

/// 0 = not checked, 1 = absent, 2 = installed. The check only stats files, and
/// runs once per process, then again when the launch files are rewritten for a
/// config change or an mdkb install or uninstall.
static MDKB_INSTALLED: AtomicU8 = AtomicU8::new(0);

fn mdkb_in(dirs: &[PathBuf]) -> bool {
    let names: &[&str] = if cfg!(windows) {
        &["mdkb.exe", "mdkb.cmd"]
    } else {
        &["mdkb"]
    };
    dirs.iter()
        .any(|dir| names.iter().any(|name| dir.join(name).is_file()))
}

fn mdkb_on_disk() -> bool {
    // A release build lacks the shell PATH, so the install locations are listed too.
    let mut dirs: Vec<PathBuf> = std::env::var_os("PATH")
        .map(|path| std::env::split_paths(&path).collect())
        .unwrap_or_default();
    if cfg!(windows) {
        if let Some(local) = std::env::var_os("LOCALAPPDATA") {
            dirs.push(PathBuf::from(local).join("Microsoft/WindowsApps"));
        }
    } else {
        dirs.extend(["/usr/local/bin", "/opt/homebrew/bin"].map(PathBuf::from));
    }
    if let Some(home) = dirs::home_dir() {
        dirs.extend([home.join(".local/bin"), home.join(".cargo/bin")]);
    }
    mdkb_in(&dirs)
}

fn mdkb_installed() -> bool {
    match MDKB_INSTALLED.load(Ordering::Relaxed) {
        0 => {
            let found = mdkb_on_disk();
            MDKB_INSTALLED.store(if found { 2 } else { 1 }, Ordering::Relaxed);
            found
        }
        state => state == 2,
    }
}

const PROGRESS_DUTY: &str = "Record finished work with `progress type=done`; use `type=blocked` only when you stop for the user's answer, not while independent work continues.\n";

/// Stable module IDs are persisted by Settings; text remains generated by TUIC.
pub(crate) const MODULES: [(&str, &str); 5] = [
    (
        "working_with_user",
        "## Working with the user\nOpen decisions and long material as Markdown in a tab with `ui action=tab`; keep simple answers in chat. Use one `- [ ]` checkbox per option. Give the current state, consequences and recommendation with each question. Reread the file, apply checked choices and `<!--tweak:...-->` comments, answer in the file and remove resolved comments. Avoid blocking interactive prompts when the user can be away: put the question in the document and continue independent work. Use `telegram` to reach an away user; use `secret` for secrets, never chat.\n",
    ),
    (
        "waiting",
        "## Waiting and long work\nRun long work in the background and end the turn; TUIC wakes you on completion. Never poll or use `sleep` to wait. Submit terminal commands with `session action=submit session_id=<id> input=<text>` once: text and Enter together, never split calls or poll afterward.\n",
    ),
    (
        "git_worktree",
        "## Git and worktrees\nCreate and remove worktrees only with `repo action=worktree_create` / `repo action=worktree_remove`, never `git worktree add/remove`: TUIC tracks the checkout and session. Never `git switch` or `git checkout <branch>` in your session's own worktree: switching closes the session.\n",
    ),
    (
        "process_safety",
        "## Process safety\nStop only PIDs you started and recorded. Never use `pkill` or `killall` by pattern: other agents carry matching text. Never walk up the process tree to choose a victim: TUIC owns the ancestors and all sessions. Stop your dev servers, watchers and browser sessions before finishing. Never start a second desktop TUICommander instance except through `make agent-test`, which isolates port, config folder, binary name, shortcuts and microphone.\n",
    ),
    (
        "children",
        "## Child agents\nStay within the assigned scope; do not delegate. Report out-of-scope defects rather than fixing them. Send exactly one final `RESULT` or `BLOCKED` to the parent with `agent action=send to=<parent> message=<report>`: changes, commands, evidence and remaining gaps. Do not send heartbeats.\n",
    ),
];

/// Convert replacement-era exact default copies once, preserving real custom text.
/// Empty legacy replacements keep their old disabled behavior through an explicit flag.
pub(crate) fn migrate_additions(agents: &mut AgentsConfig) -> bool {
    if agents.launch_additions_migrated {
        return false;
    }
    fn convert(text: &mut Option<String>, disabled: &mut bool) {
        let Some(value) = text else { return };
        if value.is_empty() {
            *disabled = true;
            *text = None;
            return;
        }
        // Match the old rendered version as well as its editable {VER} form.
        let mut normalized = value.clone();
        if let Some(start) = normalized.find("TUICommander v") {
            let start = start + "TUICommander v".len();
            if let Some(end) = normalized[start..].find(" is connected.") {
                normalized.replace_range(start..start + end, "{VER}");
            }
        }
        if [false, true].into_iter().any(|intent| {
            [false, true]
                .into_iter()
                .any(|suggest| normalized == marker_text(intent, suggest))
        }) {
            *text = None;
        }
    }
    convert(
        &mut agents.launch_instructions,
        &mut agents.launch_instructions_disabled,
    );
    for settings in agents.agents.values_mut() {
        convert(
            &mut settings.launch_instructions,
            &mut settings.launch_instructions_disabled,
        );
    }
    agents.launch_additions_migrated = true;
    true
}

pub(crate) fn effective_text(app: &AppConfig, agents: &AgentsConfig, agent: &str) -> String {
    effective_text_for(app, agents, agent, false)
}

pub(crate) fn effective_text_for(
    app: &AppConfig,
    agents: &AgentsConfig,
    agent: &str,
    spawned_child: bool,
) -> String {
    effective_text_with(app, agents, agent, spawned_child, mdkb_installed())
}

fn effective_text_with(
    app: &AppConfig,
    agents: &AgentsConfig,
    agent: &str,
    spawned_child: bool,
    mdkb: bool,
) -> String {
    let settings = agents.agents.get(agent);
    if agents.launch_instructions_disabled
        || settings.is_some_and(|s| s.launch_instructions_disabled)
    {
        return String::new();
    }
    let enabled = |id: &str| {
        agents.launch_modules.get(id).copied().unwrap_or(true)
            && settings
                .and_then(|s| s.launch_modules.get(id))
                .copied()
                .unwrap_or(true)
    };
    let (intent, suggest) = marker_flags(app, agents, Some(agent));
    let mut text = if enabled("markers") {
        marker_text(intent, suggest && !spawned_child)
    } else {
        String::new()
    };
    for (id, module) in MODULES {
        if enabled(id) && (id != "children" || spawned_child) {
            if !text.is_empty() {
                text.push('\n');
            }
            text.push_str(module);
            if id == "working_with_user"
                && app.progress_tracking
                && settings.and_then(|s| s.progress_tracking).unwrap_or(true)
            {
                text.push_str(PROGRESS_DUTY);
            }
        }
    }
    if mdkb {
        if !text.is_empty() {
            text.push('\n');
        }
        text.push_str(MDKB_RULE);
    }
    for addition in [
        agents.launch_instructions.as_deref(),
        settings.and_then(|s| s.launch_instructions.as_deref()),
    ]
    .into_iter()
    .flatten()
    {
        if !addition.is_empty() {
            if !text.is_empty() {
                text.push_str("\n\n");
            }
            text.push_str(addition);
        }
    }
    text.replace("{VER}", env!("CARGO_PKG_VERSION"))
}

/// Codex has no file flag: its file holds the whole `-c` value, a TOML string.
fn file_content(agent: &str, text: &str) -> String {
    if text.trim().is_empty() {
        String::new()
    } else if agent == "codex" {
        format!(
            "developer_instructions={}",
            toml::Value::String(text.to_owned())
        )
    } else {
        text.to_owned()
    }
}

#[cfg(test)]
pub(crate) fn file_path(config_dir: &Path, agent: &str) -> PathBuf {
    file_path_for(config_dir, agent, false)
}

fn file_path_for(config_dir: &Path, agent: &str, child: bool) -> PathBuf {
    let extension = if agent == "codex" { "toml" } else { "md" };
    let suffix = if child { "-child" } else { "" };
    config_dir.join(format!(
        "agent-hooks/instructions-{agent}{suffix}.{extension}"
    ))
}

/// Write every agent's file. A disabled agent gets an empty file, which a
/// launch treats as "inject nothing".
pub(crate) fn write_files(config_dir: &Path) -> Result<(), String> {
    MDKB_INSTALLED.store(0, Ordering::Relaxed);
    write_current(config_dir)
}

fn write_current(config_dir: &Path) -> Result<(), String> {
    let app = crate::config::load_app_config();
    let agents = crate::config::load_agents_config();
    write_files_with(config_dir, &app, &agents)
}

pub(crate) fn write_files_with(
    config_dir: &Path,
    app: &AppConfig,
    agents: &AgentsConfig,
) -> Result<(), String> {
    std::fs::create_dir_all(config_dir.join("agent-hooks")).map_err(|e| e.to_string())?;
    for agent in AGENTS {
        for child in [false, true] {
            let content = file_content(agent, &effective_text_for(app, agents, agent, child));
            let path = file_path_for(config_dir, agent, child);
            if std::fs::read_to_string(&path).ok().as_deref() != Some(content.as_str()) {
                crate::config::persist_atomic(&path, content.as_bytes())?;
            }
        }
    }
    Ok(())
}

/// The agent's file after a refresh, or `None` when its text is empty.
fn current_file(agent: &str, config_dir: &Path, child: bool) -> Option<(PathBuf, String)> {
    if let Err(error) = write_current(config_dir) {
        tracing::error!(
            source = "agent_hooks",
            "Failed to write launch instructions: {error}"
        );
    }
    let path = file_path_for(config_dir, agent, child);
    let content = std::fs::read_to_string(&path).ok()?;
    (!content.trim().is_empty()).then_some((path, content))
}

fn has_flag(args: &[String], flags: &[&str]) -> bool {
    args.iter()
        .take_while(|arg| arg.as_str() != "--")
        .any(|arg| {
            flags
                .iter()
                .any(|flag| arg == flag || arg.starts_with(&format!("{flag}=")))
        })
}

fn first_is(args: &[String], verbs: &[&str]) -> bool {
    args.first()
        .is_some_and(|first| verbs.contains(&first.as_str()))
}

/// Where the flag goes, or `None` when this launch must not carry it: a
/// headless one-shot (its output is consumed by a program), a non-session
/// subcommand, or a user who already passes the same instruction flag.
/// Claude's flag goes last, where `--settings` goes: a run config may wrap
/// the agent (`/bin/sh -c ... {prompt}`) and read its first argument.
fn insertion(agent: &str, args: &[String]) -> Option<usize> {
    match agent {
        "claude" => (!crate::agent_hook_launch::claude_is_subcommand(args, "")
            && !has_flag(
                args,
                &[
                    "-p",
                    "--print",
                    "--system-prompt",
                    "--system-prompt-file",
                    "--append-system-prompt",
                    "--append-system-prompt-file",
                ],
            ))
        .then(|| {
            args.iter()
                .position(|arg| arg == "--")
                .unwrap_or(args.len())
        }),
        "codex" => (!first_is(args, &["exec", "e"])
            && !args
                .windows(2)
                .any(|pair| pair[0] == "-c" && pair[1].starts_with("developer_instructions="))
            && !args.iter().any(|arg| {
                arg.starts_with("-cdeveloper_instructions=")
                    || arg.starts_with("--config=developer_instructions=")
            }))
        .then_some(0),
        "grok" => (!has_flag(args, &["-p", "--single", "--rules"])).then_some(0),
        "goose" => {
            (first_is(args, &["session", "s"]) && !has_flag(args, &["--system"])).then_some(1)
        }
        "pi" => (!first_is(
            args,
            &[
                "install",
                "remove",
                "uninstall",
                "update",
                "list",
                "config",
                "auth",
            ],
        ) && !has_flag(args, &["-p", "--print", "--append-system-prompt"]))
        .then_some(0),
        "ego" => {
            if has_flag(args, &["--append-system"]) {
                None
            } else if first_is(args, &["resume"]) {
                Some(1)
            } else if args.first().is_some_and(|first| {
                first == "run"
                    || crate::agent_hook_launch::EGO_NON_RUN_COMMANDS.contains(&first.as_str())
            }) {
                None
            } else {
                Some(0)
            }
        }
        _ => None,
    }
}

/// Add the agent's instruction flag to a structured launch's argv.
pub(crate) fn args(agent: &str, args: Vec<String>, config_dir: &Path) -> Vec<String> {
    args_for(agent, args, config_dir, false)
}

pub(crate) fn args_for(
    agent: &str,
    args: Vec<String>,
    config_dir: &Path,
    child: bool,
) -> Vec<String> {
    if agent == "opencode" {
        return args;
    }
    let Some(at) = insertion(agent, &args) else {
        return args;
    };
    let Some((path, content)) = current_file(agent, config_dir, child) else {
        return args;
    };
    let path = path.to_string_lossy().into_owned();
    let flag: Vec<String> = match agent {
        "claude" => vec!["--append-system-prompt-file".into(), path],
        "codex" => vec!["-c".into(), content],
        // Glued with `=`: a text that starts with `-` is not read as a flag.
        "grok" => vec![format!("--rules={content}")],
        "goose" => vec![format!("--system={content}")],
        "pi" => vec!["--append-system-prompt".into(), path],
        "ego" => vec!["--append-system".into(), format!("@{path}")],
        _ => return args,
    };
    let mut result = args;
    result.splice(at..at, flag);
    result
}

/// OpenCode has no flag; it merges `OPENCODE_CONFIG_CONTENT` into its config.
/// Callers apply this before the caller's own environment, so an explicit
/// `OPENCODE_CONFIG_CONTENT` keeps precedence.
pub(crate) fn env(agent: &str, args: &[String], config_dir: &Path) -> Option<(String, String)> {
    env_for(agent, args, config_dir, false)
}

fn env_for(
    agent: &str,
    args: &[String],
    config_dir: &Path,
    child: bool,
) -> Option<(String, String)> {
    if agent != "opencode" || first_is(args, &["run"]) {
        return None;
    }
    let (path, _) = current_file(agent, config_dir, child)?;
    Some(("OPENCODE_CONFIG_CONTENT".into(), opencode_config(&path)))
}

/// Apply `env` to a structured spawn unless the caller already set the key.
pub(crate) fn apply_env(
    cmd: &mut portable_pty::CommandBuilder,
    agent: &str,
    args: &[String],
    config_dir: &Path,
) {
    apply_env_for(cmd, agent, args, config_dir, false);
}

pub(crate) fn apply_env_for(
    cmd: &mut portable_pty::CommandBuilder,
    agent: &str,
    args: &[String],
    config_dir: &Path,
    child: bool,
) {
    if let Some((key, value)) = env_for(agent, args, config_dir, child)
        && cmd.get_env(&key).is_none()
    {
        cmd.env(key, value);
    }
}

pub(crate) fn opencode_config(path: &Path) -> String {
    serde_json::json!({"instructions": [path.to_string_lossy()]}).to_string()
}

#[cfg(test)]
#[path = "launch_instructions_tests.rs"]
mod tests;
