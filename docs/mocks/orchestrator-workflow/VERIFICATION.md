# Canvas mock verification

Date: 2026-10-29. Scope: static documentation mocks only.

## Selected direction

Option B is selected: pinned delivery subflows with a portfolio view. Option A's explicit delivery chain is now the first subflow, not a separate alternative page. The visual reference is **Flow 51 · Model catalog**. This revision changes the mocks, not the implementation plan or production workflow engine.

Success criteria: typed boxes on a two-dimensional canvas; drawn success and fail/reject edges; parallel fan-out and all-Join; a left palette grouped Author, Build, Check, Route and Hold; a right selected-node inspector; concise labels and values; verified desktop and narrow interactions.

The portfolio has two pinned `delivery@3` Calls, a branch-local Ask, an all-Join, a proof cohort and a post-landing batch. The delivery subflow has parallel independent review and checks, bounded rejection to Implement, a decision Ask, an all-Join, integration and proof gates. Solid paths show success; dashed paths show rejection or pending-input routes.

## Method

The managed browser smoke helper ran first from the read-only orchestrator checkout:

```text
python3 tools/agents/browser-wrapper-smoke.py <unique evidence directory under ~/Gits/.tmp>
```

A unique named session then opened the local HTML files through:

```text
/Users/stefano.straus/Gits/personal/brainstorming/tools/browser-stealth/ab-stealth.sh
```

The wrapper's orphan scan used a task-owned directory. The verifier closed only its named tab and browser daemon. No desktop app, Vite server, production terminal or Mission Control tab was opened.

The completed bounded pass used desktop viewport 1680 × 1060 and narrow viewport 390 × 844. It used accessibility snapshots, DOM assertions, native mouse drag, select, fill and click operations, and full-page PNG captures. Desktop captures are 1680 × 1060. Narrow captures exclude the scrollbar gutter: 375 × 1874 for the editor and 375 × 1663 for the run. These image dimensions are not viewport dimensions.

## Results

| Check | Verified result | Evidence |
| --- | --- | --- |
| Portfolio editor | 10 typed nodes, 12 drawn edges, 4 dashed routes; inspector; no document horizontal overflow | [Desktop](screenshots/canvas-editor-portfolio-desktop.png) |
| Native pointer drag | Story B moved from (424, 432) to approximately (462, 408); attached edge path changed | `trusted-pointer-drag` in JSON |
| Delivery editor | 12 nodes, 15 edges, 5 dashed routes; parallel review/checks and all-Join | [Desktop](screenshots/canvas-editor-delivery-desktop.png) |
| Author controls | Changed role and model, restored them, edited prompt, removed Scoped write, set total reviews to 3 | `author-controls-and-publication` in JSON |
| Palette and validation | Added an Author; publish rejected its unconnected state; removed it | Wrapper transcript |
| Publish and discard | Published delivery v4; both portfolio Calls and active run remained pinned to v3; discard restored review limit 3 | [Published](screenshots/canvas-editor-published-desktop.png) |
| Live portfolio | A has pending Ask; B has running Critic; all-Join waits for delivery receipts | [Desktop](screenshots/canvas-run-portfolio-desktop.png) |
| Live delivery | Critic running; node cost/state inspector and sibling receipts | [Desktop](screenshots/canvas-run-delivery-desktop.png) |
| Branch-local answer | Answer made A ready; B stayed running; portfolio Join stayed at 0/2 receipts; repeated answer disabled | [Answered](screenshots/canvas-run-answered-desktop.png) |
| Narrow editor | 390px viewport, 1100px inner scrollable canvas, no document horizontal overflow; palette toggle opens/closes; 44px toggle | [Capture](screenshots/canvas-editor-narrow.png) |
| Narrow run | Same scroll isolation and palette/queue toggle; stacked inspector | [Capture](screenshots/canvas-run-narrow.png) |

The final verifier exited 0 in 111.54 seconds after smoke setup. All eight captures have valid PNG signatures and the dimensions above. Machine-readable results are in [browser-results.json](browser-results.json). Commands, accessibility snapshots and cleanup output are in [browser-transcript.txt](browser-transcript.txt). The smoke transcript is in [browser-smoke.txt](browser-smoke.txt).

## Corrections during verification

Earlier attempts exposed three harness problems: the screenshot directory had been removed with obsolete captures; a JavaScript selector was incorrectly quoted; native mouse coordinates needed integer arguments. These were corrected before the completed pass. A subsequent role-selection assertion exposed a mock defect: select changes were bound to `input` instead of `change`. The handler now uses `change` for selects and `input` for text fields. The final pass verifies role and model changes as well as prompt and review-limit input.

## Limits

- These are browser-rendered SVG graphs with local interactions, not backend workflow execution. Drafts and published snapshots live only in page memory.
- The mock does not prove durable branch concurrency, recovery, revision fencing, integration receipts, proof freshness or IPC/HTTP parity.
- Costs, times, states and commit abbreviations are illustrative, not measured deliveries. Answering A does not fabricate a completed delivery receipt.
- Screenshot capture and DOM/layout assertions are verified. Human aesthetic approval and pixel-level image review are not claimed. The verifier does not assert an empty browser console.
- Static native HTML controls stand in for production shared Solid primitives. Product implementation must use those primitives and shared backend services.
- No Rust build/test, Vitest suite, provider call, daemon or Fabbrica cluster was run. Real-story parity and distributed recovery remain future study validation.
- `source-digests.json` and `smoke.json` retain the original study's source evidence. The browser transcript, browser results, browser smoke transcript and canvas PNGs are the evidence for this revision.
