# Claude permission accept capture, 2627-110e

Recorded from the installed Claude Code 2.1.314 on macOS, 2026-11-02,
using the Bash tool in default permission mode. Native Python PTY capture
at 30 rows by 100 columns preserves input/output bytes, direction, timing
and read boundaries. The real answer is bare Enter. The accepted tool emits
PostToolUse; receipt metadata and full provenance are documented in
`../claude_hooks_2627/README.md`. No credentials were captured.

SHA-256: `ac3a66dcb5cca0e4ad45041b37b43ecf653df95890b200fe834ef56fcf7aa2d3`

`claude_permission_recorded_answers_retract_on_native_outcome` and the critic
regression `claude_permission_answer_before_tool_result_allows_next_permission`
replay these bytes through production input bookkeeping. The second request
in the critic regression is an explicit tool-id/input perturbation of recorded
native metadata, not a claimed recording of concurrent tool execution.
This capture was relocated from `claude_hooks_2627/accept.tcap` unchanged to
meet the repository's state-detection fixture convention.
