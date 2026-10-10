use rust_stemmers::{Algorithm as StemmingAlgorithm, Stemmer};
use std::{
    borrow::Cow,
    collections::{HashSet, VecDeque},
    fmt::{self, Debug},
    sync::{LazyLock, Mutex},
};
use stop_words::LANGUAGE as StopWordLanguage;
#[cfg(feature = "language_detection")]
use whichlang::Lang as DetectedLanguage;

use crate::tokenizer::Tokenizer;

/// Languages supported by the tokenizer.
#[allow(missing_docs)]
#[derive(Debug, Clone, Eq, PartialEq, Hash)]
pub enum Language {
    Arabic,
    Danish,
    Dutch,
    English,
    French,
    German,
    Greek,
    Hungarian,
    Italian,
    Norwegian,
    Portuguese,
    Romanian,
    Russian,
    Spanish,
    Swedish,
    Tamil,
    Turkish,
}

/// The language mode used by the tokenizer. This determines the algorithm used for stemming and
/// the dictionary of stopwords. This enum is non-exhaustive as the `Detect` variant is only
/// available when the `language_detection` feature is enabled.
#[non_exhaustive]
#[derive(Debug, Clone)]
pub enum LanguageMode {
    /// Automatically detect the language. Note that this adds a small performance overhead.
    #[cfg(feature = "language_detection")]
    Detect,
    /// Use a fixed language.
    Fixed(Language),
}

impl Default for LanguageMode {
    fn default() -> Self {
        LanguageMode::Fixed(Language::English)
    }
}

impl From<Language> for LanguageMode {
    fn from(language: Language) -> Self {
        LanguageMode::Fixed(language)
    }
}

#[cfg(feature = "language_detection")]
impl TryFrom<DetectedLanguage> for Language {
    type Error = ();

    fn try_from(detected_language: DetectedLanguage) -> Result<Self, Self::Error> {
        match detected_language {
            DetectedLanguage::Ara => Ok(Language::Arabic),
            DetectedLanguage::Cmn => Err(()),
            DetectedLanguage::Deu => Ok(Language::German),
            DetectedLanguage::Eng => Ok(Language::English),
            DetectedLanguage::Fra => Ok(Language::French),
            DetectedLanguage::Hin => Err(()),
            DetectedLanguage::Ita => Ok(Language::Italian),
            DetectedLanguage::Jpn => Err(()),
            DetectedLanguage::Kor => Err(()),
            DetectedLanguage::Nld => Ok(Language::Dutch),
            DetectedLanguage::Por => Ok(Language::Portuguese),
            DetectedLanguage::Rus => Ok(Language::Russian),
            DetectedLanguage::Spa => Ok(Language::Spanish),
            DetectedLanguage::Swe => Ok(Language::Swedish),
            DetectedLanguage::Tur => Ok(Language::Turkish),
            DetectedLanguage::Vie => Err(()),
        }
    }
}

impl From<&Language> for StemmingAlgorithm {
    fn from(language: &Language) -> Self {
        match language {
            Language::Arabic => StemmingAlgorithm::Arabic,
            Language::Danish => StemmingAlgorithm::Danish,
            Language::Dutch => StemmingAlgorithm::Dutch,
            Language::English => StemmingAlgorithm::English,
            Language::French => StemmingAlgorithm::French,
            Language::German => StemmingAlgorithm::German,
            Language::Greek => StemmingAlgorithm::Greek,
            Language::Hungarian => StemmingAlgorithm::Hungarian,
            Language::Italian => StemmingAlgorithm::Italian,
            Language::Norwegian => StemmingAlgorithm::Norwegian,
            Language::Portuguese => StemmingAlgorithm::Portuguese,
            Language::Romanian => StemmingAlgorithm::Romanian,
            Language::Russian => StemmingAlgorithm::Russian,
            Language::Spanish => StemmingAlgorithm::Spanish,
            Language::Swedish => StemmingAlgorithm::Swedish,
            Language::Tamil => StemmingAlgorithm::Tamil,
            Language::Turkish => StemmingAlgorithm::Turkish,
        }
    }
}

impl TryFrom<&Language> for StopWordLanguage {
    type Error = ();

    fn try_from(language: &Language) -> Result<Self, Self::Error> {
        match language {
            Language::Arabic => Ok(StopWordLanguage::Arabic),
            Language::Danish => Ok(StopWordLanguage::Danish),
            Language::Dutch => Ok(StopWordLanguage::Dutch),
            Language::English => Ok(StopWordLanguage::English),
            Language::French => Ok(StopWordLanguage::French),
            Language::German => Ok(StopWordLanguage::German),
            Language::Greek => Ok(StopWordLanguage::Greek),
            Language::Hungarian => Ok(StopWordLanguage::Hungarian),
            Language::Italian => Ok(StopWordLanguage::Italian),
            Language::Norwegian => Ok(StopWordLanguage::Norwegian),
            Language::Portuguese => Ok(StopWordLanguage::Portuguese),
            Language::Romanian => Ok(StopWordLanguage::Romanian),
            Language::Russian => Ok(StopWordLanguage::Russian),
            Language::Spanish => Ok(StopWordLanguage::Spanish),
            Language::Swedish => Ok(StopWordLanguage::Swedish),
            Language::Tamil => Err(()),
            Language::Turkish => Ok(StopWordLanguage::Turkish),
        }
    }
}

fn normalize(text: &str) -> Cow<'_, str> {
    deunicode::deunicode_with_tofu_cow(text, "[?]")
}

// TUICommander patch: retain the bounded stopword LRU without cached's macros.
#[derive(Default)]
struct StopwordCache(VecDeque<(Language, bool, HashSet<String>)>);

impl StopwordCache {
    fn get(&mut self, language: &Language, normalized: bool) -> Option<HashSet<String>> {
        let index = self
            .0
            .iter()
            .position(|(lang, norm, _)| lang == language && *norm == normalized)?;
        let entry = self.0.remove(index)?;
        let words = entry.2.clone();
        self.0.push_front(entry);
        Some(words)
    }

    fn insert(&mut self, language: Language, normalized: bool, words: HashSet<String>) {
        if let Some(index) = self
            .0
            .iter()
            .position(|(lang, norm, _)| *lang == language && *norm == normalized)
        {
            self.0.remove(index);
        }
        self.0.push_front((language, normalized, words));
        self.0.truncate(16);
    }
}

fn get_stopwords(language: Language, normalized: bool) -> HashSet<String> {
    static CACHE: LazyLock<Mutex<StopwordCache>> =
        LazyLock::new(|| Mutex::new(StopwordCache::default()));
    let mut cache = CACHE.lock().expect("stopword cache lock poisoned");
    if let Some(words) = cache.get(&language, normalized) {
        return words;
    }
    let words = match TryInto::<StopWordLanguage>::try_into(&language) {
        Err(_) => HashSet::new(),
        Ok(lang) => stop_words::get(lang)
            .iter()
            .map(|w| match normalized {
                true => normalize(w).into(),
                false => w.to_string(),
            })
            .collect(),
    };
    cache.insert(language, normalized, words.clone());
    words
}

fn get_stemmer(language: &Language) -> Stemmer {
    Stemmer::create(language.into())
}

#[derive(Clone, Debug)]
struct Settings {
    stemming: bool,
    stopwords: bool,
    normalization: bool,
}

impl Settings {
    fn new(stemming: bool, stopwords: bool, normalization: bool) -> Self {
        Settings {
            stemming,
            stopwords,
            normalization,
        }
    }
}

struct Components {
    settings: Settings,
    normalizer: fn(&str) -> Cow<str>,
    stemmer: Option<Stemmer>,
    stopwords: HashSet<String>,
}

impl Components {
    fn new(settings: Settings, language: Option<&Language>) -> Self {
        let stemmer = match language {
            Some(lang) => match settings.stemming {
                true => Some(get_stemmer(lang)),
                false => None,
            },
            None => None,
        };
        let stopwords = match language {
            Some(lang) => match settings.stopwords {
                true => get_stopwords(lang.clone(), settings.normalization),
                false => HashSet::new(),
            },
            None => HashSet::new(),
        };
        let normalizer: fn(&str) -> Cow<str> = match settings.normalization {
            true => normalize,
            false => |text: &str| Cow::from(text),
        };
        Self {
            settings,
            stemmer,
            stopwords,
            normalizer,
        }
    }
}

#[non_exhaustive]
enum Resources {
    Static(Components),
    #[cfg(feature = "language_detection")]
    Dynamic(Settings),
}

pub struct DefaultTokenizer {
    resources: Resources,
}

impl Debug for DefaultTokenizer {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let settings = match &self.resources {
            Resources::Static(components) => components.settings.clone(),
            #[cfg(feature = "language_detection")]
            Resources::Dynamic(settings) => settings.clone(),
        };
        write!(f, "DefaultTokenizer({settings:?})")
    }
}

impl DefaultTokenizer {
    /// Creates a new `DefaultTokenizer` with the given `LanguageMode`.
    pub fn new(language_mode: impl Into<LanguageMode>) -> DefaultTokenizer {
        Self::builder().language_mode(language_mode).build()
    }

    /// Creates a new `DefaultTokenizerBuilder` that you can use to customise the tokenizer.
    pub fn builder() -> DefaultTokenizerBuilder {
        DefaultTokenizerBuilder::new()
    }

    fn _new(
        language_mode: impl Into<LanguageMode>,
        normalization: bool,
        stemming: bool,
        stopwords: bool,
    ) -> DefaultTokenizer {
        let language_mode = &language_mode.into();
        let settings = Settings::new(stemming, stopwords, normalization);
        let resources = match language_mode {
            #[cfg(feature = "language_detection")]
            LanguageMode::Detect => Resources::Dynamic(settings),
            LanguageMode::Fixed(lang) => Resources::Static(Components::new(settings, Some(lang))),
        };
        DefaultTokenizer { resources }
    }

    #[cfg(feature = "language_detection")]
    fn detect_language(text: &str) -> Option<Language> {
        Language::try_from(whichlang::detect_language(text)).ok()
    }

    fn split_on_word_boundaries(text: &str) -> impl Iterator<Item = &'_ str> {
        use unicode_segmentation::UnicodeSegmentation;
        text.unicode_words().filter(|s| !s.is_empty())
    }

    fn _tokenize(&self, input_text: &str, components: &Components) -> Vec<String> {
        // Normalize
        let text = (components.normalizer)(input_text);
        // Transform to lowercase (required for stemming and stopwords)
        let text = text.to_lowercase();
        // Split
        let tokens = Self::split_on_word_boundaries(&text);
        // Remove stopwords
        let tokens = tokens.filter(|token| !components.stopwords.contains(*token));
        // Stem
        let tokens = tokens.map(|token| match &components.stemmer {
            Some(stemmer) => stemmer.stem(token).to_string(),
            None => token.to_string(),
        });
        tokens.collect()
    }

    fn tokenize(&self, input_text: &str) -> Vec<String> {
        if input_text.is_empty() {
            return Vec::new();
        }
        match &self.resources {
            Resources::Static(components) => self._tokenize(input_text, components),
            #[cfg(feature = "language_detection")]
            Resources::Dynamic(settings) => {
                let detected_language = Self::detect_language(input_text);
                let components = Components::new(settings.clone(), detected_language.as_ref());
                self._tokenize(input_text, &components)
            }
        }
    }
}

impl Tokenizer for DefaultTokenizer {
    fn tokenize(&self, input_text: &str) -> Vec<String> {
        DefaultTokenizer::tokenize(self, input_text)
    }
}

impl Default for DefaultTokenizer {
    fn default() -> Self {
        DefaultTokenizer::new(LanguageMode::default())
    }
}

pub struct DefaultTokenizerBuilder {
    language_mode: LanguageMode,
    normalization: bool,
    stemming: bool,
    stopwords: bool,
}

impl Default for DefaultTokenizerBuilder {
    fn default() -> Self {
        DefaultTokenizerBuilder::new()
    }
}

impl DefaultTokenizerBuilder {
    /// Creates a new `DefaultTokenizerBuilder`.
    pub fn new() -> DefaultTokenizerBuilder {
        DefaultTokenizerBuilder {
            language_mode: LanguageMode::default(),
            normalization: true,
            stemming: true,
            stopwords: true,
        }
    }

    /// Sets the language mode used by the tokenizer. Default is `Language::English`.
    pub fn language_mode(mut self, language_mode: impl Into<LanguageMode>) -> Self {
        self.language_mode = language_mode.into();
        self
    }

    /// Enables or disables normalization. Normalization converts unicode characters to ASCII.
    /// (With normalization, 'é' -> 'e', '🍕' -> 'pizza', etc.)
    /// Default is `true`.
    pub fn normalization(mut self, normalization: bool) -> Self {
        self.normalization = normalization;
        self
    }

    /// Enables or disables stemming. Stemming reduces words to their root form.
    /// (With stemming, 'running' -> 'run', 'connection' -> 'connect', etc.)
    /// Default is `true`.
    pub fn stemming(mut self, stemming: bool) -> Self {
        self.stemming = stemming;
        self
    }

    /// Enables or disables stopwords. Stopwords are common words that carry little meaning.
    /// (With stopwords, 'the', 'and', 'is', etc. are removed.)
    /// Default is `true`.
    pub fn stopwords(mut self, stopwords: bool) -> Self {
        self.stopwords = stopwords;
        self
    }

    /// Builds the `DefaultTokenizer`.
    pub fn build(self) -> DefaultTokenizer {
        DefaultTokenizer::_new(
            self.language_mode,
            self.normalization,
            self.stemming,
            self.stopwords,
        )
    }
}

#[cfg(test)]
mod tests {
    use crate::test_data_loader::tests::{Recipe, read_recipes};

    use super::*;

    use insta::assert_debug_snapshot;

    // Catches cached stopwords leaking across languages or normalization settings.
    #[test]
    fn alternating_languages_and_normalization_keep_their_own_stopwords() {
        // The shipped English lists contain "the"; French contains "été", not "ete".
        // Literal expected tokens must not be derived from the cache or dictionary helper.
        let cases = [
            (Language::French, false, vec!["the", "ete", "nebula"]),
            (Language::English, true, vec!["ete", "ete", "nebula"]),
            (Language::French, true, vec!["the", "nebula"]),
            (Language::English, false, vec!["été", "ete", "nebula"]),
        ];
        for _ in 0..2 {
            for (language, normalization, expected) in &cases {
                let tokenizer = DefaultTokenizer::builder()
                    .language_mode(language.clone())
                    .normalization(*normalization)
                    .stemming(false)
                    .build();
                assert_eq!(
                    Tokenizer::tokenize(&tokenizer, "the été ete nebula"),
                    *expected,
                    "language={language:?}, normalization={normalization}"
                );
            }
        }
    }

    fn tokenize_recipes(recipe_file: &str, language_mode: LanguageMode) -> Vec<Vec<String>> {
        let recipes = read_recipes(recipe_file);

        recipes
            .iter()
            .map(|Recipe { recipe, .. }| {
                let tokenizer = DefaultTokenizer::new(language_mode.clone());
                tokenizer.tokenize(recipe)
            })
            .collect()
    }

    #[test]
    fn it_can_tokenize_english() {
        let text = "space station";
        let tokenizer = DefaultTokenizer::new(Language::English);

        let tokens = tokenizer.tokenize(text);

        assert_eq!(tokens, vec!["space", "station"]);
    }

    #[test]
    fn it_converts_to_lowercase() {
        let text = "SPACE STATION";
        let tokenizer = DefaultTokenizer::new(Language::English);

        let tokens = tokenizer.tokenize(text);

        assert_eq!(tokens, vec!["space", "station"]);
    }

    #[test]
    fn it_removes_whitespace() {
        let text = "\tspace\r\nstation\n space       station";
        let tokenizer = DefaultTokenizer::new(Language::English);

        let tokens = tokenizer.tokenize(text);

        assert_eq!(tokens, vec!["space", "station", "space", "station"]);
    }

    #[test]
    fn it_removes_stopwords() {
        let text = "i me my myself we our ours ourselves you you're you've you'll you'd";
        let tokenizer = DefaultTokenizer::new(Language::English);

        let tokens = tokenizer.tokenize(text);

        assert!(tokens.is_empty());
    }

    #[test]
    fn it_keeps_numbers() {
        let text = "42 1337 3.14";
        let tokenizer = DefaultTokenizer::new(Language::English);

        let tokens = tokenizer.tokenize(text);

        assert_eq!(tokens, vec!["42", "1337", "3.14"]);
    }

    #[test]
    fn it_keeps_contracted_words() {
        let text = "can't you're won't let's couldn't've";
        let tokenizer = DefaultTokenizer::builder()
            .language_mode(Language::English)
            .stemming(false)
            .stopwords(false)
            .build();

        let tokens = tokenizer.tokenize(text);

        assert_eq!(
            tokens,
            vec!["can't", "you're", "won't", "let's", "couldn't've"]
        );
    }

    #[test]
    fn it_removes_punctuation() {
        let test_cases = vec![
            ("space, station!", vec!["space", "station"]),
            ("space,station", vec!["space", "station"]),
            ("!\"#$%&'()*+,-./:;<=>?@[\\]^_`{|}~", vec![]),
        ];
        let tokenizer = DefaultTokenizer::new(Language::English);

        for (text, expected) in test_cases {
            let tokens = tokenizer.tokenize(text);
            assert_eq!(tokens, expected);
        }
    }

    #[test]
    fn it_stems_words() {
        let text = "connection connections connective connected connecting connect";
        let tokenizer = DefaultTokenizer::new(Language::English);

        let tokens = tokenizer.tokenize(text);

        assert_eq!(
            tokens,
            vec![
                "connect", "connect", "connect", "connect", "connect", "connect"
            ]
        );
    }

    #[test]
    fn it_tokenizes_emojis_as_text() {
        let text = "🍕 🚀 🍋";
        let tokenizer = DefaultTokenizer::new(Language::English);

        let tokens = tokenizer.tokenize(text);

        assert_eq!(tokens, vec!["pizza", "rocket", "lemon"]);
    }

    #[test]
    fn it_converts_unicode_to_ascii() {
        let text = "gemüse, Gießen";
        let tokenizer = DefaultTokenizer::builder()
            .language_mode(Language::German)
            .stemming(false)
            .build();

        let tokens = tokenizer.tokenize(text);

        assert_eq!(tokens, vec!["gemuse", "giessen"]);
    }

    #[test]
    #[cfg(feature = "language_detection")]
    fn it_handles_empty_input() {
        let text = "";
        let tokenizer = DefaultTokenizer::new(LanguageMode::Detect);

        let tokens = tokenizer.tokenize(text);

        assert!(tokens.is_empty());
    }

    #[test]
    #[cfg(feature = "language_detection")]
    fn it_detects_english() {
        let tokens_detected = tokenize_recipes("recipes_en.csv", LanguageMode::Detect);
        let tokens_en = tokenize_recipes("recipes_en.csv", LanguageMode::Fixed(Language::English));

        assert_eq!(tokens_detected, tokens_en);
    }

    #[test]
    #[cfg(feature = "language_detection")]
    fn it_detects_german() {
        let tokens_detected = tokenize_recipes("recipes_de.csv", LanguageMode::Detect);
        let token_de = tokenize_recipes("recipes_de.csv", LanguageMode::Fixed(Language::German));

        assert_eq!(tokens_detected, token_de);
    }

    #[test]
    fn it_matches_snapshot_en() {
        let tokens = tokenize_recipes("recipes_en.csv", LanguageMode::Fixed(Language::English));

        insta::with_settings!({snapshot_path => "../snapshots"}, {
            assert_debug_snapshot!(tokens);
        });
    }

    #[test]
    fn it_matches_snapshot_de() {
        let tokens = tokenize_recipes("recipes_de.csv", LanguageMode::Fixed(Language::German));

        insta::with_settings!({snapshot_path => "../snapshots"}, {
            assert_debug_snapshot!(tokens);
        });
    }

    #[test]
    fn it_does_not_convert_unicode_when_normalization_disabled() {
        let text = "étude";
        let tokenizer = DefaultTokenizer::builder()
            .language_mode(Language::French)
            .normalization(false)
            .stemming(false)
            .build();

        let tokens = tokenizer.tokenize(text);

        assert_eq!(tokens, vec!["étude"]);
    }

    #[test]
    fn it_does_not_remove_stopwords_when_stopwords_disabled() {
        let text = "i my myself we you have";
        let tokenizer = DefaultTokenizer::builder()
            .language_mode(Language::English)
            .stopwords(false)
            .build();

        let tokens = tokenizer.tokenize(text);

        assert_eq!(tokens, vec!["i", "my", "myself", "we", "you", "have"]);
    }

    #[test]
    fn it_does_not_stem_when_stemming_disabled() {
        let text = "connection connections connective connect";
        let tokenizer = DefaultTokenizer::builder()
            .language_mode(Language::English)
            .stemming(false)
            .build();

        let tokens = tokenizer.tokenize(text);

        assert_eq!(
            tokens,
            vec!["connection", "connections", "connective", "connect"]
        );
    }
}
