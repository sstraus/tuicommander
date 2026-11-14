# macOS updater extraction diagnostics (2724-31ea)

## Observations and remaining uncertainty

The v2.0.0 arm64 asset is 49,143,004 bytes, SHA256
`d54d8c606e44167f45904ade16115256fbc952dd99a123a181dc068c1a4c36f0`.
It has 18 entries, no links or sparse/PAX entries; the main executable is
118,106,736 bytes. Both system tar and a native harness copying updater 2.14.1's
`tempdir_in`/`entry.unpack` sequence extract it successfully on the coordinator
Mac. The harness pins tar 0.4.46, flate2 1.1.10 and tempfile 3.27.0.

The colleague's failure occurred under a different macOS temporary path and that
machine reportedly has 50 GB free. Its underlying filesystem error has not yet
been captured. No conclusion about its cause or successful repaired upgrade is
justified by the successful extraction here. The coordinator subsequently reported
that the colleague installed 2.0.0 successfully after running the probe kit. This
suggests a transient condition; the probe result and its original cause remain
unavailable, and no security-software attribution is established.

A separate 60 MiB HFS+ image reproduces the same unpack prefix with
`No space left on device (os error 28)`. This establishes the free-space
preflight regression, not the cause of the colleague's incident.

## Why the cause disappears

`tar::TarError` displays its contextual description only, retaining the IO error
as its source. The upstream updater serializes errors with `to_string()`, so the
cause can disappear before JavaScript receives it. Horizontal overflow in the
320 px dialog independently hides long paths. TUIC now owns the install call
using the plugin's public checked `Update` resource and walks the standard
`source()` chain: the plugin's transparent IO variant and `io::Error` forward
`Display` and `source()` to the wrapped tar error. The plugin continues
to download, verify and install; no dependency patch is applied.

The macOS preflight scans uncompressed entry lengths, rounds each entry to the
volume allocation unit and reserves 1 MiB metadata headroom. It measures the
same-volume temporary root selected by updater 2.14.1 before installation.
This is an estimate and a point-in-time check: later filesystem failures still
need the complete cause. The current app volume is derived from the running
executable, matching this application's default plugin configuration.

## Fixture binaries

`tuic-acp-fixture-agent` and `tuic-mcp-fixture-server` are test subprocesses and
have no release runtime purpose. Tauri CLI 2.12.1 selects Cargo binary targets
unless their `required-features` are disabled; omitting them from `externalBin`
is insufficient. A nondefault `test-fixtures` feature now gates both. Nextest's
fixture setup builds them explicitly, including for integration consumers.

The packaging regression invokes the real installed Tauri bundler against the
actual manifest binary declarations with pre-existing fixture executables.
Default bundles must omit both; opting into `test-fixtures` must include both.
It uses substitute executables and does not claim to prove a production signed
bundle or an in-app upgrade.

## Upstream references

[plugins-workspace #2455](https://github.com/tauri-apps/plugins-workspace/issues/2455)
concerns temporary directory/admin and AppleDouble extraction failures;
[tauri #12799](https://github.com/tauri-apps/tauri/issues/12799) is related.
Neither establishes this incident's cause.

Evidence logs live under `~/Gits/.tmp/results/2724-*`; browser screenshots and
measurements are under this checkout's `.tmp/update-error/`. The colleague probe
kit is `~/Gits/.tmp/2724-probe.zip`. No daily application was installed or restarted.
