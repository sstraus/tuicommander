//! The bundle's own SentencePiece tokenizer, and the text preparation around it.
//!
//! The model was trained on text that went through SentencePiece, so anything
//! that differs here — a missing dummy prefix, a byte-fallback piece rebuilt
//! wrongly, a chunk split mid-word — reaches the model as a different sentence
//! and comes back as different speech. The port is therefore checked against
//! the reference implementation rather than merely against itself; see
//! `ids_match_the_reference_tokenizer` at the bottom of this file.
//!
//! The C++ `sentencepiece` crate cannot be used to do that here: it statically
//! links protobuf 3.14 while onnxruntime links 3.21, and the two abort the
//! process on first use ("This program was compiled against version 3.14.0 of
//! the Protocol Buffer runtime library"). So the model proto is read in pure
//! Rust and the Viterbi is run by `tokenizers`.

use std::path::Path;

use sentencepiece_model::SentencePieceModel;
use tokenizers::Model;
use tokenizers::models::unigram::Unigram;

use super::bundle::Bundle;
use super::{Result, failed, unavailable};

/// SentencePiece escapes a space as U+2581 and prepends one to every input.
const SPACE: char = '\u{2581}';

pub struct Tokenizer {
    model: Unigram,
}

impl Tokenizer {
    pub fn open(path: &Path) -> Result<Self> {
        let spm =
            SentencePieceModel::from_file(path).map_err(|e| unavailable(path, format!("{e}")))?;
        let vocab: Vec<(String, f64)> = spm
            .pieces()
            .iter()
            .map(|piece| (piece.piece().to_string(), f64::from(piece.score())))
            .collect();
        let trainer = spm.trainer();
        let unk_id = trainer.map_or(0, |t| t.unk_id()).max(0) as usize;
        let byte_fallback = trainer.is_some_and(sentencepiece_model::TrainerSpec::byte_fallback);
        Ok(Self {
            model: Self::build(vocab, unk_id, byte_fallback)?,
        })
    }

    fn build(vocab: Vec<(String, f64)>, unk_id: usize, byte_fallback: bool) -> Result<Unigram> {
        Unigram::from(vocab, Some(unk_id), byte_fallback)
            .map_err(|e| failed(format!("building the unigram model: {e}")))
    }

    pub fn encode(&self, text: &str) -> Result<Vec<u32>> {
        let escaped = format!("{SPACE}{}", text.replace(' ', &SPACE.to_string()));
        Ok(self
            .model
            .tokenize(&escaped)
            .map_err(|e| failed(format!("tokenizing: {e}")))?
            .into_iter()
            .map(|token| token.id)
            .collect())
    }

    /// The inverse, including the byte-fallback pieces the encoder emits for
    /// anything outside the 4000-piece vocabulary.
    ///
    /// Those pieces are `<0xNN>` and a single accented character is several of
    /// them in a row, so they are accumulated and decoded as one UTF-8 run —
    /// converting each in isolation yields replacement characters.
    pub fn decode(&self, ids: &[u32]) -> String {
        let mut out = String::new();
        let mut bytes: Vec<u8> = Vec::new();
        for id in ids {
            let Some(piece) = self.model.id_to_token(*id) else {
                continue;
            };
            if let Some(byte) = piece
                .strip_prefix("<0x")
                .and_then(|rest| rest.strip_suffix('>'))
                .and_then(|hex| u8::from_str_radix(hex, 16).ok())
            {
                bytes.push(byte);
                continue;
            }
            if !bytes.is_empty() {
                out.push_str(&String::from_utf8_lossy(&bytes));
                bytes.clear();
            }
            out.push_str(&piece);
        }
        if !bytes.is_empty() {
            out.push_str(&String::from_utf8_lossy(&bytes));
        }
        let out = out.replace(SPACE, " ");
        out.strip_prefix(' ').unwrap_or(&out).to_string()
    }
}

/// Normalise a chunk the way the model was trained to receive it, and say how
/// many frames of silence it needs after the end-of-speech signal.
///
/// A very short utterance needs more: the model reaches its EOS logit before
/// the final consonant has finished decoding, so cutting at the signal clips
/// the last word.
pub fn prepare_text(bundle: &Bundle, text: &str) -> Result<(String, usize)> {
    let mut text = text.trim().to_string();
    if text.is_empty() {
        return Err(failed("text cannot be empty".to_string()));
    }
    text = text.replace(['\n', '\r'], " ").replace("  ", " ");
    if bundle.remove_semicolons {
        text = text.replace(';', ",");
    }

    let frames_after_eos = if text.split_whitespace().count() <= 4 {
        3
    } else {
        1
    };

    let mut chars = text.chars();
    if let Some(first) = chars.next()
        && !first.is_uppercase()
    {
        text = first.to_uppercase().collect::<String>() + chars.as_str();
    }
    if text.chars().last().is_some_and(char::is_alphanumeric) {
        text.push('.');
    }
    if bundle.pad_with_spaces_for_short_inputs && text.split_whitespace().count() < 5 {
        text = " ".repeat(8) + &text;
    }
    Ok((text, frames_after_eos))
}

/// Where each run of boundary tokens ends, as indices into `tokens`.
///
/// Returned as cut points including 0 and the end, so consecutive pairs are the
/// segments. A run of several boundary tokens ("...", "?!") is one cut, and
/// the punctuation stays attached to the segment it closes.
fn boundary_indices(tokens: &[u32], boundaries: &[u32]) -> Vec<usize> {
    let mut indices = vec![0usize];
    let mut previous_was_boundary = false;
    for (index, token) in tokens.iter().enumerate() {
        if boundaries.contains(token) {
            previous_was_boundary = true;
        } else {
            if previous_was_boundary {
                indices.push(index);
            }
            previous_was_boundary = false;
        }
    }
    indices.push(tokens.len());
    indices
}

fn segments(tokenizer: &Tokenizer, tokens: &[u32], indices: &[usize]) -> Vec<(usize, String)> {
    indices
        .windows(2)
        .map(|pair| {
            (
                pair[1] - pair[0],
                tokenizer.decode(&tokens[pair[0]..pair[1]]),
            )
        })
        .collect()
}

/// Split into chunks the model can hold, at sentence ends first and at commas
/// when a single sentence is still too long.
///
/// The limit is the bundle's `max_token_per_chunk`. Past it the transformer's
/// attention window is exhausted and the tail of the sentence degrades, so this
/// is a correctness bound rather than a performance one.
pub fn split_into_chunks(
    bundle: &Bundle,
    tokenizer: &Tokenizer,
    text: &str,
) -> Result<Vec<String>> {
    let (prepared, _) = prepare_text(bundle, text)?;
    let tokens = tokenizer.encode(prepared.trim())?;

    // The leading id is the dummy-prefix piece, not punctuation.
    let sentence_ends = tokenizer.encode(".!...?")?[1..].to_vec();
    let coarse = segments(
        tokenizer,
        &tokens,
        &boundary_indices(&tokens, &sentence_ends),
    );

    let fallback = tokenizer.encode(",;:")?[1..].to_vec();
    let mut refined = Vec::new();
    for (count, segment_text) in coarse {
        if count <= bundle.max_token_per_chunk {
            refined.push((count, segment_text));
            continue;
        }
        let sub_tokens = tokenizer.encode(segment_text.trim())?;
        let sub = segments(
            tokenizer,
            &sub_tokens,
            &boundary_indices(&sub_tokens, &fallback),
        );
        if sub.len() > 1 {
            refined.extend(sub);
        } else {
            // Neither sentence ends nor commas split it. Overrunning the window
            // degrades the tail; splitting mid-word is worse.
            refined.push((count, segment_text));
        }
    }

    Ok(merge_segments(refined, bundle.max_token_per_chunk))
}

/// Pack consecutive segments back together up to the token limit, so a short
/// sentence does not become a chunk of its own with its own leading breath.
fn merge_segments(segments: Vec<(usize, String)>, limit: usize) -> Vec<String> {
    let mut chunks: Vec<String> = Vec::new();
    let mut current = String::new();
    let mut current_count = 0usize;
    for (count, text) in segments {
        if current.is_empty() {
            current = text;
            current_count = count;
        } else if current_count + count > limit {
            chunks.push(current.trim().to_string());
            current = text;
            current_count = count;
        } else {
            current.push(' ');
            current.push_str(&text);
            current_count += count;
        }
    }
    if !current.trim().is_empty() {
        chunks.push(current.trim().to_string());
    }
    chunks
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::speech::SpeechError;

    fn bundle() -> Bundle {
        Bundle {
            language: "italian".into(),
            sample_rate: 24_000,
            frame_rate: 12.5,
            latent_dim: 32,
            conditioning_dim: 1024,
            tokenizer_file: "tokenizer.model".into(),
            max_token_per_chunk: 50,
            model_recommended_frames_after_eos: None,
            pad_with_spaces_for_short_inputs: false,
            remove_semicolons: false,
            flow_lm_state_manifest: Vec::new(),
            mimi_state_manifest: Vec::new(),
        }
    }

    /// A stand-in vocabulary. Small enough to reason about, and it exercises
    /// the two things the real one does that a plain word list does not: the
    /// U+2581 space escape and `<0xNN>` byte fallback.
    fn toy() -> Tokenizer {
        let mut vocab: Vec<(String, f64)> = vec![
            ("<unk>".to_string(), 0.0),
            (format!("{SPACE}ciao"), -1.0),
            (format!("{SPACE}a"), -2.0),
            ("a".to_string(), -3.0),
            ("c".to_string(), -4.0),
            ("i".to_string(), -4.0),
            ("o".to_string(), -4.0),
            (SPACE.to_string(), -5.0),
        ];
        for byte in 0u16..=255 {
            vocab.push((format!("<0x{byte:02X}>"), -20.0));
        }
        Tokenizer {
            model: Tokenizer::build(vocab, 0, true).unwrap(),
        }
    }

    #[test]
    fn decoding_reassembles_a_multi_byte_character_from_its_byte_pieces() {
        // "à" is two UTF-8 bytes and falls outside the vocabulary, so the
        // encoder emits <0xC3><0xA0>. Decoding them one at a time yields two
        // replacement characters instead of one accented letter — and Italian
        // is full of them.
        let tokenizer = toy();
        let ids = tokenizer.encode("città").unwrap();
        assert_eq!(tokenizer.decode(&ids), "città");
    }

    #[test]
    fn decoding_restores_spaces_and_drops_the_dummy_prefix() {
        let tokenizer = toy();
        let ids = tokenizer.encode("ciao ciao").unwrap();
        assert_eq!(tokenizer.decode(&ids), "ciao ciao");
    }

    #[test]
    fn encoding_escapes_spaces_so_a_word_start_is_distinguishable() {
        // "▁a" and "a" are different pieces: one starts a word, the other
        // continues one. Skipping the escape merges them and the model reads a
        // different sentence than the one it was given.
        let tokenizer = toy();
        let leading = tokenizer.encode("a").unwrap();
        let inner = tokenizer.model.tokenize("a").unwrap();
        assert_ne!(leading[0], inner[0].id);
    }

    #[test]
    fn an_id_outside_the_vocabulary_is_skipped_rather_than_panicking() {
        // Ids come from the model's own output in the chunking path; a bundle
        // whose vocabulary disagrees must not take the process down.
        let tokenizer = toy();
        assert_eq!(tokenizer.decode(&[999_999]), "");
    }

    #[test]
    fn preparation_capitalises_and_terminates_the_sentence() {
        // The model was trained on written sentences. An uncapitalised,
        // unterminated fragment is read with the wrong prosody.
        let (text, _) = prepare_text(&bundle(), "ho lasciato le chiavi sul tavolo").unwrap();
        assert_eq!(text, "Ho lasciato le chiavi sul tavolo.");
    }

    #[test]
    fn preparation_keeps_punctuation_the_writer_already_supplied() {
        let (text, _) = prepare_text(&bundle(), "Hai visto le chiavi?").unwrap();
        assert_eq!(text, "Hai visto le chiavi?");
    }

    #[test]
    fn preparation_flattens_newlines_a_model_reply_may_contain() {
        let (text, _) = prepare_text(&bundle(), "Primo.\nSecondo.\r\nTerzo.").unwrap();
        assert_eq!(text, "Primo. Secondo. Terzo.");
    }

    #[test]
    fn a_short_utterance_asks_for_more_frames_after_the_end_signal() {
        // The model hits its EOS logit before a two-word reply has finished
        // decoding; cutting at the signal clips the last consonant.
        let (_, short) = prepare_text(&bundle(), "Va bene").unwrap();
        let (_, long) = prepare_text(
            &bundle(),
            "Ho lasciato le chiavi sul tavolo della cucina ieri sera",
        )
        .unwrap();
        assert!(short > long, "short={short} long={long}");
    }

    #[test]
    fn empty_text_is_refused_instead_of_synthesised() {
        assert_eq!(
            prepare_text(&bundle(), "   \n  ").unwrap_err(),
            SpeechError::Failed("text cannot be empty".into())
        );
    }

    #[test]
    fn preparation_pads_a_short_input_only_when_the_bundle_asks_for_it() {
        let mut padding = bundle();
        padding.pad_with_spaces_for_short_inputs = true;
        let (padded, _) = prepare_text(&padding, "Va bene").unwrap();
        assert!(padded.starts_with("        "), "{padded:?}");

        let (plain, _) = prepare_text(&bundle(), "Va bene").unwrap();
        assert_eq!(plain, "Va bene.");
    }

    #[test]
    fn boundaries_cut_after_a_run_of_punctuation_not_inside_it() {
        // "..." is three boundary tokens. Cutting at each one would produce two
        // empty segments and a chunk that starts with a full stop.
        let tokens = vec![10, 11, 9, 9, 9, 12, 13, 9];
        let cuts = boundary_indices(&tokens, &[9]);
        assert_eq!(cuts, vec![0, 5, 8]);
    }

    #[test]
    fn text_with_no_boundary_at_all_is_one_segment() {
        let tokens = vec![10, 11, 12];
        assert_eq!(boundary_indices(&tokens, &[9]), vec![0, 3]);
    }

    #[test]
    fn short_sentences_are_packed_together_up_to_the_limit() {
        // One chunk per sentence would give each its own leading breath, which
        // is audible as a stutter between them.
        let merged = merge_segments(
            vec![
                (10, "Primo.".into()),
                (10, "Secondo.".into()),
                (40, "Terzo molto piu lungo.".into()),
            ],
            50,
        );
        assert_eq!(merged, vec!["Primo. Secondo.", "Terzo molto piu lungo."]);
    }

    #[test]
    fn a_segment_that_alone_exceeds_the_limit_still_becomes_its_own_chunk() {
        let merged = merge_segments(vec![(80, "Una frase lunghissima.".into())], 50);
        assert_eq!(merged, vec!["Una frase lunghissima."]);
    }

    #[test]
    fn merging_nothing_produces_no_chunks() {
        assert_eq!(
            merge_segments(Vec::new(), 50),
            [] as [std::string::String; 0]
        );
    }

    /// Parity against the reference implementation.
    ///
    /// The ids below were produced by Python `sentencepiece` reading the same
    /// `tokenizer.model`, and cover what the port can get wrong: elisions, the
    /// accented characters that go through byte fallback, and digits. A
    /// mismatch means the model is being handed a different sentence.
    ///
    /// Ignored because it needs a downloaded bundle, like the whisper-model
    /// tests. Run it with the **Italian** bundle directory in
    /// `TUIC_POCKET_BUNDLE_DIR`: every bundle carries its own vocabulary, so
    /// these ids mean nothing against another language's tokenizer.
    #[test]
    #[ignore = "needs the downloaded Italian Pocket TTS bundle: set TUIC_POCKET_BUNDLE_DIR"]
    fn ids_match_the_reference_tokenizer() {
        let dir = std::path::PathBuf::from(
            std::env::var("TUIC_POCKET_BUNDLE_DIR")
                .expect("TUIC_POCKET_BUNDLE_DIR must point at a bundle directory"),
        );
        let bundle = Bundle::load(&dir).unwrap();
        assert_eq!(
            bundle.language, "italian",
            "these ids are the Italian vocabulary's; TUIC_POCKET_BUNDLE_DIR points at {:?}",
            bundle.language
        );
        let tokenizer = Tokenizer::open(&dir.join(&bundle.tokenizer_file)).unwrap();

        let expected: [(&str, &[u32]); 2] = [
            (
                "Ho lasciato le chiavi sull'erba bagnata vicino all'acqua.",
                &[
                    1036, 949, 344, 288, 431, 279, 390, 1116, 267, 680, 700, 804, 878, 289, 260,
                    1697, 376, 267, 1323, 263,
                ],
            ),
            (
                "Città, qualità, perché, però, più.",
                &[
                    744, 397, 618, 261, 333, 392, 261, 260, 299, 261, 374, 261, 301, 263,
                ],
            ),
        ];
        for (text, ids) in expected {
            assert_eq!(tokenizer.encode(text).unwrap(), ids, "for {text:?}");
        }
    }
}
