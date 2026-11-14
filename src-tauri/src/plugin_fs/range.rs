//! Bounded random-access reads for binary preview plugins.
use super::{resolve_absolute, spawn_blocking_fs};
use crate::AppState;
use std::io::{Read, Seek, SeekFrom};
use std::sync::Arc;

const MAX_RANGE_BYTES: u64 = 8 * 1024 * 1024;

fn open_file(path: &str) -> Result<std::fs::File, String> {
    let file = std::fs::File::open(resolve_absolute(path)?)
        .map_err(|e| format!("Failed to open file: {e}"))?;
    if !file.metadata().map_err(|e| e.to_string())?.is_file() {
        return Err("Path is not a file".into());
    }
    Ok(file)
}

fn read_range(path: &str, offset: u64, length: u64) -> Result<String, String> {
    use base64::Engine;
    if length > MAX_RANGE_BYTES {
        return Err("Range length exceeds maximum size (8388608 bytes)".into());
    }
    let mut file = open_file(path)?;
    let size = file.metadata().map_err(|e| e.to_string())?.len();
    if offset > size || length > size - offset {
        return Err("Requested range is past end of file".into());
    }
    file.seek(SeekFrom::Start(offset))
        .map_err(|e| e.to_string())?;
    let mut bytes = vec![0; length as usize];
    file.read_exact(&mut bytes)
        .map_err(|e| format!("Failed to read range: {e}"))?;
    Ok(base64::engine::general_purpose::STANDARD.encode(bytes))
}

pub(crate) async fn plugin_file_size_impl(
    state: &Arc<AppState>,
    path: String,
    plugin_id: String,
) -> Result<u64, String> {
    crate::plugins::check_plugin_capability(state, &plugin_id, "fs:read")?;
    spawn_blocking_fs(move || {
        open_file(&path)?
            .metadata()
            .map(|m| m.len())
            .map_err(|e| e.to_string())
    })
    .await
}

pub(crate) async fn plugin_read_file_range_impl(
    state: &Arc<AppState>,
    path: String,
    offset: u64,
    length: u64,
    plugin_id: String,
) -> Result<String, String> {
    crate::plugins::check_plugin_capability(state, &plugin_id, "fs:read")?;
    spawn_blocking_fs(move || read_range(&path, offset, length)).await
}

#[cfg(feature = "desktop")]
#[tauri::command]
pub async fn plugin_file_size(
    path: String,
    plugin_id: String,
    state: tauri::State<'_, Arc<AppState>>,
) -> Result<u64, String> {
    plugin_file_size_impl(&state, path, plugin_id).await
}

#[cfg(feature = "desktop")]
#[tauri::command]
pub async fn plugin_read_file_range(
    path: String,
    offset: u64,
    length: u64,
    plugin_id: String,
    state: tauri::State<'_, Arc<AppState>>,
) -> Result<String, String> {
    plugin_read_file_range_impl(&state, path, offset, length, plugin_id).await
}

#[cfg(test)]
mod tests {
    use super::*;

    // Catches: seek past EOF returning empty bytes and oversized ranges allocating before refusal.
    #[tokio::test]
    async fn plugin_binary_range_rejects_eof_cap_and_relative_path() {
        let state = Arc::new(crate::state::tests_support::make_test_app_state());
        let id = "range-test".to_string();
        state
            .loaded_plugins
            .insert(id.clone(), vec!["fs:read".into()]);
        let root = tempfile::tempdir().unwrap();
        let path = root.path().join("bytes.bin");
        std::fs::write(&path, b"abcdef").unwrap();
        let path = path.to_string_lossy().into_owned();
        assert_eq!(
            plugin_file_size_impl(&state, path.clone(), id.clone())
                .await
                .unwrap(),
            6
        );
        assert_eq!(
            plugin_read_file_range_impl(&state, path.clone(), 2, 3, id.clone())
                .await
                .unwrap(),
            "Y2Rl"
        );
        for (offset, length, expected) in [
            (7, 0, "past end"),
            (5, 2, "past end"),
            (0, MAX_RANGE_BYTES + 1, "maximum size"),
        ] {
            assert!(
                plugin_read_file_range_impl(&state, path.clone(), offset, length, id.clone())
                    .await
                    .unwrap_err()
                    .contains(expected)
            );
        }
        assert!(
            plugin_read_file_range_impl(&state, "relative.bin".into(), 0, 1, id.clone())
                .await
                .unwrap_err()
                .contains("absolute")
        );
        state.loaded_plugins.remove(&id);
        assert!(plugin_file_size_impl(&state, path, id).await.is_err());
    }
}
