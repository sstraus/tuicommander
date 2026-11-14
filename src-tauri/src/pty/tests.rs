use super::*;
// Production builds a grid through `AppState::new_vt_log_buffer` so it picks up
// the config; tests that only exercise the grid construct it directly.
use crate::state::VtLogBuffer;
// Session builders live in `test_support` so modules other than this one can
// build a session an agent is deliverable to.
use crate::test_support::agent_session;
#[cfg(unix)]
use crate::test_support::insert_recording_session;
#[cfg(unix)]
use crate::test_support::{RecordingWriter, TtyMode, insert_session_with_writer};

include!("core_tests.rs");
include!("lifecycle_tests.rs");
include!("screen_tests.rs");
include!("injection_tests.rs");
include!("managed_trust_tests.rs");

include!("codex_binding_tests.rs");

include!("question_parity_tests.rs");

include!("grok_question_parity_tests.rs");
include!("grok_hook_test_support.rs");
include!("grok_hook_tests.rs");
include!("hook_side_channel_tests.rs");
include!("ego_question_parity_tests.rs");
include!("codex_question_parity_tests.rs");

include!("codex_tool_tests.rs");
include!("codex_children_tests.rs");

include!("codex_tool_capture_tests.rs");

include!("claude_hook_test_support.rs");
include!("claude_hook_tests.rs");
include!("tool_readout_tests.rs");

include!("ego_events_tests.rs");
