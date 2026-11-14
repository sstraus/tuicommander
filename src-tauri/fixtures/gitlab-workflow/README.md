# GitLab workflow contract evidence

`documented.json` preserves JSON examples extracted from the official documentation
on 2026-11-11. These are REST API v4/webhook contract examples, **not live captures**:

- https://docs.gitlab.com/api/merge_requests/#retrieve-a-merge-request-diff-version
- https://docs.gitlab.com/api/notes/#create-new-merge-request-note
- https://docs.gitlab.com/user/project/integrations/webhook_events/

The diff example uses the v4 version endpoint; `collapsed` and `too_large` are
specified since GitLab 18.4. The docs are rolling, not a claim of a tested server
release. The issue webhook example has inconsistent project identities (1 vs 14).
The normalizer refuses that exact example; it is not silently corrected.

Existing `plugins/gitlab/fixtures/recorded.json` supplies unchanged real CE 19.4.1
(191678a3764) issue/MR/pipeline responses recorded on 2026-10-11. Its provenance
records the image digest and sanitization. Pipeline statuses were set through the
console without a runner. They prove response shape, not actual runner behavior.

Local account/HTTP tests use an isolated registry, mock credential backend and
local JSON responder. The note echo tests the documented request/response boundary;
it is not a recorded GitLab publication. Negative cases deliberately mutate fixture
fields to test mismatched identities and truncation.

Live gaps: an authorized GitLab project/account is required for real webhook
registration/delivery/retry, retrieval of a fresh diff version, actual runner
status, note publication and lost-response service reconciliation. None was
supplied or authorized. General MR notes have no atomic head precondition and
are not approvals; the reviewed head/base remain explicit in body and receipt.
