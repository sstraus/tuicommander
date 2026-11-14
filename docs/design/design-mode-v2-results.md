# Design Mode v2 pilot delivery — 2026-10-10

S0–S4 implementation is on `feat/design-mode-v2-1672`. The existing desktop instance was neither rebuilt nor restarted. Browser checks used an owned Chrome for Testing profile, a worktree Vite fixture and CDP screenshots. No MacControl permission was needed.

| Slice | Delivered | Evidence |
| --- | --- | --- |
| S0 | Capability probes and native Edge remaining-check table | [S0 results](design-mode-v2-s0-results.md) |
| S1 | Explicit one-shot/multi-pick overlays, reused extraction, persisted cards, comments and delete | Card/picker Vitest; hostile evidence Rust tests; real sidebar screenshot |
| S2 | Fixed extension identity, bounded framing/version checks, explicit instance/socket binding, dedicated-profile unpacked install | Four targeted native/shared-recipient tests; real native hello against the running `mcp.sock` |
| S3 | Launching recipient retained, live same-repository agents, persisted retry key, idle-gated send | Idle/busy composer, dedup, exit and hostile-input Rust tests; real browser-to-PTY demo |
| S4 | Configured URL plus live listener/PTY-hint discovery, ambiguity list, configured Run Script launch, Edge/Chrome choice, first-use setup page | Recorded Mac listener/cwd/Vite fixtures and path-boundary tests; first-run HTML test; frontend menu/transport tests |

## Demo

[Two commented picks](evidence/design-mode-v2/two-commented-picks.png), [delivery confirmation](evidence/design-mode-v2/delivery.png), and [exact PTY receipt](evidence/design-mode-v2/test-agent-pty.txt) are committed. The recipient was an isolated real PTY running a Python receiver, not a model pretending to acknowledge delivery. The opt-in test starts its own Unix-socket backend, uses the actual installed native host and extension, and checks both comments in the received bytes. The bracketed-paste wrappers in the receipt come from the shared input gate.

Final validation passed: `pnpm build`; 33 selected Vitest tests across seven files; four native/shared-recipient tests; eight targeted Design Mode Rust tests; one opt-in end-to-end demo. After adjusting screenshot framing, that demo passed again. Only changed behavior was selected; excluded tests were not claimed as coverage.

A separate real-host check listed seven live local same-repository agents from the currently running TUIC, including API-created PTYs whose `is_remote` flag is true. Repository identity uses Git common directories; only `connection_id` identifies a remote daemon. Updating the native executable in place caused macOS to kill it with exit -9. Installing via a fresh inode and rename corrected that observed failure; the installed host then exited 0 and listed the live agents.

## Remaining checks and limits

- Boss must restart/rebuild TUIC when ready to load the Rust changes. The existing app returns a precise HTTP 404/restart error for `/design-mode/extension`; it cannot exercise the new send/discovery backend. The isolated demo exercises those changed routes.
- Native Edge Load unpacked persistence, toolbar-granted `activeTab` capture and sidebar restoration need a native Edge check. The CFT demo retained DOM evidence while reporting its screenshot permission error; it does not prove successful cropped image capture. Cross-origin frame screenshots are omitted.
- Windows/Linux were not executed. Windows listener attribution is deliberately unavailable; Windows native-host registration currently targets Edge and needs Chrome/multi-profile validation before claiming that platform pilot. Everyday profiles, remote-daemon inspection and post-agent automatic reload remain later work.
- Listener ownership is conservative process/path evidence. Multiple candidates are offered instead of guessing; saved recent URLs are not yet a discovery source. Source hints reuse available framework metadata; the old CDP backend remains for the deferred source-map migration.

Coordinator-owned independent review and post-landing native checks remain separate from this implementation evidence.
