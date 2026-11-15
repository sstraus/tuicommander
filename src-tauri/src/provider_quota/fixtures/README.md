# Codex token count fixture

The `codex-token-count.jsonl` event was recorded from a local Codex rollout on
2026-11-12. The credit balance was replaced with null. No conversation text or
credential was copied. Tests change the timestamp and utilization to exercise
ordering across files and homes.

The `openai-codex-usage.json` and `anthropic-usage.json` bodies were recorded
from successful direct usage requests on 2026-11-12 by the quota contract
capture. They retain only the fields used by these adapters; account and credit
amount data were omitted. Source: `ego-ai/tests/fixtures/subscription_quota/`.

The `claude-status-line-stdin.json` body is the status line stdin that Claude
Code 2.1.318 sent after its first response in the S5 spike on 2026-11-12
(`docs/backend/quota-source-spikes.md`). Paths, the transcript location and
the session name were replaced; `rate_limits` is unchanged.

The `ego-contract.json` document is the recorded ego quota contract example
(ego story 1065-00ec, `crates/ego-ai/tests/fixtures/subscription_quota/contract.json`).
Its values come from the two successful endpoint recordings; the credential
identifier is synthetic.
