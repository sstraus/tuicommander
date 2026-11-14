# pi provider_response channel capture

Source: real pi 0.84.2 (`pi --offline --no-session -ne -e tuic.mjs --no-tools
--model anthropic/claude-sonnet-4-5 -p 'Reply OK only.'`) with the managed TUIC
extension generated from this tree, run under `script` so the extension's
`/dev/tty` frames were recorded. Captured 2026-11-14 for story 2994-a1d0.

`TUIC_SESSION=fixture-owner`. A scratch `PI_CODING_AGENT_DIR` pointed the
built-in `anthropic` provider at a local server, so no credential and no
generating request were used. The server answered with a recorded-shape
Anthropic SSE stream and these response headers (mixed case on the wire):

- quota: `anthropic-ratelimit-unified-status`, `-5h-utilization`, `-5h-reset`,
  `-7d-utilization`, `-7d-reset` (synthetic values);
- not quota: `set-cookie`, `authorization`, `x-codex-turn-state`,
  `request-id`, `content-type`.

Observed native behaviour: `after_provider_response` fired once with
`status` 200 and lowercase header names; `ctx.model.provider` was
`anthropic`. The frame carries the five quota headers only.

Edit: the `agent_dir` and `home` fields inside the base64 frame were replaced
with `/home/fixture/...` paths and the frame re-encoded; everything else,
including frame order, is unchanged. The `script` noise before the first ESC
was dropped. Raw capture SHA-256 before the edit:
`c33cbbe871caf5d6167c47471b04a0dadf7973a2064d97d09fc20004d2cedf50`.
