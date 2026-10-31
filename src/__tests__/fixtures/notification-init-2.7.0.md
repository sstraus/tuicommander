# Native notification initialization fixture

`notification-init-2.7.0.txt` is an unmodified copy of
`tauri-plugin-notification` 2.7.0 `src/init-iife.js` from the Cargo registry.
The package is the version recorded in this checkout's Cargo.lock.
Upstream: https://github.com/tauri-apps/plugins-workspace/tree/v2/plugins/notification
License: Apache-2.0 OR MIT (the upstream package dual license).

The test substitutes the upstream `__TEMPLATE_windows__` target template with
`false` to exercise the real macOS/Linux initialization command. It does not
mock the injected script's command selection. Its command must be authorized
by the capability that applies to the native window.
