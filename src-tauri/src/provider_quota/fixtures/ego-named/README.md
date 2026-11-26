# Recorded ego named-account contract

Producer commit: `bbfa26d522052480ad5dc8826d8594d865a47786`.
Producer tree: `2746d3fbe55ecc53471af59a1ca19ffa8a0fcaff`.
Production correction: `1f3bab3cda536a89d74a8bce712148a1caf2ad1f`.

These ten JSON files are byte-for-byte copies of offline producer captures.
`SHA256SUMS` records their original hashes. Do not format or edit these files.

The producer captured them during rb run `20261117T091443Z-72244`:

- `active-quota.json`: actual `auth_usage::execute` output, including its newline.
  It also equals the producer's committed
  `crates/ego-cli/tests/fixtures/1143-named-quota-output.json`.
- `passive-rate-limits.json`: actual `CurrentQuotaReader` results serialized by
  the production `RateLimits` type. It is the array that the usage report wraps,
  not a complete report. Its distinct values identify legacy, A, B and an account
  whose ID equals the vendor. The stored observation source remains `active`;
  reading it through passive publication does not change its provenance.
- `error-*.json`: actual execute output for expired OAuth, missing stored OAuth,
  API-key access and missing named configuration. An orphan keeps its connection
  and omits its unknown vendor; the consumer must not invent one.
- `manifest-*.json`: actual `JsonlStore` manifest bytes, without an added newline.
  These captures have no model selection yet. They prove account projection and
  the unavailable state, not a complete live selected-model binding. Separate
  real-process tests provide explicit synthetic model selections and verify the
  resulting account/vendor keys.

All accounts, config, credentials and session IDs are isolated synthetic test
inputs. The quota window schema derives from an existing recorded provider
response. No live home, real credentials or provider request was used.

`ego_recorded_producer_contract_preserves_account_identity` passes these exact
bytes through the real consumer parser, passive registry observation and manifest
identity reader. Other focused tests use explicitly synthetic distinct values to
exercise active command selection, malformed identity and OAuth route-to-vendor
normalization. Legacy route names remain unchanged.
