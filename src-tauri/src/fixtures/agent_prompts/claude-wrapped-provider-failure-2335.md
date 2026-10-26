# Recorded narrow Claude provider failure — 2335-9c03

Captured on 2026-10-25 from installed Claude Code 2.1.308 in a real Python `openpty` at 41 rows by 40 columns. SHA-256: `5fed32c51298ada70662ce4e78db61831b02ff3b90924494959dc53381957232`.

Source: `~/Gits/.tmp/2335-wrapped/claude-wrapped-provider-failure-2335.tcap`, recorded by `~/Gits/.tmp/2335-sources/wrapped.py`. The script preserves output read boundaries, monotonic timestamps, native input records, and geometry in TUICCAP2 framing. No output bytes were changed. It starts the installed CLI with a private HOME, CLAUDE_CONFIG_DIR, working directory and TMPDIR, a dummy API key, and a loopback Anthropic SSE server. No paid provider or real credential is used. The test did not start or access a TUICommander desktop instance.

The loopback response ends before its advertised Content-Length and message_stop. Claude renders its final native failure as physical rows:

```text
⏺ API Error: Connection to the
  API was lost (ECONNRESET).
  This is usually temporary —
  try again.
```

The native bullet and the wrap inside `Connection to the API was lost` both prevent the old full-sentence, optional-⎿ pattern from matching. Replay drives the real TerminalGrid and parse_clean_lines boundary, not an invented transcript. The capture ends after the final repaint; the owned CLI process and loopback server are stopped by the recorder.
