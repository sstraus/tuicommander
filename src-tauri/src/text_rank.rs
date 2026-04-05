//! Generic BM25 line ranker for ad-hoc corpora (search result rerank, terminal
//! buffer scoring, etc.). Unlike [`tool_search::ToolSearchIndex`], this builds
//! a one-shot index per call and does not persist state — use it for small,
//! short-lived corpora (≤ a few thousand lines). For long-lived corpora with
//! repeated queries, wrap a persistent `bm25::SearchEngine` instead.

use bm25::{Language, SearchEngineBuilder, SearchResult};

/// Score every line in `lines` against `query` and return `(index, score)`
/// sorted by descending relevance. Lines with zero score are omitted.
///
/// The returned indices refer back into the caller's `lines` slice so the
/// caller can preserve any surrounding metadata (file paths, line numbers).
pub fn rank_lines(query: &str, lines: &[&str]) -> Vec<(usize, f32)> {
    if query.trim().is_empty() || lines.is_empty() {
        return Vec::new();
    }

    // The bm25 crate panics on empty-string documents (rust-stemmers can't
    // handle a zero-length input). Replace them with a single space so the
    // index ids stay aligned with the caller's slice indices.
    let corpus: Vec<String> = lines
        .iter()
        .map(|l| if l.is_empty() { " ".to_string() } else { l.to_string() })
        .collect();

    let engine =
        SearchEngineBuilder::<u32>::with_corpus(Language::English, corpus).build();

    let results: Vec<SearchResult<u32>> = engine.search(query, lines.len());
    results
        .into_iter()
        .map(|r| (r.document.id as usize, r.score))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_query_returns_nothing() {
        let lines = vec!["hello world", "foo bar"];
        assert!(rank_lines("", &lines).is_empty());
        assert!(rank_lines("   ", &lines).is_empty());
    }

    #[test]
    fn empty_corpus_returns_nothing() {
        assert!(rank_lines("query", &[]).is_empty());
    }

    #[test]
    fn ranks_more_relevant_line_higher() {
        let lines = vec![
            "the quick brown fox",
            "fox fox fox runs fast",
            "nothing relevant here",
        ];
        let ranked = rank_lines("fox", &lines);
        assert!(!ranked.is_empty());
        // Line 1 mentions "fox" three times → should beat line 0 which has it once.
        assert_eq!(ranked[0].0, 1, "expected line index 1 at top, got {ranked:?}");
    }

    #[test]
    fn rarer_term_wins_over_common_term() {
        // BM25 IDF should boost rare terms: "supersecretstring" appears in one
        // line only, "the" appears in every line. A query containing both
        // should rank the line with the rare term first.
        let lines = vec![
            "the cat sat on the mat",
            "the supersecretstring lives here",
            "the dog chased the cat",
        ];
        let ranked = rank_lines("the supersecretstring", &lines);
        assert_eq!(ranked[0].0, 1);
    }

    #[test]
    fn handles_empty_lines_without_panic() {
        let lines = vec!["hello", "", "hello world"];
        let ranked = rank_lines("hello", &lines);
        // Empty line must not cause panic; it simply shouldn't rank for the query.
        assert!(ranked.iter().any(|(i, _)| *i == 0 || *i == 2));
    }

    #[test]
    fn zero_score_lines_are_omitted() {
        let lines = vec!["apple", "banana", "cherry"];
        let ranked = rank_lines("apple", &lines);
        let indices: Vec<usize> = ranked.iter().map(|(i, _)| *i).collect();
        assert!(indices.contains(&0));
        assert!(!indices.contains(&1));
        assert!(!indices.contains(&2));
    }
}
