# Typed workflow HTTP and installed connectors

Story 009-e32f supplies an internal `NeutralHost` adapter, now delegated by the
installed native provider. Public starts bind authenticated backend callers before dispatch.
It uses the existing plugin
HTTP client and neutral effect ledger, with no second scheduler or credential
store. It adds no IPC command or HTTP route.

`ConnectorContract` is shared metadata for installed HTTP, client and tool-backed
operations: credential reference, resource schema, cursor pagination, rate-limit
handling, callback correlation and reconciliation. `HttpConnector::new` includes
the entire binding in the descriptor's `connector` schema default for installed
descriptors, or in a saved `ReusableTarget::Http {connector,plugin,base}`. The
existing reusable-operation store validates saved bindings through that same
constructor and preserves immutable versions; it adds no connector store. Bases
must be credential-free HTTP(S) URLs ending in `/`. Catalog pins
therefore retain the plugin identity, base URL and contract. An instance cannot override this metadata. The current backend supplies
separate connector network and write grants from the native caller's current and
captured authority; serialized requirements grant nothing. External writes also
require an exact ledger approval through the installed native provider.

An instance config has a `request` object with `method`, `endpoint`, `path`,
`query`, `headers`, `body`, `response` and `max_response_bytes`. The endpoint is
a literal under the installed base, without user information, query or fragment.
GET, HEAD and OPTIONS are read-only; every other method is a write. Publication checks
known fields and current grants. Dispatch checks resolved inputs and grants again,
including immediately before client entry. Resource-schema values become effect
ownership keys; absent schemas serialize writes at connector scope.

Path segments, query values, safe headers and JSON bodies accept either
`{"source":"literal","value":...}` or
`{"source":"input","field":"initial","max_bytes":256}`. The shared bounded
value contract also fits data argv slots of an installed client; it provides no
executable selection or process launcher. Dynamic values cannot select methods,
destinations, credentials, header names, executables or write grants. Path segments
are encoded individually; dot traversal is refused. Header slots accept Accept,
Content-Type, conditional headers and `X-*` names. Header names are checked syntactically without credential-name heuristics. Credentials use only an installed typed
credential reference; the existing account client loads and redacts them and
checks their actual destination on requests and redirects.

`response` maps output names to JSON pointers. The ordinary descriptor schema
validates mapped values before completion. Empty successful response bodies map to JSON null before output mapping, supporting 204 and HEAD. Cursor pagination names the items and
next-cursor pointers, query parameter, array output and page/item bounds. Missing,
null or empty cursors terminate. Repeated or malformed cursors, exhausted bounds,
invalid JSON and wrong output types fail rather than return a truncated success.
The cumulative byte limit covers all pages. Only nonmutating operations paginate.
The resulting typed array carries activity-output provenance into 003-e678
`ForEach` expansion and retains source order.

No request is automatically retried. HTTP 429 retains a numeric Retry-After hint;
a mutating request stays uncertain. Other write failures also retain uncertainty. Failed reads (status, transport, parsing, bounds or output validation) complete as failed activities with stopped ownership.
HTTP 202 retains the original effect digest as the pending handle when callback
reconciliation is declared, otherwise it stays uncertain. An installed correlation
header carries that precommitted digest. Callback ingestion must use the existing
fenced `EffectCommand::Observe` with the original effect key. Recovery does not
issue a probe or replacement write; resources stay held without completion and
stop evidence. The adapter cannot infer remote stop from local cancellation.

## Recorded contract and evidence limits

The targeted tests use the real installed HTTP client against a local HTTP/1.1
JSON contract responder. Its documented contract is synchronous 200 JSON results,
cursor pages `{"items":[...],"next":"cursor"|null}`, asynchronous 202, and 429
with numeric Retry-After. Tests record actual request bytes and inspect retained
effect/run outputs, rather than fabricate a successful client return.

This contract is not evidence about any external provider. No service endpoint or
credentials were supplied for live external checks. Before installing a provider
binding, its owner must record that service's response, pagination, mutation,
callback authentication and reconciliation semantics and verify the account
destination. No live external write, app restart or public cutover was performed.
