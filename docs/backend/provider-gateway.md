# Provider gateway

Every outbound request to GitHub, GitLab, Jira and to a plugin destination goes
through one gateway. The gateway owns the quota budget, the response cache, the
failure memory and the per-caller metrics. A source-scan test fails when
production code sends to a GitHub host outside it.

This document describes the gateway as built. A hook that exists in the crate
but has no production caller is listed in [Not wired](#not-wired).

## Layout

| Path | Contents |
|------|----------|
| `src-tauri/crates/tuic-gateway/` | Pure policy. No I/O. `Transport` and `Clock` (`ports.rs`) are the only doors to the outside. |
| `src-tauri/src/provider_gateway/mod.rs` | `ProviderGateway`: builds the gateway with the shared `reqwest` client, the system clock and the persistent cache tier. `AppState.gateway` holds it. |
| `provider_gateway/transport.rs` | `Transport` over pooled `reqwest` clients. The only file that sends to the wire. |
| `provider_gateway/l2_sqlite.rs` | SQLite adapter of the persistent cache tier (`provider_cache.sqlite3`). |
| `provider_gateway/direct.rs` | Sends that are not REST API reads: OAuth, account verification, update manifest, release downloads, CI logs. |
| `provider_gateway/plugin_egress.rs`, `plugin_egress_rules.rs` | Admission and send path of plugin `httpFetch` and of the workflow connectors. |
| `provider_gateway/signals.rs` | Turns poller, watcher and account events into invalidation signals. |
| `provider_gateway/stats.rs` | The diagnostics snapshot. |
| `src-tauri/src/github_gateway.rs` | The GitHub client's path to the gateway: `Origin`, the typed refusal text, the poller quota view, `StatusInvalidator`. |

## Request model

A caller builds one `GatewayRequest` (`model.rs`):

| Field | Meaning |
|-------|---------|
| `provider` | `GitHub`, `GitLab`, `Jira` or `Generic`. Selects the route table and the header parser. |
| `account` | `AccountKey { account_id, api_base }`. One credential identity on one API host. |
| `effect` | `Read` or `Write`. The caller states it. |
| `channel` | `Interactive`, `Visible` or `Background`. The caller priority. |
| `freshness` | `Default`, `Force` (the Refresh button) or `CacheOnly`. |
| `caller` | `CallerId`: a static label of the call site. |
| `graphql` | Optional identity of a cacheable GraphQL read. |

`Gateway::execute` returns a `GatewayResponse` with the status, headers, body,
a `source` (`Network`, `Cache`, `Revalidated` or `Stale`) and a `BudgetSnapshot`
of the bucket. A refusal is a `GatewayError`:

| Error | Meaning |
|-------|---------|
| `RateLimited { bucket, retry_at, reason }` | The provider refused, or the bucket is empty. |
| `Shed { bucket, retry_at, level }` | The gateway refused to protect the quota. |
| `Transport(text)` | Connection failure, timeout, a host in back-off or a `CacheOnly` miss. |
| `Cancelled` | The shared flight ended without a result. |

One `execute` call does these steps in order:

1. `routes::classify` maps the URL to a route.
2. The cache gives the stored entry of the request, if there is one.
3. `policy::admit` decides: send, send with a validator, serve the cache, shed or limit.
4. A send goes through the coalescer (reads only), reserves one unit of the bucket and calls the transport.
5. The response updates the budget, the failure memory and the burn rate.
6. A successful write raises an invalidation signal.
7. The outcome is counted one time in the metrics.

## Caller ids and channels

Every call site names itself. The GitHub client takes an `Origin`
(`github_gateway.rs`), which holds the caller id and the channel:

| Constructor | Channel | Use |
|-------------|---------|-----|
| `Origin::interactive("<module>.<fn>")` | `Interactive` | A user action. |
| `Origin::visible("<module>.<fn>")` | `Visible` | A read that a visible panel waits for. |
| `Origin::background("<module>.<fn>")` | `Background` | Polling and monitoring. Shed first. |

The id format is `<module>.<fn>`, for example `github.pr_merge`,
`github.poll`, `pr_actions.merge_or_enqueue_pr`, `github_auth.device_flow`,
`accounts.verify`, `updater.fetch_channel_manifest` and
`release_install.download_and_install`. MCP routes use `mcp.<area>.<action>`.
Native GitHub workflow code uses `workflow.<module>.<fn>` on the `Background`
channel. Workflow connectors use `neutral_jira.request`, `neutral_gitlab.fetch`
and `neutral_http.execute`. A plugin request has the id `plugin`.

`provider_gateway/caller_tests.rs` scans the source and fails when a `RepoApi`
is built without an `Origin`.

## Routes and buckets

`routes.rs` holds a closed route table. `classify` removes the API base path
(`/api/v3`, `/api/v4`, a Jira context path) and returns a `Route`: a template
(for example `/repos/{owner}/{repo}/pulls/{number}`), a bucket, a TTL class,
invalidation tags and an effect. A URL that the table does not list is the
generic route: template `*`, bucket `default`, TTL class `Volatile`, no tags.

| Provider | Buckets |
|----------|---------|
| GitHub | `core`, `graphql`, `search`, `code_search` |
| GitLab | `default`, `search` |
| Jira | `burst:<template>` for each read route and for issue creation; `per-issue:<KEY>` for each write to one issue |
| Generic | `default` |

The budget is kept per `(AccountKey, bucket)`. A limit on one account or one
bucket does not stop another.

| TTL class | Base TTL | Routes |
|-----------|----------|--------|
| `Volatile` | 30 s | Lists, CI runs and jobs, pipelines |
| `Item` | 60 s | One issue, PR, MR, comments, reviews, job logs |
| `Search` | 60 s | Search routes |
| `Meta` | 1 h | Labels, milestones, assignees, members, boards, Jira metadata |
| `Identity` | 24 h | `/user`, `/myself` |

## Budget

`budget.rs` keeps what the provider headers said about each bucket: limit,
remaining and reset. `headers.rs` parses the rate-limit headers of each
provider into one `BudgetObservation`. Each admitted send reserves one unit as
in-flight, so concurrent callers do not all see the same remaining value. A
sent request spends its unit. A cache hit and a `304` revalidation spend
nothing. After the reset time, the stored remaining value is ignored.

## Pressure levels and admission

`policy::pressure` computes one level per bucket on every admission:

| Level | Condition |
|-------|-----------|
| `Free` | More than 50 % remaining, or the limit is not known, or the reset time is in the past |
| `Watch` | 20 % to 50 % remaining |
| `Tight` | 5 % to 20 % remaining |
| `Critical` | Less than 5 % remaining |
| `Exhausted` | Remaining is 0, or the failure memory holds the bucket |

The burn rate is an exponential average of units per second with a 5 min time
constant. When the burn rate empties the bucket before the reset, the level
goes up by one step, but not to `Exhausted`.

`policy::admit` then decides per channel:

| Channel | `Free` | `Watch` | `Tight` | `Critical` |
|---------|--------|---------|---------|------------|
| `Interactive` | Send | Send | Send | Send. Shed when the bucket already has a request in flight. |
| `Visible` | Send | Serve a stale entry, else send | Send with a validator; else serve a stale entry; else send | Serve a stale entry, else shed |
| `Background` | Send | Send | Send only with a validator, else shed | Shed |

Rules that apply before the table:

- A fresh entry is served from the cache at every level.
- `Exhausted` never sends. It serves a cached entry (fresh, or stale and at most 24 h old). Without one, the answer is `RateLimited`.
- `Force` is treated as `Interactive` and ignores a fresh entry. It does not pass an `Exhausted` bucket.
- `CacheOnly` never sends.
- A shed answer gives `retry_at`: the hold or the reset when one is known, else 30 s. A limited answer falls back to 60 s.

A stale entry served at `Watch` is not revalidated in the background.

## Failure memory and retry

`failure.rs` replaces the former GitHub circuit breaker.

- **Limit hold.** A provider refusal closes the bucket until `Retry-After`, else the reset time, else 60 s. A second and later consecutive refusal doubles the wait with a jitter of 0.7 to 1.3, to a maximum of 15 min unless the provider asked for more. A `2xx` response clears the hold.
- **Host back-off.** Three consecutive transport failures close the host for 5 s. Each further failure doubles the wait, to a maximum of 5 min. A `2xx` response clears it.
- **Retry.** There is one retry rule. An `Interactive` read is sent one more time when the provider asks for a wait of 5 s or less. A write is never sent again.
- **Last unit.** A `2xx` response that spends the last unit of a bucket is delivered. The hold stops the next call.
- **Stale if error.** A transport failure, a `5xx`, a limit or a shed serves the stored entry when it is at most 24 h old. Without an entry, a `5xx` response is returned as it is.

A new login calls `Gateway::reset_account`: the gateway forgets the limits and
the quota of the account and drops its cache.

## Cache

Only an identified read is cached and coalesced: a `Read` without a body, or a
read with a `GraphqlRead` identity. The cache key is the account, the method,
the URL with sorted query pairs and no fragment, the `Accept` header and the
GraphQL identity. An entry is never served to another account.

| Tier | Store | Limits |
|------|-------|--------|
| L1 | Memory (`cache.rs`) | 64 MiB of bodies. An entry not read for 24 h is evicted. |
| L2 | `provider_cache.sqlite3` (`l2.rs`, `l2_sqlite.rs`) | 256 MiB, zstd bodies. A row not read for 7 days is deleted. Writes are flushed every 2 s. |

- Only a `200` response is stored. A body larger than 2 MiB and a `Cache-Control: no-store` response are not stored.
- A stale entry with an `ETag` or `Last-Modified` is revalidated with a conditional request. A `304` keeps the body and counts as `Revalidated`.
- Each unchanged revalidation multiplies the TTL by 1.5, to a maximum of 8 times the base TTL. A changed body sets the multiplier back to 1.
- L2 is read on an L1 miss, so validators survive a restart. Credential headers are removed before a row is written, and a URL that carries a credential is not persisted. A request with the marker header `x-tuic-l1-only` stays in L1.
- L2 is disposable. When the file cannot be opened, the gateway runs on L1 only and logs `provider cache unavailable` with `source = "gateway"`. Retention rules: [`persistence.md`](persistence.md).

Concurrent identified reads with the same key share one flight (`coalesce.rs`).
A caller that is dropped does not cancel the flight for the others. A write
never shares a flight.

## Invalidation

A stored entry is indexed under the tags of its route (`invalidate.rs`), for
example `repo:<account>/<owner>/<repo>`, `pr:<account>/<owner>/<repo>#<n>`,
`gitlab:<account>/<project>!<iid>` and `jira:<account>/<KEY>`.

| Signal | Source | Effect |
|--------|--------|--------|
| `Wrote` | A `2xx` write through the gateway on a write route | Drops the tags of the route and every search entry of the account |
| `RepoChanged` | Poller PR and issue updates, GitHub transitions, a branch head change (`signals.rs`) | Drops the entries under the tags. Search entries stay. |
| `Logout` | Account removal or logout | Drops every entry of the account in L1 and L2 |

A write to a GitHub issue also drops the PR with the same number, and the
reverse, because the issue routes return PRs too. After the cache applies a
write signal, the `SignalSink` gets it. `StatusInvalidator` uses it to drop the
parsed PR status (`git_cache.github_status`) of the repositories that the write
touched, so the PR badge refreshes immediately.

## Direct sends and streams

`provider_gateway/direct.rs` is for sends that are not cacheable API reads.
`Direct::new(origin, provider, account)` sends each request as a `Write`, so
the gateway does not cache or coalesce it. The budget, the failure memory and
the metrics apply. Options: `no_redirect()` (a credential header must not
follow a redirect), `limited_redirects()` (5 hops), `redact_url()` (the URL is
a pre-signed link, so no failure message quotes it) and `timeout()`.

`Direct::stream` uses `Gateway::execute_stream` for a large body such as a
release download. Admission, the budget, the failure memory and the metrics
apply. The cache, coalescing and stale-if-error do not. The call is counted one
time, when the body ends or is dropped, with the bytes read.

Build such a request with `direct::builder()` or `direct::builder_client()`.
Do not call `.send()` on it.

## Plugin egress and workflow connectors

Plugin `httpFetch` and the workflow connectors go through
`plugin_egress::fetch`. Admission runs first, in a fixed order: capability
`net:http`, the manifest from disk, owner destinations, then the account rules
or the `allowedUrls` rules. Only an admitted request reaches the gateway. The
module follows redirects itself, at most 5 hops, and admits each hop again.

- An account `GET` or `HEAD` is a cached read in the bucket of the account. Its body stays in L1.
- A request without an account is budgeted under `plugin-anonymous` and the URL origin. It is never cached.
- Every plugin request uses the `Interactive` channel.
- The request header `x-tuic-freshness: force` asks for `Force`. The gateway removes it before the wire. The Tasks Refresh buttons set it (`src/plugins/freshness.ts`).
- The plugin sees `x-tuic-cache: hit|revalidated|stale` on a cached answer.
- A gateway refusal becomes a synthetic `429` with `x-tuic-gateway: rate_limited` and `retry-after` in seconds.

Host code describes itself with `Declared`. `Declared::workflow(read, caller)`
sets the effect, the caller id and `Freshness::Force`. Thus every workflow
connector read (Jira, GitLab, HTTP) goes to the network: a workflow gets no
cache benefit. The gateway still budgets these reads and coalesces identical
ones that run at the same time. A Jira search is a `POST` with a body, so it is
not cached or coalesced.

## Typed rate-limit and stale states

The UI gets a refusal as data, not as text to parse.

- **Core commands.** The error string starts with `rate-limit:` followed by JSON `{kind, bucket, retry_at, message}`. `kind` is `rate_limited` or `shed`. `retry_at` is epoch milliseconds. The string is identical on IPC and HTTP. Code: `github_gateway::refusal_message`.
- **Frontend.** `parseLimitError` (`src/transport/limitError.ts`) reads the string form and the object form that plugin clients throw. `GatewayNotice` (`src/components/ui`) shows `Rate limited · resets hh:mm` and, for stale rows, `cached hh:mm`. The Tasks list, board, roadmap and the PR popover use it. The view schedules no reload before the reset time.
- **Poller.** `min_rate_budget` paces the poll interval from the `graphql` and `core` buckets. `graphql_closed` skips the poll of an account whose bucket is `Exhausted`. The GitHub diagnostics field `circuit_breaker_open` is true when a bucket is `Exhausted`.
- **Workflow connectors.** A `429` (from the provider or from the gateway) is reported in the activity diagnostic with the retry hint: `retry_at` for Jira, `retry_after` for GitLab and HTTP. No automatic retry runs and a write is not repeated. The HTTP connector accepts a `304` answer to a read as "not changed".

## Metrics

`metrics.rs` counts each `execute` one time. The key is caller, provider,
account, bucket and route template. Labels never contain a URL, a query value,
a header or an error text. The counters are `calls`, `sent`, `network2xx`,
`networkOther`, `revalidated`, `cacheHits`, `staleServed`, `shed`,
`rateLimited`, `transportErrors`, `cancelled`, `bytesIn`, `units` and the
`p50Ms`/`p95Ms` latency. A ring holds the last 2,000 requests.

Read the snapshot with `GET /diagnostics/provider-gateway[?reset=true]`, the
Tauri command `provider_gateway_stats(reset)` or MCP `debug action=gateway`.
The JSON shape is in [`diagnostics.md`](diagnostics.md#provider-gateway-traffic).

## The bypass guard

`provider_gateway/bypass_tests.rs` scans the production source. It fails when a
file calls `.send()` and names a GitHub host, or is one of the path-listed
files (`circleci.rs`, `updater.rs`, `release_install.rs`,
`remote_deploy/assets.rs`, `accounts/verify.rs`, `github_account.rs`).

The allow-list has three entries, each with its reason:

| File | Reason |
|------|--------|
| `provider_gateway/transport.rs` | The wire adapter of the gateway |
| `github_live_measurement_1995.rs` | Test-only receiver |
| `workflows/run/native_github_git.rs` | Git over HTTPS is git transport, not the REST API |

Do not add a file to the allow-list to make the test pass. Route the send
through `Gateway::execute`, `Direct::send` or `Direct::stream`.

## Add a route or a provider

**Route**

1. Add one arm to the provider function in `routes.rs` with `read(...)` or `write(...)`: template, bucket, TTL class and tags.
2. Give a write the tags of the item and of its collection, so the write drops the related reads.
3. Add a case to `src-tauri/crates/tuic-gateway/src/tests/routes.rs`.

**Provider**

1. Add a `Provider` variant in `model.rs` and its label in `metrics::ProviderLabel`.
2. Add a header parser in `headers.rs` that fills `BudgetObservation`.
3. Add a route function in `routes.rs` and call it from `classify`.
4. Add `Tag` constructors when the provider needs new tag forms.
5. Map the provider name in `plugin_egress::provider_of` when plugin accounts use it.

**Call site**

1. For GitHub, pass an `Origin` with a `<module>.<fn>` id and the correct channel.
2. For a send that is not an API read, use `Direct`.
3. For host code that uses the plugin path, pass a `Declared`.
4. Run `bypass_tests` and `caller_tests`.

## Not wired

These items exist in the crate and have tests, but no production code calls
them:

- `GraphqlRead`: no call site sets it, so every GraphQL `POST` is a write. GraphQL responses are not cached or coalesced.
- `policy::ttl_multiplier`: the TTL does not change with the pressure level.
- `BudgetBook::configure`: no local token bucket and no write spacing are configured. The budget uses provider headers only.
- `Freshness::CacheOnly`: no call site uses it.
