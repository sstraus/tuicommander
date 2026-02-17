use std::collections::{HashMap, HashSet};

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
}
