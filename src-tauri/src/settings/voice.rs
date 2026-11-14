//! Voice owner seam: `dictation-config.json` is written only through `ConfigFile`, and a save
//! refreshes the running voice through the same `refresh_voice_runtime` as `save_dictation_config`,
//! so no writer can leave the old voice speaking or a stale model snapshot cached.
use super::{config_file::ConfigFileStore, request::*, service::Store, transaction::Prepared};
use crate::config::{ConfigFile, DictationConfig};
use crate::dictation::{
    commands::{DICTATION_CONFIG_FILE, refresh_voice_runtime},
    speaker,
};
use parking_lot::Mutex;
use serde_json::Value;
use std::{path::PathBuf, sync::Arc};

const ID: &str = "voice";

/// Fields that make a queued reply wrong; keep in step with `save_dictation_config`.
const VOICE_FIELDS: [&str; 5] = [
    "voice.language",
    "voice.speech_engine",
    "voice.speech_command",
    "voice.speech_voice",
    "voice.speech_edge_voice",
];

/// Asset operations retain the existing owner's allowlisted IDs and validation.
/// No caller-provided path, URL or executable is accepted.
#[derive(serde::Deserialize, schemars::JsonSchema)]
#[serde(tag = "action", rename_all = "snake_case", deny_unknown_fields)]
pub enum VoiceAction {
    Assets,
    DownloadModel {
        model_name: String,
    },
    DeleteModel {
        model_name: String,
    },
    DownloadAsset {
        asset: String,
    },
    CancelDownload {
        asset: String,
    },
    DeleteAsset {
        asset: String,
    },
    ImportVoice {
        language: String,
        name: String,
        data_base64: String,
    },
    DeleteVoice {
        language: String,
        name: String,
    },
}

pub struct VoiceSettingsStore {
    path: PathBuf,
    /// The live speaker slot of `DictationState`; `None` before startup has one.
    speaker: Option<Arc<Mutex<Option<speaker::Armed>>>>,
}

impl VoiceSettingsStore {
    pub fn new(speaker: Option<Arc<Mutex<Option<speaker::Armed>>>>) -> Self {
        Self {
            path: crate::config::config_dir().join(DICTATION_CONFIG_FILE),
            speaker,
        }
    }

    #[cfg(test)]
    pub(crate) fn at_path(
        path: PathBuf,
        speaker: Option<Arc<Mutex<Option<speaker::Armed>>>>,
    ) -> Self {
        Self { path, speaker }
    }

    /// Called by host transport wiring with its authenticated caller context and app state.
    pub async fn perform(
        &self,
        app: &tauri::AppHandle,
        caller: &super::policy::Caller,
        action: VoiceAction,
    ) -> Result<Value, Failure> {
        use crate::dictation::{DictationState, commands};
        use tauri::Manager;
        caller.check()?;
        // The asset commands own the live runtime and machine asset directory. A
        // path-scoped fixture must not accidentally operate on that live owner.
        if self.path != crate::config::config_dir().join(DICTATION_CONFIG_FILE) {
            return Err(Error::new(ID, "not_found"));
        }
        let result = match action {
            VoiceAction::Assets => {
                return choice_value(commands::get_speech_assets(app.state::<DictationState>()));
            }
            VoiceAction::DownloadModel { model_name } => {
                commands::download_whisper_model(app.clone(), model_name).await
            }
            VoiceAction::DeleteModel { model_name } => {
                commands::delete_whisper_model(app.state::<DictationState>(), model_name)
            }
            VoiceAction::DownloadAsset { asset } => {
                commands::download_speech_asset(app.clone(), asset).await
            }
            VoiceAction::CancelDownload { asset } => {
                commands::cancel_speech_download(app.state::<DictationState>(), asset)
            }
            VoiceAction::DeleteAsset { asset } => {
                commands::delete_speech_asset(app.state::<DictationState>(), asset)
            }
            VoiceAction::ImportVoice {
                language,
                name,
                data_base64,
            } => commands::import_speech_voice(language, name, data_base64),
            VoiceAction::DeleteVoice { language, name } => {
                commands::delete_speech_voice(language, name)
            }
        };
        result
            .map(Value::String)
            .map_err(|_| Error::new(ID, "unavailable"))
    }

    fn file(&self) -> ConfigFile<DictationConfig> {
        ConfigFile::at_path(self.path.clone())
    }
}

/// Strict, lock-taking read: a corrupt file is an error, never silently the defaults.
fn read_strict(file: &ConfigFile<DictationConfig>) -> Result<DictationConfig, Failure> {
    file.update_with_strict(|config| Ok((config.clone(), false)))
        .map_err(|_| Error::new(ID, "store_failed"))
}

impl Store for VoiceSettingsStore {
    fn id(&self) -> &str {
        ID
    }
    fn target(&self) -> Target {
        Target::Machine
    }

    fn read(&self) -> Result<Value, Failure> {
        ConfigFileStore::new(ID, self.file()).read()
    }

    async fn refresh_choices(&self, resolver: &str, _path: &str) -> Result<Vec<Value>, Failure> {
        use crate::dictation::commands;
        if resolver == "audio_inputs" {
            return commands::list_audio_devices()
                .await
                .map_err(|_| Error::new(ID, "unavailable"))?
                .into_iter()
                .map(choice_value)
                .collect();
        }
        if resolver == "edge_voices" {
            let file = self.file();
            let language =
                tokio::task::spawn_blocking(move || read_strict(&file).map(|c| c.language))
                    .await
                    .map_err(|_| Error::new(ID, "store_failed"))??;
            return commands::get_edge_voices(language)
                .await
                .map_err(|_| Error::new(ID, "unavailable"))?
                .into_iter()
                .map(choice_value)
                .collect();
        }
        let file = self.file();
        let resolver = resolver.to_owned();
        tokio::task::spawn_blocking(move || match resolver.as_str() {
            "voice_models" => commands::get_model_info()
                .into_iter()
                .map(choice_value)
                .collect(),
            "installed_voices" => commands::get_speech_voices(read_strict(&file)?.language)
                .map_err(|_| Error::new(ID, "unavailable"))?
                .into_iter()
                .map(choice_value)
                .collect(),
            _ => Err(Error::new(ID, "unavailable")),
        })
        .await
        .map_err(|_| Error::new(ID, "store_failed"))?
    }

    fn commit(
        &self,
        prepare: &mut dyn FnMut(&Value) -> Result<Prepared, Failure>,
    ) -> Result<CommitReply, Failure> {
        ConfigFileStore::new(ID, self.file()).commit(prepare, |_, next| {
            // A read-response diagnostic, never persisted (as in `save_dictation_config`).
            next.recovered_from_corruption = false;
        })
    }

    fn apply(
        &self,
        reply: &CommitReply,
    ) -> impl std::future::Future<Output = Result<bool, Failure>> + Send {
        let file = self.file();
        let speaker = self.speaker.clone();
        let voice_changed = reply
            .changed_ids
            .iter()
            .any(|id| VOICE_FIELDS.contains(&id.as_str()));
        async move {
            tokio::task::spawn_blocking(move || {
                let config = read_strict(&file)?;
                refresh_voice_runtime(voice_changed, &config, speaker.as_deref());
                Ok(true)
            })
            .await
            .map_err(|_| Error::new(ID, "store_failed"))?
        }
    }
}

fn choice_value(value: impl serde::Serialize) -> Result<Value, Failure> {
    serde_json::to_value(value).map_err(|_| Error::new(ID, "store_failed"))
}
