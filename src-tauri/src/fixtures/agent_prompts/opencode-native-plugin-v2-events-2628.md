# Native plugin v2 event capture

- Source: installed OpenCode CLI `opencode v2.2.3` on macOS, 2026-11-01.
- SHA-256: `671ae501295b7c7a81510bb682b912869974e89430bdc38601803e41af4ac9bc`.
- Format: TUICCAP2, 41 rows × 128 columns. Output records retain the PTY read boundaries and monotonic timestamps. All inputs used HTTP, not terminal keystrokes.
- Command: `opencode serve --hostname 127.0.0.1 --port 19828`, in an isolated project and HOME/XDG tree under `~/Gits/.tmp/tuic-2628-capture`. A temporary password was supplied only in the process environment. It is not in this capture.
- Recorder: `capture.py` in that evidence directory. A temporary default `{id, setup}` observer plugin writes each unmodified native event to stdout. This records the real plugin subscription, not manually constructed callback JSON.
- Native Form API: create, answer without another tool, create/cancel, two simultaneous forms, a sibling-session form and independent settlements. These calls use the same native Form service as the question tool. They do not claim a model-generated question.
- Native Permission API: ask, allow once and reject, with the native request and session ids.
- Native execution: a control-only move records success. A temporary context hook raises `capture failure` to record failure. Another context hook awaits a real permission assertion before any model request; `session.interrupt` records the actual user interruption. No model key, model request or paid turn is used.
- Teardown: a pending form, session deletion, permission rejection on service close, form cancellation and location shutdown.
- The fixture contains local throwaway paths and native random ids. No user transcript, credentials or authentication headers are retained.

The earlier named-function plugin does not load on this CLI. Its separate load-failure capture records that failure. The state tests consume these recorded native events through the generated production plugin and assert emitted OSC bytes. The JSON observer is a capture probe, not a second production emitter.

Replay corpus declaration: OpenCode. The filename declares the native agent for
the authoritative terminal replay corpus. The scoped golden records the terminal
parser decisions over these captured diagnostic/API rows; it does not claim a
model-generated question or live OSC delivery from the updated plugin. The
generated-plugin tests separately consume the recorded native event objects.
