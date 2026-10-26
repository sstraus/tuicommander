//! BM25-powered search index for MCP tool definitions.
//!
//! Shared infrastructure used by:
//! - MCP tool collapsing (Speakeasy pattern: search_tools / get_tool_schema / call_tool)
//! - Command palette relevance ranking
//! - File search (future)

use bm25::{Language, SearchEngineBuilder, SearchResult};
use serde_json::Value;
use sha2::{Digest, Sha256};

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
    generation: String,
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

        Self {
            engine,
            entries,
            generation: Self::catalogue_generation(tools),
        }
    }

    /// Opaque content identity. Rebuilding an unchanged catalogue preserves admissions.
    pub fn catalogue_generation(tools: &[Value]) -> String {
        hex::encode(Sha256::digest(
            serde_json::to_vec(tools).expect("JSON tool definitions serialize"),
        ))
    }

    pub fn generation(&self) -> &str {
        &self.generation
    }

    /// Rank plain words with BM25; require every `+term` in the tool name.
    /// `select:<name>` performs exact selection without ranked fallback.
    pub fn search(&self, query: &str, limit: usize) -> Vec<&ToolEntry> {
        if limit == 0 {
            return Vec::new();
        }
        let query = query.trim();
        if let Some(name) = query.strip_prefix("select:") {
            return self
                .entries
                .iter()
                .find(|e| e.name == name)
                .into_iter()
                .collect();
        }

        let mut required = Vec::new();
        let mut optional = Vec::new();
        for word in query.split_whitespace() {
            match word.strip_prefix('+').filter(|term| !term.is_empty()) {
                Some(term) => required.push(term.to_lowercase()),
                None => optional.push(word),
            }
        }
        if required.is_empty() {
            let results: Vec<SearchResult<u32>> = self.engine.search(query, limit);
            return results
                .into_iter()
                .filter_map(|r| self.entries.get(r.document.id as usize))
                .collect();
        }

        let matches: Vec<bool> = self
            .entries
            .iter()
            .map(|entry| {
                let name = entry.name.to_lowercase();
                required.iter().all(|term| name.contains(term))
            })
            .collect();
        let rank_query = if optional.is_empty() {
            required.join(" ")
        } else {
            optional.join(" ")
        };
        // Search the whole corpus before filtering: a description-only hit
        // must not consume the requested limit and hide a valid name match.
        let ranked = self.engine.search(&rank_query, self.entries.len());
        let mut seen = vec![false; self.entries.len()];
        let mut results = Vec::new();
        for hit in ranked {
            let id = hit.document.id as usize;
            if matches[id] {
                seen[id] = true;
                results.push(&self.entries[id]);
                if results.len() == limit {
                    return results;
                }
            }
        }
        // Plain words are optional. Name matches without a BM25 token hit
        // (including alias substrings) remain eligible in catalogue order.
        results.extend(
            self.entries
                .iter()
                .enumerate()
                .filter(|(id, _)| matches[*id] && !seen[*id])
                .map(|(_, entry)| entry)
                .take(limit - results.len()),
        );
        results
    }

    /// Get the full tool definition by exact name match.
    pub fn get_schema(&self, name: &str) -> Option<&Value> {
        self.entries
            .iter()
            .find(|e| e.name == name)
            .map(|e| &e.definition)
    }

    /// Get all indexed tool entries (for help/listing).
    #[cfg(test)]
    pub fn entries(&self) -> &[ToolEntry] {
        &self.entries
    }

    /// Number of indexed tools.
    #[cfg(test)]
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    /// Whether the index is empty.
    #[cfg(test)]
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
                "name": "repo",
                "description": "Repository and version control operations. Query workspace repos, GitHub PR/CI status, and manage git worktrees for parallel work.\n\nActions: list, active, prs, status, worktree_list, worktree_create, worktree_remove.",
                "inputSchema": { "type": "object", "properties": { "action": { "type": "string" } }, "required": ["action"] }
            }),
            json!({
                "name": "agent",
                "description": "AI agent orchestration and inter-agent messaging. Spawn agents, detect running agents, and coordinate via message passing.\n\nActions: spawn, detect, stats, metrics, register, list_peers, send, inbox.",
                "inputSchema": { "type": "object", "properties": { "action": { "type": "string" } }, "required": ["action"] }
            }),
            json!({
                "name": "ui",
                "description": "Control the TUICommander UI: open panel tabs, show toast notifications, or prompt for user confirmation.\n\nActions: tab, toast, confirm.",
                "inputSchema": { "type": "object", "properties": { "action": { "type": "string" } }, "required": ["action"] }
            }),
            json!({
                "name": "config",
                "description": "Read or write app configuration. Actions: get, save.",
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
    fn search_github_pr_returns_repo_first() {
        let index = ToolSearchIndex::build(&sample_tools());
        let results = index.search("github PR status", 3);
        assert!(!results.is_empty());
        assert_eq!(results[0].name, "repo");
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

    // Catches OR-ing required terms, ignoring alias prefixes, or treating optional words as required.
    #[test]
    fn required_name_terms_all_match_including_alias_prefixes() {
        let mut tools = sample_tools();
        tools[1]["name"] = json!("tuicommander__repo");
        let index = ToolSearchIndex::build(&tools);
        for query in [
            "+tuicommander +repo",
            "+repo +tuicommander status",
            "+TUICOMMANDER +REPO zzzunknown",
        ] {
            let names: Vec<_> = index
                .search(query, 1)
                .iter()
                .map(|e| e.name.as_str())
                .collect();
            assert_eq!(names, ["tuicommander__repo"], "query: {query}");
        }
        assert!(index.search("+tuicommander +session", 10).is_empty());
    }

    // Catches dropping optional-word ranking after required names have been filtered.
    #[test]
    fn optional_words_rank_required_name_matches_before_limit() {
        let index = ToolSearchIndex::build(&sample_tools());
        let results = index.search("+o analytics costs", 1);
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].name, "okm__get_claudecode_overview");
    }

    // Catches description-only fallback when a required name has no match.
    #[test]
    fn missing_required_name_never_returns_ranked_fallback() {
        let index = ToolSearchIndex::build(&sample_tools());
        assert!(index.search("+nonexistent terminal session", 10).is_empty());
        assert!(index.search("+terminal", 10).is_empty());
        assert!(index.search("+repo", 0).is_empty());
    }

    // Catches interpreting select as BM25 text, including fallback for absent exact names.
    #[test]
    fn select_name_is_exact_not_a_ranked_query() {
        let index = ToolSearchIndex::build(&sample_tools());
        let names: Vec<_> = index
            .search("select:repo", 10)
            .iter()
            .map(|e| e.name.as_str())
            .collect();
        assert_eq!(names, ["repo"]);
        assert!(index.search("select:repos", 10).is_empty());
        assert!(index.search("select:repo", 0).is_empty());
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
