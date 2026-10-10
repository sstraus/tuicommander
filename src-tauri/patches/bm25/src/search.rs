use crate::DefaultTokenizer;
use crate::{
    Tokenizer,
    embedder::{DefaultTokenEmbedder, Embedder, EmbedderBuilder, TokenEmbedder},
    scorer::{ScoredDocument, Scorer},
};
use std::{
    collections::HashMap,
    fmt::{self, Debug, Display},
    hash::Hash,
    marker::PhantomData,
};

/// A document that you can insert into a search engine. K is the type of the document id. Note
/// that it is more effient to use a numeric type.
#[derive(Eq, PartialEq, Debug, Clone, PartialOrd, Hash)]
pub struct Document<K> {
    /// A unique identifier for the document.
    pub id: K,
    /// The contents of the document.
    pub contents: String,
}

impl<K> Display for Document<K> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.contents)
    }
}

impl<K> Document<K> {
    /// Creates a new document with the given id and contents.
    pub fn new(id: K, contents: impl Into<String>) -> Document<K> {
        Document {
            id,
            contents: contents.into(),
        }
    }
}

/// A search result, containing a document and its BM25 score.
#[derive(PartialEq, Debug, Clone)]
pub struct SearchResult<K> {
    /// The document that was found.
    pub document: Document<K>,
    /// The BM25 score of the document. A higher score means the document is more relevant to the
    /// query.
    pub score: f32,
}

/// A search engine that ranks documents with BM25. K is the type of the document id, D is the
/// type of the token embedder and T is the type of the tokenizer.
pub struct SearchEngine<K, D: TokenEmbedder = DefaultTokenEmbedder, T = DefaultTokenizer> {
    // The embedder used to convert documents into embeddings.
    embedder: Embedder<D, T>,
    // A scorer for document embeddings.
    scorer: Scorer<K, D::EmbeddingSpace>,
    // A mapping from document ids to document contents.
    documents: HashMap<K, String>,
}

impl<K: Debug, D: TokenEmbedder + Debug, T: Debug> Debug for SearchEngine<K, D, T> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "SearchEngine {{ embedder: {:?}, documents: {:?} }}",
            self.embedder, self.documents
        )
    }
}

impl<K, D, T> SearchEngine<K, D, T>
where
    K: Hash + Eq + Clone,
    D: TokenEmbedder,
    D::EmbeddingSpace: Eq + Hash + Clone,
    T: Tokenizer,
{
    /// Upserts a document into the search engine. If a document with the same id already exists,
    /// it will be replaced. Note that upserting a document will change the true value of `avgdl`.
    /// The more `avgdl` drifts from its true value, the less accurate the BM25 scores will be.
    pub fn upsert(&mut self, document: impl Into<Document<K>>) {
        let document = document.into();
        let embedding = self.embedder.embed(document.contents.as_str());

        if self.documents.contains_key(&document.id) {
            self.remove(&document.id);
        }
        self.documents
            .insert(document.id.clone(), document.contents);

        self.scorer.upsert(&document.id, embedding);
    }

    /// Removes a document from the search engine if it exists.
    pub fn remove(&mut self, document_id: &K) {
        self.documents.remove(document_id);
        self.scorer.remove(document_id);
    }

    /// Gets the contents of a document by its id.
    pub fn get(&self, document_id: &K) -> Option<Document<K>> {
        self.documents.get(document_id).map(|contents| Document {
            id: document_id.clone(),
            contents: contents.clone(),
        })
    }

    /// Returns an iterator over the documents in the search engine.
    pub fn iter(&self) -> impl Iterator<Item = Document<K>> + '_ {
        self.documents.iter().map(|(id, contents)| Document {
            id: id.clone(),
            contents: contents.clone(),
        })
    }

    /// Searches the documents for the given query and returns the top `limit` results.
    /// Only the document contents are searched, not the document ids.
    pub fn search(&self, query: &str, limit: impl Into<Option<usize>>) -> Vec<SearchResult<K>> {
        let query_embedding = self.embedder.embed(query);

        // Reduce search space by filtering out all documents whose score would be 0
        let matches = self.scorer.matches(&query_embedding);

        matches
            .into_iter()
            .take(limit.into().unwrap_or(usize::MAX))
            .filter_map(|ScoredDocument { id, score }| {
                self.get(&id)
                    .map(|document| SearchResult { document, score })
            })
            .collect()
    }
}

/// A consuming builder for SearchEngine. K is the type of the document id, D is the type of the
/// token embedder and T is the type of the tokenizer.
pub struct SearchEngineBuilder<K, D = DefaultTokenEmbedder, T = DefaultTokenizer> {
    embedder_builder: EmbedderBuilder<D, T>,
    documents: Vec<Document<K>>,
    document_id_type: PhantomData<K>,
    token_embedder_type: PhantomData<D>,
}

impl<K, D, T> SearchEngineBuilder<K, D, T>
where
    K: Hash + Eq + Clone,
    D: TokenEmbedder,
    D::EmbeddingSpace: Eq + Hash + Clone,
    T: Tokenizer + Sync,
{
    /// Constructs a new SearchEngineBuilder with the given average document length. Use this if you
    /// know the average document length in advance. If you don't, but you have your full corpus
    /// ahead of time, use `with_documents` or `with_corpus` instead.
    ///
    /// If you have neither the full corpus nor a sample of it, you can configure the embedder to
    /// disregard document length by setting `b` to 0.0. In this case, it doesn't matter what
    /// value you pass to `with_avgdl`.
    ///
    /// The average document length is the average number of tokens in a document from your corpus;
    /// if you need access to this value, you can construct an Embedder and call `avgdl` on it.
    pub fn with_avgdl(avgdl: f32) -> SearchEngineBuilder<K, D, T>
    where
        T: Default,
    {
        SearchEngineBuilder {
            embedder_builder: EmbedderBuilder::<D, T>::with_avgdl(avgdl),
            documents: Vec::new(),
            document_id_type: PhantomData,
            token_embedder_type: PhantomData,
        }
    }

    /// Constructs a new SearchEngineBuilder with the given documents. The search engine will fit
    /// to the given documents, using the given tokenizer. When you call `build`, the builder
    /// will pre-populate the search engine with the given documents, and pass on the tokenizer.
    pub fn with_tokenizer_and_documents(
        tokenizer: T,
        documents: impl IntoIterator<Item = impl Into<Document<K>>>,
    ) -> SearchEngineBuilder<K, D, T> {
        let documents = documents.into_iter().map(|d| d.into()).collect::<Vec<_>>();
        SearchEngineBuilder {
            embedder_builder: EmbedderBuilder::<D, T>::with_tokenizer_and_fit_to_corpus(
                tokenizer,
                &documents
                    .iter()
                    .map(|d| d.contents.as_str())
                    .collect::<Vec<_>>(),
            ),
            documents,
            document_id_type: PhantomData,
            token_embedder_type: PhantomData,
        }
    }

    /// Constructs a new SearchEngineBuilder with the corpus. The search engine will fit
    /// to the given corpus, using the given tokenizer. When you call `build`, the builder
    /// will pre-populate the search engine with the given corpus, and pass on the tokenizer.
    /// This function will automatically generate u32 ids for each entry in your corpus.
    pub fn with_tokenizer_and_corpus(
        tokenizer: T,
        corpus: impl IntoIterator<Item = impl Into<String>>,
    ) -> SearchEngineBuilder<u32, D, T> {
        let documents = corpus
            .into_iter()
            .enumerate()
            .map(|(id, document)| Document::new(id as u32, document.into()))
            .collect::<Vec<_>>();
        SearchEngineBuilder::<u32, D, T>::with_tokenizer_and_documents(tokenizer, documents)
    }

    /// Sets the tokenizer of the embedder.
    pub fn tokenizer(self, tokenizer: T) -> Self {
        Self {
            embedder_builder: self.embedder_builder.tokenizer(tokenizer),
            ..self
        }
    }

    /// Sets the k1 parameter of the embedder.
    pub fn k1(self, k1: f32) -> Self {
        Self {
            embedder_builder: self.embedder_builder.k1(k1),
            ..self
        }
    }

    /// Sets the b parameter of the embedder.
    pub fn b(self, b: f32) -> Self {
        Self {
            embedder_builder: self.embedder_builder.b(b),
            ..self
        }
    }

    /// Overrides the average document length of the embedder.
    pub fn avgdl(self, avgdl: f32) -> Self {
        Self {
            embedder_builder: self.embedder_builder.avgdl(avgdl),
            ..self
        }
    }

    /// Builds the search engine.
    pub fn build(self) -> SearchEngine<K, D, T> {
        let mut search_engine = SearchEngine::<K, D, T> {
            embedder: self.embedder_builder.build(),
            scorer: Scorer::<K, D::EmbeddingSpace>::new(),
            documents: HashMap::new(),
        };
        for document in self.documents {
            search_engine.upsert(document);
        }
        search_engine
    }
}

#[cfg(feature = "default_tokenizer")]
impl<K, D> SearchEngineBuilder<K, D, DefaultTokenizer>
where
    K: Hash + Eq + Clone,
    D: TokenEmbedder,
    D::EmbeddingSpace: Eq + Hash + Clone,
{
    /// Constructs a new SearchEngineBuilder with the given documents. The search engine will fit
    /// to the given documents, using the default tokenizer configured with the given language mode.
    /// When you call `build`, the builder will pre-populate the search engine with the given
    /// documents, and pass on the tokenizer.
    pub fn with_documents(
        language_mode: impl Into<crate::LanguageMode>,
        documents: impl IntoIterator<Item = impl Into<Document<K>>>,
    ) -> Self {
        Self::with_tokenizer_and_documents(DefaultTokenizer::new(language_mode), documents)
    }

    /// Constructs a new SearchEngineBuilder with the corpus. The search engine will fit
    /// to the given corpus, using the default tokenizer configured with the given language mode.
    /// When you call `build`, the builder will pre-populate the search engine with the given
    /// corpus and pass on the tokenizer. This function will automatically generate u32 ids for
    /// each entry in your corpus.
    pub fn with_corpus(
        language_mode: impl Into<crate::LanguageMode>,
        corpus: impl IntoIterator<Item = impl Into<String>>,
    ) -> SearchEngineBuilder<u32, D, DefaultTokenizer> {
        SearchEngineBuilder::<u32, D, DefaultTokenizer>::with_tokenizer_and_corpus(
            DefaultTokenizer::new(language_mode),
            corpus,
        )
    }

    /// Sets the tokenizer to the default tokenizer with the given language mode.
    pub fn language_mode(self, language_mode: impl Into<crate::LanguageMode>) -> Self {
        Self::tokenizer(self, DefaultTokenizer::new(language_mode))
    }
}

#[cfg(test)]
mod tests {
    use insta::assert_debug_snapshot;

    use super::*;
    use crate::{
        Language, LanguageMode,
        test_data_loader::tests::{Recipe, read_recipes},
    };

    impl From<Recipe> for Document<String> {
        fn from(value: Recipe) -> Self {
            Document::new(value.title, value.recipe)
        }
    }

    fn create_recipe_search_engine(
        recipe_file: &str,
        language_mode: impl Into<LanguageMode>,
    ) -> SearchEngine<String, u32> {
        let recipes = read_recipes(recipe_file);

        SearchEngineBuilder::with_documents(language_mode, recipes).build()
    }

    #[test]
    fn search_returns_relevant_documents() {
        let corpus = vec!["space station", "bacon and avocado sandwich"];
        let search_engine =
            SearchEngineBuilder::<u32>::with_corpus(Language::English, corpus).build();

        let results = search_engine.search("sandwich with bacon", 5);
        assert!(results.len() == 1);
        assert!(results[0].document.contents == "bacon and avocado sandwich");
        assert!(results[0].score > 0.0);
    }

    #[test]
    fn search_does_not_return_unrelated_documents() {
        let corpus = vec!["space station", "bacon and avocado sandwich"];
        let search_engine =
            SearchEngineBuilder::<u32>::with_corpus(Language::English, corpus).build();

        let results = search_engine.search("maths and computer science", 5);
        assert!(results.is_empty());
    }

    #[test]
    fn it_can_insert_a_document() {
        let mut search_engine = SearchEngineBuilder::<&str>::with_avgdl(2.0).build();
        let document = Document::new("hello world", "bananas and apples");
        let document_id = document.id;

        search_engine.upsert(document.clone());
        let result = search_engine.get(&document_id);

        assert!(result.unwrap() == document);
    }

    #[test]
    fn it_can_remove_a_document() {
        let mut search_engine = SearchEngineBuilder::<usize>::with_avgdl(2.0).build();
        let document = Document::new(123, "bananas and apples");
        let document_id = document.id;

        search_engine.upsert(document);
        search_engine.remove(&document_id);

        assert!(search_engine.get(&document_id).is_none());
    }

    #[test]
    fn it_can_update_a_document() {
        let document_id = "hello_world";
        let document = Document::new(document_id, "bananas and apples");
        let mut search_engine =
            SearchEngineBuilder::<&str>::with_documents(Language::English, vec![document]).build();
        let new_document = Document::new(document_id, "oranges and papayas");

        search_engine.upsert(new_document.clone());
        let result = search_engine.get(&document_id);

        assert!(result.unwrap() == new_document);
    }

    #[test]
    fn handles_empty_input() {
        let mut search_engine = SearchEngineBuilder::<u32>::with_avgdl(2.0).build();
        let document = Document::new(123, "");

        search_engine.upsert(document);

        let results = search_engine.search("bacon sandwich", 5);
        assert!(results.is_empty());
    }

    #[test]
    fn handles_empty_search() {
        let mut search_engine = SearchEngineBuilder::<u32>::with_avgdl(2.0).build();
        let document = Document::new(123, "pencil and paper");

        search_engine.upsert(document);

        let results = search_engine.search("", 5);
        assert!(results.is_empty());
    }

    #[test]
    fn it_returns_exact_matches_with_highest_score() {
        let search_engine = create_recipe_search_engine("recipes_en.csv", Language::English);

        let results = search_engine.search(
            "To make guacamole, start by mashing 2 ripe avocados in a bowl.",
            None,
        );

        assert!(!results.is_empty());
        assert_eq!(results[0].document.id, "Guacamole");
    }

    #[test]
    fn it_only_returns_results_containing_query() {
        let search_engine = create_recipe_search_engine("recipes_en.csv", Language::English);

        let results = search_engine.search("vegetable", 5);

        // At least 5 recipes contain the word "vegetable"
        assert_eq!(results.len(), 5);
        assert!(
            results
                .iter()
                .all(|result| result.document.contents.contains("vegetable"))
        );
    }

    #[test]
    fn it_returns_results_sorted_by_score() {
        let search_engine = create_recipe_search_engine("recipes_en.csv", Language::English);

        let results = search_engine.search("chicken", 1000);

        assert!(!results.is_empty());
        assert!(
            results
                .windows(2)
                .all(|result_pair| { result_pair[0].score >= result_pair[1].score })
        );
    }

    #[test]
    fn it_ranks_shorter_documents_higher() {
        let documents = [
            Document {
                id: 0,
                contents: "Correct horse battery staple bacon bacon bacon".to_string(),
            },
            Document {
                id: 1,
                contents: "Correct horse battery staple".to_string(),
            },
        ];
        let search_engine =
            SearchEngineBuilder::<u32>::with_documents(Language::English, documents).build();

        let results = search_engine.search("staple", 2);

        assert_eq!(results.len(), 2);
        assert_eq!(results[0].document.id, 1);
        assert_eq!(results[1].document.id, 0);
        assert!(results[0].score > results[1].score);
    }

    #[test]
    fn it_matches_common_unicode_equivalents() {
        let corpus = vec!["étude"];
        let search_engine =
            SearchEngineBuilder::<u32>::with_corpus(Language::French, corpus).build();

        let results_1 = search_engine.search("etude", None);
        let results_2 = search_engine.search("étude", None);

        assert_eq!(results_1.len(), 1);
        assert_eq!(results_2.len(), 1);
        assert_eq!(results_1, results_2);
    }

    #[test]
    fn it_can_search_for_emoji() {
        let corpus = vec!["🔥"];
        let search_engine =
            SearchEngineBuilder::<u32>::with_corpus(Language::English, corpus).build();

        let results_1 = search_engine.search("🔥", None);
        let results_2 = search_engine.search("fire", None);

        assert_eq!(results_1.len(), 1);
        assert_eq!(results_2.len(), 1);
        assert_eq!(results_1, results_2);
    }

    #[test]
    fn it_matches_snapshot_en() {
        let search_engine = create_recipe_search_engine("recipes_en.csv", Language::English);

        let mut results = search_engine.search("bake", None);
        // sort the results by document id to make the snapshot deterministic
        results.sort_by_key(|result| result.document.id.clone());

        insta::with_settings!({snapshot_path => "../snapshots"}, {
            assert_debug_snapshot!(results);
        });
    }

    #[test]
    fn it_matches_snapshot_de() {
        let search_engine = create_recipe_search_engine("recipes_de.csv", Language::German);

        let mut results = search_engine.search("backen", None);

        // sort the results by document id to make the snapshot deterministic
        results.sort_by_key(|result| result.document.id.clone());

        insta::with_settings!({snapshot_path => "../snapshots"}, {
            assert_debug_snapshot!(results);
        });
    }
}
