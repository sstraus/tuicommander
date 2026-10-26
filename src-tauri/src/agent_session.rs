//! Cross-platform session file discovery for AI coding agents.
//!
//! When a user launches an agent manually (without TUICommander's `--session-id`
//! injection), the session ID is unknown. This module scans known session storage
//! directories to discover the most recently created, unclaimed session file.
//!
//! Supported agents and their storage layouts:
//!
//! | Agent  | Path                                        | ID format         |
//! |--------|---------------------------------------------|-------------------|
//! | claude | `~/.claude/sessions/<pid>.json` (exact), else `~/.claude/projects/<cwd-slug>/<UUID>.jsonl` | `sessionId` field / UUID filename stem |
//! | gemini | `~/.gemini/tmp/<hash>/chats/session-*.json` | JSON `sessionId` field |
//! | codex  | `~/.codex/sessions/YYYY/MM/DD/rollout-*-<UUID>.jsonl` | UUID in filename |
//! | goose  | SQLite `~/Library/Application Support/Block/goose/sessions/sessions.db` | name field (TUIC_SESSION) |
//! | ego    | `$EGO_HOME/sessions/<UUID>/lease` (`{"pid","heartbeat"}`, exact pid binding), default `~/.ego/sessions` | UUID directory name |

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime};

/// Session discovery ignores session files/dirs older than this. Discovery
/// fires on every idle↔busy transition (~every agent turn), not just the 30s
/// poll — so without a bound its cost grows with the user's entire session
/// history × open terminals. 300s (5 min) matches the Claude/Grok bound and is
/// comfortably longer than the gap between an agent starting and writing its
/// first session file, so a just-launched session is never aged out.
const SESSION_MAX_AGE: Duration = Duration::from_secs(300);

/// Scan a directory for agent session files and return the ID of the newest
/// unclaimed session, or `None` if none can be found.
///
/// # Parameters
/// - `agent_type`: one of `"claude"`, `"gemini"`, `"codex"`, `"goose"`, `"grok"`
/// - `cwd`: the terminal's working directory (used to compute project-scoped paths)
/// - `claimed_ids`: session IDs already assigned to other terminals — excluded from results
/// - `agent_pid`: PID of the running agent process. When provided, env vars that affect
///   session storage paths (`CLAUDE_CONFIG_DIR`, `GEMINI_CLI_HOME`, `CODEX_HOME`, `HOME`) are read
///   directly from the process's initial environment — the ground-truth source.
/// - `env_overrides`: fallback env overrides from the TUIC run config. Only used for keys
///   NOT found in the process env (i.e. process env takes precedence).
#[cfg_attr(feature = "desktop", tauri::command)]
pub(crate) async fn discover_agent_session(
    agent_type: String,
    cwd: String,
    claimed_ids: Vec<String>,
    agent_pid: Option<u32>,
    env_overrides: HashMap<String, String>,
) -> Option<DiscoveredSession> {
    match tokio::task::spawn_blocking(move || {
        discover_agent_session_blocking(agent_type, cwd, claimed_ids, agent_pid, env_overrides)
    })
    .await
    {
        Ok(found) => found,
        Err(error) => {
            tracing::warn!(%error, "Agent session discovery task failed");
            None
        }
    }
}

/// Blocking service for callers already running on a blocking owner.
pub(crate) fn discover_agent_session_blocking(
    agent_type: String,
    cwd: String,
    claimed_ids: Vec<String>,
    agent_pid: Option<u32>,
    env_overrides: HashMap<String, String>,
) -> Option<DiscoveredSession> {
    let session_id =
        discover_session_id(&agent_type, &cwd, &claimed_ids, agent_pid, &env_overrides)?;
    let launch_command = agent_pid.and_then(|pid| rebuild_launch_command(&agent_type, pid));
    let provider = (agent_type == "ego")
        .then(|| {
            let env = resolve_env_overrides(&agent_type, agent_pid, &env_overrides);
            ego_session_provider(
                &ego_sessions_dir(env.get("EGO_HOME").map(|s| s.as_str()))?,
                &session_id,
            )
        })
        .flatten();
    Some(DiscoveredSession {
        session_id,
        launch_command,
        provider,
    })
}

/// What discovery found: the agent's session id, plus the command that actually
/// launched it when the live process could be read.
///
/// The launch command is the half that outlives the process. At restore time the
/// pid is gone, so a session stored under `CLAUDE_CONFIG_DIR=~/.claude-private`
/// can only be resumed if that env was captured while the agent was still running.
#[derive(Debug, Clone, PartialEq, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct DiscoveredSession {
    pub session_id: String,
    pub launch_command: Option<String>,
    /// The model provider the session runs on, for agents whose store names it
    /// (ego: first segment of the manifest `model`). Drives the sidebar icon.
    pub provider: Option<String>,
}

fn discover_session_id(
    agent_type: &str,
    cwd: &str,
    claimed_ids: &[String],
    agent_pid: Option<u32>,
    env_overrides: &HashMap<String, String>,
) -> Option<String> {
    let env = resolve_env_overrides(agent_type, agent_pid, env_overrides);
    match agent_type {
        "claude" => discover_claude_session(
            cwd,
            claimed_ids,
            env.get("CLAUDE_CONFIG_DIR").map(|s| s.as_str()),
            agent_pid,
        ),
        "gemini" => discover_gemini_session(
            cwd,
            claimed_ids,
            env.get("GEMINI_CLI_HOME").map(|s| s.as_str()),
        ),
        "codex" => {
            discover_codex_session(cwd, claimed_ids, env.get("CODEX_HOME").map(|s| s.as_str()))
        }
        // Goose stores sessions in SQLite — no filesystem discovery.
        // Shell wrapper injects --name $TUIC_SESSION for deterministic binding.
        "goose" => None,
        "grok" => discover_grok_session(cwd, claimed_ids, agent_pid),
        "ego" => discover_ego_session(agent_pid, env.get("EGO_HOME").map(|s| s.as_str())),
        _ => None,
    }
}

/// Flags that select which session an agent opens. They are mutually exclusive
/// with the `--resume <id>` TUIC appends, so a rebuilt launch command must drop
/// them — otherwise a resume carries the *previous* run's session selection.
///
/// Only agents listed here get a rebuilt launch command; the rest fall back to
/// the run config, exactly as before. Adding one means verifying its flags
/// against the real CLI, not guessing them.
///
/// DEFERRED (2026-09-08) — gemini/codex/grok are exposed to the same config-dir
/// mismatch (`GEMINI_CLI_HOME`, `CODEX_HOME`) but codex selects a session with a
/// `resume` *subcommand* rather than a flag, so the drop rule below does not
/// transfer. Add them once each CLI's session surface is checked.
const AGENT_SESSION_FLAGS: &[(&str, &[&str])] = &[(
    "claude",
    &[
        "--resume",
        "-r",
        "--continue",
        "-c",
        "--session-id",
        "--fork-session",
    ],
)];

/// Rebuild the command that actually launched an agent, from its live process.
///
/// A shell alias is expanded before `exec`, so what TUIC has on file is not what
/// runs: the run config says `c2` with no env, while the process is really
/// `claude --dangerously-skip-permissions` under
/// `CLAUDE_CONFIG_DIR=~/.claude-private`. Both halves are needed at restore time,
/// when the pid is gone — resuming a `~/.claude-private` session with the default
/// run config sends `--resume <id>` to a binary that reads `~/.claude`, and Claude
/// answers `No conversation found with session ID`.
///
/// Returns `None` for agents with no entry in `AGENT_SESSION_FLAGS`, and when argv
/// cannot be read (Windows, or the process already exited).
///
/// DEFERRED (2026-09-08) — argv is kept whole apart from session flags and Start
/// Work's own brief (`without_seed_brief`), so an agent launched with some other
/// inline prompt (`claude 'fix this'`) rebuilds with that prompt attached and
/// re-sends it on resume. Telling a prompt from a flag value needs the CLI's flag
/// arity (`--model opus` takes one, `--add-dir a b c` takes many), which argv alone
/// does not carry.
fn rebuild_launch_command(agent_type: &str, pid: u32) -> Option<String> {
    let argv = crate::process_env::read_process_argv(pid)?;
    compose_launch_command(agent_type, &argv, read_agent_env_overrides(agent_type, pid))
}

/// The pure half of `rebuild_launch_command`: everything except reading the process.
fn compose_launch_command(
    agent_type: &str,
    argv: &[String],
    env: HashMap<String, String>,
) -> Option<String> {
    let session_flags = AGENT_SESSION_FLAGS
        .iter()
        .find(|(t, _)| *t == agent_type)
        .map(|(_, flags)| *flags)?;

    let mut env: Vec<(String, String)> = env.into_iter().collect();
    env.sort(); // stable output: the command is persisted and diffed against itself
    let mut parts: Vec<String> = env
        .into_iter()
        .map(|(key, value)| format!("{key}={}", quote_if_needed(&value)))
        .collect();

    let mut argv = without_seed_brief(argv).iter().peekable();
    parts.push(quote_if_needed(argv.next()?));
    while let Some(token) = argv.next() {
        if let Some((flag, directory)) = token.split_once('=')
            && ["--add-dir", "--include-directories"].contains(&flag)
            && is_tuic_attachment_grant(directory)
        {
            continue;
        }
        if session_flags.contains(&token.as_str()) {
            // Drop the value too, when the next token is one rather than a flag.
            if argv.peek().is_some_and(|next| !next.starts_with('-')) {
                argv.next();
            }
            continue;
        }
        parts.push(quote_if_needed(token));
    }
    Some(parts.join(" "))
}

fn is_tuic_attachment_grant(directory: &str) -> bool {
    let path = Path::new(directory);
    let Some(parent) = path
        .parent()
        .filter(|parent| parent.file_name().is_some_and(|name| name == "attachments"))
    else {
        return false;
    };
    // The session directory (and even attachments/) may already be gone. Resolve
    // the existing config root on both sides, not the expired child itself.
    let Some(root) = parent.parent() else {
        return false;
    };
    let config = crate::config::config_dir();
    std::fs::canonicalize(root).unwrap_or_else(|_| root.to_path_buf())
        == std::fs::canonicalize(&config).unwrap_or(config)
        && path
            .file_name()
            .is_some_and(|name| name.to_string_lossy().starts_with("pty-"))
}

/// First line of the brief Start Work passes as a launch argument (`SEED_BRIEF_OPEN`
/// in `agentLaunchCommand.ts`). It is host-generated, so a restored session must not
/// replay it: a resumed agent would take it as a second task submission.
const SEED_BRIEF_OPEN: &str = "<untrusted-content source=\"work item\">";

/// `argv` without a trailing Start Work brief and the `--` that precedes it.
fn without_seed_brief(argv: &[String]) -> &[String] {
    match argv {
        [rest @ .., last] if last.starts_with(SEED_BRIEF_OPEN) => match rest {
            [head @ .., sep] if sep == "--" => head,
            _ => rest,
        },
        _ => argv,
    }
}

/// Shell-quote a token only when it needs it, so a rebuilt command stays readable
/// in the terminal (`claude --resume <id>`, not `'claude' '--resume' '<id>'`).
fn quote_if_needed(value: &str) -> String {
    let safe = !value.is_empty()
        && value
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || "_@%+=:,./-".contains(c));
    if safe {
        value.to_string()
    } else {
        crate::prompt::shell_quote(value)
    }
}

/// Merge env overrides: process env (ground truth) takes precedence over run config fallback.
fn resolve_env_overrides(
    agent_type: &str,
    agent_pid: Option<u32>,
    run_config_env: &HashMap<String, String>,
) -> HashMap<String, String> {
    let mut merged = run_config_env.clone();
    if let Some(pid) = agent_pid {
        let process_env = read_agent_env_overrides(agent_type, pid);
        merged.extend(process_env);
    }
    merged
}

/// Env vars that affect session storage paths, keyed by agent type.
///
/// Agents without entries here have no known env var that changes their
/// session storage path (amp, cursor, droid use cloud or hardcoded paths;
/// goose has no override).
const AGENT_ENV_VARS: &[(&str, &[&str])] = &[
    ("claude", &["CLAUDE_CONFIG_DIR"]),
    ("gemini", &["GEMINI_CLI_HOME"]),
    ("codex", &["CODEX_HOME"]),
    ("opencode", &["OPENCODE_DATA_DIR"]),
    ("ego", &["EGO_HOME"]),
];

/// Read session-relevant env vars from a running agent process.
///
/// Uses the process's initial environment (set at exec time) via platform-specific
/// APIs: `KERN_PROCARGS2` on macOS, `/proc/pid/environ` on Linux, PEB on Windows.
///
/// Returns a map suitable for passing to `discover_agent_session`/`verify_agent_session`.
pub(crate) fn read_agent_env_overrides(agent_type: &str, pid: u32) -> HashMap<String, String> {
    let mut overrides = HashMap::new();
    let vars = AGENT_ENV_VARS
        .iter()
        .find(|(t, _)| *t == agent_type)
        .map(|(_, vars)| *vars)
        .unwrap_or(&[]);
    for var in vars {
        if let Some(val) = crate::process_env::read_process_env_var(pid, var) {
            overrides.insert((*var).to_string(), val);
        }
    }
    overrides
}

/// Claude Code's project directory for `cwd`, as a path.
///
/// The `String`-returning `claude_project_dir` below is the IPC face of this;
/// callers inside the backend want a `PathBuf` to `join` onto and should use
/// this one rather than round-tripping through a `Result<String, _>`.
pub(crate) fn claude_project_dir_path(cwd: &str, config_dir: Option<&str>) -> Option<PathBuf> {
    Some(claude_projects_dir(config_dir)?.join(path_to_claude_slug(cwd)))
}

/// Return the absolute path to Claude Code's project directory for a given CWD.
/// E.g. `/Users/foo/bar` → `~/.claude/projects/-Users-foo-bar`.
#[cfg_attr(feature = "desktop", tauri::command)]
pub(crate) fn claude_project_dir(
    cwd: String,
    claude_config_dir: Option<String>,
) -> Result<String, String> {
    let path = claude_project_dir_path(&cwd, claude_config_dir.as_deref())
        .ok_or_else(|| "Could not determine home directory".to_string())?;
    path.to_str()
        .map(|s| s.to_string())
        .ok_or_else(|| "Project path contains non-UTF-8 characters".to_string())
}

// ─── Claude ──────────────────────────────────────────────────────────────────

/// Base directory for Claude Code session transcripts.
///
/// When `config_dir_override` is set (from `CLAUDE_CONFIG_DIR` in the agent's
/// run config), uses `<override>/projects/`. Otherwise defaults to `~/.claude/projects/`.
fn claude_projects_dir(config_dir_override: Option<&str>) -> Option<PathBuf> {
    if let Some(dir) = config_dir_override {
        Some(PathBuf::from(dir).join("projects"))
    } else {
        dirs::home_dir().map(|h| h.join(".claude").join("projects"))
    }
}

/// Encode a filesystem path to the slug Claude Code uses as a directory name.
///
/// Claude replaces every non-ASCII-alphanumeric character with `-`,
/// including path separators, spaces, `.`, `_`, and the Windows drive colon.
///
/// Example: `/Users/foo.bar/my_project` → `-Users-foo-bar-my-project`
fn path_to_claude_slug(path: &str) -> String {
    // Normalise separators so this works on Windows too
    let normalised = path.replace('\\', "/");
    // Strip trailing separator to avoid a trailing dash in the slug
    let trimmed = normalised.trim_end_matches('/');
    // Claude 2.1.286 replaces non-alphanumeric characters, including the
    // Windows drive colon, before joining the slug under projects/.
    // DEFERRED (2026-10-04): Claude hashes slugs longer than 200 characters;
    // matching that suffix is outside this Windows profile-name correction.
    trimmed
        .chars()
        .map(|c| if c.is_ascii_alphanumeric() { c } else { '-' })
        .collect()
}

/// Directory where Claude Code registers one JSON file per *running* process.
///
/// Sibling of `projects/`, so it follows `CLAUDE_CONFIG_DIR` the same way.
fn claude_sessions_dir(config_dir_override: Option<&str>) -> Option<PathBuf> {
    if let Some(dir) = config_dir_override {
        Some(PathBuf::from(dir).join("sessions"))
    } else {
        dirs::home_dir().map(|h| h.join(".claude").join("sessions"))
    }
}

/// Read the session id Claude Code registered for `pid`.
///
/// Claude Code (≥2.1.2xx) writes `<config dir>/sessions/<pid>.json` while it runs:
/// `{"pid":57597,"sessionId":"<uuid>","cwd":"…","version":"…", …}`. That file is the
/// only *exact* pid→session binding available — the transcript is opened, appended
/// and closed per write, so it never shows up in the process's open descriptors.
///
/// Three checks guard against a stale file left by a dead process whose pid was
/// recycled: the recorded pid must match, the recorded cwd must be the terminal's,
/// and the transcript it names must still exist. A file that fails any of them is
/// ignored, and discovery falls back to the mtime heuristic.
fn claude_session_for_pid(pid: u32, cwd: &str, config_dir: Option<&str>) -> Option<String> {
    let path = claude_sessions_dir(config_dir)?.join(format!("{pid}.json"));
    let contents = std::fs::read_to_string(path).ok()?;
    let entry: serde_json::Value = serde_json::from_str(&contents).ok()?;

    if entry.get("pid")?.as_u64()? != u64::from(pid) {
        return None;
    }
    if normalize_cwd(entry.get("cwd")?.as_str()?) != normalize_cwd(cwd) {
        return None;
    }
    let session_id = entry.get("sessionId")?.as_str()?.to_string();
    verify_claude_session(&session_id, cwd, config_dir).then_some(session_id)
}

/// Resolve the Claude session running in a terminal.
///
/// Prefers the exact pid→session binding Claude registers on disk. Only when that
/// is unavailable (older Claude, or the agent pid could not be read) does it fall
/// back to "newest unclaimed `.jsonl` under `~/.claude/projects/<cwd-slug>/`".
///
/// That fallback is not a binding and cannot be made into one: N Claude tabs in one
/// folder all scan the same directory, so whichever tab polls first takes the newest
/// transcript regardless of whose it is, and the rest take another tab's session or
/// nothing at all. Measured on a live instance with 6 Claude tabs: 3 held no session
/// id and one held a *different* tab's — which is why every tab resumed with
/// `--continue` and landed in the same conversation (issue #119). The `claimed_ids`
/// dedup only stops two tabs holding the *same* id; it cannot tell whose is whose.
///
/// A pid hit therefore ignores `claimed_ids`: the pid is ground truth, so a tab that
/// previously claimed that id by mtime is the one that is wrong, and it re-resolves
/// to its own session on its next poll.
fn discover_claude_session(
    cwd: &str,
    claimed_ids: &[String],
    config_dir: Option<&str>,
    agent_pid: Option<u32>,
) -> Option<String> {
    if let Some(pid) = agent_pid
        && let Some(id) = claude_session_for_pid(pid, cwd, config_dir)
    {
        return Some(id);
    }

    let slug = path_to_claude_slug(cwd);
    let project_dir = claude_projects_dir(config_dir)?.join(&slug);

    newest_unclaimed_file(
        &project_dir,
        |name| {
            // Filename must be `<UUID>.jsonl`
            name.strip_suffix(".jsonl")
                .filter(|stem| is_uuid(stem))
                .map(|stem| stem.to_string())
        },
        claimed_ids,
        Some(SESSION_MAX_AGE),
    )
}

// ─── Gemini ──────────────────────────────────────────────────────────────────

/// Base directory for Gemini session temp files.
///
/// When `cli_home` is set (from `GEMINI_CLI_HOME`), uses `<cli_home>/.gemini/tmp/`.
/// Otherwise defaults to `~/.gemini/tmp/`.
fn gemini_tmp_dir(cli_home: Option<&str>) -> Option<PathBuf> {
    if let Some(home) = cli_home {
        Some(PathBuf::from(home).join(".gemini").join("tmp"))
    } else {
        dirs::home_dir().map(|h| h.join(".gemini").join("tmp"))
    }
}

/// Gemini CLI stores sessions under `~/.gemini/tmp/<project-hash>/chats/`.
/// The hash is a SHA-256 of the absolute project path. Rather than recomputing
/// the hash (which would require adding sha2 as a dependency), we scan ALL
/// project directories under `~/.gemini/tmp/` and take the newest session file
/// across all of them.
///
/// DEFERRED (2026-09-06) — that scan is NOT project-scoped, contrary to what this
/// comment claimed until issue #119: every project's `chats/` dir is visited, so a
/// gemini tab can bind to a session started in a different project, which Claude
/// (slug dir) and Codex (recorded cwd) both reject. Nor does gemini publish a
/// pid→session registry the way Claude and grok do, so the within-a-folder
/// ambiguity fixed for those two also remains here. Fixing it needs ground truth
/// on gemini's on-disk format — either the hash input (to scope the scan) or a cwd
/// field in the session JSON — and gemini is not installed on this machine, so the
/// alternative was to guess. Do not "fix" this by assuming SHA-256 of the path:
/// verify against a real install first.
///
/// When `cli_home` is set (from `GEMINI_CLI_HOME` in the agent's process env),
/// uses `<cli_home>/.gemini/tmp/`. Otherwise defaults to `~/.gemini/tmp/`.
fn discover_gemini_session(
    _cwd: &str,
    claimed_ids: &[String],
    cli_home: Option<&str>,
) -> Option<String> {
    let tmp_dir = gemini_tmp_dir(cli_home)?;
    if !tmp_dir.exists() {
        return None;
    }

    // Collect (mtime, sessionId) from all session-*.json files across all project dirs
    let now = SystemTime::now();
    let mut candidates: Vec<(SystemTime, String)> = Vec::new();

    let Ok(project_entries) = std::fs::read_dir(&tmp_dir) else {
        return None;
    };
    for proj in project_entries.filter_map(|e| e.ok()) {
        let chats_dir = proj.path().join("chats");
        if !chats_dir.is_dir() {
            continue;
        }
        collect_gemini_session_files(&chats_dir, now, &mut candidates);
    }

    // Sort newest first, return first unclaimed
    candidates.sort_by_key(|a| std::cmp::Reverse(a.0));
    candidates
        .into_iter()
        .find(|(_, id)| !claimed_ids.contains(id))
        .map(|(_, id)| id)
}

fn collect_gemini_session_files(
    chats_dir: &PathBuf,
    now: SystemTime,
    out: &mut Vec<(SystemTime, String)>,
) {
    let Ok(entries) = std::fs::read_dir(chats_dir) else {
        return;
    };
    for entry in entries.filter_map(|e| e.ok()) {
        let name = entry.file_name().to_string_lossy().to_string();
        if !name.starts_with("session-") || !name.ends_with(".json") {
            continue;
        }
        let Ok(meta) = entry.metadata() else { continue };
        let Ok(mtime) = meta.modified() else { continue };
        // Skip stale files by mtime BEFORE reading/parsing their JSON — discovery
        // fires ~every agent turn, so this caps I/O to recently-touched sessions
        // instead of the user's full history.
        if now.duration_since(mtime).unwrap_or_default() > SESSION_MAX_AGE {
            continue;
        }
        // Read the sessionId from the JSON content
        if let Ok(contents) = std::fs::read_to_string(entry.path())
            && let Some(session_id) = extract_json_string_field(&contents, "sessionId")
            && is_uuid(&session_id)
        {
            out.push((mtime, session_id));
        }
    }
}

/// Extract a top-level string field from JSON.
fn extract_json_string_field(json: &str, field: &str) -> Option<String> {
    serde_json::from_str::<serde_json::Value>(json)
        .ok()?
        .get(field)?
        .as_str()
        .map(str::to_string)
}

// ─── Codex ───────────────────────────────────────────────────────────────────

/// Base directory for Codex session files.
///
/// When `codex_home` is set (from `CODEX_HOME`), uses `<codex_home>/sessions/`.
/// Otherwise defaults to `~/.codex/sessions/`.
fn codex_sessions_dir(codex_home: Option<&str>) -> Option<PathBuf> {
    if let Some(home) = codex_home {
        Some(PathBuf::from(home).join("sessions"))
    } else {
        dirs::home_dir().map(|h| h.join(".codex").join("sessions"))
    }
}

/// Codex CLI stores sessions under `~/.codex/sessions/YYYY/MM/DD/`.
/// Files are named `rollout-<timestamp>-<UUID>.jsonl`.
///
/// Unlike Claude and Grok, Codex does not partition its sessions by project — every
/// project's rollouts land in the same date tree. So the walk is filtered twice, and
/// both filters are load-bearing:
///
/// - **age**: `SESSION_MAX_AGE`, per file, matching the Claude/Gemini/Grok siblings.
///   The date-dir prune alone bounds the walk to 24-48h, which is not a recency check:
///   a terminal opened now could bind to a session abandoned this morning.
/// - **cwd**: the rollout's own recorded working directory must match this session's.
///   Without it a fresh terminal in project A took the globally-newest unclaimed
///   session and resumed project B's history.
///
/// Both filters together still leave two Codex tabs in the *same* folder telling
/// only by mtime, which is the ambiguity issue #119 reported for Claude. Claude and
/// grok were fixed by reading their pid→session registries; Codex 0.153 publishes
/// no equivalent — checked `session_index.jsonl` (id + name + updated_at only), the
/// rollout `session_meta` payload (no pid) and `codex --help` (no `--session-id` to
/// force one at launch). So this stays a heuristic until Codex exposes a binding;
/// there is nothing on disk to make it exact.
fn discover_codex_session(
    cwd: &str,
    claimed_ids: &[String],
    codex_home: Option<&str>,
) -> Option<String> {
    discover_codex_session_at(cwd, claimed_ids, codex_home, SystemTime::now())
}

fn discover_codex_session_at(
    cwd: &str,
    claimed_ids: &[String],
    codex_home: Option<&str>,
    now: SystemTime,
) -> Option<String> {
    let sessions_root = codex_sessions_dir(codex_home)?;

    if !sessions_root.exists() {
        return None;
    }

    let wanted_cwd = normalize_cwd(cwd);
    let index = CODEX_INDEXES.get_with(sessions_root.clone(), || {
        std::sync::Arc::new(CodexIndex::new(&sessions_root))
    });
    let mut candidates = index.candidates(&sessions_root, now, &wanted_cwd);

    // Sort newest first
    candidates.sort_by_key(|a| std::cmp::Reverse(a.0));

    candidates
        .into_iter()
        .find(|(_, id)| !claimed_ids.contains(id))
        .map(|(_, id)| id)
}

/// Canonical form for comparing two working directories.
///
/// Falls back to the lexical path when the directory cannot be canonicalized (it may
/// have been deleted since the session started) so a missing directory degrades to an
/// exact string match rather than matching everything.
fn normalize_cwd(cwd: &str) -> PathBuf {
    let path = PathBuf::from(cwd);
    std::fs::canonicalize(&path).unwrap_or(path)
}

/// Read the working directory Codex recorded in a rollout file.
///
/// The first JSONL record is `{"type":"session_meta","payload":{...,"cwd":"…"}}` —
/// verified against Codex 0.122 and 0.142, which agree on the field's location.
/// Only that first line is read: the payload embeds the full base instructions, so a
/// rollout is easily megabytes, and only the head is needed.
fn read_codex_rollout_cwd(path: &Path) -> Option<PathBuf> {
    use std::io::{BufRead, BufReader, Read};

    let file = std::fs::File::open(path).ok()?;
    // Cap the read: a corrupt file with no newline must not pull an unbounded amount
    // of it into memory on a path that runs every agent turn.
    let mut head = String::new();
    BufReader::new(file.take(256 * 1024))
        .read_line(&mut head)
        .ok()?;

    let meta: serde_json::Value = serde_json::from_str(head.trim_end()).ok()?;
    let cwd = meta.get("payload")?.get("cwd")?.as_str()?;
    Some(normalize_cwd(cwd))
}

#[cfg(test)]
static CODEX_ROLLOUT_STATS: std::sync::LazyLock<dashmap::DashMap<PathBuf, usize>> =
    std::sync::LazyLock::new(dashmap::DashMap::new);

#[cfg(test)]
static CODEX_SCAN_DIAGNOSTICS: std::sync::LazyLock<dashmap::DashMap<PathBuf, Vec<String>>> =
    std::sync::LazyLock::new(dashmap::DashMap::new);

fn codex_rollout_metadata(path: &Path) -> std::io::Result<std::fs::Metadata> {
    #[cfg(test)]
    {
        *CODEX_ROLLOUT_STATS.entry(path.to_path_buf()).or_insert(0) += 1;
    }
    std::fs::metadata(path)
}

/// Shared per-store index: discovery polls inspect memory, while filesystem
/// events update only the rollout paths that moved. This remains heuristic
/// discovery, not a PID/session binding.
static CODEX_INDEXES: std::sync::LazyLock<moka::sync::Cache<PathBuf, std::sync::Arc<CodexIndex>>> =
    std::sync::LazyLock::new(|| moka::sync::Cache::builder().max_capacity(16).build());

#[derive(Default)]
struct CodexUpdates {
    paths: std::collections::HashSet<PathBuf>,
    rescan: bool,
    #[cfg(test)]
    diagnostics: Vec<String>,
}

struct CodexCandidate {
    modified: SystemTime,
    id: String,
    cwd: PathBuf,
}

#[derive(Default)]
struct CodexCandidates {
    days: Vec<PathBuf>,
    files: HashMap<PathBuf, CodexCandidate>,
    initialized: bool,
}

struct CodexIndex {
    watcher: std::sync::Mutex<Option<notify::RecommendedWatcher>>,
    updates: std::sync::Arc<std::sync::Mutex<CodexUpdates>>,
    state: std::sync::Mutex<CodexCandidates>,
}

impl CodexIndex {
    fn new(root: &Path) -> Self {
        use notify::Watcher;
        let updates = std::sync::Arc::new(std::sync::Mutex::new(CodexUpdates::default()));
        let pending = updates.clone();
        let watcher = notify::recommended_watcher(move |event: notify::Result<notify::Event>| {
            if let Ok(event) = &event
                && matches!(event.kind, notify::EventKind::Access(_))
            {
                return;
            }
            let mut updates = pending.lock().unwrap_or_else(|e| e.into_inner());
            #[cfg(test)]
            if updates.diagnostics.len() < 128 {
                updates.diagnostics.push(format!("event={event:?}"));
            }
            match event {
                Ok(event) if !event.need_rescan() => {
                    for path in event.paths {
                        if updates.paths.len() >= 4096 {
                            updates.paths.clear();
                            updates.rescan = true;
                            break;
                        }
                        updates.paths.insert(path);
                    }
                }
                _ => updates.rescan = true,
            }
        })
        .and_then(|mut watcher| {
            watcher.watch(root, notify::RecursiveMode::NonRecursive)?;
            Ok(watcher)
        })
        .map_err(|error| {
            #[cfg(test)]
            CODEX_SCAN_DIAGNOSTICS
                .entry(root.to_path_buf())
                .or_default()
                .push(format!("watcher setup failed: {error}"));
            tracing::warn!(%error, "Codex discovery watcher unavailable; using fresh scans")
        })
        .ok();
        Self {
            watcher: std::sync::Mutex::new(watcher),
            updates,
            state: std::sync::Mutex::new(CodexCandidates::default()),
        }
    }

    fn candidates(&self, root: &Path, now: SystemTime, cwd: &Path) -> Vec<(SystemTime, String)> {
        let mut state = self.state.lock().unwrap_or_else(|e| e.into_inner());
        #[cfg(test)]
        let stats_before: HashMap<_, _> = CODEX_ROLLOUT_STATS
            .iter()
            .filter(|entry| entry.key().starts_with(root))
            .map(|entry| (entry.key().clone(), *entry.value()))
            .collect();
        // Drain before reading: events arriving during the refresh remain queued
        // for the next lookup rather than being erased by this snapshot.
        let updates = std::mem::take(&mut *self.updates.lock().unwrap_or_else(|e| e.into_inner()));
        // Removing/replacing a watched directory invalidates its inode watch on
        // platforms such as inotify, even when the date paths stay identical.
        // Recreate the bounded watch set through the existing next-lookup path.
        if updates.rescan
            || updates.paths.contains(root)
            || state.days.iter().any(|day| {
                day.ancestors()
                    .take(3)
                    .any(|directory| updates.paths.contains(directory))
            })
        {
            CODEX_INDEXES.invalidate(root);
        }
        let days = codex_recent_day_dirs(root, chrono::Local::now());
        let mut watcher = self.watcher.lock().unwrap_or_else(|e| e.into_inner());
        if state.days != days && watcher.is_some() {
            use notify::Watcher;
            let active = watcher.as_mut().unwrap();
            // Never recursively watch the lifetime session tree: watch only the
            // two recent date subtrees and their shallow creation boundaries.
            let old_parents: std::collections::HashSet<_> = state
                .days
                .iter()
                .flat_map(|day| day.ancestors().skip(1).take(2))
                .collect();
            for day in &state.days {
                let _ = active.unwatch(day);
            }
            for parent in old_parents {
                let _ = active.unwatch(parent);
            }
            let parents: std::collections::HashSet<_> = days
                .iter()
                .flat_map(|day| day.ancestors().skip(1).take(2))
                .collect();
            let armed = parents
                .into_iter()
                .try_for_each(|parent| active.watch(parent, notify::RecursiveMode::NonRecursive))
                .and_then(|_| {
                    days.iter()
                        .try_for_each(|day| active.watch(day, notify::RecursiveMode::Recursive))
                });
            if let Err(error) = armed {
                #[cfg(test)]
                CODEX_SCAN_DIAGNOSTICS
                    .entry(root.to_path_buf())
                    .or_default()
                    .push(format!("date watch failed: {error}"));
                tracing::warn!(%error, "Codex discovery date watch failed; using fresh scans");
                *watcher = None;
            }
        }
        let directory_changed = updates.paths.iter().any(|path| {
            path.file_name()
                .and_then(|n| n.to_str())
                .and_then(extract_codex_uuid)
                .is_none()
        });
        #[cfg(test)]
        CODEX_SCAN_DIAGNOSTICS.entry(root.to_path_buf()).or_default().push(format!(
            "root={root:?}; initialized={}; watcher_available={}; rescan={}; days_changed={}; directory_changed={directory_changed}; pending_paths={:?}; events={:?}",
            state.initialized, watcher.is_some(), updates.rescan, state.days != days, updates.paths, updates.diagnostics
        ));
        if !state.initialized
            || watcher.is_none()
            || updates.rescan
            || state.days != days
            || directory_changed
        {
            state.files.clear();
            for day in &days {
                collect_codex_candidates(day, now, &mut state.files);
            }
            state.days = days;
            state.initialized = true;
        } else {
            for path in updates.paths {
                if state.days.iter().any(|day| path.starts_with(day)) {
                    update_codex_candidate(&path, now, &mut state.files);
                }
            }
        }
        #[cfg(test)]
        CODEX_SCAN_DIAGNOSTICS
            .entry(root.to_path_buf())
            .or_default()
            .push(format!(
                "metadata reads this refresh={:?}",
                CODEX_ROLLOUT_STATS
                    .iter()
                    .filter(|entry| entry.key().starts_with(root))
                    .filter_map(|entry| {
                        let delta =
                            *entry.value() - stats_before.get(entry.key()).copied().unwrap_or(0);
                        (delta > 0).then(|| (entry.key().clone(), delta))
                    })
                    .collect::<Vec<_>>()
            ));
        state.files.retain(|_, candidate| {
            now.duration_since(candidate.modified).unwrap_or_default() <= SESSION_MAX_AGE
        });
        state
            .files
            .values()
            .filter(|candidate| candidate.cwd == cwd)
            .map(|candidate| (candidate.modified, candidate.id.clone()))
            .collect()
    }
}

fn update_codex_candidate(
    path: &Path,
    now: SystemTime,
    out: &mut HashMap<PathBuf, CodexCandidate>,
) {
    // Replacement, deletion and unreadable/truncated headers all revoke the old
    // candidate before a fresh record can be accepted.
    out.remove(path);
    let Some(id) = path
        .file_name()
        .and_then(|n| n.to_str())
        .and_then(extract_codex_uuid)
    else {
        return;
    };
    let Some(modified) = codex_rollout_metadata(path)
        .ok()
        .and_then(|m| m.modified().ok())
    else {
        return;
    };
    if now.duration_since(modified).unwrap_or_default() > SESSION_MAX_AGE {
        return;
    }
    let Some(cwd) = read_codex_rollout_cwd(path) else {
        return;
    };
    out.insert(path.to_path_buf(), CodexCandidate { modified, id, cwd });
}

fn collect_codex_candidates(
    dir: &Path,
    now: SystemTime,
    out: &mut HashMap<PathBuf, CodexCandidate>,
) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in entries.filter_map(Result::ok) {
        let path = entry.path();
        if entry.file_type().is_ok_and(|kind| kind.is_dir()) {
            collect_codex_candidates(&path, now, out);
        } else {
            update_codex_candidate(&path, now, out);
        }
    }
}

/// Resolve the `<root>/YYYY/MM/DD` session dirs that can hold a session younger
/// than `SESSION_MAX_AGE`. With a 5-min bound only today matters, but yesterday
/// is kept too to stay correct across the midnight boundary and minor
/// clock/timezone skew between TUIC and the Codex writer (Codex partitions by
/// local date). Bounds the walk to ≤2 day-dirs regardless of total history.
fn codex_recent_day_dirs(root: &Path, now: chrono::DateTime<chrono::Local>) -> Vec<PathBuf> {
    use chrono::Datelike;
    let today = now.date_naive();
    let mut dates = vec![today];
    if let Some(yesterday) = today.pred_opt() {
        dates.push(yesterday);
    }
    dates
        .into_iter()
        .map(|d| {
            root.join(format!("{:04}", d.year()))
                .join(format!("{:02}", d.month()))
                .join(format!("{:02}", d.day()))
        })
        .filter(|p| p.is_dir())
        .collect()
}

/// Extract the UUID from a Codex session filename: `rollout-<ts>-<UUID>.jsonl`
/// The UUID is the last `-`-separated segment before `.jsonl`.
fn extract_codex_uuid(name: &str) -> Option<String> {
    let stem = name.strip_suffix(".jsonl")?;
    if !stem.starts_with("rollout-") {
        return None;
    }
    // UUID is 36 chars: 8-4-4-4-12 hex + dashes = 36
    if stem.len() < 37 {
        return None;
    }
    let candidate = &stem[stem.len() - 36..];
    if is_uuid(candidate) {
        Some(candidate.to_string())
    } else {
        None
    }
}

// ─── Session verification ────────────────────────────────────────────────────

/// Check whether a session file exists on disk for the given agent type and UUID.
///
/// Used at restore time to decide if `--resume <uuid>` is safe: if the session
/// file doesn't exist, the resume command would fail.
#[cfg_attr(feature = "desktop", tauri::command)]
pub(crate) fn verify_agent_session(
    agent_type: String,
    session_id: String,
    cwd: String,
    agent_pid: Option<u32>,
    env_overrides: HashMap<String, String>,
) -> bool {
    let env = resolve_env_overrides(&agent_type, agent_pid, &env_overrides);
    match agent_type.as_str() {
        "claude" => verify_claude_session(
            &session_id,
            &cwd,
            env.get("CLAUDE_CONFIG_DIR").map(|s| s.as_str()),
        ),
        "gemini" => verify_gemini_session(
            &session_id,
            &cwd,
            env.get("GEMINI_CLI_HOME").map(|s| s.as_str()),
        ),
        "codex" => verify_codex_session(&session_id, env.get("CODEX_HOME").map(|s| s.as_str())),
        "goose" => verify_goose_session(),
        "grok" => verify_grok_session(&session_id, &cwd),
        "ego" => verify_ego_session(&session_id, env.get("EGO_HOME").map(|s| s.as_str())),
        _ => false,
    }
}

/// Check if `~/.claude/projects/<slug>/<uuid>.jsonl` exists.
fn verify_claude_session(session_id: &str, cwd: &str, config_dir: Option<&str>) -> bool {
    if !is_uuid(session_id) {
        return false;
    }
    let Some(project_dir) =
        claude_projects_dir(config_dir).map(|d| d.join(path_to_claude_slug(cwd)))
    else {
        return false;
    };
    project_dir.join(format!("{session_id}.jsonl")).exists()
}

/// Check if any session file under `~/.gemini/tmp/*/chats/` contains this sessionId.
fn verify_gemini_session(session_id: &str, _cwd: &str, cli_home: Option<&str>) -> bool {
    if !is_uuid(session_id) {
        return false;
    }
    let Some(tmp_dir) = gemini_tmp_dir(cli_home) else {
        return false;
    };
    if !tmp_dir.exists() {
        return false;
    }
    let Ok(entries) = std::fs::read_dir(&tmp_dir) else {
        return false;
    };
    for proj in entries.filter_map(|e| e.ok()) {
        let chats_dir = proj.path().join("chats");
        if !chats_dir.is_dir() {
            continue;
        }
        let Ok(files) = std::fs::read_dir(&chats_dir) else {
            continue;
        };
        for f in files.filter_map(|e| e.ok()) {
            if let Ok(contents) = std::fs::read_to_string(f.path())
                && let Some(found_id) = extract_json_string_field(&contents, "sessionId")
                && found_id == session_id
            {
                return true;
            }
        }
    }
    false
}

/// Check if any Codex session file has this UUID in its filename.
fn verify_codex_session(session_id: &str, codex_home: Option<&str>) -> bool {
    if !is_uuid(session_id) {
        return false;
    }
    let Some(sessions_root) = codex_sessions_dir(codex_home) else {
        return false;
    };
    if !sessions_root.exists() {
        return false;
    }
    codex_session_exists(&sessions_root, session_id)
}

fn codex_session_exists(dir: &std::path::Path, target_id: &str) -> bool {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return false;
    };
    for entry in entries.filter_map(|e| e.ok()) {
        let path = entry.path();
        if path.is_dir() {
            if codex_session_exists(&path, target_id) {
                return true;
            }
        } else if let Some(name) = path.file_name().map(|n| n.to_string_lossy().to_string())
            && let Some(uuid) = extract_codex_uuid(&name)
            && uuid == target_id
        {
            return true;
        }
    }
    false
}

// ─── Goose ────────────────────────────────────────────────────────────────────

/// Goose stores sessions in SQLite — we can't query it without a dependency.
/// Optimistic check: return true if the sessions DB file exists, meaning the
/// user has used Goose. The resume command handles missing sessions gracefully.
fn verify_goose_session() -> bool {
    goose_db_path().map(|p| p.exists()).unwrap_or(false)
}

fn goose_db_path() -> Option<PathBuf> {
    dirs::data_dir().map(|d| {
        d.join("Block")
            .join("goose")
            .join("sessions")
            .join("sessions.db")
    })
}

// ─── Grok ─────────────────────────────────────────────────────────────────────

/// Base directory for grok session storage: `~/.grok/sessions/`.
fn grok_sessions_dir() -> Option<PathBuf> {
    dirs::home_dir().map(|h| h.join(".grok").join("sessions"))
}

/// Encode a filesystem path the way grok names its per-CWD session directory:
/// RFC-3986 percent-encoding of the absolute path, preserving the unreserved set
/// (ALPHA / DIGIT / `-` / `.` / `_` / `~`) and escaping everything else as `%XX`
/// (uppercase hex). E.g. `/Users/foo.bar/proj` → `%2FUsers%2Ffoo.bar%2Fproj`.
fn grok_path_encode(path: &str) -> String {
    let normalised = path.replace('\\', "/");
    let mut out = String::with_capacity(normalised.len());
    for b in normalised.bytes() {
        if b.is_ascii_alphanumeric() || matches!(b, b'-' | b'.' | b'_' | b'~') {
            out.push(b as char);
        } else {
            out.push('%');
            out.push_str(&format!("{b:02X}"));
        }
    }
    out
}

/// Pick the session `pid` registered for `cwd` out of grok's `active_sessions.json`.
///
/// The file is a flat array of `{"session_id","pid","cwd","opened_at"}` — grok's
/// equivalent of Claude's per-pid registry, and the only exact pid→session binding
/// it offers. Entries outlive their process (a dead pid from the previous day was
/// still listed), so the caller must still verify the session exists on disk.
fn select_grok_session_for_pid(contents: &str, pid: u32, cwd: &str) -> Option<String> {
    let entries: serde_json::Value = serde_json::from_str(contents).ok()?;
    let wanted_cwd = normalize_cwd(cwd);

    let mut matches = entries.as_array()?.iter().filter_map(|entry| {
        let registered_pid = u32::try_from(entry.get("pid")?.as_u64()?).ok()?;
        let owns_terminal = registered_pid == pid || {
            #[cfg(unix)]
            {
                // npm's Node bootstrap retains the foreground process group while
                // the native child publishes its own PID. Do not guess by CWD.
                match (i32::try_from(registered_pid), i32::try_from(pid)) {
                    (Ok(registered_pid), Ok(group)) if registered_pid > 0 && group > 0 => {
                        // SAFETY: getpgid only queries the OS.
                        (unsafe { libc::getpgid(registered_pid) }) == group
                    }
                    _ => false,
                }
            }
            #[cfg(not(unix))]
            {
                false
            }
        };
        if !owns_terminal || normalize_cwd(entry.get("cwd")?.as_str()?) != wanted_cwd {
            return None;
        }
        Some(entry.get("session_id")?.as_str()?.to_string())
    });
    let session = matches.next()?;
    // A nested Grok in the same process group is not an exact binding.
    matches.next().is_none().then_some(session)
}

/// Read the session id grok registered for `pid`, rejecting a stale entry whose
/// session directory is gone.
fn grok_session_for_pid(pid: u32, cwd: &str) -> Option<String> {
    let path = dirs::home_dir()?.join(".grok").join("active_sessions.json");
    let contents = std::fs::read_to_string(path).ok()?;
    let session_id = select_grok_session_for_pid(&contents, pid, cwd)?;
    verify_grok_session(&session_id, cwd).then_some(session_id)
}

/// The `chat_history.jsonl` of the grok session `pid` registered for `cwd`, for
/// the Chat view. Exact registry hits only: the newest-session fallback of
/// `discover_grok_session` is a guess, and a Chat that guesses shows another
/// tab's conversation.
pub(crate) fn grok_chat_history_for_pid(home: &Path, pid: u32, cwd: &str) -> Option<PathBuf> {
    let grok = home.join(".grok");
    let contents = std::fs::read_to_string(grok.join("active_sessions.json")).ok()?;
    let session_id = select_grok_session_for_pid(&contents, pid, cwd)?;
    if !is_uuid(&session_id) {
        return None;
    }
    let dir = grok
        .join("sessions")
        .join(grok_path_encode(cwd))
        .join(session_id);
    dir.is_dir().then(|| dir.join("chat_history.jsonl"))
}

/// grok stores sessions under `~/.grok/sessions/<percent-encoded-cwd>/<UUIDv7>/`.
/// Each session is a *directory* named with its UUIDv7 id (usable with
/// `grok --resume <id>`); the newest such directory is the active session.
///
/// As for Claude, "newest" is a guess whenever a folder holds more than one grok
/// tab, so the pid registry is consulted first and the mtime sort is the fallback.
fn discover_grok_session(
    cwd: &str,
    claimed_ids: &[String],
    agent_pid: Option<u32>,
) -> Option<String> {
    if let Some(pid) = agent_pid
        && let Some(id) = grok_session_for_pid(pid, cwd)
    {
        return Some(id);
    }

    let dir = grok_sessions_dir()?.join(grok_path_encode(cwd));
    // DEFERRED (2026-06-13) — extractor accepts any UUID-named entry, not only
    // directories. grok only ever creates session *directories*, and
    // verify_grok_session() rejects non-dirs before resume, so a phantom
    // UUID-named file would at worst yield a no-op resume. A real is_dir guard
    // needs the entry kind threaded through newest_unclaimed_file (8 call sites).
    newest_unclaimed_file(
        &dir,
        |name| is_uuid(name).then(|| name.to_string()),
        claimed_ids,
        Some(SESSION_MAX_AGE),
    )
}

/// Check if `~/.grok/sessions/<percent-encoded-cwd>/<session_id>/` exists.
fn verify_grok_session(session_id: &str, cwd: &str) -> bool {
    if !is_uuid(session_id) {
        return false;
    }
    let Some(dir) = grok_sessions_dir().map(|d| d.join(grok_path_encode(cwd))) else {
        return false;
    };
    dir.join(session_id).is_dir()
}

// ─── ego ──────────────────────────────────────────────────────────────────────

/// ego's session store: `$EGO_HOME/sessions`, default `~/.ego/sessions`. Each
/// session is a UUID-named directory.
fn ego_sessions_dir(ego_home: Option<&str>) -> Option<PathBuf> {
    match ego_home {
        Some(home) => Some(PathBuf::from(home).join("sessions")),
        None => dirs::home_dir().map(|h| h.join(".ego").join("sessions")),
    }
}

#[cfg(test)]
thread_local! {
    static EGO_INDEX_READS: std::cell::Cell<usize> = const { std::cell::Cell::new(0) };
    static EGO_SESSION_SCANS: std::cell::Cell<usize> = const { std::cell::Cell::new(0) };
    static EGO_LEASE_READS: std::cell::Cell<usize> = const { std::cell::Cell::new(0) };
}

#[derive(serde::Deserialize)]
struct EgoLease {
    pid: u32,
    heartbeat: u64,
}

fn read_ego_lease(path: &Path) -> Option<(EgoLease, bool)> {
    use std::io::Read;

    #[cfg(test)]
    EGO_LEASE_READS.with(|reads| reads.set(reads.get() + 1));
    let mut file = std::fs::File::open(path).ok()?;
    // ego's writer holds flock (Unix) / byte zero (Windows) until exit.
    // A leftover pid alone is not evidence that the cached holder still lives.
    let held = matches!(
        file.try_lock_shared(),
        Err(std::fs::TryLockError::WouldBlock)
    );
    #[cfg(windows)]
    {
        use std::io::{Seek, SeekFrom};
        // Ego reserves byte zero for LockFileEx so readers can read the JSON
        // starting at byte one while its exclusive writer lock is held.
        file.seek(SeekFrom::Start(1)).ok()?;
    }
    let mut contents = String::new();
    file.read_to_string(&mut contents).ok()?;
    Some((serde_json::from_str(&contents).ok()?, held))
}

type EgoBindings = HashMap<(PathBuf, u32), String>;
static EGO_BINDINGS: std::sync::LazyLock<parking_lot::Mutex<EgoBindings>> =
    std::sync::LazyLock::new(|| parking_lot::Mutex::new(HashMap::new()));

#[derive(serde::Deserialize)]
struct EgoPidIndex {
    pid: u32,
    session_id: String,
}

/// Resolve an indexed pid against its held lease, or scan for older ego versions.
/// An invalid index fails closed; only an absent index permits the legacy scan.
/// Only a held writer lease can validate a cached binding: ego leaves the file
/// after exit and writes its heartbeat only at acquisition, so matching stale
/// pid text would otherwise hide pid reuse or an in-process session switch.
fn ego_session_for_pid(sessions_dir: &Path, pid: u32) -> Option<String> {
    let key = (sessions_dir.to_path_buf(), pid);
    let index_path = sessions_dir
        .parent()?
        .join("pids")
        .join(format!("{pid}.json"));
    #[cfg(test)]
    EGO_INDEX_READS.with(|reads| reads.set(reads.get() + 1));
    match std::fs::read(index_path) {
        Ok(contents) => {
            // The index is a locator, never liveness evidence. Read it before
            // the cache so an active session switch cannot return the old id.
            EGO_BINDINGS.lock().remove(&key);
            let index: EgoPidIndex = serde_json::from_slice(&contents).ok()?;
            if index.pid != pid || !is_uuid(&index.session_id) {
                return None;
            }
            let (lease, held) =
                read_ego_lease(&sessions_dir.join(&index.session_id).join("lease"))?;
            if lease.pid != pid || !held {
                return None;
            }
            EGO_BINDINGS.lock().insert(key, index.session_id.clone());
            return Some(index.session_id);
        }
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
        Err(_) => {
            EGO_BINDINGS.lock().remove(&key);
            return None;
        }
    }
    let cached = EGO_BINDINGS.lock().get(&key).cloned();
    if let Some(session) = cached {
        if let Some((lease, true)) = read_ego_lease(&sessions_dir.join(&session).join("lease"))
            && lease.pid == pid
        {
            return Some(session);
        }
        EGO_BINDINGS.lock().remove(&key);
    }

    #[cfg(test)]
    EGO_SESSION_SCANS.with(|scans| scans.set(scans.get() + 1));
    let (_, session, held) = std::fs::read_dir(sessions_dir)
        .ok()?
        .flatten()
        .filter_map(|entry| {
            let name = entry.file_name().to_str()?.to_string();
            if !is_uuid(&name) {
                return None;
            }
            let (lease, held) = read_ego_lease(&entry.path().join("lease"))?;
            (lease.pid == pid).then_some((lease.heartbeat, name, held))
        })
        .max()?;
    if held {
        EGO_BINDINGS.lock().insert(key, session.clone());
    }
    Some(session)
}

/// Exact terminal Chat binding, with the same store the live process reads.
/// A failed environment read must not fall back to a different user's store.
pub(crate) fn ego_chat_session_for_pid(pid: u32) -> Option<(PathBuf, String)> {
    let env = crate::process_env::read_process_environment(pid).ok()?;
    let value = |name: &str| {
        env.iter().find_map(|entry| {
            let (key, value) = entry.split_once('=')?;
            (key == name).then(|| value.to_owned())
        })
    };
    let home = value("EGO_HOME")
        .map(PathBuf::from)
        .or_else(|| value("HOME").map(|home| PathBuf::from(home).join(".ego")))?;
    let session = ego_session_for_pid(&home.join("sessions"), pid)?;
    Some((home, session))
}

/// ego has no per-cwd layout and no fallback: without a pid there is no honest
/// binding, and `ego resume` with no id already continues the workspace's latest.
fn discover_ego_session(agent_pid: Option<u32>, ego_home: Option<&str>) -> Option<String> {
    ego_session_for_pid(&ego_sessions_dir(ego_home)?, agent_pid?)
}

/// The provider half of `<sessions>/<session_id>/manifest.json`'s `model`
/// (`provider/model`, split on the first `/` exactly as ego does).
fn ego_session_provider(sessions_dir: &Path, session_id: &str) -> Option<String> {
    let manifest =
        std::fs::read_to_string(sessions_dir.join(session_id).join("manifest.json")).ok()?;
    let manifest: serde_json::Value = serde_json::from_str(&manifest).ok()?;
    let (provider, model) = manifest.get("model")?.as_str()?.split_once('/')?;
    (!provider.is_empty() && !model.is_empty()).then(|| provider.to_string())
}

/// Check if `<sessions>/<session_id>/manifest.json` exists.
fn verify_ego_session(session_id: &str, ego_home: Option<&str>) -> bool {
    is_uuid(session_id)
        && ego_sessions_dir(ego_home)
            .is_some_and(|dir| dir.join(session_id).join("manifest.json").is_file())
}

// ─── Shared helpers ──────────────────────────────────────────────────────────

/// Return true if `s` matches the UUID format: 8-4-4-4-12 lowercase hex with dashes.
fn is_uuid(s: &str) -> bool {
    if s.len() != 36 {
        return false;
    }
    let bytes = s.as_bytes();
    // Positions 8, 13, 18, 23 must be '-'
    if bytes[8] != b'-' || bytes[13] != b'-' || bytes[18] != b'-' || bytes[23] != b'-' {
        return false;
    }
    bytes.iter().enumerate().all(|(i, &b)| {
        if i == 8 || i == 13 || i == 18 || i == 23 {
            b == b'-'
        } else {
            b.is_ascii_hexdigit()
        }
    })
}

/// Recency of a session entry. For a *file* (Claude/Gemini/Codex `.jsonl`),
/// its own mtime. For a session *directory* (grok stores each session as a dir),
/// the newest of the directory's own mtime and its immediate children's — a
/// directory's own mtime only tracks entry creation/removal, not writes into
/// existing files, so an actively-written grok session would otherwise be aged
/// out of discovery by the `max_age` cap once it's 5 min old.
fn entry_recency(path: &Path, meta: &std::fs::Metadata) -> SystemTime {
    let own = meta.modified().ok();
    if !meta.is_dir() {
        return own.unwrap_or(SystemTime::UNIX_EPOCH);
    }
    let newest_child = std::fs::read_dir(path)
        .into_iter()
        .flatten()
        .filter_map(|c| c.ok()?.metadata().ok()?.modified().ok())
        .max();
    own.into_iter()
        .chain(newest_child)
        .max()
        .unwrap_or(SystemTime::UNIX_EPOCH)
}

/// Scan `dir` for files matching `extract_id`, returning the newest unclaimed ID.
///
/// When `max_age` is set, files older than this duration are ignored. This
/// prevents discovering stale session files when an agent restarts in the same
/// terminal before the new session file is created.
fn newest_unclaimed_file<F>(
    dir: &PathBuf,
    extract_id: F,
    claimed_ids: &[String],
    max_age: Option<std::time::Duration>,
) -> Option<String>
where
    F: Fn(&str) -> Option<String>,
{
    if !dir.exists() {
        return None;
    }

    let now = SystemTime::now();

    let mut candidates: Vec<(SystemTime, String)> = std::fs::read_dir(dir)
        .ok()?
        .filter_map(|e| {
            let e = e.ok()?;
            let name = e.file_name().to_string_lossy().to_string();
            let id = extract_id(&name)?;
            let meta = e.metadata().ok()?;
            let mtime = entry_recency(&e.path(), &meta);
            if max_age.is_some_and(|max| now.duration_since(mtime).unwrap_or_default() > max) {
                return None;
            }
            Some((mtime, id))
        })
        .collect();

    candidates.sort_by_key(|a| std::cmp::Reverse(a.0));

    candidates
        .into_iter()
        .find(|(_, id)| !claimed_ids.contains(id))
        .map(|(_, id)| id)
}

// ─── Tests ───────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::time::{Duration, SystemTime};
    use tempfile::TempDir;

    fn make_file(dir: &std::path::Path, name: &str) -> PathBuf {
        let path = dir.join(name);
        fs::write(&path, b"{}").unwrap();
        path
    }

    /// Write a Codex rollout whose first line is the real `session_meta` shape,
    /// which is where discovery reads the recorded working directory from.
    fn make_codex_rollout(dir: &std::path::Path, name: &str, cwd: &str) -> PathBuf {
        let path = dir.join(name);
        let head = serde_json::json!({
            "timestamp": "2026-07-01T08:31:54.793Z",
            "type": "session_meta",
            "payload": { "id": "ignored", "cwd": cwd, "originator": "codex-tui" },
        });
        fs::write(&path, format!("{head}\n{{\"type\":\"event_msg\"}}\n")).unwrap();
        path
    }

    // ── is_uuid ──

    #[test]
    fn test_is_uuid_valid() {
        assert!(is_uuid("af467730-5e79-49d9-8a17-ebd94c99f262"));
        assert!(is_uuid("00000000-0000-0000-0000-000000000000"));
    }

    #[test]
    fn test_is_uuid_invalid() {
        assert!(!is_uuid("not-a-uuid"));
        assert!(!is_uuid("af467730-5e79-49d9-8a17")); // too short
        assert!(!is_uuid("af467730-5e79-49d9-8a17-ebd94c99f262X")); // too long
        assert!(!is_uuid("zf467730-5e79-49d9-8a17-ebd94c99f262")); // non-hex
    }

    // ── grok ──

    #[test]
    fn test_grok_path_encode_matches_on_disk_layout() {
        // Captured live: grok names its per-CWD session dir by percent-encoding
        // the absolute path — '/' → %2F, '.' preserved.
        assert_eq!(
            grok_path_encode("/Users/stefano.straus/Gits/personal/tuicommander"),
            "%2FUsers%2Fstefano.straus%2FGits%2Fpersonal%2Ftuicommander"
        );
        // Unreserved set is preserved; spaces and other reserved chars escape.
        assert_eq!(grok_path_encode("/a b/c-d_e~f"), "%2Fa%20b%2Fc-d_e~f");
    }

    #[test]
    fn test_grok_path_encode_windows_separators() {
        assert_eq!(grok_path_encode(r"C:\Users\foo"), "C%3A%2FUsers%2Ffoo");
    }

    #[test]
    fn test_verify_grok_session_rejects_non_uuid() {
        assert!(!verify_grok_session("not-a-uuid", "/tmp/x"));
    }

    /// Shape captured from a live `~/.grok/active_sessions.json`.
    fn grok_active_sessions(entries: &[(&str, u32, &str)]) -> String {
        let arr: Vec<_> = entries
            .iter()
            .map(|(id, pid, cwd)| {
                serde_json::json!({
                    "session_id": id,
                    "pid": pid,
                    "cwd": cwd,
                    "opened_at": "2026-09-05T16:18:29.539752Z",
                })
            })
            .collect();
        serde_json::Value::Array(arr).to_string()
    }

    /// Lay out an ego store the way ego writes it: `<uuid>/lease` + `manifest.json`.
    fn ego_store(leases: &[(&str, u32, u64)]) -> TempDir {
        let dir = TempDir::new().unwrap();
        for (id, pid, heartbeat) in leases {
            let session = dir.path().join(id);
            fs::create_dir(&session).unwrap();
            fs::write(session.join("manifest.json"), "{}").unwrap();
            fs::write(
                session.join("lease"),
                format!(r#" {{"pid":{pid},"heartbeat":{heartbeat}}}"#),
            )
            .unwrap();
        }
        dir
    }

    const EGO_A: &str = "01a02491-6a90-7ae2-9089-fd6a8532eb82";
    const EGO_B: &str = "01a02491-81eb-79e3-975e-af01974e86cb";

    /// Catches: two ego tabs in one workspace both resuming the newest session
    /// (what a "newest unclaimed" scan would do) instead of their own.
    #[test]
    fn test_ego_session_for_pid_picks_the_session_leased_to_that_pid() {
        let dir = ego_store(&[(EGO_A, 111, 5), (EGO_B, 222, 9)]);
        assert_eq!(ego_session_for_pid(dir.path(), 111).as_deref(), Some(EGO_A));
        assert_eq!(ego_session_for_pid(dir.path(), 222).as_deref(), Some(EGO_B));
        assert_eq!(ego_session_for_pid(dir.path(), 333), None);
    }

    // ego uses flock on Unix and LockFileEx on byte zero on Windows.
    // Match its writer lock, not a mock liveness flag.
    fn hold_ego_lease(path: &Path) -> fs::File {
        let file = fs::OpenOptions::new()
            .read(true)
            .write(true)
            .open(path)
            .unwrap();
        #[cfg(unix)]
        file.lock().unwrap();
        #[cfg(windows)]
        {
            use std::os::windows::io::AsRawHandle;
            use windows_sys::Win32::Storage::FileSystem::{
                LOCKFILE_EXCLUSIVE_LOCK, LOCKFILE_FAIL_IMMEDIATELY, LockFileEx,
            };
            let mut overlapped = unsafe { std::mem::zeroed() };
            assert_ne!(
                unsafe {
                    LockFileEx(
                        file.as_raw_handle(),
                        LOCKFILE_EXCLUSIVE_LOCK | LOCKFILE_FAIL_IMMEDIATELY,
                        0,
                        1,
                        0,
                        &mut overlapped,
                    )
                },
                0
            );
        }
        file
    }

    fn write_ego_pid_index(home: &Path, pid: u32, session: &str) {
        fs::create_dir_all(home.join("pids")).unwrap();
        fs::write(
            home.join("pids").join(format!("{pid}.json")),
            serde_json::json!({"pid": pid, "session_id": session, "cwd": home}).to_string(),
        )
        .unwrap();
    }

    /// Catches: cold discovery still enumerating all historical sessions.
    #[test]
    fn ego_pid_index_binds_without_listing_sessions() {
        let home = tempfile::tempdir_in(crate::test_support::test_temp_root()).unwrap();
        let sessions = home.path().join("sessions");
        fs::create_dir_all(sessions.join(EGO_A)).unwrap();
        fs::write(
            sessions.join(EGO_A).join("lease"),
            r#" {"pid":111,"heartbeat":5}"#,
        )
        .unwrap();
        let _held = hold_ego_lease(&sessions.join(EGO_A).join("lease"));
        write_ego_pid_index(home.path(), 111, EGO_A);
        EGO_SESSION_SCANS.with(|scans| scans.set(0));
        EGO_LEASE_READS.with(|reads| reads.set(0));
        assert_eq!(ego_session_for_pid(&sessions, 111).as_deref(), Some(EGO_A));
        assert_eq!(
            EGO_SESSION_SCANS.with(|scans| scans.get()),
            0,
            "indexed lookup listed sessions"
        );
        assert_eq!(EGO_LEASE_READS.with(|reads| reads.get()), 1);
    }

    /// Catches: a stale index binding a recycled pid through the legacy scan.
    #[test]
    fn stale_ego_pid_index_is_rejected() {
        let home = tempfile::tempdir_in(crate::test_support::test_temp_root()).unwrap();
        let sessions = home.path().join("sessions");
        for (id, pid) in [(EGO_A, 222), (EGO_B, 111)] {
            fs::create_dir_all(sessions.join(id)).unwrap();
            fs::write(
                sessions.join(id).join("lease"),
                format!(r#" {{"pid":{pid},"heartbeat":5}}"#),
            )
            .unwrap();
        }
        let _held = hold_ego_lease(&sessions.join(EGO_A).join("lease"));
        let _other = hold_ego_lease(&sessions.join(EGO_B).join("lease"));
        write_ego_pid_index(home.path(), 111, EGO_A);
        assert_eq!(ego_session_for_pid(&sessions, 111), None);
    }

    /// Catches: malformed indexes, unheld writers or replaced indexes reviving
    /// a cached session or silently selecting an unrelated legacy candidate.
    #[test]
    fn ego_pid_index_fails_closed_and_tracks_active_session_switches() {
        let home = tempfile::tempdir_in(crate::test_support::test_temp_root()).unwrap();
        let sessions = home.path().join("sessions");
        for id in [EGO_A, EGO_B] {
            fs::create_dir_all(sessions.join(id)).unwrap();
            fs::write(
                sessions.join(id).join("lease"),
                r#" {"pid":111,"heartbeat":5}"#,
            )
            .unwrap();
        }
        let held = hold_ego_lease(&sessions.join(EGO_A).join("lease"));
        let other = hold_ego_lease(&sessions.join(EGO_B).join("lease"));
        write_ego_pid_index(home.path(), 111, EGO_A);
        assert_eq!(ego_session_for_pid(&sessions, 111).as_deref(), Some(EGO_A));
        write_ego_pid_index(home.path(), 111, EGO_B);
        assert_eq!(ego_session_for_pid(&sessions, 111).as_deref(), Some(EGO_B));
        drop(other);
        assert_eq!(ego_session_for_pid(&sessions, 111), None);
        for contents in [
            "broken".to_string(),
            serde_json::json!({"pid": 222, "session_id": EGO_A}).to_string(),
            serde_json::json!({"pid": 111, "session_id": "../escape"}).to_string(),
        ] {
            fs::write(home.path().join("pids/111.json"), contents).unwrap();
            assert_eq!(ego_session_for_pid(&sessions, 111), None);
        }
        drop(held);
    }

    /// Catches: the index fast path diverging from real ego writer files.
    /// Captured from native ego 584; replay its held writer lock on this host.
    #[test]
    fn ego_pid_index_real_writer_opens_only_index_and_lease() {
        let home = tempfile::tempdir_in(crate::test_support::test_temp_root()).unwrap();
        let index_bytes = include_bytes!("fixtures/ego_pid_index/pid.json");
        let index: serde_json::Value = serde_json::from_slice(index_bytes).unwrap();
        let pid = index["pid"].as_u64().unwrap() as u32;
        let expected = index["session_id"].as_str().unwrap();
        let lease_path = home.path().join("sessions").join(expected).join("lease");
        fs::create_dir_all(lease_path.parent().unwrap()).unwrap();
        fs::create_dir_all(home.path().join("pids")).unwrap();
        fs::write(
            home.path().join("pids").join(format!("{pid}.json")),
            index_bytes,
        )
        .unwrap();
        fs::write(
            &lease_path,
            include_bytes!("fixtures/ego_pid_index/lease.json"),
        )
        .unwrap();
        let _held = hold_ego_lease(&lease_path);
        EGO_INDEX_READS.with(|reads| reads.set(0));
        EGO_LEASE_READS.with(|reads| reads.set(0));
        EGO_SESSION_SCANS.with(|scans| scans.set(0));
        assert_eq!(
            ego_session_for_pid(&home.path().join("sessions"), pid).as_deref(),
            Some(expected)
        );
        assert_eq!(EGO_INDEX_READS.with(|reads| reads.get()), 1);
        assert_eq!(EGO_LEASE_READS.with(|reads| reads.get()), 1);
        assert_eq!(EGO_SESSION_SCANS.with(|scans| scans.get()), 0);
    }

    /// Catches: a released old lease retaining a recycled pid after another
    /// already-existing session acquires it with a later heartbeat.
    #[test]
    fn ego_cached_pid_reuse_rescans_after_writer_releases_lease() {
        let dir = ego_store(&[(EGO_A, 111, 5), (EGO_B, 222, 9)]);
        let held = hold_ego_lease(&dir.path().join(EGO_A).join("lease"));
        assert_eq!(ego_session_for_pid(dir.path(), 111).as_deref(), Some(EGO_A));
        drop(held);
        fs::write(
            dir.path().join(EGO_B).join("lease"),
            r#" {"pid":111,"heartbeat":10}"#,
        )
        .unwrap();
        let _new_holder = hold_ego_lease(&dir.path().join(EGO_B).join("lease"));
        assert_eq!(ego_session_for_pid(dir.path(), 111).as_deref(), Some(EGO_B));
    }

    /// Catches: a cached binding surviving a missing, corrupt or changed-pid lease.
    #[test]
    fn ego_cached_binding_revalidates_lease_before_returning() {
        for replacement in [None, Some("broken"), Some(r#" {"pid":333,"heartbeat":12}"#)] {
            let dir = ego_store(&[(EGO_A, 111, 5), (EGO_B, 111, 4)]);
            let held = hold_ego_lease(&dir.path().join(EGO_A).join("lease"));
            assert_eq!(ego_session_for_pid(dir.path(), 111).as_deref(), Some(EGO_A));
            drop(held);
            let path = dir.path().join(EGO_A).join("lease");
            match replacement {
                Some(contents) => fs::write(path, contents).unwrap(),
                None => fs::remove_file(path).unwrap(),
            }
            assert_eq!(
                ego_session_for_pid(dir.path(), 111).as_deref(),
                Some(EGO_B),
                "replacement: {replacement:?}"
            );
        }
    }

    /// Catches: identical pids under different EGO_HOME roots sharing one binding.
    #[test]
    fn ego_cached_binding_is_scoped_to_session_store() {
        let first = ego_store(&[(EGO_A, 111, 5)]);
        let second = ego_store(&[(EGO_B, 111, 9)]);
        let _first_holder = hold_ego_lease(&first.path().join(EGO_A).join("lease"));
        let _second_holder = hold_ego_lease(&second.path().join(EGO_B).join("lease"));
        assert_eq!(
            ego_session_for_pid(first.path(), 111).as_deref(),
            Some(EGO_A)
        );
        assert_eq!(
            ego_session_for_pid(second.path(), 111).as_deref(),
            Some(EGO_B)
        );
        assert_eq!(
            ego_session_for_pid(first.path(), 111).as_deref(),
            Some(EGO_A)
        );
    }

    /// Catches: every 30-second poll reopening all historical lease files.
    #[test]
    fn ego_cached_poll_reads_one_lease_among_1500_sessions() {
        let dir = tempfile::tempdir_in(crate::test_support::test_temp_root()).unwrap();
        for index in 0..1500_u32 {
            let session = dir
                .path()
                .join(format!("00000000-0000-0000-0000-{index:012x}"));
            fs::create_dir(&session).unwrap();
            fs::write(
                session.join("lease"),
                format!(r#" {{"pid":{},"heartbeat":9}}"#, index + 1),
            )
            .unwrap();
        }
        let expected = "00000000-0000-0000-0000-000000000000";
        let _held = hold_ego_lease(&dir.path().join(expected).join("lease"));
        assert_eq!(
            ego_session_for_pid(dir.path(), 1).as_deref(),
            Some(expected)
        );
        EGO_LEASE_READS.with(|reads| reads.set(0));
        assert_eq!(
            ego_session_for_pid(dir.path(), 1).as_deref(),
            Some(expected)
        );
        let reads = EGO_LEASE_READS.with(|reads| reads.get());
        assert!(reads <= 1, "cached poll read {reads} leases instead of one");
    }

    /// Catches: Tauri dispatching filesystem discovery inline on the UI thread.
    #[test]
    fn discovery_command_offloads_filesystem_work() {
        let source = include_str!("agent_session.rs");
        let command = source
            .split("#[cfg_attr(feature = \"desktop\", tauri::command)]")
            .nth(1)
            .unwrap();
        let command = command.split("/// What discovery found").next().unwrap();
        assert!(
            command.contains("async fn discover_agent_session("),
            "discovery command is synchronous"
        );
        assert!(
            command.contains("spawn_blocking"),
            "discovery parks an async worker instead of offloading filesystem work"
        );
    }

    /// Catches: a recycled pid binding a tab to a dead process's older session.
    #[test]
    fn test_ego_session_for_pid_prefers_the_latest_heartbeat() {
        let dir = ego_store(&[(EGO_A, 111, 9), (EGO_B, 111, 5)]);
        assert_eq!(ego_session_for_pid(dir.path(), 111).as_deref(), Some(EGO_A));
    }

    /// Catches: a corrupt lease or a stray non-session entry aborting discovery.
    #[test]
    fn test_ego_session_for_pid_skips_corrupt_leases_and_non_session_entries() {
        let dir = ego_store(&[(EGO_B, 111, 5)]);
        let broken = dir.path().join(EGO_A);
        fs::create_dir(&broken).unwrap();
        fs::write(broken.join("lease"), "not json").unwrap();
        fs::create_dir(dir.path().join("not-a-uuid")).unwrap();
        fs::write(
            dir.path().join("not-a-uuid").join("lease"),
            r#"{"pid":111,"heartbeat":99}"#,
        )
        .unwrap();
        assert_eq!(ego_session_for_pid(dir.path(), 111).as_deref(), Some(EGO_B));
    }

    /// Catches: discovery guessing a session when the agent pid is unknown.
    #[test]
    fn test_discover_ego_session_resolves_under_ego_home_and_needs_a_pid() {
        let home = TempDir::new().unwrap();
        let sessions = ego_store(&[(EGO_A, 111, 5)]);
        fs::rename(sessions.path().join(EGO_A), {
            fs::create_dir(home.path().join("sessions")).unwrap();
            home.path().join("sessions").join(EGO_A)
        })
        .unwrap();
        let home = home.path().to_str().unwrap();
        assert_eq!(
            discover_ego_session(Some(111), Some(home)).as_deref(),
            Some(EGO_A)
        );
        assert_eq!(discover_ego_session(None, Some(home)), None);
    }

    /// Catches: resume of an id whose session directory is gone, and a path
    /// traversal through a non-UUID id.
    #[test]
    fn test_verify_ego_session_requires_an_existing_uuid_session() {
        let home = TempDir::new().unwrap();
        let sessions = home.path().join("sessions");
        fs::create_dir(&sessions).unwrap();
        let session = sessions.join(EGO_A);
        fs::create_dir(&session).unwrap();
        fs::write(session.join("manifest.json"), "{}").unwrap();
        let home = home.path().to_str().unwrap();
        assert!(verify_ego_session(EGO_A, Some(home)));
        assert!(!verify_ego_session(EGO_B, Some(home)));
        assert!(!verify_ego_session("../sessions", Some(home)));
    }

    /// Catches: the sidebar icon reading the provider from the model name or
    /// the user config instead of the session the pid actually holds.
    #[test]
    fn test_ego_session_provider_is_the_first_segment_of_the_manifest_model() {
        let dir = ego_store(&[(EGO_A, 111, 5), (EGO_B, 222, 9)]);
        let write = |id: &str, body: &str| {
            fs::write(dir.path().join(id).join("manifest.json"), body).unwrap()
        };
        write(EGO_A, r#"{"model":"openai-codex/gpt-6-sol"}"#);
        write(EGO_B, r#"{"model":"openrouter/anthropic/claude-x"}"#);
        let h = dir.path();
        assert_eq!(
            ego_session_provider(h, EGO_A).as_deref(),
            Some("openai-codex")
        );
        assert_eq!(
            ego_session_provider(h, EGO_B).as_deref(),
            Some("openrouter")
        );
        write(EGO_A, r#"{"model":"no-separator"}"#);
        assert_eq!(ego_session_provider(h, EGO_A), None);
        write(EGO_A, "{}");
        assert_eq!(ego_session_provider(h, EGO_A), None);
    }

    /// Catches: ego missing from the env table, so a session under a custom
    /// `EGO_HOME` is looked up in `~/.ego` and never found.
    #[test]
    fn test_ego_home_is_a_session_env_var() {
        assert!(
            AGENT_ENV_VARS
                .iter()
                .any(|(t, vars)| *t == "ego" && vars.contains(&"EGO_HOME"))
        );
    }

    /// A grok home with two sessions in one folder, registered by two pids.
    fn grok_home_with_two_tabs(cwd: &str, mine: &str, other: &str) -> tempfile::TempDir {
        let home = tempfile::tempdir_in(crate::test_support::test_temp_root()).unwrap();
        let grok = home.path().join(".grok");
        for id in [mine, other] {
            let dir = grok.join("sessions").join(grok_path_encode(cwd)).join(id);
            std::fs::create_dir_all(&dir).unwrap();
            std::fs::write(dir.join("chat_history.jsonl"), id).unwrap();
        }
        std::fs::write(
            grok.join("active_sessions.json"),
            grok_active_sessions(&[(other, 111, cwd), (mine, 222, cwd)]),
        )
        .unwrap();
        home
    }

    // Catches: npm's Node launcher owns the terminal process group, but Grok
    // registers its native child PID, leaving Chat unable to read its transcript.
    #[cfg(unix)]
    #[test]
    fn test_grok_chat_history_binds_registered_native_child_of_npm_launcher() {
        use std::io::{BufRead, BufReader};
        use std::os::unix::process::CommandExt;
        use std::process::{Command, Stdio};

        struct Launcher(std::process::Child);
        impl Drop for Launcher {
            fn drop(&mut self) {
                // Only this test's newly created process group is signalled.
                unsafe { libc::kill(-(self.0.id() as i32), libc::SIGKILL) };
                let _ = self.0.wait();
            }
        }
        let mut launcher = Launcher(
            Command::new("sh")
                .args(["-c", "sleep 600 & first=$!; sleep 600 & second=$!; printf '%s %s\\n' \"$first\" \"$second\"; wait"])
                .process_group(0)
                .stdout(Stdio::piped())
                .spawn()
                .unwrap(),
        );
        let mut line = String::new();
        BufReader::new(launcher.0.stdout.take().unwrap())
            .read_line(&mut line)
            .unwrap();
        let pids: Vec<u32> = line
            .split_whitespace()
            .map(|s| s.parse().unwrap())
            .collect();
        let home = tempfile::tempdir_in(crate::test_support::test_temp_root()).unwrap();
        let cwd = home.path().to_str().unwrap();
        let mut registry: serde_json::Value = serde_json::from_str(include_str!(
            "fixtures/chat_view/grok_npm_active_sessions.json"
        ))
        .unwrap();
        registry[0]["pid"] = pids[0].into();
        registry[0]["cwd"] = cwd.into();
        let id = registry[0]["session_id"].as_str().unwrap().to_owned();
        let transcript = home
            .path()
            .join(".grok/sessions")
            .join(grok_path_encode(cwd))
            .join(id)
            .join("chat_history.jsonl");
        std::fs::create_dir_all(transcript.parent().unwrap()).unwrap();
        std::fs::write(&transcript, "registered native Grok transcript").unwrap();
        let registry_path = home.path().join(".grok/active_sessions.json");
        std::fs::write(&registry_path, registry.to_string()).unwrap();
        let bound = grok_chat_history_for_pid(home.path(), launcher.0.id(), cwd);
        assert_eq!(
            bound
                .as_ref()
                .and_then(|p| std::fs::read_to_string(p).ok())
                .as_deref(),
            Some("registered native Grok transcript")
        );
        assert_eq!(
            grok_chat_history_for_pid(home.path(), std::process::id(), cwd),
            None,
            "another process group must not claim this session"
        );
        assert_eq!(
            grok_chat_history_for_pid(home.path(), launcher.0.id(), "/other/project"),
            None
        );
        let mut second = registry[0].clone();
        second["pid"] = pids[1].into();
        registry.as_array_mut().unwrap().push(second);
        std::fs::write(&registry_path, registry.to_string()).unwrap();
        assert_eq!(
            grok_chat_history_for_pid(home.path(), launcher.0.id(), cwd),
            None,
            "multiple native registrations in one group are ambiguous"
        );
    }

    // Catches: two grok tabs in one folder showing each other's conversation, or a
    // tab the registry does not know being given the newest session in the folder.
    #[test]
    fn test_grok_chat_history_binds_each_tab_to_its_own_session_only() {
        let (mine, other) = (
            "01a0725d-38ec-73b1-9c79-fab368c84702",
            "01a0725d-38ec-73b1-9c79-fab368c84703",
        );
        let home = grok_home_with_two_tabs("/fake/project", mine, other);
        let read = |pid| {
            grok_chat_history_for_pid(home.path(), pid, "/fake/project")
                .map(|p| std::fs::read_to_string(p).unwrap())
        };
        assert_eq!(read(222).as_deref(), Some(mine));
        assert_eq!(read(111).as_deref(), Some(other));
        assert_eq!(
            read(999),
            None,
            "an unregistered pid must not fall back to the newest"
        );
    }

    // Catches: a recycled pid naming a session from another project, or a registry
    // entry whose session directory is gone.
    #[test]
    fn test_grok_chat_history_rejects_other_cwd_and_missing_session() {
        let (mine, other) = (
            "01a0725d-38ec-73b1-9c79-fab368c84702",
            "01a0725d-38ec-73b1-9c79-fab368c84703",
        );
        let home = grok_home_with_two_tabs("/fake/project", mine, other);
        assert_eq!(
            grok_chat_history_for_pid(home.path(), 222, "/fake/else"),
            None
        );
        std::fs::remove_dir_all(
            home.path()
                .join(".grok/sessions")
                .join(grok_path_encode("/fake/project"))
                .join(mine),
        )
        .unwrap();
        assert_eq!(
            grok_chat_history_for_pid(home.path(), 222, "/fake/project"),
            None
        );
    }

    #[test]
    fn test_select_grok_session_for_pid_picks_the_matching_process() {
        let mine = "01a0725d-38ec-73b1-9c79-fab368c84702";
        let other = "01a0725d-38ec-73b1-9c79-fab368c84703";
        let json =
            grok_active_sessions(&[(other, 111, "/fake/project"), (mine, 222, "/fake/project")]);

        assert_eq!(
            select_grok_session_for_pid(&json, 222, "/fake/project"),
            Some(mine.to_string()),
            "two grok tabs in one folder must be told apart by pid, not by mtime"
        );
    }

    /// grok leaves entries behind when it exits, so a recycled pid can name a
    /// session from another project.
    #[test]
    fn test_select_grok_session_for_pid_rejects_another_cwd() {
        let stale = "01a0725d-38ec-73b1-9c79-fab368c84702";
        let json = grok_active_sessions(&[(stale, 222, "/other/project")]);

        assert_eq!(
            select_grok_session_for_pid(&json, 222, "/fake/project"),
            None
        );
    }

    #[test]
    fn test_select_grok_session_for_pid_unknown_pid_returns_none() {
        let json =
            grok_active_sessions(&[("01a0725d-38ec-73b1-9c79-fab368c84702", 222, "/fake/project")]);

        assert_eq!(
            select_grok_session_for_pid(&json, 999, "/fake/project"),
            None
        );
    }

    #[test]
    fn test_select_grok_session_for_pid_tolerates_a_corrupt_file() {
        assert_eq!(
            select_grok_session_for_pid("not json", 222, "/fake/project"),
            None
        );
        assert_eq!(
            select_grok_session_for_pid("{}", 222, "/fake/project"),
            None
        );
        assert_eq!(
            select_grok_session_for_pid(r#"[{"pid":222}]"#, 222, "/fake/project"),
            None,
            "an entry with no cwd/session_id must be skipped, not panic"
        );
    }

    // ── path_to_claude_slug ──

    #[test]
    fn test_path_to_claude_slug_unix() {
        assert_eq!(path_to_claude_slug("/Users/foo/bar"), "-Users-foo-bar");
    }

    #[test]
    fn test_path_to_claude_slug_dots_in_username() {
        assert_eq!(
            path_to_claude_slug("/Users/stefano.straus/Gits/project"),
            "-Users-stefano-straus-Gits-project"
        );
    }

    #[test]
    fn test_path_to_claude_slug_underscores() {
        assert_eq!(
            path_to_claude_slug("/Users/foo/CC_Playground/my_project"),
            "-Users-foo-CC-Playground-my-project"
        );
    }

    #[test]
    fn test_path_to_claude_slug_hidden_dirs() {
        assert_eq!(
            path_to_claude_slug("/Users/foo/project/.claude-worktrees/feat"),
            "-Users-foo-project--claude-worktrees-feat"
        );
    }

    #[test]
    fn test_path_to_claude_slug_trailing_slash() {
        assert_eq!(path_to_claude_slug("/Users/foo/bar/"), "-Users-foo-bar");
    }

    /// Catches: a Windows drive colon leaking into a project directory slug.
    #[test]
    fn test_path_to_claude_slug_windows() {
        assert_eq!(
            path_to_claude_slug("C:\\Users\\foo\\bar"),
            "C--Users-foo-bar"
        );
    }

    // ── claude_project_dir ──

    #[test]
    fn test_claude_project_dir_returns_path_with_slug() {
        if let Ok(result) = claude_project_dir("/Users/foo/bar".to_string(), None) {
            assert!(
                crate::test_support::slashed(&result).ends_with("/.claude/projects/-Users-foo-bar"),
                "unexpected path: {result}"
            );
        }
        // None only if home dir unavailable (CI) — not a failure
    }

    #[test]
    fn test_claude_project_dir_dots_in_username() {
        if let Ok(result) = claude_project_dir("/Users/foo.bar/proj".to_string(), None) {
            assert!(
                result.ends_with("-Users-foo-bar-proj"),
                "unexpected path: {result}"
            );
        }
    }

    #[test]
    fn test_claude_project_dir_with_config_dir_override() {
        let dir = TempDir::new().unwrap();
        let override_path = dir.path().to_str().unwrap().to_string();
        let result = claude_project_dir("/Users/foo/bar".to_string(), Some(override_path.clone()));
        assert!(result.is_ok());
        let path = result.unwrap();
        assert!(
            path.starts_with(&override_path),
            "expected path to start with override dir, got: {path}"
        );
        assert!(
            path.ends_with("-Users-foo-bar"),
            "expected slug suffix, got: {path}"
        );
    }

    /// Catches: Windows profile spaces surviving the slug and hiding Claude transcripts.
    #[test]
    fn claude_project_dir_windows_profile_spaces_match_claude_transcript_directory() {
        let dir = tempfile::tempdir_in(crate::test_support::test_temp_root()).unwrap();
        let path = claude_project_dir(
            r"C:\Users\Boss Name\work".into(),
            Some(dir.path().to_string_lossy().into_owned()),
        )
        .unwrap();
        // Recorded from Claude 2.1.286's project-path encoder, independently of TUIC.
        assert_eq!(
            PathBuf::from(path),
            dir.path().join("projects/C--Users-Boss-Name-work")
        );
    }

    #[test]
    fn test_discover_claude_session_with_config_dir() {
        let dir = TempDir::new().unwrap();
        let projects_dir = dir.path().join("projects");
        let slug = path_to_claude_slug("/fake/project");
        let session_dir = projects_dir.join(&slug);
        fs::create_dir_all(&session_dir).unwrap();

        let uuid = "af467730-5e79-49d9-8a17-ebd94c99f262";
        make_file(&session_dir, &format!("{uuid}.jsonl"));

        let result = discover_claude_session(
            "/fake/project",
            &[],
            Some(dir.path().to_str().unwrap()),
            None,
        );
        assert_eq!(result, Some(uuid.to_string()));
    }

    // ── claude pid→session registry ──

    /// Build a config dir containing a transcript for `uuid` and, when `registry` is
    /// set, the `sessions/<pid>.json` entry Claude writes while it runs.
    fn claude_config_dir(
        cwd: &str,
        transcripts: &[&str],
        registry: Option<(u32, &str, &str)>,
    ) -> TempDir {
        let dir = TempDir::new().unwrap();
        let project_dir = dir.path().join("projects").join(path_to_claude_slug(cwd));
        fs::create_dir_all(&project_dir).unwrap();
        for uuid in transcripts {
            make_file(&project_dir, &format!("{uuid}.jsonl"));
            // Distinct mtimes so "newest" is deterministic: last listed wins.
            std::thread::sleep(Duration::from_millis(10));
        }
        if let Some((pid, session_id, recorded_cwd)) = registry {
            let sessions = dir.path().join("sessions");
            fs::create_dir_all(&sessions).unwrap();
            let entry = serde_json::json!({
                "pid": pid,
                "sessionId": session_id,
                "cwd": recorded_cwd,
                "version": "2.1.261",
                "status": "busy",
            });
            fs::write(sessions.join(format!("{pid}.json")), entry.to_string()).unwrap();
        }
        dir
    }

    /// The whole point of issue #119: with several Claude tabs in one folder the
    /// newest transcript belongs to *some* tab, not to this one. The pid registry
    /// is the only thing that can tell them apart, so it must beat the mtime sort.
    #[test]
    fn test_discover_claude_session_prefers_the_pid_registry_over_the_newest_file() {
        let mine = "aaaaaaaa-aaaa-aaaa-aaaa-aaaaaaaaaaaa";
        let other_tab = "bbbbbbbb-bbbb-bbbb-bbbb-bbbbbbbbbbbb";
        // `other_tab` is written last, so it wins the mtime sort.
        let dir = claude_config_dir(
            "/fake/project",
            &[mine, other_tab],
            Some((4242, mine, "/fake/project")),
        );
        let config = Some(dir.path().to_str().unwrap());

        assert_eq!(
            discover_claude_session("/fake/project", &[], config, Some(4242)),
            Some(mine.to_string()),
            "the pid's own session must win over the newest transcript"
        );
        assert_eq!(
            discover_claude_session("/fake/project", &[], config, None),
            Some(other_tab.to_string()),
            "without a pid there is nothing better than the mtime heuristic"
        );
    }

    /// A pid hit is ground truth. If another tab grabbed this id by mtime first, that
    /// tab is the one that is wrong — honouring its claim here would keep this tab
    /// permanently bound to someone else's conversation.
    #[test]
    fn test_discover_claude_session_pid_hit_ignores_a_wrong_claim() {
        let mine = "cccccccc-cccc-cccc-cccc-cccccccccccc";
        let dir = claude_config_dir(
            "/fake/project",
            &[mine],
            Some((4242, mine, "/fake/project")),
        );

        assert_eq!(
            discover_claude_session(
                "/fake/project",
                &[mine.to_string()],
                Some(dir.path().to_str().unwrap()),
                Some(4242),
            ),
            Some(mine.to_string())
        );
    }

    /// Registry files outlive their process, so a recycled pid can point at a session
    /// from a different project. The recorded cwd rejects it.
    #[test]
    fn test_discover_claude_session_rejects_a_registry_entry_from_another_cwd() {
        let stale = "dddddddd-dddd-dddd-dddd-dddddddddddd";
        let dir = claude_config_dir("/fake/project", &[], Some((4242, stale, "/other/project")));

        assert_eq!(
            discover_claude_session(
                "/fake/project",
                &[],
                Some(dir.path().to_str().unwrap()),
                Some(4242),
            ),
            None
        );
    }

    /// The registry names a transcript. If that transcript is gone the entry is stale,
    /// and resuming it would fail — fall through to the heuristic instead.
    #[test]
    fn test_discover_claude_session_rejects_a_registry_entry_with_no_transcript() {
        let ghost = "eeeeeeee-eeee-eeee-eeee-eeeeeeeeeeee";
        let real = "ffffffff-ffff-ffff-ffff-ffffffffffff";
        let dir = claude_config_dir(
            "/fake/project",
            &[real],
            Some((4242, ghost, "/fake/project")),
        );

        assert_eq!(
            discover_claude_session(
                "/fake/project",
                &[],
                Some(dir.path().to_str().unwrap()),
                Some(4242),
            ),
            Some(real.to_string()),
            "a registry entry naming a missing transcript must not be used"
        );
    }

    /// A pid the registry does not know (older Claude, or the process exited) must
    /// leave the existing behaviour untouched rather than returning nothing.
    #[test]
    fn test_discover_claude_session_falls_back_when_the_pid_is_unregistered() {
        let uuid = "12345678-1234-1234-1234-123456789abc";
        let dir = claude_config_dir("/fake/project", &[uuid], None);

        assert_eq!(
            discover_claude_session(
                "/fake/project",
                &[],
                Some(dir.path().to_str().unwrap()),
                Some(4242),
            ),
            Some(uuid.to_string())
        );
    }

    /// Guards against a registry file whose `pid` field disagrees with its filename.
    #[test]
    fn test_claude_session_for_pid_rejects_a_mismatched_pid_field() {
        let uuid = "abcdef01-2345-6789-abcd-ef0123456789";
        let dir = claude_config_dir(
            "/fake/project",
            &[uuid],
            Some((4242, uuid, "/fake/project")),
        );
        // Rename 4242.json to 9999.json: same content, wrong key.
        let sessions = dir.path().join("sessions");
        fs::rename(sessions.join("4242.json"), sessions.join("9999.json")).unwrap();

        assert_eq!(
            claude_session_for_pid(9999, "/fake/project", Some(dir.path().to_str().unwrap())),
            None
        );
    }

    #[test]
    fn test_verify_claude_session_with_config_dir() {
        let dir = TempDir::new().unwrap();
        let projects_dir = dir.path().join("projects");
        let slug = path_to_claude_slug("/fake/project");
        let session_dir = projects_dir.join(&slug);
        fs::create_dir_all(&session_dir).unwrap();

        let uuid = "af467730-5e79-49d9-8a17-ebd94c99f262";
        make_file(&session_dir, &format!("{uuid}.jsonl"));

        assert!(verify_claude_session(
            uuid,
            "/fake/project",
            Some(dir.path().to_str().unwrap())
        ));
        assert!(!verify_claude_session(uuid, "/fake/project", None));
    }

    #[test]
    fn test_newest_unclaimed_file_max_age_filter() {
        let dir = TempDir::new().unwrap();
        let uuid = "af467730-5e79-49d9-8a17-ebd94c99f262";
        let path = make_file(dir.path(), &format!("{uuid}.jsonl"));

        // Backdate the file mtime by 2 minutes
        let two_min_ago = std::time::SystemTime::now() - Duration::from_secs(120);
        let file = fs::File::options().write(true).open(&path).unwrap();
        file.set_times(fs::FileTimes::new().set_modified(two_min_ago))
            .unwrap();

        // With 60s max age, the stale file should be filtered out
        let result = newest_unclaimed_file(
            &dir.path().to_path_buf(),
            |name| {
                name.strip_suffix(".jsonl")
                    .filter(|s| is_uuid(s))
                    .map(|s| s.to_string())
            },
            &[],
            Some(Duration::from_secs(60)),
        );
        assert!(result.is_none(), "stale file should be filtered by max_age");

        // Without max age, should still find it
        let result = newest_unclaimed_file(
            &dir.path().to_path_buf(),
            |name| {
                name.strip_suffix(".jsonl")
                    .filter(|s| is_uuid(s))
                    .map(|s| s.to_string())
            },
            &[],
            None,
        );
        assert_eq!(result, Some(uuid.to_string()));
    }

    // ── newest_unclaimed_file ──

    #[test]
    fn test_empty_dir_returns_none() {
        let dir = TempDir::new().unwrap();
        let result = newest_unclaimed_file(
            &dir.path().to_path_buf(),
            |name| {
                name.strip_suffix(".jsonl")
                    .filter(|s| is_uuid(s))
                    .map(|s| s.to_string())
            },
            &[],
            None,
        );
        assert!(result.is_none());
    }

    #[test]
    fn test_missing_dir_returns_none() {
        let result =
            newest_unclaimed_file(&PathBuf::from("/nonexistent/path/xyz"), |_| None, &[], None);
        assert!(result.is_none());
    }

    #[test]
    fn test_single_jsonl_returns_uuid() {
        let dir = TempDir::new().unwrap();
        let uuid = "af467730-5e79-49d9-8a17-ebd94c99f262";
        make_file(dir.path(), &format!("{uuid}.jsonl"));

        let result = newest_unclaimed_file(
            &dir.path().to_path_buf(),
            |name| {
                name.strip_suffix(".jsonl")
                    .filter(|s| is_uuid(s))
                    .map(|s| s.to_string())
            },
            &[],
            None,
        );
        assert_eq!(result, Some(uuid.to_string()));
    }

    #[test]
    fn test_claimed_id_is_excluded() {
        let dir = TempDir::new().unwrap();
        let uuid = "af467730-5e79-49d9-8a17-ebd94c99f262";
        make_file(dir.path(), &format!("{uuid}.jsonl"));

        let result = newest_unclaimed_file(
            &dir.path().to_path_buf(),
            |name| {
                name.strip_suffix(".jsonl")
                    .filter(|s| is_uuid(s))
                    .map(|s| s.to_string())
            },
            &[uuid.to_string()],
            None,
        );
        assert!(result.is_none());
    }

    #[test]
    fn test_newest_file_returned_when_multiple() {
        let dir = TempDir::new().unwrap();
        let uuid1 = "aaaaaaaa-aaaa-aaaa-aaaa-aaaaaaaaaaaa";
        let uuid2 = "bbbbbbbb-bbbb-bbbb-bbbb-bbbbbbbbbbbb";

        make_file(dir.path(), &format!("{uuid1}.jsonl"));
        // Sleep briefly so mtime differs; on fast filesystems use touch
        std::thread::sleep(Duration::from_millis(10));
        make_file(dir.path(), &format!("{uuid2}.jsonl"));

        let result = newest_unclaimed_file(
            &dir.path().to_path_buf(),
            |name| {
                name.strip_suffix(".jsonl")
                    .filter(|s| is_uuid(s))
                    .map(|s| s.to_string())
            },
            &[],
            None,
        );
        assert_eq!(result, Some(uuid2.to_string()));
    }

    #[test]
    fn test_non_uuid_filenames_skipped() {
        let dir = TempDir::new().unwrap();
        make_file(dir.path(), "not-a-uuid.jsonl");
        make_file(dir.path(), "some-other-file.txt");

        let result = newest_unclaimed_file(
            &dir.path().to_path_buf(),
            |name| {
                name.strip_suffix(".jsonl")
                    .filter(|s| is_uuid(s))
                    .map(|s| s.to_string())
            },
            &[],
            None,
        );
        assert!(result.is_none());
    }

    // ── gemini discovery mtime bound ──

    #[test]
    fn test_discover_gemini_session_filters_stale_by_mtime() {
        let dir = TempDir::new().unwrap();
        let chats = dir
            .path()
            .join(".gemini")
            .join("tmp")
            .join("projhash")
            .join("chats");
        fs::create_dir_all(&chats).unwrap();
        let cli_home = dir.path().to_str().unwrap();

        let stale_uuid = "aaaaaaaa-aaaa-aaaa-aaaa-aaaaaaaaaaaa";
        let stale = chats.join("session-stale.json");
        fs::write(&stale, format!(r#"{{"sessionId":"{stale_uuid}"}}"#)).unwrap();
        // Backdate well beyond the 300s bound.
        let old = SystemTime::now() - Duration::from_secs(10 * 60);
        fs::File::options()
            .write(true)
            .open(&stale)
            .unwrap()
            .set_times(fs::FileTimes::new().set_modified(old))
            .unwrap();

        // Only a stale file present → filtered out, nothing discovered.
        assert!(
            discover_gemini_session("/whatever", &[], Some(cli_home)).is_none(),
            "stale gemini session must be filtered by mtime"
        );

        // Add a fresh file → it (and only it) is discovered.
        let fresh_uuid = "bbbbbbbb-bbbb-bbbb-bbbb-bbbbbbbbbbbb";
        fs::write(
            chats.join("session-fresh.json"),
            format!(r#"{{"sessionId":"{fresh_uuid}"}}"#),
        )
        .unwrap();
        assert_eq!(
            discover_gemini_session("/whatever", &[], Some(cli_home)),
            Some(fresh_uuid.to_string()),
            "fresh gemini session should be discovered, stale ignored"
        );
    }

    // ── codex date-subtree pruning ──

    fn codex_day_dir(root: &std::path::Path, y: i32, m: u32, d: u32) -> PathBuf {
        root.join(format!("{y:04}"))
            .join(format!("{m:02}"))
            .join(format!("{d:02}"))
    }

    #[test]
    fn test_codex_recent_day_dirs_prunes_old_subtrees() {
        use chrono::Datelike;
        let root = TempDir::new().unwrap();
        let now = chrono::Local::now();
        let today = now.date_naive();
        let yesterday = today.pred_opt().unwrap();

        for d in [
            today,
            yesterday,
            chrono::NaiveDate::from_ymd_opt(2020, 1, 1).unwrap(),
        ] {
            fs::create_dir_all(codex_day_dir(root.path(), d.year(), d.month(), d.day())).unwrap();
        }

        let dirs = codex_recent_day_dirs(root.path(), now);
        assert!(dirs.contains(&codex_day_dir(
            root.path(),
            today.year(),
            today.month(),
            today.day()
        )));
        assert!(dirs.contains(&codex_day_dir(
            root.path(),
            yesterday.year(),
            yesterday.month(),
            yesterday.day()
        )));
        assert!(
            !dirs.contains(&codex_day_dir(root.path(), 2020, 1, 1)),
            "stale date subtree must be pruned before walking"
        );
    }

    // Catches re-statting every historical rollout on each idle/busy discovery.
    #[test]
    fn codex_warm_discovery_does_not_restat_historical_rollouts() {
        // DEFERRED (2026-10-25) — Windows CI restats 130 rollouts; record watcher availability/events before attributing the two rescans or weakening the zero-stat contract.
        let root = TempDir::new().unwrap();
        let sessions = root.path().join("sessions");
        let day = codex_recent_day_dirs_for_test(&sessions);
        fs::create_dir_all(&day).unwrap();
        let cwd = root.path().to_str().unwrap();
        let current = "bbbbbbbb-bbbb-bbbb-bbbb-bbbbbbbbbbbb";
        for n in 0..64 {
            let id = format!("aaaaaaaa-aaaa-aaaa-aaaa-{n:012x}");
            let path = make_codex_rollout(&day, &format!("rollout-test-{id}.jsonl"), cwd);
            fs::File::options()
                .write(true)
                .open(path)
                .unwrap()
                .set_modified(SystemTime::now() - Duration::from_secs(3600))
                .unwrap();
        }
        make_codex_rollout(&day, &format!("rollout-test-{current}.jsonl"), cwd);
        let discover = || discover_codex_session(cwd, &[], Some(cwd));
        assert_eq!(discover().as_deref(), Some(current));
        let count = || {
            CODEX_ROLLOUT_STATS
                .iter()
                .filter(|entry| entry.key().starts_with(root.path()))
                .map(|entry| *entry.value())
                .sum::<usize>()
        };
        let before = count();
        for _ in 0..10 {
            assert_eq!(discover().as_deref(), Some(current));
        }
        assert_eq!(
            count() - before,
            0,
            "unchanged discovery must not stat rollouts again; scans={:?}; metadata counts by path={:?}",
            CODEX_SCAN_DIAGNOSTICS
                .get(&sessions)
                .map(|entry| entry.value().clone()),
            CODEX_ROLLOUT_STATS
                .iter()
                .filter(|entry| entry.key().starts_with(root.path()))
                .map(|entry| (entry.key().clone(), *entry.value()))
                .collect::<Vec<_>>()
        );
    }

    // Catches a cached candidate hiding a replacement session, stale claimed-ID
    // filtering, and a deleted or atomically replaced file remaining resumable.
    #[test]
    fn codex_watched_discovery_observes_replacement_and_deletion() {
        let root = TempDir::new().unwrap();
        let sessions = root.path().join("sessions");
        let day = codex_recent_day_dirs_for_test(&sessions);
        fs::create_dir_all(&day).unwrap();
        let cwd = root.path().to_str().unwrap();
        let first = "bbbbbbbb-bbbb-bbbb-bbbb-bbbbbbbbbbbb";
        let second = "cccccccc-cccc-cccc-cccc-cccccccccccc";
        make_codex_rollout(&day, &format!("rollout-test-{first}.jsonl"), cwd);
        assert_eq!(
            discover_codex_session(cwd, &[], Some(cwd)).as_deref(),
            Some(first)
        );
        let index = CODEX_INDEXES.get(&sessions).unwrap();
        assert!(
            index.watcher.lock().unwrap().is_some(),
            "real filesystem watcher must be available"
        );
        let wait_for = |expected: Option<&str>| {
            let deadline = std::time::Instant::now() + Duration::from_secs(30);
            loop {
                let actual = discover_codex_session(cwd, &[], Some(cwd));
                if actual.as_deref() == expected {
                    break;
                }
                assert!(
                    std::time::Instant::now() < deadline,
                    "Codex watcher did not publish {expected:?}; got {actual:?}"
                );
                std::thread::sleep(Duration::from_millis(20));
            }
        };
        let path = make_codex_rollout(&day, &format!("rollout-test-{second}.jsonl"), cwd);
        fs::File::options()
            .write(true)
            .open(&path)
            .unwrap()
            .set_modified(SystemTime::now() + Duration::from_secs(1))
            .unwrap();
        wait_for(Some(second));
        assert_eq!(
            discover_codex_session(cwd, &[second.to_string()], Some(cwd)).as_deref(),
            Some(first)
        );
        fs::remove_file(&path).unwrap();
        wait_for(Some(first));
        // Replace the remaining file with another cwd at the same path. Cached
        // cwd metadata must not let the original project keep the candidate.
        let other = root.path().join("other");
        fs::create_dir_all(&other).unwrap();
        let temporary = make_codex_rollout(&day, "replacement.tmp", other.to_str().unwrap());
        fs::rename(temporary, day.join(format!("rollout-test-{first}.jsonl"))).unwrap();
        wait_for(None);
    }

    // Catches: replacing a date directory leaves discovery attached to its deleted inode.
    #[test]
    fn codex_replaced_day_directory_keeps_observing_new_rollouts() {
        let root = TempDir::new().unwrap();
        let sessions = root.path().join("sessions");
        let day = codex_recent_day_dirs_for_test(&sessions);
        fs::create_dir_all(&day).unwrap();
        let cwd = root.path().to_str().unwrap();
        let first = "bbbbbbbb-bbbb-bbbb-bbbb-bbbbbbbbbbbb";
        let second = "cccccccc-cccc-cccc-cccc-cccccccccccc";
        let third = "dddddddd-dddd-dddd-dddd-dddddddddddd";
        make_codex_rollout(&day, &format!("rollout-test-{first}.jsonl"), cwd);
        assert_eq!(
            discover_codex_session(cwd, &[], Some(cwd)).as_deref(),
            Some(first)
        );
        let index = CODEX_INDEXES.get(&sessions).unwrap();
        assert!(
            index.watcher.lock().unwrap().is_some(),
            "real watcher must be available"
        );
        // No discovery call between removal and recreation: the date paths are equal.
        fs::remove_dir_all(&day).unwrap();
        fs::create_dir_all(&day).unwrap();
        make_codex_rollout(&day, &format!("rollout-test-{second}.jsonl"), cwd);
        let wait_for = |expected: &str| {
            let deadline = std::time::Instant::now() + Duration::from_secs(30);
            loop {
                let actual = discover_codex_session(cwd, &[], Some(cwd));
                if actual.as_deref() == Some(expected) {
                    break;
                }
                assert!(
                    std::time::Instant::now() < deadline,
                    "replacement day watcher did not discover {expected}; got {actual:?}"
                );
                std::thread::sleep(Duration::from_millis(20));
            }
        };
        wait_for(second);
        // Drain the directory-creation burst before changing only a child file.
        std::thread::sleep(Duration::from_millis(200));
        assert_eq!(
            discover_codex_session(cwd, &[], Some(cwd)).as_deref(),
            Some(second)
        );
        let path = make_codex_rollout(&day, &format!("rollout-test-{third}.jsonl"), cwd);
        fs::File::options()
            .write(true)
            .open(path)
            .unwrap()
            .set_modified(SystemTime::now() + Duration::from_secs(1))
            .unwrap();
        wait_for(third);
    }

    // Catches an unchanged in-memory candidate bypassing the five-minute age bound.
    #[test]
    fn codex_cached_candidate_still_expires_without_file_events() {
        let root = TempDir::new().unwrap();
        let sessions = root.path().join("sessions");
        let day = codex_recent_day_dirs_for_test(&sessions);
        fs::create_dir_all(&day).unwrap();
        let cwd = root.path().to_str().unwrap();
        make_codex_rollout(
            &day,
            "rollout-test-bbbbbbbb-bbbb-bbbb-bbbb-bbbbbbbbbbbb.jsonl",
            cwd,
        );
        let now = SystemTime::now();
        assert_eq!(
            discover_codex_session_at(cwd, &[], Some(cwd), now).as_deref(),
            Some("bbbbbbbb-bbbb-bbbb-bbbb-bbbbbbbbbbbb")
        );
        assert_eq!(
            discover_codex_session_at(
                cwd,
                &[],
                Some(cwd),
                now + SESSION_MAX_AGE + Duration::from_secs(1)
            ),
            None,
            "discovery must not resume an expired cached session"
        );
    }

    fn codex_recent_day_dirs_for_test(root: &Path) -> PathBuf {
        use chrono::Datelike;
        let today = chrono::Local::now().date_naive();
        codex_day_dir(root, today.year(), today.month(), today.day())
    }

    #[test]
    fn test_discover_codex_session_ignores_old_date_subtree() {
        use chrono::Datelike;
        let root = TempDir::new().unwrap();
        let sessions = root.path().join("sessions");
        let today = chrono::Local::now().date_naive();

        let today_dir = codex_day_dir(&sessions, today.year(), today.month(), today.day());
        let old_dir = codex_day_dir(&sessions, 2020, 1, 1);
        fs::create_dir_all(&today_dir).unwrap();
        fs::create_dir_all(&old_dir).unwrap();

        let project = root.path().join("project-a");
        fs::create_dir_all(&project).unwrap();
        let cwd = project.to_str().unwrap();

        let fresh = "bbbbbbbb-bbbb-bbbb-bbbb-bbbbbbbbbbbb";
        let stale = "aaaaaaaa-aaaa-aaaa-aaaa-aaaaaaaaaaaa";
        make_codex_rollout(
            &today_dir,
            &format!("rollout-2026-07-07T10-00-00-{fresh}.jsonl"),
            cwd,
        );
        // Create the stale file LAST so its mtime is newest — if the old subtree
        // weren't pruned it would win the newest-first sort, so returning `fresh`
        // proves the date prune worked.
        std::thread::sleep(Duration::from_millis(10));
        make_codex_rollout(
            &old_dir,
            &format!("rollout-2020-01-01T10-00-00-{stale}.jsonl"),
            cwd,
        );

        let result = discover_codex_session(cwd, &[], Some(root.path().to_str().unwrap()));
        assert_eq!(result, Some(fresh.to_string()));
    }

    /// The date-dir prune bounds the walk to 24-48h, which is NOT a recency check:
    /// a session abandoned this morning still sits in today's dir. Claude, Gemini and
    /// Grok all reject candidates older than SESSION_MAX_AGE per file; Codex did not.
    #[test]
    fn test_discover_codex_session_rejects_a_stale_session_in_todays_dir() {
        use chrono::Datelike;
        let root = TempDir::new().unwrap();
        let sessions = root.path().join("sessions");
        let today = chrono::Local::now().date_naive();
        let today_dir = codex_day_dir(&sessions, today.year(), today.month(), today.day());
        fs::create_dir_all(&today_dir).unwrap();

        let project = root.path().join("project-a");
        fs::create_dir_all(&project).unwrap();
        let cwd = project.to_str().unwrap();

        let stale = "cccccccc-cccc-cccc-cccc-cccccccccccc";
        let path = make_codex_rollout(
            &today_dir,
            &format!("rollout-2026-07-07T10-00-00-{stale}.jsonl"),
            cwd,
        );
        // Same day, but well past the 5-minute bound.
        let old = std::time::SystemTime::now() - Duration::from_secs(3600);
        fs::File::options()
            .write(true)
            .open(&path)
            .unwrap()
            .set_modified(old)
            .unwrap();

        assert_eq!(
            discover_codex_session(cwd, &[], Some(root.path().to_str().unwrap())),
            None,
            "a session older than SESSION_MAX_AGE must not be resumed"
        );
    }

    /// Codex does not partition sessions by project, so without cwd scoping a fresh
    /// terminal in project A bound to the globally-newest unclaimed session and
    /// resumed project B's history.
    #[test]
    fn test_discover_codex_session_ignores_another_projects_session() {
        use chrono::Datelike;
        let root = TempDir::new().unwrap();
        let sessions = root.path().join("sessions");
        let today = chrono::Local::now().date_naive();
        let today_dir = codex_day_dir(&sessions, today.year(), today.month(), today.day());
        fs::create_dir_all(&today_dir).unwrap();

        let project_a = root.path().join("project-a");
        let project_b = root.path().join("project-b");
        fs::create_dir_all(&project_a).unwrap();
        fs::create_dir_all(&project_b).unwrap();

        let mine = "dddddddd-dddd-dddd-dddd-dddddddddddd";
        let theirs = "eeeeeeee-eeee-eeee-eeee-eeeeeeeeeeee";
        make_codex_rollout(
            &today_dir,
            &format!("rollout-2026-07-07T10-00-00-{mine}.jsonl"),
            project_a.to_str().unwrap(),
        );
        // Newest, so it wins the sort — only the cwd filter can keep it out.
        std::thread::sleep(Duration::from_millis(10));
        make_codex_rollout(
            &today_dir,
            &format!("rollout-2026-07-07T10-00-01-{theirs}.jsonl"),
            project_b.to_str().unwrap(),
        );

        let home = Some(root.path().to_str().unwrap());
        assert_eq!(
            discover_codex_session(project_a.to_str().unwrap(), &[], home),
            Some(mine.to_string()),
            "the other project's newer session must not be selected"
        );
        assert_eq!(
            discover_codex_session(project_b.to_str().unwrap(), &[], home),
            Some(theirs.to_string()),
            "and project B still finds its own"
        );
    }

    /// A rollout whose cwd cannot be read is rejected. Accepting it would silently
    /// reinstate cross-project binding the moment Codex changed its file format.
    #[test]
    fn test_discover_codex_session_rejects_a_rollout_with_no_readable_cwd() {
        use chrono::Datelike;
        let root = TempDir::new().unwrap();
        let sessions = root.path().join("sessions");
        let today = chrono::Local::now().date_naive();
        let today_dir = codex_day_dir(&sessions, today.year(), today.month(), today.day());
        fs::create_dir_all(&today_dir).unwrap();

        let project = root.path().join("project-a");
        fs::create_dir_all(&project).unwrap();

        let id = "ffffffff-ffff-ffff-ffff-ffffffffffff";
        // `{}` — a well-formed JSON line with no payload, i.e. a format we don't know.
        make_file(
            &today_dir,
            &format!("rollout-2026-07-07T10-00-00-{id}.jsonl"),
        );

        assert_eq!(
            discover_codex_session(
                project.to_str().unwrap(),
                &[],
                Some(root.path().to_str().unwrap())
            ),
            None
        );
    }

    /// The recorded cwd and the session cwd may spell the same directory differently
    /// (symlinked /tmp on macOS, a trailing slash). Compare canonical paths.
    #[test]
    fn test_discover_codex_session_matches_an_equivalent_cwd_spelling() {
        use chrono::Datelike;
        let root = TempDir::new().unwrap();
        let sessions = root.path().join("sessions");
        let today = chrono::Local::now().date_naive();
        let today_dir = codex_day_dir(&sessions, today.year(), today.month(), today.day());
        fs::create_dir_all(&today_dir).unwrap();

        let project = root.path().join("project-a");
        fs::create_dir_all(&project).unwrap();

        let id = "12345678-1234-1234-1234-123456789abc";
        make_codex_rollout(
            &today_dir,
            &format!("rollout-2026-07-07T10-00-00-{id}.jsonl"),
            project.to_str().unwrap(),
        );

        // Same directory reached through a `.` component.
        let indirect = project.join(".");
        assert_eq!(
            discover_codex_session(
                indirect.to_str().unwrap(),
                &[],
                Some(root.path().to_str().unwrap())
            ),
            Some(id.to_string())
        );
    }

    // ── extract_codex_uuid ──

    #[test]
    fn test_extract_codex_uuid_valid() {
        let name = "rollout-2026-02-03T13-40-28-af467730-5e79-49d9-8a17-ebd94c99f262.jsonl";
        assert_eq!(
            extract_codex_uuid(name),
            Some("af467730-5e79-49d9-8a17-ebd94c99f262".to_string())
        );
    }

    #[test]
    fn test_extract_codex_uuid_wrong_prefix() {
        assert!(
            extract_codex_uuid("session-2026-af467730-5e79-49d9-8a17-ebd94c99f262.jsonl").is_none()
        );
    }

    #[test]
    fn test_extract_codex_uuid_no_suffix() {
        assert!(
            extract_codex_uuid("rollout-2026-af467730-5e79-49d9-8a17-ebd94c99f262.txt").is_none()
        );
    }

    // ── extract_json_string_field ──

    #[test]
    fn test_extract_json_string_field_present() {
        let json = r#"{"sessionId": "af467730-5e79-49d9-8a17-ebd94c99f262", "messages": []}"#;
        assert_eq!(
            extract_json_string_field(json, "sessionId"),
            Some("af467730-5e79-49d9-8a17-ebd94c99f262".to_string())
        );
    }

    #[test]
    fn test_extract_json_string_field_missing() {
        let json = r#"{"messages": []}"#;
        assert!(extract_json_string_field(json, "sessionId").is_none());
    }

    #[test]
    fn test_extract_json_string_field_non_string_value() {
        let json = r#"{"count": 42}"#;
        assert!(extract_json_string_field(json, "count").is_none());
    }

    // ── verify_claude_session ──

    #[test]
    fn test_verify_claude_session_exists() {
        let dir = TempDir::new().unwrap();
        let uuid = "af467730-5e79-49d9-8a17-ebd94c99f262";
        let slug = path_to_claude_slug("/fake/project");
        let project_dir = dir.path().join(&slug);
        fs::create_dir_all(&project_dir).unwrap();
        make_file(&project_dir, &format!("{uuid}.jsonl"));

        // Temporarily override the home dir by checking the file directly
        // (we can't mock dirs::home_dir, so test the inner logic)
        assert!(project_dir.join(format!("{uuid}.jsonl")).exists());
    }

    #[test]
    fn test_verify_claude_session_not_found() {
        let dir = TempDir::new().unwrap();
        let slug = path_to_claude_slug("/fake/project");
        let project_dir = dir.path().join(&slug);
        fs::create_dir_all(&project_dir).unwrap();

        assert!(!project_dir.join("nonexistent-uuid.jsonl").exists());
    }

    #[test]
    fn test_verify_agent_session_invalid_uuid() {
        // Invalid UUIDs should always return false
        assert!(!verify_claude_session("not-a-uuid", "/tmp", None));
    }

    #[test]
    fn test_verify_agent_session_unknown_agent() {
        assert!(!verify_agent_session(
            "unknown-agent".to_string(),
            "af467730-5e79-49d9-8a17-ebd94c99f262".to_string(),
            "/tmp".to_string(),
            None,
            HashMap::new(),
        ));
    }

    // ─── Launch command rebuild ──────────────────────────────────────────────

    fn argv(tokens: &[&str]) -> Vec<String> {
        tokens.iter().map(|t| (*t).to_string()).collect()
    }

    fn env(pairs: &[(&str, &str)]) -> HashMap<String, String> {
        pairs
            .iter()
            .map(|(k, v)| ((*k).to_string(), (*v).to_string()))
            .collect()
    }

    /// The bug this whole path exists for: the alias is expanded before exec, so the
    /// config dir survives only in the process env. Without it the resume command
    /// reads `~/.claude` and Claude answers "No conversation found with session ID".
    #[test]
    fn compose_launch_command_keeps_the_config_dir_the_alias_hid() {
        let cmd = compose_launch_command(
            "claude",
            &argv(&["claude", "--dangerously-skip-permissions"]),
            env(&[("CLAUDE_CONFIG_DIR", "/Users/me/.claude-private")]),
        );
        assert_eq!(
            cmd.as_deref(),
            Some(
                "CLAUDE_CONFIG_DIR=/Users/me/.claude-private claude --dangerously-skip-permissions"
            )
        );
    }

    #[test]
    fn compose_launch_command_without_env_is_just_the_argv() {
        let cmd = compose_launch_command(
            "claude",
            &argv(&["claude", "--model", "opus"]),
            HashMap::new(),
        );
        assert_eq!(cmd.as_deref(), Some("claude --model opus"));
    }

    /// A session flag from the previous run must not survive: TUIC appends its own
    /// `--resume <id>`, and two of them select different conversations.
    #[test]
    fn compose_launch_command_drops_session_flags_and_their_values() {
        let cmd = compose_launch_command(
            "claude",
            &argv(&[
                "claude",
                "--resume",
                "af467730-5e79-49d9-8a17-ebd94c99f262",
                "--model",
                "opus",
                "--continue",
                "--session-id",
                "af467730-5e79-49d9-8a17-ebd94c99f263",
                "--fork-session",
            ]),
            HashMap::new(),
        );
        assert_eq!(cmd.as_deref(), Some("claude --model opus"));
    }

    /// `--resume` takes an optional value, so a bare one must not eat the next flag.
    #[test]
    fn compose_launch_command_keeps_a_flag_that_follows_a_valueless_resume() {
        let cmd = compose_launch_command(
            "claude",
            &argv(&["claude", "--resume", "--dangerously-skip-permissions"]),
            HashMap::new(),
        );
        assert_eq!(
            cmd.as_deref(),
            Some("claude --dangerously-skip-permissions")
        );
    }

    /// Start Work launches `<cmd> -- '<brief>'`; resume would submit that task again.
    #[test]
    fn compose_launch_command_drops_the_start_work_brief_and_its_separator() {
        let brief = format!("{SEED_BRIEF_OPEN}\nFix issue 125\n</untrusted-content>\nuntrusted.");
        let cmd = compose_launch_command(
            "claude",
            &argv(&["claude", "--add-dir", "/repo/shared", "--", &brief]),
            env(&[("CLAUDE_CONFIG_DIR", "/Users/me/.claude-private")]),
        );
        assert_eq!(
            cmd.as_deref(),
            Some("CLAUDE_CONFIG_DIR=/Users/me/.claude-private claude --add-dir /repo/shared")
        );
    }

    /// Only the host's own brief goes: a user's trailing prompt and `--` stay.
    #[test]
    fn compose_launch_command_keeps_a_prompt_that_is_not_the_start_work_brief() {
        let cmd = compose_launch_command(
            "claude",
            &argv(&["claude", "--", "fix this"]),
            HashMap::new(),
        );
        assert_eq!(
            cmd,
            Some(format!(
                "claude -- {}",
                crate::prompt::shell_quote("fix this")
            ))
        );
    }

    #[test]
    fn compose_launch_command_quotes_only_what_needs_it() {
        let cmd = compose_launch_command(
            "claude",
            &argv(&["claude", "--append-system-prompt", "be terse"]),
            env(&[("CLAUDE_CONFIG_DIR", "/Users/me/My Configs/.claude")]),
        );
        // The quote character belongs to the host shell — `'` on unix, `"` on
        // `cmd` — and the command is handed to that shell. What this test pins
        // is which tokens get quoted at all: the two holding a space, and
        // neither of the two that do not.
        let dir = crate::prompt::shell_quote("/Users/me/My Configs/.claude");
        let prompt = crate::prompt::shell_quote("be terse");
        assert_eq!(
            cmd,
            Some(format!(
                "CLAUDE_CONFIG_DIR={dir} claude --append-system-prompt {prompt}"
            ))
        );
    }

    /// Agents with no verified session-flag list get no rebuilt command, so the
    /// caller keeps the run config it already had rather than a guessed rewrite.
    #[test]
    fn compose_launch_command_declines_agents_without_a_flag_list() {
        assert_eq!(
            compose_launch_command("codex", &argv(&["codex"]), HashMap::new()),
            None
        );
        assert_eq!(
            compose_launch_command("claude", &[], HashMap::new()),
            None,
            "empty argv has no binary to launch"
        );
    }

    /// The composed half is unit-tested above; this covers the half that reads a
    /// real process, which is where the platform APIs can silently return nothing.
    #[cfg(any(target_os = "macos", target_os = "linux"))]
    #[test]
    fn rebuild_launch_command_reads_a_live_process() {
        let pid = std::process::id();
        let rebuilt = rebuild_launch_command("claude", pid).expect("own process is readable");
        let argv0 = crate::process_env::read_process_argv0(pid).expect("own argv0 is readable");
        assert!(
            rebuilt.contains(&argv0),
            "rebuilt command {rebuilt:?} should name the running binary {argv0:?}"
        );
        assert_eq!(
            rebuild_launch_command("codex", pid),
            None,
            "an agent with no verified flag list gets no rebuilt command"
        );
    }

    /// Discovery must not invent a launch command when it has no pid to read.
    #[tokio::test]
    async fn discover_agent_session_reports_no_launch_command_without_a_pid() {
        let found = discover_agent_session(
            "goose".to_string(),
            "/tmp".to_string(),
            Vec::new(),
            None,
            HashMap::new(),
        )
        .await;
        assert_eq!(found, None, "goose has no filesystem discovery");
    }
}

#[cfg(test)]
mod attachment_resume_tests {
    // Catches: ego launch discovery starts persisting a deleted session read root.
    #[test]
    fn attachment_resume_does_not_persist_ego_generated_root() {
        let argv = vec![
            "ego".into(),
            "--read-only-dir=/tuic/attachments/pty-old".into(),
        ];
        assert!(super::compose_launch_command("ego", &argv, Default::default()).is_none());
    }

    // Catches: canonicalizing only the configured root retains an expired grant via its alias.
    #[cfg(unix)]
    #[test]
    fn attachment_resume_drops_expired_grants_through_config_alias() {
        let root = tempfile::tempdir_in(crate::test_support::test_temp_root()).unwrap();
        let actual = root.path().join("actual");
        std::fs::create_dir_all(actual.join("attachments")).unwrap();
        let alias = root.path().join("alias");
        std::os::unix::fs::symlink(&actual, &alias).unwrap();
        let _guard = crate::config::set_config_dir_override(alias.clone());
        let argv = vec![
            "claude".into(),
            format!("--add-dir={}", alias.join("attachments/pty-old").display()),
            "--add-dir=/user/docs".into(),
        ];
        assert_eq!(
            super::compose_launch_command("claude", &argv, Default::default()).unwrap(),
            "claude --add-dir=/user/docs"
        );
    }

    // Catches: resume reuses a removed TUIC directory or discards a user's directory grant.
    #[test]
    fn attachment_resume_drops_only_generated_directory_grants() {
        let root = tempfile::tempdir_in(crate::test_support::test_temp_root()).unwrap();
        let _guard = crate::config::set_config_dir_override(root.path().into());
        let old = root.path().join("attachments/pty-old");
        let argv = vec![
            "claude".into(),
            format!("--add-dir={}", old.display()),
            "--add-dir=/user/docs".into(),
            "--resume".into(),
            "old-session".into(),
        ];
        let rebuilt = super::compose_launch_command("claude", &argv, Default::default()).unwrap();
        assert_eq!(rebuilt, "claude --add-dir=/user/docs");
    }
}
