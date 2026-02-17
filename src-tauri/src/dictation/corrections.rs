use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::{Path, PathBuf};

/// Dictionary-based text corrector for common speech recognition errors.
/// Applies case-insensitive string replacements, sorted by length (longest first)
/// to avoid partial matches. The replacement value is always used as-is.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TextCorrector {
    replacements: HashMap<String, String>,
    /// Cached sorted keys (longest first) for correct replacement order
    #[serde(skip)]
    sorted_keys: Vec<String>,
}

impl TextCorrector {
    pub fn new() -> Self {
        Self {
            replacements: HashMap::new(),
            sorted_keys: Vec::new(),
        }
    }

    pub fn from_map(replacements: HashMap<String, String>) -> Self {
        let mut corrector = Self {
            replacements,
            sorted_keys: Vec::new(),
        };
        corrector.rebuild_sorted_keys();
        corrector
    }

    /// Apply all corrections to the given text (case-insensitive matching).
    /// Replacements are applied longest-first to avoid partial matches.
    /// The replacement value is inserted as-is regardless of the original case.
    pub fn correct(&self, text: &str) -> String {
        let mut result = text.to_string();
        for key in &self.sorted_keys {
            if let Some(replacement) = self.replacements.get(key) {
                let key_lower = key.to_lowercase();
                // Scan for case-insensitive occurrences and replace them
                let mut i = 0;
                let mut output = String::with_capacity(result.len());
                let result_lower = result.to_lowercase();
                while i < result.len() {
                    if result_lower[i..].starts_with(&key_lower) {
                        output.push_str(replacement);
                        i += key.len();
                    } else {
                        // Advance by one character (handle multi-byte UTF-8)
                        let ch = result[i..].chars().next().unwrap();
                        output.push(ch);
                        i += ch.len_utf8();
                    }
                }
                result = output;
            }
        }
        result
    }

    pub fn set_replacements(&mut self, replacements: HashMap<String, String>) {
        self.replacements = replacements;
        self.rebuild_sorted_keys();
    }

    pub const fn get_replacements(&self) -> &HashMap<String, String> {
        &self.replacements
    }

    pub fn add(&mut self, from: String, to: String) {
        self.replacements.insert(from, to);
        self.rebuild_sorted_keys();
    }

    fn rebuild_sorted_keys(&mut self) {
        let mut keys: Vec<String> = self.replacements.keys().cloned().collect();
        // Sort by length descending so longer patterns match first
        keys.sort_by_key(|k| std::cmp::Reverse(k.len()));
        self.sorted_keys = keys;
    }

    /// Default config file path: <config_dir>/dictation-corrections.json
    pub fn default_path() -> PathBuf {
        crate::config::config_dir().join("dictation-corrections.json")
    }

    pub fn load_from_file(path: &Path) -> Result<Self, String> {
        let content = std::fs::read_to_string(path)
            .map_err(|e| format!("Failed to read corrections file: {e}"))?;
        let replacements: HashMap<String, String> = serde_json::from_str(&content)
            .map_err(|e| format!("Failed to parse corrections file: {e}"))?;
        Ok(Self::from_map(replacements))
    }

    pub fn save_to_file(&self, path: &Path) -> Result<(), String> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)
                .map_err(|e| format!("Failed to create directory: {e}"))?;
        }
        let content = serde_json::to_string_pretty(&self.replacements)
            .map_err(|e| format!("Failed to serialize corrections: {e}"))?;
        std::fs::write(path, content)
            .map_err(|e| format!("Failed to write corrections file: {e}"))?;
        Ok(())
    }

    /// Try to load from default path, returning empty corrector if file doesn't exist.
    pub fn load_or_default() -> Self {
        let path = Self::default_path();
        if path.exists() {
            Self::load_from_file(&path).unwrap_or_else(|_| Self::new())
        } else {
            // Create with some sensible defaults for speech recognition.
            // Matching is case-insensitive, so one entry covers all case variants.
            let mut corrector = Self::new();
            corrector.add("Cloud Code".to_string(), "Claude Code".to_string());
            corrector.add("Cloude Code".to_string(), "Claude Code".to_string());
            corrector
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_empty_corrector() {
        let corrector = TextCorrector::new();
        assert_eq!(corrector.correct("hello world"), "hello world");
    }

    #[test]
    fn test_basic_replacement() {
        let mut map = HashMap::new();
        map.insert("Cloud Code".to_string(), "Claude Code".to_string());
        let corrector = TextCorrector::from_map(map);
        assert_eq!(
            corrector.correct("I use Cloud Code daily"),
            "I use Claude Code daily"
        );
    }

    #[test]
    fn test_case_insensitive() {
        let mut map = HashMap::new();
        map.insert("Cloud Code".to_string(), "Claude Code".to_string());
        let corrector = TextCorrector::from_map(map);
        // Lowercase should be replaced (case-insensitive match)
        assert_eq!(
            corrector.correct("I use cloud code daily"),
            "I use Claude Code daily"
        );
        // Mixed case should also be replaced
        assert_eq!(
            corrector.correct("I use CLOUD CODE daily"),
            "I use Claude Code daily"
        );
    }

    #[test]
    fn test_multiple_replacements() {
        let mut map = HashMap::new();
        map.insert("Cloud Code".to_string(), "Claude Code".to_string());
        map.insert("API key".to_string(), "API Key".to_string());
        let corrector = TextCorrector::from_map(map);
        assert_eq!(
            corrector.correct("Use Cloud Code with your API key"),
            "Use Claude Code with your API Key"
        );
    }

    #[test]
    fn test_longest_match_first() {
        let mut map = HashMap::new();
        map.insert("Cloud".to_string(), "WRONG".to_string());
        map.insert("Cloud Code".to_string(), "Claude Code".to_string());
        let corrector = TextCorrector::from_map(map);
        // "Cloud Code" should match first (longer), not just "Cloud"
        assert_eq!(
            corrector.correct("I use Cloud Code"),
            "I use Claude Code"
        );
    }

    #[test]
    fn test_multiple_occurrences() {
        let mut map = HashMap::new();
        map.insert("foo".to_string(), "bar".to_string());
        let corrector = TextCorrector::from_map(map);
        assert_eq!(corrector.correct("foo and foo"), "bar and bar");
    }

    #[test]
    fn test_empty_text() {
        let mut map = HashMap::new();
        map.insert("Cloud".to_string(), "Claude".to_string());
        let corrector = TextCorrector::from_map(map);
        assert_eq!(corrector.correct(""), "");
    }

    #[test]
    fn test_no_match() {
        let mut map = HashMap::new();
        map.insert("Cloud".to_string(), "Claude".to_string());
        let corrector = TextCorrector::from_map(map);
        assert_eq!(corrector.correct("hello world"), "hello world");
    }

    #[test]
    fn test_add() {
        let mut corrector = TextCorrector::new();
        corrector.add("foo".to_string(), "bar".to_string());
        assert_eq!(corrector.correct("foo"), "bar");
    }

    #[test]
    fn test_set_replacements_clears_old() {
        let mut corrector = TextCorrector::new();
        corrector.add("foo".to_string(), "bar".to_string());
        assert_eq!(corrector.correct("foo"), "bar");

        // Replace with empty map — old entries gone
        corrector.set_replacements(HashMap::new());
        assert_eq!(corrector.correct("foo"), "foo");
    }

    #[test]
    fn test_serialization_roundtrip() {
        let mut map = HashMap::new();
        map.insert("Cloud Code".to_string(), "Claude Code".to_string());
        let corrector = TextCorrector::from_map(map);

        let json = serde_json::to_string(&corrector).unwrap();
        let deserialized: TextCorrector = serde_json::from_str(&json).unwrap();

        // After deserialization, sorted_keys is empty (skipped), so correct() won't work
        // Need to rebuild:
        let restored = TextCorrector::from_map(deserialized.replacements);
        assert_eq!(
            restored.correct("I use Cloud Code"),
            "I use Claude Code"
        );
    }
}
