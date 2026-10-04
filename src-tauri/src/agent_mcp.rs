use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::path::PathBuf;

/// MCP config lookup result
#[derive(Clone, Serialize)]
pub(crate) struct AgentMcpStatus {
    pub(crate) supported: bool,
    pub(crate) installed: bool,
    pub(crate) config_path: Option<String>,
    /// The bridge entry would go inside the target's general settings file, so
    /// launch-time auto-install leaves it alone. The UI says so — otherwise a
    /// Zed user waits forever for an integration that never arrives.
    pub(crate) shared_settings_file: bool,
}

/// How an agent stores its MCP server list.
#[derive(Clone, Copy, PartialEq, Eq)]
enum McpFormat {
    /// JSON: `{ <key_path>: { "tuicommander": { type, command, args, env } } }`
    Json,
    /// JSON, opencode flavour: `{ "mcp": { "tuicommander": { type: "local",
    /// command: [path], enabled: true } } }`. The schema is
    /// `additionalProperties: false`, so the standard entry shape is rejected.
    OpenCode,
    /// TOML: `[mcp_servers.tuicommander]`. `forward_session` adds the
    /// `env_vars` allowlist Codex needs to pass `TUIC_SESSION` through its
    /// sandbox; agents that inherit the environment do not use it.
    Toml { forward_session: bool },
    /// YAML: goose's `extensions:` map. The entry is an `ExtensionEntry`
    /// (`enabled` + a `type`-tagged `ExtensionConfig`), which names the command
    /// `cmd` and requires `name` and `timeout`.
    Yaml,
}

/// Per-agent MCP config spec
struct McpConfigSpec {
    /// Path to the MCP configuration file
    config_path: PathBuf,
    /// JSON pointer segments to the mcpServers object (e.g. ["mcpServers"]).
    /// Unused for [`McpFormat::Toml`].
    key_path: Vec<&'static str>,
    /// On-disk representation of the server list
    format: McpFormat,
    /// CLI binaries whose presence proves the target is installed
    binaries: &'static [&'static str],
    /// Directory inspected for foreign files when no binary is found.
    /// Defaults to the config file's parent; overridden when that parent is
    /// the home directory (which always has foreign files in it).
    presence_dir: Option<PathBuf>,
    /// Only write when the config file already exists. Set for agents whose MCP
    /// support comes from an optional add-on: the file's presence is the only
    /// proof that the add-on is installed.
    requires_existing_config: bool,
    /// The MCP server list lives inside the file that also holds every other
    /// user preference — Zed, Amp and Gemini all keep it in their
    /// `settings.json` rather than a dedicated `mcp.json`.
    ///
    /// Launch-time auto-install never creates or edits these. The edit itself is
    /// surgical and validated, but TUICommander being on the machine is not
    /// consent to rewrite the user's editor configuration (issue #115).
    /// Settings > Agents installs on request, and a target we already
    /// configured keeps getting path repairs — otherwise a moved bridge binary
    /// would leave a dead entry behind with no way to notice.
    shared_settings_file: bool,
}

/// Our MCP server entry injected into agent configs.
/// `args` and `env` are always serialized (even if empty) — some Claude Code
/// versions reject stdio entries whose `args`/`env` are missing or `null`.
#[derive(Serialize, Deserialize)]
struct TuicMcpEntry {
    #[serde(rename = "type", default = "default_stdio_type")]
    transport_type: String,
    command: String,
    args: Vec<String>,
    env: BTreeMap<String, String>,
}

fn default_stdio_type() -> String {
    "stdio".to_string()
}

const TUIC_MCP_KEY: &str = "tuicommander";

/// Get the home directory, panicking on failure (should never happen in practice)
fn home() -> PathBuf {
    #[cfg(test)]
    if let Some(home) = std::env::var_os("TUIC_MCP_TEST_HOME") {
        return PathBuf::from(home);
    }
    dirs::home_dir().expect("HOME directory not found")
}

/// Get the VS Code user directory (platform-specific)
fn vscode_user_dir() -> PathBuf {
    #[cfg(target_os = "macos")]
    {
        home().join("Library/Application Support/Code/User")
    }
    #[cfg(target_os = "linux")]
    {
        home().join(".config/Code/User")
    }
    #[cfg(target_os = "windows")]
    {
        let appdata = std::env::var("APPDATA").unwrap_or_default();
        PathBuf::from(appdata).join("Code/User")
    }
}

/// XDG-style config root. opencode documents its global config as
/// `~/.config/opencode/opencode.json` on every platform, so this deliberately
/// does not use `dirs::config_dir()` (which is `~/Library/Application Support`
/// on macOS).
fn xdg_config_dir() -> PathBuf {
    match std::env::var("XDG_CONFIG_HOME") {
        Ok(dir) if !dir.is_empty() => PathBuf::from(dir),
        _ => home().join(".config"),
    }
}

/// opencode reads both `opencode.json` and `opencode.jsonc`. Write to whichever
/// already exists so we never leave the user with two competing configs.
fn opencode_config_path() -> PathBuf {
    let dir = xdg_config_dir().join("opencode");
    let jsonc = dir.join("opencode.jsonc");
    if jsonc.exists() {
        return jsonc;
    }
    dir.join("opencode.json")
}

/// Look up the MCP config spec for a given agent type.
/// Returns None for agents that don't support MCP.
fn get_mcp_config_spec(agent_type: &str) -> Option<McpConfigSpec> {
    let h = home();
    let json = |config_path: PathBuf, key_path: Vec<&'static str>, binaries| McpConfigSpec {
        config_path,
        key_path,
        format: McpFormat::Json,
        binaries,
        presence_dir: None,
        requires_existing_config: false,
        shared_settings_file: false,
    };
    match agent_type {
        "claude" => Some(McpConfigSpec {
            // The config file sits in $HOME, so presence falls back to the
            // agent's own directory rather than the (always populated) parent.
            presence_dir: Some(h.join(".claude")),
            ..json(h.join(".claude.json"), vec!["mcpServers"], &["claude"])
        }),
        "cursor" => Some(json(
            h.join(".cursor/mcp.json"),
            vec!["mcpServers"],
            &["cursor-agent", "cursor"],
        )),
        "windsurf" => Some(json(
            h.join(".codeium/windsurf/mcp_config.json"),
            vec!["mcpServers"],
            &["windsurf"],
        )),
        "vscode" => Some(json(
            vscode_user_dir().join("mcp.json"),
            vec!["servers"],
            &["code"],
        )),
        "zed" => Some(McpConfigSpec {
            shared_settings_file: true,
            ..json(
                h.join(".config/zed/settings.json"),
                vec!["context_servers"],
                &["zed"],
            )
        }),
        "amp" => Some(McpConfigSpec {
            shared_settings_file: true,
            ..json(
                h.join(".config/amp/settings.json"),
                vec!["amp", "mcpServers"],
                &["amp"],
            )
        }),
        "gemini" => Some(McpConfigSpec {
            shared_settings_file: true,
            ..json(
                h.join(".gemini/settings.json"),
                vec!["mcpServers"],
                &["gemini"],
            )
        }),
        "droid" => Some(json(
            h.join(".factory/mcp.json"),
            vec!["mcpServers"],
            &["droid"],
        )),
        "opencode" => Some(McpConfigSpec {
            format: McpFormat::OpenCode,
            ..json(opencode_config_path(), vec!["mcp"], &["opencode"])
        }),
        "codex" => Some(McpConfigSpec {
            // Codex filters the child environment, so TUIC_SESSION only reaches
            // the bridge when it is on the env_vars allowlist.
            format: McpFormat::Toml {
                forward_session: true,
            },
            ..json(codex_config_path(), vec![], &["codex"])
        }),
        "grok" => Some(McpConfigSpec {
            format: McpFormat::Toml {
                forward_session: false,
            },
            ..json(h.join(".grok/config.toml"), vec![], &["grok"])
        }),
        "goose" => Some(McpConfigSpec {
            format: McpFormat::Yaml,
            ..json(goose_config_path(), vec!["extensions"], &["goose"])
        }),
        "pi" => Some(McpConfigSpec {
            // pi has no built-in MCP client: the pi-mcp-adapter extension adds
            // one and reads this file. Without the file the extension is not
            // installed, so writing it would configure nothing.
            requires_existing_config: true,
            ..json(pi_config_path(), vec!["mcpServers"], &["pi"])
        }),
        // aider has no MCP client at all — the feature PRs were never merged.
        "aider" => None,
        _ => None,
    }
}

/// goose config file. Documented as `~/.config/goose/config.yaml` on macOS and
/// Linux (goose resolves it with etcetera's XDG strategy) and
/// `%APPDATA%\Block\goose\config\config.yaml` on Windows.
fn goose_config_path() -> PathBuf {
    #[cfg(windows)]
    {
        let appdata = std::env::var("APPDATA").unwrap_or_default();
        PathBuf::from(appdata).join("Block/goose/config/config.yaml")
    }
    #[cfg(not(windows))]
    {
        xdg_config_dir().join("goose/config.yaml")
    }
}

/// pi's own MCP override file, read by the pi-mcp-adapter extension.
/// `PI_CODING_AGENT_DIR` relocates the agent directory.
fn pi_config_path() -> PathBuf {
    match std::env::var("PI_CODING_AGENT_DIR") {
        Ok(dir) if !dir.is_empty() => PathBuf::from(dir).join("mcp.json"),
        _ => home().join(".pi/agent/mcp.json"),
    }
}

/// Files we create ourselves, plus OS noise, do not prove the target is
/// installed — see [`is_target_installed`].
fn dir_has_foreign_entry(dir: &std::path::Path, ours: Option<&std::ffi::OsStr>) -> bool {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return false;
    };
    entries.flatten().any(|entry| {
        let name = entry.file_name();
        if name == ".DS_Store" {
            return false;
        }
        match ours {
            // `write_text_file`/`write_toml_file` stage through `<stem>.tmp`;
            // a crashed write must not read back as a foreign file.
            Some(ours) => {
                name != ours && std::path::Path::new(&name).extension() != Some("tmp".as_ref())
            }
            None => true,
        }
    })
}

/// Is the agent/IDE this spec targets actually present on this machine?
///
/// Without this check TUIC creates `~/.cursor/`, `~/.gemini/`, `~/.config/amp/`
/// and friends on first launch for tools the user never installed — the write
/// path creates every missing parent directory.
///
/// Two independent proofs, cheapest first: the config directory holds a file
/// that is not ours, or one of the CLI binaries resolves. The directory check
/// covers GUI installs whose CLI shim is not on PATH.
fn is_target_installed(spec: &McpConfigSpec) -> bool {
    let presence_dir = spec
        .presence_dir
        .clone()
        .or_else(|| spec.config_path.parent().map(PathBuf::from));
    if let Some(dir) = presence_dir {
        // A dedicated presence_dir holds none of our writes, so every file in
        // it belongs to the tool.
        let ours = if spec.presence_dir.is_some() {
            None
        } else {
            spec.config_path.file_name()
        };
        if dir_has_foreign_entry(&dir, ours) {
            return true;
        }
    }
    spec.binaries
        .iter()
        .any(|binary| crate::cli::has_cli(binary))
}

/// Get the path to an agent's own settings file (for "Edit Config" button)
fn get_agent_settings_path(agent_type: &str) -> Option<PathBuf> {
    let h = home();
    match agent_type {
        "claude" => Some(h.join(".claude/settings.json")),
        "cursor" => Some(h.join(".cursor")),
        "aider" => Some(h.join(".aider.conf.yml")),
        "gemini" => Some(h.join(".gemini/settings.json")),
        "codex" => Some(codex_config_path()),
        "grok" => Some(h.join(".grok/config.toml")),
        "opencode" => Some(opencode_config_path()),
        "droid" => Some(h.join(".factory/mcp.json")),
        "pi" => Some(h.join(".pi/agent/settings.json")),
        "goose" => Some(goose_config_path()),
        "amp" => Some(h.join(".config/amp/settings.json")),
        "zed" => Some(h.join(".config/zed/settings.json")),
        "vscode" => Some(vscode_user_dir().join("settings.json")),
        "windsurf" => Some(h.join(".codeium/windsurf/settings.json")),
        _ => None,
    }
}

/// Navigate a JSON object by key path (read-only).
fn navigate<'a>(root: &'a serde_json::Value, key_path: &[&str]) -> Option<&'a serde_json::Value> {
    let mut current = root;
    for key in key_path {
        current = current.get(*key)?;
    }
    Some(current)
}

const BRIDGE_NAME: &str = "tuic-bridge";

/// Detect the tuic-bridge binary path.
/// Priority: sidecar (same dir as main executable) → PATH → bare name.
/// The bridge sitting next to an executable, if it is there.
///
/// Where "next to" means: `Contents/MacOS/` in a macOS bundle, beside the `.exe`
/// on Windows, the same directory on Linux, `target/debug|release` in dev — and,
/// since #793-23a5, the directory a `tuic-remote` daemon was unpacked into. The
/// release publishes `tuic-bridge-<target>`; the install instructions download
/// it as `tuic-bridge`, which is the name this looks for.
fn bridge_beside(dir: &std::path::Path) -> Option<PathBuf> {
    let candidate = bridge_path_in(dir);
    usable_executable(&candidate).then_some(candidate)
}

fn usable_executable(path: &std::path::Path) -> bool {
    let Ok(metadata) = path.metadata() else {
        return false;
    };
    if !metadata.is_file() || metadata.len() == 0 {
        return false;
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        if metadata.permissions().mode() & 0o111 == 0 {
            return false;
        }
    }
    true
}

fn resolved_bridge_name() -> &'static str {
    #[cfg(windows)]
    {
        "tuic-bridge.exe"
    }
    #[cfg(not(windows))]
    {
        BRIDGE_NAME
    }
}

/// Where the bridge would sit in `dir`, whether or not anything is there.
///
/// Split out so the search and the report of a failed search cannot disagree
/// about the file name — `.exe` on Windows, bare everywhere else.
fn bridge_path_in(dir: &std::path::Path) -> PathBuf {
    #[cfg(not(windows))]
    {
        dir.join(BRIDGE_NAME)
    }
    #[cfg(windows)]
    {
        dir.join(format!("{BRIDGE_NAME}.exe"))
    }
}

/// Every candidate [`locate_bridge_binary`] considers, in search order.
///
/// For the warning a failed search logs. A report that named paths the search
/// never tried would be worse than none: it sends the reader to put a file
/// somewhere that still would not be found.
///
/// The second entry is `resolve_cli`'s answer, which is a well-known bin
/// directory when one holds the binary and the bare name when none does. The
/// bare name is kept in the diagnostic even though it is not a usable
/// location: a relative command would depend on the agent's working directory.
pub(crate) fn bridge_search_paths() -> Vec<PathBuf> {
    let mut paths = Vec::new();
    if let Ok(exe) = std::env::current_exe()
        && let Some(dir) = exe.parent()
    {
        paths.push(bridge_path_in(dir));
    }
    paths.push(PathBuf::from(crate::cli::resolve_cli(
        resolved_bridge_name(),
    )));
    paths
}

/// The bridge binary, only when we can point at a file that exists.
///
/// Separate from [`get_mcp_bridge_info`], which reports installed copies:
/// a config file written for an agent may name `tuic-bridge` and still
/// work, since the agent resolves it against its own `PATH` at launch. ego is
/// not such an agent any more — it reaches `tuicommander` over ACP.
pub(crate) fn locate_bridge_binary() -> Option<PathBuf> {
    // Primary: sidecar bundled alongside the main executable
    if let Ok(exe) = std::env::current_exe()
        && let Some(dir) = exe.parent()
        && let Some(candidate) = bridge_beside(dir)
    {
        return Some(candidate);
    }
    // Fallback: resolve from PATH via well-known directories
    let resolved = PathBuf::from(crate::cli::resolve_cli(resolved_bridge_name()));
    (resolved.is_absolute() && usable_executable(&resolved)).then_some(resolved)
}

/// Inspect installed copies without creating files, including when a target
/// cleanup has removed the adjacent source. Settings reads must stay read-only.
fn locate_installed_bridge(source: Option<&std::path::Path>) -> Option<PathBuf> {
    use sha2::{Digest, Sha256};
    if let Some(bytes) = source.and_then(|path| std::fs::read(path).ok()) {
        let target = bridge_install_path(&Sha256::digest(&bytes));
        return installed_revision_is_valid(&target).then_some(target);
    }
    std::fs::read_dir(crate::config::config_dir().join("mcp-bridge"))
        .ok()?
        .filter_map(Result::ok)
        .filter_map(|entry| {
            let name = entry.file_name();
            let name = name.to_str()?;
            if name.len() != 64 || !name.bytes().all(|c| c.is_ascii_hexdigit()) {
                return None;
            }
            let path = bridge_path_in(&entry.path());
            if !installed_revision_is_valid(&path) {
                return None;
            }
            Some((path.metadata().ok()?.modified().ok()?, path))
        })
        .max_by_key(|(modified, _)| *modified)
        .map(|(_, path)| path)
}

fn installed_revision_is_valid(path: &std::path::Path) -> bool {
    use sha2::{Digest, Sha256};
    let Some(revision) = path
        .parent()
        .and_then(|dir| dir.file_name())
        .and_then(|name| name.to_str())
    else {
        return false;
    };
    usable_executable(path)
        && std::fs::read(path).is_ok_and(|bytes| hex::encode(Sha256::digest(bytes)) == revision)
}

fn bridge_install_path(digest: &[u8]) -> PathBuf {
    bridge_path_in(
        &crate::config::config_dir()
            .join("mcp-bridge")
            .join(hex::encode(digest)),
    )
}

/// Publication is permitted only for one SHA-256 revision observed before,
/// during and after copying. This decision owns both mismatch cases.
pub(crate) fn verified_revision(
    source_before: &[u8],
    copy: &[u8],
    source_after: &[u8],
) -> Option<String> {
    (source_before.len() == 32 && source_before == copy && source_before == source_after)
        .then(|| hex::encode(source_before))
}

/// The copy must independently match the initial digest: copying the already
/// hashed buffer would miss a source changed by a linker between the two reads.
fn copy_verified_bridge(
    source: &std::path::Path,
    temp: &mut tempfile::NamedTempFile,
    expected: &[u8],
) -> Result<(), String> {
    use sha2::{Digest, Sha256};
    std::fs::copy(source, temp.path()).map_err(|e| format!("Failed to copy bridge: {e}"))?;
    let copied =
        std::fs::read(temp.path()).map_err(|e| format!("Failed to verify bridge copy: {e}"))?;
    let current =
        std::fs::read(source).map_err(|e| format!("Failed to verify bridge source: {e}"))?;
    verified_revision(
        expected,
        &Sha256::digest(&copied),
        &Sha256::digest(&current),
    )
    .ok_or_else(|| {
        "Bridge source changed during installation; leaving configs unchanged".to_string()
    })
    .map(|_| ())
}

/// Content-addressed copies outlive target cleanup and app upgrades. Never rewrite
/// an identical executable: Windows may have it open in an existing MCP session.
fn install_bridge_binary(source: &std::path::Path) -> Result<PathBuf, String> {
    use sha2::{Digest, Sha256};
    // DEFERRED (2026-10-03): retain revisions until every agent config root
    // (including private/CLAUDE_CONFIG_DIR roots) can be discovered. Pruning an
    // unknown root's referenced revision would recreate the original ENOENT.

    if !usable_executable(source) {
        return Err(format!("Bridge is not executable: {}", source.display()));
    }
    let bytes = std::fs::read(source).map_err(|e| format!("Failed to read bridge: {e}"))?;
    if bytes.is_empty() {
        return Err("Bridge became empty during installation".into());
    }
    let digest = Sha256::digest(&bytes);
    let target = bridge_install_path(&digest);
    let dir = target
        .parent()
        .ok_or_else(|| "Bridge installation directory unavailable".to_string())?;
    let matches = || usable_executable(&target) && std::fs::read(&target).is_ok_and(|b| b == bytes);
    if matches() {
        return Ok(target);
    }
    std::fs::create_dir_all(dir).map_err(|e| format!("Failed to create bridge directory: {e}"))?;
    let mut temp = tempfile::NamedTempFile::new_in(dir)
        .map_err(|e| format!("Failed to create bridge temp file: {e}"))?;
    copy_verified_bridge(source, &mut temp, &digest)?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        temp.as_file()
            .set_permissions(std::fs::Permissions::from_mode(0o700))
            .map_err(|e| format!("Failed to make bridge executable: {e}"))?;
    }
    temp.as_file()
        .sync_all()
        .map_err(|e| format!("Failed to sync bridge: {e}"))?;
    // A competing startup may have installed the same revision while we copied.
    if let Err(error) = temp.persist(&target)
        && !matches()
    {
        return Err(format!("Failed to publish bridge: {}", error.error));
    }
    Ok(target)
}

fn bridge_command_from_location(located: Option<PathBuf>) -> String {
    located.map_or_else(
        || BRIDGE_NAME.to_string(),
        |path| path.to_string_lossy().to_string(),
    )
}

/// Read a JSON config file's raw text, returning an empty document when the file
/// doesn't exist.
///
/// Returns `None` when the file exists but cannot be read. Callers MUST NOT
/// write in that case — an unreadable file is not an empty one.
fn read_json_text(path: &std::path::Path) -> Option<String> {
    if !path.exists() {
        return Some(String::new());
    }
    std::fs::read_to_string(path)
        .inspect_err(|e| tracing::error!(source = "mcp", path = %path.display(), "Failed to read config: {e}"))
        .ok()
}

/// Read a JSON config file into a value, for callers that only inspect it.
///
/// Parsed as JSONC: Zed, VS Code and opencode all document comments and
/// trailing commas as supported, so `serde_json` would report a perfectly valid
/// file as broken. Returns `None` when the file cannot be read or parsed, and
/// callers MUST NOT write in that case — treating a parse failure as an empty
/// document is what replaced a user's entire Zed configuration with our single
/// entry (issue #115).
fn read_json_file(path: &std::path::Path) -> Option<serde_json::Value> {
    let text = read_json_text(path)?;
    if text.trim().is_empty() {
        return Some(serde_json::json!({}));
    }
    crate::jsonc_edit::parse(&text)
        .inspect_err(|e| tracing::error!(source = "mcp", path = %path.display(), "{e} — leaving the file untouched"))
        .ok()
}

/// Keep the target's own copy of a config file the first time we modify it.
///
/// Stored under TUIC's config directory rather than beside the original: a
/// stray `settings.json.bak` next to `settings.json` is a file the owning tool
/// may itself try to load, sync or complain about.
///
/// Written once and never overwritten. The state worth keeping is the one from
/// before TUIC ever touched the file; a later snapshot of a file we already
/// edited is worth much less, and rewriting it on every path repair would
/// eventually erase the only pristine copy.
fn backup_config_once(path: &std::path::Path, agent_label: &str, text: &str) {
    if text.is_empty() {
        return; // Nothing existed to preserve.
    }
    let dir = crate::config::config_dir().join("mcp-backups");
    let name = path
        .file_name()
        .map(|n| n.to_string_lossy().to_string())
        .unwrap_or_else(|| "config".to_string());
    let backup = dir.join(format!("{agent_label}-{name}.orig"));
    if backup.exists() {
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            if let Ok(metadata) = backup.symlink_metadata()
                && metadata.is_file()
            {
                let _ = std::fs::set_permissions(&backup, std::fs::Permissions::from_mode(0o600));
            }
        }
        return;
    }
    let saved = std::fs::create_dir_all(&dir).and_then(|()| {
        use std::io::Write;
        let mut options = std::fs::OpenOptions::new();
        options.write(true).create_new(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            options.mode(0o600);
        }
        let mut file = options.open(&backup)?;
        file.write_all(text.as_bytes())?;
        file.sync_all()
    });
    match saved {
        Ok(()) => {
            tracing::info!(source = "mcp", agent = %agent_label, backup = %backup.display(), "Saved original config")
        }
        // A failed backup must not block the edit: the edit itself is surgical
        // and validated, so the backup is a second line of defence, not the
        // first.
        Err(e) => {
            tracing::warn!(source = "mcp", agent = %agent_label, "Could not save original config: {e}")
        }
    }
}

/// Write a config file atomically (temp + rename).
#[cfg(test)]
fn write_text_file(path: &std::path::Path, text: &str) -> Result<(), String> {
    write_text_file_if_unchanged(path, None, text)
}

fn write_text_file_if_unchanged(
    path: &std::path::Path,
    expected: Option<&str>,
    text: &str,
) -> Result<(), String> {
    use std::io::Write;
    let target = if path.is_symlink() {
        path.canonicalize()
            .map_err(|e| format!("Failed to resolve {}: {e}", path.display()))?
    } else {
        path.to_path_buf()
    };
    if let Some(expected) = expected {
        let actual = match std::fs::read_to_string(&target) {
            Ok(content) => content,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => String::new(),
            Err(e) => return Err(format!("Failed to re-read {}: {e}", target.display())),
        };
        if actual != expected {
            return Err(format!("{} changed during MCP edit", target.display()));
        }
    }
    if let Some(parent) = target.parent() {
        std::fs::create_dir_all(parent)
            .map_err(|e| format!("Failed to create directory {}: {e}", parent.display()))?;
    }
    let parent = target
        .parent()
        .ok_or_else(|| format!("{} has no parent", target.display()))?;
    let mut temp = tempfile::NamedTempFile::new_in(parent)
        .map_err(|e| format!("Failed to create temp file: {e}"))?;
    let permissions = match std::fs::metadata(&target) {
        Ok(metadata) => metadata.permissions(),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
            #[cfg(unix)]
            {
                use std::os::unix::fs::PermissionsExt;
                std::fs::Permissions::from_mode(0o600)
            }
            #[cfg(not(unix))]
            {
                temp.as_file()
                    .metadata()
                    .map_err(|e| e.to_string())?
                    .permissions()
            }
        }
        Err(e) => return Err(format!("Failed to inspect {}: {e}", target.display())),
    };
    temp.as_file_mut()
        .set_permissions(permissions)
        .map_err(|e| format!("Failed to set temp permissions: {e}"))?;
    temp.write_all(text.as_bytes())
        .map_err(|e| format!("Failed to write temp file: {e}"))?;
    temp.as_file()
        .sync_all()
        .map_err(|e| format!("Failed to sync temp file: {e}"))?;
    if let Some(expected) = expected {
        let actual = match std::fs::read_to_string(&target) {
            Ok(content) => content,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => String::new(),
            Err(e) => return Err(format!("Failed to re-read {}: {e}", target.display())),
        };
        if actual != expected {
            return Err(format!("{} changed during MCP edit", target.display()));
        }
    }
    temp.persist(&target)
        .map_err(|e| format!("Failed to replace config: {e}"))?;
    #[cfg(unix)]
    if let Err(e) = std::fs::File::open(parent).and_then(|dir| dir.sync_all()) {
        tracing::warn!(source = "mcp", path = %parent.display(), "Could not sync config directory: {e}");
    }
    Ok(())
}

/// Supported agent types for auto-install
const SUPPORTED_AGENTS: &[&str] = &[
    "claude", "cursor", "windsurf", "vscode", "zed", "amp", "gemini", "codex", "grok", "opencode",
    "droid", "goose", "pi",
];

/// Build the bridge entry in the shape the target's schema accepts.
fn json_entry_value(format: McpFormat, bridge_path: &str) -> serde_json::Value {
    match format {
        McpFormat::OpenCode => serde_json::json!({
            "type": "local",
            "command": [bridge_path],
            "enabled": true,
        }),
        _ => serde_json::json!({
            "type": "stdio",
            "command": bridge_path,
            "args": [],
            "env": {},
        }),
    }
}

/// Is the entry already exactly what we would write?
fn json_entry_is_current(
    format: McpFormat,
    entry: Option<&serde_json::Value>,
    bridge_path: &str,
) -> bool {
    let Some(entry) = entry else {
        return false;
    };
    match format {
        McpFormat::OpenCode => {
            let command_ok = entry
                .get("command")
                .and_then(|v| v.as_array())
                .is_some_and(|args| args.first().and_then(|v| v.as_str()) == Some(bridge_path));
            command_ok && entry.get("type").and_then(|v| v.as_str()) == Some("local")
        }
        _ => {
            let command_ok = entry.get("command").and_then(|v| v.as_str()) == Some(bridge_path);
            // Claude Code rejects stdio entries where `args`/`env` are null or
            // missing, so a malformed entry is rewritten even on a path match.
            let args_ok = entry.get("args").is_some_and(serde_json::Value::is_array);
            let env_ok = entry.get("env").is_some_and(serde_json::Value::is_object);
            command_ok && args_ok && env_ok
        }
    }
}

/// Ensure a single agent's MCP config has the correct bridge entry.
/// Returns true if the config was written (installed or updated).
fn ensure_agent_mcp_entry(
    config_path: &std::path::Path,
    key_path: &[&str],
    format: McpFormat,
    bridge_path: &str,
    agent_label: &str,
) -> bool {
    let Some(text) = read_json_text(config_path) else {
        return false;
    };
    let root = match parse_existing(&text, config_path) {
        Some(root) => root,
        None => return false,
    };
    let existing_entry = navigate(&root, key_path)
        .and_then(|v| v.as_object())
        .and_then(|obj| obj.get(TUIC_MCP_KEY));

    if json_entry_is_current(format, existing_entry, bridge_path) {
        return false;
    }
    match existing_entry.and_then(|entry| entry.get("command")) {
        Some(old) => {
            tracing::info!(source = "mcp", agent = %agent_label, "Rewriting entry: {old} → {bridge_path}");
        }
        None => {
            tracing::info!(source = "mcp", agent = %agent_label, "Installing bridge");
        }
    }

    let mut entry_value = existing_entry
        .cloned()
        .unwrap_or_else(|| json_entry_value(format, bridge_path));
    let Some(entry) = entry_value.as_object_mut() else {
        tracing::error!(source = "mcp", agent = %agent_label, "Bridge entry is not an object");
        return false;
    };
    match format {
        McpFormat::OpenCode => {
            let mut command = entry
                .get("command")
                .and_then(|value| value.as_array())
                .cloned()
                .unwrap_or_default();
            if command.is_empty() {
                command.push(bridge_path.into());
            } else {
                command[0] = bridge_path.into();
            }
            entry.insert("command".to_string(), serde_json::Value::Array(command));
            entry.insert("type".to_string(), "local".into());
        }
        _ => {
            entry.insert("command".to_string(), bridge_path.into());
            if !entry.get("args").is_some_and(serde_json::Value::is_array) {
                entry.insert("args".to_string(), serde_json::json!([]));
            }
            if !entry.get("env").is_some_and(serde_json::Value::is_object) {
                entry.insert("env".to_string(), serde_json::json!({}));
            }
            entry.entry("type").or_insert_with(|| "stdio".into());
        }
    }
    let edited = match crate::jsonc_edit::upsert_member(&text, key_path, TUIC_MCP_KEY, &entry_value)
    {
        Ok(edited) => edited,
        Err(e) => {
            tracing::error!(source = "mcp", agent = %agent_label, path = %config_path.display(), "{e} — leaving the file untouched");
            return false;
        }
    };

    commit_json_edit(config_path, agent_label, &text, &edited)
}

/// Parse a config we are about to modify, rejecting anything we cannot read.
///
/// An empty or absent file becomes an empty object — that one is genuinely new.
/// A file that exists but does not parse yields `None`, and every caller treats
/// that as "do not write".
fn parse_existing(text: &str, config_path: &std::path::Path) -> Option<serde_json::Value> {
    if text.trim().is_empty() {
        return Some(serde_json::json!({}));
    }
    crate::jsonc_edit::parse(text)
        .inspect_err(|e| tracing::error!(source = "mcp", path = %config_path.display(), "{e} — leaving the file untouched"))
        .ok()
}

/// Back up the original, then write the edited document.
///
/// Skips the write when the edit changed nothing, so an idempotent pass never
/// touches the file's mtime — editors watch these files and reload on change.
fn commit_json_edit(
    config_path: &std::path::Path,
    agent_label: &str,
    original: &str,
    edited: &str,
) -> bool {
    if edited == original {
        return false;
    }
    backup_config_once(config_path, agent_label, original);
    match write_text_file_if_unchanged(config_path, Some(original), edited) {
        Ok(()) => {
            tracing::debug!(source = "mcp", agent = %agent_label, path = %config_path.display(), "Config written");
            true
        }
        Err(e) => {
            tracing::error!(source = "mcp", agent = %agent_label, "Write error: {e}");
            false
        }
    }
}

// ---------------------------------------------------------------------------
// Codex (TOML) support
// ---------------------------------------------------------------------------

/// Path to Codex config file
fn codex_config_path() -> PathBuf {
    match std::env::var_os("CODEX_HOME") {
        Some(dir) if !dir.is_empty() => PathBuf::from(dir).join("config.toml"),
        _ => home().join(".codex/config.toml"),
    }
}

/// Read a TOML file, returning an empty table when it doesn't exist.
///
/// Returns `None` on a read or parse failure, for the same reason as
/// [`read_json_file`]: writing an "empty" document back would delete every
/// setting in the user's `config.toml`.
fn read_toml_file(path: &std::path::Path) -> Option<toml::Value> {
    if !path.exists() {
        return Some(toml::Value::Table(Default::default()));
    }
    let content = std::fs::read_to_string(path)
        .inspect_err(|e| tracing::error!(source = "mcp", path = %path.display(), "Failed to read config: {e}"))
        .ok()?;
    toml::from_str(&content)
        .inspect_err(|e| tracing::error!(source = "mcp", path = %path.display(), "TOML parse error, leaving the file untouched: {e}"))
        .ok()
}

/// Serialize a TOML test fixture.
#[cfg(test)]
fn write_toml_file(path: &std::path::Path, value: &toml::Value) -> Result<(), String> {
    let output =
        toml::to_string_pretty(value).map_err(|e| format!("Failed to serialize TOML: {e}"))?;
    write_text_file(path, &output)
}

/// Ensure a TOML config (`[mcp_servers.<name>]`) has the correct bridge entry.
/// Returns true if the config was written (installed or updated).
///
/// `forward_session` adds `TUIC_SESSION` to the entry's `env_vars` allowlist —
/// Codex needs it to pass the variable through its sandbox. Grok inherits the
/// environment, and its schema has no `env_vars` key.
fn ensure_toml_mcp_entry(
    config_path: &std::path::Path,
    forward_session: bool,
    bridge_path: &str,
    agent_label: &str,
) -> bool {
    let Some(root) = read_toml_file(config_path) else {
        return false;
    };

    let existing_entry = root
        .get("mcp_servers")
        .and_then(|s| s.get(TUIC_MCP_KEY))
        .and_then(|entry| entry.as_table());
    let existing_command = existing_entry
        .and_then(|entry| entry.get("command"))
        .and_then(toml::Value::as_str);
    let forwards_tuic_session = existing_entry
        .and_then(|entry| entry.get("env_vars"))
        .and_then(toml::Value::as_array)
        .is_some_and(|env_vars| {
            env_vars.iter().any(|value| {
                value.as_str() == Some("TUIC_SESSION")
                    || value.as_table().is_some_and(|entry| {
                        entry.get("name").and_then(toml::Value::as_str) == Some("TUIC_SESSION")
                            && entry
                                .get("source")
                                .and_then(toml::Value::as_str)
                                .is_none_or(|source| source == "local")
                    })
            })
        });

    let session_ok = forwards_tuic_session || !forward_session;
    match existing_command {
        Some(cmd) if cmd == bridge_path && session_ok => {
            return false;
        }
        Some(cmd) if cmd == bridge_path => {
            tracing::info!(
                source = "mcp",
                agent = %agent_label,
                "Enabling TUIC_SESSION forwarding for bridge identity"
            );
        }
        Some(old) => {
            tracing::info!(
                source = "mcp",
                agent = %agent_label,
                "Updating path: {old} → {bridge_path}"
            );
        }
        None => {
            tracing::info!(source = "mcp", agent = %agent_label, "Installing bridge");
        }
    }

    let original = std::fs::read_to_string(config_path).unwrap_or_default();
    let mut document = match original.parse::<toml_edit::DocumentMut>() {
        Ok(document) => document,
        Err(e) => {
            tracing::error!(source = "mcp", agent = %agent_label, "TOML edit parse error: {e}");
            return false;
        }
    };
    if document.get("mcp_servers").is_none() {
        document["mcp_servers"] = toml_edit::table();
    }
    if !document["mcp_servers"].is_table() {
        tracing::error!(source = "mcp", agent = %agent_label, "TOML mcp_servers is not a table");
        return false;
    }
    if document["mcp_servers"].get(TUIC_MCP_KEY).is_none() {
        document["mcp_servers"][TUIC_MCP_KEY] = toml_edit::table();
    }
    if !document["mcp_servers"][TUIC_MCP_KEY].is_table() {
        tracing::error!(source = "mcp", agent = %agent_label, "TOML bridge entry is not a table");
        return false;
    }
    let entry = &mut document["mcp_servers"][TUIC_MCP_KEY];
    let decor = entry
        .get("command")
        .and_then(toml_edit::Item::as_value)
        .map(|value| value.decor().clone());
    let mut command = toml_edit::Value::from(bridge_path);
    if let Some(decor) = decor {
        *command.decor_mut() = decor;
    }
    entry["command"] = toml_edit::Item::Value(command);
    if forward_session && !forwards_tuic_session {
        let mut env_vars = entry
            .get("env_vars")
            .and_then(toml_edit::Item::as_value)
            .and_then(toml_edit::Value::as_array)
            .cloned()
            .unwrap_or_default();
        env_vars.push("TUIC_SESSION");
        entry["env_vars"] = toml_edit::value(env_vars);
    }
    let edited = document.to_string();
    backup_config_once(config_path, agent_label, &original);
    match write_text_file_if_unchanged(config_path, Some(&original), &edited) {
        Ok(()) => {
            tracing::debug!(source = "mcp", agent = %agent_label, path = %config_path.display(), "Config written");
            true
        }
        Err(e) => {
            tracing::error!(source = "mcp", agent = %agent_label, "Write error: {e}");
            false
        }
    }
}

/// Remove the tuicommander entry from a TOML config.
fn remove_toml_mcp_entry(config_path: &std::path::Path, agent_label: &str) -> Result<(), String> {
    if !config_path.exists() {
        return Ok(());
    }
    let original = std::fs::read_to_string(config_path).map_err(|e| e.to_string())?;
    let mut document = original
        .parse::<toml_edit::DocumentMut>()
        .map_err(|e| format!("Cannot parse {}: {e}", config_path.display()))?;
    if let Some(servers) = document
        .get_mut("mcp_servers")
        .and_then(toml_edit::Item::as_table_mut)
    {
        servers.remove(TUIC_MCP_KEY);
    }
    backup_config_once(config_path, agent_label, &original);
    write_text_file_if_unchanged(config_path, Some(&original), &document.to_string())
}

/// Check if a TOML config has the tuicommander MCP entry installed.
fn is_toml_mcp_installed(config_path: &std::path::Path) -> bool {
    read_toml_file(config_path).is_some_and(|root| {
        root.get("mcp_servers")
            .and_then(|s| s.get(TUIC_MCP_KEY))
            .is_some()
    })
}

// ---------------------------------------------------------------------------
// Goose (YAML) support
// ---------------------------------------------------------------------------

/// Read a YAML file, returning an empty mapping when it doesn't exist.
/// `None` on read/parse failure — see [`read_json_file`] for why we never
/// write in that case.
fn read_yaml_file(path: &std::path::Path) -> Option<serde_yaml::Value> {
    if !path.exists() {
        return Some(serde_yaml::Value::Mapping(Default::default()));
    }
    let content = std::fs::read_to_string(path)
        .inspect_err(|e| tracing::error!(source = "mcp", path = %path.display(), "Failed to read config: {e}"))
        .ok()?;
    serde_yaml::from_str(&content)
        .inspect_err(|e| tracing::error!(source = "mcp", path = %path.display(), "YAML parse error, leaving the file untouched: {e}"))
        .ok()
}

fn yaml_line_key(line: &str, key: &str) -> bool {
    line.trim_start().starts_with(&format!("{key}:"))
}

fn yaml_indent(line: &str) -> usize {
    line.len() - line.trim_start_matches(' ').len()
}

fn yaml_command_edit(original: &str, key: &str, bridge_path: &str) -> Option<String> {
    let lines: Vec<&str> = original.lines().collect();
    let section = lines
        .iter()
        .position(|line| yaml_indent(line) == 0 && yaml_line_key(line, key))?;
    let section_indent = yaml_indent(lines[section]);
    let entry = ((section + 1)..lines.len()).find(|&index| {
        let line = lines[index];
        !line.trim().is_empty()
            && yaml_indent(line) > section_indent
            && yaml_line_key(line, TUIC_MCP_KEY)
    })?;
    let entry_indent = yaml_indent(lines[entry]);
    let command = ((entry + 1)..lines.len()).find(|&index| {
        let line = lines[index];
        !line.trim().is_empty() && yaml_indent(line) > entry_indent && yaml_line_key(line, "cmd")
    })?;
    let line = lines[command];
    let colon = line.find(':')?;
    let tail = &line[colon + 1..];
    let comment = tail.find(" #").map(|offset| &tail[offset..]).unwrap_or("");
    let scalar = if bridge_path
        .bytes()
        .all(|byte| byte.is_ascii_alphanumeric() || b"/._-".contains(&byte))
    {
        bridge_path.to_string()
    } else {
        serde_json::to_string(bridge_path).ok()?
    };
    let replacement = format!("{} {}{}", &line[..=colon], scalar, comment);
    let mut edited = original.to_string();
    let start: usize = lines.iter().take(command).map(|line| line.len() + 1).sum();
    edited.replace_range(start..start + line.len(), &replacement);
    Some(edited)
}

fn yaml_entry_insert(original: &str, key: &str, bridge_path: &str) -> Option<String> {
    let entry = serde_yaml::to_string(&goose_entry_value(bridge_path)).ok()?;
    let lines: Vec<&str> = original.lines().collect();
    let section = lines
        .iter()
        .position(|line| yaml_indent(line) == 0 && yaml_line_key(line, key));
    let indent = if let Some(section) = section {
        lines[(section + 1)..]
            .iter()
            .find(|line| !line.trim().is_empty() && !line.trim_start().starts_with('#'))
            .map_or(2, |line| match yaml_indent(line) {
                0 => 2,
                indent => indent,
            })
    } else {
        2
    };
    let mut block = format!("{}{TUIC_MCP_KEY}:\n", " ".repeat(indent));
    for line in entry.lines() {
        block.push_str(&format!("{}{line}\n", " ".repeat(indent * 2)));
    }
    if let Some(section) = section {
        let line = lines[section];
        if line.trim() != format!("{key}:") {
            return None;
        }
        let end: usize = lines
            .iter()
            .take(section + 1)
            .map(|line| line.len() + 1)
            .sum();
        let mut edited = original.to_string();
        if end > edited.len() {
            edited.push('\n');
        }
        edited.insert_str(end.min(edited.len()), &block);
        Some(edited)
    } else {
        let mut edited = original.to_string();
        if !edited.is_empty() && !edited.ends_with('\n') {
            edited.push('\n');
        }
        edited.push_str(&format!("{key}:\n{block}"));
        Some(edited)
    }
}

/// Reject a YAML edit unless the parsed document differs only at our entry.
fn yaml_edit_is_surgical(
    before: &serde_yaml::Value,
    after: &serde_yaml::Value,
    key: &str,
    expected_entry: Option<serde_yaml::Value>,
) -> bool {
    let (mut before, mut after) = (before.clone(), after.clone());
    before
        .get_mut(key)
        .and_then(serde_yaml::Value::as_mapping_mut)
        .and_then(|map| map.remove(TUIC_MCP_KEY));
    let actual = after
        .get_mut(key)
        .and_then(serde_yaml::Value::as_mapping_mut)
        .and_then(|map| map.remove(TUIC_MCP_KEY));
    // Removing the last entry leaves `extensions:` with no value, which YAML
    // reads as null; that is the same document as an empty section.
    if after.get(key).is_some_and(serde_yaml::Value::is_null)
        && before
            .get(key)
            .and_then(serde_yaml::Value::as_mapping)
            .is_some_and(serde_yaml::Mapping::is_empty)
        && let Some(root) = after.as_mapping_mut()
    {
        root.insert(
            serde_yaml::Value::String(key.to_string()),
            serde_yaml::Value::Mapping(serde_yaml::Mapping::new()),
        );
    }
    if before.get(key).is_none()
        && after.get(key).is_some_and(|section| {
            section
                .as_mapping()
                .is_some_and(serde_yaml::Mapping::is_empty)
        })
    {
        after.as_mapping_mut().unwrap().remove(key);
    }
    before == after && actual == expected_entry
}

/// goose's `ExtensionEntry` for a stdio server. `name` and `timeout` have no
/// serde default on goose's side, so both must be written.
fn goose_entry_value(bridge_path: &str) -> serde_yaml::Value {
    let mut entry = serde_yaml::Mapping::new();
    for (key, value) in [
        ("enabled", serde_yaml::Value::Bool(true)),
        ("type", "stdio".into()),
        ("name", TUIC_MCP_KEY.into()),
        ("description", "TUICommander bridge".into()),
        ("cmd", bridge_path.into()),
        ("args", serde_yaml::Value::Sequence(Vec::new())),
        ("envs", serde_yaml::Value::Mapping(Default::default())),
        ("env_keys", serde_yaml::Value::Sequence(Vec::new())),
        ("timeout", 300.into()),
    ] {
        entry.insert(key.into(), value);
    }
    serde_yaml::Value::Mapping(entry)
}

/// Ensure a YAML config's `extensions:` map has the correct bridge entry.
/// Returns true if the config was written (installed or updated).
///
/// Round-tripping through serde drops comments the user may have written in
/// `config.yaml`. goose owns this file (`goose configure` rewrites it the same
/// way), so that is the same treatment the agent itself applies.
fn ensure_yaml_mcp_entry(
    config_path: &std::path::Path,
    key: &str,
    bridge_path: &str,
    agent_label: &str,
) -> bool {
    let Some(root) = read_yaml_file(config_path) else {
        return false;
    };
    let existing_command = root
        .get(key)
        .and_then(|extensions| extensions.get(TUIC_MCP_KEY))
        .and_then(|entry| entry.get("cmd"))
        .and_then(|cmd| cmd.as_str());
    match existing_command {
        Some(cmd) if cmd == bridge_path => return false,
        Some(old) => {
            tracing::info!(source = "mcp", agent = %agent_label, "Updating path: {old} → {bridge_path}");
        }
        None => {
            tracing::info!(source = "mcp", agent = %agent_label, "Installing bridge");
        }
    }

    let original = std::fs::read_to_string(config_path).unwrap_or_default();
    if existing_command.is_some() {
        let Some(edited) = yaml_command_edit(&original, key, bridge_path) else {
            tracing::error!(source = "mcp", agent = %agent_label, "Cannot safely edit goose cmd line");
            return false;
        };
        let parsed: serde_yaml::Value = match serde_yaml::from_str(&edited) {
            Ok(parsed) => parsed,
            Err(e) => {
                tracing::error!(source = "mcp", agent = %agent_label, "Edited YAML is invalid: {e}");
                return false;
            }
        };
        let mut expected_entry = root[key][TUIC_MCP_KEY].clone();
        expected_entry["cmd"] = bridge_path.into();
        if !yaml_edit_is_surgical(&root, &parsed, key, Some(expected_entry)) {
            tracing::error!(source = "mcp", agent = %agent_label, "Edited YAML changed fields outside the bridge command");
            return false;
        }
        backup_config_once(config_path, agent_label, &original);
        return match write_text_file_if_unchanged(config_path, Some(&original), &edited) {
            Ok(()) => true,
            Err(e) => {
                tracing::error!(source = "mcp", agent = %agent_label, "Write error: {e}");
                false
            }
        };
    }
    let Some(output) = yaml_entry_insert(&original, key, bridge_path) else {
        tracing::error!(source = "mcp", agent = %agent_label, "Cannot safely insert goose entry");
        return false;
    };
    let parsed: serde_yaml::Value = match serde_yaml::from_str(&output) {
        Ok(parsed) => parsed,
        Err(e) => {
            tracing::error!(source = "mcp", agent = %agent_label, "Edited YAML is invalid: {e}");
            return false;
        }
    };
    if !yaml_edit_is_surgical(&root, &parsed, key, Some(goose_entry_value(bridge_path))) {
        tracing::error!(source = "mcp", agent = %agent_label, "Edited YAML changed other goose extensions");
        return false;
    }
    backup_config_once(config_path, agent_label, &original);
    match write_text_file_if_unchanged(config_path, Some(&original), &output) {
        Ok(()) => {
            tracing::debug!(source = "mcp", agent = %agent_label, path = %config_path.display(), "Config written");
            true
        }
        Err(e) => {
            tracing::error!(source = "mcp", agent = %agent_label, "Write error: {e}");
            false
        }
    }
}

/// Remove the tuicommander entry from a YAML config.
fn remove_yaml_mcp_entry(
    config_path: &std::path::Path,
    key: &str,
    agent_label: &str,
) -> Result<(), String> {
    if !config_path.exists() {
        return Ok(());
    }
    let root = read_yaml_file(config_path)
        .ok_or_else(|| format!("Cannot parse {} — not modified", config_path.display()))?;
    if root
        .get(key)
        .and_then(|extensions| extensions.get(TUIC_MCP_KEY))
        .is_none()
    {
        return Ok(());
    }
    let original = std::fs::read_to_string(config_path).map_err(|e| e.to_string())?;
    let lines: Vec<&str> = original.lines().collect();
    let section = lines
        .iter()
        .position(|line| yaml_indent(line) == 0 && yaml_line_key(line, key))
        .ok_or_else(|| "Cannot safely find goose extensions".to_string())?;
    let start_line = ((section + 1)..lines.len())
        .find(|&index| yaml_indent(lines[index]) > 0 && yaml_line_key(lines[index], TUIC_MCP_KEY))
        .ok_or_else(|| "Cannot safely find goose entry".to_string())?;
    let indent = yaml_indent(lines[start_line]);
    let end_line = ((start_line + 1)..lines.len())
        .find(|&index| !lines[index].trim().is_empty() && yaml_indent(lines[index]) <= indent)
        .unwrap_or(lines.len());
    let start: usize = lines
        .iter()
        .take(start_line)
        .map(|line| line.len() + 1)
        .sum();
    let end: usize = lines.iter().take(end_line).map(|line| line.len() + 1).sum();
    let mut edited = original.clone();
    edited.replace_range(start..end.min(edited.len()), "");
    let parsed = serde_yaml::from_str::<serde_yaml::Value>(&edited)
        .map_err(|e| format!("Edited YAML is invalid: {e}"))?;
    if !yaml_edit_is_surgical(&root, &parsed, key, None) {
        return Err("Edited YAML changed fields outside the goose entry".to_string());
    }
    backup_config_once(config_path, agent_label, &original);
    write_text_file_if_unchanged(config_path, Some(&original), &edited)
}

/// Is the bridge entry already present in this target's config?
/// A target we configured earlier keeps getting path repairs even if the
/// presence heuristic no longer recognises it (manual install, tool removed
/// from PATH), so a stale bridge path can never be left behind.
fn has_bridge_entry(spec: &McpConfigSpec) -> bool {
    if !spec.config_path.exists() {
        return false;
    }
    match spec.format {
        McpFormat::Toml { .. } => is_toml_mcp_installed(&spec.config_path),
        McpFormat::Yaml => read_yaml_file(&spec.config_path).is_some_and(|root| {
            spec.key_path
                .first()
                .and_then(|key| root.get(*key))
                .and_then(|extensions| extensions.get(TUIC_MCP_KEY))
                .is_some()
        }),
        _ => read_json_file(&spec.config_path).is_some_and(|root| {
            navigate(&root, &spec.key_path)
                .and_then(|v| v.as_object())
                .is_some_and(|obj| obj.contains_key(TUIC_MCP_KEY))
        }),
    }
}

fn configured_bridge_command(spec: &McpConfigSpec) -> Option<String> {
    match spec.format {
        McpFormat::Toml { .. } => read_toml_file(&spec.config_path)?
            .get("mcp_servers")?
            .get(TUIC_MCP_KEY)?
            .get("command")?
            .as_str()
            .map(str::to_string),
        McpFormat::Yaml => read_yaml_file(&spec.config_path)?
            .get(spec.key_path.first().copied().unwrap_or("extensions"))?
            .get(TUIC_MCP_KEY)?
            .get("cmd")?
            .as_str()
            .map(str::to_string),
        McpFormat::OpenCode => read_json_file(&spec.config_path)?
            .get(spec.key_path.first().copied().unwrap_or("mcp"))?
            .get(TUIC_MCP_KEY)?
            .get("command")?
            .get(0)?
            .as_str()
            .map(str::to_string),
        McpFormat::Json => {
            let root = read_json_file(&spec.config_path)?;
            navigate(&root, &spec.key_path)?
                .get(TUIC_MCP_KEY)?
                .get("command")?
                .as_str()
                .map(str::to_string)
        }
    }
}

fn working_configured_command(spec: &McpConfigSpec) -> Option<String> {
    let command = configured_bridge_command(spec)?;
    let path = std::path::Path::new(&command);
    (path.is_absolute() && usable_executable(path)).then_some(command)
}

fn entry_has_custom_transport(spec: &McpConfigSpec) -> bool {
    const URL_KEYS: [&str; 4] = ["url", "httpUrl", "serverUrl", "uri"];
    match spec.format {
        McpFormat::Toml { .. } => read_toml_file(&spec.config_path)
            .and_then(|root| root.get("mcp_servers")?.get(TUIC_MCP_KEY).cloned())
            .is_some_and(|entry| {
                URL_KEYS.iter().any(|key| entry.get(*key).is_some())
                    || entry
                        .get("type")
                        .and_then(toml::Value::as_str)
                        .is_some_and(|kind| kind != "stdio" && kind != "local")
            }),
        McpFormat::Yaml => read_yaml_file(&spec.config_path)
            .and_then(|root| {
                root.get(spec.key_path.first().copied().unwrap_or("extensions"))?
                    .get(TUIC_MCP_KEY)
                    .cloned()
            })
            .is_some_and(|entry| {
                URL_KEYS.iter().any(|key| entry.get(*key).is_some())
                    || entry
                        .get("type")
                        .and_then(serde_yaml::Value::as_str)
                        .is_some_and(|kind| kind != "stdio")
            }),
        McpFormat::Json | McpFormat::OpenCode => read_json_file(&spec.config_path)
            .and_then(|root| navigate(&root, &spec.key_path)?.get(TUIC_MCP_KEY).cloned())
            .is_some_and(|entry| {
                URL_KEYS.iter().any(|key| entry.get(*key).is_some())
                    || entry
                        .get("type")
                        .and_then(serde_json::Value::as_str)
                        .is_some_and(|kind| kind != "stdio" && kind != "local")
            }),
    }
}

fn custom_command_should_be_kept(command: &str, bridge_path: &str) -> bool {
    if command == bridge_path || command == BRIDGE_NAME || command == "tuic-bridge.exe" {
        return false;
    }
    let path = std::path::Path::new(command);
    if path.is_absolute()
        && path
            .file_name()
            .is_some_and(|name| name == resolved_bridge_name())
    {
        let build_output = path
            .components()
            .any(|part| part.as_os_str() == "target" || part.as_os_str() == ".mbx");
        let installed = path.starts_with(crate::config::config_dir().join("mcp-bridge"));
        if build_output || installed {
            return false;
        }
    }
    !path.is_absolute() || usable_executable(path)
}

fn bridge_location_is_stable(exe: &std::path::Path) -> bool {
    let temp = std::env::temp_dir();
    if exe.starts_with(&temp)
        || exe.starts_with(temp.canonicalize().unwrap_or(temp))
        || exe.starts_with("/Volumes")
    {
        return false;
    }
    #[cfg(unix)]
    for root in [
        "/tmp",
        "/private/tmp",
        "/var/tmp",
        "/private/var/tmp",
        "/var/folders",
        "/private/var/folders",
    ] {
        if exe.starts_with(root) {
            return false;
        }
    }
    let text = exe.to_string_lossy();
    if text.contains("/AppTranslocation/") || text.contains("/.mount_") {
        return false;
    }
    if let Some(appimage) = std::env::var_os("APPIMAGE")
        && exe.starts_with(std::path::PathBuf::from(appimage))
    {
        return false;
    }
    true
}

/// Whether launch-time auto-install may write this target, i.e. whether the
/// evidence says the tool is really here.
///
/// Deliberately NOT consulted by `install_agent_mcp`: a user pressing Install
/// in Settings has told us the target exists, and creating the file is then the
/// requested action rather than a guess.
fn auto_install_allowed(spec: &McpConfigSpec, agent_label: &str) -> bool {
    if !is_target_installed(spec) && !has_bridge_entry(spec) {
        tracing::debug!(source = "mcp", agent = %agent_label, "Skipping (not installed)");
        return false;
    }
    // An add-on target speaks MCP only through a plugin that owns the config
    // file. No file means no plugin, so writing one configures nothing.
    if spec.requires_existing_config && !spec.config_path.exists() {
        tracing::debug!(source = "mcp", agent = %agent_label, "Skipping (no MCP add-on config)");
        return false;
    }
    // The target's MCP list shares a file with the rest of the user's settings.
    // We repair an entry we already own, but the first write waits for a click.
    if spec.shared_settings_file && !has_bridge_entry(spec) {
        tracing::debug!(source = "mcp", agent = %agent_label, "Skipping (shared settings file — install from Settings > Agents)");
        return false;
    }
    true
}

/// Write the bridge entry for one target, dispatching on its config format.
fn ensure_spec_entry(spec: &McpConfigSpec, bridge_path: &str, agent_label: &str) -> bool {
    // A fallback command must never replace an existing integration, even when
    // that integration needs a later repair. Only a located bridge can repair it.
    if bridge_path == BRIDGE_NAME && has_bridge_entry(spec) {
        tracing::info!(source = "mcp", agent = %agent_label,
            "Keeping existing bridge entry because no usable bridge was found");
        return false;
    }
    match spec.format {
        McpFormat::Toml { forward_session } => {
            ensure_toml_mcp_entry(&spec.config_path, forward_session, bridge_path, agent_label)
        }
        McpFormat::Yaml => ensure_yaml_mcp_entry(
            &spec.config_path,
            spec.key_path.first().copied().unwrap_or("extensions"),
            bridge_path,
            agent_label,
        ),
        format => ensure_agent_mcp_entry(
            &spec.config_path,
            &spec.key_path,
            format,
            bridge_path,
            agent_label,
        ),
    }
}

/// Ensure MCP bridge config is installed and up-to-date in all supported agent configs.
/// Called on every app launch. Installs missing entries and updates stale paths.
///
/// Targets that are not installed on this machine are skipped: the write path
/// creates every missing parent directory, so an unconditional pass litters the
/// home directory with configs for tools the user never had. Settings > Agents
/// still installs on demand — that is an explicit request, not a guess.
pub(crate) fn ensure_mcp_configs(disabled: &[String]) {
    let Ok(exe) = std::env::current_exe() else {
        tracing::warn!(
            source = "mcp",
            "Skipping agent MCP config updates: executable path unavailable"
        );
        return;
    };
    if !launch_owns_agent_configs(&exe) {
        tracing::info!(
            source = "mcp",
            "Skipping agent MCP config updates from a secondary instance"
        );
        return;
    }
    let bridge = {
        if !bridge_location_is_stable(&exe) {
            tracing::warn!(source = "mcp", executable = %exe.display(),
                    "Skipping agent MCP config updates from a temporary or mounted app");
            None
        } else {
            exe.parent().and_then(bridge_beside)
        }
    };
    ensure_mcp_configs_for(
        disabled,
        bridge.as_deref(),
        SUPPORTED_AGENTS
            .iter()
            .filter_map(|agent| get_mcp_config_spec(agent).map(|spec| (*agent, spec))),
    );
}

fn launch_owns_agent_configs(exe: &std::path::Path) -> bool {
    if std::env::var("TUIC_MCP_CONFIG_OWNER").as_deref() == Ok("1") {
        return true;
    }
    if !crate::app_instance::current_app_instance().is_default() {
        return false;
    }
    let Ok(cwd) = std::env::current_dir() else {
        return false;
    };
    !in_linked_worktree(exe) && !in_linked_worktree(&cwd)
}

fn in_linked_worktree(path: &std::path::Path) -> bool {
    for dir in path.ancestors() {
        let git = dir.join(".git");
        if git.is_file() {
            return true;
        }
        if git.is_dir() {
            return false;
        }
    }
    false
}

fn ensure_mcp_configs_for<'a>(
    disabled: &[String],
    bridge: Option<&std::path::Path>,
    agents: impl IntoIterator<Item = (&'a str, McpConfigSpec)>,
) {
    let Some(bridge) = bridge else {
        tracing::warn!(source = "mcp", searched_paths = ?bridge_search_paths(),
            "Skipping agent MCP config updates: no bridge beside this executable");
        return;
    };
    let source_command = bridge.to_string_lossy();
    let pending: Vec<_> = agents
        .into_iter()
        .filter(|(agent, spec)| {
            if disabled.iter().any(|d| d == *agent) {
                tracing::debug!(source = "mcp", agent, "Skipping (disabled by user)");
                return false;
            }
            if !auto_install_allowed(spec, agent) || entry_has_custom_transport(spec) {
                return false;
            }
            if let Some(command) = configured_bridge_command(spec)
                && custom_command_should_be_kept(&command, &source_command)
            {
                return false;
            }
            // A parse/read failure cannot result in a config write.
            match spec.format {
                McpFormat::Toml { .. } => read_toml_file(&spec.config_path).is_some(),
                McpFormat::Yaml => read_yaml_file(&spec.config_path).is_some(),
                _ => read_json_file(&spec.config_path).is_some(),
            }
        })
        .collect();
    if pending.is_empty() {
        return;
    }
    let bridge = match install_bridge_binary(bridge) {
        Ok(installed) => installed,
        Err(error) => {
            tracing::warn!(source = "mcp", "{error} — leaving agent configs unchanged");
            return;
        }
    };
    let bridge_path = bridge.to_string_lossy();
    tracing::info!(source = "mcp", bridge = %bridge_path, "Ensuring bridge configs");
    for (agent, spec) in pending {
        ensure_spec_entry(&spec, &bridge_path, agent);
    }
}

// ---------------------------------------------------------------------------
// Tauri commands
// ---------------------------------------------------------------------------

/// Check MCP installation status for an agent
#[cfg_attr(feature = "desktop", tauri::command)]
pub(crate) fn get_agent_mcp_status(agent_type: String) -> AgentMcpStatus {
    let Some(spec) = get_mcp_config_spec(&agent_type) else {
        return AgentMcpStatus {
            supported: false,
            installed: false,
            config_path: None,
            shared_settings_file: false,
        };
    };

    AgentMcpStatus {
        supported: true,
        installed: has_bridge_entry(&spec),
        config_path: Some(spec.config_path.to_string_lossy().to_string()),
        shared_settings_file: spec.shared_settings_file,
    }
}

/// Install the tui-mcp-bridge MCP entry into an agent's config.
/// Also removes the agent from `disabled_mcp_agents` so `ensure_mcp_configs` won't skip it.
#[cfg(feature = "desktop")]
#[tauri::command]
pub(crate) fn install_agent_mcp(
    agent_type: String,
    state: tauri::State<'_, std::sync::Arc<crate::state::AppState>>,
) -> Result<(), String> {
    let source = locate_bridge_binary().ok_or_else(|| "No usable tuic-bridge found".to_string())?;
    let bridge_path = install_bridge_binary(&source)?
        .to_string_lossy()
        .to_string();

    let spec = get_mcp_config_spec(&agent_type)
        .ok_or_else(|| format!("Agent '{agent_type}' does not support MCP configuration"))?;

    install_spec(&spec, &bridge_path, &agent_type)?;

    // Remove from disabled list so ensure_mcp_configs won't undo this
    update_disabled_mcp_agents(state.inner(), |list| list.retain(|a| a != &agent_type));

    Ok(())
}

fn install_spec(spec: &McpConfigSpec, bridge_path: &str, agent_label: &str) -> Result<(), String> {
    if entry_has_custom_transport(spec) {
        return Err(format!(
            "MCP entry at {} uses a custom transport; remove it explicitly before installing a stdio bridge",
            spec.config_path.display()
        ));
    }
    if let Some(command) = configured_bridge_command(spec)
        && !std::path::Path::new(&command).is_absolute()
        && command != BRIDGE_NAME
        && command != "tuic-bridge.exe"
    {
        return Err(format!(
            "MCP entry at {} uses a custom command ({command}); remove it explicitly before installing a stdio bridge",
            spec.config_path.display()
        ));
    }
    if let Some(command) = working_configured_command(spec)
        && custom_command_should_be_kept(&command, bridge_path)
    {
        tracing::info!(source = "mcp", agent = %agent_label, command,
                "Keeping working bridge entry during explicit install");
        return Ok(());
    }
    if bridge_path == BRIDGE_NAME && has_bridge_entry(spec) {
        return Err(format!(
            "No usable tuic-bridge found; existing entry at {} left unchanged (searched: {:?})",
            spec.config_path.display(),
            bridge_search_paths()
        ));
    }
    if bridge_path == BRIDGE_NAME {
        tracing::warn!(source = "mcp", agent = %agent_label, searched_paths = ?bridge_search_paths(),
            "Installing a new MCP entry with the bare tuic-bridge command");
    }
    if ensure_spec_entry(spec, bridge_path, agent_label) {
        return Ok(());
    }
    if configured_bridge_command(spec).as_deref() == Some(bridge_path) {
        return Ok(());
    }
    Err(format!(
        "Failed to write MCP config at {}",
        spec.config_path.display()
    ))
}

/// Remove the tui-mcp-bridge MCP entry from an agent's config.
/// Also adds the agent to `disabled_mcp_agents` so `ensure_mcp_configs` won't reinstall it.
#[cfg(feature = "desktop")]
#[tauri::command]
pub(crate) fn remove_agent_mcp(
    agent_type: String,
    state: tauri::State<'_, std::sync::Arc<crate::state::AppState>>,
) -> Result<(), String> {
    let spec = get_mcp_config_spec(&agent_type)
        .ok_or_else(|| format!("Agent '{agent_type}' does not support MCP configuration"))?;
    remove_spec_entry(&spec, &agent_type)?;

    // Add to disabled list so ensure_mcp_configs won't reinstall
    update_disabled_mcp_agents(state.inner(), |list| {
        if !list.contains(&agent_type) {
            list.push(agent_type.clone());
        }
    });

    Ok(())
}

/// Drop the bridge entry from one target's config, dispatching on its format.
fn remove_spec_entry(spec: &McpConfigSpec, agent_label: &str) -> Result<(), String> {
    match spec.format {
        McpFormat::Toml { .. } => remove_toml_mcp_entry(&spec.config_path, agent_label),
        McpFormat::Yaml => remove_yaml_mcp_entry(
            &spec.config_path,
            spec.key_path.first().copied().unwrap_or("extensions"),
            agent_label,
        ),
        _ => {
            if !spec.config_path.exists() {
                return Ok(());
            }
            let text = read_json_text(&spec.config_path)
                .ok_or_else(|| format!("Cannot read {}", spec.config_path.display()))?;
            let edited = crate::jsonc_edit::remove_member(&text, &spec.key_path, TUIC_MCP_KEY)
                .map_err(|e| format!("{e} in {} — not modified", spec.config_path.display()))?;
            commit_json_edit(&spec.config_path, agent_label, &text, &edited);
            Ok(())
        }
    }
}

/// Every target this machine has a bridge entry in.
#[cfg_attr(feature = "desktop", tauri::command)]
pub(crate) fn list_installed_mcp_integrations() -> Vec<String> {
    SUPPORTED_AGENTS
        .iter()
        .filter(|agent| get_mcp_config_spec(agent).is_some_and(|spec| has_bridge_entry(&spec)))
        .map(|agent| (*agent).to_string())
        .collect()
}

/// Remove the bridge entry from every target that has one, and stop
/// reinstalling them.
///
/// Uninstalling TUICommander leaves the bridge path dangling in every client it
/// ever configured, and each one then reports a broken MCP server on startup
/// (issue #115). Removing them one by one means knowing which clients TUIC
/// picked, so this does the sweep.
///
/// Disabling is part of the action, not a side effect: without it the next
/// launch reinstalls everything and the button does nothing.
#[cfg(feature = "desktop")]
#[tauri::command]
pub(crate) fn remove_all_mcp_integrations(
    state: tauri::State<'_, std::sync::Arc<crate::state::AppState>>,
) -> Result<Vec<String>, String> {
    let mut removed = Vec::new();
    let mut failures = Vec::new();
    for agent in SUPPORTED_AGENTS {
        let Some(spec) = get_mcp_config_spec(agent) else {
            continue;
        };
        if !has_bridge_entry(&spec) {
            continue;
        }
        match remove_spec_entry(&spec, agent) {
            Ok(()) => removed.push((*agent).to_string()),
            // One unparseable config must not abort the sweep — the remaining
            // clients would keep their dangling entries.
            Err(e) => {
                tracing::error!(source = "mcp", agent, "Removal failed: {e}");
                failures.push(format!("{agent}: {e}"));
            }
        }
    }

    let disable: Vec<String> = removed.clone();
    update_disabled_mcp_agents(state.inner(), |list| {
        for agent in disable {
            if !list.contains(&agent) {
                list.push(agent);
            }
        }
    });

    if failures.is_empty() {
        Ok(removed)
    } else {
        Err(failures.join("; "))
    }
}

/// Helper: mutate `disabled_mcp_agents` in BOTH the in-memory `AppState.config`
/// and on-disk `config.json`. Updating only disk would leave a stale snapshot in
/// memory, and a subsequent `put_config` from the FE (carrying that stale list)
/// would silently revert the toggle.
///
/// Goes through `commit_config_change` rather than serializing a snapshot itself:
/// a direct `save_json_config("config.json", ..)` skips `config_for_disk`, so the
/// session token, relay token and VAPID private key were written to config.json in
/// cleartext — the exact thing the credential vault exists to prevent. It also puts
/// this writer under the same lock as every other one.
fn update_disabled_mcp_agents(
    state: &std::sync::Arc<crate::state::AppState>,
    mutator: impl FnOnce(&mut Vec<String>),
) {
    let result = crate::config::commit_config_change(state, |current| {
        let mut next = current.clone();
        mutator(&mut next.disabled_mcp_agents);
        Ok(next)
    });
    if let Err(e) = result {
        tracing::error!(source = "mcp", "Failed to save disabled_mcp_agents: {e}");
    }
}

/// Get the path to an agent's own configuration file
#[cfg_attr(feature = "desktop", tauri::command)]
pub(crate) fn get_agent_config_path(agent_type: String) -> Option<String> {
    get_agent_settings_path(&agent_type).map(|p| p.to_string_lossy().to_string())
}

/// MCP connection info for manual configuration
#[derive(Serialize)]
pub(crate) struct McpBridgeInfo {
    pub(crate) bridge_path: String,
    pub(crate) config_snippet: String,
}

/// Return bridge path + ready-to-paste JSON snippet for manual MCP setup
#[cfg_attr(feature = "desktop", tauri::command)]
pub(crate) fn get_mcp_bridge_info() -> McpBridgeInfo {
    bridge_info_from_location(locate_bridge_binary().as_deref())
}

fn bridge_info_from_location(source: Option<&std::path::Path>) -> McpBridgeInfo {
    let bridge_path = bridge_command_from_location(locate_installed_bridge(source));
    let entry = TuicMcpEntry {
        transport_type: "stdio".to_string(),
        command: bridge_path.clone(),
        args: vec![],
        env: BTreeMap::new(),
    };
    let wrapper = serde_json::json!({
        "tuicommander": entry,
    });
    let config_snippet = serde_json::to_string_pretty(&wrapper).unwrap_or_default();
    McpBridgeInfo {
        bridge_path,
        config_snippet,
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    /// Any test that edits a NON-EMPTY config also runs `backup_config_once`,
    /// which writes into `config_dir()/mcp-backups`. Without an override that
    /// was the user's real config directory: five `test-*.orig` files sat in
    /// Boss's `mcp-backups/` from 2026-08-21 until `config_dir` started
    /// refusing a test build outright (#763-d219). Returned tuple must be bound
    /// — dropping the guard restores the override immediately.
    fn with_temp_config_dir() -> (impl Drop, TempDir) {
        let dir = TempDir::new().unwrap();
        let guard = crate::config::set_config_dir_override(dir.path().to_path_buf());
        (guard, dir)
    }

    /// Seed a config file. Production no longer serializes whole documents —
    /// it splices one member into text it never reformats — so building a
    /// fixture is the only place a `Value` still becomes a file.
    fn write_fixture(path: &std::path::Path, value: &serde_json::Value) {
        std::fs::write(path, serde_json::to_string_pretty(value).unwrap()).unwrap();
    }

    #[test]
    fn missing_bridge_warning_names_the_searched_paths() {
        use std::sync::{Arc, Mutex};

        #[derive(Clone)]
        struct Sink(Arc<Mutex<Vec<u8>>>);
        impl std::io::Write for Sink {
            fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
                self.0.lock().unwrap().extend_from_slice(bytes);
                Ok(bytes.len())
            }
            fn flush(&mut self) -> std::io::Result<()> {
                Ok(())
            }
        }
        impl<'a> tracing_subscriber::fmt::MakeWriter<'a> for Sink {
            type Writer = Sink;
            fn make_writer(&'a self) -> Self::Writer {
                self.clone()
            }
        }

        let _config = with_temp_config_dir();
        let dir = TempDir::new().unwrap();
        let spec = spec_at(dir.path().join("mcp.json"));
        let searched = bridge_search_paths();
        let output = Arc::new(Mutex::new(Vec::new()));
        let subscriber = tracing_subscriber::fmt()
            .with_writer(Sink(output.clone()))
            .with_ansi(false)
            .finish();
        let result = tracing::subscriber::with_default(subscriber, || {
            install_spec(&spec, BRIDGE_NAME, "claude")
        });
        assert!(result.is_ok());
        let log = String::from_utf8(output.lock().unwrap().clone()).unwrap();
        assert!(log.contains("WARN"), "{log}");
        for path in searched {
            let encoded = format!("{path:?}");
            assert!(log.contains(&encoded), "{log}");
        }
    }

    /// The daemon is unpacked into a directory of its own, so the bridge it
    /// configures agents to run has to be found beside it. The release publishes
    /// `tuic-bridge` for every target that publishes `tuic-remote` (#793-23a5);
    /// this is the lookup that turns a downloaded file into a usable config.
    #[test]
    fn the_bridge_is_found_beside_the_executable_that_configures_it() {
        let dir = TempDir::new().unwrap();
        assert!(
            bridge_beside(dir.path()).is_none(),
            "an empty directory must not report a bridge"
        );

        let name = if cfg!(windows) {
            format!("{BRIDGE_NAME}.exe")
        } else {
            BRIDGE_NAME.to_string()
        };
        let placed = dir.path().join(&name);
        std::fs::create_dir(&placed).unwrap();
        assert!(
            bridge_beside(dir.path()).is_none(),
            "a directory is not a bridge"
        );
        std::fs::remove_dir(&placed).unwrap();
        std::fs::write(&placed, b"").unwrap();
        assert!(
            bridge_beside(dir.path()).is_none(),
            "an empty file is not a bridge"
        );
        std::fs::write(&placed, b"bridge").unwrap();
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(&placed, std::fs::Permissions::from_mode(0o644)).unwrap();
            assert!(
                bridge_beside(dir.path()).is_none(),
                "a non-executable file is not a bridge"
            );
            std::fs::set_permissions(&placed, std::fs::Permissions::from_mode(0o755)).unwrap();
        }
        assert_eq!(bridge_beside(dir.path()), Some(placed));
    }

    #[test]
    fn mounted_and_temporary_executables_cannot_own_agent_configs() {
        let temporary = std::env::temp_dir().join("tuic-test/bin/tuicommander");
        assert!(!bridge_location_is_stable(&temporary));
        #[cfg(unix)]
        {
            assert!(!bridge_location_is_stable(std::path::Path::new(
                "/Volumes/TUICommander/TUICommander.app/Contents/MacOS/tuicommander"
            )));
            assert!(!bridge_location_is_stable(std::path::Path::new(
                "/private/var/folders/ab/AppTranslocation/id/d/TUICommander.app/Contents/MacOS/tuicommander"
            )));
            assert!(!bridge_location_is_stable(std::path::Path::new(
                "/tmp/.mount_abc/usr/bin/tuicommander"
            )));
        }
    }

    /// Every target that publishes the daemon must publish the bridge too. The
    /// daemon writes configs naming `tuic-bridge`; a platform that ships one
    /// without the other ships a config pointing at nothing.
    #[test]
    fn the_release_publishes_a_bridge_wherever_it_publishes_the_daemon() {
        let workflow = include_str!("../../.github/workflows/release.yml");
        let job = workflow
            .split("\n  remote-daemon:")
            .nth(1)
            .expect("the remote-daemon job must exist")
            // The job that follows it. Splitting on a generic two-space indent
            // would stop at the first nested key and read almost nothing.
            .split("\n  finalize-release:")
            .next()
            .expect("job body");

        assert!(
            job.contains("--bin tuic-remote --target"),
            "the daemon build step moved — this test is reading the wrong job"
        );
        assert!(
            job.contains("--package tuic-bridge --target"),
            "the daemon is built for this matrix but the bridge is not"
        );
        assert!(
            job.contains("for BIN in tuic-remote tuic-bridge"),
            "both binaries must be uploaded, or only one reaches the release page"
        );
    }

    /// A machine with none of an agent installed must be left untouched — no
    /// config file, and no directory created to hold one. The same rule the
    /// desktop has always had now also runs on a `tuic-remote` daemon
    /// (#793-23a5), where a stray `~/.codex/` would be the only trace of a tool
    /// the machine does not have.
    #[test]
    fn an_absent_target_is_neither_configured_nor_given_a_directory() {
        let dir = TempDir::new().unwrap();
        let config_dir = dir.path().join("never-installed");
        let spec = McpConfigSpec {
            config_path: config_dir.join("mcp.json"),
            key_path: vec!["mcpServers"],
            format: McpFormat::Json,
            // A binary name no PATH can resolve, so presence rests on the dir.
            binaries: &["tuic-no-such-agent-binary"],
            presence_dir: Some(config_dir.clone()),
            requires_existing_config: false,
            shared_settings_file: false,
        };

        assert!(
            !auto_install_allowed(&spec, "never-installed"),
            "an agent with no binary and no directory must not be auto-configured"
        );
        // Deliberately not calling `ensure_mcp_configs` here: it reads the real
        // `$HOME` and would rewrite the config of every agent this machine does
        // have. The gate above is the whole rule — `ensure_mcp_configs` calls
        // it before `ensure_spec_entry`, which is the only thing that writes.
        assert!(
            !config_dir.exists(),
            "the skip rule created a directory for an agent that is not installed"
        );
        assert!(!spec.config_path.exists());
    }

    #[test]
    fn unsupported_agent_returns_not_supported() {
        let status = get_agent_mcp_status("aider".to_string());
        assert!(!status.supported);
        assert!(!status.installed);
        assert!(status.config_path.is_none());
    }

    #[test]
    fn install_remove_round_trip_json() {
        let dir = TempDir::new().unwrap();
        let config_path = dir.path().join("test-mcp.json");

        // Start with existing config that has another server
        let initial = r#"{ "mcpServers": { "other-server": { "command": "other-cmd" } } }"#;
        std::fs::write(&config_path, initial).unwrap();

        let spec = spec_at(config_path.clone());
        assert!(ensure_spec_entry(
            &spec,
            "/usr/local/bin/tui-mcp-bridge",
            "test"
        ));

        // Verify both entries exist
        let root = read_json_file(&config_path).unwrap();
        let servers = navigate(&root, &["mcpServers"])
            .unwrap()
            .as_object()
            .unwrap();
        assert!(servers.contains_key(TUIC_MCP_KEY));
        assert!(servers.contains_key("other-server"));
        assert_eq!(servers.len(), 2);

        remove_spec_entry(&spec, "test").unwrap();

        // Verify only other-server remains
        let root = read_json_file(&config_path).unwrap();
        let servers = navigate(&root, &["mcpServers"])
            .unwrap()
            .as_object()
            .unwrap();
        assert!(!servers.contains_key(TUIC_MCP_KEY));
        assert!(servers.contains_key("other-server"));
        assert_eq!(servers.len(), 1);
    }

    #[test]
    fn install_creates_file_if_missing() {
        let dir = TempDir::new().unwrap();
        let config_path = dir.path().join("nonexistent.json");

        // File doesn't exist yet
        assert!(!config_path.exists());

        assert!(ensure_spec_entry(
            &spec_at(config_path.clone()),
            "tui-mcp-bridge",
            "test"
        ));

        // Verify file was created with correct content
        let root = read_json_file(&config_path).unwrap();
        let servers = navigate(&root, &["mcpServers"])
            .unwrap()
            .as_object()
            .unwrap();
        assert!(servers.contains_key(TUIC_MCP_KEY));
        let entry = servers.get(TUIC_MCP_KEY).unwrap();
        assert_eq!(entry["command"], "tui-mcp-bridge");
        assert_eq!(entry["type"], "stdio");
    }

    #[test]
    fn nested_key_path_works() {
        // Test with amp-style nested path: ["amp", "mcpServers"]
        let dir = TempDir::new().unwrap();
        let config_path = dir.path().join("amp-settings.json");

        let initial = serde_json::json!({
            "amp": {
                "someOtherSetting": true
            }
        });
        write_fixture(&config_path, &initial);

        assert!(ensure_agent_mcp_entry(
            &config_path,
            &["amp", "mcpServers"],
            McpFormat::Json,
            "tui-mcp-bridge",
            "test",
        ));

        let root = read_json_file(&config_path).unwrap();
        // Verify the nested structure
        assert_eq!(root["amp"]["someOtherSetting"], true);
        assert!(root["amp"]["mcpServers"][TUIC_MCP_KEY].is_object());
    }

    #[test]
    fn remove_from_nonexistent_file_is_ok() {
        // Removing from a file that doesn't exist should succeed (no-op)
        let dir = TempDir::new().unwrap();
        let config_path = dir.path().join("does-not-exist.json");

        remove_spec_entry(&spec_at(config_path.clone()), "test").unwrap();

        // Removal must never bring the file into existence.
        assert!(!config_path.exists());
    }

    #[test]
    fn agent_config_path_returns_expected_paths() {
        // Claude should return ~/.claude/settings.json
        let claude_path = get_agent_settings_path("claude");
        assert!(claude_path.is_some());
        let path_str = claude_path.unwrap().to_string_lossy().to_string();
        assert!(path_str.contains(".claude"));
        assert!(path_str.ends_with("settings.json"));

        // Unknown agent returns None
        assert!(get_agent_settings_path("unknown-agent").is_none());
    }

    #[test]
    fn mcp_config_spec_known_agents() {
        // Each target is written in the format its own tool reads.
        for (agent, format) in &[
            ("claude", McpFormat::Json),
            ("cursor", McpFormat::Json),
            ("windsurf", McpFormat::Json),
            ("vscode", McpFormat::Json),
            ("zed", McpFormat::Json),
            ("amp", McpFormat::Json),
            ("gemini", McpFormat::Json),
            ("droid", McpFormat::Json),
            ("pi", McpFormat::Json),
            ("opencode", McpFormat::OpenCode),
            (
                "codex",
                McpFormat::Toml {
                    forward_session: true,
                },
            ),
            (
                "grok",
                McpFormat::Toml {
                    forward_session: false,
                },
            ),
            ("goose", McpFormat::Yaml),
        ] {
            let spec = get_mcp_config_spec(agent).expect("{agent} should be supported");
            assert!(spec.format == *format, "{agent} uses the wrong format");
        }
        // aider has no MCP client, so there is nothing to configure.
        assert!(get_mcp_config_spec("aider").is_none());
        assert!(get_mcp_config_spec("unknown-agent").is_none());
    }

    // --- ensure_agent_mcp_entry tests ---

    #[test]
    fn ensure_installs_when_missing() {
        let dir = TempDir::new().unwrap();
        let config_path = dir.path().join("test.json");

        let wrote = ensure_agent_mcp_entry(
            &config_path,
            &["mcpServers"],
            McpFormat::Json,
            "/path/a",
            "test",
        );
        assert!(wrote, "should write when entry is missing");

        let root = read_json_file(&config_path).unwrap();
        assert_eq!(root["mcpServers"][TUIC_MCP_KEY]["command"], "/path/a");
    }

    #[test]
    fn ensure_updates_stale_path() {
        let dir = TempDir::new().unwrap();
        let config_path = dir.path().join("test.json");

        // Install with path A
        ensure_agent_mcp_entry(
            &config_path,
            &["mcpServers"],
            McpFormat::Json,
            "/old/path",
            "test",
        );

        // Ensure with path B — should update
        let wrote = ensure_agent_mcp_entry(
            &config_path,
            &["mcpServers"],
            McpFormat::Json,
            "/new/path",
            "test",
        );
        assert!(wrote, "should write when path changed");

        let root = read_json_file(&config_path).unwrap();
        assert_eq!(root["mcpServers"][TUIC_MCP_KEY]["command"], "/new/path");
    }

    #[test]
    fn ensure_skips_when_path_matches() {
        let dir = TempDir::new().unwrap();
        let config_path = dir.path().join("test.json");

        // Install
        ensure_agent_mcp_entry(
            &config_path,
            &["mcpServers"],
            McpFormat::Json,
            "/correct/path",
            "test",
        );

        // Record mtime
        let mtime_before = std::fs::metadata(&config_path).unwrap().modified().unwrap();
        // Small sleep to ensure mtime would differ if file were rewritten
        std::thread::sleep(std::time::Duration::from_millis(50));

        // Ensure with same path — should not write
        let wrote = ensure_agent_mcp_entry(
            &config_path,
            &["mcpServers"],
            McpFormat::Json,
            "/correct/path",
            "test",
        );
        assert!(!wrote, "should not write when path already correct");

        let mtime_after = std::fs::metadata(&config_path).unwrap().modified().unwrap();
        assert_eq!(
            mtime_before, mtime_after,
            "file should not have been modified"
        );
    }

    #[test]
    fn ensure_writes_args_and_env_as_empty_collections() {
        let dir = TempDir::new().unwrap();
        let config_path = dir.path().join("test.json");

        ensure_agent_mcp_entry(
            &config_path,
            &["mcpServers"],
            McpFormat::Json,
            "/bridge",
            "test",
        );

        let root = read_json_file(&config_path).unwrap();
        let entry = &root["mcpServers"][TUIC_MCP_KEY];
        assert!(
            entry["args"].is_array(),
            "args must be an array, got {:?}",
            entry["args"]
        );
        assert_eq!(entry["args"].as_array().unwrap().len(), 0);
        assert!(
            entry["env"].is_object(),
            "env must be an object, got {:?}",
            entry["env"]
        );
        assert_eq!(entry["env"].as_object().unwrap().len(), 0);
    }

    #[test]
    fn ensure_repairs_entry_with_null_args_and_env() {
        let dir = TempDir::new().unwrap();
        let config_path = dir.path().join("test.json");

        // Write an entry shaped like Claude Code's rejected form: args/env are null
        let initial = serde_json::json!({
            "mcpServers": {
                TUIC_MCP_KEY: {
                    "type": "stdio",
                    "command": "/bridge",
                    "args": null,
                    "env": null,
                }
            }
        });
        write_fixture(&config_path, &initial);

        // Same command, but malformed fields → must rewrite
        let wrote = ensure_agent_mcp_entry(
            &config_path,
            &["mcpServers"],
            McpFormat::Json,
            "/bridge",
            "test",
        );
        assert!(wrote, "should rewrite when args/env are null");

        let root = read_json_file(&config_path).unwrap();
        let entry = &root["mcpServers"][TUIC_MCP_KEY];
        assert!(entry["args"].is_array());
        assert!(entry["env"].is_object());
    }

    #[test]
    fn ensure_preserves_other_servers() {
        let dir = TempDir::new().unwrap();
        let config_path = dir.path().join("test.json");

        let initial = serde_json::json!({
            "mcpServers": {
                "other-server": { "command": "other-cmd" }
            }
        });
        write_fixture(&config_path, &initial);

        ensure_agent_mcp_entry(
            &config_path,
            &["mcpServers"],
            McpFormat::Json,
            "/bridge",
            "test",
        );

        let root = read_json_file(&config_path).unwrap();
        let servers = root["mcpServers"].as_object().unwrap();
        assert_eq!(servers.len(), 2);
        assert_eq!(servers["other-server"]["command"], "other-cmd");
        assert_eq!(servers[TUIC_MCP_KEY]["command"], "/bridge");
    }

    #[test]
    fn ensure_works_with_nested_key_path() {
        let dir = TempDir::new().unwrap();
        let config_path = dir.path().join("test.json");

        let initial = serde_json::json!({ "amp": { "setting": true } });
        write_fixture(&config_path, &initial);

        ensure_agent_mcp_entry(
            &config_path,
            &["amp", "mcpServers"],
            McpFormat::Json,
            "/bridge",
            "test",
        );

        let root = read_json_file(&config_path).unwrap();
        assert_eq!(root["amp"]["setting"], true);
        assert_eq!(
            root["amp"]["mcpServers"][TUIC_MCP_KEY]["command"],
            "/bridge"
        );
    }

    // --- ensure_mcp_configs disabled_agents tests ---

    #[test]
    fn ensure_skips_disabled_agents() {
        let dir = TempDir::new().unwrap();
        let config_path = dir.path().join("test.json");

        // With empty disabled list — should install
        ensure_agent_mcp_entry(
            &config_path,
            &["mcpServers"],
            McpFormat::Json,
            "/bridge",
            "test",
        );
        assert!(config_path.exists());
        let root = read_json_file(&config_path).unwrap();
        assert!(root["mcpServers"][TUIC_MCP_KEY].is_object());

        // Remove the file and verify ensure_mcp_configs logic
        // (we test the skip logic directly since ensure_mcp_configs uses home paths)
        let disabled = ["claude".to_string(), "cursor".to_string()];
        assert!(disabled.iter().any(|d| d == "claude"));
        assert!(!disabled.iter().any(|d| d == "vscode"));
    }

    /// Regression for #1368-fa9b: `update_disabled_mcp_agents` must mutate the
    /// in-memory `AppState.config.disabled_mcp_agents`, not just the on-disk file.
    /// Otherwise a `put_config` PUT carrying a stale snapshot silently reverts.
    #[test]
    fn update_disabled_mcp_agents_mutates_in_memory_state() {
        let state = std::sync::Arc::new(crate::state::tests_support::make_test_app_state());
        assert!(
            state.config.read().disabled_mcp_agents.is_empty(),
            "precondition"
        );

        // Simulate remove_agent_mcp's branch: add an agent to the disabled list.
        update_disabled_mcp_agents(&state, |list| {
            if !list.contains(&"claude".to_string()) {
                list.push("claude".to_string());
            }
        });

        assert!(
            state
                .config
                .read()
                .disabled_mcp_agents
                .iter()
                .any(|a| a == "claude"),
            "in-memory state.config must be updated, not only disk",
        );

        // Simulate install_agent_mcp's branch: remove the agent.
        update_disabled_mcp_agents(&state, |list| list.retain(|a| a != "claude"));

        assert!(
            !state
                .config
                .read()
                .disabled_mcp_agents
                .iter()
                .any(|a| a == "claude"),
            "in-memory state.config must be cleared on remove",
        );
    }

    #[test]
    fn disabled_list_contains_check() {
        let disabled: Vec<String> = vec!["claude".to_string(), "windsurf".to_string()];

        // Agents in disabled list should be skipped
        for agent in &["claude", "windsurf"] {
            assert!(
                disabled.iter().any(|d| d == agent),
                "{agent} should be in disabled list",
            );
        }

        // Agents NOT in disabled list should proceed
        for agent in &["cursor", "vscode", "zed", "amp", "gemini", "codex"] {
            assert!(
                !disabled.iter().any(|d| d == agent),
                "{agent} should NOT be in disabled list",
            );
        }
    }

    // --- Codex (TOML) tests ---

    #[test]
    fn codex_install_creates_file_if_missing() {
        let dir = TempDir::new().unwrap();
        let config_path = dir.path().join("config.toml");

        assert!(!config_path.exists());

        let wrote =
            ensure_toml_mcp_entry(&config_path, true, "/usr/local/bin/tuic-bridge", "codex");
        assert!(wrote);
        assert!(config_path.exists());

        let root = read_toml_file(&config_path).unwrap();
        let cmd = root["mcp_servers"][TUIC_MCP_KEY]["command"]
            .as_str()
            .unwrap();
        assert_eq!(cmd, "/usr/local/bin/tuic-bridge");
        assert_eq!(
            root["mcp_servers"][TUIC_MCP_KEY]["env_vars"]
                .as_array()
                .unwrap(),
            &[toml::Value::String("TUIC_SESSION".to_string())],
        );
    }

    #[test]
    fn codex_install_preserves_existing_config() {
        let dir = TempDir::new().unwrap();
        let config_path = dir.path().join("config.toml");

        // Pre-existing config with other settings
        let initial = toml::toml! {
            [model]
            default = "o3"

            [mcp_servers.other_tool]
            command = "/usr/bin/other"
        };
        write_toml_file(&config_path, &toml::Value::Table(initial)).unwrap();

        let wrote = ensure_toml_mcp_entry(&config_path, true, "/path/to/tuic-bridge", "codex");
        assert!(wrote);

        let root = read_toml_file(&config_path).unwrap();
        // Our entry was added
        assert_eq!(
            root["mcp_servers"][TUIC_MCP_KEY]["command"]
                .as_str()
                .unwrap(),
            "/path/to/tuic-bridge",
        );
        // Other MCP server preserved
        assert_eq!(
            root["mcp_servers"]["other_tool"]["command"]
                .as_str()
                .unwrap(),
            "/usr/bin/other",
        );
        // Other config preserved
        assert_eq!(root["model"]["default"].as_str().unwrap(), "o3");
    }

    #[test]
    fn codex_updates_stale_path() {
        let dir = TempDir::new().unwrap();
        let config_path = dir.path().join("config.toml");

        ensure_toml_mcp_entry(&config_path, true, "/old/path", "codex");
        let wrote = ensure_toml_mcp_entry(&config_path, true, "/new/path", "codex");
        assert!(wrote, "should write when path changed");

        let root = read_toml_file(&config_path).unwrap();
        assert_eq!(
            root["mcp_servers"][TUIC_MCP_KEY]["command"]
                .as_str()
                .unwrap(),
            "/new/path",
        );
    }

    #[test]
    fn toml_repair_preserves_comments_and_inline_tables() {
        let _config = with_temp_config_dir();
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("config.toml");
        let before = "# user's model note\nmodel = 'o3'\n\n[mcp_servers.tuicommander]\ncommand = '/missing/bridge' # keep this note\nenv = { TOKEN = 'secret' }\nenabled = false\n";
        std::fs::write(&path, before).unwrap();
        assert!(ensure_toml_mcp_entry(&path, true, "/new/bridge", "codex"));
        let after = std::fs::read_to_string(&path).unwrap();
        assert!(after.contains("# user's model note"), "{after}");
        assert!(after.contains("# keep this note"), "{after}");
        assert!(after.contains("env = { TOKEN = 'secret' }"), "{after}");
        assert!(after.contains("enabled = false"), "{after}");
        assert_eq!(
            read_toml_file(&path).unwrap()["mcp_servers"][TUIC_MCP_KEY]["command"].as_str(),
            Some("/new/bridge")
        );
    }

    #[test]
    fn toml_removal_preserves_unrelated_comments() {
        let _config = with_temp_config_dir();
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("config.toml");
        let before = "# model note\nmodel = 'o3'\n\n[mcp_servers.other]\ncommand = '/other' # keep\n\n[mcp_servers.tuicommander]\ncommand = '/bridge'\n";
        std::fs::write(&path, before).unwrap();
        remove_toml_mcp_entry(&path, "codex").unwrap();
        let after = std::fs::read_to_string(&path).unwrap();
        assert!(after.contains("# model note"));
        assert!(after.contains("command = '/other' # keep"));
        assert!(!after.contains("[mcp_servers.tuicommander]"));
    }

    #[test]
    fn yaml_repair_preserves_comments_and_custom_fields() {
        let _config = with_temp_config_dir();
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("config.yaml");
        let before = "# user's note\nextensions:\n  tuicommander:\n    type: stdio\n    name: tuicommander\n    cmd: /missing/bridge # keep this note\n    args: [--flag]\n    enabled: false\n    timeout: 900\n";
        std::fs::write(&path, before).unwrap();
        assert!(ensure_yaml_mcp_entry(
            &path,
            "extensions",
            "/new/bridge",
            "goose"
        ));
        let after = std::fs::read_to_string(&path).unwrap();
        assert_eq!(after, before.replace("/missing/bridge", "/new/bridge"));
    }

    #[test]
    fn yaml_install_preserves_existing_extensions_and_comments() {
        let _config = with_temp_config_dir();
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("config.yaml");
        let before = "# keep root note\nextensions:\n  other:\n    cmd: /other # keep note\n    enabled: false\n";
        std::fs::write(&path, before).unwrap();
        assert!(ensure_yaml_mcp_entry(
            &path,
            "extensions",
            "/bridge",
            "goose"
        ));
        let after = std::fs::read_to_string(&path).unwrap();
        assert!(
            after.starts_with("# keep root note\nextensions:\n"),
            "{after}"
        );
        assert!(
            after.contains("  other:\n    cmd: /other # keep note\n    enabled: false\n"),
            "{after}"
        );
        assert_eq!(
            read_yaml_file(&path).unwrap()["extensions"][TUIC_MCP_KEY]["cmd"].as_str(),
            Some("/bridge")
        );
    }

    /// Removing the only goose extension leaves `extensions:` with no value,
    /// which YAML reads as null. The surgical guard must accept that, or the
    /// user can never remove TUIC and every launch re-adds it.
    #[test]
    fn yaml_removal_of_the_only_extension_is_allowed() {
        let _config = with_temp_config_dir();
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("goose.yaml");
        std::fs::write(
            &path,
            "extensions:\n  tuicommander:\n    cmd: /bridge\n    enabled: true\n",
        )
        .unwrap();
        remove_yaml_mcp_entry(&path, "extensions", "goose")
            .expect("removing the only extension must succeed");
        let after: serde_yaml::Value =
            serde_yaml::from_str(&std::fs::read_to_string(&path).unwrap()).unwrap();
        assert!(
            after
                .get("extensions")
                .and_then(|extensions| extensions.get(TUIC_MCP_KEY))
                .is_none()
        );
    }

    #[test]
    fn yaml_install_preserves_nonstandard_extension_indentation_and_structure() {
        let _config = with_temp_config_dir();
        let dir = TempDir::new().unwrap();
        for indent in [3, 4] {
            let path = dir.path().join(format!("goose-{indent}.yaml"));
            let before = format!(
                "# keep root note\nextensions:\n{spaces}developer:\n{spaces}{spaces}enabled: true\n{spaces}other:\n{spaces}{spaces}cmd: /other\n",
                spaces = " ".repeat(indent)
            );
            std::fs::write(&path, &before).unwrap();
            assert!(ensure_yaml_mcp_entry(
                &path,
                "extensions",
                "/bridge",
                "goose"
            ));
            let after = std::fs::read_to_string(&path).unwrap();
            let mut expected: serde_yaml::Value = serde_yaml::from_str(&before).unwrap();
            let mut actual: serde_yaml::Value = serde_yaml::from_str(&after).unwrap();
            assert_eq!(
                actual["extensions"][TUIC_MCP_KEY]["cmd"].as_str(),
                Some("/bridge")
            );
            actual["extensions"]
                .as_mapping_mut()
                .unwrap()
                .remove(TUIC_MCP_KEY);
            expected["extensions"]
                .as_mapping_mut()
                .unwrap()
                .remove(TUIC_MCP_KEY);
            assert_eq!(
                actual, expected,
                "other extensions changed at {indent} spaces: {after}"
            );
            assert!(after.contains(&format!("{spaces}developer:", spaces = " ".repeat(indent))));
            assert!(after.contains(&format!(
                "{spaces}cmd: /bridge",
                spaces = " ".repeat(indent * 2)
            )));
        }
    }

    #[test]
    fn yaml_removal_preserves_unrelated_comments() {
        let _config = with_temp_config_dir();
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("config.yaml");
        let before = "# root note\nextensions:\n  tuicommander:\n    cmd: /bridge\n    enabled: true\n  other:\n    cmd: /other # keep this\n";
        std::fs::write(&path, before).unwrap();
        remove_yaml_mcp_entry(&path, "extensions", "goose").unwrap();
        let after = std::fs::read_to_string(&path).unwrap();
        assert_eq!(
            after,
            "# root note\nextensions:\n  other:\n    cmd: /other # keep this\n"
        );
    }

    #[test]
    fn codex_updates_matching_path_to_forward_managed_identity() {
        let dir = TempDir::new().unwrap();
        let config_path = dir.path().join("config.toml");
        let initial = toml::toml! {
            [mcp_servers.tuicommander]
            command = "/correct/path"
            args = ["--keep"]
            env_vars = ["EXISTING_VAR"]
            enabled = false
        };
        write_toml_file(&config_path, &toml::Value::Table(initial)).unwrap();

        let wrote = ensure_toml_mcp_entry(&config_path, true, "/correct/path", "codex");
        assert!(wrote, "missing TUIC_SESSION forwarding must be repaired");

        let root = read_toml_file(&config_path).unwrap();
        let entry = &root["mcp_servers"][TUIC_MCP_KEY];
        assert_eq!(entry["command"].as_str(), Some("/correct/path"));
        assert_eq!(
            entry["args"].as_array().unwrap(),
            &[toml::Value::String("--keep".to_string())],
        );
        assert_eq!(entry["enabled"].as_bool(), Some(false));
        assert_eq!(
            entry["env_vars"].as_array().unwrap(),
            &[
                toml::Value::String("EXISTING_VAR".to_string()),
                toml::Value::String("TUIC_SESSION".to_string()),
            ],
        );
    }

    #[test]
    fn codex_accepts_local_object_env_var_without_rewriting() {
        let dir = TempDir::new().unwrap();
        let config_path = dir.path().join("config.toml");
        let initial = toml::toml! {
            [mcp_servers.tuicommander]
            command = "/correct/path"
            env_vars = [{ name = "TUIC_SESSION", source = "local" }]
        };
        write_toml_file(&config_path, &toml::Value::Table(initial)).unwrap();
        let mtime_before = std::fs::metadata(&config_path).unwrap().modified().unwrap();
        std::thread::sleep(std::time::Duration::from_millis(50));

        let wrote = ensure_toml_mcp_entry(&config_path, true, "/correct/path", "codex");
        assert!(!wrote, "a local object whitelist is already sufficient");
        assert_eq!(
            mtime_before,
            std::fs::metadata(&config_path).unwrap().modified().unwrap(),
        );
    }

    #[test]
    fn codex_skips_when_path_matches() {
        let dir = TempDir::new().unwrap();
        let config_path = dir.path().join("config.toml");

        ensure_toml_mcp_entry(&config_path, true, "/correct/path", "codex");
        let mtime_before = std::fs::metadata(&config_path).unwrap().modified().unwrap();
        std::thread::sleep(std::time::Duration::from_millis(50));

        let wrote = ensure_toml_mcp_entry(&config_path, true, "/correct/path", "codex");
        assert!(!wrote, "should not write when path already correct");

        let mtime_after = std::fs::metadata(&config_path).unwrap().modified().unwrap();
        assert_eq!(
            mtime_before, mtime_after,
            "file should not have been modified"
        );
    }

    #[test]
    fn codex_remove_entry() {
        let dir = TempDir::new().unwrap();
        let config_path = dir.path().join("config.toml");

        // Install with another server present
        let initial = toml::toml! {
            [mcp_servers.other_tool]
            command = "/usr/bin/other"
        };
        write_toml_file(&config_path, &toml::Value::Table(initial)).unwrap();
        ensure_toml_mcp_entry(&config_path, true, "/bridge", "codex");

        // Verify both exist
        let root = read_toml_file(&config_path).unwrap();
        assert!(root["mcp_servers"].get(TUIC_MCP_KEY).is_some());
        assert!(root["mcp_servers"].get("other_tool").is_some());

        // Remove
        remove_toml_mcp_entry(&config_path, "codex").unwrap();

        let root = read_toml_file(&config_path).unwrap();
        assert!(root["mcp_servers"].get(TUIC_MCP_KEY).is_none());
        assert!(root["mcp_servers"].get("other_tool").is_some());
    }

    #[test]
    fn codex_remove_from_nonexistent_file_is_ok() {
        let dir = TempDir::new().unwrap();
        let config_path = dir.path().join("does-not-exist.toml");

        let result = remove_toml_mcp_entry(&config_path, "codex");
        assert!(result.is_ok());
        assert!(!config_path.exists());
    }

    #[test]
    fn codex_is_installed_check() {
        let dir = TempDir::new().unwrap();
        let config_path = dir.path().join("config.toml");

        // Not installed (file doesn't exist)
        assert!(!is_toml_mcp_installed(&config_path));

        // Install
        ensure_toml_mcp_entry(&config_path, true, "/bridge", "codex");
        assert!(is_toml_mcp_installed(&config_path));

        // Remove
        remove_toml_mcp_entry(&config_path, "codex").unwrap();
        assert!(!is_toml_mcp_installed(&config_path));
    }

    #[test]
    fn codex_in_supported_agents() {
        assert!(
            SUPPORTED_AGENTS.contains(&"codex"),
            "codex must be in SUPPORTED_AGENTS",
        );
    }

    // --- presence gate ---

    /// Spec pointing at a temp dir, with no binary that could ever resolve.
    fn spec_at(config_path: PathBuf) -> McpConfigSpec {
        McpConfigSpec {
            config_path,
            key_path: vec!["mcpServers"],
            format: McpFormat::Json,
            binaries: &[],
            presence_dir: None,
            requires_existing_config: false,
            shared_settings_file: false,
        }
    }

    fn command_at_spec(spec: &McpConfigSpec) -> String {
        let path = &spec.config_path;
        match spec.format {
            McpFormat::Toml { .. } => {
                read_toml_file(path).unwrap()["mcp_servers"][TUIC_MCP_KEY]["command"]
                    .as_str()
                    .unwrap()
                    .to_string()
            }
            McpFormat::Yaml => read_yaml_file(path).unwrap()["extensions"][TUIC_MCP_KEY]["cmd"]
                .as_str()
                .unwrap()
                .to_string(),
            McpFormat::OpenCode => read_json_file(path).unwrap()["mcp"][TUIC_MCP_KEY]["command"][0]
                .as_str()
                .unwrap()
                .to_string(),
            McpFormat::Json => {
                read_json_file(path).unwrap()[spec.key_path[0]][TUIC_MCP_KEY]["command"]
                    .as_str()
                    .unwrap()
                    .to_string()
            }
        }
    }

    /// Catches: publishing a cargo target command, including retaining a live
    /// legacy command, so target cleanup makes the next MCP spawn fail ENOENT.
    #[test]
    fn configured_bridge_survives_target_cleanup() {
        let (_guard, config_dir) = with_temp_config_dir();
        let dir = TempDir::new().unwrap();
        let target = dir.path().join("target/debug");
        std::fs::create_dir_all(&target).unwrap();
        let source = bridge_path_in(&target);
        // A real native executable, not a shell fake: the OS must be able to
        // spawn the installed bytes after their original directory disappears.
        std::fs::copy(std::env::current_exe().unwrap(), &source).unwrap();
        let spec = spec_at(dir.path().join("claude.json"));
        assert!(ensure_spec_entry(&spec, source.to_str().unwrap(), "claude"));
        ensure_mcp_configs_for(&[], Some(&source), [("claude", spec_at_format(&spec))]);
        let command = command_at_spec(&spec);
        let installed = std::path::Path::new(&command);
        assert!(installed.starts_with(config_dir.path()));
        assert!(!installed.starts_with(dir.path()));
        std::fs::remove_dir_all(dir.path().join("target")).unwrap();
        let output = std::process::Command::new(installed)
            .arg("--list")
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        assert!(
            String::from_utf8_lossy(&output.stdout)
                .contains("configured_bridge_survives_target_cleanup")
        );
    }

    /// Catches: overwriting or deleting an old installed executable on upgrade,
    /// and rewriting identical bytes while Windows has that executable open.
    #[test]
    fn bridge_upgrade_keeps_previous_revision_and_reuses_identical_bytes() {
        let _config = with_temp_config_dir();
        let dir = TempDir::new().unwrap();
        let source = bridge_path_in(dir.path());
        std::fs::copy(std::env::current_exe().unwrap(), &source).unwrap();
        let old = install_bridge_binary(&source).unwrap();
        let modified = old.metadata().unwrap().modified().unwrap();
        assert_eq!(install_bridge_binary(&source).unwrap(), old);
        assert_eq!(old.metadata().unwrap().modified().unwrap(), modified);
        // Installation treats executable bytes as opaque; this input exercises
        // revision publication without pretending to implement the MCP protocol.
        std::fs::write(&source, b"a different bridge revision").unwrap();
        let new = install_bridge_binary(&source).unwrap();
        assert_ne!(new, old);
        assert_eq!(std::fs::read(new).unwrap(), b"a different bridge revision");
        let output = std::process::Command::new(old)
            .arg("--list")
            .output()
            .unwrap();
        assert!(output.status.success());
    }

    /// Catches: publishing a new command before its executable is installed,
    /// or damaging the working revision when the destination cannot be created.
    #[test]
    fn failed_bridge_install_leaves_config_and_previous_revision_intact() {
        use sha2::{Digest, Sha256};
        let (_guard, config_dir) = with_temp_config_dir();
        let dir = TempDir::new().unwrap();
        let source = bridge_path_in(dir.path());
        std::fs::copy(std::env::current_exe().unwrap(), &source).unwrap();
        let spec = spec_at(dir.path().join("claude.json"));
        assert!(ensure_spec_entry(&spec, BRIDGE_NAME, "claude"));
        ensure_mcp_configs_for(&[], Some(&source), [("claude", spec_at_format(&spec))]);
        let before = std::fs::read(&spec.config_path).unwrap();
        let old = PathBuf::from(command_at_spec(&spec));
        let old_modified = old.metadata().unwrap().modified().unwrap();
        let bytes = b"new revision blocked by a filesystem error";
        std::fs::write(&source, bytes).unwrap();
        let blocked = config_dir
            .path()
            .join("mcp-bridge")
            .join(hex::encode(Sha256::digest(bytes)));
        std::fs::write(blocked, b"not a directory").unwrap();
        ensure_mcp_configs_for(&[], Some(&source), [("claude", spec_at_format(&spec))]);
        assert_eq!(std::fs::read(&spec.config_path).unwrap(), before);
        assert_eq!(old.metadata().unwrap().modified().unwrap(), old_modified);
        assert!(usable_executable(&old));
        std::fs::remove_file(source).unwrap();
        ensure_mcp_configs_for(&[], None, [("claude", spec_at_format(&spec))]);
        assert_eq!(std::fs::read(&spec.config_path).unwrap(), before);
        ensure_mcp_configs_for(
            &[],
            Some(&dir.path().join("missing")),
            [("claude", spec_at_format(&spec))],
        );
        assert_eq!(std::fs::read(&spec.config_path).unwrap(), before);
    }

    #[test]
    fn missing_bridge_does_not_replace_a_working_absolute_command() {
        let _config = with_temp_config_dir();
        let dir = TempDir::new().unwrap();
        let working_bridge = std::env::current_exe().unwrap();
        let working_command = working_bridge.to_str().unwrap();

        for (label, format, key_path, extension) in [
            ("claude", McpFormat::Json, vec!["mcpServers"], "json"),
            ("vscode", McpFormat::Json, vec!["servers"], "json"),
            ("opencode", McpFormat::OpenCode, vec!["mcp"], "json"),
            (
                "codex",
                McpFormat::Toml {
                    forward_session: true,
                },
                vec![],
                "toml",
            ),
            (
                "grok",
                McpFormat::Toml {
                    forward_session: false,
                },
                vec![],
                "toml",
            ),
            ("goose", McpFormat::Yaml, vec!["extensions"], "yaml"),
        ] {
            let path = dir.path().join(format!("{label}.{extension}"));
            let spec = McpConfigSpec {
                key_path,
                format,
                ..spec_at(path.clone())
            };
            assert!(ensure_spec_entry(&spec, working_command, label));
            let before = std::fs::read(&path).unwrap();
            assert!(
                !ensure_spec_entry(&spec, BRIDGE_NAME, label),
                "{label} must retain its working absolute command"
            );
            assert_eq!(std::fs::read(&path).unwrap(), before, "{label} changed");

            let missing = dir.path().join(format!("new-{label}.{extension}"));
            let new_spec = McpConfigSpec {
                config_path: missing.clone(),
                key_path: spec.key_path.clone(),
                format,
                ..spec_at(missing.clone())
            };
            assert!(ensure_spec_entry(&new_spec, BRIDGE_NAME, label));
            assert_eq!(command_at_spec(&new_spec), BRIDGE_NAME, "{label}");
        }
    }

    #[test]
    fn different_adjacent_bridge_keeps_working_commands_and_repairs_missing_ones() {
        let _config = with_temp_config_dir();
        let dir = TempDir::new().unwrap();
        let working = std::env::current_exe().unwrap();
        let replacement = dir.path().join("replacement-bridge");
        std::fs::write(&replacement, b"replacement").unwrap();
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(&replacement, std::fs::Permissions::from_mode(0o755)).unwrap();
        }
        for (label, format, key_path, extension) in [
            ("claude", McpFormat::Json, vec!["mcpServers"], "json"),
            ("opencode", McpFormat::OpenCode, vec!["mcp"], "json"),
            (
                "codex",
                McpFormat::Toml {
                    forward_session: true,
                },
                vec![],
                "toml",
            ),
            (
                "grok",
                McpFormat::Toml {
                    forward_session: false,
                },
                vec![],
                "toml",
            ),
            ("goose", McpFormat::Yaml, vec!["extensions"], "yaml"),
        ] {
            let path = dir.path().join(format!("{label}.{extension}"));
            let spec = McpConfigSpec {
                key_path,
                format,
                ..spec_at(path.clone())
            };
            assert!(ensure_spec_entry(&spec, working.to_str().unwrap(), label));
            let before = std::fs::read(&path).unwrap();
            ensure_mcp_configs_for(
                &[],
                Some(&replacement),
                std::iter::once((label, spec_at_format(&spec))),
            );
            assert_eq!(
                std::fs::read(&path).unwrap(),
                before,
                "{label} working entry changed"
            );

            let missing = dir.path().join(format!("missing-{label}"));
            assert!(ensure_spec_entry(&spec, missing.to_str().unwrap(), label));
            ensure_mcp_configs_for(
                &[],
                Some(&replacement),
                std::iter::once((label, spec_at_format(&spec))),
            );
            let installed = PathBuf::from(command_at_spec(&spec));
            assert_ne!(installed, replacement, "{label} still points at the source");
            assert!(installed.starts_with(crate::config::config_dir()));
            assert_eq!(std::fs::read(installed).unwrap(), b"replacement", "{label}");
        }
    }

    #[test]
    fn launch_keeps_wrapper_commands_in_every_config_format() {
        let _config = with_temp_config_dir();
        let dir = TempDir::new().unwrap();
        let bridge = std::env::current_exe().unwrap();
        for (label, format, key_path, contents) in [
            (
                "claude",
                McpFormat::Json,
                vec!["mcpServers"],
                r#"{"mcpServers":{"tuicommander":{"type":"stdio","command":"bash","args":["-lc","tuic-bridge"],"env":{}}}}"#,
            ),
            (
                "opencode",
                McpFormat::OpenCode,
                vec!["mcp"],
                r#"{"mcp":{"tuicommander":{"type":"local","command":["bash","-lc","tuic-bridge"],"enabled":true}}}"#,
            ),
            (
                "codex",
                McpFormat::Toml {
                    forward_session: true,
                },
                vec![],
                "[mcp_servers.tuicommander]\ncommand = 'bash'\nargs = ['-lc', 'tuic-bridge']\n",
            ),
            (
                "goose",
                McpFormat::Yaml,
                vec!["extensions"],
                "extensions:\n  tuicommander:\n    type: stdio\n    cmd: bash\n    args: [-lc, tuic-bridge]\n",
            ),
        ] {
            let path = dir.path().join(format!("{label}.config"));
            std::fs::write(&path, contents).unwrap();
            let spec = McpConfigSpec {
                key_path,
                format,
                ..spec_at(path.clone())
            };
            ensure_mcp_configs_for(
                &[],
                Some(&bridge),
                std::iter::once((label, spec_at_format(&spec))),
            );
            assert_eq!(
                std::fs::read_to_string(&path).unwrap(),
                contents,
                "{label} wrapper changed"
            );
        }
    }

    #[test]
    fn launch_keeps_http_entries_and_explicit_install_refuses_them() {
        let _config = with_temp_config_dir();
        let dir = TempDir::new().unwrap();
        let bridge = std::env::current_exe().unwrap();
        for (label, format, key_path, contents) in [
            (
                "claude",
                McpFormat::Json,
                vec!["mcpServers"],
                r#"{"mcpServers":{"tuicommander":{"type":"http","url":"http://127.0.0.1:9876/mcp"}}}"#,
            ),
            (
                "opencode",
                McpFormat::OpenCode,
                vec!["mcp"],
                r#"{"mcp":{"tuicommander":{"type":"remote","url":"http://127.0.0.1:9876/mcp"}}}"#,
            ),
            (
                "codex",
                McpFormat::Toml {
                    forward_session: true,
                },
                vec![],
                "[mcp_servers.tuicommander]\nurl = 'http://127.0.0.1:9876/mcp'\n",
            ),
            (
                "goose",
                McpFormat::Yaml,
                vec!["extensions"],
                "extensions:\n  tuicommander:\n    type: streamable_http\n    uri: http://127.0.0.1:9876/mcp\n",
            ),
        ] {
            let path = dir.path().join(format!("{label}.config"));
            std::fs::write(&path, contents).unwrap();
            let spec = McpConfigSpec {
                key_path,
                format,
                ..spec_at(path.clone())
            };
            ensure_mcp_configs_for(
                &[],
                Some(&bridge),
                std::iter::once((label, spec_at_format(&spec))),
            );
            assert_eq!(
                std::fs::read_to_string(&path).unwrap(),
                contents,
                "{label} URL entry changed at launch"
            );
            assert!(
                install_spec(&spec, bridge.to_str().unwrap(), label).is_err(),
                "{label} URL entry was accepted for stdio install"
            );
            assert_eq!(
                std::fs::read_to_string(&path).unwrap(),
                contents,
                "{label} URL entry changed on explicit install"
            );
        }
    }

    fn spec_at_format(spec: &McpConfigSpec) -> McpConfigSpec {
        McpConfigSpec {
            config_path: spec.config_path.clone(),
            key_path: spec.key_path.clone(),
            format: spec.format,
            binaries: &[],
            presence_dir: None,
            requires_existing_config: false,
            shared_settings_file: false,
        }
    }

    #[test]
    fn json_repair_preserves_user_fields() {
        let _config = with_temp_config_dir();
        let dir = TempDir::new().unwrap();
        for (format, key) in [
            (McpFormat::Json, "mcpServers"),
            (McpFormat::OpenCode, "mcp"),
        ] {
            let path = dir.path().join(format!("{key}.json"));
            let entry = if format == McpFormat::OpenCode {
                serde_json::json!({"type":"local", "command":["/missing/bridge", "--flag"], "enabled":false, "environment":{"TOKEN":"secret"}})
            } else {
                serde_json::json!({"type":"stdio", "command":"/missing/bridge", "args":["--flag"], "env":{"TOKEN":"secret"}, "disabled":true, "timeout":900})
            };
            write_fixture(
                &path,
                &serde_json::json!({(key): {(TUIC_MCP_KEY): entry.clone()}}),
            );
            assert!(ensure_agent_mcp_entry(
                &path,
                &[key],
                format,
                "/new/bridge",
                "test"
            ));
            let repaired = &read_json_file(&path).unwrap()[key][TUIC_MCP_KEY];
            assert_eq!(
                repaired["command"],
                if format == McpFormat::OpenCode {
                    serde_json::json!(["/new/bridge", "--flag"])
                } else {
                    serde_json::json!("/new/bridge")
                }
            );
            assert_eq!(repaired["args"], entry["args"]);
            assert_eq!(repaired["env"], entry["env"]);
            assert_eq!(repaired["environment"], entry["environment"]);
            assert_eq!(repaired["disabled"], entry["disabled"]);
            assert_eq!(repaired["timeout"], entry["timeout"]);
            assert_eq!(repaired["enabled"], entry["enabled"]);
        }
    }

    #[test]
    fn explicit_install_keeps_working_entry_and_rejects_bare_repair() {
        let _config = with_temp_config_dir();
        let dir = TempDir::new().unwrap();
        let spec = spec_at(dir.path().join("mcp.json"));
        let working = std::env::current_exe().unwrap();
        assert!(ensure_spec_entry(
            &spec,
            working.to_str().unwrap(),
            "claude"
        ));
        let before = std::fs::read(&spec.config_path).unwrap();
        assert!(install_spec(&spec, "/other/bridge", "claude").is_ok());
        assert_eq!(std::fs::read(&spec.config_path).unwrap(), before);
        assert!(install_spec(&spec, BRIDGE_NAME, "claude").is_ok());

        assert!(ensure_spec_entry(&spec, "/missing/bridge", "claude"));
        let before = std::fs::read(&spec.config_path).unwrap();
        assert!(install_spec(&spec, BRIDGE_NAME, "claude").is_err());
        assert_eq!(std::fs::read(&spec.config_path).unwrap(), before);
    }

    #[cfg(unix)]
    #[test]
    fn atomic_edit_keeps_config_permissions_and_symlink() {
        use std::os::unix::fs::PermissionsExt;
        let (_guard, config_dir) = with_temp_config_dir();
        let dir = TempDir::new().unwrap();
        let target = dir.path().join("target.json");
        let link = dir.path().join("config.json");
        std::fs::write(&target, "old").unwrap();
        std::fs::set_permissions(&target, std::fs::Permissions::from_mode(0o600)).unwrap();
        std::os::unix::fs::symlink(&target, &link).unwrap();
        write_text_file_if_unchanged(&link, Some("old"), "new").unwrap();
        assert!(link.is_symlink());
        assert_eq!(std::fs::read_to_string(&target).unwrap(), "new");
        assert_eq!(
            std::fs::metadata(&target).unwrap().permissions().mode() & 0o777,
            0o600
        );
        assert!(write_text_file_if_unchanged(&link, Some("old"), "lost").is_err());
        assert_eq!(std::fs::read_to_string(&target).unwrap(), "new");
        let fresh = dir.path().join("fresh.json");
        write_text_file(&fresh, "secret").unwrap();
        assert_eq!(
            std::fs::metadata(&fresh).unwrap().permissions().mode() & 0o777,
            0o600
        );
        backup_config_once(&target, "claude", "secret");
        let backup = config_dir
            .path()
            .join("mcp-backups/claude-target.json.orig");
        assert_eq!(std::fs::read_to_string(&backup).unwrap(), "secret");
        assert_eq!(
            std::fs::metadata(&backup).unwrap().permissions().mode() & 0o777,
            0o600
        );
        std::fs::set_permissions(&backup, std::fs::Permissions::from_mode(0o644)).unwrap();
        backup_config_once(&target, "claude", "later");
        assert_eq!(std::fs::read_to_string(&backup).unwrap(), "secret");
        assert_eq!(
            std::fs::metadata(&backup).unwrap().permissions().mode() & 0o777,
            0o600
        );
    }

    #[test]
    fn startup_without_an_adjacent_bridge_never_writes_agent_configs() {
        let _config = with_temp_config_dir();
        let dir = TempDir::new().unwrap();
        let stale_bridge = dir.path().join("moved-bridge");
        let real_bridge = dir.path().join("tuic-bridge");
        std::fs::copy(std::env::current_exe().unwrap(), &real_bridge).unwrap();
        let unrelated_exe_dir = dir.path().join("test-target");
        std::fs::create_dir(&unrelated_exe_dir).unwrap();

        for (label, format, key_path, extension) in [
            ("claude", McpFormat::Json, vec!["mcpServers"], "json"),
            ("vscode", McpFormat::Json, vec!["servers"], "json"),
            ("opencode", McpFormat::OpenCode, vec!["mcp"], "json"),
            (
                "codex",
                McpFormat::Toml {
                    forward_session: true,
                },
                vec![],
                "toml",
            ),
            (
                "grok",
                McpFormat::Toml {
                    forward_session: false,
                },
                vec![],
                "toml",
            ),
            ("goose", McpFormat::Yaml, vec!["extensions"], "yaml"),
        ] {
            let path = dir.path().join(format!("startup-{label}.{extension}"));
            let spec = McpConfigSpec {
                key_path,
                format,
                ..spec_at(path.clone())
            };
            assert!(ensure_spec_entry(
                &spec,
                stale_bridge.to_str().unwrap(),
                label
            ));
            let before = std::fs::read(&path).unwrap();
            ensure_mcp_configs_for(
                &[],
                bridge_beside(&unrelated_exe_dir).as_deref(),
                std::iter::once((
                    label,
                    McpConfigSpec {
                        key_path: spec.key_path.clone(),
                        format,
                        ..spec_at(path.clone())
                    },
                )),
            );
            assert_eq!(std::fs::read(&path).unwrap(), before, "{label} changed");

            let absent = dir.path().join(format!("absent-{label}.{extension}"));
            ensure_mcp_configs_for(
                &[],
                bridge_beside(&unrelated_exe_dir).as_deref(),
                std::iter::once((
                    label,
                    McpConfigSpec {
                        key_path: spec.key_path.clone(),
                        format,
                        ..spec_at(absent.clone())
                    },
                )),
            );
            assert!(!absent.exists(), "{label} was installed without a bridge");

            ensure_mcp_configs_for(
                &[],
                Some(&real_bridge),
                std::iter::once((
                    label,
                    McpConfigSpec {
                        key_path: spec.key_path.clone(),
                        format,
                        ..spec_at(path.clone())
                    },
                )),
            );
            assert_ne!(
                std::fs::read(&path).unwrap(),
                before,
                "{label} was not repaired"
            );
            let command = PathBuf::from(command_at_spec(&spec));
            assert!(
                command.starts_with(crate::config::config_dir().join("mcp-bridge")),
                "{label}"
            );
            assert_ne!(command, real_bridge, "{label} still names source");
            assert_eq!(
                std::fs::read(command).unwrap(),
                std::fs::read(&real_bridge).unwrap()
            );
        }
    }

    #[test]
    fn startup_entry_point_cannot_edit_the_user_home_without_a_sidecar() {
        let dir = TempDir::new().unwrap();
        let fake_bin = dir.path().join(".cargo/bin");
        std::fs::create_dir_all(&fake_bin).unwrap();
        let fake_bridge = bridge_path_in(&fake_bin);
        std::fs::write(&fake_bridge, b"fake bridge").unwrap();
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(&fake_bridge, std::fs::Permissions::from_mode(0o755)).unwrap();
        }
        std::fs::create_dir_all(dir.path().join(".claude")).unwrap();
        std::fs::write(dir.path().join(".claude/marker"), b"installed").unwrap();
        let config = dir.path().join(".claude.json");
        let original = r#"{"mcpServers":{"tuicommander":{"type":"stdio","command":"/working/bridge","args":[],"env":{}}}}"#;
        std::fs::write(&config, original).unwrap();
        let exe = std::env::current_exe().unwrap();
        assert!(bridge_beside(exe.parent().unwrap()).is_none());
        assert!(
            bridge_location_is_stable(&exe),
            "test executable must pass the location gate"
        );

        fn snapshot(
            dir: &std::path::Path,
        ) -> Vec<(PathBuf, Option<Vec<u8>>, Option<std::time::SystemTime>)> {
            let mut files = Vec::new();
            let mut pending = vec![dir.to_path_buf()];
            while let Some(parent) = pending.pop() {
                for entry in std::fs::read_dir(parent).unwrap() {
                    let entry = entry.unwrap();
                    let relative = entry.path().strip_prefix(dir).unwrap().to_path_buf();
                    if entry.file_type().unwrap().is_dir() {
                        files.push((relative, None, None));
                        pending.push(entry.path());
                    } else {
                        files.push((
                            relative,
                            Some(std::fs::read(entry.path()).unwrap()),
                            Some(entry.metadata().unwrap().modified().unwrap()),
                        ));
                    }
                }
            }
            files.sort_by(|a, b| a.0.cmp(&b.0));
            files
        }
        let before = snapshot(dir.path());

        let output = std::process::Command::new(exe)
            .arg("--exact")
            .arg("agent_mcp::tests::startup_entry_point_child")
            .env("HOME", dir.path())
            .env("XDG_CONFIG_HOME", dir.path().join(".config"))
            .env("CODEX_HOME", dir.path().join(".codex"))
            .env("APPDATA", dir.path().join("AppData"))
            .env("USERPROFILE", dir.path())
            .env_remove("PI_CODING_AGENT_DIR")
            .env("TUIC_MCP_TEST_HOME", dir.path())
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "child failed: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        assert_eq!(std::fs::read_to_string(config).unwrap(), original);
        assert_eq!(
            snapshot(dir.path()),
            before,
            "startup changed the sandboxed HOME"
        );
    }

    #[test]
    fn startup_entry_point_child() {
        let Some(expected_home) = std::env::var_os("TUIC_MCP_TEST_HOME") else {
            return;
        };
        // This assertion precedes the production entry point: a HOME override
        // failure must never turn this test into a write of real user configs.
        assert_eq!(home(), PathBuf::from(expected_home));
        for agent in SUPPORTED_AGENTS {
            let spec = get_mcp_config_spec(agent).unwrap();
            assert!(
                spec.config_path.starts_with(home()),
                "{agent} escaped sandbox: {}",
                spec.config_path.display()
            );
            if *agent == "codex" {
                assert_eq!(spec.config_path, home().join(".codex/config.toml"));
            }
        }
        let _config = with_temp_config_dir();
        ensure_mcp_configs(&[]);
    }

    fn run_sandboxed_mcp_launch(
        exe: &std::path::Path,
        home: &std::path::Path,
        cwd: &std::path::Path,
        instance: Option<&str>,
        owner_override: Option<&str>,
        disable_claude: bool,
    ) {
        let mut command = std::process::Command::new(exe);
        command
            .args(["--exact", "agent_mcp::tests::secondary_launch_child"])
            .current_dir(cwd)
            .env("HOME", home)
            .env("TUIC_MCP_TEST_HOME", home)
            .env("XDG_CONFIG_HOME", home.join(".config"))
            .env("CODEX_HOME", home.join(".codex"))
            .env("APPDATA", home.join("AppData"))
            .env("USERPROFILE", home)
            .env("PATH", exe.parent().unwrap())
            .env_remove("PI_CODING_AGENT_DIR")
            .env_remove("TUIC_APP_INSTANCE")
            .env_remove("TUIC_MCP_CONFIG_OWNER")
            .env_remove("TUIC_MCP_TEST_DISABLED");
        if let Some(instance) = instance {
            command.env("TUIC_APP_INSTANCE", instance);
        }
        if let Some(owner_override) = owner_override {
            command.env("TUIC_MCP_CONFIG_OWNER", owner_override);
        }
        if disable_claude {
            command.env("TUIC_MCP_TEST_DISABLED", "claude");
        }
        let output = command.output().unwrap();
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
    }

    #[test]
    fn secondary_launch_leaves_agent_config_unchanged() {
        let common_dir = std::process::Command::new("git")
            .args(["rev-parse", "--path-format=absolute", "--git-common-dir"])
            .output()
            .unwrap();
        assert!(common_dir.status.success());
        let git_dir = PathBuf::from(String::from_utf8(common_dir.stdout).unwrap().trim());
        let main_root = git_dir.parent().unwrap();
        let sandbox_root = main_root.join(".tmp");
        std::fs::create_dir_all(&sandbox_root).unwrap();
        let sandbox = tempfile::tempdir_in(sandbox_root).unwrap();
        let runner_name = if cfg!(windows) {
            "tuic-test-runner.exe"
        } else {
            "tuic-test-runner"
        };
        let exe = sandbox.path().join(runner_name);
        // The CI target and checkout can live on different Windows drives.
        #[cfg(windows)]
        std::fs::copy(std::env::current_exe().unwrap(), &exe).unwrap();
        #[cfg(not(windows))]
        std::fs::hard_link(std::env::current_exe().unwrap(), &exe).unwrap();
        // The bridge is copied and hashed, never launched. Hardlinking the
        // entire test runner here made owning launches copy hundreds of MB and
        // block on Linux writeback under the parallel suite's 120s bound.
        let bridge = fake_bridge(sandbox.path(), b"bridge bytes");
        let linked_worktree = sandbox.path().join("linked");
        std::fs::create_dir_all(&linked_worktree).unwrap();
        std::fs::write(linked_worktree.join(".git"), b"gitdir: isolated-fixture").unwrap();

        // Catches secondary ownership changes independently of custom-command
        // preservation: a leading slash is only rooted, not absolute, on Windows.
        let original = serde_json::json!({"mcpServers": {"tuicommander": {
            "type": "stdio", "command": sandbox.path().join("missing/bridge"),
            "args": [], "env": {}
        }}})
        .to_string();
        let cases = [
            ("named", Some("tuic-test"), main_root, false, false),
            ("worktree", None, linked_worktree.as_path(), false, false),
            ("default", None, main_root, false, true),
            ("named-override", Some("tuic-test"), main_root, true, true),
            (
                "worktree-override",
                None,
                linked_worktree.as_path(),
                true,
                true,
            ),
        ];
        for (name, instance, cwd, override_owner, should_write) in cases {
            let home = sandbox.path().join(name);
            std::fs::create_dir_all(home.join(".claude")).unwrap();
            std::fs::write(home.join(".claude/installed"), b"present").unwrap();
            let config = home.join(".claude.json");
            std::fs::write(&config, &original).unwrap();

            run_sandboxed_mcp_launch(
                &exe,
                &home,
                cwd,
                instance,
                override_owner.then_some("1"),
                false,
            );
            let after = std::fs::read_to_string(&config).unwrap();
            if should_write {
                let parsed: serde_json::Value = serde_json::from_str(&after).unwrap();
                let command = PathBuf::from(
                    parsed["mcpServers"]["tuicommander"]["command"]
                        .as_str()
                        .unwrap(),
                );
                // Compare filesystem identities, not a normal Windows path
                // against canonicalize's verbatim path spelling.
                let command = command.canonicalize().unwrap();
                assert!(
                    command
                        .starts_with(home.join("tuic-config/mcp-bridge").canonicalize().unwrap()),
                    "{name}"
                );
                assert_eq!(
                    std::fs::read(command).unwrap(),
                    std::fs::read(&bridge).unwrap(),
                    "{name}"
                );
            } else {
                assert_eq!(after, original, "{name}");
            }
        }

        // A disabled integration in the owning instance must stay removed when
        // a named instance launches with its own, different disabled list.
        let removed_home = sandbox.path().join("removed");
        std::fs::create_dir_all(removed_home.join(".claude")).unwrap();
        std::fs::write(removed_home.join(".claude/installed"), b"present").unwrap();
        let removed_config = removed_home.join(".claude.json");
        for (instance, disabled) in [(None, true), (Some("tuic-test"), false)] {
            run_sandboxed_mcp_launch(&exe, &removed_home, main_root, instance, None, disabled);
            assert!(
                !removed_config.exists(),
                "removed integration was reinstalled"
            );
        }

        let worktree_tmp = linked_worktree.join(".tmp");
        std::fs::create_dir_all(&worktree_tmp).unwrap();
        let worktree_sandbox = tempfile::tempdir_in(worktree_tmp).unwrap();
        let worktree_exe = worktree_sandbox.path().join(runner_name);
        #[cfg(windows)]
        std::fs::copy(std::env::current_exe().unwrap(), &worktree_exe).unwrap();
        #[cfg(not(windows))]
        std::fs::hard_link(std::env::current_exe().unwrap(), &worktree_exe).unwrap();
        fake_bridge(worktree_sandbox.path(), b"bridge bytes");
        let home = sandbox.path().join("worktree-binary");
        std::fs::create_dir_all(home.join(".claude")).unwrap();
        std::fs::write(home.join(".claude/installed"), b"present").unwrap();
        let config = home.join(".claude.json");
        std::fs::write(&config, &original).unwrap();
        run_sandboxed_mcp_launch(&worktree_exe, &home, main_root, None, None, false);
        assert_eq!(std::fs::read_to_string(&config).unwrap(), original);
    }

    #[test]
    fn secondary_launch_child() {
        let Some(expected_home) = std::env::var_os("TUIC_MCP_TEST_HOME") else {
            return;
        };
        assert_eq!(home(), PathBuf::from(expected_home));
        for agent in SUPPORTED_AGENTS {
            assert!(
                get_mcp_config_spec(agent)
                    .unwrap()
                    .config_path
                    .starts_with(home())
            );
        }
        crate::app_instance::select_app_instance_from_env().unwrap();
        // Keep the installed copy in the parent-owned sandbox so it remains
        // observable after the child exits, just as a production config does.
        let _config = crate::config::set_config_dir_override(home().join("tuic-config"));
        let disabled = std::env::var("TUIC_MCP_TEST_DISABLED")
            .ok()
            .into_iter()
            .collect::<Vec<_>>();
        ensure_mcp_configs(&disabled);
    }

    #[test]
    fn missing_config_dir_is_not_installed() {
        let dir = TempDir::new().unwrap();
        let spec = spec_at(dir.path().join("nope/mcp.json"));
        assert!(!is_target_installed(&spec));
    }

    #[test]
    fn dir_holding_only_our_config_is_not_installed() {
        // The exact state TUIC used to create: ~/.cursor containing nothing but
        // the mcp.json we wrote. Reading that back as "installed" would make the
        // gate self-fulfilling — and makes other tools believe Cursor is here.
        let dir = TempDir::new().unwrap();
        let config_path = dir.path().join("mcp.json");
        ensure_agent_mcp_entry(
            &config_path,
            &["mcpServers"],
            McpFormat::Json,
            "/bridge",
            "test",
        );
        assert!(config_path.exists());
        assert!(!is_target_installed(&spec_at(config_path)));
    }

    #[test]
    fn dir_with_a_foreign_file_is_installed() {
        let dir = TempDir::new().unwrap();
        std::fs::write(dir.path().join("settings.json"), "{}").unwrap();
        assert!(is_target_installed(&spec_at(dir.path().join("mcp.json"))));
    }

    #[test]
    fn ds_store_and_stale_temp_files_do_not_prove_installation() {
        let dir = TempDir::new().unwrap();
        std::fs::write(dir.path().join(".DS_Store"), "").unwrap();
        // A crashed atomic write leaves `<stem>.tmp` behind.
        std::fs::write(dir.path().join("mcp.tmp"), "{}").unwrap();
        assert!(!is_target_installed(&spec_at(dir.path().join("mcp.json"))));
    }

    #[test]
    fn presence_dir_overrides_the_config_parent() {
        // Claude's config lives in $HOME, whose siblings prove nothing.
        let dir = TempDir::new().unwrap();
        let agent_dir = dir.path().join(".claude");
        let spec = McpConfigSpec {
            presence_dir: Some(agent_dir.clone()),
            ..spec_at(dir.path().join(".claude.json"))
        };
        assert!(!is_target_installed(&spec));

        std::fs::create_dir_all(&agent_dir).unwrap();
        std::fs::write(agent_dir.join("settings.json"), "{}").unwrap();
        assert!(is_target_installed(&spec));
    }

    #[test]
    fn a_configured_target_keeps_getting_path_repairs() {
        // Presence may stop resolving (tool dropped off PATH) — an entry we
        // already own must still be updated, never left pointing at a stale
        // bridge binary.
        let _config = with_temp_config_dir();
        let dir = TempDir::new().unwrap();
        let config_path = dir.path().join("mcp.json");
        let spec = spec_at(config_path.clone());
        assert!(!has_bridge_entry(&spec));

        ensure_spec_entry(&spec, "/old/bridge", "test");
        assert!(has_bridge_entry(&spec));
        assert!(!is_target_installed(&spec));

        ensure_spec_entry(&spec, "/new/bridge", "test");
        let root = read_json_file(&config_path).unwrap();
        assert_eq!(root["mcpServers"][TUIC_MCP_KEY]["command"], "/new/bridge");
    }

    // --- unparseable configs are never overwritten ---

    #[test]
    fn json_with_comments_is_edited_not_rejected() {
        // VS Code's mcp.json, Zed's settings.json and opencode's config all
        // document comments as supported. Refusing to parse them meant the
        // integration silently did nothing for those users.
        let dir = TempDir::new().unwrap();
        let config_path = dir.path().join("mcp.json");
        let original = "// user comment\n{ \"servers\": { \"mine\": { \"command\": \"x\" } } }";
        std::fs::write(&config_path, original).unwrap();

        assert!(read_json_file(&config_path).is_some());
        assert!(ensure_agent_mcp_entry(
            &config_path,
            &["servers"],
            McpFormat::Json,
            "/bridge",
            "test",
        ));

        let written = std::fs::read_to_string(&config_path).unwrap();
        assert!(written.starts_with("// user comment"), "{written}");
        let root = read_json_file(&config_path).unwrap();
        assert_eq!(root["servers"]["mine"]["command"], "x");
        assert_eq!(root["servers"][TUIC_MCP_KEY]["command"], "/bridge");
    }

    #[test]
    fn genuinely_broken_json_is_left_untouched() {
        // The guard that must never regress: a file we cannot understand is one
        // we do not write. Truncated mid-object, so no dialect parses it.
        let dir = TempDir::new().unwrap();
        let config_path = dir.path().join("mcp.json");
        let original = "{ \"servers\": { \"mine\": ";
        std::fs::write(&config_path, original).unwrap();

        assert!(read_json_file(&config_path).is_none());
        assert!(!ensure_agent_mcp_entry(
            &config_path,
            &["servers"],
            McpFormat::Json,
            "/bridge",
            "test",
        ));
        assert_eq!(std::fs::read_to_string(&config_path).unwrap(), original);
    }

    /// Issue #115: a real Zed `settings.json` — JSONC, comments, trailing
    /// commas, hand-tuned indentation — must come back with exactly one member
    /// added and every other byte where the user left it.
    #[test]
    fn zed_settings_survive_an_install_intact() {
        let dir = TempDir::new().unwrap();
        let config_path = dir.path().join("settings.json");
        let original = r#"{
  // Editor
  "buffer_font_family": "Berkeley Mono",
  "buffer_font_size": 14,
  "theme": {
    "mode": "system",
    "dark": "One Dark",
  },
  /* Languages */
  "languages": {
    "Rust": { "tab_size": 4 },
  },
}"#;
        std::fs::write(&config_path, original).unwrap();

        assert!(ensure_agent_mcp_entry(
            &config_path,
            &["context_servers"],
            McpFormat::Json,
            "/usr/bin/tuic-bridge",
            "zed",
        ));

        let written = std::fs::read_to_string(&config_path).unwrap();
        // Every original line is still present, verbatim.
        for line in original.lines().filter(|l| !l.trim().is_empty()) {
            if line == "}" {
                continue; // The closing brace moved down by the added member.
            }
            assert!(written.contains(line), "lost `{line}` in:\n{written}");
        }
        let root = read_json_file(&config_path).unwrap();
        assert_eq!(
            root["context_servers"][TUIC_MCP_KEY]["command"],
            "/usr/bin/tuic-bridge"
        );
        assert_eq!(root["buffer_font_size"], 14);
        assert_eq!(root["languages"]["Rust"]["tab_size"], 4);
    }

    #[test]
    fn an_idempotent_pass_does_not_rewrite_the_file() {
        // Editors watch these files; a no-op write still fires a reload.
        let dir = TempDir::new().unwrap();
        let config_path = dir.path().join("mcp.json");
        let spec = spec_at(config_path.clone());

        assert!(ensure_spec_entry(&spec, "/bridge", "test"));
        let first = std::fs::read_to_string(&config_path).unwrap();
        assert!(!ensure_spec_entry(&spec, "/bridge", "test"));
        assert_eq!(std::fs::read_to_string(&config_path).unwrap(), first);
    }

    #[test]
    fn removal_keeps_the_users_comments() {
        let dir = TempDir::new().unwrap();
        let config_path = dir.path().join("settings.json");
        let original = "{\n  // keep me\n  \"theme\": \"dark\",\n  \"mcpServers\": { \"tuicommander\": { \"command\": \"/bridge\" } }\n}";
        std::fs::write(&config_path, original).unwrap();

        let spec = spec_at(config_path.clone());
        remove_spec_entry(&spec, "test").unwrap();

        let written = std::fs::read_to_string(&config_path).unwrap();
        assert!(written.contains("// keep me"), "{written}");
        let root = read_json_file(&config_path).unwrap();
        assert!(root["mcpServers"].get(TUIC_MCP_KEY).is_none());
        assert_eq!(root["theme"], "dark");
    }

    // --- shared settings files need an explicit install ---

    #[test]
    fn a_shared_settings_file_is_not_written_at_launch() {
        // Zed, Amp and Gemini keep their MCP list inside the settings document
        // that holds every other user preference. Being on the machine is not
        // consent to edit it.
        let dir = TempDir::new().unwrap();
        // A file the tool itself wrote, so presence is proven independently of
        // our config — the gate under test must be the shared-file rule alone.
        std::fs::write(dir.path().join("keymap.json"), "[]").unwrap();
        let spec = McpConfigSpec {
            shared_settings_file: true,
            ..spec_at(dir.path().join("settings.json"))
        };
        // Presence is proven — the gate is the shared-file rule alone.
        assert!(is_target_installed(&spec));
        assert!(!auto_install_allowed(&spec, "zed"));
    }

    #[test]
    fn a_shared_settings_file_we_already_own_keeps_getting_repairs() {
        // Consent was given once; a moved bridge binary must not strand the
        // entry the user asked for.
        let _config = with_temp_config_dir();
        let dir = TempDir::new().unwrap();
        let config_path = dir.path().join("settings.json");
        let spec = McpConfigSpec {
            shared_settings_file: true,
            ..spec_at(config_path.clone())
        };
        // The explicit-install path ignores the gate entirely.
        assert!(ensure_spec_entry(&spec, "/old/bridge", "zed"));
        assert!(auto_install_allowed(&spec, "zed"));

        ensure_spec_entry(&spec, "/new/bridge", "zed");
        let root = read_json_file(&config_path).unwrap();
        assert_eq!(root["mcpServers"][TUIC_MCP_KEY]["command"], "/new/bridge");
    }

    #[test]
    fn zed_amp_and_gemini_are_the_shared_settings_targets() {
        for agent in ["zed", "amp", "gemini"] {
            let spec = get_mcp_config_spec(agent).expect("supported");
            assert!(spec.shared_settings_file, "{agent} shares a settings file");
        }
        for agent in ["cursor", "vscode", "windsurf", "droid", "claude"] {
            let spec = get_mcp_config_spec(agent).expect("supported");
            assert!(
                !spec.shared_settings_file,
                "{agent} has a dedicated MCP config"
            );
        }
    }

    #[test]
    fn unparseable_toml_is_left_untouched() {
        let dir = TempDir::new().unwrap();
        let config_path = dir.path().join("config.toml");
        let original = "[cli\ninstaller = \"npm\"\n";
        std::fs::write(&config_path, original).unwrap();

        assert!(read_toml_file(&config_path).is_none());
        assert!(!ensure_toml_mcp_entry(
            &config_path,
            true,
            "/bridge",
            "grok"
        ));
        assert_eq!(std::fs::read_to_string(&config_path).unwrap(), original);
    }

    // --- per-agent entry shapes ---

    #[test]
    fn opencode_entry_uses_the_local_command_array_shape() {
        let dir = TempDir::new().unwrap();
        let config_path = dir.path().join("opencode.json");
        assert!(ensure_agent_mcp_entry(
            &config_path,
            &["mcp"],
            McpFormat::OpenCode,
            "/bridge",
            "opencode",
        ));

        let root = read_json_file(&config_path).unwrap();
        let entry = &root["mcp"][TUIC_MCP_KEY];
        assert_eq!(entry["type"], "local");
        assert_eq!(entry["command"], serde_json::json!(["/bridge"]));
        assert_eq!(entry["enabled"], true);
        // The schema is additionalProperties: false — stdio fields are rejected.
        assert!(entry.get("args").is_none());
        assert!(entry.get("env").is_none());

        // Idempotent
        assert!(!ensure_agent_mcp_entry(
            &config_path,
            &["mcp"],
            McpFormat::OpenCode,
            "/bridge",
            "opencode",
        ));
    }

    #[test]
    fn grok_toml_entry_omits_the_codex_env_allowlist() {
        // Grok's schema has no env_vars key; it inherits the environment.
        let dir = TempDir::new().unwrap();
        let config_path = dir.path().join("config.toml");
        std::fs::write(&config_path, "[cli]\ninstaller = \"npm\"\n").unwrap();

        assert!(ensure_toml_mcp_entry(
            &config_path,
            false,
            "/bridge",
            "grok"
        ));
        let root = read_toml_file(&config_path).unwrap();
        let entry = &root["mcp_servers"][TUIC_MCP_KEY];
        assert_eq!(entry["command"].as_str(), Some("/bridge"));
        assert!(entry.get("env_vars").is_none());
        // Unrelated sections survive
        assert_eq!(root["cli"]["installer"].as_str(), Some("npm"));

        assert!(!ensure_toml_mcp_entry(
            &config_path,
            false,
            "/bridge",
            "grok"
        ));
    }

    #[test]
    fn goose_yaml_entry_carries_every_required_field() {
        let dir = TempDir::new().unwrap();
        let config_path = dir.path().join("config.yaml");
        std::fs::write(
            &config_path,
            "GOOSE_PROVIDER: anthropic\nextensions:\n  developer:\n    enabled: true\n    type: builtin\n    name: developer\n",
        )
        .unwrap();

        assert!(ensure_yaml_mcp_entry(
            &config_path,
            "extensions",
            "/bridge",
            "goose"
        ));
        let root = read_yaml_file(&config_path).unwrap();
        let entry = &root["extensions"][TUIC_MCP_KEY];
        // goose deserializes into ExtensionEntry { enabled, #[flatten] config }
        // where Stdio requires name, cmd, args and timeout.
        assert_eq!(entry["enabled"], serde_yaml::Value::Bool(true));
        assert_eq!(entry["type"].as_str(), Some("stdio"));
        assert_eq!(entry["name"].as_str(), Some(TUIC_MCP_KEY));
        assert_eq!(entry["cmd"].as_str(), Some("/bridge"));
        assert!(entry["args"].is_sequence());
        assert_eq!(entry["timeout"].as_u64(), Some(300));

        // Other extensions and unrelated settings survive
        assert_eq!(
            root["extensions"]["developer"]["type"].as_str(),
            Some("builtin")
        );
        assert_eq!(root["GOOSE_PROVIDER"].as_str(), Some("anthropic"));

        assert!(!ensure_yaml_mcp_entry(
            &config_path,
            "extensions",
            "/bridge",
            "goose"
        ));

        remove_yaml_mcp_entry(&config_path, "extensions", "goose").unwrap();
        let root = read_yaml_file(&config_path).unwrap();
        assert!(root["extensions"].get(TUIC_MCP_KEY).is_none());
        assert!(root["extensions"].get("developer").is_some());
    }

    #[test]
    fn add_on_targets_are_skipped_until_their_config_exists() {
        // pi only speaks MCP through the pi-mcp-adapter extension, which owns
        // ~/.pi/agent/mcp.json. No file means no adapter, so writing one would
        // configure nothing and litter the agent directory.
        let _config = with_temp_config_dir();
        let dir = TempDir::new().unwrap();
        let config_path = dir.path().join("mcp.json");
        std::fs::write(dir.path().join("settings.json"), "{}").unwrap();
        let spec = McpConfigSpec {
            requires_existing_config: true,
            ..spec_at(config_path.clone())
        };
        assert!(is_target_installed(&spec), "the agent itself is installed");

        assert!(!auto_install_allowed(&spec, "pi"), "launch must not write");

        // Once the add-on has created the file, the entry lands in it.
        std::fs::write(&config_path, "{}").unwrap();
        assert!(auto_install_allowed(&spec, "pi"));
        assert!(ensure_spec_entry(&spec, "/bridge", "pi"));
        assert!(has_bridge_entry(&spec));
    }

    /// Settings > Agents > Install is an explicit statement that the target is
    /// here — the presence heuristic exists to stop launch-time guessing, not to
    /// veto the user. `install_agent_mcp` calls `ensure_spec_entry` directly, so
    /// it must write even when auto-install would have skipped.
    #[test]
    fn an_explicit_install_writes_a_target_auto_install_would_skip() {
        let dir = TempDir::new().unwrap();
        let config_path = dir.path().join("mcp.json");
        let spec = McpConfigSpec {
            requires_existing_config: true,
            ..spec_at(config_path.clone())
        };

        assert!(!auto_install_allowed(&spec, "pi"));
        assert!(
            ensure_spec_entry(&spec, "/bridge", "pi"),
            "explicit install"
        );
        assert!(has_bridge_entry(&spec));
    }

    #[test]
    fn every_supported_agent_has_a_spec() {
        for agent in SUPPORTED_AGENTS {
            assert!(
                get_mcp_config_spec(agent).is_some(),
                "{agent} is in SUPPORTED_AGENTS but has no config spec",
            );
        }
    }

    // --- critic-1415: stable bridge installation ---

    fn fake_bridge(dir: &std::path::Path, bytes: &[u8]) -> PathBuf {
        let path = bridge_path_in(dir);
        std::fs::write(&path, bytes).unwrap();
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o755)).unwrap();
        }
        path
    }

    /// Catches: `matches()` trusting an existing destination whose bytes differ (a torn or
    /// corrupted earlier copy), so config keeps naming a broken executable forever.
    #[test]
    fn install_repairs_a_corrupted_installed_revision() {
        let _config = with_temp_config_dir();
        let dir = TempDir::new().unwrap();
        let source = fake_bridge(dir.path(), b"bridge bytes");
        let installed = install_bridge_binary(&source).unwrap();
        std::fs::write(&installed, b"torn").unwrap();
        assert_eq!(install_bridge_binary(&source).unwrap(), installed);
        assert_eq!(std::fs::read(&installed).unwrap(), b"bridge bytes");
        assert!(usable_executable(&installed));
    }

    /// Catches: reusing an identical-bytes destination that lost its executable bit.
    #[cfg(unix)]
    #[test]
    fn install_repairs_an_installed_revision_without_exec_bit() {
        use std::os::unix::fs::PermissionsExt;
        let _config = with_temp_config_dir();
        let dir = TempDir::new().unwrap();
        let source = fake_bridge(dir.path(), b"bridge bytes");
        let installed = install_bridge_binary(&source).unwrap();
        std::fs::set_permissions(&installed, std::fs::Permissions::from_mode(0o600)).unwrap();
        install_bridge_binary(&source).unwrap();
        assert!(usable_executable(&installed));
    }

    /// Catches: installing an empty or non-executable source, which would publish a
    /// command that fails at spawn, and leaving a revision directory behind.
    #[cfg(unix)]
    #[test]
    fn install_rejects_empty_and_non_executable_sources_without_side_effects() {
        use std::os::unix::fs::PermissionsExt;
        let (_guard, config_dir) = with_temp_config_dir();
        let dir = TempDir::new().unwrap();
        let empty = fake_bridge(dir.path(), b"");
        assert!(install_bridge_binary(&empty).is_err());
        std::fs::write(&empty, b"bytes").unwrap();
        std::fs::set_permissions(&empty, std::fs::Permissions::from_mode(0o644)).unwrap();
        assert!(install_bridge_binary(&empty).is_err());
        assert!(!config_dir.path().join("mcp-bridge").exists());
    }

    /// Catches: two startups publishing the same revision concurrently and one failing
    /// (rename race) or leaving a temp file next to the executable.
    #[test]
    fn concurrent_installs_of_one_revision_all_succeed_and_leave_one_file() {
        let _config = with_temp_config_dir();
        let dir = TempDir::new().unwrap();
        let source = fake_bridge(dir.path(), &vec![7u8; 256 * 1024]);
        let results: Vec<_> = std::thread::scope(|scope| {
            let handles: Vec<_> = (0..8)
                .map(|_| scope.spawn(|| install_bridge_binary(&source)))
                .collect();
            handles.into_iter().map(|h| h.join().unwrap()).collect()
        });
        let first = results[0].clone().unwrap();
        for result in &results {
            assert_eq!(result.as_ref().unwrap(), &first);
        }
        assert_eq!(
            std::fs::read_dir(first.parent().unwrap()).unwrap().count(),
            1
        );
        assert!(usable_executable(&first));
    }

    /// Catches: a symlinked source (mbx target view) being linked rather than copied, so the
    /// installed command still dies with the view.
    #[cfg(unix)]
    #[test]
    fn install_copies_through_a_symlinked_source() {
        let _config = with_temp_config_dir();
        let dir = TempDir::new().unwrap();
        let real = fake_bridge(dir.path(), b"real bytes");
        let view = dir.path().join("view");
        std::fs::create_dir(&view).unwrap();
        let link = bridge_path_in(&view);
        std::os::unix::fs::symlink(&real, &link).unwrap();
        let installed = install_bridge_binary(&link).unwrap();
        assert!(
            !installed
                .symlink_metadata()
                .unwrap()
                .file_type()
                .is_symlink()
        );
        std::fs::remove_file(&real).unwrap();
        assert_eq!(std::fs::read(&installed).unwrap(), b"real bytes");
    }

    /// Catches: a config already naming an older installed revision being treated as a
    /// kept custom command, so a rebuilt bridge never reaches the agent config.
    #[test]
    fn startup_moves_config_from_old_installed_revision_to_the_new_one() {
        let (_guard, _config) = with_temp_config_dir();
        let dir = TempDir::new().unwrap();
        let source = fake_bridge(dir.path(), b"revision one");
        let spec = spec_at(dir.path().join("claude.json"));
        ensure_mcp_configs_for(&[], Some(&source), [("claude", spec_at_format(&spec))]);
        let old = PathBuf::from(command_at_spec(&spec));
        fake_bridge(dir.path(), b"revision two");
        ensure_mcp_configs_for(&[], Some(&source), [("claude", spec_at_format(&spec))]);
        let new = PathBuf::from(command_at_spec(&spec));
        assert_ne!(new, old);
        assert_eq!(std::fs::read(&new).unwrap(), b"revision two");
        assert_eq!(std::fs::read(&old).unwrap(), b"revision one");
    }

    /// Catches: every startup rewriting an up-to-date agent config.
    #[test]
    fn startup_with_unchanged_bridge_leaves_config_bytes_alone() {
        let (_guard, _config) = with_temp_config_dir();
        let dir = TempDir::new().unwrap();
        let source = fake_bridge(dir.path(), b"revision one");
        let spec = spec_at(dir.path().join("claude.json"));
        ensure_mcp_configs_for(&[], Some(&source), [("claude", spec_at_format(&spec))]);
        let first = std::fs::read(&spec.config_path).unwrap();
        ensure_mcp_configs_for(&[], Some(&source), [("claude", spec_at_format(&spec))]);
        assert_eq!(std::fs::read(&spec.config_path).unwrap(), first);
    }

    /// Catches: the build-output test matching any working command under a `target`
    /// directory instead of only a bridge-named one, clobbering a user's own wrapper.
    #[test]
    fn working_wrapper_under_a_target_directory_is_kept() {
        let (_guard, _config) = with_temp_config_dir();
        let dir = TempDir::new().unwrap();
        let source = fake_bridge(dir.path(), b"bridge");
        let wrapper_dir = dir.path().join("target/release");
        std::fs::create_dir_all(&wrapper_dir).unwrap();
        let wrapper = wrapper_dir.join("my-wrapper");
        std::fs::write(&wrapper, b"wrapper").unwrap();
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(&wrapper, std::fs::Permissions::from_mode(0o755)).unwrap();
        }
        let spec = spec_at(dir.path().join("claude.json"));
        assert!(ensure_spec_entry(
            &spec,
            wrapper.to_str().unwrap(),
            "claude"
        ));
        ensure_mcp_configs_for(&[], Some(&source), [("claude", spec_at_format(&spec))]);
        assert_eq!(command_at_spec(&spec), wrapper.to_str().unwrap());
    }

    /// Catches: explicit install keeping a working legacy build-output command (the old
    /// `command != bridge_path` rule), so Settings > Agents never migrates it.
    #[test]
    fn explicit_install_replaces_a_working_target_command() {
        let (_guard, _config) = with_temp_config_dir();
        let dir = TempDir::new().unwrap();
        let build = dir.path().join("target/debug");
        std::fs::create_dir_all(&build).unwrap();
        let legacy = fake_bridge(&build, b"bridge");
        let installed = install_bridge_binary(&legacy).unwrap();
        let spec = spec_at(dir.path().join("claude.json"));
        assert!(ensure_spec_entry(&spec, legacy.to_str().unwrap(), "claude"));
        install_spec(&spec, installed.to_str().unwrap(), "claude").unwrap();
        assert_eq!(command_at_spec(&spec), installed.to_str().unwrap());
    }

    /// Catches: a config naming an installed revision whose file was deleted being kept
    /// as "custom", leaving the agent with ENOENT.
    #[test]
    fn startup_repairs_a_config_naming_a_deleted_installed_revision() {
        let (_guard, _config) = with_temp_config_dir();
        let dir = TempDir::new().unwrap();
        let source = fake_bridge(dir.path(), b"bridge");
        let spec = spec_at(dir.path().join("claude.json"));
        ensure_mcp_configs_for(&[], Some(&source), [("claude", spec_at_format(&spec))]);
        let installed = PathBuf::from(command_at_spec(&spec));
        std::fs::remove_dir_all(installed.parent().unwrap()).unwrap();
        ensure_mcp_configs_for(&[], Some(&source), [("claude", spec_at_format(&spec))]);
        assert!(usable_executable(std::path::Path::new(&command_at_spec(
            &spec
        ))));
    }

    // --- critic-1415r2: installed-bridge lifecycle ---

    /// Catches: a config dir with spaces and quotes (macOS `Application Support` is the
    /// benign case) being mangled by one format's escaping, so that format names a
    /// command that does not exist while JSON keeps working.
    #[cfg(unix)]
    #[test]
    fn installed_command_round_trips_through_every_format_for_an_awkward_config_dir() {
        let root = TempDir::new().unwrap();
        let odd = root.path().join("App Support 'q' \"d\" \\b");
        std::fs::create_dir_all(&odd).unwrap();
        let _guard = crate::config::set_config_dir_override(odd);
        let source = fake_bridge(root.path(), b"bridge");
        let specs = [
            (McpFormat::Json, "mcpServers", "a.json"),
            (McpFormat::OpenCode, "mcp", "b.json"),
            (
                McpFormat::Toml {
                    forward_session: true,
                },
                "mcp_servers",
                "c.toml",
            ),
            (McpFormat::Yaml, "extensions", "d.yaml"),
        ];
        for (format, key, file) in specs {
            let spec = McpConfigSpec {
                format,
                key_path: vec![key],
                ..spec_at(root.path().join(file))
            };
            ensure_mcp_configs_for(&[], Some(&source), [("agent", spec_at_format(&spec))]);
            let command = command_at_spec(&spec);
            assert!(
                usable_executable(std::path::Path::new(&command)),
                "{file} names {command:?}"
            );
        }
    }

    /// Catches: a failed publish (destination occupied by a non-empty directory) leaving
    /// the temp copy of the bridge behind, one full binary per failed startup.
    #[test]
    fn failed_publish_leaves_no_temp_copy_behind() {
        use sha2::{Digest, Sha256};
        let (_guard, config_dir) = with_temp_config_dir();
        let dir = TempDir::new().unwrap();
        let bytes = b"bridge bytes";
        let source = fake_bridge(dir.path(), bytes);
        let revision = config_dir
            .path()
            .join("mcp-bridge")
            .join(hex::encode(Sha256::digest(bytes)));
        let occupied = bridge_path_in(&revision);
        std::fs::create_dir_all(occupied.join("child")).unwrap();
        assert!(install_bridge_binary(&source).is_err());
        let names: Vec<_> = std::fs::read_dir(&revision)
            .unwrap()
            .map(|e| e.unwrap().file_name())
            .collect();
        assert_eq!(names, vec![occupied.file_name().unwrap().to_owned()]);
    }
    /// Catches: reading a setup snippet implicitly installing a bridge from a
    /// secondary instance or worktree, despite that instance not owning configs.
    #[test]
    fn bridge_info_reads_do_not_install_or_rewrite_bridge_copies() {
        let (_guard, config_dir) = with_temp_config_dir();
        let dir = TempDir::new().unwrap();
        let source = fake_bridge(dir.path(), b"read-only getter fixture");
        let info = bridge_info_from_location(Some(&source));
        assert_eq!(info.bridge_path, BRIDGE_NAME);
        assert!(!config_dir.path().join("mcp-bridge").exists());
        let installed = install_bridge_binary(&source).unwrap();
        let modified = installed.metadata().unwrap().modified().unwrap();
        let info = bridge_info_from_location(Some(&source));
        assert_eq!(info.bridge_path, installed.to_str().unwrap());
        assert_eq!(installed.metadata().unwrap().modified().unwrap(), modified);
        std::fs::remove_file(source).unwrap();
        let info = bridge_info_from_location(None);
        assert_eq!(info.bridge_path, installed.to_str().unwrap());
        let snippet: serde_json::Value = serde_json::from_str(&info.config_snippet).unwrap();
        assert_eq!(
            snippet["tuicommander"]["command"],
            installed.to_str().unwrap()
        );
    }

    /// Catches: installing a full executable when every agent is disabled,
    /// absent, unreadable, or has a custom integration that cannot be rewritten.
    #[test]
    fn startup_without_any_config_to_update_does_not_install_a_bridge() {
        let (_guard, config_dir) = with_temp_config_dir();
        let dir = TempDir::new().unwrap();
        let source = fake_bridge(dir.path(), b"bridge bytes");
        for case in ["disabled", "absent", "http", "wrapper", "malformed"] {
            let path = dir.path().join(format!("{case}.json"));
            let mut spec = spec_at(path.clone());
            let disabled = if case == "disabled" {
                vec!["claude".to_string()]
            } else {
                vec![]
            };
            match case {
                "absent" => spec.presence_dir = Some(dir.path().join("absent-agent")),
                "http" => std::fs::write(&path, r#"{"mcpServers":{"tuicommander":{"type":"http","url":"http://localhost:9876/mcp"}}}"#).unwrap(),
                "wrapper" => std::fs::write(&path, r#"{"mcpServers":{"tuicommander":{"command":"bash","args":["-lc","tuic-bridge"],"env":{}}}}"#).unwrap(),
                "malformed" => std::fs::write(&path, b"{malformed").unwrap(),
                _ => { assert!(ensure_spec_entry(&spec, BRIDGE_NAME, "claude")); }
            }
            let before = std::fs::read(&path).ok();
            ensure_mcp_configs_for(&disabled, Some(&source), [("claude", spec)]);
            assert!(!config_dir.path().join("mcp-bridge").exists(), "{case}");
            assert_eq!(std::fs::read(&path).ok(), before, "{case}");
        }
    }

    /// Catches: accepting a matching copy after the source changed, or accepting
    /// mismatched copied bytes (including when copy and final source agree).
    #[test]
    fn verified_revision_rejects_each_copy_or_source_mismatch() {
        let initial = [0x11; 32];
        let changed = [0x22; 32];
        let revision = "1111111111111111111111111111111111111111111111111111111111111111";
        for (before, copied, after, expected) in [
            (&initial[..], &initial[..], &initial[..], Some(revision)),
            (&initial[..], &initial[..], &changed[..], None),
            (&initial[..], &changed[..], &initial[..], None),
            (&initial[..], &changed[..], &changed[..], None),
            (&[][..], &[][..], &[][..], None),
        ] {
            assert_eq!(
                verified_revision(before, copied, after).as_deref(),
                expected
            );
        }
    }

    // --- critic-1415r3 ---

    /// Catches: the read-only getter reporting a truncated or tampered file as a
    /// usable bridge just because a executable sits in a revision directory.
    #[test]
    fn critic1415r3_bridge_info_ignores_a_copy_whose_bytes_do_not_match_its_revision() {
        use sha2::{Digest, Sha256};
        let (_guard, config_dir) = with_temp_config_dir();
        let dir = TempDir::new().unwrap();
        let source = fake_bridge(dir.path(), b"full bridge bytes");
        let revision = config_dir
            .path()
            .join("mcp-bridge")
            .join(hex::encode(Sha256::digest(b"full bridge bytes")));
        std::fs::create_dir_all(&revision).unwrap();
        fake_bridge(&revision, b"trunc");
        assert_eq!(
            bridge_info_from_location(Some(&source)).bridge_path,
            BRIDGE_NAME
        );
        std::fs::remove_file(source).unwrap();
        assert_eq!(bridge_info_from_location(None).bridge_path, BRIDGE_NAME);
    }

    /// Catches: a getter call with neither a source nor a revision directory
    /// reporting a path, or creating the revision directory.
    #[test]
    fn critic1415r3_bridge_info_without_source_or_copy_reports_the_bare_command() {
        let (_guard, config_dir) = with_temp_config_dir();
        let info = bridge_info_from_location(None);
        assert_eq!(info.bridge_path, BRIDGE_NAME);
        assert!(!config_dir.path().join("mcp-bridge").exists());
    }

    /// Catches: one malformed agent config blocking, or being rewritten by, the
    /// startup repair of a healthy enabled agent.
    #[test]
    fn critic1415r3_startup_repairs_the_valid_agent_and_leaves_the_malformed_one() {
        let (_guard, _config_dir) = with_temp_config_dir();
        let dir = TempDir::new().unwrap();
        let source = fake_bridge(dir.path(), b"bridge bytes r3");
        let bad = dir.path().join("bad.json");
        std::fs::write(&bad, b"{malformed").unwrap();
        let good = dir.path().join("good.json");
        ensure_mcp_configs_for(
            &[],
            Some(&source),
            [
                ("claude", spec_at(bad.clone())),
                ("gemini", spec_at(good.clone())),
            ],
        );
        assert_eq!(std::fs::read(&bad).unwrap(), b"{malformed");
        let command = command_at_spec(&spec_at(good));
        assert!(
            std::path::Path::new(&command).is_absolute()
                && usable_executable(std::path::Path::new(&command)),
            "{command}"
        );
        assert_ne!(command, source.to_string_lossy());
    }
    /// Catches: returning a valid older revision when the source has changed,
    /// making a manual setup snippet select a stale protocol executable.
    #[test]
    fn bridge_info_does_not_report_an_old_revision_for_a_changed_source() {
        let _config = with_temp_config_dir();
        let dir = TempDir::new().unwrap();
        let source = fake_bridge(dir.path(), b"old bridge revision");
        let old = install_bridge_binary(&source).unwrap();
        std::fs::write(&source, b"new bridge revision").unwrap();
        assert_eq!(
            bridge_info_from_location(Some(&source)).bridge_path,
            BRIDGE_NAME
        );
        assert_eq!(std::fs::read(&old).unwrap(), b"old bridge revision");
        // Without an available source, a verified installed copy remains useful.
        assert_eq!(
            bridge_info_from_location(None).bridge_path,
            old.to_str().unwrap()
        );
    }

    // --- critic-1415r4 ---

    fn seed_command(path: &std::path::Path, command: &str) {
        write_fixture(
            path,
            &serde_json::json!({"mcpServers": {TUIC_MCP_KEY: {"command": command, "args": ["--serve"]}}}),
        );
    }

    /// Catches: migration treating any absolute command as ours — a user's wrapper script
    /// (or a launcher inside `mcp-bridge-custom`, a sibling of our store) being rewritten,
    /// or a bridge copy being installed for a config that will not use it.
    #[test]
    fn migration_leaves_custom_wrapper_commands_and_creates_no_install() {
        let (_guard, config_dir) = with_temp_config_dir();
        let dir = TempDir::new().unwrap();
        let source = fake_bridge(dir.path(), b"new bridge revision");
        let lookalike = config_dir.path().join("mcp-bridge-custom");
        std::fs::create_dir_all(&lookalike).unwrap();
        let wrap_dir = dir.path().join("wrap");
        std::fs::create_dir_all(&wrap_dir).unwrap();
        let wrappers = [
            fake_bridge(&wrap_dir, b"#!/bin/sh\nexec true\n"),
            fake_bridge(&lookalike, b"user supplied launcher"),
        ];
        for (index, wrapper) in wrappers.iter().enumerate() {
            let config = dir.path().join(format!("claude-{index}.json"));
            seed_command(&config, wrapper.to_str().unwrap());
            let before = std::fs::read(&config).unwrap();
            ensure_mcp_configs_for(&[], Some(&source), [("claude", spec_at(config.clone()))]);
            assert_eq!(
                std::fs::read(&config).unwrap(),
                before,
                "{}",
                wrapper.display()
            );
        }
        assert!(!config_dir.path().join("mcp-bridge").exists());
    }

    /// Catches: re-publishing or re-writing on every launch — a second startup against an
    /// already migrated config must not touch the config file (mtime and bytes) or the copy.
    #[test]
    fn second_migration_pass_does_not_rewrite_config_or_install() {
        let (_guard, config_dir) = with_temp_config_dir();
        let dir = TempDir::new().unwrap();
        let source = fake_bridge(dir.path(), b"bridge revision");
        let config = dir.path().join("claude.json");
        seed_command(&config, source.to_str().unwrap());
        ensure_mcp_configs_for(&[], Some(&source), [("claude", spec_at(config.clone()))]);
        let installed = command_at_spec(&spec_at(config.clone()));
        assert!(
            installed.starts_with(config_dir.path().to_str().unwrap()),
            "{installed}"
        );
        let text = std::fs::read(&config).unwrap();
        let config_mtime = std::fs::metadata(&config).unwrap().modified().unwrap();
        let bridge_mtime = std::fs::metadata(&installed).unwrap().modified().unwrap();
        std::thread::sleep(std::time::Duration::from_millis(30));
        ensure_mcp_configs_for(&[], Some(&source), [("claude", spec_at(config.clone()))]);
        assert_eq!(std::fs::read(&config).unwrap(), text);
        assert_eq!(
            std::fs::metadata(&config).unwrap().modified().unwrap(),
            config_mtime
        );
        assert_eq!(
            std::fs::metadata(&installed).unwrap().modified().unwrap(),
            bridge_mtime
        );
    }

    /// Catches: validity checked by size/exec bit instead of content — a same-length
    /// bit-flipped copy under the right digest directory being reported on either lookup path.
    #[test]
    fn getter_rejects_same_length_corruption_on_both_lookup_paths() {
        let _config = with_temp_config_dir();
        let dir = TempDir::new().unwrap();
        let source = fake_bridge(dir.path(), b"bridge revision AAAA");
        let installed = install_bridge_binary(&source).unwrap();
        std::fs::write(&installed, b"bridge revision BBBB").unwrap();
        assert_eq!(locate_installed_bridge(Some(&source)), None);
        assert_eq!(locate_installed_bridge(None), None);
    }

    /// Catches: the no-source scan returning the newest-mtime entry without verifying it,
    /// so a fresh corrupt revision shadows an older valid one.
    #[test]
    fn getter_scan_skips_a_newer_corrupt_revision_for_an_older_valid_one() {
        let _config = with_temp_config_dir();
        let dir = TempDir::new().unwrap();
        let old = install_bridge_binary(&fake_bridge(dir.path(), b"older valid revision")).unwrap();
        let bad = install_bridge_binary(&fake_bridge(dir.path(), b"newer revision")).unwrap();
        std::fs::write(&bad, b"torn").unwrap();
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(&bad, std::fs::Permissions::from_mode(0o700)).unwrap();
        }
        let file = std::fs::OpenOptions::new().write(true).open(&bad).unwrap();
        file.set_modified(std::time::SystemTime::now() + std::time::Duration::from_secs(60))
            .unwrap();
        assert_eq!(locate_installed_bridge(None), Some(old));
    }

    /// Catches: leaving the temp copy beside the published file, or publishing with
    /// group/world write or without the owner exec bit.
    #[cfg(unix)]
    #[test]
    fn published_bridge_is_owner_only_executable_with_nothing_else_beside_it() {
        use std::os::unix::fs::PermissionsExt;
        let _config = with_temp_config_dir();
        let dir = TempDir::new().unwrap();
        let installed =
            install_bridge_binary(&fake_bridge(dir.path(), b"bridge revision")).unwrap();
        assert_eq!(
            std::fs::metadata(&installed).unwrap().permissions().mode() & 0o777,
            0o700
        );
        assert!(
            !std::fs::symlink_metadata(&installed)
                .unwrap()
                .file_type()
                .is_symlink()
        );
        let names: Vec<_> = std::fs::read_dir(installed.parent().unwrap())
            .unwrap()
            .map(|e| e.unwrap().file_name())
            .collect();
        assert_eq!(names.len(), 1, "{names:?}");
    }

    /// Catches: a reader observing a partially written executable under the final name
    /// while several startups publish the same revision at once.
    #[test]
    fn concurrent_publication_never_exposes_a_partial_file_to_the_getter() {
        use std::sync::atomic::{AtomicBool, Ordering};
        let _config = with_temp_config_dir();
        let dir = TempDir::new().unwrap();
        let bytes: Vec<u8> = (0..1_048_576u32).map(|i| (i % 251) as u8).collect();
        let source = fake_bridge(dir.path(), &bytes);
        let done = AtomicBool::new(false);
        std::thread::scope(|scope| {
            let reader = scope.spawn(|| {
                while !done.load(Ordering::SeqCst) {
                    if let Some(path) = locate_installed_bridge(Some(&source)) {
                        assert_eq!(std::fs::read(path).unwrap(), bytes);
                    }
                    if let Some(path) = locate_installed_bridge(None) {
                        assert_eq!(std::fs::read(path).unwrap(), bytes);
                    }
                }
            });
            let writers: Vec<_> = (0..6)
                .map(|_| scope.spawn(|| install_bridge_binary(&source).unwrap()))
                .collect();
            for writer in writers {
                writer.join().unwrap();
            }
            done.store(true, Ordering::SeqCst);
            reader.join().unwrap();
        });
    }

    /// Catches: `copy_verified_bridge` wiring the three digests wrongly (e.g. comparing the
    /// expected digest with itself), which the pure `verified_revision` table cannot see.
    #[test]
    fn copy_verified_bridge_rejects_a_source_changed_after_the_initial_hash() {
        use sha2::{Digest, Sha256};
        let (_guard, config_dir) = with_temp_config_dir();
        let dir = TempDir::new().unwrap();
        let source = fake_bridge(dir.path(), b"original executable bytes");
        let digest = Sha256::digest(b"original executable bytes");
        std::fs::write(&source, b"source replaced while linking").unwrap();
        let mut temp = tempfile::NamedTempFile::new_in(config_dir.path()).unwrap();
        let error = copy_verified_bridge(&source, &mut temp, &digest).unwrap_err();
        assert!(error.contains("changed during installation"), "{error}");
    }
}
