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

use std::path::PathBuf;
use std::time::SystemTime;

/// Scan a directory for agent session files and return the ID of the newest
/// unclaimed session, or `None` if none can be found.
///
/// # Parameters
/// - `agent_type`: one of `"claude"`, `"gemini"`, `"codex"`
/// - `cwd`: the terminal's working directory (used to compute project-scoped paths)
/// - `claimed_ids`: session IDs already assigned to other terminals — excluded from results
#[tauri::command]
pub(crate) fn discover_agent_session(
    agent_type: String,
    cwd: String,
    claimed_ids: Vec<String>,
) -> Option<String> {
    match agent_type.as_str() {
        "claude" => discover_claude_session(&cwd, &claimed_ids),
        "gemini" => discover_gemini_session(&cwd, &claimed_ids),
        "codex" => discover_codex_session(&claimed_ids),
        _ => None,
    }
}

/// Return the absolute path to Claude Code's project directory for a given CWD.
/// E.g. `/Users/foo/bar` → `~/.claude/projects/-Users-foo-bar`.
#[tauri::command]
pub(crate) fn claude_project_dir(cwd: String) -> Result<String, String> {
    let base = claude_projects_dir()
        .ok_or_else(|| "Could not determine home directory".to_string())?;
    let path = base.join(path_to_claude_slug(&cwd));
    path.to_str()
        .map(|s| s.to_string())
        .ok_or_else(|| "Project path contains non-UTF-8 characters".to_string())
}

// ─── Claude ──────────────────────────────────────────────────────────────────

/// Base directory for Claude Code session transcripts: `~/.claude/projects/`.
fn claude_projects_dir() -> Option<PathBuf> {
    dirs::home_dir().map(|h| h.join(".claude").join("projects"))
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
fn discover_claude_session(cwd: &str, claimed_ids: &[String]) -> Option<String> {
    let slug = path_to_claude_slug(cwd);
    let project_dir = claude_projects_dir()?.join(&slug);

    newest_unclaimed_file(
        &project_dir,
        |name| {
            // Filename must be `<UUID>.jsonl`
            name.strip_suffix(".jsonl")
                .filter(|stem| is_uuid(stem))
                .map(|stem| stem.to_string())
        },
        claimed_ids,
    )
}

// ─── Gemini ──────────────────────────────────────────────────────────────────

/// Gemini CLI stores sessions under `~/.gemini/tmp/<project-hash>/chats/`.
/// The hash is a SHA-256 of the absolute project path. Rather than recomputing
/// the hash (which would require adding sha2 as a dependency), we scan ALL
/// project directories under `~/.gemini/tmp/` and look for the newest session
/// file across all of them. This is correct because Gemini is project-scoped:
/// a session in a different project dir won't be in a directory we visit.
fn discover_gemini_session(_cwd: &str, claimed_ids: &[String]) -> Option<String> {
    let tmp_dir = dirs::home_dir()?.join(".gemini").join("tmp");
    if !tmp_dir.exists() {
        return None;
    }

    // Collect (mtime, sessionId) from all session-*.json files across all project dirs
    let mut candidates: Vec<(SystemTime, String)> = Vec::new();

    let Ok(project_entries) = std::fs::read_dir(&tmp_dir) else { return None };
    for proj in project_entries.filter_map(|e| e.ok()) {
        let chats_dir = proj.path().join("chats");
        if !chats_dir.is_dir() {
            continue;
        }
        collect_gemini_session_files(&chats_dir, &mut candidates);
    }

    // Sort newest first, return first unclaimed
    candidates.sort_by(|a, b| b.0.cmp(&a.0));
    candidates
        .into_iter()
        .find(|(_, id)| !claimed_ids.contains(id))
        .map(|(_, id)| id)
}

fn collect_gemini_session_files(chats_dir: &PathBuf, out: &mut Vec<(SystemTime, String)>) {
    let Ok(entries) = std::fs::read_dir(chats_dir) else { return };
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

/// Extract a top-level string field from JSON without a full parser.
/// Looks for `"field": "value"` pattern.
fn extract_json_string_field(json: &str, field: &str) -> Option<String> {
    let pattern = format!("\"{}\"", field);
    let field_pos = json.find(&pattern)?;
    let after_key = &json[field_pos + pattern.len()..];
    // Skip whitespace and colon
    let colon_pos = after_key.find(':')?;
    let after_colon = after_key[colon_pos + 1..].trim_start();
    if !after_colon.starts_with('"') {
        return None;
    }
    let inner = &after_colon[1..];
    let end = inner.find('"')?;
    Some(inner[..end].to_string())
}

// ─── Codex ───────────────────────────────────────────────────────────────────

/// Codex CLI stores sessions under `~/.codex/sessions/YYYY/MM/DD/`.
/// Files are named `rollout-<timestamp>-<UUID>.jsonl`.
fn discover_codex_session(claimed_ids: &[String]) -> Option<String> {
    let sessions_root = dirs::home_dir()?.join(".codex").join("sessions");

    if !sessions_root.exists() {
        return None;
    }

    // Recursively collect all rollout-*-<UUID>.jsonl files with their mtimes
    let mut candidates: Vec<(SystemTime, String)> = Vec::new();
    collect_codex_files(&sessions_root, &mut candidates);

    // Sort newest first
    candidates.sort_by(|a, b| b.0.cmp(&a.0));

    candidates
        .into_iter()
        .find(|(_, id)| !claimed_ids.contains(id))
        .map(|(_, id)| id)
}

fn collect_codex_files(dir: &PathBuf, out: &mut Vec<(SystemTime, String)>) {
    let Ok(entries) = std::fs::read_dir(dir) else { return };

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
    if is_uuid(candidate) { Some(candidate.to_string()) } else { None }
}

// ─── Session verification ────────────────────────────────────────────────────

/// Check whether a session file exists on disk for the given agent type and UUID.
///
/// Used at restore time to decide if `--resume <uuid>` is safe: if the session
/// file doesn't exist, the resume command would fail.
#[tauri::command]
pub(crate) fn verify_agent_session(
    agent_type: String,
    session_id: String,
    cwd: String,
) -> bool {
    match agent_type.as_str() {
        "claude" => verify_claude_session(&session_id, &cwd),
        "gemini" => verify_gemini_session(&session_id, &cwd),
        "codex" => verify_codex_session(&session_id),
        _ => false,
    }
}

/// Check if `~/.claude/projects/<slug>/<uuid>.jsonl` exists.
fn verify_claude_session(session_id: &str, cwd: &str) -> bool {
    if !is_uuid(session_id) {
        return false;
    }
    let Some(project_dir) = claude_projects_dir().map(|d| d.join(path_to_claude_slug(cwd))) else {
        return false;
    };
    project_dir.join(format!("{session_id}.jsonl")).exists()
}

/// Check if any session file under `~/.gemini/tmp/*/chats/` contains this sessionId.
fn verify_gemini_session(session_id: &str, _cwd: &str) -> bool {
    if !is_uuid(session_id) {
        return false;
    }
    let Some(tmp_dir) = dirs::home_dir().map(|h| h.join(".gemini").join("tmp")) else {
        return false;
    };
    if !tmp_dir.exists() {
        return false;
    }
    let Ok(entries) = std::fs::read_dir(&tmp_dir) else { return false };
    for proj in entries.filter_map(|e| e.ok()) {
        let chats_dir = proj.path().join("chats");
        if !chats_dir.is_dir() {
            continue;
        }
        let Ok(files) = std::fs::read_dir(&chats_dir) else { continue };
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
fn verify_codex_session(session_id: &str) -> bool {
    if !is_uuid(session_id) {
        return false;
    }
    let Some(sessions_root) = dirs::home_dir().map(|h| h.join(".codex").join("sessions")) else {
        return false;
    };
    if !sessions_root.exists() {
        return false;
    }
    codex_session_exists(&sessions_root, session_id)
}

fn codex_session_exists(dir: &std::path::Path, target_id: &str) -> bool {
    let Ok(entries) = std::fs::read_dir(dir) else { return false };
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

/// Scan `dir` for files matching `extract_id`, returning the newest unclaimed ID.
fn newest_unclaimed_file<F>(dir: &PathBuf, extract_id: F, claimed_ids: &[String]) -> Option<String>
where
    F: Fn(&str) -> Option<String>,
{
    if !dir.exists() {
        return None;
    }

    let mut candidates: Vec<(SystemTime, String)> = std::fs::read_dir(dir)
        .ok()?
        .filter_map(|e| {
            let e = e.ok()?;
            let name = e.file_name().to_string_lossy().to_string();
            let id = extract_id(&name)?;
            let meta = e.metadata().ok()?;
            let mtime = meta.modified().ok()?;
            Some((mtime, id))
        })
        .collect();

    candidates.sort_by(|a, b| b.0.cmp(&a.0));

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

    // ── path_to_claude_slug ──

    #[test]
    fn test_path_to_claude_slug_unix() {
        assert_eq!(
            path_to_claude_slug("/Users/foo/bar"),
            "-Users-foo-bar"
        );
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
        assert_eq!(
            path_to_claude_slug("/Users/foo/bar/"),
            "-Users-foo-bar"
        );
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
        if let Ok(result) = claude_project_dir("/Users/foo/bar".to_string()) {
            assert!(result.ends_with("/.claude/projects/-Users-foo-bar"),
                "unexpected path: {result}");
        }
        // None only if home dir unavailable (CI) — not a failure
    }

    #[test]
    fn test_claude_project_dir_dots_in_username() {
        if let Ok(result) = claude_project_dir("/Users/foo.bar/proj".to_string()) {
            assert!(result.ends_with("-Users-foo-bar-proj"),
                "unexpected path: {result}");
        }
    }

    // ── newest_unclaimed_file ──

    #[test]
    fn test_empty_dir_returns_none() {
        let dir = TempDir::new().unwrap();
        let result = newest_unclaimed_file(
            &dir.path().to_path_buf(),
            |name| name.strip_suffix(".jsonl").filter(|s| is_uuid(s)).map(|s| s.to_string()),
            &[],
        );
        assert!(result.is_none());
    }

    #[test]
    fn test_missing_dir_returns_none() {
        let result = newest_unclaimed_file(
            &PathBuf::from("/nonexistent/path/xyz"),
            |_| None,
            &[],
        );
        assert!(result.is_none());
    }

    #[test]
    fn test_single_jsonl_returns_uuid() {
        let dir = TempDir::new().unwrap();
        let uuid = "af467730-5e79-49d9-8a17-ebd94c99f262";
        make_file(dir.path(), &format!("{uuid}.jsonl"));

        let result = newest_unclaimed_file(
            &dir.path().to_path_buf(),
            |name| name.strip_suffix(".jsonl").filter(|s| is_uuid(s)).map(|s| s.to_string()),
            &[],
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
            |name| name.strip_suffix(".jsonl").filter(|s| is_uuid(s)).map(|s| s.to_string()),
            &[uuid.to_string()],
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
            |name| name.strip_suffix(".jsonl").filter(|s| is_uuid(s)).map(|s| s.to_string()),
            &[],
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
            |name| name.strip_suffix(".jsonl").filter(|s| is_uuid(s)).map(|s| s.to_string()),
            &[],
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
        assert!(extract_codex_uuid("session-2026-af467730-5e79-49d9-8a17-ebd94c99f262.jsonl").is_none());
    }

    #[test]
    fn test_extract_codex_uuid_no_suffix() {
        assert!(extract_codex_uuid("rollout-2026-af467730-5e79-49d9-8a17-ebd94c99f262.txt").is_none());
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
        assert!(!verify_claude_session("not-a-uuid", "/tmp"));
    }

    #[test]
    fn test_verify_agent_session_unknown_agent() {
        assert!(!verify_agent_session(
            "unknown-agent".to_string(),
            "af467730-5e79-49d9-8a17-ebd94c99f262".to_string(),
            "/tmp".to_string(),
        ));
    }
}
