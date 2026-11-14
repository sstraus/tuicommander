# Refused browser settings edit — 2785-33f2

## Result

The fix is already on main in `db7ef46eb54b852ceadaecf171628de022cb9962`.
The 2026-11-06 audit makes no production or test changes.

The consumer contract is:

- A `403 confirmation_required` save reloads the stored settings and drops the refused edit.
- A warning names the owner confirmation requirement.
- A later ordinary edit does not send the refused value again.

The stored configuration is the source of truth. The HTTP route refuses the
whole request, not individual fields. Ordinary edits in that same request are
also reverted. The recovery assumes that `load_config` succeeds.

## Existing execution evidence

The main-store worklog dated 2026-11-03 records:

- RED before the fix: `egoExecutable` stayed pending.
- GREEN after the fix: settings store suites passed, 26/26.
- Biome and TypeScript checks passed.

The regression is
`src/__tests__/stores/settingsRefusedEdit.test.ts`:
`drops the refused edit, names the confirmation requirement, and lets a later save write`.
Its pre-fix commit is `a9da9bb009cd813e11ecb95ba75dd3042322b4b1`.
The worklog does not retain the exact test commands or the historical tool
versions. This audit does not reconstruct or claim them.

## Evidence reuse

At audit base `2e0811e3659bf78f8ff9a61cdb979f500704d028`, both files have
exactly the same Git blobs as the original fix:

| File | Git blob |
| --- | --- |
| `src/stores/settings.ts` | `3cf9e8a7a789b12cab19290c321216e5ff39c643` |
| `src/__tests__/stores/settingsRefusedEdit.test.ts` | `8d2741acad44ea13c1b8066dc59506d5920070eb` |

Verified with `git merge-base --is-ancestor db7ef46eb5 HEAD`,
`git diff db7ef46eb5 HEAD -- src/stores/settings.ts src/__tests__/stores/settingsRefusedEdit.test.ts`,
and `git ls-tree` at both revisions. The diff is empty.
Per the repository rule for fixes already on main, no tests were rerun.

## Acceptance audit

1. The regression submits a changed `ego_executable`. Its save adapter throws
   the real `HttpRpcError` with status 403 and `confirmation_required`. It then
   verifies restoration, the confirmation message, and a successful later IDE edit.
2. `persist` handles only that error shape. `revertRefusedEdits` calls
   `applyConfig(config, false)`, which updates rendered state and saved baselines.
   It adds the existing warning toast with the owner confirmation requirement.
3. `updateAppConfig` loads a fresh base before each write. After recovery the
   later IDE edit has no executable delta. The regression records one accepted
   packet with `ide = zed`. The backend `put_config` returns HTTP 200 on success.
   This is store-test and source evidence, not a new live HTTP observation.

Adjacent cases checked in source:

- Other HTTP errors do not discard pending edits: the catch branch requires both
  status 403 and the exact confirmation detail.
- Several edits in one refused request are reverted together: recovery applies
  the complete stored configuration without dirty-edit preservation.
- A JSON error response is recognized: `HttpRpcError` extracts its `error` field.

No UI, CSS, layout, transport, or backend code changed in this audit. No browser
screenshot, desktop launch, Rust build, or extra test run was needed.
