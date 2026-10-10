use parking_lot::Mutex;
use portable_pty::{CommandBuilder, PtySize, native_pty_system};
use serde::Serialize;
use std::io::{Read, Write};
use std::sync::atomic::{AtomicBool, AtomicI64, AtomicU64, Ordering};
use std::sync::{Arc, LazyLock};
#[cfg(feature = "desktop")]
use tauri::{AppHandle, Emitter, State};
use uuid::Uuid;

use crate::input_line_buffer::{InputAction, InputLineBuffer};
use crate::output_parser::{OutputParser, ParsedEvent};
use crate::state::{
    AppState, ChangedRow, EscapeAwareBuffer, KittyAction, KittyKeyboardState,
    MAX_CONCURRENT_SESSIONS, OUTPUT_RING_BUFFER_CAPACITY, OrchestratorStats, OutputRingBuffer,
    PtyConfig, PtySession, Utf8ReadBuffer, VT_LOG_BUFFER_CAPACITY, strip_kitty_sequences,
};
use crate::worktree::{
    WorktreeConfig, WorktreeResult, create_worktree_with_stale_recovery, remove_worktree_internal,
};

mod injection;
mod lifecycle;
mod screen;

pub(crate) use injection::*;
pub(crate) use lifecycle::*;
pub(crate) use screen::*;

#[cfg(all(test, unix))]
mod child_reaping_tests;
#[cfg(feature = "desktop")]
mod commands;
#[cfg(feature = "desktop")]
pub(crate) use commands::*;

/// Capacity of the per-session raw-byte flight recorder (story 056-7545).
/// 2 MiB ≈ several minutes of heavy agent output — enough to capture the
/// corruption window when a duplication shows up in the wild.
const PTY_RAW_RING_CAP: usize = 2 * 1024 * 1024;

/// One watcher-line batch per this window. Each batch is a Tauri event the
/// WebView main thread must deserialize and dispatch; per-chunk emission
/// starved the event loop under an output flood (`yes`), so keydown never ran
/// and Ctrl+C never reached write_pty.
const WATCHER_LINE_WINDOW: std::time::Duration = std::time::Duration::from_millis(100);

/// Emit early once this much text is batched, so a flood cannot grow the batch
/// without bound.
const WATCHER_BATCH_CAP: usize = 256 * 1024;

/// One "this session produced output" pulse per this window.
///
/// DROPPING PULSES INSIDE THE WINDOW IS CORRECT, and that is what separates this
/// from the `pty-output` throttle deleted in `cda39f31`. That one dropped chunks
/// of a byte stream whose reassembler (`LineBuffer`) carried a partial line
/// across the gap, so a drop spliced the tail of one chunk onto the head of a
/// later one and produced a line that never existed (audit F1). This pulse
/// carries no payload and is idempotent: "output happened" does not accumulate,
/// so N pulses in a window and one pulse in a window mean the same thing.
/// Nothing reassembles it, nothing can be spliced, and there is no state to
/// carry across a dropped pulse.
///
/// So do NOT "fix" this into a coalescer with a buffer behind it. There is
/// nothing for such a buffer to hold.
///
/// One per second, not the old ten: the only consumers are a last-seen timestamp
/// rendered at second resolution and a boolean unread flag that latches on the
/// first pulse.
const ACTIVITY_PULSE_WINDOW: std::time::Duration = std::time::Duration::from_secs(1);

/// Tell this session's frontends that output is flowing. Payload-free by design
/// — see [`ACTIVITY_PULSE_WINDOW`].
///
/// Dual-emitted because there is no bus→window forwarder: the desktop Tauri
/// event and the bus push are two separate writes of one signal. Desktop reads
/// `pty-activity-{id}`, browser/PWA reads the `{"type":"activity"}` frame on the
/// session WebSocket, and both arrive through `subscribePty` on the frontend.
fn emit_pty_activity(state: &AppState, session_id: &str) {
    #[cfg(feature = "desktop")]
    if let Some(app) = state.app_handle.read().as_ref() {
        let _ = app.emit(
            &format!("pty-activity-{session_id}"),
            serde_json::json!({ "session_id": session_id }),
        );
    }
    state.emit_pty_event(crate::state::AppEvent::PtyActivity {
        session_id: session_id.to_string(),
    });
}

/// Rate limiter for [`emit_pty_activity`], owned by the PTY reader thread.
///
/// A struct rather than a bare `Option<Instant>` in the read loop so the
/// throttle can be driven by a test: the loop itself needs a live PTY, but the
/// decision of when a pulse is due does not.
struct ActivityPulse {
    last: Option<std::time::Instant>,
}

impl ActivityPulse {
    fn new() -> Self {
        Self { last: None }
    }

    /// Emit a pulse if one is due. `None` fires immediately, so a session that
    /// emits one short burst and then goes quiet still reports it.
    fn pulse(&mut self, state: &AppState, session_id: &str) {
        if self
            .last
            .is_none_or(|t| t.elapsed() >= ACTIVITY_PULSE_WINDOW)
        {
            emit_pty_activity(state, session_id);
            self.last = Some(std::time::Instant::now());
        }
    }
}

/// Push one batch of assembled lines to the frontends of this session.
fn emit_watcher_lines(
    state: &AppState,
    session_id: &str,
    lines: Vec<crate::output_watchers::WatcherLine>,
) {
    if lines.is_empty() {
        return;
    }
    #[cfg(feature = "desktop")]
    {
        if let Some(app) = state.app_handle.read().as_ref() {
            let _ = app.emit(
                &format!("pty-watcher-lines-{session_id}"),
                crate::output_watchers::WatcherLines {
                    session_id: session_id.to_string(),
                    lines: lines.clone(),
                },
            );
        }
    }
    // The bus carries the same batch to browser/PWA clients over their session
    // WebSocket and to the SSE stream. Those are bounded broadcast channels: a
    // client that lags far enough behind loses events, and nothing replays them.
    // Batching is what keeps that theoretical — a session emits at most ten of
    // these per second regardless of how many lines match — but a browser
    // watcher is best-effort where a desktop one is not.
    state.emit_pty_event(crate::state::AppEvent::PluginWatcherLines {
        session_id: session_id.to_string(),
        lines,
    });
}

/// Assemble the lines of one PTY chunk and match them against the plugin
/// OutputWatchers. Runs on the reader thread: the WebView is woken for the
/// lines that matched instead of ANSI-stripping and regex-testing every line on
/// the thread that paints the terminal (audit F3).
///
/// Rust is the only line assembler. When no watcher is registered the chunk
/// still goes through [`StreamLines`], because a watcher that registers
/// mid-line must still see that line whole — with two assemblers the line was
/// split between them and seen by neither.
fn assemble_watcher_lines(
    state: &AppState,
    session_id: &str,
    chunk: &str,
    lines: &mut crate::output_watchers::StreamLines,
    batcher: &parking_lot::Mutex<crate::output_watchers::WatcherLineBatcher>,
    eof: bool,
) {
    // One read lock for the whole chunk: the compiled set and its "I still need
    // every line" answer must come from the same snapshot, or a line published
    // between the two is matched by neither side.
    let watchers = state.plugin_output_watchers.read();
    if watchers.is_idle() {
        drop(watchers);
        lines.push_discarding(chunk);
        return;
    }
    let needs_all = watchers.needs_all_lines();
    let mut assembled = lines.push(chunk);
    // At end of stream the unterminated tail is a line too: `printf DONE` put
    // `DONE` on the wire and then exited, and nothing is coming to close it.
    if eof && let Some(tail) = lines.flush() {
        assembled.push(tail);
    }
    let mut pending = Vec::new();
    for raw_line in assembled {
        let text = crate::output_watchers::clean_line(&raw_line);
        let matched_ids = watchers.matching_ids(&text);
        if matched_ids.is_empty() && !needs_all {
            continue;
        }
        pending.push(crate::output_watchers::WatcherLine { text, matched_ids });
    }
    drop(watchers);
    if pending.is_empty() {
        return;
    }

    let now = std::time::Instant::now();
    // Emit under the batcher lock: releasing it between take and emit lets the
    // frame ticker interleave and deliver an older tail after a newer batch.
    let mut batch = batcher.lock();
    for line in pending {
        if let Some(due) = batch.push(line, now) {
            emit_watcher_lines(state, session_id, due);
        }
    }
    drop(batch);
}

/// macOS thread QoS for the interactive terminal path.
///
/// `lower_pty_child_priority` nices compiler/test workloads *down*, but on
/// Apple Silicon the scheduler is QoS-band driven: nice only reorders threads
/// *within* a band, so under a saturating `cargo build` our PTY reader, frame
/// ticker, and keystroke-write threads — all at default QoS — still waited
/// behind the compiler's many default-QoS worker threads. Raising our own
/// threads to USER_INTERACTIVE puts the interactive path in a higher band, the
/// lever that keeps typing/echo responsive under load (the native trick AppKit
/// apps like iTerm get for free on the foreground GUI thread).
///
/// macOS-only: Linux/Windows have no per-thread QoS equivalent that helps here
/// (raising priority needs privilege); there we rely on lowering children.
#[cfg(target_os = "macos")]
mod thread_qos {
    use std::os::raw::{c_int, c_uint};

    /// `QOS_CLASS_USER_INTERACTIVE` from `<sys/qos.h>`.
    const QOS_CLASS_USER_INTERACTIVE: c_uint = 0x21;

    unsafe extern "C" {
        fn pthread_set_qos_class_self_np(qos_class: c_uint, relative_priority: c_int) -> c_int;
        // NOT cfg(test): this was test-only when the only reader was a test probe,
        // but `QosBoost` reads the class in every build to restore it afterwards.
        // Leaving the gate on compiled fine under `cargo test` and broke clippy,
        // the release build and the headless `tuic-remote` binary.
        fn pthread_get_qos_class_np(
            thread: libc::pthread_t,
            qos_class: *mut c_uint,
            relative_priority: *mut c_int,
        ) -> c_int;
    }

    /// Raise the calling thread to USER_INTERACTIVE QoS. Best-effort: a failure
    /// leaves the thread at its current QoS (degraded latency, not broken), so
    /// the non-zero return is intentionally ignored.
    pub(super) fn raise_self_to_user_interactive() {
        set_self_qos(QOS_CLASS_USER_INTERACTIVE, 0);
    }

    fn set_self_qos(class: c_uint, relative_priority: c_int) {
        // SAFETY: extern "C" call with scalar args; affects only the calling thread.
        unsafe {
            pthread_set_qos_class_self_np(class, relative_priority);
        }
    }

    /// Read back the calling thread's QoS class and relative priority.
    fn current_qos() -> (c_uint, c_int) {
        let mut class: c_uint = 0;
        let mut rel: c_int = 0;
        // SAFETY: out-params point to valid stack locals; pthread_self is always valid.
        unsafe {
            pthread_get_qos_class_np(libc::pthread_self(), &mut class, &mut rel);
        }
        (class, rel)
    }

    #[cfg(test)]
    pub(super) fn current_qos_class() -> c_uint {
        current_qos().0
    }

    /// The full pair, so a test can prove the restore is exact rather than
    /// merely landing back in the same band.
    #[cfg(test)]
    pub(super) fn current_qos_pair() -> (c_uint, c_int) {
        current_qos()
    }

    /// Raises the calling thread to USER_INTERACTIVE for as long as it lives, then
    /// puts the thread back exactly where it was found.
    ///
    /// For a thread the process owns end to end — the PTY reader, the frame ticker
    /// — the plain raise is right and this is unnecessary. It exists for work that
    /// runs on a *borrowed* thread: a keystroke served by the tokio blocking pool
    /// hands its thread back when it is done, and an unrestored bump leaves that
    /// thread in the interactive band for whatever unrelated blocking work lands on
    /// it next. A few keystrokes and the pool the terminal competes against is the
    /// pool the terminal promoted.
    pub(super) struct QosBoost {
        previous: (c_uint, c_int),
    }

    impl QosBoost {
        pub(super) fn user_interactive() -> Self {
            let previous = current_qos();
            raise_self_to_user_interactive();
            Self { previous }
        }
    }

    impl Drop for QosBoost {
        fn drop(&mut self) {
            set_self_qos(self.previous.0, self.previous.1);
        }
    }
}

/// Raise the calling thread's scheduling QoS for the interactive terminal I/O
/// path. macOS-only (see [`thread_qos`]); a no-op on other platforms.
#[cfg(target_os = "macos")]
fn raise_thread_for_interactive_io() {
    thread_qos::raise_self_to_user_interactive();
}

#[cfg(not(target_os = "macos"))]
fn raise_thread_for_interactive_io() {}

/// Raise the calling thread for the duration of the returned guard, then put it
/// back. Use this — never the bare raise — on a thread the caller does not own,
/// such as one borrowed from the tokio blocking pool. See [`thread_qos::QosBoost`].
#[cfg(target_os = "macos")]
#[must_use = "the QoS is restored when the guard drops; dropping it immediately bumps nothing"]
fn interactive_io_boost() -> thread_qos::QosBoost {
    thread_qos::QosBoost::user_interactive()
}

/// The guard the other platforms have nothing to restore into. It exists so the
/// call reads identically everywhere and `#[must_use]` keeps meaning "hold this",
/// rather than degrading to a unit that a caller binds and clippy rejects.
#[cfg(not(target_os = "macos"))]
pub(crate) struct QosBoost;

#[cfg(not(target_os = "macos"))]
#[must_use = "the QoS is restored when the guard drops; dropping it immediately bumps nothing"]
fn interactive_io_boost() -> QosBoost {
    QosBoost
}

// Re-export from chrome module for use by this module and tests.
use crate::chrome::is_chrome_row;
use alacritty_terminal::vte;

use crate::chrome::{is_prompt_line, is_separator_line};

/// How many bottom screen rows to check when verifying a question candidate.
/// Wide enough to cover agent footer layouts (mode line, spinner, Wiz HUD,
/// suggest/intent blocks, trailing disclaimer text) that push the actual
/// question several rows above the prompt box.
const SCREEN_VERIFY_ROWS: usize = 20;

struct TimerIdleTransition {
    transitioned: bool,
    force_cleared_subtasks: bool,
    /// Read only by tests: it separates a transition the screen confirmed from
    /// one the silence timer forced, which `transitioned` alone cannot. The
    /// `cfg_attr` keeps the lint armed in test builds, so the field still goes
    /// dead-code the moment the last assertion on it disappears.
    #[cfg_attr(not(test), allow(dead_code))]
    screen_confirms_idle: bool,
    // Returned provenance is inspected by regression tests; production uses the transition result.
    #[cfg_attr(
        not(test),
        expect(dead_code, reason = "test-observed transition provenance")
    )]
    evidence: Option<Evidence>,
}

fn try_timer_idle_transition(
    state: &AppState,
    silence: &Arc<Mutex<SilenceState>>,
    session_id: &str,
    screen_activity: AgentScreenActivity,
    agent_type: Option<&str>,
    evidence_turn_epoch: Option<u64>,
) -> TimerIdleTransition {
    let lifecycle = Arc::clone(silence);
    // Read the foreground process group before taking the lifecycle lock: the
    // probe locks the PtySession, which the reader thread holds while it takes
    // this same SilenceState.
    let nested_prompt = agent_type.is_none() && explicit_busy_is_a_nested_prompt(state, session_id);
    let (transitioned, force_cleared_subtasks, screen_confirms_idle, evidence, parent_dispatch) = {
        let mut silence = silence.lock();
        if evidence_turn_epoch.is_some_and(|observed| {
            state
                .session_maps
                .session_states
                .get(session_id)
                .is_some_and(|session| session.turn_epoch != observed)
        }) {
            return TimerIdleTransition {
                transitioned: false,
                force_cleared_subtasks: false,
                screen_confirms_idle: false,
                evidence: None,
            };
        }

        let idle_was_confirmed = silence.idle_confirmed();
        let submission_stale = screen_activity == AgentScreenActivity::Unknown
            && agent_type == Some("claude")
            && silence.unacknowledged_submission_is_stale();
        let protocol_stale =
            screen_activity == AgentScreenActivity::Ready && silence.protocol_busy_is_stale();
        let screen_confirms_idle = match screen_activity {
            AgentScreenActivity::Ready if protocol_stale => {
                tracing::warn!(
                    session_id = %session_id,
                    activity_source = "protocol-stale",
                    "authoritative busy signal became stale on a stable ready screen"
                );
                silence
                    .evidence
                    .force_idle(EvidenceRank::Process, "protocol-stale");
                true
            }
            AgentScreenActivity::Ready => silence.note_ready_screen(),
            AgentScreenActivity::Interrupted => silence.note_interrupted_screen(),
            AgentScreenActivity::Unknown => {
                silence.note_unknown_screen();
                false
            }
            AgentScreenActivity::Working => false,
        };
        let is_busy = state
            .session_maps
            .shell_states
            .get(session_id)
            .is_some_and(|atom| atom.load(std::sync::atomic::Ordering::Acquire) == SHELL_BUSY);
        if !is_busy && screen_confirms_idle && !idle_was_confirmed {
            tracing::debug!(
                session_id,
                queued_commands = queued_command_count(state, session_id),
                "Ready confirmed after shell became idle"
            );
        }
        if !is_busy || screen_activity == AgentScreenActivity::Working {
            return TimerIdleTransition {
                transitioned: false,
                force_cleared_subtasks: false,
                screen_confirms_idle,
                evidence: None,
            };
        }

        let hold_for_ready_confirmation = matches!(
            screen_activity,
            AgentScreenActivity::Ready | AgentScreenActivity::Interrupted
        ) && !screen_confirms_idle;
        let probe = if screen_confirms_idle {
            foreground_probe(state, session_id, &lifecycle)
        } else {
            ForegroundProbe::Open
        };
        let decision =
            if submission_stale || (screen_confirms_idle && probe != ForegroundProbe::Pending) {
                IdleDecision::yes(evidence_turn_epoch)
            } else if screen_confirms_idle
                || (silence.explicit_busy() && !nested_prompt)
                || hold_for_ready_confirmation
                || silence.is_api_retry_active()
            {
                IdleDecision::NO
            } else {
                should_transition_idle(state, session_id)
            };
        if !decision.should_transition {
            return TimerIdleTransition {
                transitioned: false,
                force_cleared_subtasks: false,
                screen_confirms_idle,
                evidence: None,
            };
        }
        if probe == ForegroundProbe::Quiet {
            // The probe looked at the process table and found nothing left
            // running under the agent. That outranks the ready screen that
            // asked for it, so the turn closes on `activity_source=process`
            // rather than on `agent-ready-screen` (#771-4733) — a reader can
            // now tell a close backed by a live process observation from a
            // `protocol-stale` give-up and from a bare silence timeout.
            //
            // `force_idle`, like the two screen adapters: the `else if` chain
            // above already decided this transition is allowed, so the generic
            // busy-rank gate in `record_idle` must not re-reject it.
            silence
                .evidence
                .force_idle(EvidenceRank::Process, "process");
        }
        if submission_stale {
            silence
                .evidence
                .force_idle(EvidenceRank::Process, "submission-stale");
        } else if !screen_confirms_idle {
            // Silence-timeout evidence, forced in regardless of rank: the
            // `else if` chain above (mirroring the old checks exactly, incl.
            // `nested_prompt`) already decided this is allowed, so the generic
            // busy-rank gate in `record_idle` must not re-reject it.
            let source = if agent_type.is_none() {
                "silence-timeout-shell"
            } else {
                "silence-timeout-agent"
            };
            silence.evidence.force_idle(EvidenceRank::Silence, source);
        }
        let evidence = match decide(&silence.evidence, true, std::time::Instant::now()) {
            Some(Transition::ToIdle(evidence)) => Some(evidence),
            _ => None,
        };
        let (transitioned, parent_dispatch) = try_shell_transition_locked(
            ShellTransitionRequest {
                state,
                session_id,
                expected: SHELL_BUSY,
                new: SHELL_IDLE,
                notify_parent: true,
                observed_turn_epoch: decision.turn_epoch,
            },
            Some(&mut silence),
            || {},
        );
        (
            transitioned,
            decision.force_cleared_subtasks,
            screen_confirms_idle,
            evidence,
            parent_dispatch,
        )
    };
    if let Some(dispatch) = parent_dispatch {
        dispatch_parent_lifecycle(state, dispatch);
    }
    TimerIdleTransition {
        transitioned,
        force_cleared_subtasks,
        screen_confirms_idle,
        evidence,
    }
}

fn completion_adjusted_screen_activity(
    state: &AppState,
    silence: &Arc<Mutex<SilenceState>>,
    session_id: &str,
    screen_activity: AgentScreenActivity,
) -> AgentScreenActivity {
    if screen_activity != AgentScreenActivity::Working {
        return screen_activity;
    }
    // Claude may declare completion before running a blocking Stop hook, while
    // keeping an active phase marker on screen for minutes. That live marker
    // reopens the same turn in `apply_working_evidence`; only adapters whose
    // completed screen can retain a stale Working row need this downgrade.
    if state
        .session_maps
        .session_states
        .get(session_id)
        .is_some_and(|session| session.agent_type.as_deref() == Some("claude"))
    {
        return AgentScreenActivity::Working;
    }
    let silence = silence.lock();
    if silence
        .evidence
        .busy
        .is_some_and(|busy| busy.rank > EvidenceRank::Screen)
    {
        return AgentScreenActivity::Working;
    }
    if state
        .session_maps
        .session_states
        .get(session_id)
        .is_some_and(|session| silence.completion_declared_for_epoch(session.turn_epoch))
    {
        AgentScreenActivity::Ready
    } else {
        screen_activity
    }
}

/// Spawn the silence-detection timer thread. Shared by desktop and headless readers.
///
/// Two strategies run in priority order:
/// 1. **Screen-based**: read the terminal screen, find the last chat line above the
///    prompt box (delimited by two separator lines), check if it ends with `?`.
/// 2. **Chunk-based fallback**: use `check_silence()` with `pending_question_line`
///    for agents that don't have a prompt box (plain shell, etc.).
fn spawn_silence_timer(
    silence: Arc<Mutex<SilenceState>>,
    running: Arc<AtomicBool>,
    session_id: String,
    state: Arc<AppState>,
) {
    tokio::spawn(async move {
        // Track the inter-tick gap in WALL-CLOCK time, not `Instant`.
        // `should_transition_idle` measures idle elapsed against the wall clock
        // (`last_output_ms` is epoch millis), so sleep detection MUST use the
        // same clock. On macOS, `Instant` (mach_absolute_time) does not advance
        // while the system is asleep — an Instant-based gap stays ~1s across a
        // lid-close sleep and never detects the wake, letting the wall-clock
        // jump fire a false busy→idle (completion sound) on every terminal.
        let mut last_tick_ms = now_epoch_ms();
        while running.load(Ordering::Relaxed) {
            tokio::time::sleep(SILENCE_CHECK_INTERVAL).await;
            if !running.load(Ordering::Relaxed) {
                break;
            }

            // Sleep-wake detection: if the wall-clock gap between consecutive
            // ticks is much larger than SILENCE_CHECK_INTERVAL, the system was
            // asleep (lid closed) or the clock stepped. Reset timestamps so
            // stale elapsed times don't trigger false idle transitions /
            // completion sounds.
            let epoch_now = now_epoch_ms();
            let tick_gap = std::time::Duration::from_millis(epoch_now.saturating_sub(last_tick_ms));
            last_tick_ms = epoch_now;
            if tick_gap >= SLEEP_WAKE_GAP {
                tracing::info!(
                    source = "silence_timer",
                    session_id = %session_id,
                    gap_secs = tick_gap.as_secs(),
                    "Sleep-wake detected — resetting timestamps"
                );
                if let Some(ts) = state.session_maps.last_output_ms.get(&session_id) {
                    ts.store(epoch_now, std::sync::atomic::Ordering::Release);
                }
                {
                    let mut sl = silence.lock();
                    let now = std::time::Instant::now();
                    sl.last_output_at = now;
                    sl.last_chunk_at = now;
                }
                continue;
            }

            if orchestrator_recipient_for_pty(&state, &session_id)
                .and_then(|recipient| state.orchestrator_wake_needed_through(&recipient))
                .is_some()
            {
                silence.lock().expire_orchestrator_notice_uncertainty();
            }

            // Discovery must not depend on a frontend polling the desktop IPC.
            // Headless/manual launches need the same parser and mail eligibility.
            refresh_session_agent(&state, &session_id);

            // Reconcile high-confidence screen evidence before the silence
            // fallback. Working here means Codex's presence-based status line
            // (the only screen classifier that returns Working, #446-596f); it
            // runs regardless of current state so it repairs an already-false-
            // idle session instead of merely keeping a pre-existing BUSY alive.
            // Claude/Gemini/Aider BUSY is movement-driven in the reader.
            let idle_evidence_turn_epoch = state
                .session_maps
                .session_states
                .get(&session_id)
                .map(|session| session.turn_epoch);
            let agent_type = state
                .session_maps
                .session_states
                .get(&session_id)
                .and_then(|s| s.agent_type.clone());
            // Reused from the reader chunk path (#744-138c) instead of a fresh
            // `detect_agent_screen_activity` call: with no chunk having
            // arrived since the last classification, the screen the function
            // would see is byte-identical, so the cached verdict is the same
            // answer, not a stale one. Keeps the classifier to at most one
            // call per session per `SILENCE_CHECK_INTERVAL` (previously two:
            // one here, one in the reader).
            let screen_activity = silence.lock().cached_screen_activity;
            let screen_activity =
                completion_adjusted_screen_activity(&state, &silence, &session_id, screen_activity);
            let tracked_background_work = state
                .session_maps
                .session_states
                .get(&session_id)
                .is_some_and(|session| session.background_work);
            let shell_is_busy = state
                .session_maps
                .shell_states
                .get(&session_id)
                .is_some_and(|shell| shell.load(Ordering::Acquire) == SHELL_BUSY);
            if tracked_background_work
                || (shell_is_busy
                    && matches!(
                        screen_activity,
                        AgentScreenActivity::Ready | AgentScreenActivity::Interrupted
                    ))
            {
                refresh_background_work(&state, &session_id);
            }
            if screen_activity == AgentScreenActivity::Working {
                apply_working_evidence(&state, &silence, &session_id, epoch_now, "working-screen");
            } else {
                // Evidence mutation, silence decision, and BUSY→IDLE CAS share
                // one lifecycle transaction. A new submitted epoch therefore
                // wins before any stale Ready/Interrupted/Unknown evidence can
                // alter its SilenceState.
                let transition = try_timer_idle_transition(
                    &state,
                    &silence,
                    &session_id,
                    screen_activity,
                    agent_type.as_deref(),
                    idle_evidence_turn_epoch,
                );
                if transition.transitioned {
                    if transition.force_cleared_subtasks {
                        emit_active_subtasks(&state, &session_id, 0, "");
                    }
                    if let Some(vt) = state.grid.vt_log_buffers.get(&session_id) {
                        vt.lock().process(b"\x1b[?25h");
                    }
                    emit_shell_state(&state, &session_id, "idle");
                    reevaluate_orchestrator_mail_wake(&state, &session_id);
                    flush_pending_injections(&state, &session_id);
                    record_inferred_outcome_if_no_osc133(&state, &session_id);
                } else if transition.screen_confirms_idle
                    && !shell_is_busy
                    && queued_command_count(&state, &session_id) > 0
                {
                    // Silence can mark the shell idle before the Ready screen
                    // stabilizes. That later confirmation has no second shell
                    // edge, so it must retry the existing self-guarded flush.
                    flush_pending_injections(&state, &session_id);
                }
            }

            // Update startup grace state (checks if output has settled).
            {
                let mut sl = silence.lock();
                sl.check_startup_settle();
                if sl.is_startup_grace() {
                    continue; // Still in startup burst — suppress question detection
                }
            }

            // Tool-error turn-end: `Error: Exit code N` + silence = fire playError.
            // Checked before question detection — a tool error is not a question.
            if let Some(text) = silence.lock().check_tool_error() {
                let parsed = ParsedEvent::ToolError { matched_text: text };
                if let Ok(json) = serde_json::to_value(&parsed) {
                    #[cfg(feature = "desktop")]
                    if let Some(app) = state.app_handle.read().as_ref() {
                        let _ = app.emit(&format!("pty-parsed-{session_id}"), &json);
                    }
                    state.emit_pty_event(crate::state::AppEvent::PtyParsed {
                        session_id: session_id.clone(),
                        parsed: json.into(),
                    });
                }
            }

            // Suggest turn-end: drain parked `suggest:` items once the shell
            // has transitioned to IDLE. The reader parks them at parse time
            // (see write_pty's emit loop); gating the drain on shell_state ==
            // IDLE makes the frontend's `pendingSuggest` race impossible —
            // the event physically cannot reach the UI before idle.
            emit_pending_suggest_if_idle(&state, &silence, &session_id);
            emit_open_intent_if_idle(&state, &silence, &session_id);

            // Retraction is a reconciliation loop, not part of the one-shot
            // question-emission gate. Once a low-confidence wait has fired,
            // `question_already_emitted` is true by design; gating this check on
            // `is_silent()` made the documented backstop unreachable forever.
            let quiet_for_retraction = silence.lock().is_quiet_for_question_retraction();
            if quiet_for_retraction {
                let active_question =
                    state
                        .session_maps
                        .session_states
                        .get(&session_id)
                        .and_then(|s| {
                            (s.awaiting_input && !s.question_confident && s.choice_prompt.is_none())
                                .then(|| s.question_text.clone())
                                .flatten()
                        });
                if let Some(active_question) = active_question {
                    let still_current =
                        state
                            .grid
                            .vt_log_buffers
                            .get(&session_id)
                            .is_some_and(|vt| {
                                match current_chat_question(&vt.lock().screen_rows()) {
                                    CurrentChatQuestion::PromptAnchored(Some(current)) => {
                                        current.trim() == active_question.trim()
                                    }
                                    CurrentChatQuestion::PromptAnchored(None) => false,
                                    CurrentChatQuestion::NoPromptAnchor => false,
                                }
                            });
                    if !still_current {
                        emit_question_cleared_if_stale(&state, &session_id);
                    }
                }
            }

            // Check temporal conditions first (shared by both strategies).
            // Snapshot the epoch while holding the lifecycle mutex shared with
            // `note_submitted_input`. If input begins after this point, the
            // accumulator rejects the old-epoch Question; if it began before,
            // `suppress_user_input` makes `is_silent` false.
            let (is_silent, question_turn_epoch) = {
                let sl = silence.lock();
                let epoch = state
                    .session_maps
                    .session_states
                    .get(&session_id)
                    .map(|session| session.turn_epoch)
                    .unwrap_or(0);
                (sl.is_silent(), epoch)
            };
            if !is_silent {
                continue;
            }

            // Strategy 1: screen-based — walk upward from the prompt box looking
            // for the most recent plausible question within a bounded window.
            // This is robust to trailing non-question text between the question
            // and the prompt box (e.g. "(stopping here — waiting for your answer)").
            let current_question = state.grid.vt_log_buffers.get(&session_id).map(|vt| {
                let rows = vt.lock().screen_rows();
                let question = current_chat_question(&rows);
                tracing::trace!(
                    session_id = %session_id,
                    found = matches!(&question, CurrentChatQuestion::PromptAnchored(Some(_))),
                    "DIAG silence_timer: screen strategy"
                );
                question
            });

            // Strategy 2: chunk-based fallback — pending_question_line + screen verify.
            let prompt_text = match current_question {
                Some(CurrentChatQuestion::PromptAnchored(Some(line))) => line,
                // A current prompt exists and later non-question content is above
                // the historical candidate. That is decisive turn-order evidence:
                // never let the fallback dig through it to resurrect an old `?`.
                Some(CurrentChatQuestion::PromptAnchored(None)) => {
                    silence.lock().clear_stale_question();
                    emit_question_cleared_if_stale(&state, &session_id);
                    continue;
                }
                Some(CurrentChatQuestion::NoPromptAnchor) | None => {
                    let question = silence.lock().check_silence();
                    match question {
                        Some(ref text) => {
                            let on_screen = state
                                .grid
                                .vt_log_buffers
                                .get(&session_id)
                                .map(|vt| {
                                    verify_question_on_screen(
                                        &vt.lock().screen_rows(),
                                        text,
                                        SCREEN_VERIFY_ROWS,
                                    )
                                })
                                .unwrap_or(false);
                            tracing::debug!(
                                session_id = %session_id,
                                question = %text,
                                on_screen = on_screen,
                                "silence_timer: chunk fallback"
                            );
                            if !on_screen {
                                silence.lock().clear_stale_question();
                                emit_question_cleared_if_stale(&state, &session_id);
                                continue;
                            }
                            text.clone()
                        }
                        None => {
                            tracing::trace!(
                                session_id = %session_id,
                                "silence_timer: silent but no question candidate"
                            );
                            emit_question_cleared_if_stale(&state, &session_id);
                            continue;
                        }
                    }
                }
            };

            // Suppress heuristics only after a hook marker was observed at
            // runtime. A persisted config flag alone can be stale after a failed
            // install or an agent-version change.
            let hook_configured = state
                .session_maps
                .session_states
                .get(&session_id)
                .map(|s| s.hook_instrumented)
                .unwrap_or(false);
            if hook_configured && silence.lock().hook_state_seen {
                silence.lock().clear_stale_question();
                continue;
            }

            // Emit question event.
            silence.lock().mark_emitted(&prompt_text);
            let parsed = ParsedEvent::Question {
                prompt_text: prompt_text.clone(),
                confident: false,
            };
            if let Ok(mut json) = serde_json::to_value(&parsed) {
                if let Some(object) = json.as_object_mut() {
                    object.insert("_turn_epoch".to_string(), question_turn_epoch.into());
                }
                #[cfg(feature = "desktop")]
                if let Some(app) = state.app_handle.read().as_ref() {
                    let _ = app.emit(&format!("pty-parsed-{session_id}"), &json);
                }
                state.emit_pty_event(crate::state::AppEvent::PtyParsed {
                    session_id: session_id.clone(),
                    parsed: json.into(),
                });
            }
        }
    });
}

// ---------------------------------------------------------------------------
// ChunkProcessor: shared output processing logic for desktop & headless readers
// ---------------------------------------------------------------------------

/// Extract a clean prompt from grok's "⚠ Action Required" OSC 0 title.
/// Strips the leading warning / "Action Required" marker, the spinner braille
/// frame, and separators, leaving the human-readable action description.
/// `"⚠ Action Required - ⠙ - Running: echo x - Execute Shell …"` → `"Running: echo x - Execute Shell …"`.
fn clean_action_required_title(title: &str) -> String {
    let after = title.split("Action Required").nth(1).unwrap_or(title);
    let cleaned = after
        .trim_start_matches(|c: char| {
            c == '-' || c == ' ' || ('\u{2800}'..='\u{28FF}').contains(&c)
        })
        .trim();
    if cleaned.is_empty() {
        "grok is awaiting approval".to_string()
    } else {
        cleaned.to_string()
    }
}

/// Map a TUIC `state=` verb to the awaiting-input `ParsedEvent` it implies.
///
/// busy/idle shell transitions are handled by `handle_tuic_state`; this covers
/// only the separate `awaiting_input` field, which is driven by Question /
/// UserInput events in `state.rs`:
/// - `awaiting` → confident `Question` (sets `awaiting_input` + `question_confident`)
/// - `busy`     → `UserInput` clear (hook busy is authoritative — clears an awaiting
///   set by a prior `PreToolUse(AskUserQuestion)`; empty content never overwrites
///   `last_prompt`). Fires on every tool call too, so it carries no prompt row
///   (`line = -1`).
/// - `prompt`   → same clear, sent only by the user-prompt-submit hook, and the only
///   one that carries the prompt row for the scrollbar marker
/// - anything else (incl. `idle`, unknown) → `None`
fn tuic_state_awaiting_event(payload: &str, line: i64) -> Option<ParsedEvent> {
    match payload {
        "awaiting" => Some(ParsedEvent::Question {
            prompt_text: String::new(),
            confident: true,
        }),
        "busy" => Some(ParsedEvent::UserInput {
            content: String::new(),
            line: -1,
        }),
        // `line` is the absolute prompt row (history_size + cursor row) at the
        // submit — the row the user's prompt sits on. Carried so the frontend can
        // mark user-prompt lines on the scrollbar.
        "prompt" => Some(ParsedEvent::UserInput {
            content: String::new(),
            line,
        }),
        _ => None,
    }
}

/// Whether `agent_type`'s config enables native-hook instrumentation. Resolved
/// once when the session's agent type becomes known (config changes apply on the
/// next agent launch, matching when the hooks themselves take effect).
pub(crate) fn hook_instrumented_for(
    agents: &crate::config::AgentsConfig,
    agent_type: Option<&str>,
) -> bool {
    let Some(agent_type) = agent_type else {
        return false;
    };
    // ego emits OSC 7770 natively: there is no hook to install or disable.
    if agent_type == "ego" {
        return true;
    }
    let settings = agents.agents.get(agent_type);
    if matches!(agent_type, "claude" | "codex") {
        settings
            .and_then(|s| s.native_status_signals)
            .unwrap_or(true)
    } else {
        settings
            .and_then(|s| s.hook_instrumentation)
            .unwrap_or(false)
    }
}

/// Events carried by the RAW byte stream, before any VT rendering — sequences
/// the vt100/alacritty parsers consume and that are therefore invisible in the
/// clean rows every other parser reads.
///
/// Deliberately separate from the clean-row parsers: everything appended here
/// skips `suppress_heuristic_question`. That filter exists to stop regex
/// *guesses* from double-firing against the hook's `state=awaiting`; the OSC 777
/// parser accepts only unambiguous permission or approval wording. Claude's
/// generic idle notification is not a question. Plan and skill Ink pickers
/// without a hook state use the visible footer's presence recovery instead.
///
/// Shared with the fixture harness (`awaiting_signal_fixtures`) so a test can
/// never assert against a composition that production does not run.
fn raw_stream_events(carry: &mut String, data: &str, out: &mut Vec<ParsedEvent>) {
    let combined = if carry.is_empty() {
        std::borrow::Cow::Borrowed(data)
    } else {
        let mut joined = std::mem::take(carry);
        joined.push_str(data);
        std::borrow::Cow::Owned(joined)
    };
    if let Some(evt) = crate::output_parser::parse_osc94(&combined) {
        out.push(evt);
    }
    out.extend(crate::output_parser::parse_osc777_notifies(&combined));
    *carry = unterminated_osc_tail(&combined);
}

/// Longest suffix of a chunk that opens an OSC sequence but never closes it.
///
/// Only an *unterminated* tail is carried, so a sequence can be matched once and
/// only once: a complete one leaves nothing behind. A tail longer than
/// [`MAX_RAW_CARRY`] is dropped rather than grown without bound — at that length
/// it is not a notification, it is a payload we do not parse (or a stream that
/// never terminates it), and holding it would pin memory for the session.
fn unterminated_osc_tail(data: &str) -> String {
    // Anchored on the last ESC, not on the last `ESC]`: a read can end on the
    // ESC itself, with the `]` arriving in the next chunk. Anchoring on the
    // pair dropped that ESC and left the next chunk starting at `]777;…`,
    // which is no longer an escape sequence at all.
    let Some(start) = data.rfind('\x1b') else {
        return String::new();
    };
    let tail = &data[start..];
    // A lone trailing ESC may still become an OSC introducer.
    if tail == "\x1b" {
        return tail.to_string();
    }
    // Anything else that is not an OSC introducer (CSI, ST, charset select) is
    // consumed by the VT parser, not by us.
    if !tail.starts_with("\x1b]") {
        return String::new();
    }
    // BEL, or ST (ESC backslash) — the ESC of an ST is not the introducer's own.
    if tail.contains('\x07') || tail[1..].contains("\x1b\\") {
        return String::new();
    }
    if tail.len() > MAX_RAW_CARRY {
        return String::new();
    }
    tail.to_string()
}

/// Cap for [`unterminated_osc_tail`]. Comfortably above any OSC we parse: the
/// longest observed notify body is under 60 bytes.
const MAX_RAW_CARRY: usize = 512;

/// Record an `intent:` marker as a Progress journal entry.
///
/// Silent on every skip. Three of them are ordinary and none is the user's
/// problem: collection is off, the session is not inside a registered project,
/// or the text does not survive validation. A missing project is deliberately
/// NOT resolved to the focused UI repository — that files one agent's work
/// under whatever the human happened to be looking at.
///
/// The write is synchronous. This runs on the PTY reader thread, which is not
/// the async executor, and an intent arrives a few times a minute against a
/// sub-millisecond WAL insert.
fn record_intent_in_journal(state: &AppState, session_id: &str, text: &str) -> bool {
    let (agent_type, agent_name) = crate::progress::session_identity(state, session_id);
    if !crate::progress::progress_tracking_enabled(state, agent_type.as_deref()) {
        return true;
    }
    let Some(project) = crate::progress::project_for_session(state, session_id) else {
        return true;
    };
    match crate::progress::record_intent(
        state,
        Some(&project),
        text,
        agent_name,
        agent_type.as_deref(),
        Some(session_id),
    ) {
        Ok(entry) => {
            crate::mcp_http::mcp_transport::emit_progress_entry(state, entry);
            true
        }
        Err(error) => {
            tracing::warn!(
            source = "progress",
            session_id = %session_id,
            error = %error,
            "intent: not recorded in the Progress journal"
            );
            false
        }
    }
}

fn publish_intent_event(state: &AppState, session_id: &str, event: &ParsedEvent, turn_epoch: u64) {
    let ParsedEvent::Intent { text, .. } = event else {
        return;
    };
    if !record_intent_in_journal(state, session_id, text) {
        return;
    }
    state.note_marker(session_id, crate::state::MarkerKind::Intent);
    if let Ok(mut json) = serde_json::to_value(event) {
        if let Some(object) = json.as_object_mut() {
            object.insert("_turn_epoch".to_string(), turn_epoch.into());
        }
        #[cfg(feature = "desktop")]
        if let Some(app) = state.app_handle.read().as_ref() {
            let _ = app.emit(&format!("pty-parsed-{session_id}"), &json);
        }
        state.emit_pty_event(crate::state::AppEvent::PtyParsed {
            session_id: session_id.to_string(),
            parsed: json.into(),
        });
    }
}

/// Whether a heuristic `Question` event should be suppressed for this session.
/// Hook-instrumented agents report awaiting via OSC 7770 (`state=awaiting`), so
/// the silence/regex question heuristics would only double-fire. Only `Question`
/// is suppressed — idle/busy transitions and every other event pass through.
fn suppress_heuristic_question(hook_instrumented: bool, event: &ParsedEvent) -> bool {
    hook_instrumented && matches!(event, ParsedEvent::Question { .. })
}

/// Restore the awaiting badge while an Ink dialog is still open on screen.
///
/// The badge is `SessionState.awaiting_input`, driven by events; the dialog is a
/// screen condition that outlives them. A multi-question `AskUserQuestion` is the
/// case where the two part ways: answering sub-question 1 clears the badge, and
/// sub-question 2 repaints its title and options but NOT the footer row — the one
/// row `parse_clean_lines` needs to see change in order to fire again. The result
/// is a tab reading "working" while the agent waits.
///
/// Presence of the footer is the entire signal. Nothing structural is read: title,
/// option list and the `⊠ … ✓ Submit` tab bar all move as the wizard advances,
/// while the footer is byte-identical throughout — useless as a change signal,
/// exact as a presence one.
///
/// Returns an event only when the badge is actually off, so this is one event per
/// spurious clear, never one per repaint. A live `choice_prompt` owns the awaiting
/// state through its own resolve path and is left alone.
///
/// `question_this_tick` is the same rule read one step earlier. `awaiting_input`
/// comes from `SessionState`, which this tick's events have not reached yet, so on
/// the FIRST sub-question — the footer row genuinely changed, `parse_clean_lines`
/// parsed the real question, badge still off — both fire. Two `Question` events
/// land, and the accumulator keeps the LAST `prompt_text`: the tab then shows
/// `⊠ … ✓ Submit` where the question should be. Not exotic, this is every
/// non-hook `AskUserQuestion`'s opening frame.
fn rearm_awaiting_for_open_dialog(
    screen: &[String],
    awaiting_input: bool,
    has_choice_prompt: bool,
    question_this_tick: bool,
) -> Option<ParsedEvent> {
    if awaiting_input || has_choice_prompt || question_this_tick {
        return None;
    }
    crate::output_parser::ink_dialog_footer(screen).map(|footer| ParsedEvent::Question {
        prompt_text: footer.to_string(),
        confident: true,
    })
}

/// Only the first, default-No workspace trust picker of a managed Claude child.
/// Require the question and both choices on the rendered screen so ordinary
/// permission prompts, chat text and a manually changed selection stay intact.
fn managed_claude_trust_dialog(screen: &[String]) -> bool {
    let text = screen.join(" ");
    if !text.contains("Quick safety check:")
        || !text.contains("Is this a project you created or one you trust?")
    {
        return false;
    }
    let yes = screen
        .iter()
        .position(|row| row.trim_start().starts_with("Yes,"));
    let selected_no = screen.iter().position(|row| {
        let row = row.trim_start();
        let choice = row
            .strip_prefix('❯')
            .or_else(|| row.strip_prefix('›'))
            .or_else(|| row.strip_prefix('>'));
        choice.is_some_and(|choice| choice.trim_start().contains("No, exit"))
    });
    matches!((yes, selected_no), (Some(yes), Some(no)) if yes < no)
}

#[cfg(test)]
mod managed_claude_trust_tests {
    use super::managed_claude_trust_dialog;

    #[test]
    fn leaves_other_questions_and_changed_selections_untouched() {
        for rows in [
            vec!["Allow this command?", "  Yes, allow", "❯ No, exit"],
            vec![
                "Quick safety check: Is this a project you created or one you trust?",
                "❯ Yes, I trust this folder",
                "  No, exit",
            ],
            vec![
                "Quick safety check: Is this a project you created or one you trust?",
                "❯ No, exit",
                "  Yes, I trust this folder",
            ],
        ] {
            assert!(
                !managed_claude_trust_dialog(
                    &rows.into_iter().map(str::to_string).collect::<Vec<_>>()
                ),
                "must not answer a different or manually changed choice"
            );
        }
    }
}

fn accept_managed_claude_trust_dialog(state: &AppState, session_id: &str) -> Result<(), String> {
    let writer = state
        .pty_writer(session_id)
        .ok_or_else(|| "Session not found".to_string())?;
    let mut writer = writer.lock();
    state.retire_turn_interrupt(session_id);
    writer
        .write_all(b"\x1b[A")
        .and_then(|()| writer.flush())
        .map_err(|error| format!("Trust selection failed: {error}"))?;
    std::thread::sleep(INJECT_ENTER_GAP);
    writer
        .write_all(b"\r")
        .and_then(|()| writer.flush())
        .map_err(|error| format!("Trust confirmation failed: {error}"))
}

/// Per-session mutable state for processing PTY output chunks.
/// Holds dedup state, parser, and session CWD for PlanFile resolution.
/// Used by `spawn_reader_thread`.
struct ChunkProcessor {
    /// An explicit idle marker asks the reader to flush after publishing this chunk.
    queued_idle_flush: bool,
    parser: OutputParser,
    intent_break_parser: vte::Parser,
    /// Dedup: only emit StatusLine when task_name actually changes *within a
    /// turn*, stored as `(turn_epoch, task_name)`. The epoch is part of the key
    /// because agents may name every turn identically — Codex always reports
    /// "Working" — and a session-lifetime dedup would then swallow the status
    /// line of every turn after the first. The suppressed event is the only
    /// thing that clears the previous turn's `suggested_actions`, which
    /// `session_state_with_shell` treats as a completion marker, so a working
    /// agent would stay reported as completed/idle for the rest of the session.
    last_status_task: Option<(u64, String)>,
    /// Dedup: don't re-emit the same question prompt_text
    last_question_text: Option<String>,
    /// Tail of the previous chunk holding an OSC sequence the read split in
    /// half. A PTY read boundary falls wherever the kernel decides, so a chunk
    /// can end mid-escape; the raw-stream parsers match on complete sequences
    /// only (correctly — a truncated one must never match, or its fields would
    /// run on into unrelated later output), so without this the signal is simply
    /// lost. Observed: an Ink repaint split `ESC]777;notify;…BEL` and the
    /// awaiting badge never lit. Bounded by [`MAX_RAW_CARRY`].
    raw_carry: String,
    /// Dedup: last emitted ChoicePrompt signature (title + option keys).
    /// Prevents re-emit on repaint while the dialog stays on screen.
    last_choice_prompt_sig: Option<String>,
    /// A busy hook cleared the choice; wait for a dialog-row repaint before
    /// admitting the same signature again, so stale screen cells cannot re-arm it.
    choice_prompt_needs_repaint: bool,
    /// Session CWD for resolving relative plan-file paths
    session_cwd: Option<String>,
    /// Plan files awaiting creation on disk (agent announces before writing).
    /// Tuples of (absolute_path, deadline). Checked each chunk until file appears
    /// or 10s deadline expires. Already-emitted paths tracked for dedup.
    pending_planfiles: Vec<(String, std::time::Instant)>,
    /// Plan file paths already emitted — prevents re-emitting on spinner redraws.
    emitted_planfiles: std::collections::HashSet<String>,
    /// Plan file paths that exhausted their retry window without appearing on
    /// disk. Tombstoned so a still-on-screen reference (re-parsed every chunk)
    /// is not re-queued forever — that was a source of endless retry-log spam.
    gaveup_planfiles: std::collections::HashSet<String>,
    /// Tracks whether the terminal is in alternate screen buffer mode.
    /// Set on ESC[?1049h, cleared on ESC[?1049l.
    pub(crate) in_alt_buffer: bool,
    /// Structured terminal mode with nesting depth and app detection.
    terminal_mode: crate::ai_agent::tui_detect::TerminalMode,
    /// One-shot flag: inject ESC[2J before the next ESC[H cursor-home.
    /// Set on alt-buffer entry and when content may have shrunk (detected via
    /// cursor-up ESC[nA with n > previous). Consumed after inject fires.
    alt_buffer_needs_clear: bool,
    /// Tracks the largest cursor-up (ESC[nA) value seen since last clear.
    /// When a new ESC[nA arrives with n < last_cursor_up_n, content has shrunk
    /// and we need a clear to prevent ghost artifacts.
    last_cursor_up_n: u16,
    /// Last VtLogBuffer total_lines observed — distinguishes a chunk that
    /// scrolled in new output from one that only repainted existing rows.
    last_vt_log_total: usize,
    /// Report a TUIC-managed agent entering alternate screen only once per PTY.
    alt_screen_warned: bool,
    /// The startup toast fires only for an alternate-screen entry before this instant.
    startup_deadline: std::time::Instant,
    /// The startup toast is shown once per PTY.
    alt_screen_toasted: bool,
    /// Command text captured on OSC 133 C — used when the matching D arrives
    /// to build a `CommandOutcome`. Cleared after D.
    pending_command: Option<String>,
    /// `Instant` when OSC 133 C arrived; used for `duration_ms`.
    pending_command_started: Option<std::time::Instant>,
    /// TUIC_SESSION UUID for this PTY — used to create flag files that
    /// signal the shell wrapper to stop injecting `--session-id`.
    tuic_session: Option<String>,
    /// Last time we created a no-session-inject flag file in response to
    /// an `AgentSessionConflict` event. Gates subsequent marks so a single
    /// burst of conflict output fires the mitigation exactly once.
    last_session_conflict_mark: Option<std::time::Instant>,
    /// Absolute buffer line of the last heuristic agent-block start.
    /// Used to emit AgentBlock end when the next block starts or agent exits.
    last_agent_block_line: Option<usize>,
    /// Edge-detect an "Action Required" OSC 0 title so a permission prompt fires
    /// the question notification exactly once (the title repaints every spinner
    /// tick). Agent-agnostic: any agent that puts "Action Required" in its title
    /// (grok, Codex, …) drives this. True while the last title signalled
    /// awaiting-approval.
    title_awaiting: bool,
    /// Question raised by Codex's approval title. A later cancellation may
    /// clear only this question, even if another confident question arrived.
    codex_approval_question: Option<String>,
    /// A cancellation row painted since the current Codex approval began.
    codex_approval_canceled: bool,
    /// Reusable screen snapshot handed to the post-lock consumers
    /// (`parse_slash_menu`, `parse_choice_prompt`, the question-dedup absence
    /// check and `rearm_awaiting_for_open_dialog`). Retained across chunks so
    /// the snapshot reuses the row `String` allocations instead of allocating
    /// one per visible row on every chunk that moved anything.
    screen_buf: Vec<String>,
}

/// A terminal that enters the alternate screen this soon after spawn is treated
/// as starting there; later entries are ordinary full-screen apps (vim, less).
const ALT_SCREEN_STARTUP_WINDOW: std::time::Duration = std::time::Duration::from_secs(15);

/// Toast attributed to `session_id`, on the desktop window and the event bus.
fn emit_session_toast(
    state: &AppState,
    session_id: &str,
    title: &str,
    message: String,
    level: &str,
) {
    #[cfg(feature = "desktop")]
    if let Some(ref app) = *state.app_handle.read() {
        let _ = app.emit(
            "mcp-toast",
            serde_json::json!({
                "title": title,
                "message": message,
                "level": level,
                "sound": null,
                "origin_session_id": session_id,
            }),
        );
    }
    let _ = state.event_bus.send(crate::state::AppEvent::McpToast {
        title: title.into(),
        message: Some(message),
        level: level.into(),
        sound: None,
        origin_repo_path: None,
        origin_session_id: Some(session_id.into()),
    });
}

fn alt_screen_toast_message(label: &str, agent_type: Option<&str>) -> String {
    let fix = match agent_type {
        Some("claude") => {
            "Set CLAUDE_CODE_DISABLE_ALTERNATE_SCREEN=1 and CLAUDE_CODE_DISABLE_AGENT_VIEW=1 (or disableAgentView in settings)."
        }
        Some("codex" | "grok") => "Start it with --no-alt-screen.",
        Some("opencode") => "Start it with --mini.",
        _ => "Check the program for a no-alternate-screen option.",
    };
    format!(
        "{label} switched to the alternate screen at startup. This breaks native scrollback and TUIC agent state detection. {fix}"
    )
}

impl ChunkProcessor {
    fn should_warn_alt_screen(&mut self, agent_type: Option<&str>, alt_screen: bool) -> bool {
        if agent_type.is_none() || !alt_screen || self.alt_screen_warned {
            return false;
        }
        self.alt_screen_warned = true;
        true
    }

    /// True once per PTY, when the screen is alternate on the first startup-window chunk that shows it.
    fn should_toast_alt_screen(&mut self, alt_screen: bool) -> bool {
        if !alt_screen
            || self.alt_screen_toasted
            || std::time::Instant::now() > self.startup_deadline
        {
            return false;
        }
        self.alt_screen_toasted = true;
        true
    }

    fn new(session_cwd: Option<String>, tuic_session: Option<String>) -> Self {
        Self {
            queued_idle_flush: false,
            parser: OutputParser::new(),
            intent_break_parser: vte::Parser::new(),
            last_status_task: None,
            last_question_text: None,
            raw_carry: String::new(),
            last_choice_prompt_sig: None,
            choice_prompt_needs_repaint: false,
            session_cwd,
            pending_planfiles: Vec::new(),
            emitted_planfiles: std::collections::HashSet::new(),
            gaveup_planfiles: std::collections::HashSet::new(),
            in_alt_buffer: false,
            terminal_mode: crate::ai_agent::tui_detect::TerminalMode::Shell,
            alt_buffer_needs_clear: false,
            last_cursor_up_n: 0,
            last_vt_log_total: 0,
            alt_screen_warned: false,
            startup_deadline: std::time::Instant::now() + ALT_SCREEN_STARTUP_WINDOW,
            alt_screen_toasted: false,
            pending_command: None,
            pending_command_started: None,
            tuic_session,
            last_session_conflict_mark: None,
            last_agent_block_line: None,
            title_awaiting: false,
            codex_approval_question: None,
            codex_approval_canceled: false,
            screen_buf: Vec::new(),
        }
    }

    /// Handle OSC 7770 `state=idle|busy` from the TUIC protocol.
    fn handle_tuic_state(&self, payload: &str, session_id: &str, state: &AppState) {
        let (target, label) = match payload {
            "idle" => (SHELL_IDLE, "idle"),
            "busy" | "prompt" => (SHELL_BUSY, "busy"),
            _ => return,
        };
        transition_explicit_shell_state(state, session_id, target, label, true);
    }

    /// Handle a single OSC 133 event from the VTE handler.
    /// On 'C' captures the command text; on 'D' builds a `CommandOutcome`.
    fn handle_osc133_event(
        &mut self,
        command: char,
        params: &str,
        session_id: &str,
        state: &AppState,
    ) {
        use crate::ai_agent::knowledge::{CommandOutcome, OutcomeClass, classify_error};

        // Deterministic state transitions from shell integration markers.
        // A = prompt shown (idle), C = command execution started (busy).
        // These bypass the silence timer entirely when OSC 133 is available.
        match command {
            'A' => {
                transition_explicit_shell_state(state, session_id, SHELL_IDLE, "idle", false);
            }
            'C' => {
                transition_explicit_shell_state(state, session_id, SHELL_BUSY, "busy", false);
                let cmd = state
                    .session_maps
                    .input_buffers
                    .get(session_id)
                    .map(|b| b.lock().content())
                    .unwrap_or_default();
                self.pending_command = Some(cmd);
                self.pending_command_started = Some(std::time::Instant::now());
            }
            'D' => {
                // A 'D' (command finished) with no preceding 'C' (command
                // started) means no command actually ran — e.g. Enter on an
                // empty prompt, where the shell still emits D carrying the
                // previous command's exit code. Recording it would create a
                // phantom outcome with an empty command and "unknown" error
                // type, polluting both the knowledge panel and the agent's
                // injected prompt. Skip it.
                if self.pending_command_started.is_none() {
                    self.pending_command = None;
                    return;
                }
                let exit_code = params.parse::<i32>().unwrap_or(0);
                let command = self.pending_command.take().unwrap_or_default();
                let duration_ms = self
                    .pending_command_started
                    .take()
                    .map(|t| t.elapsed().as_millis() as u64)
                    .unwrap_or(0);
                let cwd = self.session_cwd.clone().unwrap_or_default();
                let output_snippet = state
                    .grid
                    .vt_log_buffers
                    .get(session_id)
                    .map(|b| {
                        let buf = b.lock();
                        buf.screen_rows().join("\n")
                    })
                    .unwrap_or_default();
                let mut tail_start = output_snippet.len().saturating_sub(500);
                while tail_start > 0 && !output_snippet.is_char_boundary(tail_start) {
                    tail_start += 1;
                }
                let output_snippet = output_snippet[tail_start..].to_string();

                let classification = if exit_code == 0 {
                    OutcomeClass::Success
                } else if let Some(error_type) = classify_error(&output_snippet) {
                    OutcomeClass::Error { error_type }
                } else {
                    OutcomeClass::Error {
                        error_type: "unknown".into(),
                    }
                };

                let outcome = CommandOutcome {
                    timestamp: std::time::SystemTime::now()
                        .duration_since(std::time::UNIX_EPOCH)
                        .map(|d| d.as_secs())
                        .unwrap_or(0),
                    command,
                    cwd,
                    exit_code: Some(exit_code),
                    output_snippet,
                    classification,
                    duration_ms,
                    id: 0,
                };
                state.knowledge_entry(session_id).lock().terminal_mode = self.terminal_mode.clone();
                state.record_outcome(session_id, outcome);
            }
            _ => {}
        }
    }

    /// Classify an inline TUI (mouse reporting on the primary screen) the same
    /// way `transform_xterm` classifies `1049h`. Alt-screen nesting still owns
    /// `terminal_mode` once `1049h` has been seen; this only covers the
    /// `grok --no-alt-screen` case that never sends it.
    fn apply_inline_tui_mode(
        &mut self,
        alt_screen: bool,
        mouse_reporting: bool,
        agent_type: Option<&str>,
    ) {
        if alt_screen || self.in_alt_buffer {
            return;
        }
        if mouse_reporting {
            if !self.terminal_mode.is_fullscreen() {
                self.terminal_mode = crate::ai_agent::tui_detect::TerminalMode::FullscreenTui {
                    app_hint: agent_type.map(str::to_string),
                    depth: 1,
                };
            }
            return;
        }
        if matches!(
            self.terminal_mode,
            crate::ai_agent::tui_detect::TerminalMode::FullscreenTui { depth: 1, .. }
        ) {
            self.terminal_mode = crate::ai_agent::tui_detect::TerminalMode::Shell;
        }
    }

    /// Colorize `intent:` tokens and apply alternate-buffer fixes on the xterm
    /// stream. Suggest tokens are NOT concealed here — the frontend's
    /// `eraseSuggestFromBuffer()` handles that via rAF after xterm renders.
    fn transform_xterm<'a>(&mut self, data: &'a str) -> Option<std::borrow::Cow<'a, str>> {
        // Track alternate screen buffer state for the clear-before-home fix below.
        if data.contains("\x1b[?1049h") {
            self.in_alt_buffer = true;
            self.alt_buffer_needs_clear = true;
            self.terminal_mode = self.terminal_mode.on_alt_enter();
        } else if data.contains("\x1b[?1049l") {
            self.in_alt_buffer = false;
            self.alt_buffer_needs_clear = false;
            self.terminal_mode = self.terminal_mode.on_alt_exit();
        }

        // Detect render-height change in alternate buffer: when Ink's cursor-up
        // (ESC[nA) value changes, the chrome area may have shifted vertically.
        // Ink never sends ESC[K (erase to end of line), so rows that were chrome
        // in the previous render but aren't overwritten in the new one persist as
        // ghost artifacts — starting from the bottom and expanding upward.
        if self.in_alt_buffer
            && let Some(n) = extract_largest_cursor_up(data)
        {
            if n != self.last_cursor_up_n && self.last_cursor_up_n > 0 {
                self.alt_buffer_needs_clear = true;
            }
            self.last_cursor_up_n = n;
        }

        // Inject ESC[2J (clear screen) before the first positioning sequence when
        // needed. Tries cursor-home (ESC[H) first, then falls back to cursor-up
        // (ESC[nA). Ink re-renders use cursor-up for repositioning, not cursor-home,
        // so the fallback is essential — without it the flag accumulates forever.
        // Borrowed unless an injection actually fires: the overwhelming majority
        // of chunks pass straight through, and this used to copy every one.
        if !self.alt_buffer_needs_clear {
            return Some(std::borrow::Cow::Borrowed(data));
        }
        let injected = inject_clear_before_cursor_home(data);
        if injected.len() != data.len() {
            self.alt_buffer_needs_clear = false;
            return Some(std::borrow::Cow::Owned(injected));
        }
        let injected = inject_clear_before_cursor_up(data);
        if injected.len() != data.len() {
            self.alt_buffer_needs_clear = false;
            return Some(std::borrow::Cow::Owned(injected));
        }
        Some(std::borrow::Cow::Borrowed(data))
    }

    /// Resolve a relative plan-file path to absolute using session CWD.
    /// Returns None if the path is relative and no CWD is available.
    fn resolve_planfile_path(&self, path: &str) -> Option<String> {
        // Both shapes of absolute: `Path::is_absolute` covers `C:\…` on Windows
        // but not a leading `/`, and the agents that emit these lines write
        // either one there. Joining an absolute path onto the session cwd would
        // produce a path that does not exist.
        if path.starts_with('/') || std::path::Path::new(path).is_absolute() {
            Some(path.to_string())
        } else if let Some(ref cwd) = self.session_cwd {
            let joined = std::path::PathBuf::from(cwd).join(path);
            Some(normalize_path(&joined).to_string_lossy().into_owned())
        } else {
            None
        }
    }

    /// Create a flag file that tells the shell wrapper to stop injecting
    /// `--session-id $TUIC_SESSION` into `claude` invocations. This is the
    /// safe alternative to writing `export TUIC_SESSION=…` into the PTY,
    /// which can corrupt fullscreen TUI output or race with user input.
    ///
    /// Guarded by a 3-second cooldown: Claude prints the error line multiple
    /// times as it exits, and we want exactly one flag per conflict burst.
    fn mark_session_no_inject(&mut self, kind: &str) {
        const COOLDOWN: std::time::Duration = std::time::Duration::from_secs(3);
        let now = std::time::Instant::now();
        if self
            .last_session_conflict_mark
            .is_some_and(|t| now.duration_since(t) < COOLDOWN)
        {
            return;
        }
        self.last_session_conflict_mark = Some(now);

        let Some(ref tuic_session) = self.tuic_session else {
            return;
        };

        let flag_path =
            crate::config::config_dir().join(format!("no-session-inject.{tuic_session}"));
        match std::fs::write(&flag_path, b"") {
            Ok(()) => {
                tracing::info!(
                    tuic_session = %tuic_session,
                    kind = %kind,
                    "Created no-session-inject flag after agent-session-conflict"
                );
            }
            Err(e) => {
                tracing::warn!(
                    tuic_session = %tuic_session,
                    error = %e,
                    "Failed to create no-session-inject flag"
                );
            }
        }
    }

    /// Drain pending plan files: emit event for files that now exist, drop expired ones.
    fn check_pending_planfiles(&mut self, session_id: &str, state: &AppState) {
        if self.pending_planfiles.is_empty() {
            return;
        }
        let now = std::time::Instant::now();
        let mut i = 0;
        while i < self.pending_planfiles.len() {
            let (ref path, deadline) = self.pending_planfiles[i];
            if now > deadline {
                tracing::debug!("[plan-file] Retry expired (10s), dropping: {path}");
                let path = self.pending_planfiles.swap_remove(i).0;
                // Tombstone so the still-visible reference isn't re-queued forever.
                self.gaveup_planfiles.insert(path);
                continue;
            }
            if std::path::Path::new(path).is_file() {
                let path = self.pending_planfiles.swap_remove(i).0;
                tracing::info!("[plan-file] Retry succeeded: {path}");
                self.emitted_planfiles.insert(path.clone());
                let evt = ParsedEvent::PlanFile { path };
                if let Ok(json) = serde_json::to_value(&evt).map(std::sync::Arc::new) {
                    state.emit_pty_event(crate::state::AppEvent::PtyParsed {
                        session_id: session_id.to_string(),
                        parsed: std::sync::Arc::clone(&json),
                    });
                    #[cfg(feature = "desktop")]
                    if let Some(a) = state.app_handle.read().as_ref() {
                        let _ = a.emit(
                            "pty-parsed",
                            serde_json::json!({
                                "session_id": session_id,
                                "parsed": &*json,
                            }),
                        );
                    }
                }
                continue;
            }
            i += 1;
        }
    }

    /// Process a chunk of PTY output after kitty-sequence stripping.
    /// Handles: VT log buffer, ring buffer, WebSocket broadcast, event parsing,
    /// dedup, resize-grace filtering, PlanFile resolution, event emission,
    /// silence state, last_output_ms, and shell state transitions.
    ///
    /// Returns true when the chunk was non-empty, i.e. the caller should hand
    /// the SAME borrowed bytes to `transform_xterm`. It used to return an owned
    /// copy of the chunk, which allocated and memcpy'd up to 64 KB per PTY read
    /// for a value the caller already held.
    /// `app` is Some for desktop mode (emits Tauri IPC), None for headless.
    fn process_chunk(
        &mut self,
        data: &str,
        silence: &Arc<Mutex<SilenceState>>,
        session_id: &str,
        state: &AppState,
    ) -> bool {
        if data.is_empty() {
            return false;
        }

        // Check pending plan files: emit if file appeared, drop if deadline expired.
        self.check_pending_planfiles(session_id, state);

        // Read once, before the vt_log lock: the screen classifier needs it inside
        // that lock, and taking a session_states shard while holding the vt_log
        // mutex would introduce a lock order this file does not otherwise have.
        let agent_type = state
            .session_maps
            .session_states
            .get(session_id)
            .and_then(|s| s.agent_type.clone());

        // The screen snapshot is refilled in place: `screen_rows()` is
        // `prev_rows.clone()`, one allocation per visible row per chunk. Taking
        // the buffer out of `self` keeps the later `&mut self` uses (parser,
        // dedup markers) borrow-checkable; it is put back at the end.
        let mut screen_buf = std::mem::take(&mut self.screen_buf);
        let mut unexpected_alt_screen = false;
        let mut startup_alt_screen = false;
        let mut choice_changed_rows = Vec::new();

        // Feed raw data (post-kitty-strip) into VT100 log buffer.
        // `total_lines` comes back with it: a chunk that grew the buffer produced
        // real output, a chunk that did not merely repainted the screen.
        let (
            changed_rows,
            vt_output_grew,
            term_events,
            screen_present,
            screen_activity,
            cursor_row,
            logical_prefix,
            physical_prefix,
            history_size,
            intent_origin,
            intent_candidate,
        ): VtProcessResult = if let Some(vt_log) = state.grid.vt_log_buffers.get(session_id) {
            let mut vt = vt_log.lock();
            let mut changed = vt.process(data.as_bytes());
            // Publish the real sync state (a nested BSU keeps it open) so the
            // frame ticker knows whether this session can have a stalled
            // synchronized update worth taking the lock for.
            if let Some(flag) = state.grid.sync_update_active.get(session_id) {
                flag.store(vt.is_sync_update_active(), Ordering::Relaxed);
            }
            let total = vt.total_lines();
            unexpected_alt_screen =
                self.should_warn_alt_screen(agent_type.as_deref(), vt.is_alternate_screen());
            startup_alt_screen = self.should_toast_alt_screen(vt.is_alternate_screen());
            let hist = vt.grid_history_size();
            let intent_origin = vt.grid_screen_origin();
            // Did this chunk produce real output, or merely repaint rows that were
            // already there (SIGWINCH reflow, cursor blink, statusline)? In the
            // PRIMARY screen a repaint never grows the durable log while real work
            // scrolls new lines into it, so the total answers the question.
            //
            // In the ALTERNATE screen it cannot: `VtLogBuffer::process` skips log
            // capture entirely while alt is active, so the total is frozen however
            // much the agent writes, and "did not grow" is not evidence of a
            // repaint. Nothing is lost by reporting growth there — the only reader
            // is the resize-grace extension below, which covers a reflow, and the
            // grid performs no reflow in alt (`VtLogBuffer::resize` picks
            // `ReflowMode::None`). Reading a frozen total as a repaint instead
            // re-armed the grace on every chunk, so a single resize suppressed
            // low-confidence questions, rate-limit and API-error events and the
            // BUSY transition until the agent paused for a full second.
            let vt_output_grew = vt.is_alternate_screen() || total > self.last_vt_log_total;
            self.last_vt_log_total = self.last_vt_log_total.max(total);
            // Grid is the source of truth for mouse DECSET (including combined
            // `?1000;1002;1006h`). String-matching the chunk would miss grok.
            self.apply_inline_tui_mode(
                vt.is_alternate_screen(),
                vt.is_mouse_reporting(),
                agent_type.as_deref(),
            );
            let tevts = vt.grid_drain_events();
            // Did ANYTHING on screen move? Taken before the chrome filter below,
            // because that filter drops rows under the input-area border and a
            // choice dialog can render there.
            //
            // DEFERRED (2026-09-06) — moving this AFTER the chrome filter was
            // proposed to stop a 1 Hz status line paying for the snapshot, and
            // it is wrong: Claude Code renders its slash menu BELOW the input
            // box, so on a slash-menu tick every changed row is under the
            // cutoff and the menu would never be parsed. Reproduced — flipping
            // the two lines turns `chunk_path_scenarios_emit_the_same_events`
            // from `["slash-menu"]` into `[]`. Any future attempt needs a
            // per-consumer gate, not one shared flag.
            let any_row_changed = !changed.is_empty();
            if self.last_choice_prompt_sig.is_some() {
                choice_changed_rows.extend(changed.iter().map(|row| row.row_index));
            }

            // ONE borrow of the rendered screen, shared by all three consumers
            // below (chrome cutoff, screen classification, snapshot refill).
            // They used to take three independent `screen_rows_ref()` borrows
            // and the last one cloned.
            let screen_ref = vt.screen_rows_ref();

            // Filter out changed rows below the input area border (horizontal rule).
            // Claude Code (and similar agents) render a quota/budget status bar below
            // the input box separator. Those rows are cosmetic chrome — processing them
            // resets the silence timer and causes false busy→idle→question transitions.
            //
            // `retain` in place: the filter used to rebuild the whole Vec even
            // when the cutoff dropped nothing.
            let chrome_cutoff = if let Some(screen) = screen_ref
                && !changed.is_empty()
            {
                let refs: Vec<&str> = screen.iter().map(String::as_str).collect();
                // Fails OPEN by contract: no anchor found is `None`, and `None`
                // must mean "parse everything", never "parse nothing".
                crate::chrome::find_chrome_cutoff(&refs)
            } else {
                None
            };
            if let Some(cutoff) = chrome_cutoff {
                changed.retain(|r| r.row_index < cutoff);
            }
            let changed = changed;

            // Screen classification runs on EVERY chunk, borrowed, never cloned: a
            // repaint that is byte-identical produces no changed rows, and holding
            // BUSY through exactly that case (a frozen spinner, DEC 2026 frame
            // coalescing) is the point of `detect_agent_screen_activity`.
            let screen_activity = screen_ref
                .map(|rows| {
                    detect_agent_screen_activity_at(
                        agent_type.as_deref(),
                        rows,
                        Some(vt.grid_columns()),
                    )
                })
                .unwrap_or(AgentScreenActivity::Unknown);

            // ONE snapshot per tick, cloned into the retained buffer and handed
            // to every post-lock consumer. A slash menu or a choice dialog
            // cannot have appeared on a screen where nothing moved, so a chunk
            // that changed no row skips the snapshot entirely — that is the
            // per-chunk hot path.
            //
            // `any_row_changed` is deliberately the PRE-cutoff answer: Claude
            // Code renders its slash menu BELOW the input-box chrome, so gating
            // on the trimmed rows would stop the menu being seen at all
            // (proved by `chunk_path_scenarios_emit_the_same_events`).
            let screen_present = match screen_ref.filter(|_| any_row_changed) {
                Some(rows) => {
                    screen_buf.truncate(rows.len());
                    for (slot, row) in screen_buf.iter_mut().zip(rows) {
                        slot.clear();
                        slot.push_str(row);
                    }
                    screen_buf.extend(rows[screen_buf.len().min(rows.len())..].iter().cloned());
                    true
                }
                None => false,
            };
            let cursor_row = vt.cursor_point().0;
            let logical_prefix = vt.logical_prefix_at_cursor();
            let physical_prefix = vt.physical_prefix_at_cursor();
            let intent_candidate = agent_type.as_ref().and_then(|_| {
                // A repaint can change many rows below the same anchor. Cache
                // its result for this tick, including rejected candidates.
                let mut cache = std::collections::HashMap::new();
                changed.iter().rev().find_map(|row| {
                    // A later read may update only an indented continuation.
                    // Search its bounded predecessors for the unchanged anchor.
                    (row.row_index
                        .saturating_sub(crate::output_parser::MAX_INTENT_CONTINUATION_ROWS)
                        ..=row.row_index)
                        .rev()
                        .find_map(|anchor_row| {
                            if screen_ref.is_some_and(|screen| {
                                !screen
                                    .get(anchor_row)
                                    .is_some_and(|text| text.contains("intent:"))
                            }) {
                                return None;
                            }
                            let cached = cache.entry(anchor_row).or_insert_with(|| {
                                #[cfg(test)]
                                INTENT_CANDIDATE_GRID_READS
                                    .with(|reads| reads.set(reads.get() + 1));
                                let mut line = vt.logical_line_at_row(anchor_row)?;
                                if chrome_cutoff.is_some_and(|cutoff| line.end_row >= cutoff) {
                                    return None;
                                }
                                if crate::output_parser::structured_token_anchor(&line.text)
                                    != Some(crate::output_parser::StructuredTokenAnchor::Intent)
                                {
                                    return None;
                                }
                                let anchor_text = line.text.clone();
                                let mut block = anchor_text.clone();
                                let mut continuation_ends = Vec::new();
                                let mut physical_widths = Vec::new();
                                physical_widths.push(
                                    screen_ref
                                        .and_then(|screen| screen.get(line.end_row))
                                        .map_or_else(
                                            || {
                                                unicode_width::UnicodeWidthStr::width(
                                                    anchor_text.as_str(),
                                                )
                                            },
                                            |row| {
                                                unicode_width::UnicodeWidthStr::width(
                                                    row.trim_end(),
                                                )
                                            },
                                        ),
                                );
                                let mut next = line.end_row + 1;
                                // DEFERRED (2026-09-25) — Stop at the chrome cutoff once a
                                // production-path test captures a task panel under an intent.
                                for _ in 0..crate::output_parser::MAX_INTENT_CONTINUATION_ROWS {
                                    if crate::output_parser::intent_row_is_complete(&block) {
                                        break;
                                    }
                                    if chrome_cutoff.is_some_and(|cutoff| next >= cutoff) {
                                        break;
                                    }
                                    #[cfg(test)]
                                    INTENT_CONTINUATION_GRID_READS
                                        .with(|reads| reads.set(reads.get() + 1));
                                    let Some(continuation) = vt.logical_line_at_row(next) else {
                                        break;
                                    };
                                    if continuation.start_row != next {
                                        break;
                                    }
                                    block.push('\n');
                                    block.push_str(&continuation.text);
                                    continuation_ends.push(continuation.end_row);
                                    physical_widths.push(
                                        screen_ref
                                            .and_then(|screen| screen.get(continuation.end_row))
                                            .map_or_else(
                                                || {
                                                    unicode_width::UnicodeWidthStr::width(
                                                        continuation.text.as_str(),
                                                    )
                                                },
                                                |row| {
                                                    unicode_width::UnicodeWidthStr::width(
                                                        row.trim_end(),
                                                    )
                                                },
                                            ),
                                    );
                                    let (joined, absorbed) =
                                        crate::output_parser::dewrap_intent_continuation_with_rows(
                                            &block,
                                            Some((vt.grid_columns(), &physical_widths)),
                                        );
                                    if absorbed != continuation_ends.len() {
                                        break;
                                    }
                                    if crate::output_parser::intent_row_is_complete(
                                        joined.lines().next().unwrap_or_default(),
                                    ) {
                                        break;
                                    }
                                    next = continuation.end_row + 1;
                                }
                                let (dewrapped, absorbed) =
                                    crate::output_parser::dewrap_intent_continuation_with_rows(
                                        &block,
                                        Some((vt.grid_columns(), &physical_widths)),
                                    );
                                line.text =
                                    dewrapped.lines().next().unwrap_or_default().to_string();
                                if absorbed > 0 {
                                    line.end_row = continuation_ends[absorbed - 1];
                                }
                                Some((line, anchor_text))
                            });
                            let (line, anchor_text) = cached.as_ref()?;
                            (line.start_row..=line.end_row)
                                .contains(&row.row_index)
                                .then(|| (line.clone(), anchor_text.clone()))
                        })
                })
            });

            (
                changed,
                vt_output_grew,
                tevts,
                screen_present,
                screen_activity,
                Some(cursor_row),
                logical_prefix,
                physical_prefix,
                hist,
                intent_origin,
                intent_candidate,
            )
        } else {
            (
                Vec::new(),
                false,
                Vec::new(),
                false,
                AgentScreenActivity::Unknown,
                None,
                None,
                None,
                0,
                0,
                None,
            )
        };

        if screen_present
            && agent_type.as_deref() == Some("claude")
            && state.managed_trust_dialogs.contains(session_id)
            && managed_claude_trust_dialog(&screen_buf)
            && state.managed_trust_dialogs.remove(session_id).is_some()
            && let Err(error) = accept_managed_claude_trust_dialog(state, session_id)
        {
            tracing::warn!(source = "terminal", session_id, %error, "Could not accept managed Claude workspace trust dialog");
        }

        if startup_alt_screen {
            let label = state
                .session_maps
                .sessions
                .get(session_id)
                .and_then(|entry| entry.lock().display_name.clone())
                .unwrap_or_else(|| session_id.to_string());
            emit_session_toast(
                state,
                session_id,
                "Terminal is on the alternate screen",
                alt_screen_toast_message(&label, agent_type.as_deref()),
                "warn",
            );
        }

        if unexpected_alt_screen {
            let agent = agent_type.as_deref().unwrap_or("unknown").to_string();
            let session_id = session_id.to_string();
            std::thread::spawn(move || {
                let version = crate::agent::detect_agent_binary_sync(agent.clone())
                    .version
                    .unwrap_or_else(|| "unknown".to_string());
                tracing::warn!(
                    source = "terminal",
                    session_id,
                    agent,
                    version,
                    "Agent entered alternate screen despite native scrollback default"
                );
            });
        }

        // Nothing is emitted for scrollback growth. There was a throttled
        // `pty-vt-log-total-{session_id}` here whose comment claimed the frontend
        // listened for it and refreshed the scrollback overlay; no such listener
        // ever existed on either transport. `Manager::emit` serializes the
        // payload before it consults the listener registry, so a dead event is
        // not free — and a comment describing a consumer that is not there costs
        // more, because the next reader builds the frontend half rather than
        // deleting the emit. The overlay reads the totals when it fetches a
        // chunk. `last_vt_log_total` stays: `vt_output_grew` is a real reader.

        // Handle terminal events from alacritty (title, clipboard, PTY writes, OSC 133, TUIC)
        let mut tuic_events: Vec<ParsedEvent> = Vec::new();
        let mut explicit_idle_in_chunk = false;
        if !term_events.is_empty() {
            use crate::terminal_grid::{Osc133Event, TermEvent};
            for evt in term_events {
                match evt {
                    TermEvent::PtyWrite(response) => {
                        // Four substring scans of a terminal reply, for a
                        // diagnostic error line only. Behind the toggle.
                        if crate::cpu_watchdog::diagnostic_mode()
                            && (response.contains("\x1b[?1049")
                                || response.contains("\x1b[?1047")
                                || response.contains("\x1b[?47l")
                                || response.contains("\x1b[?25h"))
                        {
                            tracing::error!(source = "terminal", session_id = %session_id,
                                "PtyWrite contains DEC private mode sequences! response={:?}",
                                response.as_bytes().iter().take(200).collect::<Vec<_>>());
                        }
                        write_terminal_reply(state, session_id, response.as_bytes(), "PtyWrite");
                    }
                    TermEvent::Title(title) => {
                        state.emit_pty_event(crate::state::AppEvent::PtyTitle {
                            session_id: session_id.to_string(),
                            title: title.clone(),
                        });
                        #[cfg(feature = "desktop")]
                        if let Some(a) = state.app_handle.read().as_ref() {
                            let _ = a.emit(&format!("pty-title-{session_id}"), &title);
                        }
                        // Some agents signal an awaiting-approval permission prompt by
                        // putting "Action Required" in their OSC 0 title (grok prefixes
                        // "⚠ Action Required - ⠙ - Running: echo … - Execute Shell …";
                        // Codex uses "[ . ] Action Required | …"). Agent-agnostic: any
                        // such title drives this. The title repaints every spinner tick,
                        // so edge-detect the false→true transition and fire the question
                        // exactly once.
                        // DEFERRED (2026-06-11) — grok 0.2.45 in always-approve mode
                        // emits titles like "Run Shell Command echo … - grok" with NO
                        // "Action Required" prefix (verified live). The prefix may be
                        // version/permission-mode specific; the on-screen "◆ …?" prompt
                        // (cliclack path in output_parser) covers real approvals. Re-verify
                        // grok's title in default (non-always-approve) mode before removing.
                        let title_awaiting = title.contains("Action Required");
                        if title_awaiting && !self.title_awaiting {
                            let prompt_text = clean_action_required_title(&title);
                            if agent_type.as_deref() == Some("codex") {
                                self.codex_approval_question = Some(prompt_text.clone());
                                self.codex_approval_canceled = false;
                            }
                            tuic_events.push(ParsedEvent::Question {
                                prompt_text,
                                confident: true,
                            });
                        }
                        self.title_awaiting = title_awaiting;
                    }
                    TermEvent::ResetTitle => {
                        state.emit_pty_event(crate::state::AppEvent::PtyTitle {
                            session_id: session_id.to_string(),
                            title: String::new(),
                        });
                        #[cfg(feature = "desktop")]
                        if let Some(a) = state.app_handle.read().as_ref() {
                            let _ = a.emit(&format!("pty-title-{session_id}"), "");
                        }
                        self.title_awaiting = false;
                    }
                    TermEvent::ClipboardStore(text) => {
                        #[cfg(feature = "desktop")]
                        if let Some(a) = state.app_handle.read().as_ref() {
                            let _ = a.emit(&format!("pty-clipboard-store-{session_id}"), &text);
                        }
                    }
                    TermEvent::Osc133 {
                        command,
                        params,
                        line,
                    } => {
                        explicit_idle_in_chunk |= command == 'A';
                        state
                            .session_maps
                            .has_osc133_integration
                            .insert(session_id.to_string(), ());
                        self.handle_osc133_event(command, &params, session_id, state);
                        // Dual-emitted: there is no bus→window forwarder, so the
                        // desktop event and the bus push are two separate writes of
                        // one signal. The bus copy is what gives a browser/PWA
                        // client its command blocks, gutter marks and Cmd+Up/Down
                        // navigation, through the `osc133` grid-WS frame.
                        let exit_code = parse_osc133_exit_code(command, &params);
                        #[cfg(feature = "desktop")]
                        if let Some(a) = state.app_handle.read().as_ref() {
                            let _ = a.emit(
                                &format!("pty-osc133-{session_id}"),
                                &Osc133Event {
                                    marker: command.to_string(),
                                    line,
                                    exit_code,
                                },
                            );
                        }
                        state.emit_pty_event(crate::state::AppEvent::PtyOsc133 {
                            session_id: session_id.to_string(),
                            marker: command.to_string(),
                            line,
                            exit_code,
                        });
                    }
                    TermEvent::Osc7(url) => {
                        #[allow(clippy::collapsible_if)]
                        if let Ok(cwd) = parse_osc7_cwd(&url) {
                            if let Some(entry) = state.session_maps.sessions.get(session_id) {
                                entry.lock().cwd = Some(cwd.clone());
                            }
                            // `{ cwd }` rather than a bare string so this payload
                            // is identical to the `cwd` grid-WS frame — a WS frame
                            // must carry a `type` discriminator and therefore
                            // cannot be a bare string. Same shape on both
                            // transports means no branch in CanvasTerminal.
                            #[cfg(feature = "desktop")]
                            if let Some(a) = state.app_handle.read().as_ref() {
                                let _ = a.emit(
                                    &format!("pty-cwd-{session_id}"),
                                    serde_json::json!({ "cwd": cwd }),
                                );
                            }
                            state.emit_pty_event(crate::state::AppEvent::PtyCwd {
                                session_id: session_id.to_string(),
                                cwd,
                            });
                        }
                    }
                    TermEvent::Tuic {
                        verb,
                        payload,
                        line,
                    } => match verb.as_str() {
                        "state" => {
                            // idle/busy drive the shell-state machine; awaiting is
                            // ignored here (it's a separate field). The awaiting_input
                            // field is driven by Question/UserInput events instead.
                            explicit_idle_in_chunk |= payload == "idle";
                            self.handle_tuic_state(&payload, session_id, state);
                            if let Some(evt) = tuic_state_awaiting_event(&payload, line as i64) {
                                tuic_events.push(evt);
                            }
                        }
                        "suggest" => {
                            // Tolerate an optional `[ … ]` wrapper so this OSC
                            // channel accepts the same payload as the text token
                            // (`suggest: [ A | B | C ]`).
                            let inner = payload.trim();
                            let inner = inner.strip_prefix('[').unwrap_or(inner);
                            let inner = inner.strip_suffix(']').unwrap_or(inner);
                            let items: Vec<String> = inner
                                .split('|')
                                .map(|s| s.trim().to_string())
                                .filter(|s| !s.is_empty())
                                .collect();
                            if !items.is_empty() {
                                tuic_events.push(ParsedEvent::Suggest { items });
                            }
                        }
                        "intent" => {
                            let (text, title) = if let Some(paren_start) = payload.rfind('(') {
                                let desc = payload[..paren_start].trim().to_string();
                                let t = payload[paren_start + 1..]
                                    .trim_end_matches(')')
                                    .trim()
                                    .to_string();
                                (desc, if t.is_empty() { None } else { Some(t) })
                            } else {
                                (payload.clone(), None)
                            };
                            if let Some(event) = silence.lock().accept_intent(text, title) {
                                tuic_events.push(event);
                            }
                        }
                        "block" => {
                            let (action, exit_code) =
                                if let Some(rest) = payload.strip_prefix("end;") {
                                    ("end".to_string(), rest.parse::<i32>().ok())
                                } else {
                                    (payload.clone(), None)
                                };
                            if action == "start" || action == "end" {
                                tuic_events.push(ParsedEvent::AgentBlock {
                                    action,
                                    line: line as i64,
                                    exit_code,
                                });
                            }
                        }
                        _ => {}
                    },
                    TermEvent::MouseCursorDirty | TermEvent::CursorBlinkingChange => {}
                }
            }
        }

        // Write to ring buffer and broadcast to WebSocket clients while
        // holding the ring lock. Serializing these two steps prevents a race
        // with WS catch-up: a newly-connecting handler that also takes
        // ring.lock() for its snapshot cannot observe a state where the byte
        // is in the ring but also still queued for live delivery, which
        // would cause the catch-up and the live stream to replay the same
        // bytes to the client.
        let mut output_offset_after_chunk = 0;
        if let Some(ring) = state.session_maps.output_buffers.get(session_id) {
            let mut ring_guard = ring.lock();
            ring_guard.write(data.as_bytes());
            output_offset_after_chunk = ring_guard.total_written;
            crate::state::broadcast_to_ws_clients(&state.ws_clients, session_id, data);
            drop(ring_guard);
        }

        // Parse events: OSC 9;4 progress from raw stream, others from clean rows.
        // One critical section for every flag this chunk reads out of
        // SilenceState — `hook_state_seen` used to take the lock a second time
        // a few lines below, for a single bool.
        let (in_resize_grace, in_startup_grace, parser_dedup_reset, hook_state_seen) = {
            let mut sl = silence.lock();
            (
                sl.is_resize_grace(),
                sl.is_startup_grace(),
                sl.take_parser_dedup_reset(),
                sl.hook_state_seen,
            )
        };
        // A line was submitted since the last chunk: the same API error or
        // session conflict recurring now is a new failure, not a repaint.
        if parser_dedup_reset {
            self.parser.reset_input_dedup();
        }
        let suppress_notifications = in_resize_grace || in_startup_grace;
        let mut events = tuic_events;
        // Hook-instrumented sessions get awaiting from OSC 7770; drop heuristic
        // (regex) Question events from the parser so they don't double-fire.
        let hook_instrumented = state
            .session_maps
            .session_states
            .get(session_id)
            .map(|s| s.hook_instrumented)
            .unwrap_or(false)
            && hook_state_seen;
        // Capture tap: off by default, one relaxed atomic load when it is.
        // Recorded before any parsing so a fixture replays exactly the bytes
        // the detectors saw, chunk boundaries included — those boundaries are
        // themselves a failure mode (a split OSC matches nothing).
        if crate::pty_capture::is_enabled() {
            let capture_geometry = state.grid.vt_log_buffers.get(session_id).map(|vt| {
                let vt = vt.lock();
                (vt.grid_screen_lines() as u16, vt.grid_columns() as u16)
            });
            crate::pty_capture::record_with_geometry(session_id, data.as_bytes(), capture_geometry);
        }

        raw_stream_events(&mut self.raw_carry, data, &mut events);
        let agent_active_for_parse = state
            .session_maps
            .session_states
            .get(session_id)
            .map(|s| s.agent_type.is_some())
            .unwrap_or(false);
        let mut breaks = IntentBreaks::default();
        if agent_active_for_parse
            && (intent_candidate.is_some() || silence.lock().open_intent.is_some())
        {
            self.intent_break_parser
                .advance(&mut breaks, data.as_bytes());
        }
        let mut intent_events = Vec::new();
        if agent_active_for_parse {
            let candidate = intent_candidate.and_then(|(line, anchor_text)| {
                let ParsedEvent::Intent { text, title } =
                    crate::output_parser::parse_intent(&line.text, true)?
                else {
                    return None;
                };
                Some((line, anchor_text, text, title))
            });
            let mut sl = silence.lock();
            let mut candidate_grew = false;
            let mut same_anchor_repaint = false;
            if let Some((line, anchor_text, text, title)) = candidate {
                candidate_grew = sl.open_intent.as_ref().map_or(!breaks.strong, |open| {
                    text.starts_with(&open.text) && text.len() > open.text.len()
                });
                same_anchor_repaint = sl
                    .open_intent
                    .as_ref()
                    .is_some_and(|open| open.anchor_text == anchor_text);
                let compatible = sl.open_intent.as_ref().is_some_and(|open| {
                    text.starts_with(&open.text)
                        || open.text.starts_with(&text)
                        || same_anchor_repaint
                });
                if sl.open_intent.is_some()
                    && !compatible
                    && let Some(event) = sl.close_open_intent()
                {
                    intent_events.push(event);
                }
                // Ink can erase the continuation row, briefly paint the next
                // paragraph there, then move the intact anchor up one row and
                // finish its title. Keep the longer candidate during that gap.
                if let Some(title) = title {
                    sl.open_intent = None;
                    if let Some(event) = sl.accept_intent(text, Some(title)) {
                        intent_events.push(event);
                    }
                } else if sl.last_intent.as_ref() != Some(&(text.clone(), None))
                    && !(same_anchor_repaint
                        && sl
                            .open_intent
                            .as_ref()
                            .is_some_and(|open| !text.starts_with(&open.text)))
                {
                    let start_row = if same_anchor_repaint {
                        sl.open_intent
                            .as_ref()
                            .map_or(intent_origin + line.start_row, |open| open.start_row)
                    } else {
                        intent_origin + line.start_row
                    };
                    sl.open_intent = Some(OpenIntent {
                        text,
                        anchor_text,
                        start_row,
                        end_row: start_row + line.end_row.saturating_sub(line.start_row),
                    });
                }
            }
            let close = sl.open_intent.as_ref().is_some_and(|open| {
                let end_row = open.end_row.saturating_sub(intent_origin);
                let prose_below = !candidate_grew
                    && !same_anchor_repaint
                    && changed_rows.iter().any(|row| {
                        row.row_index > end_row
                            && !row.text.trim().is_empty()
                            && !is_chrome_row(&row.text)
                            && crate::output_parser::structured_token_anchor(&row.text).is_none()
                    });
                let broken_line = breaks.any
                    && !candidate_grew
                    && !same_anchor_repaint
                    && cursor_row.is_some_and(|row| row > end_row);
                let replaced = !same_anchor_repaint
                    && changed_rows.iter().any(|row| {
                        intent_origin + row.row_index == open.start_row
                            && crate::output_parser::structured_token_anchor(&row.text)
                                != Some(crate::output_parser::StructuredTokenAnchor::Intent)
                    });
                (prose_below || broken_line || replaced) && !incomplete_intent_title(&open.text)
                    || explicit_idle_in_chunk
            });
            if close && let Some(event) = sl.close_open_intent() {
                intent_events.push(event);
            }
        }
        // Cursor-completeness guard: parse a suggest token from the bounded grid
        // prefix through the cursor, never from stale cells to its right. When a
        // soft-wrapped continuation changes in a later chunk, replace its whole
        // physical range with one synthetic logical row so the unchanged anchor
        // remains available to the existing parser. Intent capture is handled
        // by the open state above.
        let mut structured_rows = None;
        let structured_prefix = logical_prefix
            .filter(|prefix| crate::output_parser::structured_token_anchor(&prefix.text).is_some())
            .or_else(|| {
                physical_prefix.filter(|prefix| {
                    self.parser
                        .is_complete_suggest(&prefix.text, agent_active_for_parse)
                })
            });
        if let Some(prefix) = structured_prefix {
            let intersects = changed_rows
                .iter()
                .any(|row| (prefix.start_row..=prefix.end_row).contains(&row.row_index));
            if intersects
                && let Some(anchor) = crate::output_parser::structured_token_anchor(&prefix.text)
            {
                let complete_suggest = anchor
                    == crate::output_parser::StructuredTokenAnchor::Suggest
                    && self
                        .parser
                        .is_complete_suggest(&prefix.text, agent_active_for_parse);
                let mut rows: Vec<_> = changed_rows
                    .iter()
                    .filter(|row| !(prefix.start_row..=prefix.end_row).contains(&row.row_index))
                    .cloned()
                    .collect();
                if complete_suggest {
                    rows.push(crate::state::ChangedRow {
                        row_index: prefix.start_row,
                        text: prefix.text,
                    });
                    rows.sort_by_key(|row| row.row_index);
                }
                structured_rows = Some(rows);
            }
        } else if let Some(cursor_row) = cursor_row
            && changed_rows.iter().any(|row| {
                row.row_index == cursor_row
                    && crate::output_parser::structured_token_anchor(&row.text).is_some()
            })
        {
            structured_rows = Some(
                changed_rows
                    .iter()
                    .filter(|row| row.row_index != cursor_row)
                    .cloned()
                    .collect(),
            );
        }
        let rows = structured_rows.as_deref().unwrap_or(&changed_rows);
        events.extend(
            self.parser
                .parse_clean_lines(rows, agent_active_for_parse)
                .into_iter()
                .filter(|e| !suppress_heuristic_question(hook_instrumented, e)),
        );
        events.extend(intent_events);

        // Heuristic agent-block detection for Claude Code tool calls.
        // CC renders tool calls as `⏺ ToolName(args)` — detect these and
        // synthesize AgentBlock start/end events so the block system works
        // without CC emitting OSC 7770;block= sequences.
        if !agent_active_for_parse && let Some(prev) = self.last_agent_block_line.take() {
            events.push(ParsedEvent::AgentBlock {
                action: "end".into(),
                line: prev as i64,
                exit_code: None,
            });
        }
        if agent_active_for_parse {
            for row in &changed_rows {
                if is_cc_tool_call_header(&row.text) {
                    let abs_line = history_size + row.row_index;
                    if Some(abs_line) == self.last_agent_block_line {
                        continue;
                    }
                    if let Some(prev) = self.last_agent_block_line {
                        events.push(ParsedEvent::AgentBlock {
                            action: "end".into(),
                            line: prev as i64,
                            exit_code: None,
                        });
                    }
                    events.push(ParsedEvent::AgentBlock {
                        action: "start".into(),
                        line: abs_line as i64,
                        exit_code: None,
                    });
                    self.last_agent_block_line = Some(abs_line);
                }
            }
        }

        // The snapshot was refilled once inside the vt_log lock scope above and
        // is handed to every consumer below as one borrowed slice.
        let screen_cache: Option<&[String]> = screen_present.then_some(screen_buf.as_slice());

        // Slash menu detection — use full screen rows (not chrome-trimmed).
        // Claude Code v2.1+ renders autocomplete items BELOW the prompt chrome,
        // so trimming to above-chrome would discard the menu. parse_slash_menu
        // scans bottom-up, skips empty rows, and stops at the first non-matching
        // row (separator/chrome), so it safely finds items regardless of position.
        let slash_on = state
            .session_maps
            .slash_mode
            .get(session_id)
            .is_some_and(|v| v.load(std::sync::atomic::Ordering::Relaxed));
        if slash_on && let Some(screen) = screen_cache {
            // This runs in the per-PTY-chunk hot path. Do not log each parse:
            // a stale slash-mode flag during sustained output previously sent
            // thousands of identical records through the application logger,
            // adding avoidable lock/contention pressure to terminal delivery.
            if let Some(evt) = crate::output_parser::parse_slash_menu(screen) {
                events.push(evt);
            }
        }

        // ChoicePrompt detection — numbered confirmation dialogs rendered below
        // the prompt line (edit-confirm, bash-confirm, apply-patch). Runs on
        // every chunk (unlike slash_menu which is gated by slash_mode) because
        // these dialogs appear asynchronously when the agent requests input.
        // Parser uses a strict shape (title with ?/verb + ≥2 numbered options)
        // so false-positive cost is low. Dedup via last_choice_prompt_sig
        // guards against repaint re-emission.
        if events
            .iter()
            .any(|event| matches!(event, ParsedEvent::UserInput { .. }))
        {
            self.choice_prompt_needs_repaint = true;
        }
        if let Some(screen) = screen_cache {
            let choice = if agent_type.as_deref() == Some("claude") {
                crate::output_parser::parse_claude_ask_user_question(screen)
                    .or_else(|| crate::output_parser::parse_choice_prompt(screen))
            } else {
                crate::output_parser::parse_choice_prompt(screen)
            };
            match choice {
                Some(evt) => events.push(evt),
                // Dialog is no longer on screen — retire its dedup signature so the
                // same dialog is detected again the next time it appears, instead of
                // being swallowed for the rest of the session.
                None => {
                    self.choice_prompt_needs_repaint = false;
                    if self.last_choice_prompt_sig.take().is_some() {
                        events.push(ParsedEvent::ChoiceCleared);
                    }
                }
            }
        }

        // Retire the question dedup as soon as its prompt leaves the screen. The
        // marker exists only to stop an Ink menu repaint from re-notifying while
        // the SAME prompt is still displayed; it used to live for the session's
        // lifetime, and since every Ink footer is the byte-identical
        // "Enter to select · ↑/↓ to navigate · Esc to cancel", the first menu of a
        // session permanently swallowed every later one — the awaiting badge was a
        // one-shot per session. Screen absence is the real end-of-prompt signal:
        // the user answering, the agent withdrawing the prompt, and a repaint that
        // scrolls it away all collapse into it.
        if let Some(screen) = screen_cache {
            let prompt_gone = self
                .last_question_text
                .as_deref()
                .is_some_and(|last| !screen.iter().any(|row| row.contains(last)));
            if prompt_gone {
                self.last_question_text = None;
            }
        }

        // Re-arm awaiting while an Ink dialog is still on screen.
        //
        // `parse_clean_lines` only sees CHANGED rows, and the footer row is
        // byte-identical across the sub-questions of a multi-question
        // AskUserQuestion ("⊠ CLI.md · □ Exit codes · ✓ Submit"). Answering the
        // first sub-question clears awaiting; the second one repaints its title
        // and options but NOT the footer, so nothing ever set it again and the tab
        // read "working" while the agent sat blocked on the user.
        //
        // Presence of the footer is the whole signal — no title, option or tab-bar
        // parsing, none of which survives the wizard advancing. It re-arms only
        // when the badge is actually off, so a repaint cannot storm: one event per
        // spurious clear, never one per frame. This applies to hooked sessions
        // too: later busy hooks can clear awaiting while the dialog stays open,
        // and a multi-question tool emits its awaiting hook only once.
        if let Some(screen) = screen_cache {
            let (awaiting, has_choice) = state
                .session_maps
                .session_states
                .get(session_id)
                .map(|s| (s.awaiting_input, s.choice_prompt.is_some()))
                .unwrap_or((false, false));
            // The accumulator has not seen this chunk yet. Respect its LAST
            // awaiting mutation: an earlier Question or ChoicePrompt followed
            // by hook-busy's UserInput no longer protects the badge from being cleared.
            let pending_awaiting = events.iter().rev().find_map(|event| match event {
                ParsedEvent::Question { .. } | ParsedEvent::ChoicePrompt { .. } => Some(true),
                ParsedEvent::UserInput { .. } | ParsedEvent::ChoiceCleared => Some(false),
                _ => None,
            });
            if let Some(evt) = rearm_awaiting_for_open_dialog(
                screen,
                pending_awaiting.unwrap_or(awaiting),
                has_choice,
                pending_awaiting == Some(true),
            ) {
                // Clear the dedup: the badge is off, so this event must reach state.
                self.last_question_text = None;
                events.push(evt);
            }
        }

        let regex_found_question = if suppress_notifications {
            false
        } else {
            events
                .iter()
                .any(|e| matches!(e, ParsedEvent::Question { .. }))
        };

        // Read the turn epoch once so every event in this chunk is attributed to
        // the same turn, and per-turn dedup cannot straddle a boundary mid-chunk.
        let turn_epoch = state
            .session_maps
            .session_states
            .get(session_id)
            .map(|session| session.turn_epoch)
            .unwrap_or(0);

        // Emit events with dedup, grace filtering, and PlanFile resolution.
        for event in &events {
            // During startup/resize grace, suppress low-confidence notifications to
            // avoid boot-noise false positives — but let CONFIDENT questions through.
            // An agent can signal an approval prompt via its "Action Required" title
            // (confident), yet its continuous animation keeps resetting last_output,
            // so the startup grace never settles by silence and would otherwise
            // suppress the approval prompt for the full 120s safety cap.
            let suppress_this = suppress_notifications
                && match event {
                    ParsedEvent::Question { confident, .. } => !*confident,
                    ParsedEvent::RateLimit { .. } | ParsedEvent::ApiError { .. } => true,
                    _ => false,
                };
            if suppress_this {
                continue;
            }

            // Count what the model actually emitted, at the funnel every parsed
            // event passes through. Counted here rather than at emission because a
            // suggest parked for a turn that ends early is still a marker the
            // agent produced (#4421).
            match event {
                ParsedEvent::Intent { .. } => {
                    publish_intent_event(state, session_id, event, turn_epoch);
                    continue;
                }
                ParsedEvent::Suggest { .. } => {
                    state.note_marker(session_id, crate::state::MarkerKind::Suggest)
                }
                _ => {}
            }

            if let ParsedEvent::AgentSessionConflict { kind, .. } = event
                && matches!(
                    self.terminal_mode,
                    crate::ai_agent::tui_detect::TerminalMode::Shell
                )
            {
                self.mark_session_no_inject(kind);
                continue;
            }

            // Suggest: park in SilenceState and defer emission until silence
            // confirms the turn has ended. The frontend used to buffer these
            // events in `pendingSuggest` to compensate for suggest arriving
            // before `shell-state: idle`; gating the emission backend-side
            // removes the race and simplifies the Terminal event handler.
            if let ParsedEvent::Suggest { items } = event {
                let mut silence_state = silence.lock();
                silence_state.mark_suggest_candidate(items.clone(), turn_epoch);
                continue;
            }

            // Dedup status-line: skip only a repeat within the same turn.
            if let ParsedEvent::StatusLine { task_name, .. } = event {
                let seen = (turn_epoch, task_name.clone());
                if self.last_status_task.as_ref() == Some(&seen) {
                    continue;
                }
                self.last_status_task = Some(seen);
            }

            // Dedup question: skip if same prompt_text already emitted. Retired as
            // soon as the prompt leaves the screen (see the screen-absence reset
            // above), so this guards one pending prompt, not the whole session.
            if let ParsedEvent::Question { prompt_text, .. } = event {
                if self.last_question_text.as_deref() == Some(prompt_text.as_str()) {
                    continue;
                }
                self.last_question_text = Some(prompt_text.clone());
            }

            // Dedup choice-prompt: skip if same (title + option keys) already emitted.
            // Signature keeps option order but ignores highlighted drift so cursor
            // movement within the dialog doesn't re-fire. Retired when the dialog
            // leaves the screen (see the parse site above).
            if let ParsedEvent::ChoicePrompt { title, options, .. } = event {
                let sig = format!(
                    "{}|{}",
                    title,
                    options
                        .iter()
                        .map(|o| o.key.as_str())
                        .collect::<Vec<_>>()
                        .join(","),
                );
                let dialog_row_repainted = choice_changed_rows.iter().any(|&row_index| {
                    screen_cache
                        .and_then(|screen| screen.get(row_index))
                        .is_some_and(|row| {
                            row.contains(title)
                                || options.iter().any(|option| row.contains(&option.label))
                        })
                });
                if self.last_choice_prompt_sig.as_deref() == Some(sig.as_str())
                    && !(self.choice_prompt_needs_repaint && dialog_row_repainted)
                {
                    continue;
                }
                self.last_choice_prompt_sig = Some(sig);
                self.choice_prompt_needs_repaint = false;
            }

            // Resolve relative plan-file paths to absolute using session CWD.
            // If the file doesn't exist yet (agent announces before writing),
            // queue it for retry — checked each chunk for up to 10 seconds.
            let resolved = if let ParsedEvent::PlanFile { path } = event {
                match self.resolve_planfile_path(path) {
                    Some(p)
                        if self.emitted_planfiles.contains(&p)
                            || self.gaveup_planfiles.contains(&p) =>
                    {
                        // Already emitted, or it exhausted its retry window — skip
                        // (spinner redraws re-parse the same on-screen line).
                        continue;
                    }
                    Some(p) if std::path::Path::new(&p).is_file() => {
                        tracing::info!("[plan-file] Detected: {p} (cwd={:?})", self.session_cwd);
                        self.emitted_planfiles.insert(p.clone());
                        Some(ParsedEvent::PlanFile { path: p })
                    }
                    Some(p) => {
                        // File not on disk yet — queue for retry if not already pending
                        if !self.pending_planfiles.iter().any(|(pp, _)| pp == &p) {
                            tracing::debug!(
                                "[plan-file] Queued for retry: {p} (cwd={:?})",
                                self.session_cwd
                            );
                            let deadline =
                                std::time::Instant::now() + std::time::Duration::from_secs(10);
                            self.pending_planfiles.push((p, deadline));
                        }
                        continue;
                    }
                    None => {
                        tracing::warn!(
                            "[plan-file] Cannot resolve relative path: {path} (cwd={:?})",
                            self.session_cwd
                        );
                        continue;
                    }
                }
            } else {
                None
            };

            let emit_event = resolved.as_ref().unwrap_or(event);

            // Serialize once, reuse for both broadcast and Tauri IPC
            if let Ok(mut json) = serde_json::to_value(emit_event) {
                if let Some(object) = json.as_object_mut() {
                    object.insert("_turn_epoch".to_string(), turn_epoch.into());
                }
                #[cfg(feature = "desktop")]
                if let Some(app) = state.app_handle.read().as_ref() {
                    let _ = app.emit(&format!("pty-parsed-{session_id}"), &json);
                }
                state.emit_pty_event(crate::state::AppEvent::PtyParsed {
                    session_id: session_id.to_string(),
                    parsed: json.into(),
                });
            }
        }

        // Codex can cancel an approval with Esc without emitting a typed line
        // or protocol busy marker. The title has left Action Required and the
        // recorded screen shows the cancellation above its ready composer.
        // Carry the originating prompt so a later, different question cannot
        // be cleared. Require a newly painted cancellation as well: old ones
        // remain in the transcript when another approval opens.
        if agent_type.as_deref() == Some("codex")
            && self.codex_approval_question.is_some()
            && changed_rows
                .iter()
                .any(|row| row.text.contains("You canceled the request"))
        {
            self.codex_approval_canceled = true;
        }
        let canceled_codex_approval = if agent_type.as_deref() == Some("codex")
            && !self.title_awaiting
            && self.codex_approval_canceled
            && matches!(
                screen_activity,
                AgentScreenActivity::Ready | AgentScreenActivity::Interrupted
            )
            && screen_cache.is_some_and(|screen| {
                screen
                    .iter()
                    .any(|row| row.contains("You canceled the request"))
            })
            && !events.iter().any(|event| {
                matches!(
                    event,
                    ParsedEvent::Question { .. } | ParsedEvent::ChoicePrompt { .. }
                )
            }) {
            self.codex_approval_question
                .as_deref()
                .and_then(|expected| {
                    state
                        .session_maps
                        .session_states
                        .get(session_id)
                        .and_then(|session| {
                            (session.awaiting_input
                                && session.question_confident
                                && session.question_text.as_deref() == Some(expected))
                            .then(|| (expected.to_string(), session.turn_epoch))
                        })
                })
        } else {
            None
        };
        if let Some((expected_question_text, turn_epoch)) = canceled_codex_approval {
            self.codex_approval_question = None;
            self.codex_approval_canceled = false;
            state.emit_pty_event(crate::state::AppEvent::PtyParsed {
                session_id: session_id.to_string(),
                parsed: serde_json::json!({
                    "type": "protocol-question-cleared",
                    "expected_question_text": expected_question_text,
                    "_turn_epoch": turn_epoch,
                })
                .into(),
            });
        }

        // Claude prints this tool result when Esc dismisses AskUserQuestion.
        // A bare Esc has no line-input event, so its confident notification
        // otherwise stays latched after the completed turn. Require the newly
        // painted result and a ready composer; a dialog still on screen must
        // retain its badge, including while Claude repaints its status line.
        let declined_screen = agent_type.as_deref() == Some("claude")
            && screen_activity == AgentScreenActivity::Ready
            && changed_rows.iter().any(|row| {
                row.text.contains("User declined") && row.text.contains("answer questions")
            })
            && screen_cache
                .is_some_and(|screen| crate::output_parser::ink_dialog_footer(screen).is_none())
            && !events.iter().any(|event| {
                matches!(
                    event,
                    ParsedEvent::Question { .. } | ParsedEvent::ChoicePrompt { .. }
                )
            });
        let declined_session = declined_screen
            .then(|| state.session_maps.session_states.get(session_id))
            .flatten()
            .filter(|session| session.awaiting_input && session.question_confident)
            .map(|session| {
                // A live choice overlay owns its own clear; only the shell
                // state below is ours then.
                let question = if session.choice_prompt.is_none() {
                    session.question_text.clone()
                } else {
                    None
                };
                (question, session.turn_epoch)
            });
        if let Some((question, turn_epoch)) = declined_session {
            if let Some(expected_question_text) = question {
                state.emit_pty_event(crate::state::AppEvent::PtyParsed {
                    session_id: session_id.to_string(),
                    parsed: serde_json::json!({
                        "type": "protocol-question-cleared",
                        "expected_question_text": expected_question_text,
                        "_turn_epoch": turn_epoch,
                    })
                    .into(),
                });
            }
            // Esc ends the turn without a Stop hook (live capture: busy, busy,
            // awaiting, then nothing), so the hook-driven BUSY would stay latched
            // with the queue stuck until the next input. The ready composer under
            // the fresh cancellation is the Stop it never sent.
            transition_explicit_shell_state(state, session_id, SHELL_IDLE, "idle", true);
        }

        // Update silence state for fallback question detection.
        let has_status_line = events
            .iter()
            .any(|e| matches!(e, ParsedEvent::StatusLine { .. }));
        let last_q_line = extract_question_line(&changed_rows);
        // A chunk is chrome-only when no real output reached the screen.
        // Path 0: changed_rows is empty — nothing visible happened (cursor
        //   blink, OSC title update, mouse report, SGR-only sequence). Must
        //   count as chrome-only or these periodic re-emits latch the shell
        //   state to busy forever during genuine idle.
        // Path 1: every row has a chrome marker (is_chrome_row).
        // Path 2: parse_status_line detected a spinner pattern (Gemini braille,
        //   Aider Knight Rider) AND no row contains real agent output. A row is
        //   "real output" if it is not chrome and not blank — this prevents
        //   has_status_line from suppressing chunks that mix spinner + output.
        let all_chrome_markers = changed_rows.iter().all(|r| is_chrome_row(&r.text));
        let has_suggest = events
            .iter()
            .any(|e| matches!(e, ParsedEvent::Suggest { .. }))
            || rows.iter().any(|row| {
                self.parser
                    .is_complete_suggest(&row.text, agent_active_for_parse)
            });
        let no_real_output = changed_rows.iter().all(|r| {
            is_chrome_row(&r.text)
                || r.text.trim().is_empty()
                || crate::chrome::is_separator_line(&r.text)
                || crate::chrome::is_prompt_line(&r.text)
                // Suggest tokens are protocol markers, not real agent output.
                // Without this, a visible suggest row makes the chunk look like
                // "real output" and increments the question staleness counter.
                || (has_suggest && is_suggest_row(&r.text))
        });
        let chrome_only = !regex_found_question
            && last_q_line.is_none()
            && (changed_rows.is_empty()
                || all_chrome_markers
                || ((has_status_line || has_suggest) && no_real_output));
        // Suggest-only: chunk produced only Suggest events (no real text).
        let suggest_only = has_suggest
            && !regex_found_question
            && last_q_line.is_none()
            && !has_status_line
            && no_real_output;
        // Tool-error detection: scan visible rows for `Error: Exit code N`
        // emitted by Claude Code / Codex at the end of a failing tool call.
        // Fires playError() via silence_timer when followed only by chrome
        // until SILENCE_TOOL_ERROR_THRESHOLD elapses (= turn ended on error).
        //
        // The scan is two regexes per changed row and it used to run INSIDE the
        // SilenceState critical section, holding the lock the silence timer and
        // every sibling reader contend for while it matched. Only its verdict
        // needs the lock.
        let mut error_line: Option<String> = None;
        let mut retry_seen = false;
        for row in changed_rows.iter() {
            if is_retry_line(&row.text) {
                retry_seen = true;
            } else if is_tool_error_line(&row.text) {
                error_line = Some(row.text.trim().to_string());
            }
        }
        {
            let mut sl = silence.lock();
            // Shared with the silence timer (#744-138c): the screen has not
            // changed since this classification unless a later chunk arrives
            // to overwrite it, so the timer reuses this instead of calling
            // `detect_agent_screen_activity` itself — see `cached_screen_activity`.
            sl.record_screen_activity(screen_activity, output_offset_after_chunk);
            sl.on_chunk(
                regex_found_question,
                last_q_line,
                has_status_line,
                chrome_only,
                suggest_only,
            );

            if retry_seen {
                // Agent is auto-retrying a failed API call — hold BUSY across the
                // frozen gap between attempts. Takes precedence over the recovery
                // clear below: the retry line IS real output but is not recovery.
                sl.mark_api_retry();
            } else if let Some(line) = error_line {
                sl.mark_tool_error_candidate(line);
            } else if !chrome_only {
                // Real output without an error/retry line → agent recovered/continued.
                sl.clear_tool_error_on_recovery();
            }
        }

        // Screen activity is evaluated on the full, unfiltered snapshot. The
        // generic chrome cutoff is a presentation/logging boundary and must not
        // erase agent-specific liveness evidence (Codex tool separators are the
        // canonical counterexample).
        // `screen_activity` was classified inside the vt_log lock above, from a
        // borrowed screen — see the note there.
        let working_status_moved = changed_rows
            .iter()
            .any(|row| crate::chrome::is_working_status_row(&row.text));
        let apply_working =
            screen_activity == AgentScreenActivity::Working && !explicit_idle_in_chunk;
        let working_source = if working_status_moved {
            "working-screen-movement"
        } else {
            "working-screen"
        };
        let can_reopen_completed = apply_working && {
            let working_agent_type = state
                .session_maps
                .session_states
                .get(session_id)
                .and_then(|session| session.agent_type.clone());
            working_agent_type.as_deref() == Some("claude")
                || (working_agent_type.as_deref() == Some("codex")
                    && working_source == "working-screen-movement")
        };

        // Stamp last_output_ms for real output and for active spinner repaints.
        // Spinner rows (dingbats ✻, braille ⠋, Aider ░█) prove the agent is
        // alive even though they are chrome-only — keeping the timestamp fresh
        // prevents should_transition_idle from firing mid-think.
        //
        // Spinner detection runs on the SAME post-cutoff `changed_rows` as
        // everything else. Real spinners (Gemini braille, Aider Knight Rider,
        // Claude `✻ Thinking…`) all render ABOVE the input separator and LEAD
        // their row, so they survive the chrome cutoff and still keep the agent
        // alive here. A status-line HUD's `█░` progress bar or a `·`-bearing
        // footer is NOT a spinner (`is_spinner_row` requires the glyph to lead
        // the line, #446-596f), so it can never keep a session busy even if it
        // renders above the cutoff.
        let has_spinner = chrome_only
            && changed_rows
                .iter()
                .any(|r| crate::chrome::is_spinner_row(&r.text));
        //
        // This, the resize-grace re-arm and the BUSY gate below all read or
        // write the same SilenceState, and each used to take the lock for
        // itself. They are one critical section now; nothing between them
        // touches SilenceState (`stamp_last_output_now` writes an AppState
        // atomic, `begin_suggest_working_turn` the parser), and the ordering
        // inside the section is the ordering the three had.
        let real_activity = (!chrome_only || has_spinner) && !explicit_idle_in_chunk;
        // The working-evidence gate (turn_completed/explicit_idle blocking a
        // stale Working row, unless `can_reopen_completed`) folded in here so
        // this is the ONLY SilenceState lock in the chunk path — previously
        // `apply_working_evidence` took its own separate lock ahead of this
        // one (DEFERRED 2026-09-06). Working evidence and real/spinner
        // activity are both one-shot busy evidence (see the comment on
        // `apply_working_evidence`): recorded to win THIS chunk's CAS via
        // `decide()`, then cleared so they cannot block a later, unrelated
        // idle-evidence recording (e.g. the silence-timeout fallback for a
        // plain shell or an agent with no OSC133/hook integration).
        let (in_resize_grace_after, evidence_snapshot, working_applied, reopened_completion) = {
            let mut sl = silence.lock();
            let mut working_applied = false;
            let mut reopened_completion = false;
            if apply_working {
                let turn_completed = state
                    .session_maps
                    .session_states
                    .get(session_id)
                    .is_some_and(|session| sl.completion_declared_for_epoch(session.turn_epoch));
                let blocked = (turn_completed || sl.explicit_idle()) && !can_reopen_completed;
                if !blocked {
                    let reopen = can_reopen_completed && (turn_completed || sl.explicit_idle());
                    if reopen {
                        // Claude can emit Stop/suggest before a blocking Stop hook
                        // finishes; Codex can start an internal continuation
                        // without a PTY submission. Current semantic movement is
                        // stronger than either stale boundary.
                        sl.reset_suggest_memory();
                        reopened_completion = true;
                    }
                    sl.note_working_screen();
                    invalidate_background_probe_boundary_locked(state, session_id);
                    let rank = if reopen {
                        EvidenceRank::Protocol
                    } else {
                        EvidenceRank::Screen
                    };
                    sl.evidence.record_busy(rank, working_source);
                    working_applied = true;
                }
            }
            let activity_source = if has_spinner {
                "spinner-active"
            } else {
                "real-activity"
            };
            // Arbitrate BEFORE updating activity metadata: those updates clear
            // idle evidence. Doing them first bypasses record_busy's rank gate
            // and lets decorative repaints reopen a hook-completed turn.
            if real_activity
                && (sl.evidence.busy.is_some()
                    || sl
                        .evidence
                        .record_busy(EvidenceRank::Screen, activity_source))
            {
                if has_spinner {
                    sl.note_working_screen();
                } else {
                    sl.note_real_activity();
                }
                invalidate_background_probe_boundary_locked(state, session_id);
            }
            // SIGWINCH reflow repaints content rows for longer than the initial 1s
            // resize grace, but a reflow never grows the buffer — it only repaints
            // existing rows. While such pure-repaint chunks keep arriving within the
            // grace window, re-arm the grace so a resize never flips an idle agent to
            // busy. A growing chunk (genuine new output) is NOT extended, so real work
            // started right after a resize still registers as busy. An already-busy
            // session is unaffected (idle transitions are silence-timer only).
            //
            // The extension has no stop condition of its own — each qualifying chunk
            // pushes the deadline a full RESIZE_GRACE forward — so `vt_output_grew`
            // is the ONLY thing that ends it. It must stay a signal that a working
            // agent actually trips; see its definition for why the alternate screen
            // needs its own answer rather than the durable-log total.
            if !vt_output_grew && sl.is_resize_grace() {
                sl.on_resize();
            }
            (
                sl.is_resize_grace(),
                sl.evidence.clone(),
                working_applied,
                reopened_completion,
            )
        };
        if working_applied {
            stamp_last_output_now(state, session_id, now_epoch_ms());
        }
        if reopened_completion
            && let Some(mut session) = state.session_maps.session_states.get_mut(session_id)
        {
            session.suggested_actions = None;
        }
        if real_activity {
            stamp_last_output_now(state, session_id, now_epoch_ms());
        }

        // Suggest dedup is intentionally not reset on submission: the previous
        // marker may repaint while still visible. Once this turn has real
        // working evidence, however, an identical terminal marker is a valid
        // new completion. Update the parser after this chunk was parsed so a
        // stale marker repainted alongside the first activity remains ignored.
        if !explicit_idle_in_chunk
            && (screen_activity == AgentScreenActivity::Working || !chrome_only || has_spinner)
            && let Some(turn_epoch) = state
                .session_maps
                .session_states
                .get(session_id)
                .map(|session| session.turn_epoch)
        {
            self.parser.begin_suggest_working_turn(turn_epoch);
        }

        // Shell state: reader transitions → BUSY on real output OR active spinner.
        // Idle transitions are handled exclusively by the silence timer to
        // eliminate the two-path race that caused 15+ fix/revert cycles.
        // Load `prev` and drop the shell_states Ref before try_shell_transition (which
        // re-gets the same key): holding a Ref across that second get risks the CONC-C
        // re-entrant-read deadlock (story 099-6526).
        //
        // Working-evidence's CAS is unconditional once recorded (matching the
        // old `apply_working_evidence`, which was never gated by resize grace);
        // real/spinner activity's CAS keeps its own resize-grace gate.
        let prev = if working_applied || (real_activity && !in_resize_grace_after) {
            state
                .session_maps
                .shell_states
                .get(session_id)
                .map(|atom| atom.load(std::sync::atomic::Ordering::Acquire))
        } else {
            None
        };
        if let Some(prev) = prev
            && let Some(Transition::ToBusy) = decide(
                &evidence_snapshot,
                prev == SHELL_BUSY,
                std::time::Instant::now(),
            )
            && try_shell_transition(state, session_id, prev, SHELL_BUSY, true)
        {
            emit_shell_state(state, session_id, "busy");
        }
        if working_applied || real_activity {
            // One-shot: this evidence must not persist to block a later,
            // unrelated idle-evidence recording (silence-timeout fallback,
            // ready-screen confirmation) — see the comment above.
            let mut silence = silence.lock();
            if silence.evidence.busy.is_some_and(|busy| {
                busy.source == working_source
                    || busy.source == "spinner-active"
                    || busy.source == "real-activity"
            }) {
                silence.evidence.busy = None;
            }
        }

        // Update terminal mode in SessionState when it changes.
        // Detect TUI app from visible screen rows while in alternate buffer.
        if self.terminal_mode.is_fullscreen() {
            let row_texts: Vec<&str> = changed_rows.iter().map(|r| r.text.as_str()).collect();
            if let Some(app) = crate::ai_agent::tui_detect::detect_app_from_rows(&row_texts) {
                self.terminal_mode = self.terminal_mode.with_app_hint(app.to_string());
            }
        }
        if let Some(mut entry) = state.session_maps.session_states.get_mut(session_id) {
            let new_mode = if self.terminal_mode.is_fullscreen() {
                Some(self.terminal_mode.clone())
            } else {
                None
            };
            if entry.terminal_mode != new_mode {
                entry.terminal_mode = new_mode;
            }
        }

        self.screen_buf = screen_buf;
        self.queued_idle_flush |= explicit_idle_in_chunk;
        true
    }
}

/// Process kitty keyboard actions (push/pop/query) shared by both reader threads.
fn process_kitty_actions(kitty_actions: &[KittyAction], session_id: &str, state: &AppState) {
    if kitty_actions.is_empty() {
        return;
    }
    let entry = state
        .session_maps
        .kitty_states
        .entry(session_id.to_string())
        .or_insert_with(|| Mutex::new(KittyKeyboardState::new()));
    let mut ks = entry.lock();
    for action in kitty_actions {
        match action {
            KittyAction::Push(flags) => ks.push(*flags),
            KittyAction::Pop => ks.pop(),
            KittyAction::Query => {
                let flags = ks.current_flags();
                let response = format!("\x1b[?{}u", flags);
                write_terminal_reply(state, session_id, response.as_bytes(), "kitty query");
            }
        }
    }
    let flags = ks.current_flags();
    drop(ks);
    #[cfg(feature = "desktop")]
    if let Some(app) = state.app_handle.read().as_ref() {
        let _ = app.emit(&format!("kitty-keyboard-{session_id}"), flags);
    }
}

/// Whether the session's line discipline would swallow a reply written now.
///
/// **Measured 2026-09-07** (capture `f2bddfb0`, frames 25-35): Claude Code
/// emits `ESC[c` *before* it switches the tty out of cooked mode, so our
/// `ESC[?6c` was painted as the literal text `^[[?6c` at the top of the startup
/// banner and never delivered. Claude, having received nothing, re-queried
/// 100ms later from raw mode and got a clean answer. Withholding the premature
/// reply therefore costs no information: the querier retries once it can read.
///
/// **The predicate is `ICANON`, not `ECHO`, and the difference is load-bearing.**
/// `ECHO` decides whether the bytes are *also* painted on screen; `ICANON`
/// decides whether they are *delivered at all*, because a canonical-mode read
/// blocks until a newline that a terminal reply never contains. In cbreak
/// (`ICANON` off, `ECHO` on) the reply reaches the querier immediately — ugly,
/// but read. Gating on `ECHO` there would withhold a reply nothing else will
/// resend and hang the querier, trading a cosmetic defect for a hang.
///
/// A failure to look the tty up answers "no": the old behaviour was to always
/// write, and a reply we cannot prove is undeliverable is better sent than lost.
#[cfg(not(windows))]
fn tty_would_swallow_reply(state: &AppState, session_id: &str) -> bool {
    let Some(entry) = state.session_maps.sessions.get(session_id) else {
        return false;
    };
    let Some(fd) = entry.value().lock().master.as_raw_fd() else {
        return false;
    };
    let mut termios = std::mem::MaybeUninit::<libc::termios>::uninit();
    // SAFETY: `fd` is the live PTY master owned by the session we just locked,
    // and `tcgetattr` only writes through the pointer when it returns 0.
    if unsafe { libc::tcgetattr(fd, termios.as_mut_ptr()) } != 0 {
        return false;
    }
    unsafe { termios.assume_init() }.c_lflag & libc::ICANON != 0
}

/// Serialize a terminal-generated protocol reply with every other PTY write.
///
/// The writer has its own mutex, separate from the session metadata. Waiting
/// here is safe: the reader remains able to drain PTY output even when another
/// thread is blocked in a kernel write, so the old session-lock deadlock cannot
/// occur and mandatory replies are never discarded merely due to contention.
///
/// Replies are withheld while the tty is canonical — see
/// [`tty_would_swallow_reply`].
fn write_terminal_reply(state: &AppState, session_id: &str, response: &[u8], kind: &str) {
    #[cfg(not(windows))]
    if tty_would_swallow_reply(state, session_id) {
        tracing::debug!(source = "terminal", session_id = %session_id, %kind,
            "Terminal reply withheld: tty is canonical, the querier cannot read it yet");
        return;
    }
    // Terminal protocol replies are not replacement user input.
    let result = state
        .pty_writer(session_id)
        .ok_or_else(|| "Session not found".to_string())
        .and_then(|writer| {
            let mut writer = writer.lock();
            writer
                .write_all(response)
                .and_then(|()| writer.flush())
                .map_err(|error| format!("Write failed: {error}"))
        });
    if let Err(error) = result {
        tracing::warn!(source = "terminal", session_id = %session_id, %kind, %error,
            "Terminal reply failed");
    }
}

/// Flush remaining bytes at EOF and write to ring buffer + WebSocket.
/// Returns the flushed data (may be empty).
fn flush_eof(
    utf8_buf: &mut Utf8ReadBuffer,
    esc_buf: &mut EscapeAwareBuffer,
    session_id: &str,
    state: &AppState,
) -> String {
    let utf8_tail = utf8_buf.flush();
    let esc_remaining = if utf8_tail.is_empty() {
        esc_buf.flush()
    } else {
        let mut flushed = esc_buf.push(&utf8_tail);
        flushed.push_str(&esc_buf.flush());
        flushed
    };
    if !esc_remaining.is_empty()
        && let Some(ring) = state.session_maps.output_buffers.get(session_id)
    {
        let mut ring_guard = ring.lock();
        ring_guard.write(esc_remaining.as_bytes());
        crate::state::broadcast_to_ws_clients(&state.ws_clients, session_id, &esc_remaining);
        drop(ring_guard);
    }
    esc_remaining
}

type VtProcessResult = (
    Vec<crate::state::ChangedRow>,
    bool,
    Vec<crate::terminal_grid::TermEvent>,
    // Whether the reusable screen snapshot was refilled this tick.
    bool,
    AgentScreenActivity,
    Option<usize>,
    Option<crate::terminal_grid::LogicalPrefix>,
    Option<crate::terminal_grid::LogicalPrefix>,
    usize,
    usize,
    Option<(crate::terminal_grid::LogicalPrefix, String)>,
);

#[cfg(feature = "dictation")]
pub use tuic_dictation::continuous::{VoiceHold, VoiceWrite};

/// Detect anomalous ANSI sequences that may cause scroll-jump-to-top or viewport resets.
/// Returns a list of human-readable labels for each detected sequence.
/// These are logged as warnings for diagnostic purposes — data is never modified.
fn detect_anomalous_sequences(data: &str) -> Vec<&'static str> {
    let bytes = data.as_bytes();
    let len = bytes.len();
    let mut found = Vec::new();
    let mut i = 0;

    while i < len {
        if bytes[i] == 0x1b && i + 1 < len && bytes[i + 1] == b'[' {
            i += 2; // skip ESC[

            // Check for ESC[? private mode sequences (alt screen)
            if i < len && bytes[i] == b'?' {
                i += 1;
                let num_start = i;
                while i < len && bytes[i].is_ascii_digit() {
                    i += 1;
                }
                if i < len {
                    let num_str = std::str::from_utf8(&bytes[num_start..i]).unwrap_or("");
                    match (num_str, bytes[i]) {
                        ("1049", b'h') => found.push("ESC[?1049h (Alt Screen Enter)"),
                        ("1049", b'l') => found.push("ESC[?1049l (Alt Screen Exit)"),
                        _ => {}
                    }
                    i += 1;
                }
                // No continue — let the outer while loop re-evaluate i < len
            } else {
                // Parse numeric params: n or n;m
                let num_start = i;
                while i < len && (bytes[i].is_ascii_digit() || bytes[i] == b';') {
                    i += 1;
                }
                if i < len {
                    let params = std::str::from_utf8(&bytes[num_start..i]).unwrap_or("");
                    match bytes[i] {
                        b'J' => match params {
                            "2" => found.push("ESC[2J (Clear Screen)"),
                            "3" => found.push("ESC[3J (Clear Scrollback)"),
                            _ => {}
                        },
                        b'H' => {
                            // ESC[H or ESC[1;1H = Cursor Home
                            if params.is_empty() {
                                found.push("ESC[H (Cursor Home)");
                            } else if params == "1;1" {
                                found.push("ESC[1;1H (Cursor Home)");
                            }
                            // Other ESC[n;mH = regular cursor position, not anomalous
                        }
                        _ => {}
                    }
                    i += 1;
                }
            }
        } else {
            i += 1;
        }
    }

    found
}

/// Extract the largest ESC[nA (cursor-up) value from `data`.
/// Ink emits ESC[nA where n equals the previous render height before redrawing.
/// A decrease in n between consecutive redraws signals content shrinkage.
fn extract_largest_cursor_up(data: &str) -> Option<u16> {
    let bytes = data.as_bytes();
    let len = bytes.len();
    let mut i = 0;
    let mut max_n: Option<u16> = None;

    while i < len {
        if bytes[i] == 0x1b && i + 1 < len && bytes[i + 1] == b'[' {
            i += 2;
            let num_start = i;
            while i < len && bytes[i].is_ascii_digit() {
                i += 1;
            }
            if i < len
                && bytes[i] == b'A'
                && i > num_start
                && let Ok(n) = std::str::from_utf8(&bytes[num_start..i])
                    .unwrap_or("")
                    .parse::<u16>()
            {
                max_n = Some(max_n.map_or(n, |prev: u16| prev.max(n)));
            }
            if i < len {
                i += 1;
            }
        } else {
            i += 1;
        }
    }
    max_n
}

/// Inject ESC[2J (clear screen) before the first ESC[H or ESC[1;1H (cursor home) in `data`.
///
/// Ink-based TUIs render differentially: they position the cursor at home and overwrite
/// changed cells but never send ESC[K (erase to end of line). When output shrinks between
/// redraws, old characters — especially box-drawing separators — persist as ghost artifacts.
///
/// Injecting a single ESC[2J before the cursor-home ensures the screen is blank before
/// the redraw starts. Because xterm.js processes the entire write() atomically (clear +
/// cursor home + new content happen before the next paint), no intermediate blank frame
/// is ever rendered to the user.
///
/// Only injects once per call (before the first cursor-home) to avoid unnecessary clears
/// for chunks that contain multiple ESC[H sequences (common in Ink's rapid redraws).
fn inject_clear_before_cursor_home(data: &str) -> String {
    let bytes = data.as_bytes();
    let len = bytes.len();
    let mut i = 0;

    while i < len {
        if bytes[i] == 0x1b && i + 1 < len && bytes[i + 1] == b'[' {
            let seq_start = i;
            i += 2; // skip ESC[
            // Parse optional numeric parameters
            let num_start = i;
            while i < len && (bytes[i].is_ascii_digit() || bytes[i] == b';') {
                i += 1;
            }
            if i < len && bytes[i] == b'H' {
                let params = std::str::from_utf8(&bytes[num_start..i]).unwrap_or("");
                // ESC[H (no params) or ESC[1;1H — both mean cursor home
                if params.is_empty() || params == "1;1" {
                    // Inject ESC[2J before this cursor-home sequence
                    let mut result = String::with_capacity(len + 4);
                    result.push_str(&data[..seq_start]);
                    result.push_str("\x1b[2J");
                    result.push_str(&data[seq_start..]);
                    return result;
                }
            }
            if i < len {
                i += 1; // skip command byte
            }
        } else {
            i += 1;
        }
    }

    // No cursor-home found — return as-is
    data.to_string()
}

/// Inject ESC[2J before the first ESC[nA (cursor-up, n > 0) in `data`.
///
/// Fallback for `inject_clear_before_cursor_home`: Ink re-renders reposition via
/// cursor-up (ESC[nA), not cursor-home (ESC[H). Without this path the
/// `alt_buffer_needs_clear` flag is set but never consumed, and ghost rows
/// from previous renders accumulate from the bottom upward.
fn inject_clear_before_cursor_up(data: &str) -> String {
    let bytes = data.as_bytes();
    let len = bytes.len();
    let mut i = 0;

    while i < len {
        if bytes[i] == 0x1b && i + 1 < len && bytes[i + 1] == b'[' {
            let seq_start = i;
            i += 2; // skip ESC[
            let num_start = i;
            while i < len && bytes[i].is_ascii_digit() {
                i += 1;
            }
            if i < len && bytes[i] == b'A' && i > num_start {
                // ESC[nA with n > 0 — inject ESC[2J before it
                let mut result = String::with_capacity(len + 4);
                result.push_str(&data[..seq_start]);
                result.push_str("\x1b[2J");
                result.push_str(&data[seq_start..]);
                return result;
            }
            if i < len {
                i += 1; // skip command byte
            }
        } else {
            i += 1;
        }
    }

    data.to_string()
}

/// Spawn a reader thread that reads from a PTY, processes output, and emits events.
/// Unified for both desktop (Tauri IPC) and headless (event_bus only) modes.
/// 1-minute system load average divided by the online CPU count — a measure of
/// machine-wide CPU oversubscription (NOT this process's own usage, which the
/// cpu_watchdog covers via getrusage). >= 1.0 means the run queue is as long as
/// there are cores: things are queueing and the WebView main thread gets starved.
/// Used to gate the typing frame-throttle so it only kicks in under real load.
/// Returns 0.0 where unavailable (Windows) — throttle stays off, behaviour unchanged.
#[cfg(unix)]
fn system_load_per_core() -> f64 {
    let mut avg = [0f64; 3];
    let n = unsafe { libc::getloadavg(avg.as_mut_ptr(), 3) };
    if n < 1 {
        return 0.0;
    }
    let ncpu = unsafe { libc::sysconf(libc::_SC_NPROCESSORS_ONLN) };
    let ncpu = if ncpu < 1 { 1.0 } else { ncpu as f64 };
    avg[0] / ncpu
}

#[cfg(not(unix))]
fn system_load_per_core() -> f64 {
    0.0
}

/// Minimum interval (ms) the grid ticker must wait between frame sends.
/// `0` = no floor (send at the full 16 ms tick / ~60 fps), for short bursts so
/// latency stays low. The two floors give the WebView main thread breathing room:
///  - `input_recent` (user typing under CPU saturation) → ~20 fps, the most
///    aggressive floor, so keystroke dispatch + echo aren't stuck behind output.
///  - sustained animation (grid dirty ≥ 6 consecutive ticks, e.g. a spinner TUI)
///    → ~30 fps.
///
/// Typing wins over sustained because it's the latency-critical case.
fn grid_send_min_interval_ms(input_recent: bool, dirty_run: u32) -> u64 {
    const SUSTAINED_DIRTY_TICKS: u32 = 6;
    const SUSTAINED_MIN_INTERVAL_MS: u64 = 33; // ~30 fps while animating
    const INPUT_MIN_INTERVAL_MS: u64 = 50; // ~20 fps while typing under load
    if input_recent {
        INPUT_MIN_INTERVAL_MS
    } else if dirty_run >= SUSTAINED_DIRTY_TICKS {
        SUSTAINED_MIN_INTERVAL_MS
    } else {
        0
    }
}

/// Stamp the per-session last-input timestamp (epoch ms). Read by the grid
/// ticker to throttle frame sends while the user types under CPU saturation,
/// keeping the WebView/browser main thread free for keystroke dispatch + echo.
/// Called from every interactive input entry point (desktop `write_pty` +
/// HTTP/PWA `write_to_session`).
pub(crate) fn stamp_input_ms(state: &AppState, session_id: &str) {
    let now_ms = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64;
    state
        .session_maps
        .last_input_ms
        .entry(session_id.to_string())
        .or_insert_with(|| std::sync::atomic::AtomicU64::new(0))
        .store(now_ms, std::sync::atomic::Ordering::Relaxed);
}

/// Apply the semantic effects of a submitted terminal line for every transport.
/// Empty content is still a submission: bare Enter resolves highlighted choices
/// and confirmation prompts, so it must clear an active wait and advance the
/// turn just like a non-empty reply.
pub(crate) fn record_submitted_line(
    state: &Arc<AppState>,
    session_id: &str,
    content: String,
    line: i64,
) {
    note_submitted_input(state, session_id);
    if content.split_whitespace().count() >= 10 {
        state
            .session_maps
            .last_prompts
            .insert(session_id.to_string(), content.clone());
    }
    let parsed = ParsedEvent::UserInput { content, line };
    if let Ok(json) = serde_json::to_value(&parsed).map(std::sync::Arc::new) {
        state.emit_pty_event(crate::state::AppEvent::PtyParsed {
            session_id: session_id.to_string(),
            parsed: std::sync::Arc::clone(&json),
        });
        #[cfg(feature = "desktop")]
        if let Some(app) = state.app_handle.read().as_ref() {
            let _ = app.emit(&format!("pty-parsed-{session_id}"), &*json);
        }
    }
    if let Some(ss) = state.session_maps.silence_states.get(session_id) {
        let mut sl = ss.lock();
        sl.suppress_user_input();
        // The parser's api-error / session-conflict dedup lives in the reader
        // thread and cannot observe this event; park the reset for it.
        sl.request_parser_dedup_reset();
    }
}

pub(crate) fn spawn_reader_thread(
    mut reader: Box<dyn Read + Send>,
    paused: Arc<AtomicBool>,
    session_id: String,
    state: Arc<AppState>,
    tuic_session: Option<String>,
) {
    let silence = Arc::new(Mutex::new(SilenceState::new()));
    let running = Arc::new(AtomicBool::new(true));

    state
        .session_maps
        .silence_states
        .insert(session_id.clone(), silence.clone());
    state.session_maps.shell_states.insert(
        session_id.clone(),
        std::sync::atomic::AtomicU8::new(SHELL_NULL),
    );

    spawn_silence_timer(
        silence.clone(),
        running.clone(),
        session_id.clone(),
        state.clone(),
    );

    // Frame ticker: decouples PTY read() from frame serialization.
    // Reader sets dirty flag; ticker serializes+sends at fixed interval.
    // Coalesces rapid writes (spinner erase+rewrite) into a single frame.
    let frame_dirty = Arc::new(AtomicBool::new(false));
    state
        .grid
        .frame_dirty
        .insert(session_id.clone(), frame_dirty.clone());
    // Scroll target the ticker consumes, created alongside the dirty flag the
    // same handler sets. It belongs to the session, not to one of its front
    // ends: when the desktop `subscribe_terminal_grid` owned it, a session only
    // a browser had ever rendered had nowhere to record a scroll, and closing
    // the desktop terminal took the entry away from an attached browser.
    state
        .grid
        .pending_scroll
        .insert(session_id.clone(), Arc::new(AtomicI64::new(-1)));
    let sync_active = Arc::new(AtomicBool::new(false));
    state
        .grid
        .sync_update_active
        .insert(session_id.clone(), sync_active.clone());
    // Shared by the PTY reader (which batches lines) and the frame ticker (which
    // drains a tail the reader cannot: read() blocks, so the last line of a burst
    // would otherwise wait for output that may never come).
    let watcher_batcher = Arc::new(parking_lot::Mutex::new(
        crate::output_watchers::WatcherLineBatcher::new(WATCHER_LINE_WINDOW, WATCHER_BATCH_CAP),
    ));
    let ticker_batcher = watcher_batcher.clone();
    let ticker_running = running.clone();
    let ticker_dirty = frame_dirty.clone();
    let ticker_sync_active = Some(sync_active);
    let ticker_state = state.clone();
    let ticker_sid = session_id.clone();
    std::thread::spawn(move || {
        // Frame serialize+emit is the Rust side of the echo→render path; keep it
        // in the high QoS band so output stays live under a saturating build.
        raise_thread_for_interactive_io();
        const TICK: std::time::Duration = std::time::Duration::from_millis(16);
        // Safety net: if in_flight stays true for this long (~500 ms),
        // force-reset it so frame delivery resumes. Prevents permanent blank
        // terminal when the frontend fails to ack (crash, corrupt frame, etc.).
        //
        // This is also the deadline a HIDDEN tab races on purpose: it acks on a
        // trailing timer (HIDDEN_ACK_INTERVAL_MS in canvasTerminalUtils.ts), so
        // the two constants are coupled and neither may be re-tuned alone. The
        // difference between them is the frontend's drift budget — at 400 vs 500
        // it was 100 ms and lost constantly, logging the warning below for tabs
        // that were merely in the background.
        const MAX_IN_FLIGHT_MS: u64 = 500;
        // After this many consecutive force-resets, back off for STUCK_PAUSE_MS
        // to let the JS event loop drain the Tauri channel backlog before
        // sending more frames. Kept short (1s, chunked) so a transient JS stall
        // — e.g. a repo-changed git/IPC burst that blocks the WebView thread for
        // ~1-2s — doesn't freeze an otherwise-healthy terminal for the full
        // pause. The loop re-applies the back-off if the frontend is still
        // behind, so persistent saturation still gets cumulative backpressure.
        const MAX_STUCK_BEFORE_PAUSE: u32 = 3;
        const STUCK_PAUSE_MS: u64 = 1_000;
        // Send-rate floors live in grid_send_min_interval_ms() (unit-tested).
        // How long after a keystroke the typing-throttle stays armed.
        const INPUT_THROTTLE_WINDOW_MS: u64 = 150;
        const LOAD_SATURATION_RATIO: f64 = 1.0; // 1-min load >= cores
        const LOAD_SAMPLE_INTERVAL: std::time::Duration = std::time::Duration::from_secs(1);
        let mut stuck_since: Option<std::time::Instant> = None;
        let mut stuck_count: u32 = 0;
        let mut dirty_run: u32 = 0;
        let mut last_sent: Option<std::time::Instant> = None;
        let mut last_load_check: Option<std::time::Instant> = None;
        let mut system_saturated = false;
        while ticker_running.load(Ordering::Relaxed) {
            std::thread::sleep(TICK);
            // Drain a watcher-line tail the reader left batched. read() blocks, so
            // without this the last line of a burst waits for output that may
            // never arrive and a rare-line watcher matches minutes late. It must
            // run BEFORE the dirty guard below: the tick that clears frame_dirty
            // is usually the one *before* the batching window expires, and every
            // later idle tick would return early and never look at the batch.
            {
                let mut batch = ticker_batcher.lock();
                // Emit under the lock so this tail cannot be interleaved behind a
                // newer batch the reader emits concurrently.
                if let Some(due) = batch.flush_due(std::time::Instant::now()) {
                    emit_watcher_lines(&ticker_state, &ticker_sid, due);
                }
                drop(batch);
            }
            let mut effective_dirty = ticker_dirty.swap(false, Ordering::Relaxed);
            // DEC 2026: the vendored VTE records a 150ms deadline but never fires
            // it, so a synchronized update left open (delayed/lost ESU, or a stream
            // that simply stops mid-update) would buffer forever and wedge the
            // terminal. This is the only wakeup that can end it — no PTY bytes are
            // coming — so it must run BEFORE the non-dirty early return. The
            // atomic hint keeps idle sessions from touching the vt lock at all.
            let mut sync_timeout_flush = false;
            if ticker_sync_active
                .as_ref()
                .is_some_and(|f| f.load(Ordering::Relaxed))
                && let Some(vt) = ticker_state.grid.vt_log_buffers.get(&ticker_sid)
            {
                let mut g = vt.lock();
                if g.flush_sync_timeout_if_needed() {
                    sync_timeout_flush = true;
                    effective_dirty = true;
                }
                let still_active = g.is_sync_update_active();
                drop(g);
                if let Some(f) = ticker_sync_active.as_ref() {
                    f.store(still_active, Ordering::Relaxed);
                }
            }
            if !effective_dirty {
                // Idle tick: leave sustained-animation mode so the next burst
                // (keystroke, fresh output) gets full 60 fps low-latency response.
                dirty_run = 0;
                continue;
            }
            // F28: nobody is looking. Everything below — the vt lock and a full
            // serialize_dirty_rows — would produce bytes that send_grid_frame
            // drops on the floor, which is what a PTY still running behind a
            // closed tab used to pay on every dirty tick.
            //
            // Nothing is lost by skipping. The damage stays on the vt because
            // serialize_dirty_rows is what would have cleared it, and both
            // subscribe paths repaint from scratch anyway: terminal_request_frame
            // forces full damage before serializing, and the WS path
            // (mcp_http/session.rs full_frame_for_single_client) forces it twice
            // and re-arms this ticker.
            //
            // A pending scroll is the one thing that must NOT be skipped with the
            // frame: it is session state, not pixels. `/terminal/scroll-info`,
            // the row reads and the next full frame all answer from the grid's
            // display offset, so a target dropped here would silently disagree
            // with every later read — and an HTTP client that scrolls without
            // holding a grid WebSocket is exactly the browser/PWA case. It costs
            // the vt lock only when a client actually asked for a scroll.
            //
            // DEFERRED (2026-08-20) — parking the THREAD itself, which is what
            // F28 asked for. What is left after the skip above is timer churn, not
            // work: ~62.5 wakeups/s per session, each an atomic load and two
            // checks, and macOS coalesces them. Parking needs a condvar the PTY
            // writer signals, plus a `wait_timeout` for the DEC 2026 sync flush
            // above, which must keep running headlessly — and a missed notify
            // shows up as a terminal that silently stops painting. Small win,
            // worst failure mode of the group.
            if !grid_has_subscriber(&ticker_state, &ticker_sid) {
                if let Some(target) = take_pending_scroll(&ticker_state, &ticker_sid)
                    && let Some(vt) = ticker_state.grid.vt_log_buffers.get(&ticker_sid)
                {
                    vt.lock().grid_scroll_to_offset(target);
                }
                dirty_run = 0;
                continue;
            }
            dirty_run = dirty_run.saturating_add(1);
            // Clone the Arc out: the guard below would otherwise hold a DashMap
            // shard read lock across the stuck back-off sleep, blocking every
            // writer on that shard for up to a second.
            let gate = ticker_state
                .grid
                .gates
                .get(&ticker_sid)
                .map(|g| Arc::clone(g.value()));
            // The gate belongs to the desktop WebView and to nothing else. It used
            // to stop this tick outright, which is correct only while the desktop
            // is the sole consumer: a browser/PWA client rides a different
            // transport with its own flow control, and a stalled WebView is not
            // its problem. So the stall accounting below still runs — the gate is
            // still what decides whether the desktop CHANNEL gets this frame, in
            // `send_grid_frame` — but it may only stop the tick when there is
            // nobody else to serve.
            let watchers = grid_has_watcher(&ticker_state, &ticker_sid);
            if gate.as_ref().is_some_and(|g| !g.is_open()) {
                let now = std::time::Instant::now();
                let since = stuck_since.get_or_insert(now);
                let elapsed = now.duration_since(*since).as_millis() as u64;
                if elapsed > MAX_IN_FLIGHT_MS {
                    stuck_count += 1;
                    tracing::warn!(
                        session_id = %ticker_sid,
                        elapsed_ms = elapsed,
                        stuck_count,
                        watchers,
                        outstanding = gate.as_ref().map_or(0, |g| g.outstanding()),
                        "grid frame gate stuck, abandoning the outstanding frame"
                    );
                    if let Some(g) = gate.as_ref() {
                        g.abandon();
                    }
                    stuck_since = None;
                    if stuck_count >= MAX_STUCK_BEFORE_PAUSE {
                        stuck_count = 0;
                        // Back off to let JS drain the channel backlog before retrying.
                        // Sleep in short chunks so (a) a recovered frontend resumes
                        // within ~one chunk rather than the full pause, and (b) session
                        // close isn't delayed up to the full pause on shutdown.
                        //
                        // Skipped entirely when a browser is watching: this pause is
                        // the desktop's recovery time, and spending it on the thread
                        // that is the only frame source for the other transport
                        // freezes a client that never fell behind.
                        let mut waited = 0u64;
                        while !watchers
                            && waited < STUCK_PAUSE_MS
                            && ticker_running.load(Ordering::Relaxed)
                        {
                            std::thread::sleep(std::time::Duration::from_millis(100));
                            waited += 100;
                        }
                    }
                }
                if !watchers {
                    ticker_dirty.store(true, Ordering::Relaxed);
                    continue;
                }
            } else {
                stuck_since = None;
                stuck_count = 0;
            }
            // Adaptive frame-rate floor: a TUI that animates continuously (e.g.
            // grok's spinner repaints its whole bordered UI and walks the cursor
            // around every tick) keeps the grid dirty 100% of the time, so the
            // ticker would emit ~50 multi-KB frames/s. The in_flight gate prevents
            // queue overflow but NOT WebView main-thread starvation — it paints
            // flat-out and never yields to input/console, so the UI looks frozen.
            // Once dirtiness is sustained, cap the send rate to ~30 fps to give the
            // JS thread breathing room. Short bursts stay at the full 60 fps tick.
            let now = std::time::Instant::now();
            // Refresh the machine-saturation gate ~once/sec (cheap getloadavg).
            if last_load_check.is_none_or(|t| now.duration_since(t) >= LOAD_SAMPLE_INTERVAL) {
                system_saturated = system_load_per_core() >= LOAD_SATURATION_RATIO;
                last_load_check = Some(now);
            }
            // Typing-under-load throttle: only when saturated AND the user typed
            // recently. now_epoch_ms() is computed only on the saturated path.
            let input_recent = system_saturated && {
                let now_ms = std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .unwrap_or_default()
                    .as_millis() as u64;
                ticker_state
                    .session_maps
                    .last_input_ms
                    .get(&ticker_sid)
                    .map(|ts| {
                        now_ms.saturating_sub(ts.load(Ordering::Relaxed)) < INPUT_THROTTLE_WINDOW_MS
                    })
                    .unwrap_or(false)
            };
            // Pick the send-rate floor: typing-under-load (~20 fps) wins, else the
            // sustained-animation floor (~30 fps), else full 60 fps for short bursts.
            // A sync-timeout flush is a protocol deadline, not animation — the
            // frame-rate floor must not defer it into the next tick.
            let min_interval = if sync_timeout_flush {
                0
            } else {
                grid_send_min_interval_ms(input_recent, dirty_run)
            };
            if min_interval > 0
                && let Some(last) = last_sent
                && (now.duration_since(last).as_millis() as u64) < min_interval
            {
                ticker_dirty.store(true, Ordering::Relaxed); // keep pending for a later tick
                continue;
            }
            // The WebView missed frames while its gate was closed and has caught
            // up: those rows went to the WebSocket subscribers and left the shared
            // damage, so a delta now would land on a row map with holes in it.
            // Pay the debt with a full frame private to this channel — it consumes
            // no damage, so the delta below still reaches everyone else.
            let desktop_owed_full_frame = gate
                .as_ref()
                .is_some_and(|g| g.is_open() && g.take_missed());
            if let Some(vt) = ticker_state.grid.vt_log_buffers.get(&ticker_sid) {
                let mut g = vt.lock();
                if let Some(target) = take_pending_scroll(&ticker_state, &ticker_sid) {
                    g.grid_scroll_to_offset(target);
                }
                // Cut before the delta, from the same locked state, so the rows the
                // delta carries are already in it.
                let repair = desktop_owed_full_frame.then(|| g.serialize_full_frame());
                let frame = g.serialize_dirty_rows();
                drop(g);
                #[cfg(feature = "desktop")]
                if let Some(repair) = repair {
                    send_desktop_grid_frame(&ticker_state, &ticker_sid, repair);
                }
                #[cfg(not(feature = "desktop"))]
                let _ = repair;
                send_grid_frame(&ticker_state, &ticker_sid, frame);
                last_sent = Some(now);
            }
        }
        // Final flush after reader exits. Session teardown is the other "no more
        // PTY bytes arrive" case: drain any still-buffered synchronized update
        // BEFORE serializing, or its content is dropped with the session.
        // No watcher-line drain here on purpose: teardown has exactly one owner,
        // the reader's EOF path, which assembles the flush_eof remainder and
        // drains whatever is still batched, in order. A second drain racing from
        // this thread could deliver the older tail after it.
        if let Some(vt) = ticker_state.grid.vt_log_buffers.get(&ticker_sid) {
            let mut g = vt.lock();
            g.force_stop_sync_if_buffered();
            let frame = g.serialize_dirty_rows();
            drop(g);
            send_grid_frame(&ticker_state, &ticker_sid, frame);
        }
        ticker_state.grid.frame_dirty.remove(&ticker_sid);
        ticker_state.grid.sync_update_active.remove(&ticker_sid);
    });

    std::thread::spawn(move || {
        // PTY reader drives byte intake → echo; keep it above default-QoS builds.
        raise_thread_for_interactive_io();
        let sid_for_panic = session_id.clone();
        let state_for_panic = state.clone();
        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            let mut buf = [0u8; 65536];
            let mut utf8_buf = Utf8ReadBuffer::new();
            let mut esc_buf = EscapeAwareBuffer::new();
            let session_cwd: Option<String> = state
                .session_maps
                .sessions
                .get(&session_id)
                .and_then(|s| s.lock().cwd.clone());
            let mut processor = ChunkProcessor::new(session_cwd, tuic_session);
            // Line reassembly for plugin watcher matching. Separate from the VT
            // parser: watchers match the byte stream as it scrolls past, not the
            // screen contents.
            let mut watcher_lines = crate::output_watchers::StreamLines::new();
            let mut activity_pulse = ActivityPulse::new();
            loop {
                while paused.load(Ordering::Relaxed) {
                    std::thread::sleep(std::time::Duration::from_millis(10));
                }
                match reader.read(&mut buf) {
                    Ok(0) => break,
                    Ok(n) => {
                        state.metrics.bytes_emitted.fetch_add(n, Ordering::Relaxed);
                        // Flight recorder: keep the last PTY_RAW_RING_CAP raw bytes
                        // (pre-transform) so a wild rendering corruption can be
                        // dumped and replayed offline (story 056-7545).
                        {
                            let ring = state
                                .grid
                                .pty_raw_rings
                                .entry(session_id.clone())
                                .or_default();
                            let mut ring = ring.lock();
                            ring.extend(&buf[..n]);
                            if ring.len() > PTY_RAW_RING_CAP {
                                let excess = ring.len() - PTY_RAW_RING_CAP;
                                ring.drain(..excess);
                            }
                        }
                        let utf8_data = utf8_buf.push(&buf[..n]);
                        let esc_data = esc_buf.push(&utf8_data);
                        let (kitty_clean, kitty_actions) = strip_kitty_sequences(&esc_data);
                        // Diagnostic-only substring scan over the whole chunk. It ran
                        // on every PTY read to catch a DECRST leak that has not been
                        // seen since; behind the Diagnostics toggle it costs one
                        // relaxed atomic load instead of two passes over 64 KB.
                        // Enable with POST /diagnostics {"enabled":true} to get it back.
                        if crate::cpu_watchdog::diagnostic_mode()
                            && kitty_clean.contains("1049l")
                            && !kitty_clean.contains("\x1b[?1049l")
                        {
                            tracing::error!(source = "terminal", session_id = %session_id,
                            "DECRST leak: kitty_clean has bare '1049l' without ESC[? prefix. \
                             esc_data({} bytes)={:?}, kitty_clean({} bytes)={:?}, actions={:?}",
                            esc_data.len(), esc_data.as_bytes().iter().take(200).collect::<Vec<_>>(),
                            kitty_clean.len(), kitty_clean.as_bytes().iter().take(200).collect::<Vec<_>>(),
                            kitty_actions);
                        }
                        let data = kitty_clean;

                        process_kitty_actions(&kitty_actions, &session_id, &state);

                        if processor.process_chunk(&data, &silence, &session_id, &state)
                            && let Some(xterm_data) = processor.transform_xterm(&data)
                        {
                            let clamped_data = xterm_data;

                            let agent_active = state
                                .session_maps
                                .session_states
                                .get(&session_id)
                                .map(|s| s.agent_type.is_some())
                                .unwrap_or(false);
                            // Also diagnostic-only: the ESC scan plus
                            // `detect_anomalous_sequences` ran on every shell-session
                            // chunk purely to emit a warning nothing acts on.
                            if crate::cpu_watchdog::diagnostic_mode()
                                && !processor.in_alt_buffer
                                && !agent_active
                                && clamped_data.as_bytes().contains(&0x1b)
                            {
                                let anomalies = detect_anomalous_sequences(&clamped_data);
                                for label in &anomalies {
                                    tracing::warn!(source = "terminal", session_id = %session_id, "Anomalous ANSI sequence: {label}");
                                }
                            }

                            // Plugin OutputWatcher matching. The canvas renders
                            // from grid frames and never read this text; the only
                            // consumer was pluginRegistry, which reassembled lines
                            // in the WebView. It happens here now, on the reader
                            // thread, and only the lines that matter cross the
                            // boundary — throttled but LOSSLESS.
                            //
                            // Losslessness is the point: the original throttle
                            // DROPPED chunks inside its window, which spliced the
                            // tail of one chunk onto the head of a later one and
                            // reported a line that never existed (audit F1).
                            assemble_watcher_lines(
                                &state,
                                &session_id,
                                &clamped_data,
                                &mut watcher_lines,
                                &watcher_batcher,
                                false,
                            );

                            // "Output happened" for the activity dot and the
                            // last-seen timestamp. Sits here, in the same block the
                            // deleted `pty-output` emit occupied, so it reports on
                            // exactly the chunks that one did — a chunk the
                            // processor swallowed whole was never activity.
                            activity_pulse.pulse(&state, &session_id);
                        }

                        if std::mem::take(&mut processor.queued_idle_flush)
                            && state
                                .pending_injections
                                .get(&session_id)
                                .is_some_and(|queue| !queue.is_empty())
                        {
                            flush_pending_injections(&state, &session_id);
                        }

                        frame_dirty.store(true, Ordering::Relaxed);
                    }
                    Err(e) => {
                        tracing::error!(session_id = %session_id, "PTY reader error: {e}");
                        break;
                    }
                }
            }
            running.store(false, Ordering::Relaxed);

            if try_shell_transition(&state, &session_id, SHELL_BUSY, SHELL_IDLE, false) {
                emit_shell_state(&state, &session_id, "idle");
                // EOF bypass: should_transition_idle was not called, so force-clear
                // active_sub_tasks to avoid leaving the frontend notification gate
                // in an inconsistent state after process crash/exit.
                let needs_clear = state
                    .session_maps
                    .session_states
                    .get_mut(&session_id)
                    .filter(|e| e.active_sub_tasks > 0)
                    .map(|mut e| {
                        e.active_sub_tasks = 0;
                    })
                    .is_some();
                if needs_clear {
                    emit_active_subtasks(&state, &session_id, 0, "");
                }
            }

            let remaining = flush_eof(&mut utf8_buf, &mut esc_buf, &session_id, &state);
            // The remainder goes through the assembler, not straight out: it may
            // close a line the last read left partial. Then drain unconditionally
            // — teardown has no later tick to flush the batch.
            assemble_watcher_lines(
                &state,
                &session_id,
                &remaining,
                &mut watcher_lines,
                &watcher_batcher,
                true,
            );
            {
                // Hold the lock across the emit: the ticker may still be running
                // its own flush, and releasing between take and emit would let it
                // deliver an earlier tail after this final batch.
                let mut batch = watcher_batcher.lock();
                if let Some(due) = batch.take() {
                    emit_watcher_lines(&state, &session_id, due);
                }
                drop(batch);
            }

            state.emit_pty_event(crate::state::AppEvent::PtyExit {
                session_id: session_id.clone(),
            });
            #[cfg(feature = "desktop")]
            if let Some(app) = state.app_handle.read().as_ref() {
                let _ = app.emit(
                    &format!("pty-exit-{session_id}"),
                    serde_json::json!({ "session_id": session_id }),
                );
            }
            tracing::info!(source = "pty", session_id = %session_id, "Session closed: process exited");
            state.emit_pty_event(crate::state::AppEvent::SessionClosed {
                session_id: session_id.clone(),
                reason: "process_exit".to_string(),
            });
            #[cfg(feature = "desktop")]
            if let Some(app) = state.app_handle.read().as_ref() {
                let agent_type = state
                    .session_maps
                    .session_states
                    .get(&session_id)
                    .and_then(|s| s.agent_type.clone());
                let _ = app.emit(
                    "session-closed",
                    serde_json::json!({
                        "session_id": session_id,
                        "reason": "process_exit",
                        "agent_type": agent_type,
                    }),
                );
            }

            mark_session_exited(&session_id, &state);
        })); // end catch_unwind
        if let Err(panic_info) = result {
            let msg = if let Some(s) = panic_info.downcast_ref::<&str>() {
                s.to_string()
            } else if let Some(s) = panic_info.downcast_ref::<String>() {
                s.clone()
            } else {
                "unknown panic payload".to_string()
            };
            tracing::error!(session_id = %sid_for_panic, "READER THREAD PANICKED: {msg}");
            // The store that ends the frame ticker and the 1 Hz silence timer
            // lives inside the closure above, so a panic skips it: without this
            // the ticker keeps waking ~62 times a second and the tokio timer
            // keeps ticking for the life of the process, and the ticker never
            // reaches the code after its loop that removes this session's
            // grid_frame_dirty / sync_update_active entries.
            running.store(false, Ordering::Relaxed);
            mark_session_exited(&sid_for_panic, &state_for_panic);
        }
    });
}

#[derive(Debug, Clone, Serialize)]
pub struct VtLogChunk {
    pub lines: Vec<crate::state::LogLine>,
    pub screen: Vec<crate::state::LogLine>,
    pub total_lines: usize,
    pub oldest: usize,
}

/// Consume the coalesced scroll target a client left for this session.
///
/// Both transports write it (`terminal_scroll_to_offset` as a Tauri command and
/// as an HTTP route) without taking the vt lock, and the frame ticker is the only
/// reader — so the swap here is what makes "latest wins" true: whatever arrived
/// since the last tick is applied once, and `-1` means nothing is pending.
fn take_pending_scroll(state: &AppState, session_id: &str) -> Option<usize> {
    let target = state
        .grid
        .pending_scroll
        .get(session_id)?
        .swap(-1, Ordering::Relaxed);
    (target >= 0).then_some(target as usize)
}

/// Is anyone waiting for this session's grid frames?
///
/// The two consumers `send_grid_frame` knows about: the desktop IPC channel, and
/// the watch that feeds browser/PWA clients. A watch *entry* is not a consumer —
/// the sender outlives its receivers, so the count is what decides.
///
/// Used by the frame ticker to skip the encode entirely rather than serialize a
/// frame `send_grid_frame` would drop. It must stay in step with that function:
/// a consumer this misses is a client that stops repainting.
pub(crate) fn grid_has_subscriber(state: &AppState, session_id: &str) -> bool {
    #[cfg(feature = "desktop")]
    if state.grid.channels.contains_key(session_id) {
        return true;
    }
    grid_has_watcher(state, session_id)
}

/// Does this session have a browser/PWA client on the grid WebSocket?
///
/// Narrower than [`grid_has_subscriber`] on purpose: the frame ticker uses this
/// one to decide whether a stalled *desktop* WebView may stop the frames, and a
/// desktop channel is exactly what must not count towards that answer.
fn grid_has_watcher(state: &AppState, session_id: &str) -> bool {
    state
        .grid
        .watch
        .get(session_id)
        .is_some_and(|tx| tx.receiver_count() > 0)
}

/// Repair after a frame lost the ordering race.
///
/// The frame that was dropped carries rows the delivered one does not: the
/// damage behind them was consumed when it was serialized, so nothing will ever
/// send them again. Dropping it silently leaves those rows stale on every client
/// until something else happens to repaint them. Damage the grid again and wake
/// the ticker instead — the next frame is a full one and every transport is
/// whole. Unlike a per-subscriber resync (`serialize_full_frame`), re-damaging is
/// the *right* answer here: the loss is shared, so the repair has to be.
fn repaint_after_reorder(state: &AppState, session_id: &str) {
    tracing::debug!(session_id = %session_id, "grid frame arrived out of order, forcing a full repaint");
    if let Some(vt) = state.grid.vt_log_buffers.get(session_id) {
        vt.lock().grid_force_full_damage();
    }
    if let Some(dirty) = state.grid.frame_dirty.get(session_id) {
        dirty.store(true, Ordering::Relaxed);
    }
}

/// Send a grid frame through the session's channel and close the delivery gate.
/// Also publishes to the watch channel for WebSocket subscribers.
///
/// Frames are serialized under the vt lock and arrive here after it was
/// released, so two producers can reach this point in the opposite order. The
/// `order` the frame carries was stamped inside that critical section and is the
/// only record of which one is newer; a frame that lost the race is dropped here
/// and repaired with a full repaint rather than painted over a newer screen.
pub(crate) fn send_grid_frame(
    state: &AppState,
    session_id: &str,
    frame: crate::grid_gate::GridFrame,
) {
    if frame.is_empty() {
        return;
    }
    let crate::grid_gate::GridFrame { order, bytes } = frame;
    // Clone only when both consumers want the frame. A browser-only session has
    // no desktop channel, so the watch can take the original; cloning first and
    // then finding nothing to hand the original to was pure copy.
    #[cfg(feature = "desktop")]
    let desktop_wants_it = state.grid.channels.contains_key(session_id);
    #[cfg(not(feature = "desktop"))]
    let desktop_wants_it = false;

    let frame = match state.grid.watch.get(session_id) {
        Some(watch_tx) => {
            let watched = watch_tx.receiver_count() > 0;
            let (for_watch, for_desktop) = match (watched, desktop_wants_it) {
                (true, true) => (Some(bytes.clone()), Some(bytes)),
                (true, false) => (Some(bytes), None),
                (false, _) => (None, Some(bytes)),
            };
            // The claim and the publish share one critical section: claiming
            // first and publishing after would let two producers claim in the
            // order they serialized and then publish in the other one.
            if crate::grid_watch::claim_grid_frame(&watch_tx, order, for_watch)
                == crate::grid_gate::FrameOrder::Stale
            {
                repaint_after_reorder(state, session_id);
                return;
            }
            match for_desktop {
                Some(bytes) => bytes,
                None => return,
            }
        }
        None => bytes,
    };

    #[cfg(feature = "desktop")]
    {
        // A closed gate means the WebView has not painted the frame before this
        // one. The ticker used to stop entirely here, which held the damage on
        // the vt but starved every browser subscriber; they keep receiving now,
        // so the rows in this frame have already left the shared damage without
        // reaching this channel. Record the debt and let the ticker pay it with a
        // full frame once the gate reopens — sending the delta now would only
        // deepen the backlog this gate exists to drain.
        let stalled = desktop_wants_it && {
            let gate = state.grid.gates.get(session_id);
            match gate.as_deref() {
                Some(gate) if !gate.is_open() => {
                    gate.note_missed();
                    true
                }
                _ => false,
            }
        };
        if stalled {
            // Arm the ticker so the debt is paid even if the session falls silent
            // the instant the WebView recovers: the repair rides a tick, and an
            // undirty session never takes one.
            if let Some(dirty) = state.grid.frame_dirty.get(session_id) {
                dirty.store(true, Ordering::Relaxed);
            }
            return;
        }
        send_desktop_grid_frame(state, session_id, frame);
    }
}

/// Hand `bytes` to the desktop IPC channel and count the frame against the
/// delivery gate. No-op when the session has no desktop channel.
///
/// Separate from [`send_grid_frame`] because the two callers differ in what the
/// frame IS: the broadcast path sends the shared delta to every transport, while
/// the ticker's stall repair sends a full frame to this channel and to nothing
/// else.
#[cfg(feature = "desktop")]
fn send_desktop_grid_frame(state: &AppState, session_id: &str, bytes: Vec<u8>) {
    let Some(ch) = state.grid.channels.get(session_id) else {
        return;
    };
    let gate = state.grid.gates.get(session_id);
    if let Some(gate) = gate.as_deref() {
        gate.mark_sent();
    }
    // `tauri::ipc::Response` is what keeps this binary. A `Vec<u8>` matches
    // only the blanket `IpcResponse` impl, i.e. `serde_json::to_string`, so a
    // 110 KB frame left Rust as a ~280 KB string of decimal numbers, took the
    // over-threshold path (one extra IPC round trip per frame) and arrived in
    // JS as a `number[]` to be walked back into bytes. `Response` carries the
    // bytes as `Raw` and the frontend already accepts an ArrayBuffer.
    if let Err(error) = ch.channel.send(tauri::ipc::Response::new(bytes)) {
        // A frame that never reached the webview will never be acked, and the
        // counters are absolute: leaving this one counted would put the gate one
        // frame behind for the rest of the session, i.e. every later frame would
        // travel at the ticker's 500 ms give-up rate. Give up on it now instead.
        tracing::debug!(session_id = %session_id, %error, "grid frame send failed");
        if let Some(gate) = gate.as_deref() {
            gate.abandon();
        }
    }
}

// --- Scroll commands ---

// DEFERRED (2026-08-18) — `set_ansi_colors` (above) is the fifth: it locks EVERY
// vt buffer in a loop on the IPC thread, so its stall grows with session count.
// Same reordering objection as below — two concurrent calls could leave some
// buffers on the old palette — and it fires once, when the user picks a theme.

// DEFERRED (2026-08-18) — the four grid commands that *mutate* before serializing
// (`terminal_scroll`, `terminal_scroll_to`, `terminal_request_frame`,
// `terminal_exit_alt_screen`) still take the vt lock inline on the IPC thread.
// They carry the same stall as the reads, but not the same safety: two
// `spawn_blocking` hops for the same session can run in either order, and
// `terminal_scroll_to(line)` is absolute — reordering two of them lands the
// viewport on the wrong line. The reads are idempotent, so they moved (F95);
// these need the coalescing `terminal_scroll_to_offset` already has, which is
// also why they are the low-frequency path: the wheel and the scrollbar drag go
// through the offset command and never touch this lock.

/// Run a read against a session's VT buffer on the blocking pool.
///
/// Every grid read takes the VT mutex, and the PTY reader holds that same mutex
/// through a full `serialize_dirty_rows`. A command that waits for it inline in
/// the IPC handler — on macOS, the main thread — freezes the WebView for the
/// length of someone else's serialize.
///
/// All of them go through here, including the ones that only read a single row.
/// The cost that matters is not the work the closure does, it is the wait for
/// the lock, and a one-cell read waits exactly as long as a whole-scrollback
/// search. A line drawn between "cheap" and "expensive" reads would only rot.
///
/// The lock is taken *inside* the closure, on the pool thread. Taking it before
/// the hop would put the wait straight back on the thread this exists to keep
/// free, which is the whole bug.
///
/// This is the shared unit between the two transports rather than the command:
/// the grid commands are `#[cfg(feature = "desktop")]`, so the HTTP routes —
/// which also compile into the headless `tuic-remote` binary — cannot call them.
/// Both call this instead, so neither transport can quietly go back to blocking.
///
/// `None` means the session is gone, which the desktop commands read as a
/// default and the HTTP routes as a 404.
pub(crate) async fn vt_try_read<T, F>(
    state: &Arc<AppState>,
    session_id: String,
    f: F,
) -> Result<Option<T>, String>
where
    F: FnOnce(&mut crate::state::VtLogBuffer) -> T + Send + 'static,
    T: Send + 'static,
{
    let state = Arc::clone(state);
    tokio::task::spawn_blocking(move || {
        state
            .grid
            .vt_log_buffers
            .get(&session_id)
            .map(|vt| f(&mut vt.lock()))
    })
    .await
    .map_err(|e| format!("terminal read failed: {e}"))
}

/// [`vt_try_read`] for the callers that answer a closed session with a default.
///
/// A tab can be closed while a hover, a selection or a row-cache fill is still
/// in flight; that is a race the frontend already tolerates, not an error worth
/// surfacing.
pub(crate) async fn vt_read<T, F>(
    state: &Arc<AppState>,
    session_id: String,
    f: F,
) -> Result<T, String>
where
    F: FnOnce(&mut crate::state::VtLogBuffer) -> T + Send + 'static,
    T: Default + Send + 'static,
{
    vt_try_read(state, session_id, f)
        .await
        .map(Option::unwrap_or_default)
}

// --- Search command ---

// --- Row text command ---

#[cfg(test)]
mod tests;
