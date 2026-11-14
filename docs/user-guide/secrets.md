# Private secret forms

An agent can request named username, password, OTP or SSO-link fields with the
`secret` MCP tool. TUICommander opens a separate native window containing only
those fields. The tool receives names and `stored` or `declined`, never values.
SSO HTTP(S) URLs are selectable links with a **Copy URL** button. Click a link
to open it in the system browser; links never open automatically. Other SSO
display values remain plain text.

`request` waits for entry or decline; the form expires after five minutes.
The CLI and MCP bridge keep this request open for 305 seconds. `run` allows
another 120 seconds for the approved child (425 seconds in the transports).
Update both companions when installing this fix. Restart the rebuilt desktop
manually; Rust does not hot-reload.

If no window appears, inspect logs with source `secrets`: `Secret tool dispatched`,
`Opening private secret form`, `Creating private secret window`, then
`Private secret window created` or `Private secret window creation failed`.
These stages do not include field values or the private entry capability.

The form states: **the agent cannot read this value, but a command you approve
can send it anywhere**. Approve only commands you trust. This boundary protects
TUICommander tool results and its entry UI; it is not an OS sandbox against an
agent running as the same user, or protection against arbitrary transformations
or exfiltration by the approved executable.

Username fields remain write-only and are injected into approved child commands,
but do not register output masks. Password and OTP fields remain masked, as do
all task-scoped `put` values. A username matching another stored sensitive value
can still be masked by that sensitive value.

Removing a form value clears it from future output-query masking and new terminal
streams. Existing terminal streams retain learned masks until session teardown;
in-flight approved commands retain their captured masks until completion.
Retained output already sanitized by the PTY reader cannot be restored. Task-scoped
masks in a running child remain until that session ends, including after TTL expiry.

Values exist only in backend memory, shared by name across all callers of that
backend process. They are not scoped to an MCP connection or a TUIC session.
A bridge reconnect or a new MCP session does not clear them. A request that uses
an existing name replaces that value for all callers; a declined request does not.
Removal and normal application exit zeroize values. Restart clears values and
templates; `run` reports `missing` when a requested name has no stored value.
There is no Keychain entry or configuration file for this feature.

`secret action=put scope=<task> ttl=<s> references=[{name, op, account?}]`
fills the same task scope from references instead of values. `op` is an
`op://vault/item/field` reference read
once, at put time, with the 1Password CLI (`op read`, 30 s limit). Each failure
is a typed error (`op` missing, not signed in, locked or authorization not
granted, unknown reference, timeout, invalid) and stores nothing. The reply
carries names only; the value and the reference stay out of logs and config.

`secret action=remove names=["GITHUB_TOKEN"]` returns the requested names and
`status: "removed"` when deletion completes. Removal is idempotent: the same
result is returned when a name was already absent. It does not report prior
presence. Earlier builds returned `status: "missing"` after successful removal;
that meant the names were absent **after** deletion, not that storage had failed.

## Run and consent

`secret action=run names=["GITHUB_TOKEN"] argv=["gh","api","user"] cwd="/absolute/project"`
executes the resolved executable directly, with no shell. Requested values are
injected only into that child's environment. The child inherits only core
path, home, locale and temporary-directory variables. It has no terminal input,
and stdout/stderr are captured in pipes rather than a PTY or a tcap stream.

Commands outside the in-memory allowlist show the exact resolved argv, directory
and secret names for consent. You can retain that exact argv until exit.
Consent also binds the names and canonical working directory. Changing any
argument, name or directory requires fresh consent; placeholders are not used.

Exact-argv consent decides which command may receive values. Options such as
`git -c color.ui=false`, `grep -e`, `ssh -p` and executables such as `env` reach
the consent form; there is no lexical ban on option letters or program names.
Approve only commands you trust: an approved shell or interpreter can run any
code and exfiltrate the values. Structural checks still refuse an empty program,
NUL bytes, more than 128 arguments or an argument over 8192 bytes, and Windows
`.bat`/`.cmd` files, which would run through an implicit `cmd.exe`. The program
is always spawned directly, without a shell. Captured output masks exact values, standard and
URL-safe base64 (also inside Basic-auth payloads), JSON-escaped values,
lower/upper hex and URL/form encodings, including line breaks
inside an echo. Output over 1 MiB per stream or a 120-second timeout is withheld.
No raw output is logged, journalled, or added to a terminal ring.

## HTTP and phone entry

The private window shows a full one-time URL under **Open on a trusted browser
or phone**. Select its text, use **Copy URL**, open the link, or scan its QR code.
The address uses the same network and HTTPS policy as Remote Mobile Connection;
it does not contain the pairing session token. If Remote Access is off or no
network address is available, the window gives setup guidance instead of a link.
Enable it in **Settings → Remote Access**, then request a new form.
Account-vault entry stays native-only and has no phone link.

The URL carries a nonce in its fragment; the form
removes it from history immediately. A nonce is bound to one open form and is
consumed only by a valid submit. Replay and guessed nonces are rejected.
Responses are not cached or frameable.

Browser and phone entry use the existing application router, authentication and
transport. Use HTTPS to protect values in transit. This feature does not enforce
TLS or loopback-only HTTP, and does not isolate entry from application-origin
service workers. A headless host (no desktop window) starts the request through the
same one-time phone link: the tool call waits, Web Push sends the link to the subscribed
phone, and the value goes from the phone to the secret store without passing through the
agent. The link is not sent on the `/events` stream, which local agents can read. If Remote
Access is off, no address is available, or no phone has turned on **Push notifications**
(see [Remote Access](remote-access.md)), the request fails with the setup guidance and
opens no form. The consent form for an exact `secret run` argv uses the same headless path. A closed, expired or consumed link is rejected; an
expired or cancelled request leaves no pending form. Account-vault entry still needs the
native window.

While the private native window is open, TUIC blocks new inspection and
capture calls only: `ui` screenshots, HTML tabs and design mode, `debug`
JavaScript, frame dumps and logs, and upstream MCP calls to servers that can run
on this machine (stdio, loopback, LAN, `.local` or single-label hosts, or an
unknown upstream). `call_tool` wrapping, nested or not, gets the same decision.
Toasts, confirmations, `debug agent_detection`/`sessions`, and HTTP upstreams on
public hosts stay available. Already-running inspection calls are not cancelled
and their results are not withheld. This does not isolate the form from other
programs of the same OS user that capture the screen.
Bootstrap checks the native window identity; the main WebView cannot acquire
its nonce. Inputs are cleared and unmounted before submit begins, and the native
window is destroyed after successful entry. OS-level screen capture outside
TUIC is not controlled by this mechanism.

## Task-scoped controller entry

A controller can call the `secret` MCP tool through the authenticated app
transport without opening a desktop form:

```json
{"action":"put","names":["SERVICE_TOKEN"],"values":["example-value"],"scope":"task-42","ttl":300}
```

`values` is an array in the same order as `names`. Names must be unique valid
environment names, and cannot replace `TUIC_SESSION` or `TUIC_PARENT`. Each value
must be nonempty, contain no NUL, and fit in 16 KiB. At most 16 names are accepted.
`ttl` is a positive whole number of seconds. The reply contains names and
`status: "stored"`, never values. Storage is memory-only. Use HTTPS for remote
entry; the existing app authentication and local-machine trust boundary apply.

Pass the same task ID as `scope` to `agent action=spawn`, or to
`POST /sessions/agent`. Only the child environment receives these values; TUIC
does not add them to arguments, prompt receipts or terminal input. A missing or
expired scope is rejected at launch admission. Each spawn attempt rechecks the
deadline; a reservation delayed past expiry starts without the scoped values.
Scopes do not share values with the form-based `run` store or with other scopes.
A new put replaces that scope atomically, without changing an existing child.

TTL revokes future injection. It cannot revoke an environment the OS already
copied into a running child. That session therefore retains zeroizing masking
material until session teardown, even after expiry or scope replacement.
No masking material or raw values are persisted. Normal exit clears the store.
The `remove` action continues to remove form-store names; task values expire
with their scope.

Scoped output is masked before the terminal grid, VT history, HTTP output,
WebSocket streams, output watchers and diagnostic captures receive it. A possible
secret prefix at a read boundary is withheld until later output disambiguates
it; an unfinished prefix is discarded when the session ends. Oversized ambiguous
runs are redacted rather than retained without a bound. The diagnostic byte ring
also receives sanitized output for these sessions. Captures of scoped output
are therefore not byte-for-byte recordings of the original child stream.
As with approved commands, arbitrary transformations and deliberate exfiltration
by a child are outside this protection.

## Literal terminal reads

Agents read terminals through `session action=output`, which masks
secret-shaped text such as `TEST_TOKEN=not-a-secret`. To let an agent inspect
fake fixture values while it debugs, right-click the terminal tab and choose
**Allow Literal Terminal Reads**. Reads of that session that ask for
`literal=true` then return that text unchanged. Choose **Mask Terminal Reads
Again** to revoke. The grant is only in memory and ends when the session closes
or TUIC restarts. Only the desktop window can grant it.

Values you register through secret forms or task scopes stay masked in literal
reads. Default reads and stored data stay masked.
