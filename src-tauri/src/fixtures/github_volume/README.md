# Controlled GitHub client measurement

These files are test-only TLS material for `measure_github_client_fixture_1995`.
The private key is public fixture data, not a GitHub credential. The test uses a
fake token, disables proxy discovery, trusts only this certificate and resolves
`api.github.com` to its loopback TLS receiver. It does not access a live account.

The API bodies are controlled protocol inputs, not captured GitHub responses.
The receiver records requests sent by the production poller and HTTP client.
The workload selects one hot repository, one account and a visible app. It
replays the open/reopen checks calls observed in the separate frontend trace;
it does not drive a browser and Rust backend together.

Run only the explicitly ignored experiment, through the required build admission:

```sh
scripts/with-test-tmp.sh cargo nextest run --manifest-path src-tauri/Cargo.toml \
  --lib -E 'test(measure_github_client_fixture_1995)' --run-ignored all \
  --success-output immediate
```

The default Nextest profile has a test-specific 360-second outer bound, above
the experiment's 300-second hang bound. Routine tests keep their existing bound.
The experiment deliberately waits through two real 60-second poll windows.
It prints one `GITHUB_VOLUME_1995` JSON record with receiver counts and queries.

The certificate was generated with:

```sh
openssl req -x509 -newkey rsa:2048 -nodes \
  -keyout key.pem -out cert.pem -subj /CN=api.github.com \
  -addext subjectAltName=DNS:api.github.com \
  -addext basicConstraints=critical,CA:FALSE -days 3650
```

Counts do not establish GraphQL point cost, secondary-limit safety, a whole-app
upper bound, or the cause of a historical blocked tab. The controlled rate-limit
cases prove zero receiver sends on an immediate retry after exhausted 403/429,
then one send after `retry-after: 1` expires, for REST and GraphQL separately.


## Joined live-account browser observation

`observe_joined_live_github_1995` is a separate, explicitly ignored native
macOS dev harness. It serves the production HTTP router with an isolated test
AppState and configuration directory under the checkout test temp root. The
production `App` runs through Vite, including store hydration, subscriptions,
the GitHub poller, panel actions and CI loading. It does not start a desktop
instance or change installed application settings.

A TLS receiver redirects only the harness client's `api.github.com` socket.
It forwards read-only GETs and GraphQL queries to live GitHub with the
process-local token. It refuses mutations. Response bodies and safe request
metadata are retained under `github-live-1995/run-*/` in `<host temp dir>/tuic-tests/` (`$TUIC_TEST_HOST_TMPDIR` under the wrapper; it outlives the wrapper's per-run root), never inside the checkout; authorization
headers and the token are never logged or written. These captured response
fixtures can contain account/repository data. Do not commit them.

Prerequisites: authenticated `gh`, native build authorization, the existing
agent-browser stealth wrapper, local node dependencies and free localhost
ports 14360/14361. Use the required build-slot wrapper and mbx shim for this
native test executable; it must run on the Mac with the browser. Do not set
`CARGO_TARGET_DIR`.

1. Compile the native lib test executable
   with `scripts/with-test-tmp.sh cargo test --manifest-path src-tauri/Cargo.toml
   --lib --no-run --message-format=json`, retaining complete JSON output in
   a log under `~/Gits/.tmp/`.
2. Set `TUIC_1995_BUILD_LOG` to that log, then run
   `scripts/with-test-tmp.sh python3 scripts/observe-github-volume-1995.py`.
   Run this bounded observation in the background through the managed runner.

The driver obtains `gh auth token` into process memory, starts only the
selected ignored harness test and Vite, and opens a unique named browser tab.
It changes the origin of its own temporary empty repository to the public
`tauri-apps/tauri` repository before App startup, because the product repository
may have no open PRs. It observes idle, then opens the GitHub panel and expands
one real PR with trusted clicks. The opening actions are included in the
second window, followed by 60 seconds mounted. Summary rates use the actual
elapsed seconds, not an assumed exact minute. The driver closes its browser,
Vite, isolated sessions and backend before it exits.

The harness's libtest pass means the bounded driver lifecycle completed;
`summary.json` must also report `success: true` and `backend_exit: 0` to claim
the observation. A live-account or browser setup failure is not a product
regression. The original fake-response experiment remains independent and
requires neither credentials nor a browser.

The receiver records arrival before awaiting upstream responses, so a slow
response crossing a window boundary does not disappear from that window.
Status fields are filled after completion; the final trace distinguishes an
in-flight request from a successful response.
