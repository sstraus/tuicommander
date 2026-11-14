# Recorded workflow descriptor

`custom_script_descriptor.json` is copied byte-for-byte from backend commit
`b017427c01fa3953014d6ce6761dd25ddfe3be4f`, path
`src-tauri/src/workflows/fixtures/custom_script_descriptor.json` (story 007).
SHA-256: `e5bd28582d2cd2184f59b3e09f74bc53b6f769209566dc53c693f0255608bf82`.

The backend serialized this descriptor. Its drift test is
`workflows::run::neutral_script::descriptor_fixture_tests::published_custom_script_descriptor_matches_shared_renderer_fixture`;
owner-reported rb task `20261111T111123Z-51879` passed (1/1).
The frontend copy lets the Designer branch verify rendering before the backend
branch is integrated. It is an IO contract fixture, not proof of an installed
host or execution authority. No script is executed by the UI test.

`github-issue-opened.json` is copied byte-for-byte from story 010 commit `c3e4974dc1`, `src-tauri/src/workflows/fixtures/github-issue-opened.json`. Its backend README documents normalization provenance. It is a documented normalized issue fixture, not a captured live subscription. Trigger UI tests pass the complete event unchanged to mapping preview.

`neutral-transport-engine.json` is copied byte-for-byte from story 015's rb capture `20261111T122028Z-84608` (3/3 GREEN). SHA-256 `c0976e0559602768fe3380c7a055968746a68544a84b52de08a74827d1d0087a`. It contains actual reply envelopes for catalog, publication and retained inspection, including coherent graph/pins/ledger identities and answer8. Its provenance explicitly identifies the internal `host.increment` engine fixture; the ephemeral rb project path is retained unchanged. It is not native production-provider or live external-service evidence.

`neutral-transport-authoring.json` is copied byte-for-byte from story 015's real-command capture on rb task `20261111T123303Z-73484` (1/1 GREEN). SHA-256 `37984162dc7e8721bc6dbea0c535085f71cd6e503fa1c52d644d089deedd454a`. Draft/update/publication/clone/preview/invalid-validation envelopes belong to their own coherent project and identity set; tests do not join them to the retained-engine fixture. Provenance is internal `host.increment`, not a production native provider.

Both 015 observed engine/authoring fixtures are committed in owner checkpoint `912aa358f226d1cb79b696c72bca53af4d218cd0`; the hashes above are unchanged.

Historical capture before pending-batch selection (the current 50-descriptor capture is recorded below): `neutral-native-catalog.json` was copied byte-for-byte from the backend fixture `src-tauri/src/workflows/fixtures/neutral-native-catalog.json`, SHA256 `726b616be90af27d6fab82c0e9741bbe586921e0252a0120390ff6be62d376a1`, regenerated on rb task `20261112T081111Z-15675` at integration HEAD `c69d032895` plus decision G2 to add `native/story.read@1` (earlier: `67ac6f9aa8` plus B(a), SHA256 `934366453734913b786846605b6f30e943fb674f9a89e869c847cc8d249a05fc`, added `native/story.cohort.created@1`; `367b617091` plus 2960-d562, SHA256 `caad231fcfcc14cf5239a4e1f03e35823ddae7bf64f0968c5fde402d230d9072`, added `native/workspace.stage@1`; `385da9fc5e` plus the role fix, SHA256 `15be3ac6da8e579657c286ab728df4dfd1a3600516748a8c1c471bedd6288068`, removed the inherited `text` input of `agent/role@1`; `f4bbecb24c`, SHA256 `40206fdf24a17fb96aac1d44c91a34d0487886ee572692ca82f88426ae140e7e`, added `inbox/approval@1` and the optional `text` input of `model/request@1`; `feat/wer-022-batch-barrier`, SHA256 `9fa5203ac0f82ed370ef309e8810046638e2c07b07359dfe1613566628bd030a`, added `native/story.batch.{admit,land,pin,verdict}@1`; `580c6af133`, SHA256 `7c6f4862cc5be2bb0ef70ea69a1cd51670ba1cc238493c28495e7cd891047faa`, added `native/operator.wait@1`, `native/operator.automatic_close@1`, `native/review.bounded@1` and `native/correction.policy.assess@1`, and the optional `policy` config field of `agent/role@1` (earlier: `c216f4dfb5` added `agent/role@1`, `native/agent.report.read@1` and `native/story.cohort.observe@1`, SHA256 `02bb4a7ee1093becb9798cfd4ca17332366f8049e7ab0c2ad517a12a99bacac3`; `be67307d97` added `native/issue.{read,triage,effect,handoff}@1`; `8c0a3fcca5` added `native/story.create@1`). Previous direct-service production capture: rb task `20261111T155312Z-70134`, SHA256 `b39b278d8f224ac3457bdc85250b8cc24e4dc4a7f8ef723157b2600d21e35788`, plugin `b8075d825cb72407ff67dbc276d74f50dd21300e`. The real configured NativeHost and publication/validation/preview/correction services serialize NeutralReply envelopes; logical project `/workflow-fixture/native` keeps pins deterministic. Catalog has 49 descriptors across actual configured native/model/script/HTTP/Jira/GitLab/Telegram families. The retained correction scope is a projection excluding random run/time; owner stops at loop admission. Generator bypasses normal equality comparison (reserved for parent aggregate). No live HTTP wrapper, effect execution, network, credentials or native review/check claim. Coordinator explicitly accepts this direct-service layer; live wrapper parity belongs to018.


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
