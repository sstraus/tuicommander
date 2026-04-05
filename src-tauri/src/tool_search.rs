//! BM25-powered search index for MCP tool definitions.
//!
//! Shared infrastructure used by:
//! - MCP tool collapsing (Speakeasy pattern: search_tools / get_tool_schema / call_tool)
//! - Command palette relevance ranking
//! - File search (future)

use bm25::{Language, SearchEngineBuilder, SearchResult};
use serde_json::Value;

/// A single indexed tool entry.
#[derive(Debug, Clone)]
pub struct ToolEntry {
    /// Tool name (e.g., "session", "okm__get_claudecode_overview")
    pub name: String,
    /// One-line description (first sentence)
    pub summary: String,
    /// Full tool definition (name + description + inputSchema)
    pub definition: Value,
}

/// BM25 search index over MCP tool definitions.
pub struct ToolSearchIndex {
    engine: bm25::SearchEngine<u32>,
    entries: Vec<ToolEntry>,
}

/// Extract the first sentence from a description string.
fn first_sentence(desc: &str) -> String {
    // Split on ". " or "\n" to get the first sentence
    let end = desc
        .find(". ")
        .map(|i| i + 1)
        .or_else(|| desc.find('\n'))
        .unwrap_or(desc.len());
    desc[..end].trim().to_string()
}

impl ToolSearchIndex {
    /// Build a new index from an array of MCP tool definition objects.
    ///
    /// Each object is expected to have `name` (string) and `description` (string) fields.
    pub fn build(tools: &[Value]) -> Self {
        let mut entries = Vec::with_capacity(tools.len());
        let mut corpus = Vec::with_capacity(tools.len());

        for tool in tools {
            let name = tool["name"].as_str().unwrap_or("").to_string();
            let desc = tool["description"].as_str().unwrap_or("").to_string();
            let summary = first_sentence(&desc);

            // BM25 corpus entry: combine name and description for searchability
            corpus.push(format!("{} {}", name, desc));

            entries.push(ToolEntry {
                name,
                summary,
                definition: tool.clone(),
            });
        }

        let engine = SearchEngineBuilder::<u32>::with_corpus(Language::English, corpus).build();

        Self { engine, entries }
    }

    /// Search for tools matching a query string, returning up to `limit` results
    /// ranked by BM25 relevance score.
    pub fn search(&self, query: &str, limit: usize) -> Vec<&ToolEntry> {
        let results: Vec<SearchResult<u32>> = self.engine.search(query, limit);
        results
            .into_iter()
            .filter_map(|r| self.entries.get(r.document.id as usize))
            .collect()
    }

    /// Get the full tool definition by exact name match.
    pub fn get_schema(&self, name: &str) -> Option<&Value> {
        self.entries
            .iter()
            .find(|e| e.name == name)
            .map(|e| &e.definition)
    }

    /// Get all indexed tool entries (for help/listing).
    pub fn entries(&self) -> &[ToolEntry] {
        &self.entries
    }

    /// Number of indexed tools.
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    /// Whether the index is empty.
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn sample_tools() -> Vec<Value> {
        vec![
            json!({
                "name": "session",
                "description": "Manage PTY terminal sessions.\n\nActions: list, create, input, output, resize, close, kill, pause, resume.",
                "inputSchema": { "type": "object", "properties": { "action": { "type": "string" } }, "required": ["action"] }
            }),
            json!({
                "name": "github",
                "description": "Query GitHub integration: PR statuses, CI rollup, merge readiness.",
                "inputSchema": { "type": "object", "properties": { "action": { "type": "string" } }, "required": ["action"] }
            }),
            json!({
                "name": "worktree",
                "description": "Manage git worktrees for parallel work. Actions: list, create, remove.",
                "inputSchema": { "type": "object", "properties": { "action": { "type": "string" } }, "required": ["action"] }
            }),
            json!({
                "name": "agent",
                "description": "Detect and manage AI agents. Actions: detect, spawn, stats, metrics.",
                "inputSchema": { "type": "object", "properties": { "action": { "type": "string" } }, "required": ["action"] }
            }),
            json!({
                "name": "notify",
                "description": "Show notifications to the TUIC user. Actions: toast, confirm.",
                "inputSchema": { "type": "object", "properties": { "action": { "type": "string" } }, "required": ["action"] }
            }),
            json!({
                "name": "config",
                "description": "Read or write app configuration. Actions: get, save.",
                "inputSchema": { "type": "object", "properties": { "action": { "type": "string" } }, "required": ["action"] }
            }),
            json!({
                "name": "knowledge",
                "description": "Cross-repo knowledge base powered by mdkb. Search docs, code, symbols, and call graphs.",
                "inputSchema": { "type": "object", "properties": { "action": { "type": "string" } }, "required": ["action"] }
            }),
            json!({
                "name": "plugin_dev_guide",
                "description": "Returns comprehensive plugin authoring reference: manifest format, PluginHost API, structured event types, and working examples.",
                "inputSchema": { "type": "object", "properties": {}, "required": [] }
            }),
            json!({
                "name": "debug",
                "description": "Dev-only diagnostics for debugging TUICommander internals.",
                "inputSchema": { "type": "object", "properties": { "action": { "type": "string" } }, "required": ["action"] }
            }),
            json!({
                "name": "okm__get_claudecode_overview",
                "description": "[via okm] Analytics dashboard for ClaudeCode usage, costs, and trends.",
                "inputSchema": { "type": "object", "properties": { "start_date": { "type": "string" }, "end_date": { "type": "string" } }, "required": ["start_date", "end_date"] }
            }),
        ]
    }

    #[test]
    fn search_terminal_session_returns_session_first() {
        let index = ToolSearchIndex::build(&sample_tools());
        let results = index.search("terminal session", 3);
        assert!(!results.is_empty(), "Expected at least one result");
        assert_eq!(results[0].name, "session");
    }

    #[test]
    fn search_github_pr_returns_github_first() {
        let index = ToolSearchIndex::build(&sample_tools());
        let results = index.search("github PR status", 3);
        assert!(!results.is_empty());
        assert_eq!(results[0].name, "github");
    }

    #[test]
    fn search_analytics_returns_okm() {
        let index = ToolSearchIndex::build(&sample_tools());
        let results = index.search("analytics costs", 3);
        assert!(!results.is_empty());
        assert_eq!(results[0].name, "okm__get_claudecode_overview");
    }

    #[test]
    fn search_plugin_authoring_returns_plugin_dev_guide() {
        let index = ToolSearchIndex::build(&sample_tools());
        let results = index.search("plugin authoring", 3);
        assert!(!results.is_empty());
        assert_eq!(results[0].name, "plugin_dev_guide");
    }

    #[test]
    fn search_unknown_returns_empty() {
        let index = ToolSearchIndex::build(&sample_tools());
        let results = index.search("unknown xyz123 zzzzz", 3);
        assert!(results.is_empty(), "Expected no results for nonsense query");
    }

    #[test]
    fn get_schema_existing_returns_definition() {
        let index = ToolSearchIndex::build(&sample_tools());
        let schema = index.get_schema("session");
        assert!(schema.is_some());
        let def = schema.unwrap();
        assert_eq!(def["name"], "session");
        assert!(def["inputSchema"].is_object());
    }

    #[test]
    fn get_schema_nonexistent_returns_none() {
        let index = ToolSearchIndex::build(&sample_tools());
        assert!(index.get_schema("nonexistent").is_none());
    }

    #[test]
    fn build_empty_corpus() {
        let index = ToolSearchIndex::build(&[]);
        assert!(index.is_empty());
        assert_eq!(index.len(), 0);
        assert!(index.search("anything", 5).is_empty());
    }

    #[test]
    fn entries_returns_all() {
        let tools = sample_tools();
        let index = ToolSearchIndex::build(&tools);
        assert_eq!(index.len(), tools.len());
        assert_eq!(index.entries().len(), tools.len());
    }

    #[test]
    fn first_sentence_extracts_correctly() {
        assert_eq!(
            first_sentence("Manage PTY terminal sessions.\n\nActions: list, create."),
            "Manage PTY terminal sessions."
        );
        assert_eq!(
            first_sentence("Query GitHub integration: PR statuses, CI rollup, merge readiness."),
            "Query GitHub integration: PR statuses, CI rollup, merge readiness."
        );
        assert_eq!(
            first_sentence("Short desc. More details here."),
            "Short desc."
        );
    }
}
