//! The installed speech engines, and the rule that keeps installing from
//! racing speaking.
//!
//! One engine per language, built on first use and kept: opening the graphs
//! costs about a quarter of a second, which is most of a short reply's latency.
//! Keeping them is also what makes replacement dangerous — the files under a
//! loaded engine are memory-mapped by onnxruntime, and renaming a directory out
//! from under it is undefined rather than merely stale.
//!
//! So the order is fixed, and it is the whole point of this module:
//!
//! ```text
//! download and verify into .staging   <- no lock; the engine may be speaking
//! unload the engine for this language <- waits for the sentence in flight
//! rename .staging into place          <- microseconds, nothing can speak
//! next synthesis loads the new files  <- lazily, as it always did
//! ```
//!
//! The long part holds nothing. The part that excludes synthesis is a rename.

use std::collections::HashMap;
use std::future::Future;
use std::path::PathBuf;
use std::sync::Arc;

use parking_lot::Mutex;

use super::SpeechCancel;
use super::assets::{self, Asset, InstallError};
use super::pocket::PocketSpeech;

/// Where a voice a language can speak with comes from.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "lowercase")]
pub enum VoiceSource {
    /// Shipped inside the language download.
    Default,
    /// A catalogue voice downloaded on its own.
    Downloaded,
    /// A voice file the user imported.
    User,
}

/// One voice a language can speak with right now.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
pub struct VoiceChoice {
    /// The name a configuration stores and `voice_path` resolves.
    pub id: String,
    pub source: VoiceSource,
}

/// The voices a language can speak with: the one it ships, the catalogue
/// voices that are fully downloaded, then the files the user imported, in
/// that order and each name once.
///
/// A download that is not complete is not offered: choosing it would fail at
/// the first spoken word instead of at the choice. The same holds for the
/// language itself — every voice speaks through its model — so while the
/// language is not fully downloaded the list is empty.
pub fn available_voices(language: &Asset) -> Vec<VoiceChoice> {
    if assets::status(language) != assets::Status::Ready {
        return Vec::new();
    }
    installed_voices(language)
}

/// The voices a language holds, in [`available_voices`]' order, whether or not
/// the language itself is downloaded. A configured voice name is checked
/// against this, so a missing language is reported as missing rather than as a
/// voice that does not exist.
pub fn installed_voices(language: &Asset) -> Vec<VoiceChoice> {
    let Some(name) = language.language() else {
        return Vec::new();
    };
    let mut choices: Vec<VoiceChoice> = language
        .voices()
        .iter()
        .map(|voice| VoiceChoice {
            id: (*voice).to_string(),
            source: VoiceSource::Default,
        })
        .collect();
    for asset in assets::downloadable_voices(name) {
        if assets::status(asset) == assets::Status::Ready
            && let Some(voice) = asset.voice()
        {
            push_once(&mut choices, voice.to_string(), VoiceSource::Downloaded);
        }
    }
    let mut imported: Vec<String> = std::fs::read_dir(assets::user_voices_dir(name))
        .into_iter()
        .flatten()
        .filter_map(|entry| {
            let path = entry.ok()?.path();
            (path.extension()? == "safetensors").then_some(())?;
            let stem = path.file_stem()?.to_str()?.to_string();
            assets::is_voice_name(&stem).then_some(stem)
        })
        .collect();
    imported.sort_unstable();
    for voice in imported {
        push_once(&mut choices, voice, VoiceSource::User);
    }
    choices
}

/// Store a voice file the user chose for a language, after checking that it
/// is one: a voice name, not a catalogue voice's name, not the name of a voice
/// the user already imported for this language, within the size cap,
/// a safetensors file, and made for this language's model
/// ([`PocketSpeech::validate_voice`]). Anything that fails is refused with the
/// reason and nothing is written.
///
/// The file goes to `<speech>/user-voices/<language>/<name>.safetensors`,
/// outside the language directory, so updating the language keeps it. It is
/// written beside its final name and linked into place ([`place_voice_file`]),
/// so a failed write never leaves half a voice that the voice list would offer
/// and a voice already stored under the name is never replaced.
pub fn import_speech_voice(language: &Asset, name: &str, bytes: &[u8]) -> Result<(), String> {
    let Some(language_name) = language.language() else {
        return Err(format!("{} is not a language", language.display_name));
    };
    if !assets::is_voice_name(name) || name.len() > assets::MAX_USER_VOICE_NAME {
        return Err(format!(
            "\"{name}\" is not a voice name: use up to {} letters, digits, - or _",
            assets::MAX_USER_VOICE_NAME
        ));
    }
    let catalogue = language.voices().contains(&name)
        || assets::downloadable_voices(language_name).any(|voice| voice.voice() == Some(name));
    if catalogue {
        return Err(format!(
            "\"{name}\" is the name of a catalogue voice for {}; choose another name",
            language.display_name
        ));
    }
    let dest = assets::user_voices_dir(language_name).join(format!("{name}.safetensors"));
    if dest.exists() {
        // Refused like a catalogue name: storing over it would cost the user
        // the voice file they already have. Checked first so a refused file is
        // never decoded; `place_voice_file` is what makes it hold.
        return Err(already_imported(language, name));
    }
    if bytes.len() > assets::MAX_USER_VOICE_BYTES {
        return Err(format!(
            "the voice file is {} MB; the limit is {} MB",
            bytes.len().div_ceil(1024 * 1024),
            assets::MAX_USER_VOICE_BYTES / (1024 * 1024)
        ));
    }
    PocketSpeech::for_language(language_name)
        .validate_voice(bytes)
        .map_err(|reason| format!("not a voice for {}: {reason}", language.display_name))?;

    let dir = assets::user_voices_dir(language_name);
    std::fs::create_dir_all(&dir).map_err(|e| format!("{}: {e}", dir.display()))?;
    let partial = voice_partial_file(&dir, name).map_err(|e| format!("{}: {e}", dir.display()))?;
    std::fs::write(partial.path(), bytes)
        .map_err(|e| format!("{}: {e}", partial.path().display()))?;
    place_voice_file(partial.path(), &dest).map_err(|error| match error.kind() {
        std::io::ErrorKind::AlreadyExists => already_imported(language, name),
        _ => format!("{}: {error}", dest.display()),
    })
}

/// Create a private staging file beside a voice's destination.
///
/// It must share `dest`'s directory because promotion is an atomic hard link,
/// while its random suffix keeps simultaneous imports of the same name from
/// overwriting each other's validated bytes before either can promote them.
fn voice_partial_file(
    dir: &std::path::Path,
    name: &str,
) -> std::io::Result<tempfile::NamedTempFile> {
    tempfile::Builder::new()
        .prefix(&format!(".{name}."))
        .suffix(".partial")
        .tempfile_in(dir)
}

fn already_imported(language: &Asset, name: &str) -> String {
    format!(
        "You already have a voice called \"{name}\" for {}; delete it first or choose another name",
        language.display_name
    )
}

/// Move a written voice file to its name, never over a file already there.
///
/// A hard link fails with `AlreadyExists` when `dest` is taken, atomically, so
/// a second import of the same name that passed the existence check at the
/// same moment as the first is refused here instead of replacing it — which is
/// what `rename` would do. The partial file is removed either way.
fn place_voice_file(partial: &std::path::Path, dest: &std::path::Path) -> std::io::Result<()> {
    let placed = std::fs::hard_link(partial, dest);
    let _ = std::fs::remove_file(partial);
    placed
}

/// Remove a voice file the user imported. Absent is success, as for every
/// other delete here: the caller asked for it to be gone, and it is.
pub fn delete_speech_voice(language: &Asset, name: &str) -> Result<(), String> {
    let Some(language_name) = language.language() else {
        return Err(format!("{} is not a language", language.display_name));
    };
    if !assets::is_voice_name(name) {
        return Err(format!("\"{name}\" is not a voice name"));
    }
    let path = assets::user_voices_dir(language_name).join(format!("{name}.safetensors"));
    match std::fs::remove_file(&path) {
        Ok(()) => Ok(()),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(e) => Err(format!("{}: {e}", path.display())),
    }
}

fn push_once(choices: &mut Vec<VoiceChoice>, id: String, source: VoiceSource) {
    if !choices.iter().any(|choice| choice.id == id) {
        choices.push(VoiceChoice { id, source });
    }
}

#[derive(Default)]
pub struct SpeechLibrary {
    /// Shared across voices and conversations, independent of engine lifetime.
    pub rejections: Arc<super::rejection::Rejections>,
    /// Built lazily, one per language, dropped on replacement, deletion and
    /// shutdown.
    engines: Mutex<HashMap<String, Arc<PocketSpeech>>>,
    /// The cancel handle of each download in flight, by asset id. A download
    /// that finishes or fails takes its own entry out.
    downloads: Mutex<HashMap<String, SpeechCancel>>,
}

impl SpeechLibrary {
    pub fn new() -> Self {
        Self::default()
    }

    /// The engine for a language, built if this is the first time.
    ///
    /// Construction reads nothing, so this cannot fail and a language that is
    /// not installed becomes a [`super::SpeechError::ModelUnavailable`] at the
    /// first spoken word rather than an error here.
    pub fn engine(&self, language: &str) -> Arc<PocketSpeech> {
        let mut engines = self.engines.lock();
        Arc::clone(
            engines
                .entry(language.to_string())
                .or_insert_with(|| Arc::new(PocketSpeech::for_language(language))),
        )
    }

    /// Drop the loaded graphs for one language, waiting for any synthesis in
    /// flight to finish first.
    ///
    /// The engine handle survives — a caller holding an `Arc` keeps working,
    /// and reloads from disk the next time it speaks.
    pub fn unload(&self, language: &str) {
        let engine = self.engines.lock().get(language).map(Arc::clone);
        if let Some(engine) = engine {
            engine.unload();
        }
    }

    /// Forget every engine. Called when dictation shuts down: an `ort::Session`
    /// holds the whole graph resident, which for one language is about 125 MB.
    pub fn shutdown(&self) {
        // Unload before dropping the handles. Another thread may still hold an
        // `Arc`, in which case dropping ours frees nothing and only `unload`
        // actually releases the memory.
        let engines: Vec<Arc<PocketSpeech>> =
            self.engines.lock().values().map(Arc::clone).collect();
        for engine in engines {
            engine.unload();
        }
        self.engines.lock().clear();
        // Anything still downloading is downloading for a process that is
        // going away.
        for cancel in self.downloads.lock().values() {
            cancel.cancel();
        }
    }

    /// Is a download of this asset in flight?
    pub fn is_downloading(&self, id: &str) -> bool {
        self.downloads.lock().contains_key(id)
    }

    /// Abandon a download in flight. Returns whether there was one — a caller
    /// that cancels a finished download should be told so rather than
    /// answered with a silent success.
    pub fn cancel_download(&self, id: &str) -> bool {
        match self.downloads.lock().get(id) {
            Some(cancel) => {
                cancel.cancel();
                true
            }
            None => false,
        }
    }

    /// Download an asset and put it in place.
    ///
    /// Refuses a second concurrent install of the same asset: two of them would
    /// share one staging directory and each would delete the other's files.
    pub async fn install<P, F, Fut>(
        &self,
        asset: &'static Asset,
        on_progress: P,
        stage: F,
    ) -> Result<PathBuf, InstallError>
    where
        P: Fn(u64, u64) + Send + 'static,
        F: FnOnce(&'static Asset, SpeechCancel, P) -> Fut,
        Fut: Future<Output = Result<PathBuf, InstallError>>,
    {
        let cancel = SpeechCancel::new();
        {
            let mut downloads = self.downloads.lock();
            if downloads.contains_key(asset.id) {
                return Err(InstallError::Disk(format!(
                    "{} is already downloading",
                    asset.display_name
                )));
            }
            downloads.insert(asset.id.to_string(), cancel.clone());
        }

        let staged = stage(asset, cancel.clone(), on_progress).await;
        self.downloads.lock().remove(asset.id);
        let staging = staged?;

        // Only now is anything held. `unload` waits for a sentence in flight;
        // `promote` is two renames.
        if let Some(language) = asset.language() {
            self.unload(language);
        }
        let result = assets::promote(asset, &staging);
        if result.is_err() {
            assets::discard(&staging);
        }
        result
    }

    /// Remove an installed asset, releasing whatever it had loaded first.
    pub fn delete(&self, asset: &Asset) -> Result<(), InstallError> {
        if let Some(language) = asset.language() {
            self.unload(language);
            self.engines.lock().remove(language);
        }
        assets::remove(asset)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::speech::assets::{CATALOGUE, find};
    use crate::speech::pocket::bundle::fixture;

    fn library() -> (tempfile::TempDir, impl Drop, SpeechLibrary) {
        let root = tempfile::tempdir().unwrap();
        let guard = tuic_core::config_dir::set_override(root.path().to_path_buf());
        (root, guard, SpeechLibrary::new())
    }

    #[test]
    fn asking_twice_for_a_language_gives_the_same_engine() {
        // Two engines for one language would each load 125 MB of graphs, and
        // unloading one would leave the other holding files that are about to
        // be renamed away.
        let (_root, _guard, library) = library();
        let first = library.engine("italian");
        let second = library.engine("italian");
        assert!(Arc::ptr_eq(&first, &second));
    }

    #[test]
    fn different_languages_get_different_engines() {
        let (_root, _guard, library) = library();
        let italian = library.engine("italian");
        let english = library.engine("english");
        assert!(!Arc::ptr_eq(&italian, &english));
        assert!(italian.bundle_dir().ends_with("italian"));
        assert!(english.bundle_dir().ends_with("english"));
    }

    #[test]
    fn an_engine_handle_survives_being_unloaded() {
        // The caller in `continuous.rs` holds one for the life of a hands-free
        // session. Invalidating it on every model swap would mean re-fetching
        // it on every reply just in case.
        let (_root, _guard, library) = library();
        let engine = library.engine("italian");
        library.unload("italian");
        assert!(Arc::ptr_eq(&engine, &library.engine("italian")));
    }

    #[test]
    fn unloading_a_language_that_was_never_loaded_does_nothing() {
        let (_root, _guard, library) = library();
        library.unload("italian");
        assert!(!library.is_downloading("italian"));
    }

    #[test]
    fn shutdown_lets_go_of_every_engine() {
        let (_root, _guard, library) = library();
        let engine = library.engine("italian");
        library.shutdown();
        // The map is empty, so the next `engine` call builds a new one rather
        // than handing back the old handle.
        assert!(!Arc::ptr_eq(&engine, &library.engine("italian")));
    }

    #[test]
    fn deleting_a_language_forgets_its_engine_as_well_as_its_files() {
        // Keeping the engine would leave a handle pointing at a directory that
        // no longer exists, and the next reply would fail with a missing file
        // instead of reporting the language as not installed.
        let (_root, _guard, library) = library();
        let engine = library.engine("italian");
        let asset = find("italian").unwrap();
        library.delete(asset).unwrap();
        assert!(!Arc::ptr_eq(&engine, &library.engine("italian")));
    }

    #[test]
    fn cancelling_a_download_nobody_started_says_so() {
        let (_root, _guard, library) = library();
        assert!(!library.cancel_download("italian"));
    }

    /// Put a file where `promote` would, at the size the catalogue pins.
    fn install_voice(asset: &Asset) {
        for file in asset.installed_files() {
            let path = asset.install_dir().join(file.name);
            std::fs::create_dir_all(path.parent().unwrap()).unwrap();
            std::fs::File::create(path)
                .unwrap()
                .set_len(file.size_bytes.unwrap_or(1))
                .unwrap();
        }
    }

    /// A voice file the user imported for a language.
    fn user_voice(language: &str, name: &str) {
        let dir = assets::user_voices_dir(language);
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join(format!("{name}.safetensors")), b"x").unwrap();
    }

    fn italian() -> &'static Asset {
        find("italian").unwrap()
    }

    #[test]
    fn available_voices_offer_nothing_while_the_language_is_not_downloaded() {
        // Every voice speaks through the language's model, so without it no
        // voice can say a word: not the one it ships, not a downloaded one,
        // not an imported one.
        let (_root, _guard, _library) = library();
        install_voice(find("voice-italian-jean").unwrap());
        user_voice("italian", "nonna");
        assert_eq!(available_voices(italian()), Vec::new());
    }

    #[test]
    fn available_voices_offer_nothing_while_the_language_download_is_incomplete() {
        let (_root, _guard, _library) = library();
        installed_italian();
        assert_eq!(available_voices(italian()), Vec::new());
    }

    #[test]
    fn installed_voices_name_every_voice_the_language_holds_even_before_it_is_downloaded() {
        // What `choose_voice` checks a configured name against: a name that
        // is on disk is known, and the missing language is reported where the
        // engine opens.
        let (_root, _guard, _library) = library();
        user_voice("italian", "nonna");
        let ids: Vec<String> = installed_voices(italian())
            .into_iter()
            .map(|v| v.id)
            .collect();
        assert_eq!(ids, vec!["giovanni".to_string(), "nonna".to_string()]);
    }

    #[test]
    fn available_voices_start_with_the_one_the_language_ships() {
        // The language downloaded, nothing else: it speaks in its own voice.
        let (_root, _guard, _library) = library();
        install_voice(italian());
        assert_eq!(
            available_voices(italian()),
            vec![VoiceChoice {
                id: "giovanni".into(),
                source: VoiceSource::Default
            }]
        );
    }

    #[test]
    fn available_voices_add_downloaded_and_user_voices_of_that_language_only() {
        let (_root, _guard, _library) = library();
        install_voice(italian());
        install_voice(find("voice-italian-jean").unwrap());
        install_voice(find("voice-german-vera").unwrap());
        user_voice("italian", "nonna");
        user_voice("german", "oma");

        assert_eq!(
            available_voices(italian()),
            vec![
                VoiceChoice {
                    id: "giovanni".into(),
                    source: VoiceSource::Default
                },
                VoiceChoice {
                    id: "jean".into(),
                    source: VoiceSource::Downloaded
                },
                VoiceChoice {
                    id: "nonna".into(),
                    source: VoiceSource::User
                },
            ]
        );
    }

    #[test]
    fn available_voices_skip_a_download_that_is_not_complete() {
        // A voice file of the wrong size is a download that died: offering it
        // would fail at the first word instead of at the choice.
        let (_root, _guard, _library) = library();
        install_voice(italian());
        let jean = find("voice-italian-jean").unwrap();
        std::fs::create_dir_all(jean.install_dir()).unwrap();
        std::fs::write(jean.install_dir().join("jean.safetensors"), b"truncated").unwrap();
        assert_eq!(available_voices(italian()).len(), 1);
    }

    #[test]
    fn available_voices_ignore_files_that_are_not_voices() {
        let (_root, _guard, _library) = library();
        install_voice(italian());
        let dir = assets::user_voices_dir("italian");
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("notes.txt"), b"x").unwrap();
        std::fs::write(dir.join("bad name.safetensors"), b"x").unwrap();
        assert_eq!(available_voices(italian()).len(), 1);
    }

    /// Italian "installed" with a 6-layer manifest, which is all an import
    /// reads from the language.
    fn installed_italian() {
        let dir = italian().install_dir();
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("bundle.json"), fixture::bundle_json(6)).unwrap();
    }

    fn imported(name: &str) -> PathBuf {
        assets::user_voices_dir("italian").join(format!("{name}.safetensors"))
    }

    #[test]
    fn concurrent_imports_stage_same_named_voices_in_distinct_files() {
        let (_root, _guard, _library) = library();
        let dir = assets::user_voices_dir("italian");
        std::fs::create_dir_all(&dir).unwrap();

        let first = voice_partial_file(&dir, "nonna").unwrap();
        let second = voice_partial_file(&dir, "nonna").unwrap();
        std::fs::write(first.path(), b"first").unwrap();
        std::fs::write(second.path(), b"second").unwrap();

        assert_ne!(first.path(), second.path());
        assert_eq!(std::fs::read(first.path()).unwrap(), b"first");
        assert_eq!(std::fs::read(second.path()).unwrap(), b"second");

        // Interleave promotion after both imports have staged their validated
        // bytes: one wins the name, and the loser's bytes never reach it.
        let destination = dir.join("nonna.safetensors");
        place_voice_file(second.path(), &destination).unwrap();
        assert_eq!(
            place_voice_file(first.path(), &destination)
                .unwrap_err()
                .kind(),
            std::io::ErrorKind::AlreadyExists
        );
        assert_eq!(std::fs::read(destination).unwrap(), b"second");
    }

    #[test]
    fn import_speech_voice_stores_a_valid_voice_and_offers_it_as_the_users() {
        let (_root, _guard, _library) = library();
        installed_italian();
        let bytes = fixture::voice_bytes(6, 4, None);
        assert_eq!(import_speech_voice(italian(), "nonna", &bytes), Ok(()));
        assert_eq!(std::fs::read(imported("nonna")).unwrap(), bytes);
        // `installed_voices`, not `available_voices`: the fixture manifest is
        // not the pinned download, so the language never reads as ready here.
        assert!(installed_voices(italian()).contains(&VoiceChoice {
            id: "nonna".into(),
            source: VoiceSource::User
        }));
    }

    #[test]
    fn import_speech_voice_refuses_a_name_that_is_not_a_voice_name() {
        let (_root, _guard, _library) = library();
        installed_italian();
        let bytes = fixture::voice_bytes(6, 4, None);
        for name in ["", "../nonna", "no spaces", "a.b", &"x".repeat(33)] {
            assert!(
                import_speech_voice(italian(), name, &bytes).is_err(),
                "{name:?}"
            );
        }
        assert!(import_speech_voice(italian(), &"x".repeat(32), &bytes).is_ok());
    }

    #[test]
    fn import_speech_voice_refuses_the_name_of_a_catalogue_voice() {
        // It would be shadowed by the shipped or downloaded voice of that name
        // and never heard, or shadow a download the user makes later.
        let (_root, _guard, _library) = library();
        installed_italian();
        let bytes = fixture::voice_bytes(6, 4, None);
        for name in ["giovanni", "jean"] {
            let error = import_speech_voice(italian(), name, &bytes).unwrap_err();
            assert!(error.contains(name), "{error}");
        }
    }

    #[test]
    fn import_speech_voice_refuses_a_file_over_the_size_cap() {
        let (_root, _guard, _library) = library();
        installed_italian();
        let bytes = vec![0u8; assets::MAX_USER_VOICE_BYTES + 1];
        let error = import_speech_voice(italian(), "nonna", &bytes).unwrap_err();
        assert!(error.contains("MB"), "{error}");
        assert!(!imported("nonna").exists());
    }

    #[test]
    fn import_speech_voice_refuses_what_is_not_a_voice_of_this_model_and_stores_nothing() {
        let (_root, _guard, _library) = library();
        installed_italian();
        let cases = [
            ("not safetensors", b"hello".to_vec()),
            ("24-layer voice", fixture::voice_bytes(24, 4, None)),
            ("extra pad tensor", fixture::voice_bytes(6, 4, Some("pad"))),
        ];
        for (what, bytes) in cases {
            assert!(
                import_speech_voice(italian(), "nonna", &bytes).is_err(),
                "{what}"
            );
            assert!(!imported("nonna").exists(), "{what} was stored");
        }
    }

    #[test]
    fn import_speech_voice_refuses_a_name_the_user_already_imported_and_keeps_the_file() {
        // Adding a file must never cost the user one they already have: the
        // same name is refused, as the catalogue names are, rather than
        // renamed over the stored voice.
        let (_root, _guard, _library) = library();
        installed_italian();
        let first = fixture::voice_bytes(6, 4, None);
        import_speech_voice(italian(), "nonna", &first).unwrap();

        let second = fixture::voice_bytes(6, 5, None);
        assert_ne!(first, second);
        let error = import_speech_voice(italian(), "nonna", &second).unwrap_err();
        assert!(error.contains("nonna"), "{error}");
        assert!(error.contains("delete"), "{error}");
        assert_eq!(std::fs::read(imported("nonna")).unwrap(), first);

        // Once the user deletes it, the name is free again.
        delete_speech_voice(italian(), "nonna").unwrap();
        assert_eq!(import_speech_voice(italian(), "nonna", &second), Ok(()));
        assert_eq!(std::fs::read(imported("nonna")).unwrap(), second);
    }

    #[test]
    fn placing_a_voice_file_never_replaces_one_that_appeared_after_the_check() {
        // Two imports of one new name can both pass the existence check; the
        // second to reach the move must lose, not overwrite the first.
        let (_root, _guard, _library) = library();
        let dir = assets::user_voices_dir("italian");
        std::fs::create_dir_all(&dir).unwrap();
        let dest = dir.join("nonna.safetensors");
        let partial = dir.join(".nonna.partial");
        std::fs::write(&dest, b"first").unwrap();
        std::fs::write(&partial, b"second").unwrap();

        let error = place_voice_file(&partial, &dest).unwrap_err();
        assert_eq!(error.kind(), std::io::ErrorKind::AlreadyExists);
        assert_eq!(std::fs::read(&dest).unwrap(), b"first");
        assert!(!partial.exists(), "the refused partial file is left behind");
    }

    #[test]
    fn placing_a_voice_file_moves_it_to_a_free_name() {
        let (_root, _guard, _library) = library();
        let dir = assets::user_voices_dir("italian");
        std::fs::create_dir_all(&dir).unwrap();
        let dest = dir.join("nonna.safetensors");
        let partial = dir.join(".nonna.partial");
        std::fs::write(&partial, b"voice").unwrap();
        place_voice_file(&partial, &dest).unwrap();
        assert_eq!(std::fs::read(&dest).unwrap(), b"voice");
        assert!(!partial.exists());
    }

    #[test]
    fn import_speech_voice_accepts_a_name_another_language_already_uses() {
        let (_root, _guard, _library) = library();
        installed_italian();
        user_voice("german", "nonna");
        let bytes = fixture::voice_bytes(6, 4, None);
        assert_eq!(import_speech_voice(italian(), "nonna", &bytes), Ok(()));
    }

    #[test]
    fn import_speech_voice_refusals_read_correctly_for_a_language_that_starts_with_a_vowel() {
        // "a Italian voice" is what a hard-coded article produced; English is
        // the other catalogue language it breaks for.
        let (_root, _guard, _library) = library();
        let bytes = fixture::voice_bytes(6, 4, None);
        for language in [italian(), find("english").unwrap()] {
            let shipped = language.voices()[0];
            let errors = [
                import_speech_voice(language, "nonna", b"hello").unwrap_err(),
                import_speech_voice(language, shipped, &bytes).unwrap_err(),
            ];
            for error in errors {
                assert!(error.contains(language.display_name), "{error}");
                assert!(
                    !error.contains(&format!("a {}", language.display_name)),
                    "{error}"
                );
            }
        }
    }

    #[test]
    fn import_speech_voice_needs_the_language_downloaded_first() {
        // The check is against the language's own manifest.
        let (_root, _guard, _library) = library();
        let bytes = fixture::voice_bytes(6, 4, None);
        let error = import_speech_voice(italian(), "nonna", &bytes).unwrap_err();
        assert!(error.to_lowercase().contains("download"), "{error}");
    }

    #[test]
    fn delete_speech_voice_removes_the_file_and_is_idempotent() {
        let (_root, _guard, _library) = library();
        installed_italian();
        import_speech_voice(italian(), "nonna", &fixture::voice_bytes(6, 4, None)).unwrap();
        assert_eq!(delete_speech_voice(italian(), "nonna"), Ok(()));
        assert!(!imported("nonna").exists());
        assert_eq!(delete_speech_voice(italian(), "nonna"), Ok(()));
        assert!(delete_speech_voice(italian(), "../giovanni").is_err());
    }

    #[test]
    fn every_catalogue_asset_can_be_deleted_when_it_is_not_installed() {
        // Delete is the recovery path for a half-finished download, so it has
        // to work on every asset in every state, including "no directory".
        let (_root, _guard, library) = library();
        for asset in CATALOGUE {
            assert_eq!(library.delete(asset), Ok(()), "{}", asset.id);
        }
    }
}
