# Codex 0.166 configuration fallback notification

Recorded from installed `codex-cli 0.166.0` on 2026-10-30 by critic-2541.
Source: `~/Gits/.tmp/results/critic-2541-probes/relative-profile.jsonl`.
The JSON fixture is the exact `configWarning` line from that capture, not a
hand-written response. No credentials are included.

The profile contains `model_instructions_file = "instructions.md"`. The file
exists beside the profile, but a relative CLI override resolves from the process
working directory. App Server emits this notification, then keeps answering RPCs
using defaults. `config/read` still reports the requested layer, so it cannot
prove that startup accepted the configuration.

The path-field inventory comes from the official `rust-v0.166.0` sources:

- `codex-rs/core/config.schema.json`: every field referencing `AbsolutePathBuf`.
- `codex-rs/config/src/loader/mod.rs`, `resolve_relative_paths_in_config_toml`:
  resolves these typed fields against each layer's directory before merging.
- `codex-rs/utils/absolute-path/src/lib.rs`: preserves absolute paths and expands
  home-relative paths through the process home context.

A separate isolated native replay by tuic-2541-f1 confirmed that the relative
override emits the defaults warning and the absolute rebased override does not.
Evidence: `~/Gits/.tmp/results/tuic-2541-f1-evidence/live-*.jsonl` and argv files.
