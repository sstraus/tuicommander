# Doctor predicate mutation replay (2621-b87c)

The two historical survivors are caught by tests already committed in
`2dc41aebcc1ad1803d1eee52c74b26277b14ee66` for story 2616-b008. No production
or test changes were needed. This report records the independent replay.

## Scope and result

The rb Linux aarch64 run tested commit
`0262cf751bb5ca84f8e4b9c8868d20db2fa18aaf`. The unmutated baseline ran two
tests: both passed. Cargo-mutants applied each exact historical mutation in
isolation and restored the source afterward: two tested, two caught, zero
survivors, zero unviable mutants. Both generated diff files match the historical
gate artifacts byte for byte.

| Mutation in `src-tauri/src/ego_cli.rs` | Public-boundary regression | RED result |
| --- | --- | --- |
| 583:29 `==` to `!=` in `providers_in` | `providers_in_reports_unknown_when_credential_probe_is_in_another_section` | `Missing` instead of `Unknown`; Nextest exit 100 |
| 584:17 `&&` to `\|\|` in `providers_in` | `providers_in_reports_unknown_when_credentials_section_has_no_credential_probe` | `Missing` instead of `Unknown`; Nextest exit 100; the other named test also failed |

The tests call `providers_in` using CLI output derived from the recorded
`ego-doctor-exit1.json` fixture. They protect the distinction between missing
credentials and missing credential evidence. Neither mutant is equivalent.

## Reproduction

From the repository root, through `build-slot.sh --remote --gate`, with
`RB_REF=0262cf751bb5ca84f8e4b9c8868d20db2fa18aaf`, run inside `src-tauri`:

```sh
cargo mutants --in-place --jobserver=false --test-workspace=true \
  --file src/ego_cli.rs \
  --re 'src/ego_cli.rs:58[34]:.* in providers_in' \
  --exclude-re 'delete field' --caught --output ../.tmp/doctor-2621 -- \
  -E 'test(providers_in_reports_unknown_when_credential_probe_is_in_another_section) | test(providers_in_reports_unknown_when_credentials_section_has_no_credential_probe)'
```

First list the selected mutants and require exactly the two names above. The
repository mutation config supplies `--lib`; do not repeat it. The runtime
filter covers only the two named regressions. This is not a full-suite result
or a new coverage/CRAP claim.

## Retained evidence

All local evidence is under `/Users/stefano.straus/Gits/.tmp/`:

- `results/codex-2621-replay-3.log`: successful command and summary.
- `results/codex-2621-artifacts/mutants.out/`: baseline log, per-mutant logs,
  exact diffs, `outcomes.json`, and mutant list.
- `rb/20261101T074207Z-70325/`: pinned source receipt and mutation metadata.
- `results/codex-2621-run.sh`: background replay command.

Two setup attempts ran no tests: the first rejected an ordering assumption in
the list guard; the second built successfully but Nextest rejected duplicate
`--lib`. The corrected third attempt supplies all GREEN and RED evidence.

Risk is low: this follow-up records evidence only. No native Mac Rust tests,
desktop instance, production changes, or new test assertions are involved.
