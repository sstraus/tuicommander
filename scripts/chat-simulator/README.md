# iOS standalone transcript measurement harness

This opt-in harness measures the existing mobile session Chat view. It changes
no application bundle. See the committed
[measurement report](../../docs/evidence/mobile-cli-chat-1770-simulator/README.md)
for accepted numbers and endpoint limits.

## Prerequisites and ownership

Use a linked worktree, an authorized native `make agent-test` instance, Xcode,
XcodeGen, Node and one newly created task-owned iOS simulator. Do not use a
shared device, daily application port or daily application socket. Keep build
outputs, fixture transcripts and result bundles under `~/Gits`. Stop the proxy,
delete its PTY, stop the named agent-test instance and delete the owned device
when finished. Native simulator infrastructure also uses its normal CoreSimulator
storage; obtain authorization for that storage when the task does not grant it.

## Procedure

1. Check both proposed ports with `lsof`, then run the named instance's
   `AGENT_TEST_DRY_RUN=1` before starting it. The recorded run used
   `make agent-test AGENT_TEST_INSTANCE=agent-test-1770 AGENT_TEST_PORT=19876`.
   Follow the repository's native build and background-readiness procedure.
2. Set `EVIDENCE_DIR` to a new directory under `~/Gits/.tmp/results/` and
   `AGENT_TEST_SOCKET` to that named instance's socket from the dry run.
3. Build the inert peer, not a real model agent:
   `clang -fmodules-cache-path="$EVIDENCE_DIR/modules-c" scripts/chat-simulator/synthetic-claude.c -o "$EVIDENCE_DIR/synthetic-claude"`.
4. Run `node scripts/chat-simulator/server.mjs` as an owned long-running process.
   `PROBE_PORT` defaults to 11770 and `ENTRY_COUNT` defaults to 2000. Use a separate
   evidence directory for a smaller setup probe. The server creates the JSONL,
   a Claude-labelled PTY and its exact PID registry under that evidence directory.
   It rejects an unresolved binding or the wrong number of backend updates.
   Wait for its READY line; do not start XCUITest against a failed proxy.
5. Create one new simulator with `xcrun simctl create`, record its UUID, boot it
   and open `http://127.0.0.1:11770/mobile` on that UUID. Never use `booted` as
   the device selector. The recorded run used iPhone 13 / iOS 26.4.
6. Copy `Measurement.swift` and `project.yml` to a new evidence subdirectory,
   then run `xcodegen generate --spec <that-directory>/project.yml`. Keep the
   generated project out of Git. The fixture source stays unchanged.
7. Through the authorized build-slot/background runner, run:

   ```sh
   xcodebuild test \
     -project "$EVIDENCE_DIR/ChatMeasure1770.xcodeproj" \
     -scheme ChatMeasure1770 \
     -destination "platform=iOS Simulator,id=$OWNED_SIMULATOR_UUID" \
     -derivedDataPath "$EVIDENCE_DIR/derived" \
     -resultBundlePath "$EVIDENCE_DIR/install.xcresult" \
     -parallel-testing-enabled NO -collect-test-diagnostics never \
     -only-testing:ChatMeasure1770/Measurement/testInstall \
     CLANG_MODULE_CACHE_PATH="$EVIDENCE_DIR/modules" CODE_SIGNING_ALLOWED=NO
   ```

   Change only the result bundle and test filter to run
   `ChatMeasure1770/Measurement/testMeasure` after installation succeeds.
   Test output must show all five iterations completed. Add to Home Screen
   and Safari's accessibility labels are version-dependent; investigate a
   setup failure rather than accepting partial samples.
8. Generate statistics:
   `node scripts/chat-simulator/summarize.mjs "$EVIDENCE_DIR/events.jsonl"`.
   Export app-only attachments with `xcrun xcresulttool export attachments
   --path <measure.xcresult> --output-path <screenshots> --filter '*.png'`.
9. Delete the session ID in `session.json` through the named instance's socket,
   stop the owned proxy process, run the matching `make agent-test-stop`, then
   `xcrun simctl shutdown <owned-uuid>` and `xcrun simctl delete <owned-uuid>`.
   Verify both owned ports are closed.

The probe also records snapshot fetch-to-headers and body-read intervals, JSON
parse time for the synthetic snapshot, and a post-decode timer interval covering
synchronous replay/render and its queued highlight/follow microtasks. It measures
transcript `scrollHeight` reads separately as a subset of that interval, including
forced style/layout work. These are measurement-only wrappers, not application
instrumentation. Browser clock precision can report a sub-tick parse as zero;
`bytes` is the decoded JavaScript string length, not a wire-byte measurement.
The timer is not a CPU profile and its residual is not an exact render-only time.

The probe observes a trusted click or a visible-document event and two animation
frame opportunities around visible last-entry geometry. Do not call it physical
first paint, OS foreground latency, network latency or a phone benchmark. The
fixture has no live agent work, background transcript growth or memory eviction.
The real-phone story criterion must remain unchecked.
