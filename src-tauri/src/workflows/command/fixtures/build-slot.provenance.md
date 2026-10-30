# Local wrapper contract fixture

Recorded on 2026-10-30 from the installed orchestrator `tools/build/build-slot.sh`.
This is the complete installed wrapper, not a substitute implementation. Tests
use its isolated registry and documented probe overrides. In particular, `--gate`
creates a separate child process group, and freeze checks `BUILD_SLOT_AGENT`.

`build/build-slot.sh` and `coord-state.sh` preserve the installed relative source
layout. `coord-state.sh` was recorded from the same tools checkout. Probe
overrides keep the coordinator registry unused during these tests.
