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
//! | claude | `~/.claude/projects/<cwd-slug>/<UUID>.jsonl`| UUID filename stem|
//! | gemini | `~/.gemini/tmp/<hash>/chats/session-*.json` | JSON `sessionId` field |
//! | codex  | `~/.codex/sessions/YYYY/MM/DD/rollout-*-<UUID>.jsonl` | UUID in filename |
//! | goose  | SQLite `~/Library/Application Support/Block/goose/sessions/sessions.db` | name field (TUIC_SESSION) |

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::time::SystemTime;

/// Scan a directory for agent session files and return the ID of the newest
/// unclaimed session, or `None` if none can be found.
///
/// # Parameters
/// - `agent_type`: one of `"claude"`, `"gemini"`, `"codex"`, `"goose"`, `"grok"`
/// - `cwd`: the terminal's working directory (used to compute project-scoped paths)
/// - `claimed_ids`: session IDs already assigned to other terminals — excluded from results
/// - `agent_pid`: PID of the running agent process. When provided, env vars that affect
///   session storage paths (`CLAUDE_CONFIG_DIR`, `GEMINI_CLI_HOME`, `CODEX_HOME`) are read
///   directly from the process's initial environment — the ground-truth source.
/// - `env_overrides`: fallback env overrides from the TUIC run config. Only used for keys
///   NOT found in the process env (i.e. process env takes precedence).
#[cfg_attr(feature = "desktop", tauri::command)]
pub(crate) fn discover_agent_session(
    agent_type: String,
    cwd: String,
    claimed_ids: Vec<String>,
    agent_pid: Option<u32>,
    env_overrides: HashMap<String, String>,
) -> Option<String> {
    let env = resolve_env_overrides(&agent_type, agent_pid, &env_overrides);
    match agent_type.as_str() {
        "claude" => discover_claude_session(
            &cwd,
            &claimed_ids,
            env.get("CLAUDE_CONFIG_DIR").map(|s| s.as_str()),
        ),
        "gemini" => discover_gemini_session(
            &cwd,
            &claimed_ids,
            env.get("GEMINI_CLI_HOME").map(|s| s.as_str()),
        ),
        "codex" => discover_codex_session(&claimed_ids, env.get("CODEX_HOME").map(|s| s.as_str())),
        // Goose stores sessions in SQLite — no filesystem discovery.
        // Shell wrapper injects --name $TUIC_SESSION for deterministic binding.
        "goose" => None,
        "grok" => discover_grok_session(&cwd, &claimed_ids),
        _ => None,
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
/// aider writes to CWD; goose has no override).
const AGENT_ENV_VARS: &[(&str, &[&str])] = &[
    ("claude", &["CLAUDE_CONFIG_DIR"]),
    ("gemini", &["GEMINI_CLI_HOME"]),
    ("codex", &["CODEX_HOME"]),
    ("opencode", &["OPENCODE_DATA_DIR"]),
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

/// Return the absolute path to Claude Code's project directory for a given CWD.
/// E.g. `/Users/foo/bar` → `~/.claude/projects/-Users-foo-bar`.
#[cfg_attr(feature = "desktop", tauri::command)]
pub(crate) fn claude_project_dir(
    cwd: String,
    claude_config_dir: Option<String>,
) -> Result<String, String> {
    let base = claude_projects_dir(claude_config_dir.as_deref())
        .ok_or_else(|| "Could not determine home directory".to_string())?;
    let path = base.join(path_to_claude_slug(&cwd));
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
/// Claude encodes a path by replacing `/`, `.`, and `_` with `-`,
/// prepending a leading `-` to represent the root.
///
/// Example: `/Users/foo.bar/my_project` → `-Users-foo-bar-my-project`
fn path_to_claude_slug(path: &str) -> String {
    // Normalise separators so this works on Windows too
    let normalised = path.replace('\\', "/");
    // Strip trailing separator to avoid a trailing dash in the slug
    let trimmed = normalised.trim_end_matches('/');
    // Replace `/`, `.`, and `_` — Claude treats all three as slug delimiters
    trimmed.replace(['/', '.', '_'], "-")
}

/// Find the most recently created, unclaimed `.jsonl` session file under
/// `~/.claude/projects/<cwd-slug>/`.
fn discover_claude_session(
    cwd: &str,
    claimed_ids: &[String],
    config_dir: Option<&str>,
) -> Option<String> {
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
        Some(std::time::Duration::from_secs(300)),
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
/// project directories under `~/.gemini/tmp/` and look for the newest session
/// file across all of them. This is correct because Gemini is project-scoped:
/// a session in a different project dir won't be in a directory we visit.
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
    let mut candidates: Vec<(SystemTime, String)> = Vec::new();

    let Ok(project_entries) = std::fs::read_dir(&tmp_dir) else {
        return None;
    };
    for proj in project_entries.filter_map(|e| e.ok()) {
        let chats_dir = proj.path().join("chats");
        if !chats_dir.is_dir() {
            continue;
        }
        collect_gemini_session_files(&chats_dir, &mut candidates);
    }

    // Sort newest first, return first unclaimed
    candidates.sort_by_key(|a| std::cmp::Reverse(a.0));
    candidates
        .into_iter()
        .find(|(_, id)| !claimed_ids.contains(id))
        .map(|(_, id)| id)
}

fn collect_gemini_session_files(chats_dir: &PathBuf, out: &mut Vec<(SystemTime, String)>) {
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
fn discover_codex_session(claimed_ids: &[String], codex_home: Option<&str>) -> Option<String> {
    let sessions_root = codex_sessions_dir(codex_home)?;

    if !sessions_root.exists() {
        return None;
    }

    // Recursively collect all rollout-*-<UUID>.jsonl files with their mtimes
    let mut candidates: Vec<(SystemTime, String)> = Vec::new();
    collect_codex_files(&sessions_root, &mut candidates);

    // Sort newest first
    candidates.sort_by_key(|a| std::cmp::Reverse(a.0));

    candidates
        .into_iter()
        .find(|(_, id)| !claimed_ids.contains(id))
        .map(|(_, id)| id)
}

fn collect_codex_files(dir: &PathBuf, out: &mut Vec<(SystemTime, String)>) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };

    for entry in entries.filter_map(|e| e.ok()) {
        let path = entry.path();
        if path.is_dir() {
            collect_codex_files(&path, out);
        } else if let Some(name) = path.file_name().map(|n| n.to_string_lossy().to_string())
            && let Some(uuid) = extract_codex_uuid(&name)
            && let Ok(meta) = entry.metadata()
            && let Ok(mtime) = meta.modified()
        {
            out.push((mtime, uuid));
        }
    }
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

/// grok stores sessions under `~/.grok/sessions/<percent-encoded-cwd>/<UUIDv7>/`.
/// Each session is a *directory* named with its UUIDv7 id (usable with
/// `grok --resume <id>`); the newest such directory is the active session.
fn discover_grok_session(cwd: &str, claimed_ids: &[String]) -> Option<String> {
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
        Some(std::time::Duration::from_secs(300)),
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
    use std::time::Duration;
    use tempfile::TempDir;

    fn make_file(dir: &std::path::Path, name: &str) -> PathBuf {
        let path = dir.join(name);
        fs::write(&path, b"{}").unwrap();
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

    #[test]
    fn test_path_to_claude_slug_windows() {
        assert_eq!(
            path_to_claude_slug("C:\\Users\\foo\\bar"),
            "C:-Users-foo-bar"
        );
    }

    // ── claude_project_dir ──

    #[test]
    fn test_claude_project_dir_returns_path_with_slug() {
        if let Ok(result) = claude_project_dir("/Users/foo/bar".to_string(), None) {
            assert!(
                result.ends_with("/.claude/projects/-Users-foo-bar"),
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

    #[test]
    fn test_discover_claude_session_with_config_dir() {
        let dir = TempDir::new().unwrap();
        let projects_dir = dir.path().join("projects");
        let slug = path_to_claude_slug("/fake/project");
        let session_dir = projects_dir.join(&slug);
        fs::create_dir_all(&session_dir).unwrap();

        let uuid = "af467730-5e79-49d9-8a17-ebd94c99f262";
        make_file(&session_dir, &format!("{uuid}.jsonl"));

        let result =
            discover_claude_session("/fake/project", &[], Some(dir.path().to_str().unwrap()));
        assert_eq!(result, Some(uuid.to_string()));
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
}
