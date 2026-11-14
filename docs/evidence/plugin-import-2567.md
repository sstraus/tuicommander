# User-plugin import recovery — story 2567-ee9c

## Observed incident and attribution limits

The assignment and existing story worklog report all 12 user-plugin imports
failing three times at backend timestamp `1793450061092`
(`2026-10-31T12:34:21.092Z`). The earlier investigation recorded successful loads
between `1793450062029` and `1793450062085`, without intervening plugin-change
messages. These are retained investigation statements, not a new runtime capture.
They do not prove why the imports failed or whether the same document recovered.

On 2026-11-01, read-only searches of the default instance's rotated files in
`~/Library/Application Support/com.tuic.commander/logs/` found no
`Failed to import plugin`, `Loaded plugin` or `loadCycleId` entries. The retained
files covered October 26 through November 1. There was no
`~/.tuicommander-dev/logs/` directory. No credentials were read, and the running
app was not attached to, reloaded or restarted.

Source explains the evidence limit: `push_log` in
`src-tauri/src/app_logger.rs` inserts frontend logs into `state.log_buffer`.
It does not emit a tracing event to the rotated file subscriber. The original
ring entries and any later provenance fields were therefore not recovered from
these files. The historical cause remains **unattributed**.

## Audited load and serving paths

- `usePluginRuntime` calls `initPlugins` at mount. `initPlugins` synchronizes the
  disabled list and calls `loadUserPlugins(false)`.
- Desktop discovery calls `list_user_plugins`; enabled manifests enter
  `loadPlugin`. Browser/PWA mode skips external plugin imports.
- Before this fix, native `import()` rejection logged an error, updated the
  plugin store, and returned. There was no retry or readiness subscription.
- macOS/Linux import `plugin://<id>/~v<number>/<main>`; Windows imports
  `http://plugin.localhost/<id>/~v<number>/<main>`.
- `register_plugin_protocol` is applied to the Tauri builder before window
  creation. It strips the numeric version segment and reads the file directly
  from the plugin directory. Success returns the asset MIME type and CORS header;
  invalid paths return 400 and file-read failures return 404. The URL does not
  wait for the application's HTTP server. No retained response proves that any
  of these statuses caused the historical incident.

## Bounded recovery policy

Each load has at most three import attempts: immediately, after 1 second, and
then after another 3 seconds. Total scheduled delay is 4 seconds, plus native
import durations. There is no timeout or cancellation of a pending native import.
Each attempt gets a new monotonic numeric path version, including when the clock
does not advance or moves backwards. Relative modules inherit that version.

Only import rejection is retried. A valid module proceeds to normal registration
once; validation and activation failures stop without retry. A successful
registration clears the store error. After three rejected imports, the enabled
plugin stays unloaded with the last error visible. A disable or replacement load
retires stale results before registration. Native module evaluation itself cannot
be undone, so plugins must avoid top-level side effects.

Every rejection retains URL, error details, known status or null, document ID,
load-cycle ID and attempt number. The message includes both cycle and attempt so
application-log coalescing cannot discard later diagnostic data.

## Validation scope

The targeted `src/__tests__/plugins/pluginLoader.test.ts` tests drive the real
startup/reload/enable entry points. The module-import adapter controls rejection
and success to test host recovery policy; registry registration is the activation
boundary. These are unit tests, not a captured WebView transport fixture or proof
of a historical transport cause. Native Node custom-scheme rejection supplies
the existing diagnostic test's error oracle. No desktop, Rust or full-suite
validation is claimed.
