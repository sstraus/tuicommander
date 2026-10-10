# Configuration

**Module:** `src-tauri/src/config.rs`

Manages all application configuration as JSON files in the platform config directory.

## Config Directory

| Platform | Path |
|----------|------|
| macOS | `~/Library/Application Support/com.tuic.commander/` |
| Linux | `~/.config/com.tuic.commander/` |
| Windows | `%APPDATA%/com.tuic.commander/` |

Legacy paths `{platform_config}/tuicommander/`, `{platform_config}/tui-commander/`
and `~/.tuicommander/` are auto-migrated on first launch.

`tuic-remote --instance <id>` selects an isolated named namespace at process
bootstrap. Named files live below the platform path above at
`instances/<id>/`; the default instance keeps the paths in the table unchanged.
The ID is a 1–63 character lowercase ASCII DNS label (alphanumeric ends,
internal hyphens allowed), and `default` is reserved. A named instance never
runs the default or legacy file migrations and never falls back to those
locations when its own files are absent.

The desktop binary has no `--instance` flag of its own (Tauri's own CLI/single-instance
plugin already owns argv there), but reuses the same `AppInstance::named` selection
via the `TUIC_APP_INSTANCE` env var, read once at the very top of `run()` in
`lib.rs`, before any `config_dir()` call. `TUIC_APP_INSTANCE=<id> make dev` (or any
debug/test launch) gets the same isolated `instances/<id>/` namespace `tuic-remote
--instance <id>` gets — an actual code-enforced boundary rather than a documented
risk, closing the gap in AGENTS.md's "Isolation caveat" where a second debug
instance previously shared `repositories.json` with Boss's production instance with
nothing but discipline preventing a throwaway test repo from being persisted there
(#763-d219). An invalid or already-selected id fails the process at startup rather
than silently falling back to the default instance.

Which target defaults to what is deliberate and guarded. `make test` is the
throwaway verification launch and defaults to `instances/tuic-test/`; `make dev`
is Boss's daily driver and carries **no** instance, because switching it looks
exactly like every repository having vanished. That default is written
`test: TUIC_APP_INSTANCE?=tuic-test` — target-specific. A line-start
`TUIC_APP_INSTANCE?=…` is a *global* make variable however far down the file it
appears, reads identically, and has sent `make dev` to the empty test instance
twice; `scripts/check-make-instance-scope.sh` asks `make -n` what each target
actually expands, and runs from both `make check` and `pre-commit`. An
environment assignment still beats a target-specific `?=`, so
`TUIC_APP_INSTANCE=<id> make dev` keeps working and `make dev` announces the
configuration directory it starts on — no check can see a developer's shell.

The `tuic` CLI and `tuic-bridge` sidecar use the same immutable instance
selection through `tuic-ipc`. Both accept `--instance <id>`, which takes
precedence over `TUIC_APP_INSTANCE`. The CLI's background runner inherits that
selection, so wake markers stay in the same instance directory. On Unix,
`TUIC_SOCKET` takes precedence over automatic endpoint resolution.

The credential namespace follows the same immutable selection. The default
vault remains keyring service `tuicommander`, user `vault`; a named instance
uses service `tuicommander-instance-<id>`, user `vault`. Named instances never
scan, import, modify, or delete default or legacy credential entries, including
the dynamic legacy MCP credential locations. Release `tuic-remote` probes a
named vault before binding its socket and exits on failure; it does not interpret
a keyring error as an empty vault or fall back to a file. Debug builds retain the
file-backed credential adapter, scoped below the selected instance directory.
Instance selection precedes `--set-password`, so password setup writes only to
the selected namespace.

`TUIC_CAPTURE_DIR` overrides only the raw PTY capture directory. Set it to an
absolute path before starting TUICommander to write `.tcap` files outside the
instance config directory. A relative value rejects capture activation; it
does not fall back to the default directory. `GET /diagnostics/capture` reports
the selected directory while recording.

Desktop verification may also set `TUIC_PORT=<port>` to choose the process-local
HTTP listener port without changing `config.json`; an occupied port still uses the
existing next-port retry.

**For the default instance, debug and release builds share this one directory**
— `config_dir()` never branches on `cfg!(debug_assertions)`. The single-instance lock is release-only
(`lib.rs`, `#[cfg(not(debug_assertions))]`), so a `make dev` build runs happily
alongside the installed app, and both read and write the exact same
`config.json`, `repositories.json`, and every other file below. What makes that
safe is the locking model in `ConfigFile<T>` (see Core Functions): a
cross-process advisory file lock. Ordinary `AppConfig`, upstream MCP, and
repository writes additionally apply caller deltas to the latest value while
that lock is held, so independent edits from two processes compose instead of
becoming ordered whole-document overwrites. `repositories.json` used to be the
one exception, seeded into a separate `~/.tuicommander-dev/` directory on first
debug run; that seeding path is gone and it now lives here like everything
else (see below).

### A Rust test can never name the real directory

In a `cfg(test)` build `config_dir()` returns the override set by
`set_config_dir_override(dir)` when one is in scope, and otherwise a safe,
process-scoped fallback under the OS temp directory
(`test_fallback_config_dir`, a `OnceLock` computed once per test process:
`<temp_dir>/tuic-test-fallback-<pid>`) — **never** the user's platform config
directory, with or without an explicit override.

It used to fall back to the user's platform directory instead, so a test that
forgot the override read and wrote Boss's live `config.json` and
`repositories.json` in silence. That silence is what let fifteen `tempfile`
roots become permanent repository rows (#763-d219): the damage was
indistinguishable from normal operation until someone diffed the file.

A first fix made the no-override branch **panic** instead of falling back —
correct in isolation, but it reproducibly broke the full `cargo nextest run
--lib` suite two different ways: (1) a `#[should_panic]` test exercising that
exact panic poisoned the guarding mutex on unwind (a temporary `MutexGuard`
was still alive through the panicking expression), and a later `Drop` call
locking the poisoned mutex aborted the whole process (SIGABRT) instead of
unwinding; and (2) any test that already held an explicit override (via an
`isolated_config()`-style helper) and then called a shared helper that also
tried to set one self-deadlocked, because the guarding mutex is not reentrant.
The silent process-scoped fallback removes both failure modes: nothing needs
the exclusive lock unless a test deliberately wants one, and dozens of
call chains that reach `config_dir()` without caring where it lives (most not
touching `repositories.json` at all) get a safe, stable-within-the-process
answer instead of a panic or a hang. `config_dir_in_a_test_never_names_the_real_user_directory`
and `fallback_config_dir_is_never_the_real_directory` (`config.rs`) prove the
fallback and the real directory can never coincide.

A thread-local fallback was considered and rejected: code under test that
offloads work to `spawn_blocking` (`finalize_merged_worktree`,
`merge_and_archive_worktree`) runs on a different OS thread than the test
itself, so a thread-local override set on the test's thread would not be
visible there — reintroducing the same gap on a thread the test can't reach.

`without_config_dir_override()` takes the same exclusive lock while
deliberately leaving the override unset — the one way to observe the fallback
branch without racing a concurrent test that did set an override. It is not an
escape hatch.

This guard covers unit tests only. Integration tests under `src-tauri/tests/`
compile the library without `cfg(test)`, and `make dev` is not a test at all —
both isolate with `TUIC_APP_INSTANCE=<id>` as described above.

The `plugins/` subdirectory holds external plugin packages. Version 1.7.7
externalizes Plan Tracker and Stories Ticker by seeding `plugins/plan/` and
`plugins/stories-ticker/` once. The root marker
`.externalized-plan-stories-v1` records completion: existing packages are never
overwritten, and removing either seeded package after migration is permanent.

One runtime cache and one transient SSH socket support SSH-managed remote daemons:

- `remote-bin/<version>/tuic-remote-<target>` stores a verified release asset
  after an atomic staging download. It contains executables, never credentials.
- `~/.ssh/tuic-%C` is the hashed OpenSSH ControlMaster socket shared by TUIC's
  tunnel, remote-command and SCP processes. Keeping it under the user's short,
  access-controlled SSH directory avoids macOS `Application Support` spaces and
  keeps the expanded Unix socket path below the platform length limit. TUIC uses
  `ControlPersist=no`: a live tunnel remains the reusable master, while a
  one-shot command cannot leave an unmonitored master behind.

Remote pairing tokens use the credential vault key
`remote/connection/<uuid>/pairing-token`; they never live in either directory or
in `connections.json`.
Each connection stores `auto_update` there. Missing values default to `false`.

## Core Functions

Launch assets use temporary-file writes, fsync and atomic rename. On Unix,
`claude.json` is published as `0600` and `codex-notify.sh` as `0700`: the script's
execute permission is set before rename, so concurrent boots cannot expose a
non-executable notify script. Other config writes retain the `0600` default.

```rust
pub fn config_dir() -> PathBuf
pub fn load_json_config<T: DeserializeOwned + Default>(filename: &str) -> T
```

Config domains write through `ConfigFile<T>`:

```rust
impl<T: Serialize + DeserializeOwned + Default> ConfigFile<T> {
    pub fn load(&self) -> (T, Stamp)
    pub fn update<F: FnOnce(&mut T) -> bool>(&self, mutate: F) -> Result<(), String>
    pub fn update_with<R, F>(&self, mutate: F) -> Result<R, String>
    pub fn update_with_strict<R, F>(&self, mutate: F) -> Result<R, String>
    pub fn save_checked(&self, value: &T, stamp: Stamp) -> Result<(), ConfigWriteError>
    pub fn save_delta(&self, base: &T, desired: &T) -> Result<(), String>
    pub fn save_delta_strict(&self, base: &T, desired: &T) -> Result<(), String>
    pub fn save_delta_recovering(&self, base: &T, desired: &T) -> Result<(), String>
    pub fn save(&self, value: &T) -> Result<(), String>
}
```

Two locks protect every write: an in-process `CONFIG_WRITE_LOCK` mutex, and a
cross-process advisory file lock (`std::fs::File::lock()` on a sibling
`<file>.lock`) that serializes writers across the debug/release instances that
now share one config dir. `save_checked` additionally compares a `Stamp`
(mtime+len, captured at `load()`) against the file's current on-disk state and
returns `ConfigWriteError::Conflict` instead of overwriting a change it never
saw. Interactive per-domain saves use `save_delta`: each request carries the
snapshot loaded by that client (`base`) and its edited document (`config`). The
server applies only the base-to-config changes to the latest locked file. Object
keys merge recursively, arrays replace as a unit, and JSON null deletes a key.
Null also deletes a key within a newly added nested object, so that key is not
persisted there.
`save_delta_strict` uses the same rules but refuses to replace a corrupt notes
file. Dictation's recovery save rechecks under the lock: it repairs a file that
is still malformed, or merges its edit into a valid file saved meanwhile.
Remote connection edits use the same JSON delta per connection id; they also
hold the shared file lock while reading and writing `connections.json`.
`config.json` (`AppConfig`) and `mcp-upstreams.json` also merge deltas under
the lock. `repositories.json` uses the ID-keyed optimistic delta protocol
documented below. See
[`2026-08-08-config-deltas-under-lock.md`](../decisions/2026-08-08-config-deltas-under-lock.md).

### Corrupt Files Are Moved Aside, Never Overwritten

A config file that exists but does not parse is renamed to
`<name>.corrupt-<uuid>` before anything falls back to defaults
(`preserve_corrupt_config`). The UUID is fresh per occurrence on purpose: a fixed
backup name would let a second corrupt load erase the document the first one
saved, which is the same data loss one step removed. Nothing ever deletes these
files — recovery is by hand.

Two entry points reach it. `load_json_config_strict` refuses to return `Default`
for a broken file on the **read** side (`notes.json`, GH #107), and
`update_with_strict` does the same on the **write** side, so a read-modify-write
aborts instead of persisting `Default` over real data (`repositories.json`,
`mcp-upstreams.json`).

`config.json` reaches neither, because `load_app_config` must return an
`AppConfig` and has no error channel to a caller. It preserves the file directly:
an unparseable `config.json` is moved aside and defaults are returned, so the
first-run branch that fills in a missing session token and VAPID key is free to
write a fresh document without destroying the old one. Only a **parse** failure
triggers the rename — an I/O error leaves the file alone, since the document may
be intact and only the read failed.

## Config Files and Commands

### Application Config (`config.json`)

**Type:** `AppConfig`

Frontend surfaces use the shared `updateAppConfig()` queue. Each save sends the
loaded snapshot and edited config; the queue keeps writes within one WebView in
order.

**Ordinary saves merge under the cross-process lock; they do not replace the
document.** IPC `save_config`/`save_app_config`, `PUT /config`, and MCP `config`
with `action: "save"` require `{ "base": <loaded AppConfig>, "config": <edited
AppConfig> }`. The backend computes that client's changes before taking the
file lock. `commit_config_save` locks `config.json`, reloads and hydrates the
latest disk value, applies only those changes, persists it, and refreshes
`state.config` from the result. Objects merge key by key; arrays and scalars
replace wholesale (so an empty array still clears a list, `null` clears an
optional field, and `""` still blanks a string).
This is not cosmetic: every field carries `#[serde(default)]`, so deserializing a
partial body on its own reset the omitted ones — `services.server.enabled` defaults
to `false`, which is how a partial save used to switch remote access off on disk
while the already-bound listener kept serving, surfacing only at the next boot.

All three writers also share `server_settings_changed` and rebind the listener
through `restart_after_server_settings_change` when `services.server.{enabled,port,
ipv6_enabled}` or `services.auth.{username,password_hash}` move, so the running
process can never serve a configuration the disk disagrees with.

`commit_config_change` itself bumps `relay.config_revision` after every write.
`relay_client::supervise` — spawned at boot whether or not the relay is enabled —
watches that counter and starts, stops or restarts the relay client when
`services.relay.{enabled,url,token,session_id}` move. The signal is raised at the
choke point rather than by each writer on purpose: a `ConfigSaveEffects` flag is
only actioned by the callers that remember it, and a relay toggle that silently
needs an app restart is what that costs.

| Field | Type | Default | Description |
|-------|------|---------|-------------|
| `shell` | `Option<String>` | `None` | Shell override (platform default if None) |
| `font_family` | `String` | `"JetBrains Mono"` | Terminal font family |
| `font_size` | `u16` | `14` | Terminal font size |
| `theme` | `String` | `"commander"` | Terminal theme. An empty or unknown key falls back to `commander` (`DEFAULT_THEME`, `src/stores/settings.ts`) |
| `ide` | `String` | `""` | IDE for "Open in..." |
| `ego_executable` | `String` | `""` | Absolute path to the one ego binary this host may launch for ACP. Read at each connect, so a correction takes effect without a restart. Empty means ACP is not configured here and every connect is refused. No ACP command carries it: a connect supplies a working directory and nothing else, so no request can choose which binary runs. It is edited in `Settings > General` and written through `save_config` like any other field |
| `ai_chat_workspace` | `String` | `""` | Absolute directory every AI Chat conversation runs in. Empty means the home directory of the host that runs ego. A missing directory is created at connect; a relative path or an uncreatable one is refused with a message naming this setting. Conversations and peer identities are keyed by this path, so changing it starts from a fresh conversation list. Edited in `Settings > General > AI Chat workspace`, which refuses a non-absolute path |
| `ego_profile` | `String` | `""` | Optional name of an ego user-config profile for AI Chat ACP launches. Empty omits `--profile`; a nonempty valid name adds `--profile <name>` after the repository root. Names with whitespace, control characters, a leading dash, or more than 64 UTF-8 bytes are refused before launch. TUICommander does not copy profile policy into ACP requests. When `.tuic.json` selects a repo profile, this explicit selection is also its non-relaxable ACP `ceilingProfile`. |
| `ai_chat_sessions` | `Map<String, String>` | `{}` | Last selected ego session ID per repository root. The AI Chat panel saves it through the shared serialized config update path and uses it to load the previous conversation after restart. |
| `ai_chat_peer_ids` | `Map<String, String>` | `{}` | Host-issued ACP orchestration peer UUID per canonical repository root. The backend persists it before launching ego and reuses it across reconnect and restart. It is not a PTY tab ID. |
| `default_font_size` | `u16` | `13` | Default font size for reset |
| `attachment_max_bytes` | `u64` | `26214400` (25 MiB) | Binary upload cap per file; must be positive. |
| `attachment_retention_days` | `u32` | `7` | Uploaded files older than this are removed when their PTY or ACP session closes. |
| `mcp_server_enabled` | `bool` | `true` | Enable MCP HTTP server |
| `mcp_port` | `u16` | `9876` | Fixed port for MCP server (0 = OS-assigned) |
| `collapse_tools` | `bool` | `false` | Replace the full MCP tool list with 3 lazy-discovery meta-tools (`search_tools`, `get_tool_schema`, `call_tool`). Discovered native schemas are unchanged: managed commands still use one `call_tool` invocation of `session action=submit` and receive the bounded receipt in that response. Grok sessions use this surface automatically without changing the stored value — see [`mcp-http.md`](mcp-http.md#lazy-tool-discovery-collapse_tools). Size figures for both surfaces, and how to reproduce them, are in [Measuring the surfaces](mcp-http.md#measuring-the-surfaces) — the reduction is measured, never estimated |
| `services` | `ServicesConfig` | `{}` | Nested remote-access config: `server`, `auth`, `tls`, `relay`, `push` (replaces the former flat `remote_access_*`/`push_enabled`/`relay_enabled` fields) |

Remote-access secrets under `services` are not persisted in plaintext
`config.json`: `auth.session_token`, `relay.token`, and
`push.vapid_private_key` live in the OS keyring-backed credential vault. The
JSON file keeps only the non-secret settings plus `session_token_exists`,
`token_exists`, and `vapid_private_key_exists` booleans for UI state.

A vault **read failure** is never treated as "the secret is absent": on error
`hydrate_one_secret` keeps the `*_exists` flag that `config.json` recorded, so a
momentarily locked keychain cannot flip the flag to `false` and make the next
save delete a live credential. Plaintext still found in `config.json` is moved
into the vault at load time and the file is rewritten immediately, so the
cleartext copy does not survive on disk.

| `confirm_before_quit` | `bool` | `true` | Show quit confirmation |
| `confirm_before_closing_tab` | `bool` | `true` | Show tab close confirmation |
| `copy_on_select` | `bool` | `true` | Auto-copy terminal selection to clipboard |
| `osc52_clipboard` | `bool` | `true` | Honor OSC 52 clipboard-write sequences from terminal output (a notice shows on each write; disable to ignore them) |
| `bell_style` | `String` | `"visual"` | Terminal bell: "none", "visual", "sound", "both" |
| `disabled_agents` | `Vec<String>` | `[]` | Agent IDs hidden from the Add menu |
| `global_hotkey` | `Option<String>` | `null` | OS-level window toggle hotkey combo |
| `intent_tab_title` | `bool` | `true` | Show agent intent as tab title |
| `language` | `String` | `"en"` | UI language code |
| `max_tab_name_length` | `u32` | `25` | Max tab name display length |
| `tab_cycling_all_types` | `bool` | `false` | When true, next/prev-tab shortcuts cycle file/diff/markdown/editor tabs too (default cycles terminals only) |
| `tab_tree_enabled` | `bool` | `false` | Opt-in sidebar activity cards. `false` renders no activity caret, card, or nested agent/session rows. `true` lets every branch with an open terminal session expand its assigned sessions. Applies immediately; no restart is required. |
| `prevent_sleep_when_busy` | `bool` | `false` | Prevent macOS sleep when terminal is busy |
| `suggest_followups` | `bool` | `true` | Show `suggest:` follow-up actions |
| `issue_filter` | `Option<String>` | `"assigned"` | GitHub Issues filter: "assigned", "created", "mentioned", "all", "disabled" |
| `experimental_features_enabled` | `bool` | `false` | Opts in to the AI Chat panel shell and SSH Tunnels. It has no sub-flags: `ai_chat_enabled`, `ai_triage_enabled` and `ai_watchers_enabled` went with the embedded AI engine (#784-0aec). A `config.json` written before that upgrade still carries them and still loads — `AppConfig` has no `deny_unknown_fields`, and `a_config_written_before_the_ai_engine_was_deleted_still_loads` holds that open |
| `auto_show_pr_popover` | `bool` | `false` | Auto-show PR popover when switching to a branch with a PR |
| `update_channel` | `String` | `"stable"` | Update channel: "stable" or "nightly" |
| `inline_blame_enabled` | `bool` | `true` | Show GitLens-style inline git blame on the code editor's active line |
| `show_block_timestamps` | `bool` | `true` | Label each command block with its elapsed time while Ctrl+Cmd is held. Frontend-gated (painted by the renderer); stored here for persistence |
| `show_scrollbar_marks` | `bool` | `true` | Draw command-block marks on the terminal scrollbar. Frontend-gated, toggled from Settings > Terminal |
| `block_folding_enabled` | `bool` | `true` | Let the `block-fold-toggle` shortcut collapse a command block's output. Frontend-gated. Blocks already folded stay folded when this is off |
| `scrollback_reflow` | `bool` | `true` | Re-wrap scrollback history on a column resize instead of truncating it. Backend-gated: `AppState::new_vt_log_buffer` applies it to a new grid and `commit_config_change` pushes a change to grids already open. Defaults `true` — including for a config.json written before the key existed — because the grid reflowed unconditionally before the flag had a consumer |
| `index_strategy` | `String` | `"active_and_switch"` | Which repos get a BM25 content index: `"active_and_switch"` (the boot repo plus every repo switched to), `"active_only"` (boot repo only), `"all_sequential"`, `"disabled"`. Read from the in-memory config on every switch and every `RepoChanged` — never `load_app_config()`, which takes a cross-process file lock |
| `index_memory_budget_mb` | `usize` | `1024` | Total heap the resident content indices may hold before `content_index::enforce_memory_budget` drops the least recently used. An ordinary repo indexes to 60–100 MB, so 1 GB holds 10–15 resident and only evicts for an outlier. An evicted index is snapshotted to `<data_dir>/content-index/` and reloaded on return, so eviction costs a stat walk rather than a rebuild. Configurable rather than a constant because the Rust backend does not hot-reload |

**Commands:** `load_app_config()`, `save_app_config(base, config)`

Every writer of `config.json` — IPC `save_config`, `PUT /config`, MCP
`config action=save`, session-token rotation, `set_global_hotkey`, the
`disabled_mcp_agents` toggle and the push auto-enable on first subscription —
goes through
`config::commit_config_change`, which holds one process-wide mutex across the
whole cache-delta → file-lock → latest-disk-read → delta-merge →
preserve-secrets → write → update-`state.config` sequence. The cross-process
file lock spans the authoritative disk read and write. This distinction matters:
locking whole-document saves merely orders lost updates, while applying the
delta after the locked read preserves unrelated fields written by another
debug or release process.
Rotation
(`config::rotate_session_token`, shared by the desktop command and
`POST /auth/rotate-session-token`) goes through the same path so the vault, the
file and `state.config` cannot disagree — previously the in-memory config kept
the pre-rotation token and the next unrelated save wrote it back.

The vault and `config.json` are one logical commit. Before changing any of the
three vault-backed fields, `save_app_config` snapshots their previous values.
If either a later vault operation or the atomic file replacement fails, all
three vault values are restored before the error returns; `state.config` and
the live authentication token are updated only after success. A rollback
failure is appended to the original persistence error instead of being hidden.
Individual credential `set` and `delete` operations also publish their
in-memory vault clone only after the OS keyring accepts it.

Routing every writer through it also guarantees the file is produced by
`config_for_disk`. A writer that serialized the config itself (the
`disabled_mcp_agents` toggle called `save_json_config("config.json", ..)`)
skipped the stripping step and wrote the session token, relay token and VAPID
private key to disk in cleartext.

### MCP Bridge Auto-Install

On launch `agent_mcp::ensure_mcp_configs` writes the `tuicommander` bridge
entry into each supported agent's own MCP config only when a non-empty,
executable `tuic-bridge` is beside the running executable. Launches from a
temporary directory, App Translocation, a mounted volume or an AppImage mount
do not write agent configs. A test binary without that sidecar also leaves the
configs untouched. Launch-time installation and repair belong to the unnamed
default instance outside a linked Git worktree. Named instances and worktree
builds leave agent configs unchanged, including integrations disabled in the
default instance. Set `TUIC_MCP_CONFIG_OWNER=1` on a launch only when that
instance is deliberately assigned ownership of global agent configs. Explicit
Install and Remove actions in Settings remain user-requested edits.
Each target uses the format its tool reads:

| Agent | Config file | Shape |
|---|---|---|
| Claude Code | `~/.claude.json` | JSON `mcpServers` |
| Cursor | `~/.cursor/mcp.json` | JSON `mcpServers` |
| Windsurf | `~/.codeium/windsurf/mcp_config.json` | JSON `mcpServers` |
| VS Code | `<user dir>/mcp.json` | JSON `servers` |
| Zed | `~/.config/zed/settings.json` | JSON `context_servers` |
| Amp | `~/.config/amp/settings.json` | JSON `amp.mcpServers` |
| Gemini CLI | `~/.gemini/settings.json` | JSON `mcpServers` |
| Droid | `~/.factory/mcp.json` | JSON `mcpServers` |
| opencode | `~/.config/opencode/opencode.json[c]` | JSON `mcp`, `{type:"local", command:[…]}` |
| Codex | `$CODEX_HOME/config.toml` or `~/.codex/config.toml` | TOML `[mcp_servers]` + `env_vars` allowlist |
| Grok | `~/.grok/config.toml` | TOML `[mcp_servers]` |
| goose | `~/.config/goose/config.yaml` | YAML `extensions` (`ExtensionEntry`) |
| pi | `~/.pi/agent/mcp.json` | JSON `mcpServers` (pi-mcp-adapter extension) |

Aider is absent because it has no MCP client.

**A target is written only when it is installed.** The writer creates every
missing parent directory, so an unconditional pass used to create `~/.cursor/`,
`~/.gemini/`, `~/.config/amp/` and friends for tools the user never had —
which makes *other* software report Cursor or Windsurf as installed. Presence is
proven two ways, cheapest first:

1. the config directory holds a file that is not the one we write (`.DS_Store`
   and stale `*.tmp` staging files do not count), or
2. one of the target's CLI binaries resolves via `cli::has_cli`.

Claude's config sits in `$HOME`, so it uses `~/.claude` as its presence
directory instead of the config file's parent. pi is stricter still: its MCP
support comes from the optional pi-mcp-adapter extension, which owns
`~/.pi/agent/mcp.json` — with no such file there is no adapter, so an
auto-written entry would configure nothing.

A target that already holds a `tuicommander` entry gets a path repair only when
its command is the bare `tuic-bridge` name or a broken absolute executable path.
Wrapper and templated commands, and HTTP entries with a URL or URI, belong to
the user and are left unchanged at launch. An explicit install rejects a custom
command or HTTP transport until the user removes that entry. An explicit
install may use the bare `tuic-bridge` name for a new entry when no bridge is
located; it reports an error rather than using that fallback to repair an
existing entry. A working absolute command is preserved. All target-presence
gates live in `auto_install_allowed`,
which only the launch pass consults: Settings → Agents installs on demand
through `ensure_spec_entry` directly, because pressing Install states that the
target is there — that is an explicit request, not a guess.

**Shared settings files need an explicit install.** Zed, Amp and Gemini keep
their MCP server list inside the `settings.json` that also holds every other
user preference, not a dedicated `mcp.json`. Those three carry
`shared_settings_file: true` and the launch pass never creates or edits them:
TUICommander being on the machine is not consent to rewrite the user's editor
configuration. Settings → Agents installs them on request, and once installed
their missing or unusable bridge commands can be repaired like any other target.
`get_agent_mcp_status` returns
the flag so the UI can say why the entry is missing.

### Never Reserializing a Third-Party Config

Configs that exist but do not parse are **never** overwritten (JSON, TOML and
YAML alike). Treating a parse failure as an empty document is what reduced a
user's 400-line Zed `settings.json` to our single entry
([#115](https://github.com/sstraus/tuicommander/issues/115)).

JSON targets are never reserialized as a whole. `jsonc_edit`
parses the document into a concrete syntax tree, splices exactly one member,
and prints it back, so text outside that member is byte-identical. This matters
three times over — `serde_json` rejects the comments and trailing commas Zed,
VS Code and opencode all document as supported; `serde_json::Map` is a
`BTreeMap`, so a round trip alphabetises the user's keys; and
`to_string_pretty` discards their indentation.

Only the documented dialect is accepted. Comments and trailing commas parse;
single-quoted strings, unquoted keys and hexadecimal numbers do not, because a
file using them is one the owning tool cannot read either — writing it back as
if it were fine would be worse than refusing. TOML edits preserve comments and
formatting through `toml_edit`. Goose YAML edits splice the bridge command or
entry with the section's indentation and compare the parsed document before
and after to verify that other extensions and fields remain unchanged;
unsupported YAML layouts are left untouched.
Existing config files are backed up once under TUICommander's `mcp-backups`
directory before any edit. Atomic replacements preserve the file's permissions
and follow config symlinks to their targets.

Guard rails around the write:

- The edited text is re-parsed and the member compared against what was
  requested before anything reaches disk.
- The first time we modify a file, its original is copied to
  `<config dir>/mcp-backups/<agent>-<filename>.orig`. Written once and never
  overwritten — the state worth keeping is the one from before TUICommander
  ever touched it. It lives under our config directory rather than beside the
  original, where the owning tool might try to load or sync it.
- An edit that changes nothing skips the write entirely, so an idempotent pass
  never moves the mtime of a file an editor is watching.

### Removing Every Integration

`remove_all_mcp_integrations` drops the `tuicommander` entry from every target
that has one and adds them all to `disabled_mcp_agents` — without that the next
launch reinstalls them and the action does nothing. `list_installed_mcp_integrations`
backs the Settings → Agents panel that lists them. Uninstalling TUICommander
otherwise leaves a dangling `tuic-bridge` path in each client it ever
configured, and each one reports a broken MCP server on startup. One
unparseable config does not abort the sweep: the rest are still cleaned and the
failures are reported together.

### Upstream MCP Config (`mcp-upstreams.json`)

**Type:** `UpstreamMcpConfig`

Interactive saves carry both the configuration the caller loaded (`base`) and
its desired `config`. The backend derives additions, intentional removals,
order changes, and per-server field deltas keyed by stable server ID, then
applies them to the latest document inside `ConfigFile::update_with`. A popup
toggle therefore changes only `enabled`; an OAuth/DCR auth record written after
the popup loaded is preserved. Removing a server or clearing an optional auth
field remains explicit and is not mistaken for an omitted/unchanged field.

Validation and the runtime registry diff use the exact merged pre/post values
from the locked transaction. The lock is released before asynchronous reconnect
work starts.

Header metadata uses an explicit empty array when no headers remain, so removing the last header stays a valid config delta. Authenticated HTTP upstreams cannot change origin in place. The locked save compares the latest prior entries by ID or credential-owning name, before persistence and activation, even if the request clears auth/header metadata. Use a separate upstream name for another provider. Same-origin path edits remain allowed.

**Commands:** `load_mcp_upstreams()`, `save_mcp_upstreams(base, config)`

### Notification Config (`notifications.json`)

**Type:** `NotificationConfig`

| Field | Type | Default | Description |
|-------|------|---------|-------------|
| `enabled` | `bool` | `true` | Global enable |
| `volume` | `f64` | `0.5` | Volume (0.0-1.0) |
| `sounds.question` | `bool` | `true` | Play on agent question |
| `sounds.error` | `bool` | `true` | Play on error |
| `sounds.completion` | `bool` | `true` | Play on completion |
| `sounds.warning` | `bool` | `true` | Play on warning |
| `silence_remote_completions` | `bool` | `true` | Suppress the completion chime for HTTP/MCP-created sessions |
| `toasts_in_bell` | `bool` | `true` | Mirror every toast into the toolbar bell, under a MESSAGES section |
| `pr_native_notifications` | `bool` | `true` | OS notification for PR ready / CI failed / changes requested / merged transitions |

**Commands:** `load_notification_config()`, `save_notification_config(base, config)`

### Files the embedded AI engine owned (#784-0aec)

`ai-chat-config.json`, `providers.json`, `ai-prompts.json`, `ai-watchers.json`,
`ai-cron.json` and `ai-chat-conversations/` are no longer read or written.
Nothing migrates or deletes them: a file left behind by an older version is
inert, and removing a user's data on upgrade is a worse default than leaving it.

`ai-sessions/<session_id>.json` is the exception and stays live — `pty.rs` writes
command outcomes there and `ai_agent::knowledge::spawn_persist_task` flushes them,
neither of which involves a model. Nothing reads them back yet.

### UI Preferences (`ui-prefs.json`)

**Type:** `UIPrefsConfig`

| Field | Type | Default | Description |
|-------|------|---------|-------------|
| `sidebar_visible` | `bool` | `true` | Sidebar visibility |
| `sidebar_width` | `u32` | `260` | Sidebar width in pixels |
| `diff_panel_visible` | `bool` | `false` | Diff panel open |
| `markdown_panel_visible` | `bool` | `false` | Markdown panel open |
| `notes_panel_visible` | `bool` | `false` | Notes panel open |
| `file_browser_panel_visible` | `bool` | `false` | File browser panel open |
| `plan_panel_visible` | `bool` | `false` | Plan panel open |
| `git_panel_visible` | `bool` | `false` | Git panel open |
| `outline_panel_visible` | `bool` | `false` | Outline panel open |
| `references_panel_visible` | `bool` | `false` | References panel open |
| `ai_chat_panel_visible` | `bool` | `false` | AI chat panel open |
| `file_browser_view_mode` | `String` | `"tree"` | File browser listing: `flat` or `tree` |
| `sidebar_density` | `String` | `"auto"` | Sidebar layout: `auto` (rich for a short list or a finger), `compact` or `rich` |
| `mobile_theme` | `String` | `"commander"` | Mobile PWA appearance (`commander` or `vscode-light`), separate from the desktop theme |
| `diff_panel_width` | `u32` | `400` | Diff panel width in pixels |
| `markdown_panel_width` | `u32` | `400` | Markdown panel width in pixels |
| `notes_panel_width` | `u32` | `350` | Notes panel width in pixels |
| `plan_panel_width` | `u32` | `350` | Plan panel width in pixels |
| `git_panel_width` | `u32` | `380` | Git panel width in pixels |
| `settings_nav_width` | `u32` | `180` | Settings nav column width in pixels |
| `settings_expert_mode` | `bool` | `false` | Settings "expert mode": off hides an expert control that is still at its default; on shows every control |
| `diff_view_mode` | `String` | `"split"` | Diff viewer: `split` or `unified` |
| `detached_panels` | `HashMap<String, String>` | `{}` | Panel id to detached window label |
| `github_section_collapsed` | `HashMap<String, bool>` | `{}` | Collapsed GitHub sections (`my-prs`, `prs`, `issues`); absent key means the section's own default |

The eight `*_panel_visible` flags for markdown, file browser, git, outline,
references, AI chat, AI triage and notes are **mutually exclusive** — the
frontend opens one and closes the rest. The backend does not enforce that; it
stores whatever it is sent.

Every key the frontend sends must be declared here. Serde has no
`deny_unknown_fields` on this struct, so an undeclared key is dropped on the
way in without an error and `load_ui_prefs` can never return it. The panel
then looks like it saves and silently fails to survive a restart.

**Commands:** `load_ui_prefs()`, `save_ui_prefs(base, config)`

### Repository Settings (`repo-settings.json`)

**Type:** `RepoSettingsMap` (HashMap of `RepoSettingsEntry`)

Per-repository fields:

| Field | Type | Default | Description |
|-------|------|---------|-------------|
| `path` | `String` | -- | Repository path |
| `display_name` | `String` | -- | Display name |
| `base_branch` | `String` | `"main"` | Base branch for worktrees |
| `copy_ignored_files` | `bool` | `false` | Copy .gitignored files to worktree |
| `copy_untracked_files` | `bool` | `false` | Copy untracked files to worktree |
| `setup_script` | `String` | `""` | Script to run after worktree creation |
| `run_script` | `String` | `""` | Default run command |
| `auto_fetch_interval_minutes` | `u32` | `0` | Auto-fetch interval in minutes (0 = disabled) |
| `auto_delete_on_pr_close` | `AutoDeleteOnPrClose` | `"off"` | Auto-delete branch when PR merged/closed (`off`/`ask`/`auto`) |
| `archive_script` | `String` | `""` | Script to run before archive/delete (non-zero exit blocks) |
| `dev_server_url` | `Option<String>` | `None` | URL opened by Design Mode for this repository; when unset, opens `about:blank`. Stored locally and excluded from `.tuic.json` and repository defaults |

**Commands:** `load_repo_settings()`, `save_repo_settings(base, config)`, `check_has_custom_settings(path)`

### Repository Defaults (`repo-defaults.json`)

**Type:** `RepoDefaultsConfig`

Default values applied to new repositories when no per-repo override exists.

| Field | Type | Default | Description |
|-------|------|---------|-------------|
| `base_branch` | `String` | `"automatic"` | Default base branch |
| `copy_ignored_files` | `bool` | `false` | Copy .gitignored files to worktree |
| `copy_untracked_files` | `bool` | `false` | Copy untracked files to worktree |
| `setup_script` | `String` | `""` | Default setup script |
| `run_script` | `String` | `""` | Default run command |
| `archive_script` | `String` | `""` | Default archive script |
| `orphan_cleanup_countdown_seconds` | `u32` | `10` | Ask-dialog countdown for safe orphan worktrees; Expert Mode offers 5, 10, 20, or 30 seconds |

**Commands:** `load_repo_defaults()`, `save_repo_defaults(base, config)`

### Repositories (`repositories.json`)

Explicit server registration (`repo action=add`, also used by CLI directory opens)
uses a strict locked read-modify-write. It adds only the canonical repository row,
its initial workspace and order entry, and selects it. Existing rows and unrelated
configuration survive repeated calls. Corrupt configuration aborts registration
and is moved to the existing `repositories.corrupt-<uuid>` recovery backup. Successful changes emit
`repositories-changed` to both transports.


**Type:** `serde_json::Value` (flexible persisted JSON, shape defined by frontend)

Stored in the shared config directory like every other file (see Config
Directory) — debug and release builds read and write the same
`repositories.json`. Every write uses a versioned delta inside the existing
`save_repositories(config)` argument:

```json
{
  "mutationVersion": 1,
  "repos": [{ "id": "/repo", "before": {}, "after": {} }],
  "groups": [{ "id": "group-id", "before": null, "after": {} }],
  "repoOrder": { "before": [], "after": ["/repo"] },
  "activeRepoPath": { "before": null, "after": "/repo" },
  "groupOrder": { "before": [], "after": ["group-id"] }
}
```

`before` is the last value that client successfully loaded or persisted;
`after` is its intended value, and `null` in an ID-keyed mutation means absence
or deletion. The backend acquires the cross-process lock, strictly reloads the
latest document, and applies each repository/group mutation by ID. Mutations
to different IDs compose. Independent membership additions/removals in
`repoOrder` and `groupOrder` are three-way merged; incompatible reorders
conflict. Active-repository changes use the same `before`/`after` check.

A stale mutation of the same repository, group, active selection, or order is
rejected with a deterministic conflict instead of overwriting the newer value.

**Derived branch fields are exempt from that check.** `additions`, `deletions`,
`isMerged`, `lastActiveTerminal`, `lastCommitTs` and `lifecycleStatus`
(`DERIVED_BRANCH_FIELDS` in `config.rs`) are a cache each client recomputes from
the repository itself, on
its own refresh cadence — two windows legitimately hold two different values at
the same instant, so comparing them turns every save into a conflict. Measured
2026-08-31: `ego` moved 331 → 357 additions in 70 seconds while 29 consecutive
saves were rejected, and unrelated intent — registering a repository — was
wedged behind a number nobody edited. The conflict check compares records with
those fields stripped; the "already applied" check stays exact, so a
derived-only update still persists and the cache keeps moving. Everything a
human sets (name, order, grouping, active branch) is still fully guarded, and
concurrent edits to the derived fields themselves resolve last-writer-wins.
IPC reports that error to the frontend, where it creates a user-visible Errors
badge; HTTP returns `409 Conflict`. Malformed deltas return HTTP `400`. A
versioned delta is required; unversioned whole-document bodies are rejected.

**A successful write is announced, so the other clients converge instead of
colliding.** One backend serves the desktop WebView, the browser and the PWA at
once, and each keeps its own `before` baseline. Until this event existed, a save
told the other clients nothing: their baseline stayed stale until a conflict
taught them otherwise, which is late — by then the two documents have already
diverged. Both save paths (`save_repositories` over IPC, `PUT
/config/repositories` over HTTP) now call `AppState::notify_repositories_changed()`,
which dual-emits the payload-free `repositories-changed` event to the desktop
window and onto the `/events` SSE bus.

It fires **only when the document actually moved**:
`save_repositories_request` returns `Ok(true)` for a write and `Ok(false)` for a
delta already applied, so a no-op save does not wake every client. The payload is
empty by design — a receiver only needs "disk moved, re-read it", and shipping
the document would copy the whole repository set to everyone on every save.

On the receiving side `repositoriesStore` re-reads `repositories.json` and adopts
a key **only when it has no unsaved intent for it** — when its live value still
equals its baseline. Adopted keys move in the store *and* in the baseline
together: those two are diffed against each other on every save, so refreshing
one alone would make the next diff revert what the other client just wrote. Two
keys are deliberately never adopted: `activeRepoPath`, because which repo a
window is looking at is per-window and a background event must not move the
user's focus, and a repository whose disk record disappeared while it still holds
open terminals, because dropping it would orphan panes the user is looking at.
Keys this client did change are left untouched and still resolve through the
compare-and-swap rebase.

Four details make that gate hold up in practice:

- **The "unsaved intent" test runs on an intent view, not the whole record.**
  `repositoriesStore` mirrors the backend's `DERIVED_BRANCH_FIELDS` (plus
  `hadTerminals`, which is session state that happens to be persisted) and
  normalizes both sides through the same migration pass `hydrate` applies. Without
  the first, a repo under active work drifts from its own baseline every few
  seconds via `updateBranchStats`, which saves nothing — and adoption would be
  refused for exactly the repos the user is working in. Without the second, a
  baseline read off disk *before* the migration defaults were added never matches
  the migrated store, and a record written by an older build is never adopted.
- **The live-terminal rule applies per branch, not only per repository.** A repo
  record that survives on disk with one branch removed goes through the merge path,
  where the repo-level guard never runs; a branch this window still has a pane in is
  kept there instead.
- **Every repo the store holds stays in `repoOrder`.** The order arrives from the
  client that *did* drop the repo, so a record kept for its live terminals would
  otherwise leave the sidebar while its panes keep running. Grouping is an overlay:
  `getGroupedLayout` filters grouped paths out of `repoOrder` at render time, so
  putting a repo back there is always safe.
- **Adoption waits for an in-flight save.** A save ends by assigning the baseline
  it computed before it was sent, and nothing orders the broadcast against that
  save's own reply — so an adoption landing inside the window would have its
  baseline overwritten while its store changes stay, which is the one state this
  whole path exists to prevent.

**Version skew is the dangerous case, and it is one-sided.** A backend from
before the delta protocol does not decode `config` at all — it stores it as the
whole document, so `repositories.json` becomes the delta envelope and every
repository is lost. This is not theoretical: `make dev` never hot-reloads Rust,
so a hot-reloaded frontend meeting a stale backend did exactly that on
2026-08-21. The old binary cannot be fixed retroactively, so the guard lives at
the read end: `repositoriesStore.hydrate()` treats a root `mutationVersion`, or
`repos` as an array, as a poisoned file — it logs a user-visible error, refuses
to hydrate, and leaves `hydrated` false so **no save can run**. That last part is
what makes a hand-restored backup stick; without it, the running app clobbers the
restored file within seconds.

`repositories.json` used to be the one file exempt from the (then-real)
debug/release split: it was seeded into a separate `~/.tuicommander-dev/`
directory on first debug run so a dev instance wouldn't start with an empty
repo list. That seeding path is gone now that both builds share one
directory for `repositories.json` and all other config domains covered by
this document. (`~/.tuicommander-dev/` itself still exists for an unrelated
purpose — see `crates/tuic-core/src/credentials.rs`'s debug-only credential store.)

**Commands:** `load_repositories()`, `save_repositories(config)`

#### Stale-temp repository repair (#763-d219)

Live evidence: 15 hydrated `repositories.json` rows pointed at paths under macOS
temp roots or `$HOME/Gits/.tmp` that no longer existed on disk — throwaway shell
repos a prior session or agent worktree run created and never cleaned up. Each
was non-git, held exactly one shell-only workspace, and carried no terminals,
saved terminals, diffstat, commit, parent, or user metadata: a maximally
"empty" record, which is exactly why a missing path alone is not sufficient
evidence — a legitimate repository on an unmounted drive or a machine the user
switched away from looks identical on that one axis.

**Classifier — `classify_stale_temp_repo` (`config.rs`), ALL of:**

1. `std::fs::metadata(path)` returns `NotFound` — the local path is proven not
   to exist. Permission and other I/O errors preserve the row because they do
   not prove absence.
2. The path falls under a recognized temp root (`recognized_temp_roots()`:
   `std::env::temp_dir()`, `/tmp`, `/private/tmp`, `/var/folders`,
   `/private/var/folders`, `$HOME/Gits/.tmp`).
3. `isGitRepo` is explicitly `false` (never merely absent or `true`).
4. Exactly one workspace, and it is shell-only: no live or saved terminals, no
   last-active terminal, no diffstat/commit/merge state, no parent, no run
   command, no CI auto-heal.
5. No user metadata on the repo record itself: `collapsed`/`parked` are their
   defaults (`false`), and no `connectionId`/non-empty `initials`.

Any single failing check leaves the record untouched — this is an ALL-of test,
not a heuristic score, so a legitimate offline/unmounted/renamed repository
always survives.

**Commands** (IPC + `GET`/`POST /config/repositories/stale-temp` HTTP twin):

- `list_stale_temp_repository_candidates()` — read-only preview; re-classifies
  the on-disk document fresh on every call, never a cached list.
- `repair_stale_temp_repositories(paths)` — the only mutating path, and the
  only place implicit or global deletion is refused: it takes the exact paths
  the user confirmed and re-validates every one of them against the classifier
  using the document as it stands on disk *inside the same file lock* as the
  write. If even one no longer classifies (reconnected, edited since the
  preview, or never stale to begin with), the **entire** request is rejected
  before anything is written — one versioned transactional delta, not a
  best-effort sweep. On success it writes an exact pre-repair backup to
  `repositories.repair-backup-<UTC timestamp>.json` in the config directory,
  then removes the validated rows from `repos`, `repoOrder`, every group's
  `repoOrder`, and clears `activeRepoPath` if it pointed at a removed row — all
  inside `ConfigFile::update_with_strict`'s file lock, then calls
  `notify_repositories_changed()` like `save_repositories`.

**Why there is no separate restore-on-failure path:** `ConfigFile::write_atomic`
(temp file + fsync + rename) already guarantees a failed write never partially
overwrites the live document — the original is simply untouched. The backup
file is therefore a *human* recovery artifact for undoing a repair that
succeeded but was unwanted, not a mechanism this code needs to invoke itself on
a write failure. Recorded as a deliberate design trade-off in story
763-d219's worklog rather than layering a second, redundant recovery path on
top of a write that is already atomic.

**Frontend:** classified candidates are hidden from the sidebar's normal repo
list (quarantined, never silently deleted) pending an explicit repair; see
`repositoriesStore` (`refreshStaleTempCandidates`/`repairStaleTemp`,
`src/stores/repositories.ts`) and the discoverable repair entry point — a red
flagged-repo icon with a count badge in the sidebar footer, opening
`StaleTempRepairPopover` (`src/components/Sidebar/StaleTempRepairPopover.tsx`),
next to the existing parked-repositories popover.

### Prompt Library (`prompt-library.json`)

**Type:** `PromptLibraryConfig`

```rust
struct PromptEntry {
    id: String,
    label: String,
    text: String,
    pinned: bool,
}
```

**Commands:** `load_prompt_library()`, `save_prompt_library(base, config)`

### Notes (`notes.json`)

**Type:** `serde_json::Value` (flexible JSON, shape defined by frontend)

**Commands:** `load_notes()`, `save_notes(base, config)`

### Keybindings (`keybindings.json`)

**Type:** `serde_json::Value` (flexible JSON, shape defined by frontend)

Custom keyboard shortcut overrides.

**Commands:** `load_keybindings()`, `save_keybindings(base, config)`

### Agents Config (`agents.json`)

The `ego` entry supports optional `ego_mode` (`plan|default|edits|auto|yolo`) and `ego_sandbox` (`ro|workspace`). Missing values add no flags. The shared Rust launch translator injects `--mode` and `--sandbox` for terminal and MCP launches (including `run` and `resume`), replacing corresponding raw overrides before `--`. Administrative subcommands (including `mcp-server`) and ACP are excluded. The prompt separator `--` and its full suffix stay unchanged, even after a raw option without a value. An invalid saved ego choice is ignored for that field only; the valid sibling choice and other agents' settings remain intact. These choices apply only to new launches and do not rewrite ego configuration.

Each agent entry may contain `native_status_signals: boolean`. For Claude and Codex, an absent value means `true`; `false` disables launch argument injection. `hook_instrumentation` controls only explicit global installation and remains off when absent.

Each agent entry may also contain `prevent_alt_screen: boolean`. An absent value means `true`. When true, TUIC uses a verified control where one exists: Claude's environment variable, Codex and Grok's `--no-alt-screen`, or OpenCode's `--mini`. A false value suppresses TUIC's screen control for that agent on new structured and shell launches. An agent without a verified control remains unaffected.

Claude and Codex entries may contain `skip_trust_dialog: boolean`. An absent value means `true`. The setting applies only to MCP `agent spawn`: Codex receives a launch-only project trust override for the canonical working directory, including through custom launchers that forward arguments, while Claude's first exact startup picker is answered through the managed PTY. A false value leaves the CLI's normal trust question in place. TUICommander does not modify either CLI's saved trust file.

**Type:** `AgentsConfig`

Per-agent run configurations (custom commands, arguments, model, environment variables).

Loading `agents.json` performs a locked, one-time Codex migration. A missing
Codex configuration gains an explicit default; an existing direct Codex default
(or the first configuration if none is marked default) gains
`--dangerously-bypass-approvals-and-sandbox`. Other configurations and wrapper
arguments are preserved. The `codex-bypass-migrated` stamp in the same config
directory prevents later loads from restoring a flag the user removed, even
after an older backend rewrites `agents.json` and drops unknown fields. The
`codex_bypass_migrated` field remains a serialization-compatible mirror. MCP launch composition
does not add that argument; the configuration owns the choice.

```rust
struct AgentRunConfig {
    name: String,
    command: String,
    args: Vec<String>,
    model: Option<String>, // MCP spawn default; the spawn parameter overrides it
    env: HashMap<String, String>,
    is_default: bool,
}

struct AgentSettings {
    run_configs: Vec<AgentRunConfig>,
    codex_bypass_migrated: bool, // one-time launch-argument migration
    idle_close_minutes: u32, // default 15; 0 disables managed-child cleanup
    prevent_alt_screen: Option<bool>, // absent = true
    skip_trust_dialog: Option<bool>, // absent = true; MCP spawns only
}

struct AgentsConfig {
    agents: HashMap<String, AgentSettings>,
}
```

**Commands:** `load_agents_config()`, `save_agents_config(base, config)`

The optional `model` field adds `--model <value>` when MCP `agent spawn`
selects the run config. A model passed on the spawn call overrides it. Existing
`--model` entries in `args` remain unchanged; they still conflict with an
explicit spawn model. Set the model in Settings > Agents when a run config needs
an overrideable default.

`idle_close_minutes` is per agent type. Only orchestrator-spawned children use it;
user-created terminals are never candidates. The idle window restarts on input,
output, mail, or agent-state changes. Unread mail, background work, a running
`tuic bg` job, a failed, uncertain or retrying background wake, and a per-session keep-open
mark prevent closure.
The latest background-wake status for each session is stored atomically at
`<config_dir>/bg-wakes/<TUIC_SESSION>.json`; a `retrying`, `uncertain` or `failed` status
keeps that managed child open.

**This file belongs to a machine, not to the app.** Every backend reads its own copy, and
the frontend keeps one per machine: a tab opened on a repository registered against a
remote connection launches with that connection's `agents.json`, because the agent binary,
its paths and its config directory are on that box. Neither command carries a path or a
session, so the transport's argument-driven routing cannot place them — the connection is
named explicitly (`src/stores/agentConfigs.ts`, `agentConfigsFor` / `ensureAgentConfigs`).
The cache is dropped on every connection status edge and refilled when the machine comes
up. `set_agent_hook_instrumentation`, `set_agent_native_status_signals` and the upstream
MCP config follow the same rule; `install_agent_mcp`, `get_agent_config_path` and the
upstream OAuth flow do not, having no HTTP route and nothing to open a browser with.

If a machine fails to load `agents.json`, its frontend store records the error and stays
unloaded so the next read can retry. The error names the connection and `/config/agents`
endpoint. Saves are refused until a load succeeds, preventing an empty fallback from
overwriting that machine's config. A daemon that predates this route must be updated.

The families that stay local are the ones describing this app rather than a machine:
`config.json`, `keybindings.json`, the pane layout, the notification config and
`repositories.json` — the last definitionally so, since it is the list deciding which
repository maps to which machine.

The hands_free_spoken_replies boolean defaults to true and persists on the server device. Turning it off cancels the current speech queue and keeps dictation armed.

### Dictation Config (`dictation-config.json`)

**Type:** `DictationConfig`

Defined in the root `src-tauri/src/config.rs` and re-exported by the dictation command adapter. It is desktop-gated; the audio and speech domain lives in `tuic-dictation`.

| Field | Type | Default | Description |
|-------|------|---------|-------------|
| `enabled` | `bool` | `false` | Dictation enabled |
| `hotkey` | `String` | `"F5"` | Push-to-talk hotkey |
| `language` | `String` | `"auto"` | Transcription language |
| `model` | `String` | `"large-v3-turbo"` | Whisper model name |
| `auto_send` | `bool` | `true` | Auto-submit after transcription |

**Commands:** `get_dictation_config()`, `set_dictation_config(base, config)`

### Config Defaults (read-only, no file)

**Type:** `ConfigDefaults`

Backs Settings "expert mode" (SPEC.md → Settings navigation): an expert
control compares its live value against the matching default here to decide
whether it is hidden (at default, basic mode) or shown (modified, or expert
mode on). Every field is that domain's own `Default::default()` — the exact
value `load_json_config` falls back to when the file is missing — never a
second, hand-copied literal. A dedicated test
(`config::tests::get_config_defaults_returns_each_domains_own_default`) fails
if the command ever diverges from that source, and a second one
(`assert_no_field_default_drift_except`) fails if a struct's own
`#[serde(default = ...)]` attributes drift from its `impl Default` — with a
documented exception list for fields that are deliberately asymmetric, e.g.
`mcp_server_enabled` (`true` for a brand-new install, `false` for an existing
config.json written before the field existed — see
`app_config_serde_default_for_new_fields`).

| Field | Type | Description |
|-------|------|-------------|
| `app` | `AppConfig` | `AppConfig::default()` |
| `notifications` | `NotificationConfig` | `NotificationConfig::default()` |
| `agent_settings` | `AgentSettings` | Default for one entry of `AgentsConfig.agents` — there is no single default for the map itself |
| `repo_defaults` | `RepoDefaultsConfig` | `RepoDefaultsConfig::default()` — what a `repo-defaults.json`-less install loads |
| `agents` | `AgentsConfig` | `AgentsConfig::default()` — `AgentsConfig`-level fields such as `headless_agent` |
| `github_accounts` | `GitHubAccountRegistry` | `GitHubAccountRegistry::default()` — `{ "accounts": [] }`, the same shape as `github_accounts.json`; Settings shows the "Add another GitHub account" entry point in basic mode only when accounts exist |
| `dictation` | `DictationConfig` | Desktop builds only — absent under `--no-default-features` (`tuic-remote`), where `mod dictation` does not compile and the route is not registered |

Fields marked `skip_serializing_if = "Option::is_none"` are omitted when
`None` (`notifications.audio_device`, `agents.headless_agent`, several
`agent_settings` fields). The payload keeps that attribute because it shapes
the config files; `settingsExpert.ts` reads a leaf missing from a present
object as the default `null`.

**Command:** `get_config_defaults()`. HTTP: `GET /config/defaults` (see `docs/api/http-api.md`).

## Cache Files

### Claude Usage Cache (`claude-usage-cache.json`)

**Module:** `src-tauri/src/claude_usage.rs`

Persistent cache for incremental JSONL parsing of Claude session transcripts. Stored in the config directory. The cache maps `project_slug -> (filename -> CachedFileStats)` and tracks per-file byte offsets so only newly appended data is parsed on subsequent scans.

This is an internal cache file, not user-editable. It is automatically pruned when projects or session files are deleted.

The separate Anthropic rate-limit API cache is in memory only. Its responses and rate-limit backoff are keyed by the Claude session's credential profile directory, so switching `CLAUDE_CONFIG_DIR` cannot reuse another account's quota.

## Repo-Local Config (`.tuic.json`)

**Module:** `src-tauri/src/config.rs`

A `.tuic.json` file in the repository root provides team-shareable settings. It is read-only from the app — teams edit it directly in their repo and commit it.

The Design Mode `dev_server_url` is deliberately absent from this format: a committed repository file cannot choose the browser destination on another user's machine. Set it in the repository's Scripts tab instead.

**Precedence chain:** `.tuic.json` > per-repo app settings (`repo-settings.json`) > global defaults (`repo-defaults.json`)

**Type:** `RepoLocalConfig` (all fields `Option<T>`, missing fields fall through to lower tiers)

| Field | Type | Description |
|-------|------|-------------|
| `ego_profile` | `String` | Ego profile for new ACP sessions in this workspace; bounded by the explicit machine `ego_profile`. Ego returns clamp warnings. Missing machine selection refuses the repo selection. |
| `base_branch` | `String` | Base branch for worktrees |
| `copy_ignored_files` | `bool` | Copy .gitignored files to worktree |
| `copy_untracked_files` | `bool` | Copy untracked files to worktree |
| `setup_script` | `String` | Script to run after worktree creation |
| `run_script` | `String` | Default run command |
| `archive_script` | `String` | Script to run before archive/delete |
| `worktree_storage` | `WorktreeStorage` | Storage strategy (sibling/app-dir/inside-repo) |
| `delete_branch_on_remove` | `bool` | Delete branch when removing worktree |
| `auto_archive_merged` | `bool` | Auto-archive merged worktrees |
| `orphan_cleanup` | `OrphanCleanup` | Orphan worktree handling |
| `pr_merge_strategy` | `MergeStrategy` | PR merge method preference |
| `after_merge` | `WorktreeAfterMerge` | Post-merge worktree action |
| `auto_delete_on_pr_close` | `AutoDeleteOnPrClose` | Auto-delete on PR close |

**Command:** `load_repo_local_config(repo_path)` — returns `RepoLocalConfig` or `null` if file is missing or malformed.

## Progress Storage (`progress.sqlite3`)

**Module:** `src-tauri/src/progress/` (`store.rs`, `ownership.rs`, `model.rs`,
`service.rs`, `flow.rs`)

One append-only journal in one database, `<config dir>/progress.sqlite3`, with
`project` and nullable `pty_id` columns. Existing databases gain `pty_id` on
open; old rows keep `NULL` because their PTY cannot be reconstructed. A
project-wide query includes every PTY and the unattributed rows; a PTY query
selects one source. Nothing is written inside a repository — no `.tuic`
directory, no `.git/info/exclude` registration, no export lock, and so nothing
for the repository watcher or the content index to ignore.
Separate `pty_views` and `project_views` tables keep last-visit marks for each
PTY and the repository aggregate.
`repo_watcher.rs` asserts that: it snapshots the repository tree byte-for-byte
around a record/delete/mark-viewed cycle and requires it unchanged.

Five entry kinds. `done` and `blocked` are reported by agents through the MCP
`progress` tool, the HTTP routes or the Tauri commands. `intent` is written by
TUIC from the agent's `intent:` marker. `delegated` and `message` are written by
TUIC at `agent action=spawn` and `agent action=send`, with nullable
`target_pty_id` and `target_name` columns naming the other terminal. All three
host-written kinds are refused on every reporting path — they are observed, not
claimed.
Every journal insert redacts secret-shaped text and step fields before SQLite
stores them. Agent and target display names are also redacted and capped at 80
characters. Reads therefore return the stored redacted values in both the list
and Flow view.

A journal created before the hand-off kinds has a `CHECK` constraint that
refuses them. SQLite cannot alter a `CHECK`, so opening such a database copies
`entries` into a new table in one `IMMEDIATE` transaction. Ids are copied as
they are, and the high-water mark is carried over so a deleted newest id is
never issued again. The mark is the larger of the `sqlite_sequence` value and
the highest id present: the earliest journals used a bare `INTEGER PRIMARY KEY`
and have no `sqlite_sequence` table at all.

`list` skips a row whose kind this build does not know, with one warning,
instead of failing the whole read: debug and release builds share this file, so
a newer build's kind must not blank an older build's list. The repeated-`intent:`
check ignores `delegated` and `message` rows, so a hand-off between two
repaints of the same intent does not record it twice.

A terminal's project is its cwd's registered repository or, for a managed
worktree outside the repository root, its registered workspace, which then
resolves to the parent project.

Ownership resolution starts from an authoritative registered project and follows
recorded linked and nested workspace parent records, so a worktree's entries land
in its parent project's journal. It never uses the focused UI repository or a
bare CWD; unbound callers fail with `project_required`.

The schema is one `entries` table (rowid identity, ordering and cursor in one,
`AUTOINCREMENT` so a deleted id is never handed out again) plus a `project_views`
table holding the per-project last-visit timestamp. There is no revision, no
sequence, no collection state and no correction history: an entry is appended
once and either kept or deleted. `list` returns the newest 500 entries for one
project, newest first.

Each operation opens a fresh SQLite connection in WAL mode with a five-second
busy timeout, leaving SQLite locking as the cross-thread and cross-process
serialization boundary.

Collection is gated by `progress_tracking`: the global `AppConfig` flag ANDed
with the per-agent `AgentSettings` override, which defaults to on. Global off
also removes the `progress` tool from every agent's tool list; a per-agent off
answers `progress_tracking_disabled` instead, because the tool index is built
once and has no session context.

## Native Story Storage (`stories.sqlite3`)

**Module:** `src-tauri/src/stories/` (`model.rs`, `store.rs`, `store/records.rs`,
`store/transitions.rs`)

Native plans and stories use `<config dir>/stories.sqlite3` on the machine that
owns the project. The plan row links a project path to a source document; its
prose remains in that document. Story rows contain criteria, checked state,
dependencies, priority, origin, declared file scope, status, revision and an
optional manual session claim. No story database files are written to a
repository, and there is no import or export path.

Each story action opens one SQLite connection in WAL mode with a five-second busy
timeout and reuses it for the action's reads and transaction. Schema setup runs
once at open. New databases are created at SQLite `PRAGMA user_version = 1`.
Version 0 is upgraded in place because its schema is compatible with version 1;
a database with a newer version is rejected rather than opened with an unknown
schema. Mutations use an immediate transaction and an expected story revision.
Dependency cycles, cross-plan dependencies and concurrent claims are rejected.
A story becomes ready when all dependencies are done. Plan state is derived from
stored story statuses rather than stored separately or decoded a second time
after a story list. A manual claim is released when its
terminal closes.

## Workflow Definition Storage (`workflows.sqlite3`)

**Module:** `src-tauri/src/workflows/` (`definition.rs`, `store.rs`, `api.rs`)

Project-scoped workflow drafts and immutable published revisions use `<config dir>/workflows.sqlite3` on the owning machine. Draft edits use an expected revision. Publication validates graph structure and pinned story-template references in an immediate SQLite transaction. The two built-in templates are seeded atomically once per project. This database holds definitions only; run events and attempts belong to the workflow runtime.

## Workflow Run Storage (`workflow_runs.sqlite3`)

**Module:** `src-tauri/src/workflows/run/`

Plan runs, their sequenced event history, idempotent command receipts, node attempts, story executions, and external effect intents use `<config dir>/workflow_runs.sqlite3` on the owning machine. WAL and immediate write transactions keep each event and its projections together. The database is independent of terminal sessions and draft definitions; active runs pin published definition revisions.

## Additional Commands

| Command | Module | Description |
|---------|--------|-------------|
| `hash_password(password)` | `lib.rs` | Bcrypt hash for remote access authentication |
| `list_markdown_files(path)` | `lib.rs` | List .md files in a directory |
| `read_file(path, file)` | `lib.rs` | Read a file's contents |
| `get_mcp_status()` | `lib.rs` | Get MCP server status (enabled, port, connected clients) |
| `clear_caches()` | `lib.rs` | Clear in-memory caches |
| `get_local_ip()` | `lib.rs` | Get primary local IP address |
| `get_local_ips()` | `lib.rs` | List all local network interfaces |
| `get_claude_usage_api()` | `claude_usage.rs` | Fetch rate-limit usage from Anthropic OAuth API |
| `get_claude_usage_timeline(scope, days?)` | `claude_usage.rs` | Get hourly token usage timeline from session transcripts |
| `get_claude_session_stats(scope)` | `claude_usage.rs` | Scan JSONL transcripts for aggregated token/session stats |
| `get_claude_project_list()` | `claude_usage.rs` | List Claude project slugs with session counts |
| `fetch_plugin_registry()` | `registry.rs` | Fetch remote plugin registry index |

### Workflow recovery boundaries

Runtime reconciliation refreshes integrated dependency projections without interrupting live attempts or marking their in-flight effects uncertain. The first workflow store open after a process restart uses a separate recovery path that interrupts old attempts and marks intended effects uncertain. A failed run is logged so other runs can recover, and its identifier remains pending for recovery on a later open or runtime reconciliation. Only runs captured at the first open are eligible for restart recovery; new live runs are never swept into retries. Startup dependency refresh still invokes Git; moving those probes outside write transactions requires the freshness contract tracked in story 959-c69c.

### Telegram setup files

Telegram Settings uses `~/.config/tuic-telegram/`: `bot.token`, `allowed_chat_ids` (positive private-chat IDs, one per line), and `config.json` (`enabled`, `bot_alias`). TUIC creates the directory as 0700 and files as 0600 on Unix, refuses secret-file symlinks, and atomically replaces files. A separate `setup.lock` serializes cross-process setup and pairing writes. No token is serialized by the read API.

`pairing.json` holds the one-use six-character code and its absolute ten-minute expiry under the same private permissions, so desktop setup and daemon polling share the authorization credential. A valid private-chat update consumes it; a wrong/expired code or `/start` grants nothing. An explicitly empty allowlist permits polling for pairing, but no outbound sends. A missing or malformed allowlist still fails closed. Enable/target/token changes restart the single daemon adapter; desktop never polls. Status keeps only connectivity, an error category and the last accepted message timestamp.

`status.json` shares safe daemon connectivity/error/timestamps with desktop Settings on the same host. A connection record older than one minute is shown as disconnected. The file contains no token or message text.

### Per-conversation AI Chat launch authority

`ai_chat_launches` maps ego session ids to `{ executable, profile, workspace, peerId }` objects (camelCase). Creation snapshots the selected overrides and defaults; reopen uses that saved object, including the peer identity. Writes use the locked configuration delta path. The global `ego_executable`, `ego_profile`, and `ai_chat_workspace` settings are unchanged.

Repository records may contain backend-authored `declaredWorktrees`, keyed by
the stable `TUIC_SESSION`. Values contain `workspaceId`, `branch` and
`worktreePath`. Caller-bound MCP declarations update this map through the
repository delta under the existing cross-process lock. Frontend saves retain
it. Existing saved terminal records are moved to the declared workspace without
changing their actual shell cwd. The association survives backend restart; it
does not turn an externally created worktree into a disposable PTY-owned one.

## Automation Definitions

The Rust definition store (`automations/definitions.rs`) uses the selected
instance's `automations.json`. This is the storage foundation for the
[Automations scheduler plan](../../plans/automations-scheduler.md); scheduler
execution and public commands are not available in this step.

The version-1 document contains `version`, `max_concurrent_runs` (default `2`)
and a `definitions` array (default empty). Missing files return these defaults
without creating a JSON document. Reading can create its directory and `.lock`
file. Edits use `ConfigFile<T>::update_with_strict`: load fresh state under the
in-process and cross-process locks, mutate one id or the global limit, and
atomically persist. No-op edits do not rewrite the document.

Each definition stores:

| Field | Meaning |
|---|---|
| `id`, `name`, `prompt` | Stable id, display name and literal agent prompt |
| `run_config`, `repository` | Agent run-config name and repository workspace |
| `workspace` | `{ "mode": "existing" }` or `{ "mode": "new_per_run", "base_branch": "main" }` |
| `cron`, `timezone` | Five-field schedule text (empty for Once) and the per-automation IANA zone |
| `once_local` | Optional ISO local date-time, for example `2099-10-09T10:00:00`; mutually exclusive with nonempty cron |
| `enabled` | Whether scheduled admission is enabled |
| `grace_secs`, `max_duration_secs` | Positive grace and execution bounds in seconds |
| `overlap` | `"skip"`; session reuse and queued overlap are unsupported |
| `precheck` | Optional `{ "command": "...", "timeout_secs": 30 }` |

Required strings must not be blank. Limits must be positive. Duplicate ids,
identity changes, missing edit/delete targets and unsupported schema versions
return errors before writing. Unknown fields or malformed JSON cause the strict
ConfigFile loader to preserve the original bytes in `.corrupt-<uuid>` recovery
files and abort that operation. Semantic errors leave the original document in
place. No independent unattended-permission flag exists: the run config owns
permissions.

`automations/schedule.rs` validates exactly five Vixie fields, with day-of-month
and day-of-week OR semantics when both day fields are restricted. If either day
field starts with `*` (including `*/2`), both day fields must match. An explicit
range such as `1-31/2` remains restricted. Seconds, years, shorthand macros and Quartz
extensions are rejected. Named months/weekdays, ranges, lists and steps are
supported. Patterns with no possible calendar occurrence return a bounded error;
croner 4.0.1 limits its search iterations and years (through year 5000).

At creation an omitted (empty internal) timezone is resolved with the operating
system's local IANA zone and persisted. Failure to discover a supported zone is
an error, with no UTC fallback. Loads and edits require a valid stored zone;
they never reinterpret missing zones using the current host. Schedule evaluation
uses chrono-tz 0.10.4 and the stored zone. Next occurrences are strictly after
the supplied UTC instant; latest-due occurrences include that instant and must
be strictly after an optional scheduled cursor.

Fixed single wall-clock times skip spring gaps and run once, at the earlier
fall-fold instant. Hourly/wildcard intervals retain both real fold occurrences.
The wrapper filters croner's shifted gap results rather than dispatching them.
Backend hourly, daily, weekday and weekly presets validate their controls and
produce cron text; custom cron uses the same validator. Preview returns 1–20
UTC occurrences with the cron and zone. These are Rust core functions; public
HTTP/IPC commands arrive in plan Step 8. This core does not dispatch agents.

Once resolves its stored local date-time in the stored zone. Creation rejects
instants at or before now and nonexistent spring-gap times. An ambiguous fold
resolves to the earlier UTC instant. Loads retain elapsed Once definitions.
The Rust schedule wrapper returns the sole instant before consumption, then no
next/latest-due occurrence after the scheduled cursor reaches it. Preview returns
at most that one future instant. A consumed Once definition is completed, remains
stored, and can still be inspected; manual runs do not consume its schedule.
Ledger reservation and the scheduler's durable occurrence cursor prevent replay
on restart and catch-up. Public views use this backend state in plan Step 8.

### Automation run ledger

`automations/store.rs` stores execution history in the selected instance's
`automation_runs.sqlite3` (schema version 1). WAL and a five-second busy timeout
serialize reservations. A unique index on automation ID and UTC occurrence
prevents duplicate scheduled reservations; manual runs get separate UUIDs.
History stores a definition snapshot and survives definition deletion.

The runtime must acquire `RunOwner` before dispatch. Its adjacent `.owner.lock`
file stays on disk so every process locks the same inode. Reader handles cannot
reserve or mutate runs. Ownership acquisition interrupts all open records in one
transaction without retrying them. Reserved, prechecking, running and needs-you
are open; completed, failed, unknown, timed-out, interrupted and each skip reason
are final. The first final transition is immutable. Opening history does not
interrupt a live runtime. Unsupported schema versions fail closed.

Saved stdout, stderr and both precheck streams each retain at most 256 KiB at a
UTF-8 boundary with a truncation flag. History is paginated newest first. Summary
windows are elapsed UTC 24 hours and seven days, based on reservation time.
Retention defaults to 90 days after finalization and never removes open records.
Notification attempts are deduplicated by run, transition and channel and settle
as confirmed or unknown independently of execution. Boot marks outstanding
attempts unknown; it does not replay them. Saved output remains the canonical
report. Scheduler boot is wired on desktop and headless hosts. Public execution
transports remain pending; the existing MCP tool manages definitions only.

### Precheck execution foundation

`automations/precheck.rs` runs a configured precheck in the dispatcher's resolved
workspace through the same clean, non-interactive shell/environment policy as
Smart Prompts (`sh -c`, or `cmd /D /S /C` on Windows). Only exit code 0 admits
agent dispatch; nonzero exits, signals, timeouts and execution errors require
`skipped_precheck`. Spawn errors remain distinct from exit failures.

Each stream is drained independently and retained up to 256 KiB, with truncation
flags and elapsed milliseconds. The configured timeout starts after process
setup. Teardown kills the owned process group (Unix) or job (Windows), including
pipe-owning descendants; capture cleanup has a separate two-second bound and
reports incomplete capture as an error. Manual Run Now returns a persistable
`bypassed` outcome without spawning; no configured precheck returns
`not_configured`. Precheck output does not modify the prompt.

The shared automation runtime acquires the ledger owner lock on both desktop
and headless boot. It reconciles open runs as interrupted without retrying,
then admits only Once schedules on a 30-second tick. Recurring cron definitions
remain stored but are not dispatched in phase 1.

Dispatch atomically claims a reservation before workspace or agent effects,
then checks the definition again before the precheck and before launch. Deleted,
paused scheduled runs and changed definitions fail without launching. Manual
runs can use paused definitions and share the same overlap/concurrency limits.
Existing mode uses the repository; new-per-run mode uses the shared workspace
creator with the configured base branch, preserving workflow defaults.

The ledger saves `precheck_outcome` as `not_configured`, `bypassed`, or
`executed` with termination, capped stdout/stderr, truncation and duration.
Failed checks finalize as `skipped_precheck` without an agent. Successful checks
retain their evidence before launch; their stdout never changes the literal
prompt. The configured saved agent profile must exist; dispatch does not fall
back to another CLI. Workspace path/id and task/session ids are saved after
their effects, and errors finalize as failed. A child whose binding cannot be
saved is stopped through the managed session API. Completion and maximum-duration
handling remain a separate integration step; idle is not treated as success.

Automation definitions may contain `created_by_session`, the host-issued identity
of their creating agent. Older definitions omit it. Shared definition actions
ignore client-supplied creator provenance on creation and preserve the original
value on update. Pause and resume change only `enabled` under the definition lock.

### Automation transport ownership

HTTP `/automations/action` and IPC `automation_action` use the instance-scoped
`automations.json` and `automation_runs.sqlite3` through one Rust API. Per-id
create/update/delete and enabled-only updates use the existing definition lock;
run snapshots remain immutable across later edits and definition deletion.
A reader can inspect history without owning execution. Run Now requires this
process's already-acquired runtime owner and never acquires a second owner or
forwards execution. The global concurrency default remains two. Summaries use
elapsed UTC `24h`/`7d` windows; previews and next-run values use stored IANA zones
and the durable scheduled cursor. No new configuration setting is required.

### Automation completion and deadlines

Runs persist their reservation deadline and dispatch start in the ledger. The
30-second runtime wake and PTY/progress events reconcile task, session and durable
progress records, including after broadcast lag. Idle alone leaves a run active;
reported completion or a known zero process exit confirms success. Failed tasks
and nonzero exits fail the run; missing or unverifiable completion becomes
`unknown`. A task completion with no known exit code does not prove success.

`needs_you` stays open and counts toward overlap, capacity and maximum duration.
The deadline includes dispatch and precheck time. Expiry stops only the session
bound to that run. Final output is bounded to 256 KiB; final states reject late
or duplicate evidence. Boot preserves final history and interrupts open runs
without retry. `automation-run-changed` is dual-emitted to desktop and SSE with
an identical `{ "run": ... }` payload, including failure and needs-you transitions.
