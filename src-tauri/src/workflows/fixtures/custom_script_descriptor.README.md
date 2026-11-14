# Custom script descriptor fixture

`custom_script_descriptor.json` is the exact single-line JSON emitted by
`serde_json` from a retained published `ActivityDescriptor`, captured on rb task
`20261111T110652Z-89920` on 2026-11-11. It describes the same custom calculation
script exercised through the neutral actor in story 007-7ce3. Its pinned
interpreter is `python3`, even when a different host renders the publication.

Capture producer and default drift check:
`workflows::run::neutral_script::descriptor_fixture_tests::published_custom_script_descriptor_matches_shared_renderer_fixture`.
Run the default test with the repository's test-temp wrapper and targeted nextest
filter through the authorized build runner. The explicit capture-only mode sets
`TUIC_CAPTURE_SCRIPT_DESCRIPTOR=1` and `--success-output immediate`; it prints
`TUIC_SCRIPT_DESCRIPTOR=<actual serialized JSON>`. Capture mode generates data
and does not prove that the saved fixture matches. Default mode reads this file
and compares its complete value with the real published backend serializer.

Story 014-5d73 consumes this fixture for generic Designer rendering and visual
proof, with no per-script form or kernel case. No credentials or external
service response is represented here; the script is owned test code.
