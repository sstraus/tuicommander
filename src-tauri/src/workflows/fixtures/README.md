# GitHub trigger fixtures

These are authored fixtures from the documented GitHub REST/webhook contract, not recorded live-account successes. Source inspected 2026-11-11: [GitHub webhook payloads](https://docs.github.com/en/webhooks/webhook-events-and-payloads#issues), [REST issues](https://docs.github.com/en/rest/issues/issues#get-an-issue) and [REST pull requests](https://docs.github.com/en/rest/pulls/pulls#get-a-pull-request). The current official examples specify REST API version `2026-03-10` (the older `apiVersion=2022-11-28` documentation URL resolves to the current page).

`github-issue-opened.json` is the normalized backend event for an open issue with number 1347, title/body material and the bug label. Its actor is absent because a poll does not identify the sender of a change. Material digest reuses the existing title-NUL-body SHA256 contract; transport delivery identity does not determine generation. Designer 014 can use this fixture for selection/mapping preview; source schema is checked against the installed normalizer, not trusted as a grant from JSON.

`neutral-transport-engine.json` is actual JSON captured from the retained-output
transport integration on rb task20261111T122028Z-84608. It includes catalog,
publication/internal graph and inspection reply envelopes with consistent
ledger IDs/digests. Its project path is an ephemeral rb fixture directory.
The host is the internal increment fixture; it proves engine/transport shapes,
not production native integration, external calls or authorization of writes.

`neutral-transport-authoring.json` is actual JSON from the existing publication,
preview and validation regression on rb task20261111T123303Z-73484. It contains
catalog/draft/update/publication/clone/preview/invalid-validation envelopes from
one coherent project. Its identity set is separate from the retained-run fixture.
Both captures use `TUIC_NEUTRAL_FIXTURE_CAPTURE=1`; no client fixture is rebuilt
from source assumptions or merged across projects. Example capture command:
`build-slot.sh --remote env TUIC_NEUTRAL_FIXTURE_CAPTURE=1 scripts/with-test-tmp.sh cargo nextest run --manifest-path src-tauri/Cargo.toml --lib --success-output immediate -E 'test(neutral_transport_catalog_publication_preview_keep_native_authority)'`.

`github-issue-rest.json` and `github-pr-rest.json` are authored REST source snapshots for generation and exact head/base normalization. The tests also exercise complete bounded open-resource list pagination, excluding PR entries from the issue source and refusing truncated discovery. These do not assert live account behavior.

## Installed native catalog

The paragraph below records the historical capture before pending-batch selection.
The current 50-descriptor capture is recorded at the end of this file.

`neutral-native-catalog.json` is actual serialized service output from the installed
NativeHost, regenerated on rb task `20261112T081111Z-15675` with
`TUIC_CAPTURE_NATIVE_CATALOG=1` at integration HEAD `c69d032895` plus decision G2 (adds
`native/story.read@1`; the other 48 descriptors are unchanged). Earlier captures: rb task
`20261112T072219Z-90439` at `67ac6f9aa8` added `native/story.cohort.created@1`; rb task
`20261112T045430Z-23813` at `367b617091` added `native/workspace.stage@1`; rb task
`20261112T024012Z-21358` at `385da9fc5e` removed the inherited `text` input of
`agent/role@1`; rb task `20261112T022351Z-71701` at `f4bbecb24c` added
`inbox/approval@1` and the optional literal `text` input of `model/request@1`; rb task
`20261112T020708Z-17933` on `feat/wer-022-batch-barrier` added
`native/story.batch.{admit,land,pin,verdict}@1`; rb task `20261112T004238Z-23519` at `580c6af133` added
`native/operator.wait@1`, `native/operator.automatic_close@1`,
`native/review.bounded@1` and `native/correction.policy.assess@1`; `agent/role@1`
gains the optional `policy` config field from 025; the other 36 descriptors are
unchanged). Earlier captures: rb task `20261111T223208Z-95250` at `c216f4dfb5` added
`agent/role@1` (handler `host.agent.role`), `native/agent.report.read@1` and
`native/story.cohort.observe@1`; rb task `20261111T213844Z-86410` at `be67307d97` added
`native/issue.{read,triage,effect,handoff}@1`; rb task `20261111T172329Z-97345` at
`8c0a3fcca5` added `native/story.create@1`. The generator reads
only the Jira and GitLab plugin manifests, which are identical in plugin commits
`3b0b27a05f` (executed checkout) and `b8075d825c` (pinned). Previous capture: rb task
`20261111T155312Z-70134` at `4dee1c2acc`, artifact SHA256
`b39b278d8f224ac3457bdc85250b8cc24e4dc4a7f8ef723157b2600d21e35788`. Capture at `c216f4dfb5`: artifact SHA256 `02bb4a7ee1093becb9798cfd4ca17332366f8049e7ab0c2ad517a12a99bacac3`. Capture at `580c6af133`: artifact SHA256 `7c6f4862cc5be2bb0ef70ea69a1cd51670ba1cc238493c28495e7cd891047faa`. Capture on `feat/wer-022-batch-barrier`: artifact SHA256 `9fa5203ac0f82ed370ef309e8810046638e2c07b07359dfe1613566628bd030a`. Capture at `f4bbecb24c`: artifact SHA256 `40206fdf24a17fb96aac1d44c91a34d0487886ee572692ca82f88426ae140e7e`. Capture with the role fix: artifact SHA256 `15be3ac6da8e579657c286ab728df4dfd1a3600516748a8c1c471bedd6288068`. Capture with workspace.stage: artifact SHA256 `caad231fcfcc14cf5239a4e1f03e35823ddae7bf64f0968c5fde402d230d9072`. Capture with story.cohort.created: artifact SHA256 `934366453734913b786846605b6f30e943fb674f9a89e869c847cc8d249a05fc`. Artifact SHA256:
`726b616be90af27d6fab82c0e9741bbe586921e0252a0120390ff6be62d376a1`.

The single logical project `/workflow-fixture/native` covers native operations,
saved script/HTTP, configured model/profile, installed Jira/GitLab and Telegram.
Catalog, pure preparation, compatible/incompatible pinned subflow validation,
correction-region reply and selected GetPublication share exact service pins.
Correction inspection projects the retained owner scope identity/generation and
iteration, excluding random run IDs and time. Owner steps stop at loop admission;
no effects, executable, credentials, network or native review/checks execute.

Generate from the repository root:

```sh
build-slot.sh --remote scripts/with-test-tmp.sh env TUIC_CAPTURE_NATIVE_CATALOG=1 cargo nextest run --manifest-path src-tauri/Cargo.toml --lib --no-fail-fast --nocapture -E 'test(native_catalog_wire_matches_shared_designer_fixture)'
```

Extract the one `TUIC_NATIVE_CATALOG=` line from the collected rb `log` artifact;
the wrapper log can split long lines. Serialize that complete JSON with two-space
indentation and a trailing newline. Generator mode passed 1/1; it bypasses the
ordinary full JSON equality assertion and is not drift-comparison evidence.
Story 018 records the normal comparison above; later integration reuses compatible evidence. No live provider,
paid ACP, desktop or transport-parity claim follows from this fixture.


### 2026-11-12 pending-batch catalog capture

The current native catalog was captured from the installed host by the passing
`native_catalog_wire_matches_shared_designer_fixture` generator case in rb task
`20261112T172034Z-9963`. It adds `native/story.batch.pending@1`; all other
descriptors retain their contracts. The backend and frontend JSON copies are
byte-identical, SHA256
`81d9ad79b8c79f5de7bbca49750ee815379dfb79b4287ecb7cff8062e8695c21`.
The generator bypasses equality comparison; this is capture evidence, not an
extra Rust drift-validation pass. The surrounding targeted run had 12 passing
and three failing conversion/depth cases, so this capture does not imply that
the complete conversion or parity suite passed.


### 2026-11-12 role-context catalog capture

The current capture has 51 descriptors. The installed-host generator in rb task
`20261112T190308Z-37899` adds `native/agent.context.read@1` and the optional
typed `data` input of `agent/role@1`. Both JSON copies are byte-identical, SHA256
`25e7ea6ee21e0b8c505ab376bc7066afb477da199e287e4da2d3a41a91f4e657`.
The targeted run passed five cases, including native snapshot behavior and
resolve-plan publication validation. Generator mode is capture evidence, not
a normal drift comparison. The Designer consumer passed four tests in 3.35s
with Vitest 5.0.0. This does not establish live provider or complete execution parity.

## Issue graph contract capture (2026-11-12)

The current capture has 54 descriptors. It comes from the installed-host generator
in rb task `20261112T204853Z-82903`, adding separate issue preparation, graph
receipt and publication-disposition operations. Both shared files have SHA256
`c88c85cb0f030d70b3c0ea53fe67ffb101ee589d5bc217ce02353c07d1a0f74e`.
Generator mode passed; the same run found an unreachable legacy accepted end in
the new issue converter. That terminal node is removed. Normal catalog drift
comparison and the current production-dispatch test run separately; this capture
is not an effect-execution or live-provider parity claim.

## Cutover compatibility decisions

`cutover-compatibility.json` (019-683f) is an authored decision table, not a
recording of an external system. It names one decision for each legacy source and
version the cutover can meet; anything absent is refused.
`migration::compatibility_tests` checks its template rows against `convert`; the
other rows name the code and the existing tests that enforce them.
