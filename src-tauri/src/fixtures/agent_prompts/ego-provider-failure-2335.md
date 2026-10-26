# Recorded ego provider failure — 2335-9c03

Captured on 2026-10-25 from the installed `/Users/stefano.straus/.local/bin/ego` (0.1.4) in a private 41-row × 128-column PTY. Source file: `/Users/stefano.straus/Gits/.tmp/2335-capture/ego-provider-failure-2335.tcap`. SHA-256: `c2592d852dc08b3e2f1aad7a37ce8b5e051a679d9ce3c04baeb0ea58ed7990b0`.

Command: `ego --bare --no-tools --retry 0 --thinking off --model anthropic/claude-haiku-5-5 'Report the word hello.'`. HOME, EGO_HOME, cwd and TMPDIR were private directories below `~/Gits/.tmp/2335-capture`. A dummy non-secret Anthropic key and a loopback Anthropic endpoint were supplied. No credential store was read, no provider was contacted, and no installed binary was replaced.

The local HTTP server sent a real Anthropic message_start, content_block_start and text delta, then closed before the advertised Content-Length and message_stop. The harness itself rendered the partial response and emitted its native final provider-failure notification, progress failure, and idle marker. The capture contains the original output reads and monotonic read timestamps, without text edits or re-chunking. It stops at the final failure notification. The recorder answered the terminal cursor-position query; this terminal handshake input is not retained. No user prompt input was injected: the initial prompt was an argv argument. The recorder then terminated only its own ego process and stopped its own local HTTP server.

The decisive native output is OSC 777 `notify;εgo;<cause>; review the output, then send a new prompt to continue`, followed by OSC 7770 `state=idle`. The cause is `the response stream failed: HTTP response body failed (127.0.0.1), so the model never reported why the generation stopped`. This is a final turn failure, not a countdown or an awaiting-person signal.

Producer source inspected read-only: ego HEAD `52d1746ee981cf06e3352ba84917535c01df951c`, `crates/ego-cli/src/output.rs` ProviderTurnNotices and `crates/ego-cli/src/run/interactive_driver.rs` Announce::signal. The source is an explanation of the recorded bytes, not a replacement for them.
