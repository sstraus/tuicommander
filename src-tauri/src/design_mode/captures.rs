//! Shared design-grabs persistence for extension and retained CDP captures.
use std::path::{Path, PathBuf};

pub(super) const MAX_PNG_BYTES: usize = 2 * 1024 * 1024;

pub(super) struct Capture {
    pub bytes: Vec<u8>,
    pub format: &'static str,
    pub media_type: &'static str,
}

impl Capture {
    pub(super) fn from_data_url(value: &str, limit: usize) -> Result<Self, String> {
        use base64::Engine;
        let (encoded, format, media_type) =
            if let Some(data) = value.strip_prefix("data:image/png;base64,") {
                (data, "png", "image/png")
            } else if let Some(data) = value.strip_prefix("data:image/webp;base64,") {
                (data, "webp", "image/webp")
            } else {
                return Err("Screenshot must be a PNG or WebP data URI".into());
            };
        if encoded.len() > limit.div_ceil(3) * 4 {
            return Err("Screenshot exceeds size limit".into());
        }
        let bytes = base64::engine::general_purpose::STANDARD
            .decode(encoded)
            .map_err(|_| "Invalid screenshot base64")?;
        let valid = match format {
            "png" => bytes.starts_with(b"\x89PNG\r\n\x1a\n"),
            _ => bytes.len() >= 12 && bytes.starts_with(b"RIFF") && &bytes[8..12] == b"WEBP",
        };
        if !valid || bytes.len() > limit {
            return Err("Invalid or oversized screenshot".into());
        }
        Ok(Self {
            bytes,
            format,
            media_type,
        })
    }

    pub(super) fn save(&self, grab_dir: &Path) -> Result<Option<PathBuf>, String> {
        save(grab_dir, &self.bytes, self.format)
    }
}

fn save(grab_dir: &Path, bytes: &[u8], format: &str) -> Result<Option<PathBuf>, String> {
    if bytes.is_empty() || bytes.len() > MAX_PNG_BYTES {
        return Ok(None);
    }
    std::fs::create_dir_all(grab_dir).map_err(|error| error.to_string())?;
    let path = grab_dir.join(format!("{}.{format}", uuid::Uuid::now_v7()));
    std::fs::write(&path, bytes).map_err(|error| error.to_string())?;
    Ok(Some(path))
}

pub(super) fn save_png(grab_dir: &Path, bytes: &[u8]) -> Result<Option<PathBuf>, String> {
    save(grab_dir, bytes, "png")
}
