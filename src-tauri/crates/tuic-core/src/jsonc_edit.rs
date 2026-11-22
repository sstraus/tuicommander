//! Surgical edits to third-party JSONC configuration files.
//!
//! Every JSON target we install the MCP bridge into is a file the *user* owns —
//! and for Zed and Amp it is the editor's entire settings document, not
//! a dedicated `mcp.json`. Round-tripping such a file through
//! `serde_json::Value` destroys it in three ways at once: comments and trailing
//! commas fail to parse at all, `serde_json::Map` is a `BTreeMap` so surviving
//! keys come back alphabetised, and `to_string_pretty` discards the user's
//! indentation. Issue #115 is the worst case of that class — a 400-line Zed
//! `settings.json` reduced to our single entry.
//!
//! So we never reserialize. The document is parsed into a concrete syntax tree
//! that keeps every byte of trivia, exactly one member is spliced, and the tree
//! is printed back. Text outside the edited member is unchanged.

use jsonc_parser::ParseOptions;
use jsonc_parser::cst::{CstInputValue, CstObject, CstRootNode};

/// Parse options for a third-party config: tolerate what the owning tool
/// tolerates. Zed, VS Code and opencode all document comments and trailing
/// commas as supported, so rejecting them would mean refusing to configure a
/// perfectly valid file.
fn parse_options() -> ParseOptions {
    ParseOptions {
        allow_comments: true,
        allow_trailing_commas: true,
        // The remaining extensions are not part of any target's documented
        // format. Accepting them would let us "successfully" parse a file the
        // tool itself cannot read, and then write it back as if it were fine.
        allow_loose_object_property_names: false,
        allow_missing_commas: false,
        allow_single_quoted_strings: false,
        allow_hexadecimal_numbers: false,
        allow_unary_plus_numbers: false,
    }
}

/// Parse JSONC text into a `serde_json::Value` for inspection.
///
/// Read-only callers (status checks, "is our entry current?") use this instead
/// of `serde_json::from_str`, which rejects the comments these files legally
/// contain.
pub fn parse(text: &str) -> Result<serde_json::Value, String> {
    let parsed: Option<serde_json::Value> =
        jsonc_parser::parse_to_serde_value(text, &parse_options())
            .map_err(|e| format!("JSONC parse error: {e}"))?;
    Ok(parsed.unwrap_or(serde_json::Value::Null))
}

/// Walk `key_path`, creating missing objects, and return the object that should
/// hold our member.
///
/// Uses the `_or_create` variants throughout: a path segment that exists but
/// holds a non-object (a string, an array, `null`) is an error, never something
/// we overwrite. The user put that value there and we do not know what it means.
fn navigate_or_create(root: &CstRootNode, key_path: &[&str]) -> Result<CstObject, String> {
    let mut current = root
        .object_value_or_create()
        .ok_or_else(|| "root value is not an object".to_string())?;
    for key in key_path {
        current = current
            .object_value_or_create(key)
            .ok_or_else(|| format!("`{key}` exists but is not an object"))?;
    }
    Ok(current)
}

/// Walk `key_path` without creating anything.
fn navigate(root: &CstRootNode, key_path: &[&str]) -> Option<CstObject> {
    let mut current = root.object_value()?;
    for key in key_path {
        current = current.object_value(key)?;
    }
    Some(current)
}

/// Translate a `serde_json::Value` into the CST's input shape.
///
/// Object key order is the one the caller built, not `serde_json::Map`'s: our
/// entries are constructed with `serde_json::json!`, whose `Map` is a `BTreeMap`
/// unless `preserve_order` is on. That only affects the four keys of the entry
/// we are inserting, never the surrounding document.
fn to_input(value: &serde_json::Value) -> CstInputValue {
    match value {
        serde_json::Value::Null => CstInputValue::Null,
        serde_json::Value::Bool(b) => CstInputValue::Bool(*b),
        serde_json::Value::Number(n) => CstInputValue::Number(n.to_string()),
        serde_json::Value::String(s) => CstInputValue::String(s.clone()),
        serde_json::Value::Array(items) => {
            CstInputValue::Array(items.iter().map(to_input).collect())
        }
        serde_json::Value::Object(members) => CstInputValue::Object(
            members
                .iter()
                .map(|(key, value)| (key.clone(), to_input(value)))
                .collect(),
        ),
    }
}

/// Insert or replace `<key_path>/<key>` with `value`, leaving the rest of the
/// document byte-identical.
///
/// The returned text is re-parsed before it is handed back, so a bug in the
/// splice surfaces here rather than in the user's config file.
pub fn upsert_member(
    text: &str,
    key_path: &[&str],
    key: &str,
    value: &serde_json::Value,
) -> Result<String, String> {
    let root = CstRootNode::parse(text, &parse_options())
        .map_err(|e| format!("JSONC parse error: {e}"))?;
    let parent = navigate_or_create(&root, key_path)?;
    match parent.get(key) {
        Some(prop) => prop.set_value(to_input(value)),
        None => {
            parent.append(key, to_input(value));
        }
    }
    let output = root.to_string();
    validate(&output, key_path, key, Some(value))?;
    Ok(output)
}

/// Remove `<key_path>/<key>`, leaving the rest of the document byte-identical.
/// A member that is not there is not an error — the end state is what matters.
pub fn remove_member(text: &str, key_path: &[&str], key: &str) -> Result<String, String> {
    let root = CstRootNode::parse(text, &parse_options())
        .map_err(|e| format!("JSONC parse error: {e}"))?;
    let Some(parent) = navigate(&root, key_path) else {
        return Ok(text.to_string());
    };
    let Some(prop) = parent.get(key) else {
        return Ok(text.to_string());
    };
    prop.remove();
    let output = root.to_string();
    validate(&output, key_path, key, None)?;
    Ok(output)
}

/// Prove the edited document still parses and says what we meant it to say.
///
/// This is the last gate before a write touches a file we do not own, so it
/// checks the outcome rather than trusting the edit: parse the printed text
/// back and compare the member against what was requested.
fn validate(
    output: &str,
    key_path: &[&str],
    key: &str,
    expected: Option<&serde_json::Value>,
) -> Result<(), String> {
    let reparsed = parse(output)?;
    let mut current = &reparsed;
    for segment in key_path {
        current = current
            .get(*segment)
            .ok_or_else(|| format!("edited document lost `{segment}`"))?;
    }
    match (current.get(key), expected) {
        (Some(actual), Some(expected)) if actual == expected => Ok(()),
        (Some(_), Some(_)) => Err(format!("edited document holds the wrong `{key}`")),
        (None, Some(_)) => Err(format!("edited document is missing `{key}`")),
        (None, None) => Ok(()),
        (Some(_), None) => Err(format!("edited document still holds `{key}`")),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn entry() -> serde_json::Value {
        serde_json::json!({
            "type": "stdio",
            "command": "/usr/bin/tuic-bridge",
            "args": [],
            "env": {},
        })
    }

    /// The exact shape of issue #115: a JSONC Zed settings file whose comments
    /// and trailing commas made `serde_json` bail, after which the old writer
    /// serialized an empty document over it.
    #[test]
    fn zed_jsonc_settings_keep_every_unrelated_line() {
        let original = r#"{
  // Font settings I spent an hour on
  "buffer_font_family": "Berkeley Mono",
  "buffer_font_size": 14,
  /* block comment */
  "theme": {
    "mode": "system",
    "dark": "One Dark",
  },
  "languages": {
    "Rust": { "tab_size": 4 },
  },
}"#;
        let edited = upsert_member(original, &["context_servers"], "tuicommander", &entry())
            .expect("edit succeeds");

        for line in [
            "// Font settings I spent an hour on",
            "\"buffer_font_family\": \"Berkeley Mono\"",
            "/* block comment */",
            "\"dark\": \"One Dark\",",
            "\"Rust\": { \"tab_size\": 4 },",
        ] {
            assert!(edited.contains(line), "lost `{line}` in:\n{edited}");
        }

        let value = parse(&edited).unwrap();
        assert_eq!(
            value["context_servers"]["tuicommander"]["command"],
            "/usr/bin/tuic-bridge"
        );
        assert_eq!(value["buffer_font_size"], 14);
        assert_eq!(value["theme"]["dark"], "One Dark");
    }

    #[test]
    fn existing_entry_is_replaced_not_duplicated() {
        let original = r#"{
  "context_servers": {
    "other": { "command": "/opt/other" },
    "tuicommander": { "type": "stdio", "command": "/old/path", "args": [], "env": {} }
  }
}"#;
        let edited =
            upsert_member(original, &["context_servers"], "tuicommander", &entry()).unwrap();
        let value = parse(&edited).unwrap();
        assert_eq!(
            value["context_servers"]["tuicommander"]["command"],
            "/usr/bin/tuic-bridge"
        );
        assert_eq!(value["context_servers"]["other"]["command"], "/opt/other");
        assert_eq!(edited.matches("\"tuicommander\"").count(), 1);
    }

    #[test]
    fn indentation_of_untouched_lines_survives() {
        let original = "{\n\t\"theme\": \"dark\"\n}";
        let edited = upsert_member(original, &[], "tuicommander", &entry()).unwrap();
        assert!(edited.contains("\t\"theme\": \"dark\""), "{edited}");
    }

    #[test]
    fn nested_key_path_is_created_when_missing() {
        let original = r#"{ "amp": { "other": true } }"#;
        let edited =
            upsert_member(original, &["amp", "mcpServers"], "tuicommander", &entry()).unwrap();
        let value = parse(&edited).unwrap();
        assert_eq!(value["amp"]["other"], true);
        assert_eq!(
            value["amp"]["mcpServers"]["tuicommander"]["command"],
            "/usr/bin/tuic-bridge"
        );
    }

    #[test]
    fn a_path_segment_holding_a_non_object_is_refused() {
        let original = r#"{ "context_servers": "disabled" }"#;
        let error = upsert_member(original, &["context_servers"], "tuicommander", &entry())
            .expect_err("must refuse");
        assert!(error.contains("not an object"), "{error}");
    }

    #[test]
    fn a_non_object_root_is_refused() {
        let error =
            upsert_member("[1, 2, 3]", &[], "tuicommander", &entry()).expect_err("must refuse");
        assert!(error.contains("root value is not an object"), "{error}");
    }

    /// Single quotes are not part of any target's documented format, so a file
    /// using them is one we do not understand — refusing beats guessing.
    #[test]
    fn undocumented_json_extensions_are_refused() {
        let error = upsert_member("{ 'theme': 'dark' }", &[], "tuicommander", &entry())
            .expect_err("must refuse");
        assert!(error.contains("parse error"), "{error}");
    }

    #[test]
    fn removal_keeps_siblings_and_comments() {
        let original = r#"{
  // keep me
  "theme": "dark",
  "context_servers": {
    "other": { "command": "/opt/other" },
    "tuicommander": { "command": "/usr/bin/tuic-bridge" }
  }
}"#;
        let edited = remove_member(original, &["context_servers"], "tuicommander").unwrap();
        assert!(edited.contains("// keep me"), "{edited}");
        let value = parse(&edited).unwrap();
        assert!(value["context_servers"].get("tuicommander").is_none());
        assert_eq!(value["context_servers"]["other"]["command"], "/opt/other");
        assert_eq!(value["theme"], "dark");
    }

    #[test]
    fn removing_an_absent_member_returns_the_input_unchanged() {
        let original = "{\n  \"theme\": \"dark\"\n}";
        let edited = remove_member(original, &["context_servers"], "tuicommander").unwrap();
        assert_eq!(edited, original);
    }

    #[test]
    fn an_empty_document_becomes_a_single_entry_object() {
        let edited = upsert_member("", &["mcpServers"], "tuicommander", &entry()).unwrap();
        let value = parse(&edited).unwrap();
        assert_eq!(
            value["mcpServers"]["tuicommander"]["command"],
            "/usr/bin/tuic-bridge"
        );
    }

    #[test]
    fn parse_reads_jsonc_that_serde_json_rejects() {
        let text = r#"{
  // comment
  "a": 1,
}"#;
        assert!(serde_json::from_str::<serde_json::Value>(text).is_err());
        assert_eq!(parse(text).unwrap()["a"], 1);
    }
}
