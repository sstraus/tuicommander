//! Bounded binary pages for HTTP file browsing; ordinary editor reads stay textual.
use base64::Engine;
use serde::Serialize;
use std::io::{Read, Seek, SeekFrom};
use std::path::Path;

/// One MiB raw pages bound memory and base64 replies (at most 1,398,104 characters).
const MAX_RANGE_BYTES: u64 = 1024 * 1024;

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct FileRange {
    /// Standard padded base64 preserves arbitrary bytes and split UTF-8 sequences.
    data: String,
    offset: u64,
    bytes_read: u64,
    total_size: u64,
    has_more: bool,
    clamped: bool,
}

/// Repo file page for HTTP file browsing: no whole-file ceiling, the page is the bound.
pub(crate) fn read(
    repo_path: &str,
    file: &str,
    offset: u64,
    length: Option<u64>,
) -> Result<FileRange, String> {
    read_path(
        &crate::resolve_repo_file(repo_path, file)?,
        offset,
        length,
        u64::MAX,
    )
}

/// Repo file page for the code editor. Paging bounds memory per reply; the
/// whole document still keeps the editor's [`crate::MAX_EDITOR_LARGE_FILE_SIZE`]
/// ceiling, so a client cannot page in a file the desktop editor would refuse.
pub(crate) fn read_editor(
    repo_path: &str,
    file: &str,
    offset: u64,
    length: Option<u64>,
) -> Result<FileRange, String> {
    read_path(
        &crate::resolve_repo_file(repo_path, file)?,
        offset,
        length,
        crate::MAX_EDITOR_LARGE_FILE_SIZE,
    )
}

/// Absolute-path editor page. Callers on the HTTP boundary gate the path first.
pub(crate) fn read_editor_external(
    path: &str,
    offset: u64,
    length: Option<u64>,
) -> Result<FileRange, String> {
    let path = Path::new(path);
    if !path.is_absolute() {
        return Err("read_editor_file_external_page requires an absolute path".to_string());
    }
    read_path(path, offset, length, crate::MAX_EDITOR_LARGE_FILE_SIZE)
}

fn read_path(
    path: &Path,
    offset: u64,
    length: Option<u64>,
    max_total: u64,
) -> Result<FileRange, String> {
    let mut source = std::fs::File::open(path).map_err(|e| format!("Failed to read file: {e}"))?;
    let metadata = source
        .metadata()
        .map_err(|e| format!("Failed to read file metadata: {e}"))?;
    if !metadata.is_file() {
        return Err("Cannot read a range from a non-regular file".to_string());
    }
    let total_size = metadata.len();
    if total_size > max_total {
        return Err(crate::editor_too_large_error(total_size, max_total));
    }
    let requested = length.unwrap_or(MAX_RANGE_BYTES);
    let limit = requested
        .min(MAX_RANGE_BYTES)
        .min(total_size.saturating_sub(offset));
    let mut bytes = Vec::new();
    // Do not seek past EOF: even u64::MAX is a valid empty page offset.
    if limit > 0 {
        source
            .seek(SeekFrom::Start(offset))
            .map_err(|e| format!("Failed to seek file: {e}"))?;
        source
            .take(limit)
            .read_to_end(&mut bytes)
            .map_err(|e| format!("Failed to read file: {e}"))?;
    }
    let bytes_read = bytes.len() as u64;
    Ok(FileRange {
        data: base64::engine::general_purpose::STANDARD.encode(bytes),
        offset,
        bytes_read,
        total_size,
        has_more: offset.saturating_add(bytes_read) < total_size,
        clamped: requested > MAX_RANGE_BYTES,
    })
}
