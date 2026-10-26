# Missing detached registration fixture

`missingDetachedOrphans.fixture.json` records actual Rust output from the real-Git test
`worktree::tests::orphan_session_guard::missing_detached_registrations_require_explicit_cleanup`.
The test creates detached worktrees, deletes one checkout and moves the other.
It captures discovery diagnostics and session-aware cleanup assessments together.
Paths are retained exactly as emitted by the Linux rb runner; they are fixture data,
not paths that the frontend test accesses.

## Provenance (2157-eb64, critic C1)

- Pre-fix commit: `861068c1f`.
- Backend RED: `~/Gits/.tmp/2157/c1-red-rust.log`.
- Coordinator RED: `~/Gits/.tmp/2157/c1-red-coordinator.log`.
- Pre-fix response: `~/Gits/.tmp/2157/c1-pre-fix-assessments.json`.
  Both unavailable registrations were cleanup candidates marked `safe: true`;
  the coordinator attempted automatic removal in both Ask and On modes.
- Fixed capture: `~/Gits/.tmp/2157/c1-green-rust-r2.log`, marker
  `2157_MISSING_ORPHAN_ASSESSMENTS=`.
- Runner result: `~/Gits/.tmp/rb/20261023T130315Z-61586`.
- Command: `build-slot.sh --remote scripts/with-test-tmp.sh cargo nextest run --manifest-path src-tauri/Cargo.toml -p tuic-git -p tuicommander --lib --no-fail-fast --success-output immediate -E 'test(discovery_2157) | test(discovery_2156) | test(orphan)'`.
- Result: 75 targeted tests passed, including explicit removal and the
  disappearance-after-assessment automatic-removal guard.

The fixed fixture was parsed from the marker, not edited to set safety values.
It extends the original assessment-only response with the inventory diagnostics,
so the coordinator test proves that two registrations remain visible as diagnostics
while neither enters automatic cleanup. The no-removal assertion is unchanged.
