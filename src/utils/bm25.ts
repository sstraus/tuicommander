/**
 * Minimal BM25 ranker for small in-memory corpora (Command Palette, etc.).
 *
 * Corpus size is expected to be in the low hundreds and queries run per
 * keystroke, so the implementation trades configurability for a tiny
 * zero-dependency hot path. Query terms are expanded to vocab terms via
 * prefix match so a palette query like "term" ranks docs containing
 * "terminal". For the MCP tool index backend, see the `bm25` crate wired
 * in `src-tauri/src/tool_search.rs`.
 */

/** Standard BM25 hyperparameters. */
const K1 = 1.2;
const B = 0.75;

/** Split on non-alphanumeric, lowercase, drop empty segments. */
export function tokenize(text: string): string[] {
  return text.toLowerCase().split(/[^a-z0-9]+/).filter(Boolean);
}

export interface Bm25Doc<T> {
  /** The item being ranked — opaque to BM25, returned unchanged in results. */
  item: T;
  /** Searchable text used to build the term-frequency map. */
  text: string;
}

interface PreparedDoc<T> {
  item: T;
  tf: Map<string, number>;
  length: number;
}

/**
 * Pre-compute per-document term frequencies + corpus stats (IDF table,
 * average document length). Call once per corpus and reuse `score()`
 * across many queries.
 */
export function buildIndex<T>(docs: Bm25Doc<T>[]): {
  score: (query: string) => { item: T; score: number }[];
} {
  const prepared: PreparedDoc<T>[] = docs.map((d) => {
    const tokens = tokenize(d.text);
    const tf = new Map<string, number>();
    for (const tok of tokens) {
      tf.set(tok, (tf.get(tok) ?? 0) + 1);
    }
    return { item: d.item, tf, length: tokens.length };
  });

  const docCount = prepared.length;
  const totalLength = prepared.reduce((sum, d) => sum + d.length, 0);
  const avgdl = docCount > 0 ? totalLength / docCount : 0;

  // Document frequency: how many docs contain each term.
  const df = new Map<string, number>();
  for (const doc of prepared) {
    for (const term of doc.tf.keys()) {
      df.set(term, (df.get(term) ?? 0) + 1);
    }
  }

  // Okapi BM25 IDF with +1 offset to keep scores non-negative for common terms.
  const idf = new Map<string, number>();
  for (const [term, freq] of df) {
    idf.set(term, Math.log(1 + (docCount - freq + 0.5) / (freq + 0.5)));
  }

  const vocab = Array.from(df.keys());

  /**
   * Expand a query token to matching vocab terms. Exact match wins; otherwise
   * any vocab term starting with the query token is included (so "term"
   * matches "terminal"). Returns at most one exact match or all prefix hits.
   */
  const expand = (qt: string): string[] => {
    if (df.has(qt)) return [qt];
    return vocab.filter((vt) => vt.startsWith(qt));
  };

  return {
    score(query: string) {
      const terms = tokenize(query);
      if (terms.length === 0) return [];

      const expanded = terms.map(expand);

      const results: { item: T; score: number }[] = [];
      for (const doc of prepared) {
        let score = 0;
        let matchedTerms = 0;
        for (const matches of expanded) {
          if (matches.length === 0) continue;
          // Sum tf across all expanded vocab terms for this doc; use the
          // highest IDF among actual hits so rare-term matches dominate.
          let tf = 0;
          let bestIdf = 0;
          for (const vt of matches) {
            const docTf = doc.tf.get(vt);
            if (docTf) {
              tf += docTf;
              const termIdf = idf.get(vt) ?? 0;
              if (termIdf > bestIdf) bestIdf = termIdf;
            }
          }
          if (tf === 0) continue;
          matchedTerms += 1;
          const denom = tf + K1 * (1 - B + (B * doc.length) / (avgdl || 1));
          score += bestIdf * ((tf * (K1 + 1)) / denom);
        }
        // Require every query term to hit — avoids ambiguous partial matches
        // in a palette context where the user expects all words to count.
        if (matchedTerms === terms.length && score > 0) {
          results.push({ item: doc.item, score });
        }
      }
      results.sort((a, b) => b.score - a.score);
      return results;
    },
  };
}
