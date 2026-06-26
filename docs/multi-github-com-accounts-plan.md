# Plan: Multiple github.com accounts

**Status:** Implemented (Path A — additive). Offline-tested; runtime verification
(real keyring / live device-flow / live polling) pending Boss.
**Last updated:** 2026-06-10

## As-built (Path A)

Boss-approved + GPT-5.4 second opinion. The ambient github.com default stays
synthesized with the stable id `github.com` and its legacy token slot (boot-safe,
zero-migration); ADDITIONAL github.com accounts are first-class per-account
entries (id = login). The single routing predicate is `is_ambient_default()`
(id == `github.com`) — it replaced `is_cloud()` at every global-vs-per-account
branch, so a named github.com account falls onto the same per-account machinery
GHE already uses, while current single-account behavior stays byte-identical.

Stories delivered: 001 (per-account storage) · 002 (per-account runtime
state + `min_rate_budget`) · 003 (`github_poll_add_account` device-flow add) ·
004 (multi-candidate host resolution) · 005 (named-account token anchoring, no
`gh` drift) · 006 (single-account UI collapse). Verified: 211 Rust tests, 9
vitest, `tsc`/biome/`cargo fmt`/clippy clean.

## Problem

The account model is asymmetric. GitHub Enterprise (GHE) accounts are fully
per-account (`ghe_state: DashMap<id, GheAccountState>` with per-account token,
viewer cache, circuit breaker, rate budget). github.com is a **singleton**
synthesized from global state, which is what breaks if a user wants two
github.com identities — and is also the root of the current asymmetry.

Enforcement / singleton points today:

| Concern | Location | Why it blocks multi github.com |
|---|---|---|
| PAT rejected for cloud | `github_account.rs:524` | `github_add_account` errors on `is_cloud()` |
| Single token slot | `credentials.rs:79,93` (`github/oauth-token`) | one keyring entry, not keyed by id |
| Single runtime token | `state.github_token` (`github.rs:1361`) | nowhere to store a 2nd token |
| Hardcoded id | `GITHUB_COM_ID = "github.com"` | two accounts can't share one id |
| Global viewer login | `state.github_viewer_login` (`github.rs:946`) | only one `@me` identity cached |
| `author:{viewer}` PR search | `github.rs:1269` | wrong identity for 2nd account |
| Issue filters | `github.rs:1090-1092` (`assignee/createdBy/mentioned`) | same |
| Global rate budget + breaker | `github.rs:1443`, `github_circuit_breaker` | two accounts clobber each other; poller throttle starves one |
| Host-only disambiguation | `github_account.rs:421` (`.find()`) | two github.com share host `github.com` |

What does **not** break: the per-repo PR/issue listing
(`repository().pullRequests(states:[OPEN])`, `github.rs:1255`) is
identity-agnostic and works as soon as tokens are per-account.

## Decision: one uniform path, single-account is the common case

Reject a separate "fast path" for the single-account case. With one account the
per-account machinery (a one-entry DashMap, one viewer cache) is effectively
free; a second code path only doubles bug surface and re-creates the asymmetry
that is the current bug source. The single-vs-multi intelligence belongs in the
**UX layer**, not in storage/runtime.

### Backend — unify github.com onto the per-account machine

1. The github.com account becomes a normal registry/`ghe_state`-style entry,
   auto-created on device-flow login, instead of a synthesized singleton.
   Retire `github_com_account()` as a special path.
2. Keyring keyed by account id for cloud too: `github/account/{id}/token`
   (drop the fixed `github/oauth-token` slot, or migrate it).
3. Per-account viewer cache, circuit breaker, rate budget for cloud — reuse the
   GHE `GheAccountState` path (`with_account_breaker` already branches; collapse
   the cloud branch into the per-account one).
4. `match_host` (`github_account.rs:421`): `.find()` → collect **all** accounts
   for a host, so two github.com accounts surface as two bind candidates.
5. **Anchor each account to an explicit token (OAuth/PAT).** Do not let a cloud
   account resolve via the shared `gh auth token` fallback (`github_auth.rs:514`)
   — that causes silent identity drift on `gh auth switch`. Keep the `gh`/env
   fallback only for the zero-config implicit default, not for named accounts.

### UX — collapse to simple when count == 1

- `accounts.len() == 1 && no remote ambiguity` → hide Enterprise Accounts +
  Repository Bindings entirely; auto-bind silently. The 99% flow stays as simple
  as today.
- The binding UI (and the already-native `.bindSelect` dropdown) appears only
  when a 2nd account is added or a repo is genuinely ambiguous.

## `gh` CLI interaction (no structural conflict)

TUIC and the agent's `gh` are separate auth domains; TUIC only *reads* `gh`
(`gh auth token`), never writes it. A PR authored under a different `gh` account
still shows via the per-repo listing (just not via `author:@me`). The only real
footgun is the `gh auth token` fallback drift — addressed by backend step 5.

## Testing

- Two github.com accounts: independent viewer identity, token, rate budget.
- Ambiguous repo (github.com remote + GHE remote): both surface as candidates.
- Single-account: binding UI hidden, auto-bind, behavior unchanged from today.
- `gh auth switch` does not move a TUIC named account's identity.
