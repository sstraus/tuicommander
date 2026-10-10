use crate::embedder::{DefaultEmbeddingSpace, Embedding};
use std::{
    cmp::Ordering,
    collections::{HashMap, HashSet},
    fmt::Debug,
    hash::Hash,
};

/// A document scored by the BM25 algorithm. K is the type of the document id.
#[derive(PartialEq, Debug)]
pub struct ScoredDocument<K> {
    /// The id of the document.
    pub id: K,
    /// The BM25 score of the document.
    pub score: f32,
}

/// Efficiently scores the relevance of a query embedding to document embeddings using BM25.
/// K is the type of the document id and D is the type of the embedding space.
#[derive(Default)]
pub struct Scorer<K, D = DefaultEmbeddingSpace> {
    // A mapping from document ids to the document embeddings.
    embeddings: HashMap<K, Embedding<D>>,
    // A mapping from token indices to the set of documents that contain that token.
    inverted_token_index: HashMap<D, HashSet<K>>,
}

impl<K, D> Scorer<K, D>
where
    D: Eq + Hash + Clone,
    K: Eq + Hash + Clone,
{
    /// Creates a new `Scorer`.
    pub fn new() -> Scorer<K, D> {
        Scorer {
            embeddings: HashMap::new(),
            inverted_token_index: HashMap::new(),
        }
    }

    /// Upserts a document embedding into the scorer. If an embedding with the same id already
    /// exists, it will be replaced. Note that upserting a document will change the true value of
    /// `avgdl`. The more `avgdl` drifts from its true value, the less accurate the BM25 scores
    /// will be.
    pub fn upsert(&mut self, document_id: &K, embedding: Embedding<D>) {
        if self.embeddings.contains_key(document_id) {
            self.remove(document_id);
        }
        for token_index in embedding.indices() {
            let documents_containing_token = self
                .inverted_token_index
                .entry(token_index.clone())
                .or_default();
            documents_containing_token.insert(document_id.clone());
        }
        self.embeddings.insert(document_id.clone(), embedding);
    }

    /// Iterates the stored document embeddings.
    ///
    /// TUICommander patch. Upstream keeps `embeddings` private and offers no way
    /// to read it back, so an index can be built and queried but never persisted:
    /// the only route from a live `Scorer` to bytes on disk is through the
    /// embeddings it holds. Restoring already works with the public API
    /// (`Embedder::avgdl` plus `upsert`), so this closes the write half.
    ///
    /// Read-only and allocation-free — it hands out references into the map.
    pub fn embeddings(&self) -> impl Iterator<Item = (&K, &Embedding<D>)> {
        self.embeddings.iter()
    }

    /// Removes a document embedding from the scorer if it exists.
    pub fn remove(&mut self, document_id: &K) {
        if let Some(embedding) = self.embeddings.remove(document_id) {
            for token_index in embedding.indices() {
                if let Some(matches) = self.inverted_token_index.get_mut(token_index) {
                    matches.remove(document_id);
                }
            }
        }
    }

    /// Scores the embedding for the given document against a given query embedding. Returns `None`
    /// if the document does not exist in the scorer.
    pub fn score(&self, document_id: &K, query_embedding: &Embedding<D>) -> Option<f32> {
        let document_embedding = self.embeddings.get(document_id)?;
        Some(self.score_(document_embedding, query_embedding))
    }

    /// Returns all documents relevant (i.e., score > 0) to the given query embedding, sorted by
    /// relevance.
    pub fn matches(&self, query_embedding: &Embedding<D>) -> Vec<ScoredDocument<K>> {
        let relevant_embeddings_it = query_embedding
            .indices()
            .filter_map(|token_index| self.inverted_token_index.get(token_index))
            .flat_map(|document_set| document_set.iter())
            .collect::<HashSet<_>>()
            .into_iter()
            .filter_map(|document_id| self.embeddings.get(document_id).map(|e| (document_id, e)));

        let mut scores: Vec<_> = relevant_embeddings_it
            .map(|(document_id, document_embedding)| ScoredDocument {
                id: document_id.clone(),
                score: self.score_(document_embedding, query_embedding),
            })
            .collect();

        scores.sort_by(|a, b| b.score.partial_cmp(&a.score).unwrap_or(Ordering::Equal));
        scores
    }

    fn idf(&self, token_index: &D) -> f32 {
        let token_frequency = self
            .inverted_token_index
            .get(token_index)
            .map_or(0, |documents| documents.len()) as f32;
        let numerator = self.embeddings.len() as f32 - token_frequency + 0.5;
        let denominator = token_frequency + 0.5;
        (1f32 + (numerator / denominator)).ln()
    }

    fn score_(&self, document_embedding: &Embedding<D>, query_embedding: &Embedding<D>) -> f32 {
        let mut document_score = 0f32;

        for token_index in query_embedding.indices() {
            let token_idf = self.idf(token_index);
            let token_index_value = document_embedding
                .iter()
                .find(|token_embedding| token_embedding.index == *token_index)
                .map(|token_embedding| token_embedding.value)
                .unwrap_or(0f32);
            let token_score = token_idf * token_index_value;
            document_score += token_score;
        }
        document_score
    }
}

#[cfg(test)]
mod tests {
    use crate::TokenEmbedding;

    use super::*;

    fn scorer_with_embeddings(embeddings: &[Embedding]) -> Scorer<usize> {
        let mut scorer = Scorer::<usize>::new();

        for (i, document_embedding) in embeddings.iter().enumerate() {
            scorer.upsert(&i, document_embedding.clone());
        }

        scorer
    }

    #[test]
    fn it_scores_missing_document_as_none() {
        let scorer = Scorer::<usize>::new();
        let query_embedding = Embedding::any();
        let score = scorer.score(&12345, &query_embedding);
        let matches = scorer.matches(&query_embedding);
        assert_eq!(score, None);
        assert!(matches.is_empty());
    }

    #[test]
    fn it_scores_mutually_exclusive_indices_as_zero() {
        let document_embeddings = vec![Embedding(vec![TokenEmbedding::new(1, 1.0)])];
        let scorer = scorer_with_embeddings(&document_embeddings);

        let query_embedding = Embedding(vec![TokenEmbedding::new(0, 1.0)]);
        let score = scorer.score(&0, &query_embedding);

        assert_eq!(score, Some(0.0));
    }

    #[test]
    fn it_scores_rare_indices_higher_than_common_ones() {
        // BM25 should score rare token matches higher than common token matches.
        let document_embeddings = vec![
            Embedding(vec![TokenEmbedding::new(0, 1.0)]),
            Embedding(vec![TokenEmbedding::new(0, 1.0)]),
            Embedding(vec![TokenEmbedding::new(1, 1.0)]),
        ];
        let scorer = scorer_with_embeddings(&document_embeddings);

        let score_1 = scorer.score(&0, &Embedding(vec![TokenEmbedding::new(0, 1.0)]));
        let score_2 = scorer.score(&2, &Embedding(vec![TokenEmbedding::new(1, 1.0)]));

        assert!(score_1.unwrap() < score_2.unwrap());
    }

    #[test]
    fn it_scores_longer_embeddings_lower_than_shorter_ones() {
        let document_embeddings = vec![
            // Longer embeddings will have a lower value for unique tokens.
            Embedding(vec![
                TokenEmbedding::new(0, 0.9),
                TokenEmbedding::new(1, 0.9),
            ]),
            Embedding(vec![TokenEmbedding::new(0, 1.0)]),
        ];
        let scorer = scorer_with_embeddings(&document_embeddings);

        let score_1 = scorer.score(&0, &Embedding(vec![TokenEmbedding::new(0, 1.0)]));
        let score_2 = scorer.score(&1, &Embedding(vec![TokenEmbedding::new(0, 1.0)]));

        assert!(score_1.unwrap() < score_2.unwrap());
    }

    #[test]
    fn it_only_matches_embeddings_with_non_zero_score() {
        let document_embeddings = vec![
            Embedding(vec![TokenEmbedding::new(0, 1.0)]),
            Embedding(vec![TokenEmbedding::new(1, 1.0)]),
        ];
        let scorer = scorer_with_embeddings(&document_embeddings);

        let query_embedding = Embedding(vec![TokenEmbedding::new(0, 1.0)]);
        let matches = scorer.matches(&query_embedding);

        assert_eq!(
            matches,
            vec![ScoredDocument {
                id: 0,
                score: std::f32::consts::LN_2
            }]
        );
    }

    #[test]
    fn it_does_not_score_frequent_terms_negatively() {
        // In versions 2.2.1 and earlier, the IDF considered the total occurrences of a token where
        // it should have considered the total number of documents containing the token. In
        // instances where the occurrences exceeded the number of documents, the IDF (and therefore
        // the score) would be negative.
        // See this bug report for more information: https://github.com/Michael-JB/bm25/pull/20
        let document_embeddings = vec![Embedding(vec![
            TokenEmbedding::new(0, 1.5),
            TokenEmbedding::new(0, 1.5),
        ])];
        let scorer = scorer_with_embeddings(&document_embeddings);
        let query_embedding = Embedding(vec![TokenEmbedding::new(0, 1.0)]);

        let matches = scorer.matches(&query_embedding);

        assert!(matches[0].score >= 0.0);
    }

    #[test]
    fn it_sorts_matches_by_score() {
        let document_embeddings = vec![
            Embedding(vec![
                TokenEmbedding::new(0, 0.9),
                TokenEmbedding::new(1, 0.9),
            ]),
            Embedding(vec![TokenEmbedding::new(0, 1.0)]),
        ];
        let scorer = scorer_with_embeddings(&document_embeddings);

        let query_embedding = Embedding(vec![TokenEmbedding::new(0, 1.0)]);
        let matches = scorer.matches(&query_embedding);

        assert_eq!(
            matches,
            vec![
                ScoredDocument {
                    id: 1,
                    score: 0.1823216
                },
                ScoredDocument {
                    id: 0,
                    score: 0.16408943
                }
            ]
        );
    }
}
