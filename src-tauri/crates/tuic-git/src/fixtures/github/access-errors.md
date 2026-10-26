# GitHub partial-error fixture

Recorded on 2026-10-22 through the already authenticated `gh api graphql` command.
No credential was read, printed, copied or stored. Response bytes are in
`access-errors.json`.

```graphql
query AccessReasonFixture {
  missing: repository(owner:"stefanostraus",name:"caplock") { name }
  restricted: organization(login:"Lansweeper") { auditLog(first:1) { totalCount } }
}
```

The repository returns NOT_FOUND. The organization audit log returns FORBIDDEN
with a nested alias path. This proves the partial-error wire shape, not that
TUIC's OAuth app is blocked by organization policy. Separate browser evidence
below confirms pending organization approval, but does not establish the exact
repository error response.

GitHub documents that NOT_FOUND/404 can also hide inaccessible resources; do not
claim it proves deletion. GitHub also documents the `X-GitHub-SSO` authorization
URL and that app-generated access tokens are automatically SAML-authorized:
https://docs.github.com/en/rest/authentication/authenticating-to-the-rest-api

## OAuth application organization status

On 2026-10-22, the authenticated `sstraus` browser session loaded:
https://github.com/settings/connections/applications/Ov23liN95BHKQDboVFRl

The TUICommander application's Organization access section showed:

- Lansweeper: **Access request pending**.
- lansweeper-oss: **Access request pending**.
- TUICommander: **Allowed**.

No grant, request, or revoke control was clicked. This confirms that TUIC's own
OAuth app awaits approval for the affected organizations. Access through the
separate `gh` OAuth app does not grant access to TUIC's app. Repeating TUIC login
does not resolve the pending organization approval.

The story retains the browser snapshot and screenshot evidence at
`~/Gits/.tmp/results/2127-authorized-app.txt` and
`~/Gits/.tmp/results/2127-authorized-app.png`. These are organization-policy
observations, not a repository GraphQL recording or a screenshot of the changed
Settings UI.

The isolated `make agent-test` build started successfully on port 19890 and
returned healthy socket status. Its Device Flow reached GitHub's account
confirmation page, but authorization was not completed while the Mac screen was
locked. The coordinator accepted the authorized-app page as the cause evidence
and stopped the login attempt. No token was read, printed, copied, or stored by
the verification client.

DEFERRED (2026-10-22) — capture a real OAuth-restriction/SAML repository response
after an interactive isolated-instance login. The coordinator treats this as
supporting follow-up evidence, not a completion gate. Do not label the recorded
audit-log denial as either of those policy reasons.
