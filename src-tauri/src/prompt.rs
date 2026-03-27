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
    let mut result = String::with_capacity(content.len());
    let bytes = content.as_bytes();
    let len = bytes.len();
    let mut i = 0;

    while i < len {
        if bytes[i] == b'{' {
            if let Some(end) = content[i + 1..].find('}') {
                let name = &content[i + 1..i + 1 + end];
                if let Some(value) = variables.get(name) {
                    result.push_str(value);
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

const MAX_VARIABLE_LEN: usize = 50_000;

/// Run a git command in the given repo and return trimmed stdout, or None on failure.
fn git_output(repo_path: &str, args: &[&str]) -> Option<String> {
    let output = Command::new("git")
        .arg("-C")
        .arg(repo_path)
        .args(args)
        .output()
        .ok()?;
    if output.status.success() {
        let s = String::from_utf8_lossy(&output.stdout).trim().to_string();
        if s.is_empty() {
            None
        } else {
            Some(s)
        }
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
/// Best-effort: variables that fail to resolve are simply omitted from the map.
#[tauri::command]
pub(crate) fn resolve_context_variables(repo_path: String) -> HashMap<String, String> {
    let mut vars = HashMap::new();

    if let Some(v) = git_output(&repo_path, &["rev-parse", "--abbrev-ref", "HEAD"]) {
        vars.insert("branch".to_string(), v);
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
    if let Some(v) = git_output(&repo_path, &["diff"]) {
        vars.insert("diff".to_string(), truncate(v, MAX_VARIABLE_LEN));
    }
    if let Some(v) = git_output(&repo_path, &["diff", "--staged"]) {
        vars.insert("staged_diff".to_string(), truncate(v, MAX_VARIABLE_LEN));
    }
    if let Some(v) = git_output(&repo_path, &["status", "--short"]) {
        vars.insert("changed_files".to_string(), v);
    }
    if let Some(v) = git_output(&repo_path, &["log", "--oneline", "-20"]) {
        vars.insert("commit_log".to_string(), v);
    }
    if let Some(v) = git_output(&repo_path, &["log", "-1", "--format=%H %s"]) {
        vars.insert("last_commit".to_string(), v);
    }
    if let Some(v) = git_output(&repo_path, &["diff", "--name-only", "--diff-filter=U"]) {
        vars.insert("conflict_files".to_string(), v);
    }
    if let Some(v) = git_output(&repo_path, &["stash", "list"]) {
        vars.insert("stash_list".to_string(), v);
    }

    vars
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

    #[test]
    fn resolve_context_variables_non_git_path() {
        let vars = resolve_context_variables("/tmp".to_string());
        // Should return empty or near-empty map, no panic
        assert!(
            vars.get("branch").is_none() || !vars.get("branch").unwrap().is_empty()
        );
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
