//! Process environment and output redaction shared by neutral script runners.
use std::collections::BTreeMap;
use std::path::PathBuf;
use std::process::Command;

/// Daemon variables a local command may inherit: locale, temp folders, identity and non-secret
/// tool config (TLS CA bundles, the ssh agent socket path). Everything else, including any token in the
/// TUIC process environment, stays out: a command gets only these and its declared `secret_refs`.
/// `PATH` is not listed because `execution_path` sets it.
fn inherits_daemon_variable(name: &str) -> bool {
    let name = if cfg!(windows) {
        name.to_ascii_uppercase()
    } else {
        name.to_owned()
    };
    matches!(
        name.as_str(),
        "HOME"
            | "LANG"
            | "LANGUAGE"
            | "TMPDIR"
            | "TMP"
            | "TEMP"
            | "USER"
            | "LOGNAME"
            | "SSH_AUTH_SOCK"
            | "SSL_CERT_FILE"
            | "SSL_CERT_DIR"
            | "NODE_EXTRA_CA_CERTS"
            | "NODE_USE_SYSTEM_CA"
    ) || name.starts_with("LC_")
        || (cfg!(windows)
            && matches!(
                name.as_str(),
                "SYSTEMROOT" | "WINDIR" | "COMSPEC" | "PATHEXT" | "USERPROFILE"
            ))
}

/// Start a local command from an empty environment plus the allowlisted daemon variables.
/// The build-slot wrapper is an installed policy adapter, not the workflow's program: it also
/// receives its own `BUILD_SLOT_*` and `RB_*` control variables (gate limits, scope, mbx).
pub(in crate::workflows) fn local_environment(command: &mut Command, adapter_controls: bool) {
    command.env_clear();
    for (name, value) in std::env::vars_os() {
        let allowed = name.to_str().is_some_and(|name| {
            inherits_daemon_variable(name)
                || (adapter_controls
                    && (name.starts_with("BUILD_SLOT_") || name.starts_with("RB_")))
        });
        if allowed {
            command.env(name, value);
        }
    }
}

pub(in crate::workflows) fn execution_path() -> std::ffi::OsString {
    let path = std::env::var_os("PATH").unwrap_or_default();
    if cfg!(target_os = "macos") {
        let paths =
            std::iter::once(PathBuf::from("/opt/homebrew/bin")).chain(std::env::split_paths(&path));
        std::env::join_paths(paths).unwrap_or(path)
    } else {
        path
    }
}

fn secret_spellings(values: &BTreeMap<String, String>) -> Vec<String> {
    let mut spellings = Vec::new();
    for value in values.values() {
        spellings.push(value.clone());
        let quoted = serde_json::to_string(value).expect("string serialization");
        let escaped = quoted[1..quoted.len() - 1].to_owned();
        // Python and many JSON tools emit ASCII-only JSON, including surrogate pairs.
        let mut ascii = String::new();
        for ch in escaped.chars() {
            if ch.is_ascii() {
                ascii.push(ch);
            } else {
                for unit in ch.encode_utf16(&mut [0; 2]) {
                    ascii.push_str(&format!("\\u{unit:04x}"));
                }
            }
        }
        spellings.extend([escaped, ascii]);
    }
    spellings.sort_by_key(|value| std::cmp::Reverse(value.len()));
    spellings.dedup();
    spellings
}

pub(in crate::workflows) fn redact_captured(
    bytes: &[u8],
    values: &BTreeMap<String, String>,
) -> String {
    let mut text = String::from_utf8_lossy(bytes).into_owned();
    // Capture truncation must not retain a partial raw or JSON-escaped secret.
    if bytes.len() == 1024 * 1024 {
        let spellings = secret_spellings(values);
        let prefix = spellings
            .iter()
            .flat_map(|value| {
                value
                    .char_indices()
                    .skip(1)
                    .map(move |(end, _)| &value[..end])
            })
            .filter(|prefix| text.ends_with(prefix))
            .max_by_key(|prefix| prefix.len());
        if let Some(prefix) = prefix {
            text.truncate(text.len() - prefix.len());
            text.push_str("[REDACTED]");
        }
    }
    redact(&text, values)
}

fn redact(text: &str, values: &BTreeMap<String, String>) -> String {
    let mut result = text.to_owned();
    for value in secret_spellings(values) {
        result = result.replace(&value, "[REDACTED]");
    }
    result
}
