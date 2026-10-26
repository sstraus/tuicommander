# Italian speech investigation — 1996-3cff

## Scope

Boss authorized isolated speech output with microphone capture disabled on 2026-10-25. The worktree used `agent-test-1996`, port `19996`, and its own Unix socket. No daily-instance API, configuration, microphone, or installed executable was accessed.

The target was a throwaway PTY running `/bin/cat` with an explicit test agent identity, not a second AI worker. Model notices and earcons were off. The text was:

> Ciao, questa è una risposta in italiano. Una sola voce legge questa frase.

## Observations

The initial effective configuration reported `speech_engine=edge`. An HTTP save retained that value, selected `language=it` and `speech_edge_voice=it-IT-ElsaNeural`, and added an inactive external command `["say", "-o", "{out}", "{text}"]`.

The subsequent configuration read unexpectedly reported `speech_engine=external`. The saved raw engine remained empty. At `2026-10-25T17:15:42.131817Z`, the actual reply logged:

```text
Speech synthesis start source="dictation" engine="External speech engine" voice="" language=
```

It failed at `17:15:42.683108Z`; `say` reported `Opening output file failed: fmt?`. This is a wrong-engine start, not two successful outputs. The command was introduced by this controlled reproduction; it is not evidence of Boss's historical configuration.

After a second save explicitly changed External to Edge, the same text logged:

```text
2026-10-25T17:16:36.376200Z Speech synthesis start source="dictation" engine="Microsoft Edge" voice="it-IT-ElsaNeural" language=it
2026-10-25T17:16:43.226309Z Speech synthesis complete source="dictation" engine="Microsoft Edge" voice="it-IT-ElsaNeural" language=it succeeded=true
```

The utterance reached `finished`, with no error and one synthesis start for that request. The coordinator relayed Boss's assessment, “audio italiano perfetto”. This is non-reproduction evidence for the historical two-English-voices report.

Hands-free reported `armed=true`, `listening=false`; a push-to-talk request was refused as non-owner. The instance retained non-owner global-input status. Cleanup disarmed the conversation, deleted its PTY and stopped only the named supervisor.

## Proven defect and fix

`get_dictation_config` resolves an empty stored engine into a user-visible engine. The old save applied the caller's delta to the unnormalized disk structure. Because the form's unchanged Edge choice was absent from that delta, the stored engine stayed empty. Adding an inactive command then caused legacy resolution to choose External, ignoring the Italian Edge voice.

The fix normalizes the latest configuration while holding both configuration locks, then applies only the fields the caller changed. This pins the resolved choice without overwriting a concurrent explicit engine selection. Recovery still rechecks the latest document under the lock.

The RED regression is `saving_an_italian_edge_reply_never_selects_a_stale_system_command_1996`. On pre-fix HEAD `5e1f47c686facc3ca0467914a311d918abb052ac` plus the uncommitted diagnostics harness and regression, rb selected one test and failed at the intended assertion: actual `external`, expected `edge`. Compilation took 5m33s; the test took 0.039s. The source was not normalized until after this failure.

Command:

```sh
build-slot.sh --remote cargo nextest run --manifest-path src-tauri/Cargo.toml --features dictation --lib -E 'test(saving_an_italian_edge_reply_never_selects_a_stale_system_command_1996)'
```

The runner reported `RB_BOX_DIRTY=no` and source-tree drift because documentation and adjacent tests were added after synchronization; the regression and pre-fix save implementation were unchanged. No post-fix GREEN test run is claimed: batch integration owns that validation.

## Remaining uncertainty

The historical failing build, its configuration, and its complete tool-call sequence have not been captured. There is no evidence here that two engines start for one request, or that this configuration-save defect caused Boss's original report. Do not close that attribution criterion based on the successful Italian reply or the newly reproduced save defect.

Local raw evidence is retained under `~/Gits/.tmp/results/tuic-1996-evidence/`, `tuic-1996-agent-test.log`, and `tuic-1996-red.log`; rb result `20261025T181323Z-61766`.

## Root-cause and regression audit — 2026-10-26

Audited revision: `461b4f34b43ca1a90e0637f04349187f58496e96`, which includes the configuration fix merged as `f3ab7ad787`. This is a source and retained-evidence audit, not a new audio reproduction or test run.

### What is proven

The reproduced cause is **wrong engine selection during configuration save**, not an Edge failure followed by system speech. The raw `config-after-save.json` records Italian and Elsa alongside `speech_engine=external`; the log records only External starting and then failing. The separate successful request records only Edge starting, with Elsa and Italian. These were different requests, not two engines for one reply.

The normalization mismatch explains the reproduced engine change:

- `src-tauri/src/dictation/commands.rs:2975-2976` resolves the engine on read.
- `commands.rs:690-698` selects External for an unset legacy engine with a nonempty command.
- `commands.rs:3045-3056` now normalizes the locked latest value before applying the form delta. The old implementation did not do this.
- `commands.rs:883-886` deliberately gives External an empty language; its command controls the voice and language. Selecting External can therefore bypass Italian Edge selection. It does not prove the historical command or audible language.

### Is there a second-engine or system fallback path?

No such path was found in the audited reply pipeline:

| Boundary | Decisive source evidence |
|---|---|
| MCP request | `src-tauri/src/mcp_http/mcp_transport_ancillary.rs:954-964` dispatches one `dictation::speak` and returns its result. |
| Desktop/HTTP request | `commands.rs:1525-1530` delegates once to the same `speak`; `src-tauri/src/mcp_http/dictation_routes.rs:231` uses that command. These are alternative entry points, not successive speech calls. |
| Engine construction | `commands.rs:757-805` selects one engine. External and Edge return immediately; missing Pocket assets return an error, not another engine. |
| Language | `commands.rs:866-891` uses the fixed language or the last detected turn language. Unknown Auto language is an error, not an English default. |
| Queue ownership | `commands.rs:1311-1363` holds the speaker slot lock, rebuilds for a changed language, stops the preview, and queues one `say`. |
| Queue worker | `src-tauri/crates/tuic-dictation/src/speaker.rs:408-428` starts one worker for the selected Speech object; `:664-724` synthesizes a dequeued reply once. Synthesis errors mark it failed. There is no retry with another engine. |
| Service rejection | `src-tauri/crates/tuic-dictation/src/speech/rejection.rs:79-103` calls its selected engine once, then returns the result or records an outage. It has no system-command fallback. |
| Edge requests | `src-tauri/crates/tuic-dictation/src/speech/edge.rs:240-256` splits long text into requests with the same voice and propagates errors. Multiple chunks are not multiple engines. |
| External command | `src-tauri/crates/tuic-dictation/src/speech/external.rs:164-199` spawns the configured command only when this adapter was selected. Its arbitrary command implementation is outside the one-engine guarantee. |

The Edge SSML root still says `en-US` (`edge.rs:329`), but its nested voice is explicit (`:330`). The successful Elsa audio under this same format and Boss's Italian assessment do not support attributing the historical English report to this root attribute.

This audit does not rule out multiple independent tool requests, an external command with its own output, or another audio producer. The original failing request sequence, configuration and audio/start trace are absent. None of those alternatives is established as the historical cause.

### What the existing tests actually assert

| Test | Protected boundary | What it does not prove |
|---|---|---|
| `saving_an_italian_edge_reply_never_selects_a_stale_system_command_1996` (`commands.rs:6410-6436`) | Saves the Italian Edge selection with an inactive `say` command; asserts the stored engine stays Edge, language is `it`, and voice is Elsa. Recorded RED on `5e1f47c686` fails with `external` versus `edge`. | Does not call `speak`, count engine starts, or play audio. |
| `one_italian_reply_sends_one_request_with_the_resolved_italian_voice_1996` (`edge.rs:833-860`) | A short reply sends exactly one config and one SSML message to a recorded socket, with one Italian voice. Stale English and empty choices resolve to an Italian default. | Does not pass through configuration save or the Speaker, and cannot observe a separate system-command start. It already exists at `5e1f47c686`; the configuration fix does not alter its production boundary. No pre-fix RED for duplicate starts is recorded. |
| `italian_missing_voice_never_opens_english_or_external_fallback_1954` (`commands.rs:5357-5383`) | With stale `say` configured, missing Italian Pocket assets fail. Once installed, engine identity is the Italian Pocket instance, not the English one. | Does not synthesize a reply or count starts; this test also exists at `5e1f47c686`. |
| `a_reply_is_rendered_and_played` (`speaker.rs:1233-1241`) | One queued reply reaches the fake output as one playback. | Does not select a production engine or assert language/system-command exclusion. |

The story worklog records batch GREEN for the configuration regressions. That evidence applies to the configuration defect, not to a reproduced second-engine bug. The existing tests are useful boundary coverage, but combining their names does not create the requested end-to-end RED.

### Disposition of the two original criteria

Criterion 2 is rejected, not checked: the three proposed historical causes cannot be distinguished with the retained evidence. Wrong engine selection during the isolated save is proven and fixed, but attributing the original two English voices to it would exceed that evidence.

Criterion 3 is rejected, not checked: no second-engine transition was reproduced or found to remove, and no test is recorded failing on `5e1f47c686` for a second start. The real configuration RED is already a separate checked criterion. A new passing characterization must not be presented as that missing RED.

No production change or new test is needed for this disposition. No new Rust, audio, desktop or live-instance execution was performed.
