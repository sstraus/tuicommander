# L3 dependency audit — 2026-10-04

Source before: 9996c4d2a4ee6357dd17fe549154d2feda1f096b, fix/w3-buildgraph.

## Unused direct dependencies

`cargo-machete --with-metadata` found root cron, gix, jsonc-parser, keyring, notify-rust; tuic-terminal vte; tuic-ipc dev tuic-test-support. All removed after source inspection. The extracted domain owners retain gix/jsonc-parser/keyring, and terminal code uses alacritty_terminal::vte. cc is used by build.rs for native notifications and retained alongside tauri-build in the machete ignore metadata.

The metadata-backed audit exits 1 because excluded patched crates cannot be treated as independent workspace packages. Its findings for actual workspace members are usable. A raw non-metadata scan reports chromey falsely: the package exports chromiumoxide, which design_mode/browser.rs uses. Patched serde remains an optional public feature; patched rustls remains the provider feature edge. Neither false positive is removed.

## Features and graph

Root default enables only desktop. It adds desktop rendering, native notifications/dialogs, notification playback, browser automation and source maps. These are needed for a desktop dev build, so they remain. Plain root builds do not select the dictation domain. Tauri dev/release builds request dictation through config; desktop CI requests it explicitly. Workspace tests select the dictation crate intentionally. cuda/vulkan imply dictation; tokio-console is non-default instrumentation.

Cargo normal/build graph: headless 511 packages; desktop 680; desktop+dictation 742. Each excludes aws-lc-rs/aws-lc-sys. Desktop and headless exclude whisper-rs/whisper-rs-sys/webrtc-audio-processing/webrtc-audio-processing-sys; dictation includes them. Exact default-feature crate delta is in desktop-added-crates.txt; opt-in native speech delta is in dictation-added-crates.txt. Full feature edges are in tree-before-features.txt and tree-after-features.txt.

`cargo tree --offline -d` duplicate-name count: 79 before, 71 after (default application graph). This differs from all-target Cargo metadata counts because metadata resolves target/optional packages too. No speculative upgrades: oauth2 still requests reqwest 0.12 while application/updater/chromey request 0.13; transitive major-version contracts are preserved. Removing provider/default feature edges drops AWS-LC and avoids its C build. Root HTTP2, charset and system-proxy defaults are retained explicitly.

## Verification limitations

Graph commands inspect feature resolution; they do not prove linking/runtime behavior. Exact matched-load fresh-worktree build-time comparison has not been performed. Prior native baseline from tuic-c-build was 253.01 s on an uncontrolled loaded Mac; it cannot be compared with rb or warm checks. Broad suites/mutation, native macOS/Windows checks, full release packaging and real audio hardware checks belong to the coordinator/Boss. Targeted compile/tests are recorded separately in validation logs.

## Stage A follow-up — 2026-10-09

PKCE now uses the existing sha2, base64 and rand dependencies: 32 random verifier bytes, URL-safe base64 without padding, and an S256 challenge. The RFC 7636 appendix B vector protects the encoding. The BM25 fork keeps its 16-entry stopword LRU, keyed by language and normalization, with standard-library storage. Generated IDs keep 24 lowercase base36 characters and a leading letter, sampled with the existing cryptographic random generator. WebSocket clients share tungstenite 0.29 with axum.

ZIP defaults remain enabled. Plugin archives have no compression-method restriction, so removing codecs would reject previously supported archives. This stage skips that reduction to preserve behavior.
