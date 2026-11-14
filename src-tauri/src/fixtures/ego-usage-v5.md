# Single-session ego usage v5 fixture

Recorded on 2026-11-10 from the native debug binary built at ego master
`aa5c027c6372a18d85524a50e9c6dda9accca8f5` (includes 1037-5927).
Command: `ego usage --json --session 01a1c787-d41f-7067-9cb5-db90deb9ca8d`.
The command read the live store without modifying it. The JSON is the unmodified
stdout, not a generated approximation. It contains one session with model
`openai-codex/gpt-6.1-sol`, context 102135 and capacity 272000.
