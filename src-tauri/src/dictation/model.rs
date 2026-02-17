use std::path::PathBuf;

const MODEL_BASE_URL: &str = "https://huggingface.co/ggerganov/whisper.cpp/resolve/main";

/// Supported Whisper GGML model variants.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WhisperModel {
    Small,
    SmallEn,
    LargeV2,
    LargeV3Turbo,
}

impl WhisperModel {
    /// All available model variants.
    pub const ALL: [WhisperModel; 4] = [
        Self::Small,
        Self::SmallEn,
        Self::LargeV2,
        Self::LargeV3Turbo,
    ];

    pub const fn filename(&self) -> &'static str {
        match self {
            Self::Small => "ggml-small.bin",
            Self::SmallEn => "ggml-small.en.bin",
            Self::LargeV2 => "ggml-large-v2.bin",
            Self::LargeV3Turbo => "ggml-large-v3-turbo.bin",
        }
    }

    pub fn download_url(&self) -> String {
        format!("{}/{}", MODEL_BASE_URL, self.filename())
    }

    pub const fn display_name(&self) -> &'static str {
        match self {
            Self::Small => "Whisper Small",
            Self::SmallEn => "Whisper Small (English)",
            Self::LargeV2 => "Whisper Large V2",
            Self::LargeV3Turbo => "Whisper Large V3 Turbo",
        }
    }

    pub const fn size_hint_mb(&self) -> u64 {
        match self {
            Self::Small => 488,
            Self::SmallEn => 488,
            Self::LargeV2 => 3090,
            Self::LargeV3Turbo => 1620,
        }
    }

    pub fn from_name(name: &str) -> Option<Self> {
        match name {
            "small" => Some(Self::Small),
            "small-en" | "small.en" => Some(Self::SmallEn),
            "large-v2" => Some(Self::LargeV2),
            "large-v3-turbo" => Some(Self::LargeV3Turbo),
            _ => None,
        }
    }

    pub const fn name(&self) -> &'static str {
        match self {
            Self::Small => "small",
            Self::SmallEn => "small-en",
            Self::LargeV2 => "large-v2",
            Self::LargeV3Turbo => "large-v3-turbo",
        }
    }
}

/// Model storage directory: <config_dir>/models/
pub fn models_dir() -> PathBuf {
    crate::config::config_dir().join("models")
}

/// Full path to a model file.
pub fn model_path(model: WhisperModel) -> PathBuf {
    models_dir().join(model.filename())
}

/// Check if a model is already downloaded.
pub fn model_exists(model: WhisperModel) -> bool {
    let path = model_path(model);
    path.exists() && path.metadata().map(|m| m.len() > 1_000_000).unwrap_or(false)
}

/// Get model file size on disk (0 if not present).
pub fn model_size_bytes(model: WhisperModel) -> u64 {
    model_path(model)
        .metadata()
        .map(|m| m.len())
        .unwrap_or(0)
}

/// Delete a downloaded model file.
pub fn delete_model(model: WhisperModel) -> Result<(), String> {
    let path = model_path(model);
    if path.exists() {
        std::fs::remove_file(&path)
            .map_err(|e| format!("Failed to delete model {}: {e}", model.filename()))?;
    }
    Ok(())
}

/// Download a model from HuggingFace with progress callback.
/// The callback receives (bytes_downloaded, total_bytes).
pub async fn download_model(
    model: WhisperModel,
    on_progress: impl Fn(u64, u64) + Send + 'static,
) -> Result<PathBuf, String> {
    let dest = model_path(model);
    if let Some(parent) = dest.parent() {
        std::fs::create_dir_all(parent)
            .map_err(|e| format!("Failed to create models directory: {e}"))?;
    }

    let url = model.download_url();
    let client = reqwest::Client::new();
    let resp = client
        .get(&url)
        .send()
        .await
        .map_err(|e| format!("Download request failed: {e}"))?;

    if !resp.status().is_success() {
        return Err(format!("Download failed with status: {}", resp.status()));
    }

    let total_size = resp.content_length().unwrap_or(0);
    let mut downloaded: u64 = 0;

    // Write to a temp file first, then rename (atomic-ish)
    let tmp_path = dest.with_extension("bin.downloading");
    let mut file = tokio::fs::File::create(&tmp_path)
        .await
        .map_err(|e| format!("Failed to create temp file: {e}"))?;

    use tokio::io::AsyncWriteExt;
    let mut stream = resp.bytes_stream();
    use futures_util::StreamExt;

    while let Some(chunk) = stream.next().await {
        let chunk = chunk.map_err(|e| format!("Download stream error: {e}"))?;
        file.write_all(&chunk)
            .await
            .map_err(|e| format!("Failed to write chunk: {e}"))?;
        downloaded += chunk.len() as u64;
        on_progress(downloaded, total_size);
    }

    file.flush()
        .await
        .map_err(|e| format!("Failed to flush file: {e}"))?;
    drop(file);

    // Rename temp file to final path
    tokio::fs::rename(&tmp_path, &dest)
        .await
        .map_err(|e| format!("Failed to rename downloaded file: {e}"))?;

    Ok(dest)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_all_models_have_unique_names() {
        let names: Vec<&str> = WhisperModel::ALL.iter().map(|m| m.name()).collect();
        let mut dedup = names.clone();
        dedup.sort();
        dedup.dedup();
        assert_eq!(names.len(), dedup.len());
    }

    #[test]
    fn test_all_models_have_unique_filenames() {
        let filenames: Vec<&str> = WhisperModel::ALL.iter().map(|m| m.filename()).collect();
        let mut dedup = filenames.clone();
        dedup.sort();
        dedup.dedup();
        assert_eq!(filenames.len(), dedup.len());
    }

    #[test]
    fn test_from_name_roundtrip() {
        for model in &WhisperModel::ALL {
            let name = model.name();
            let parsed = WhisperModel::from_name(name);
            assert_eq!(parsed, Some(*model), "roundtrip failed for {name}");
        }
    }

    #[test]
    fn test_from_name_unknown() {
        assert_eq!(WhisperModel::from_name("unknown"), None);
        assert_eq!(WhisperModel::from_name("base"), None);
    }

    #[test]
    fn test_from_name_small_en_alias() {
        assert_eq!(WhisperModel::from_name("small.en"), Some(WhisperModel::SmallEn));
        assert_eq!(WhisperModel::from_name("small-en"), Some(WhisperModel::SmallEn));
    }

    #[test]
    fn test_filenames() {
        assert_eq!(WhisperModel::Small.filename(), "ggml-small.bin");
        assert_eq!(WhisperModel::SmallEn.filename(), "ggml-small.en.bin");
        assert_eq!(WhisperModel::LargeV2.filename(), "ggml-large-v2.bin");
        assert_eq!(WhisperModel::LargeV3Turbo.filename(), "ggml-large-v3-turbo.bin");
    }

    #[test]
    fn test_size_hints() {
        assert_eq!(WhisperModel::Small.size_hint_mb(), 488);
        assert_eq!(WhisperModel::SmallEn.size_hint_mb(), 488);
        assert_eq!(WhisperModel::LargeV2.size_hint_mb(), 3090);
        assert_eq!(WhisperModel::LargeV3Turbo.size_hint_mb(), 1620);
    }

    #[test]
    fn test_download_urls() {
        for model in &WhisperModel::ALL {
            let url = model.download_url();
            assert!(url.starts_with("https://huggingface.co/ggerganov/whisper.cpp/resolve/main/ggml-"));
            assert!(url.ends_with(".bin"));
        }
    }

    #[test]
    fn test_delete_nonexistent_model_is_ok() {
        // Deleting a model that doesn't exist should be a no-op
        let result = delete_model(WhisperModel::Small);
        assert!(result.is_ok());
    }
}
