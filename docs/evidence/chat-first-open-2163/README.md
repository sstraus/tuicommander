# Chat first-open profile — #2163-a1db

## Cause and selected fix

The complete snapshot was applied as separate reactive frames. Each frame made
`TerminalChatTranscript` project the growing history again. For 2,000 entries,
this visits 2,001,000 entry positions rather than 2,000. It also schedules a
follow-bottom microtask for every partial history.

`chatViewStore.readOnce` now batches the synchronous snapshot reset, ordered
frame application, cursor update and unavailable-state clearing. The reducer,
transport, complete history and shared transcript renderer are unchanged.
Desktop and mobile use this same boundary. No virtualization, render cap,
search restriction, touch handler or follow-bottom rule was added.

## Baseline: one standalone simulator document

Source: `83cfeca46` before the production fix. The measurement-only probe adds
fetch/body/parse timing and measures transcript `scrollHeight` reads. The
existing trusted-click and visible-last-entry endpoint is unchanged.

| Interval | First open | CLI → Chat remount |
| --- | ---: | ---: |
| Trusted click to ready frame opportunity | 12,520 ms | 15,212 ms |
| Snapshot headers and body | 19 ms | 11 ms |
| JSON parse (reduced browser clock precision) | 0 ms | 0 ms |
| Post-decode consumer interval | 11,621 ms | 14,276 ms |
| `scrollHeight` style/layout reads, included above | 157 ms | 160 ms |
| Number of measured layout reads | 2,001 | 2,001 |

Each snapshot contains 2,000 updates and 481,442 decoded string characters.
Both Chat samples contain all 2,000 distinct rendered entry markers and show
the last entry at x=32, y=579, width=326, height=88. The dominant cost is repeated
synchronous history projection/render, not snapshot transfer, parse or the
measured follow-layout reads. The timer includes queued highlights and follow
work; its residual is not an exact render-only CPU measurement. A zero parse
reading means below the clock tick, not free decoding. Fetch intervals also
include backend and scheduling time, not just network transit.

The real desktop renderer independently reproduces the cause in
`TerminalChatSnapshot.test.tsx` (`snapshot replay publishes only the complete
transcript`). Its pre-fix RED observed 2,000 partial-history publications,
lengths 1 through 2,000, instead of one complete publication. Desktop
replay/render took 21,265.0 ms in Vitest; the test failed as intended. This is a
deterministic publication assertion, not a load-sensitive time budget.

## Environment and limits

- Same production-asset harness as
  [the earlier measurement](../mobile-cli-chat-1770-simulator/README.md):
  real backend snapshot, synthetic Claude JSONL, exact PID binding, real
  shared Chat renderer, no model calls and no mocked snapshot route.
- Isolated native `make agent-test AGENT_TEST_INSTANCE=agent-test-2163
  AGENT_TEST_PORT=19863`; both proposed ports were free at preflight.
- Loopback proxy `127.0.0.1:11663` forwards only to that instance's Unix socket.
- New task-owned iPhone 13 simulator, iOS 26.4.1 (`23E254a`), UUID
  `5893EA05-61F2-4B7C-8085-EAF37D0E61C5`. Home Screen standalone PWA;
  both standalone flags are checked, with a 390 × 797 document viewport.
- Baseline uses one iteration of the existing XCUITest measurement; final
  measurement uses its original five iterations and nearest-rank percentiles.
- External raw evidence: `~/Gits/.tmp/results/2163-simulator/`. Baseline
  XCUITest passed in 54.296 seconds. The first install attempt failed because
  the proxy process had exited; no rejected setup run is a timing sample.
- Shared host load is uncontrolled. These are simulator frame-opportunity
  measurements, not physical-phone, compositor-present or cold-device claims.
  Assets and backend caches can be warm.

## Corrected measurements

All intervals are milliseconds. Nearest-rank p95 is the maximum of five samples,
not a stable tail estimate. `summarize.mjs` accepted the exact trusted-tap and
transition sequence, both standalone flags, and all 2,000 rendered markers in
every Chat/resume sample. No history was omitted to satisfy the endpoint.

| Transition | Samples in run order | p50 | p95 |
| --- | --- | ---: | ---: |
| First Chat open | 405, 686, 406, 399, 448 | 406 | 686 |
| CLI → Chat remount | 617, 418, 407, 408, 383 | 408 | 617 |
| Retained foreground resume | 51, 82, 64, 63, 51 | 63 | 82 |
| Chat → CLI | 83, 40, 122, 55, 39 | 55 | 122 |
| Sum of remount and return-to-CLI render intervals | 700, 458, 529, 463, 422 | 463 | 700 |

Both required p50 values are below 1,000 ms. The post-decode consumer interval
is now 353–587 ms across the ten initial/remount snapshots. Each snapshot
causes one measured follow-layout read instead of 2,001; those reads take
158–188 ms. This retains the full initial layout cost but removes repeated
history projections. The five-iteration XCUITest passed in 146.663 seconds.

The corrected native binary reports SHA-256
`7d943b9604af343894d47856529f72a00191e687166c15a6994d576c105a8fc7`
and target `aarch64-apple-darwin`. Production changes are the snapshot batch and
removal of the now-resolved measurement deferral comment. The baseline and
corrected runs use the same simulator, proxy port, fixture shape and endpoint.

Committed evidence: [baseline events](baseline-events.jsonl),
[corrected events](fixed-events.jsonl), [statistics](summary.json), and the
[corrected first-open screenshot](first-open-1.png). All 15 app-only corrected
screenshots and the Xcode result bundle remain in the external evidence root.
They are captured after the ready marker, not at the compositor's first frame.
No separate pixel-level visual review is claimed.

## Focused regression validation

One final local Vitest command ran through `build-slot.sh` and
`scripts/with-test-tmp.sh`, selecting only:

- `src/__tests__/stores/chatView.test.ts`
- `src/__tests__/components/Terminal/TerminalChatSnapshot.test.tsx`
- `src/__tests__/components/Terminal/TerminalChatView.test.tsx`
- `src/mobile/__tests__/SessionChatView.contract.test.tsx`
- `src/mobile/__tests__/Transcript-touch.test.tsx`
- `src/__tests__/components/Transcript.newText.test.tsx`

Result: **63 tests passed in six files**, 3.20 seconds after scheduler admission.
The corrected real desktop 2,000-entry snapshot test took 614 ms in total,
including replay/render and its atomic-publication assertion. Reset replacement,
streamed delta chunks and empty reset each publish one coherent result.
Existing desktop/mobile touch-end, touch-cancel, gentle upward detachment,
stationary touch, follow-bottom, new-text and lifecycle tests stay green.
The existing desktop view tests print `TermStore.remove` debug lines; no test
error or warning is hidden. No full application suite was run.

The owned proxy, PTY, agent-test instance and simulator are stopped after the
measurement. No daily application port or installed executable was used.
The native Cargo builds are only for the authorized live-check binary; no
Rust source changed and no Rust test run is claimed.
