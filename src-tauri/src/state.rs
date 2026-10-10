use dashmap::{DashMap, DashSet};
use parking_lot::Mutex;
use serde::{Deserialize, Serialize};
use std::borrow::Cow;
use std::collections::{HashMap, VecDeque};
use std::io::Write;
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicI64, AtomicU16, AtomicU64, AtomicUsize};
use std::time::{Duration, Instant};
#[cfg(feature = "desktop")]
use tauri::{AppHandle, Emitter};

pub(crate) use tuic_terminal::vt_log::{ChangedRow, LogLine, VT_LOG_BUFFER_CAPACITY, VtLogBuffer};

/// A submission waiting for the agent's next safe idle window.
///
/// Server-authored notices, a child's own initial prompt and user-composed
/// commands share one FIFO so acceptance order is preserved across producers.
///
/// What is NOT in here is the point: peer `send` payloads never enter this
/// queue. They are agent-authored text of arbitrary content, and typing them
/// into a recipient's composer turns mail into keystrokes — the recipient's TUI
/// renders it as something the user typed, and an unsubmitted remainder leaves
/// the composer partial. Peer mail lives in the inbox; the only thing this
/// queue may carry on its behalf is the payload-free `PEER_MAIL_WAKE` notice.
///
/// Every variant carries an id. Non-user entries used to be invisible to the
/// Compose count/list, which is how a single parked entry blocked `submit` with
/// `queued_commands_pending` for minutes against an empty composer and no
/// visible pending work. Everything parked here is now listable and removable.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum PendingInjection {
    /// Server-authored one-liner: the payload-free mail wake, or a child
    /// lifecycle summary. Never carries peer `send` content.
    Notice { id: u64, text: String },
    /// A spawned child's own task prompt, withheld from argv for prefill-only
    /// TUIs (codex) and typed once the child's TUI reaches its ready prompt.
    InitialPrompt { id: u64, text: String },
    /// The id is what the Compose panel deletes by: a queue position would shift
    /// under the caller as the FIFO drains on the next idle window.
    UserCommand { id: u64, text: String },
}

/// Ids are unique per process, not per session — a Compose delete carries both.
static NEXT_INJECTION_ID: AtomicU64 = AtomicU64::new(1);

fn next_injection_id() -> u64 {
    NEXT_INJECTION_ID.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
}

impl PendingInjection {
    pub(crate) fn notice(text: impl Into<String>) -> Self {
        Self::Notice {
            id: next_injection_id(),
            text: text.into(),
        }
    }

    pub(crate) fn initial_prompt(text: impl Into<String>) -> Self {
        Self::InitialPrompt {
            id: next_injection_id(),
            text: text.into(),
        }
    }

    pub(crate) fn user_command(text: impl Into<String>) -> Self {
        Self::UserCommand {
            id: next_injection_id(),
            text: text.into(),
        }
    }

    pub(crate) fn id(&self) -> u64 {
        match self {
            Self::Notice { id, .. }
            | Self::InitialPrompt { id, .. }
            | Self::UserCommand { id, .. } => *id,
        }
    }

    pub(crate) fn text(&self) -> &str {
        match self {
            Self::Notice { text, .. }
            | Self::InitialPrompt { text, .. }
            | Self::UserCommand { text, .. } => text,
        }
    }

    /// Stable wire label. The submit rejection and the Compose list both report
    /// it, so an operator can tell whose work is blocking the composer.
    pub(crate) fn kind(&self) -> &'static str {
        match self {
            Self::Notice { .. } => "notice",
            Self::InitialPrompt { .. } => "initial_prompt",
            Self::UserCommand { .. } => "user_command",
        }
    }
}

/// A spawned child's initial prompt that has not reached its composer yet.
///
/// `notified` makes the delivery watchdog idempotent without dropping the
/// prompt: the parent is told once that delivery has not happened, and the
/// prompt stays queued so the child's next ready window still types it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct PendingInitialPrompt {
    pub(crate) prompt: String,
    pub(crate) notified: bool,
}

impl PendingInitialPrompt {
    pub(crate) fn new(prompt: impl Into<String>) -> Self {
        Self {
            prompt: prompt.into(),
            notified: false,
        }
    }
}

// ---------------------------------------------------------------------------
// AppEvent — unified event bus for all backend events
// ---------------------------------------------------------------------------

/// Wire payload of `worktree-created`, on both transports.
///
/// The workspace is named by its id; `branch` rides along for display only. A
/// consumer that wants the branch reads this field rather than parsing the id.
///
/// Typed rather than a hand-written `json!` at each emit site: the desktop
/// `emit` and the SSE arm serialize this same struct, so the two transports
/// cannot spell a field differently.
#[derive(Clone, Debug, Serialize, PartialEq, Eq)]
pub(crate) struct WorktreeCreatedPayload {
    /// Live PTY resolved from the authenticated MCP caller, never request arguments.
    pub(crate) creator_session: Option<String>,
    /// An explicit spawn keeps the creator in its existing workspace.
    pub(crate) spawn_session: bool,
    pub(crate) repo_path: String,
    pub(crate) workspace_id: String,
    pub(crate) branch: String,
    pub(crate) worktree_path: String,
    /// Workspace kind, kept explicit on the shared event payload.
    pub(crate) kind: crate::worktree::WorkspaceKind,
}

/// Wire payload of `worktree-removed`, on both transports.
///
/// `branch` must be captured *before* the checkout is disposed of — the id stops
/// resolving the moment the worktree is gone, so a consumer cannot look it up
/// and neither can this event.
#[derive(Clone, Debug, Serialize, PartialEq, Eq)]
pub(crate) struct WorktreeRemovedPayload {
    pub(crate) repo_path: String,
    pub(crate) workspace_id: String,
    pub(crate) branch: String,
}

/// Events broadcast to SSE/WebSocket consumers via `tokio::sync::broadcast`.
/// All event producers (PTY reader, watchers, session lifecycle, plugins) send
/// to this channel. Consumers: SSE endpoint, WebSocket multiplexer, session
/// state accumulator, Tauri bridge (desktop backward compat).
#[derive(Clone, Debug, Serialize)]
#[serde(tag = "event", content = "payload")]
pub enum AppEvent {
    #[serde(rename = "head-changed")]
    HeadChanged { repo_path: String, branch: String },
    #[serde(rename = "repo-changed")]
    /// A repo changed on disk.
    ///
    /// CONTRACT for any new producer: call `invalidate_repo_caches` BEFORE
    /// sending. The frontend no longer invokes `clear_repo_caches` on this
    /// event (story 619-0685) precisely because every producer already does —
    /// a producer that forgets ships stale panels with nothing red.
    ///
    /// DEFERRED (2026-08-18) — not enforced by a test. Enforcing it needs
    /// either a single choke-point emit helper or a compile-time wrapper type;
    /// with three producers, all audited, the indirection costs more than it
    /// buys. Revisit when a fourth appears.
    RepoChanged {
        repo_path: String,
        kind: crate::repo_watcher::RepoChangeKind,
    },
    #[serde(rename = "session-created")]
    SessionCreated {
        session_id: String,
        cwd: Option<String>,
        agent_type: Option<String>,
        display_name: Option<String>,
        /// `$TUIC_SESSION` of the agent that spawned this PTY, so every client can
        /// mark it as a sub-agent. `None` for tabs a user or plain client opened.
        #[serde(skip_serializing_if = "Option::is_none")]
        parent_session: Option<String>,
    },
    #[serde(rename = "session-closed")]
    SessionClosed { session_id: String, reason: String },
    #[serde(rename = "design-mode-changed")]
    DesignModeChanged {
        repo_path: String,
        session_id: String,
        status: String,
    },
    #[serde(rename = "pty-parsed")]
    PtyParsed {
        session_id: String,
        /// Behind an `Arc` because `emit_pty_event` fans this out to three
        /// broadcast lanes and every receiver clones again on `recv()`. Inline,
        /// that is a deep JSON copy per lane per subscriber on every PTY chunk —
        /// for consumers that read one `type` field and drop the rest.
        parsed: Arc<serde_json::Value>,
    },
    #[serde(rename = "pty-exit")]
    PtyExit { session_id: String },
    /// An OSC 133 shell-integration marker. Field-for-field identical to the
    /// desktop `Osc133Event` payload so the grid WS frame and the Tauri event
    /// carry the same shape and the frontend needs no per-transport branch.
    #[serde(rename = "pty-osc133")]
    PtyOsc133 {
        session_id: String,
        marker: String,
        line: usize,
        exit_code: Option<i32>,
    },
    /// Working directory reported by the shell through OSC 7.
    #[serde(rename = "pty-cwd")]
    PtyCwd { session_id: String, cwd: String },
    /// Terminal title set (or reset to empty) through OSC 0/2.
    #[serde(rename = "pty-title")]
    PtyTitle { session_id: String, title: String },
    /// "This session produced output." Payload-free on purpose: the only
    /// consumers are a last-seen timestamp and an unread flag, neither of which
    /// needs a byte of the output itself. Throttled at the producer — see
    /// [`crate::pty::ACTIVITY_PULSE_WINDOW`] for why dropping pulses is sound.
    #[serde(rename = "pty-activity")]
    PtyActivity { session_id: String },
    /// Assembled PTY lines for the plugin OutputWatchers, carrying the ids Rust
    /// already matched. The frontend re-runs the real `RegExp` on the cleaned
    /// text to hand the plugin a genuine `RegExpExecArray`, and scans the line
    /// for the watchers Rust could not compile.
    #[serde(rename = "plugin-watcher-lines")]
    PluginWatcherLines {
        session_id: String,
        lines: Vec<crate::output_watchers::WatcherLine>,
    },
    /// A tab's display name changed from the backend (MCP `session action=rename`).
    /// IPC/HTTP renames start in the frontend and do not emit it.
    #[serde(rename = "session-renamed")]
    SessionRenamed {
        session_id: String,
        name: String,
        is_custom: bool,
    },
    /// An MCP client asked for a tab to be suspended (`session action=suspend`).
    /// The tab, its agent identity and its resume path live in the frontend, so it
    /// performs the suspend; the backend has already refused a busy session.
    #[serde(rename = "session-suspend-requested")]
    SessionSuspendRequested {
        session_id: String,
        request_id: String,
    },
    /// Orchestrator-supplied description of the work currently assigned to a PTY.
    /// The chat view of a Claude terminal has new entries. Wake only: the entries
    /// stay behind `chat_view_snapshot`, so a client that missed this event reads
    /// the same truth as one that did not.
    #[serde(rename = "chat-view-changed")]
    ChatViewChanged { session_id: String, seq: u64 },
    #[serde(rename = "pty-description-changed")]
    PtyDescriptionChanged {
        session_id: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        description: Option<String>,
    },
    /// The address other agents reach a terminal by (e.g. `tu-3`).
    #[serde(rename = "term-alias-assigned")]
    TermAliasAssigned { session_id: String, alias: String },
    #[serde(rename = "plugin-changed")]
    #[allow(dead_code)] // reserved for future plugin hot-reload notifications
    PluginChanged { plugin_ids: Vec<String> },
    #[serde(rename = "upstream-status-changed")]
    UpstreamStatusChanged { name: String, status: String },
    /// Frontend should open the user's browser at `authorization_url` —
    /// an upstream MCP server has triggered an OAuth flow.
    #[serde(rename = "mcp-oauth-start")]
    McpOAuthStart {
        name: String,
        authorization_url: String,
    },
    /// Toast notification from MCP tool. `sound` carries a resolved
    /// `NotificationSound` name (never a bare boolean): the caller's `true` is
    /// mapped from the level before the event is sent, so every consumer plays
    /// through the notification scheme — volume, device and per-sound mutes
    /// included — instead of inventing its own tone.
    #[serde(rename = "mcp-toast")]
    McpToast {
        title: String,
        message: Option<String>,
        level: String,
        sound: Option<String>,
        #[serde(skip_serializing_if = "Option::is_none")]
        origin_repo_path: Option<String>,
        /// The TUIC session that raised the toast, when the caller is bound to
        /// one. A repo holds many tabs, so the path alone cannot navigate:
        /// clicking the toast uses this to land on the terminal that spoke.
        #[serde(skip_serializing_if = "Option::is_none")]
        origin_session_id: Option<String>,
    },
    /// An MCP agent is blocked on a yes/no answer from the human.
    ///
    /// Broadcast to every client, not just the desktop window: the answer gates a
    /// destructive operation, and a human who is away from the machine must still
    /// be able to give it. The first client to answer resolves the request; the
    /// rest are told to dismiss by `McpConfirmResolved`.
    #[serde(rename = "mcp-confirm")]
    McpConfirm {
        request_id: String,
        title: String,
        message: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        origin_repo_path: Option<String>,
        /// The TUIC session that asked, when the caller is bound to one — so a
        /// client can show which terminal is waiting.
        #[serde(skip_serializing_if = "Option::is_none")]
        origin_session_id: Option<String>,
    },
    /// A pending confirmation was answered (or expired). Clients showing that
    /// request must dismiss it; `confirmed` is the answer that won.
    #[serde(rename = "mcp-confirm-resolved")]
    McpConfirmResolved { request_id: String, confirmed: bool },
    /// Something happened on an ACP connection that a client may want to look at.
    ///
    /// Only the wake signal rides this bus. The ordered frames of a turn stay on
    /// the per-connection Channel/WebSocket, because a single turn emits more of
    /// them per second than this 256-entry broadcast can carry without lagging
    /// every unrelated subscriber in the app.
    ///
    /// The payload keeps the ACP surface's camelCase field names rather than the
    /// snake_case used by the PTY events around it: it is the same object the
    /// `/acp` routes and the desktop commands return, and renaming it here would
    /// give a client two spellings for one thing.
    #[serde(rename = "acp-notice")]
    AcpNotice(crate::acp::AcpNotice),
    /// `repositories.json` was written by some client.
    ///
    /// Payload-free on purpose. One backend serves the desktop WebView, the
    /// browser and the PWA, and each keeps its own compare-and-swap baseline; the
    /// only thing a receiver needs to know is "disk moved, re-read it". Shipping
    /// the document instead would copy the whole repository set to every client on
    /// every save — including the client that just wrote it.
    #[serde(rename = "repositories-changed")]
    RepositoriesChanged,
    /// Directory contents changed (non-git filesystem watcher)
    #[serde(rename = "dir-changed")]
    DirChanged { dir_path: String },
    /// A worktree was created via MCP — frontend may offer to switch to it
    #[serde(rename = "worktree-created")]
    WorktreeCreated(WorktreeCreatedPayload),
    /// Caller-only placement; the directory already exists.
    #[serde(rename = "session-worktree-declared")]
    SessionWorktreeDeclared(WorktreeCreatedPayload),
    /// A worktree was removed (UI, MCP, HTTP, or merge&archive) — frontend must
    /// drop its sidebar row and close any terminal still living in it.
    #[serde(rename = "worktree-removed")]
    WorktreeRemoved(WorktreeRemovedPayload),
    /// A peer agent registered for inter-agent messaging
    #[serde(rename = "peer-registered")]
    PeerRegistered { tuic_session: String, name: String },
    /// A peer agent was unregistered (session closed or reaped)
    #[serde(rename = "peer-unregistered")]
    PeerUnregistered { tuic_session: String },
    /// Open or update a plugin panel tab from MCP
    #[serde(rename = "ui-tab")]
    UiTab {
        id: String,
        title: String,
        html: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        url: Option<String>,
        pinned: bool,
        focus: bool,
        /// Caller repo/cwd — so the tab is scoped to the repo the MCP session
        /// is working in, not to whichever repo has focus in the frontend.
        #[serde(skip_serializing_if = "Option::is_none")]
        origin_repo_path: Option<String>,
    },
    /// GitHub PR statuses updated for a repo (from Rust poller)
    #[serde(rename = "github-pr-update")]
    GitHubPrUpdate {
        repo_path: String,
        statuses: Vec<crate::github::BranchPrStatus>,
    },
    /// GitHub PR transition detected (merged, closed, blocked, etc.)
    #[serde(rename = "github-transition")]
    GitHubTransition {
        transition: crate::github_poller::PrTransition,
    },
    /// GitHub issues updated for a repo (from Rust poller)
    #[serde(rename = "github-issues-update")]
    GitHubIssuesUpdate {
        repo_path: String,
        issues: Vec<crate::github::GitHubIssue>,
    },
    /// Close HTML tabs owned by a session (emitted on session exit)
    #[serde(rename = "close-html-tabs")]
    CloseHtmlTabs { tab_ids: Vec<String> },
    /// Reserved lifecycle event for a GitHub Ops workflow. The producer is
    /// wired when the workflow graduates from primitive to runtime flow.
    #[allow(dead_code)]
    #[serde(rename = "conflict-assist-status")]
    ConflictAssistStatus {
        repo_path: String,
        payload: serde_json::Value,
    },
    /// A Progress event was durably persisted (`progress` MCP tool, or the
    /// equivalent Tauri command/HTTP route). `repo_path` is the *owning
    /// project root* the event was written under, which may differ from the
    /// reporting workspace for a managed linked or nested workspace. Never
    /// sent for a `duplicate` or `paused` outcome — only a receipt actually
    /// backed by a new row reaches this bus.
    #[serde(rename = "progress-recorded")]
    ProgressRecorded {
        repo_path: String,
        payload: serde_json::Value,
    },
    #[serde(rename = "workflow-run-changed")]
    WorkflowRunChanged {
        repo_path: String,
        payload: serde_json::Value,
    },
    /// An ego PR review started or finished.
    ///
    /// Two events per review and not one per file: the review is a single
    /// unattended ego turn (`acp::oneshot`), so there is no per-file phase to
    /// report and a payload that claimed one would be inventing it. The
    /// dashboard column needs to know a review is running and what it found;
    /// that is exactly what this carries.
    #[serde(rename = "review-progress")]
    ReviewProgress {
        repo_path: String,
        payload: serde_json::Value,
    },
    /// An ego improvement scan produced its proposals.
    ///
    /// Pushed rather than returned only, because a scan started from the
    /// dashboard must also reach a dashboard open on another transport.
    #[serde(rename = "proposals-ready")]
    ProposalsReady {
        repo_path: String,
        payload: serde_json::Value,
    },
    /// A session's derived lifecycle state moved (working / idle / awaiting).
    ///
    /// The only OUTPUT of the session-state accumulator, and the only variant
    /// nothing feeds back into it — see the no-op arm in
    /// `apply_event_to_session_state`. Published once per real transition so the
    /// desktop can render badges from a push instead of a 1 Hz
    /// `list_active_sessions` poll (#687-be9d).
    ///
    /// The payload is field-for-field an entry of that command's response
    /// (`SessionInfo`'s `session_id` + `state`), and the same object the
    /// browser-mode WebSocket already sends as `{"type":"state","state":…}`, so
    /// one frontend applier serves every transport.
    #[serde(rename = "session-state-changed")]
    SessionStateChanged {
        session_id: String,
        /// Boxed: `SessionState` is ~528 bytes against ~144 for the next-largest
        /// arm, and `AppEvent` is cloned once per broadcast subscriber, so an
        /// inline copy makes every *other* event pay for this one.
        state: Box<SessionState>,
    },
    /// A remote connection's status, base URL or token moved.
    ///
    /// The payload is the whole client view of that connection, not a delta: a
    /// client that missed an event must never be left holding a base URL the
    /// backend has retracted.
    #[serde(rename = "remote-connection-status")]
    RemoteConnectionStatusChanged { payload: serde_json::Value },
    /// An event a remote daemon published, repeated verbatim on this bus.
    ///
    /// `event` is the daemon's own event name and `payload` its own body, so a
    /// consumer cannot tell a mirrored event from a local one — which is the
    /// point: a remote session must raise the same badge, the same notification
    /// and the same queue gate as a local one, with no second code path to keep
    /// in step. It also means a new event type crosses for free.
    ///
    /// It is a no-op for the session-state accumulator: the daemon ran its own
    /// accumulator already and this is its output, not an input (#791-055e).
    #[serde(rename = "remote-mirrored")]
    RemoteMirrored {
        connection_id: String,
        event: String,
        payload: serde_json::Value,
    },
    /// A Whisper model download moved. `payload` is the body the desktop
    /// `dictation-download-progress` emit carries, built once so the two
    /// transports cannot describe the same download differently.
    #[cfg(feature = "dictation")]
    #[serde(rename = "dictation-download-progress")]
    DictationDownloadProgress { payload: serde_json::Value },
    /// A speech asset download moved — the runtime library or one language
    /// bundle. Keyed by asset inside the payload, because a user can start two
    /// downloads at once and one shared percent would show each of them the
    /// other's.
    #[cfg(feature = "dictation")]
    #[serde(rename = "speech-download-progress")]
    SpeechDownloadProgress { payload: serde_json::Value },
    /// A spoken reply changed state: queued, rendering, speaking, finished,
    /// interrupted or failed.
    ///
    /// `payload` is a serialized `dictation::commands::SpokenReply`, the same
    /// shape `speak` returns and `speech_status` nests. Carried as a value
    /// rather than as fields for the reason above: one serializer, so a client
    /// polling the status and a client reading the stream cannot be told two
    /// different things about one reply.
    ///
    /// Pushed from the speaker's own transitions, never from `speak` — the
    /// interesting ones (`finished`, `interrupted`) happen on the render thread
    /// long after `speak` returned, and a consumer that had to discover them
    /// would be polling.
    #[cfg(feature = "dictation")]
    #[serde(rename = "speech-utterance")]
    SpeechUtterance { payload: serde_json::Value },
}

/// The wire body of [`AppEvent::SessionStateChanged`], shared by the desktop
/// window event and the `/events` SSE arm.
///
/// One builder rather than two: the payload has to be byte-identical across the
/// transports for the same frontend applier to consume both, and the pairs this
/// codebase builds from separate code in separate files are exactly the ones
/// that drift.
pub(crate) fn session_state_payload(session_id: &str, state: &SessionState) -> serde_json::Value {
    serde_json::json!({ "session_id": session_id, "state": state })
}

impl AppEvent {
    /// Session id for the PTY-scoped variants that are routed to a per-session
    /// channel in addition to the global bus. `None` for all other variants
    /// (they ride the global `event_bus` only). Keep this in sync with the
    /// variants the session-scoped WS handlers care about.
    pub(crate) fn pty_session_id(&self) -> Option<&str> {
        match self {
            AppEvent::SessionCreated { session_id, .. }
            | AppEvent::PtyParsed { session_id, .. }
            | AppEvent::PluginWatcherLines { session_id, .. }
            | AppEvent::PtyExit { session_id }
            | AppEvent::PtyActivity { session_id }
            | AppEvent::PtyTitle { session_id, .. }
            | AppEvent::PtyOsc133 { session_id, .. }
            | AppEvent::PtyCwd { session_id, .. }
            | AppEvent::PtyDescriptionChanged { session_id, .. }
            | AppEvent::SessionRenamed { session_id, .. }
            | AppEvent::SessionSuspendRequested { session_id, .. }
            | AppEvent::TermAliasAssigned { session_id, .. }
            | AppEvent::SessionClosed { session_id, .. } => Some(session_id),
            _ => None,
        }
    }
}

// ---------------------------------------------------------------------------
// SessionState — server-side accumulator for REST polling (mobile/browser)
// ---------------------------------------------------------------------------

fn is_zero(v: &u32) -> bool {
    *v == 0
}

/// Per-session state accumulated from broadcast events.
/// Updated by a background task that subscribes to the event bus.
/// Read by `GET /sessions` to enrich the response for REST-polling clients.
///
/// `Deserialize` because a remote daemon's `GET /sessions` row carries this
/// state back to the machine that mirrors it (#791-055e). `serde(default)` is
/// load-bearing there: most fields are `skip_serializing_if`, so the wire form
/// omits everything the remote had nothing to say about.
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(default)]
pub(crate) struct SessionState {
    /// True when a Question parsed event is pending (no subsequent user-input or pty-exit)
    pub awaiting_input: bool,
    /// The question text, if awaiting input
    #[serde(skip_serializing_if = "Option::is_none")]
    pub question_text: Option<String>,
    /// True when the question was detected with high confidence (Ink menu footer).
    /// False for silence-based `?` heuristic detections.
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    pub question_confident: bool,
    /// True when a rate-limit parsed event is active
    pub rate_limited: bool,
    /// Retry-after in ms from the rate-limit event
    #[serde(skip_serializing_if = "Option::is_none")]
    pub retry_after_ms: Option<u64>,
    /// Epoch ms when rate_limited was set; used for auto-expiry.
    #[serde(skip)]
    pub rate_limit_set_ms: u64,
    /// Usage limit percentage (0-100), if detected
    #[serde(skip_serializing_if = "Option::is_none")]
    pub usage_limit_pct: Option<u8>,
    /// Shell state derived from PTY output timing (matches desktop model).
    /// "busy" = recent PTY output (< 500ms), "idle" = no recent output.
    /// Computed on-the-fly when serializing; None for sessions with no output yet.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub shell_state: Option<String>,
    /// Agent task lifecycle, distinct from PTY activity. `completed` requires
    /// the explicit `suggest:` protocol marker; silence alone remains `idle`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub agent_state: Option<String>,
    /// True while the agent owns a meaningful background descendant even if
    /// its terminal composer is ready for input. Persistent integration helpers
    /// are excluded by the PTY process-tree classifier.
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    pub background_work: bool,
    /// Turn whose confirmed-ready observation is waiting for a process snapshot
    /// newer than `background_probe_after_generation`.
    #[serde(skip)]
    pub(crate) background_probe_turn_epoch: Option<u64>,
    /// Snapshot generation visible when the ready observation was made.
    #[serde(skip)]
    pub(crate) background_probe_after_generation: Option<u64>,
    /// Turn whose ready probe has been reconciled against a newer snapshot.
    #[serde(skip)]
    pub(crate) background_probe_satisfied_turn_epoch: Option<u64>,
    /// Last process snapshot applied to this session. Prevents repeated work
    /// from one shared cache generation.
    #[serde(skip)]
    pub(crate) background_snapshot_generation: u64,
    /// Timestamp of last activity (any event for this session).
    /// Excluded from PartialEq — telemetry field, not logical state.
    pub last_activity_ms: u64,
    /// Detected agent type, if known
    #[serde(skip_serializing_if = "Option::is_none")]
    pub agent_type: Option<String>,
    /// Run-config identity is armed before first observation; discovered identity
    /// is revoked when a shell returns to the foreground.
    #[serde(skip)]
    pub(crate) agent_type_from_run_config: bool,
    /// A preset is armed during shell startup only. Once its agent has been
    /// observed, shell foreground means it has exited and cannot receive input.
    #[serde(skip)]
    pub(crate) agent_foreground_observed: bool,
    /// Telegram opt-in lifetime, retired synchronously on observed agent exit.
    #[serde(skip)]
    pub(crate) telegram_registration_lifetime: Option<Arc<()>>,
    /// Accepted OS foreground snapshot order; late completion cannot overwrite
    /// a newer observation from another timer/IPC/HTTP caller.
    #[serde(skip)]
    pub(crate) foreground_probe_generation: u64,
    #[serde(skip)]
    pub(crate) spawn_root_role: SpawnRootRole,
    /// A direct agent child owns foreground, or root observation is unavailable.
    #[serde(skip)]
    pub(crate) foreground_input_blocked: bool,
    #[serde(skip)]
    pub(crate) foreground_probe_result: Option<String>,
    /// Keep the foreground detection warning to one record per session.
    #[serde(skip)]
    pub(crate) unknown_foreground_warned: bool,
    /// True when this agent has native-hook instrumentation enabled, so heuristic
    /// question-detection is suppressed (awaiting comes from OSC 7770 instead).
    /// Resolved from config when `agent_type` is set; internal, not serialized.
    #[serde(skip)]
    pub hook_instrumented: bool,
    /// Last API error, if any
    #[serde(skip_serializing_if = "Option::is_none")]
    pub last_error: Option<String>,
    /// Current agent intent text (from `intent: ...` tokens)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub agent_intent: Option<String>,
    /// Current task name from the agent status line
    #[serde(skip_serializing_if = "Option::is_none")]
    pub current_task: Option<String>,
    /// Last user prompt with >= 10 words
    #[serde(skip_serializing_if = "Option::is_none")]
    pub last_prompt: Option<String>,
    /// Current progress value (0-100); None when no active progress bar
    #[serde(skip_serializing_if = "Option::is_none")]
    pub progress: Option<u8>,
    /// Number of active sub-tasks (local agents, bash, background tasks) from ›› mode line
    #[serde(skip_serializing_if = "is_zero")]
    pub active_sub_tasks: u32,
    /// Commands waiting in `pending_injections` for this session's next
    /// BUSY→IDLE transition. Derived at snapshot time like `shell_state`, not
    /// accumulated from events.
    #[serde(skip_serializing_if = "is_zero")]
    pub queued_commands: u32,
    /// Suggested follow-up actions from the agent (from `suggest: ...` tokens)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub suggested_actions: Option<Vec<String>>,
    /// Monotonic input-turn epoch. Suggest events carry the epoch in which they
    /// were parsed so the async accumulator cannot restore prior-turn completion.
    #[serde(skip)]
    pub(crate) turn_epoch: u64,
    /// One draft may interrupt this turn until the next native input write.
    #[serde(skip)]
    pub(crate) turn_interrupt: Option<Arc<()>>,
    /// Native input retired this epoch; late drafts cannot rearm it.
    #[serde(skip)]
    pub(crate) turn_interrupt_retired_epoch: Option<u64>,
    /// Slash command menu items (from slash-menu parsed events)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub slash_menu_items: Option<Vec<crate::output_parser::SlashMenuItem>>,
    /// Active numbered choice dialog (edit-confirm, bash-confirm, apply-patch, ...).
    /// Cleared when the dialog disappears (e.g. on input, scroll, ptyexit).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub choice_prompt: Option<crate::output_parser::ChoicePromptPayload>,
    /// Current terminal mode — Shell or FullscreenTui with app hint + nesting depth.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub terminal_mode: Option<crate::ai_agent::tui_detect::TerminalMode>,
    /// Epoch ms of last push notification sent for this session (rate limiting)
    #[serde(skip)]
    pub last_push_ms: Option<u64>,
}

/// Immutable spawn metadata; unknown mirrors cannot authorize local injection.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) enum SpawnRootRole {
    #[default]
    Unknown,
    Shell,
    DirectProgram,
}

impl SessionState {
    pub(crate) fn has_pending_background_probe(&self) -> bool {
        self.background_probe_turn_epoch == Some(self.turn_epoch)
    }
}

/// Lossless lane for events that mutate the authoritative per-session state.
/// The public broadcast bus may drop messages for a lagging receiver; state
/// transitions cannot, because losing either SET or CLEAR strands clients in a
/// state that never existed or never ended.
pub(crate) struct SessionStateEventQueue {
    tx: tokio::sync::mpsc::UnboundedSender<AppEvent>,
    rx: Mutex<Option<tokio::sync::mpsc::UnboundedReceiver<AppEvent>>>,
    /// Events sent and not yet applied, counted on both ends. See [`Self::depth`].
    depth: AtomicUsize,
}

impl Default for SessionStateEventQueue {
    fn default() -> Self {
        Self::new()
    }
}

impl SessionStateEventQueue {
    pub(crate) fn new() -> Self {
        let (tx, rx) = tokio::sync::mpsc::unbounded_channel();
        Self {
            tx,
            rx: Mutex::new(Some(rx)),
            depth: AtomicUsize::new(0),
        }
    }

    /// Queue `event`, counting it as outstanding until the accumulator applies it.
    fn send(&self, event: AppEvent) {
        // Counted BEFORE the send, never after: the accumulator runs on another task and
        // can receive and decrement before `send` even returns here. An increment after
        // that would leave the depth permanently one too high, and a `fetch_sub` reaching
        // zero first would wrap a `usize` to `usize::MAX`.
        self.depth
            .fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        if self.tx.send(event).is_err() {
            // The receiver is gone, so nothing will ever apply this one.
            self.depth
                .fetch_sub(1, std::sync::atomic::Ordering::Relaxed);
        }
    }

    /// Record that the accumulator finished applying one event.
    fn applied(&self) {
        self.depth
            .fetch_sub(1, std::sync::atomic::Ordering::Relaxed);
    }

    /// Events queued on the lane and not yet applied.
    ///
    /// The lane is deliberately unbounded, so a stalled accumulator is invisible until
    /// it is a memory problem; the Diagnostics snapshot reports this and nothing acts on
    /// it. Counted on both ends rather than read off the channel for two reasons. Tokio
    /// puts `len()` on the RECEIVER, not the sender, and the receiver is `take()`n by
    /// `spawn_session_state_accumulator` so no other caller can reach it. More
    /// importantly, publishing `rx.len()` from inside that task would only refresh while
    /// the task still runs — it would report a stale small number exactly when the task
    /// wedges, which is the failure this metric exists to reveal.
    pub(crate) fn depth(&self) -> usize {
        self.depth.load(std::sync::atomic::Ordering::Relaxed)
    }
}

pub(crate) fn resolve_choice_prompt_input(state: &AppState, session_id: &str, data: &str) -> bool {
    {
        let Some(mut session) = state.session_maps.session_states.get_mut(session_id) else {
            return false;
        };
        let Some(prompt) = session.choice_prompt.as_ref() else {
            return false;
        };
        let resolves = (prompt.selection_mode.is_none()
            && prompt.options.iter().any(|option| option.key == data))
            || matches!(data, "\r" | "\n")
            || match prompt.dismiss_key.as_deref() {
                Some("cancel") => data == "\x1b",
                Some("ctrl+]") => data == "\x1d",
                _ => false,
            }
            || match prompt.amend_key.as_deref() {
                Some("amend") => data == "\t",
                Some("alt+down") => data == "\x1b[1;3B",
                _ => false,
            };
        if !resolves {
            return false;
        }
        session.choice_prompt = None;
        session.awaiting_input = false;
        session.question_text = None;
        session.question_confident = false;
    }
    // #744-138c: clear the shared awaiting evidence too, outside the
    // session_states lock just dropped above (SilenceState → SessionState
    // lock order — see apply_event_to_session_state's PtyParsed arm).
    if let Some(silence) = state.session_maps.silence_states.get(session_id) {
        silence.lock().clear_awaiting();
    }
    state.emit_pty_event(AppEvent::PtyParsed {
        session_id: session_id.to_string(),
        parsed: serde_json::json!({ "type": "choice-cleared" }).into(),
    });
    true
}

/// PartialEq excludes last_activity_ms (telemetry, not logical state).
impl SessionState {
    /// Seed configured identity synchronously, before readers/events can use it.
    pub(crate) fn seed_configured_agent(&mut self, agent_type: Option<String>) {
        self.agent_type_from_run_config = agent_type.is_some();
        self.agent_foreground_observed = false;
        self.agent_type = agent_type;
    }
}

/// Used by WS dedup to avoid sending identical state frames.
impl PartialEq for SessionState {
    fn eq(&self, other: &Self) -> bool {
        self.awaiting_input == other.awaiting_input
            && self.question_text == other.question_text
            && self.question_confident == other.question_confident
            && self.rate_limited == other.rate_limited
            && self.retry_after_ms == other.retry_after_ms
            && self.usage_limit_pct == other.usage_limit_pct
            && self.shell_state == other.shell_state
            && self.agent_state == other.agent_state
            && self.background_work == other.background_work
            && self.agent_type == other.agent_type
            && self.last_error == other.last_error
            && self.agent_intent == other.agent_intent
            && self.current_task == other.current_task
            && self.active_sub_tasks == other.active_sub_tasks
            && self.queued_commands == other.queued_commands
            && self.last_prompt == other.last_prompt
            && self.progress == other.progress
            && self.suggested_actions == other.suggested_actions
            && self.slash_menu_items == other.slash_menu_items
            && self.choice_prompt == other.choice_prompt
            && self.terminal_mode == other.terminal_mode
    }
}

/// TTL for git operations (local disk): 60s fallback.
/// Primary invalidation is event-driven via repo_watcher FSEvents.
/// This TTL is a safety net for cases where the watcher misses an event.
pub(crate) const GIT_CACHE_TTL: Duration = Duration::from_secs(60);

/// TTL for GitHub operations (network): aligned with poller BASE_INTERVAL
pub(crate) const GITHUB_CACHE_TTL: Duration = Duration::from_secs(60);

/// Connect timeout for the shared HTTP client. A TCP handshake to GitHub that
/// has not completed in 10s is a dead route — a dropped VPN, a captive portal,
/// a black-holed SYN — not a slow one. Without it the OS default applies
/// (~75s on macOS) and every caller inherits that stall.
pub(crate) const HTTP_CONNECT_TIMEOUT: Duration = Duration::from_secs(10);

/// Total timeout for one request on the shared HTTP client, matching
/// `plugin_http::DEFAULT_TIMEOUT_SECS`. It covers the whole exchange, including
/// reading the body, so it bounds the half-open socket a VPN drop leaves
/// behind: the peer keeps the connection open and simply never answers, which
/// no connect timeout can catch. The GitHub poller awaits these requests inline
/// in its `select!`, so an unbounded one wedges the poller itself.
pub(crate) const HTTP_REQUEST_TIMEOUT: Duration = Duration::from_secs(30);

/// Build the shared HTTP client with explicit timeouts.
///
/// Split from [`build_http_client`] so tests can drive the same builder with
/// timeouts short enough to observe.
fn http_client_with_timeouts(connect: Duration, request: Duration) -> reqwest::Client {
    reqwest::Client::builder()
        .connect_timeout(connect)
        .timeout(request)
        .build()
        .expect("Failed to build shared HTTP client")
}

/// The shared async HTTP client used for every GitHub API call.
pub(crate) fn build_http_client() -> reqwest::Client {
    http_client_with_timeouts(HTTP_CONNECT_TIMEOUT, HTTP_REQUEST_TIMEOUT)
}

/// Buffer that handles UTF-8 characters split across read boundaries.
/// Carries incomplete trailing bytes from one read to the next.
pub(crate) struct Utf8ReadBuffer {
    /// Incomplete bytes from the previous read (at most 3 bytes for a 4-byte sequence)
    pub(crate) remainder: Vec<u8>,
}

impl Utf8ReadBuffer {
    pub(crate) fn new() -> Self {
        Self {
            remainder: Vec::with_capacity(4),
        }
    }

    /// Process raw bytes from a read, returning valid UTF-8 text.
    /// Incomplete multi-byte sequences at the end are saved for the next call.
    pub(crate) fn push(&mut self, new_bytes: &[u8]) -> String {
        let mut combined = Vec::with_capacity(self.remainder.len() + new_bytes.len());
        combined.extend_from_slice(&self.remainder);
        combined.extend_from_slice(new_bytes);
        self.remainder.clear();

        // Walk the buffer iteratively, emitting valid runs and one U+FFFD per invalid
        // sequence. Iterative (not recursive) on purpose: a fully-binary chunk is one
        // invalid run per byte, and the old recursive version blew the reader thread's
        // stack on ~thousands of consecutive invalid bytes (4 KB binary read).
        let mut result = String::with_capacity(combined.len());
        let mut pos = 0;
        loop {
            let slice = &combined[pos..];
            match std::str::from_utf8(slice) {
                Ok(s) => {
                    result.push_str(s);
                    break;
                }
                Err(e) => {
                    let valid = e.valid_up_to();
                    // SAFETY: slice[..valid] verified valid UTF-8 by valid_up_to().
                    result.push_str(unsafe { std::str::from_utf8_unchecked(&slice[..valid]) });
                    match e.error_len() {
                        // Incomplete trailing sequence — save for the next read and stop.
                        None => {
                            self.remainder.extend_from_slice(&slice[valid..]);
                            break;
                        }
                        // Invalid byte(s) — emit one replacement char, skip them, continue.
                        Some(error_len) => {
                            result.push('\u{FFFD}');
                            pos += valid + error_len;
                        }
                    }
                }
            }
        }
        result
    }

    /// Flush any remaining bytes (at EOF). Incomplete sequences are dropped.
    pub(crate) fn flush(&mut self) -> String {
        if self.remainder.is_empty() {
            return String::new();
        }
        let remaining = std::mem::take(&mut self.remainder);
        String::from_utf8_lossy(&remaining).to_string()
    }
}

/// Buffer that prevents escape sequences from being split across write boundaries.
/// Detects incomplete ANSI/OSC sequences at the end of a chunk and carries them
/// to the next call, so xterm.js always receives complete sequences.
pub(crate) struct EscapeAwareBuffer {
    /// Incomplete escape sequence bytes carried from the previous chunk.
    remainder: String,
}

impl EscapeAwareBuffer {
    pub(crate) fn new() -> Self {
        Self {
            remainder: String::new(),
        }
    }

    /// Process a UTF-8 string chunk, returning text safe to write to xterm.js.
    /// Any trailing incomplete escape sequence is held for the next call.
    pub(crate) fn push(&mut self, input: &str) -> String {
        if input.is_empty() && self.remainder.is_empty() {
            return String::new();
        }

        let mut data = std::mem::take(&mut self.remainder);
        data.push_str(input);

        // Find safe split point: the last position where we're NOT inside an escape sequence
        let safe = find_safe_boundary(&data);

        if safe == data.len() {
            // Entire string is safe
            data
        } else if safe == 0 {
            // Entire string is an incomplete escape — hold it all
            // But cap at 256 bytes to prevent unbounded growth from garbage input
            if data.len() > 256 {
                // Give up and emit it raw — likely not a real escape sequence
                data
            } else {
                self.remainder = data;
                String::new()
            }
        } else {
            self.remainder = data[safe..].to_string();
            data.truncate(safe);
            data
        }
    }

    /// Flush remaining bytes at EOF.
    pub(crate) fn flush(&mut self) -> String {
        std::mem::take(&mut self.remainder)
    }
}

/// Find the last byte position where the string is not inside an incomplete escape sequence.
/// Returns data.len() if the entire string is safe, or a smaller index if trailing bytes
/// form an incomplete sequence.
fn find_safe_boundary(data: &str) -> usize {
    let bytes = data.as_bytes();
    let len = bytes.len();
    if len == 0 {
        return 0;
    }

    // Scan backwards from the end to find incomplete escape sequences.
    // We only need to check the last ~256 bytes (max reasonable escape sequence length).
    let scan_start = len.saturating_sub(256);

    // Walk forward through the tail to track escape state
    let mut i = scan_start;
    let mut last_safe = len; // Assume safe unless we find an incomplete escape

    while i < len {
        let b = bytes[i];
        if b == 0x1b {
            // ESC — start of a potential escape sequence
            let seq_start = i;
            i += 1;
            if i >= len {
                // ESC at very end — incomplete
                last_safe = seq_start;
                break;
            }

            match bytes[i] {
                b'[' => {
                    // CSI sequence: ESC [ <params> <final byte>
                    // Parameter bytes: 0x30-0x3F, intermediate: 0x20-0x2F
                    // Final byte: 0x40-0x7E (@A-Z[\]^_`a-z{|}~)
                    i += 1;
                    let mut found_final = false;
                    while i < len {
                        let c = bytes[i];
                        if (0x40..=0x7E).contains(&c) {
                            // Final byte found — sequence is complete
                            i += 1;
                            found_final = true;
                            break;
                        }
                        if c == 0x1b {
                            // New ESC interrupts — this CSI is broken, treat as complete
                            found_final = true;
                            break;
                        }
                        i += 1;
                    }
                    if !found_final {
                        // Ran off the end without finding final byte — incomplete
                        last_safe = seq_start;
                    }
                }
                b']' => {
                    // OSC sequence: ESC ] <text> (ST | BEL)
                    // ST = ESC \ , BEL = 0x07
                    i += 1;
                    let mut terminated = false;
                    while i < len {
                        let c = bytes[i];
                        if c == 0x07 {
                            // BEL terminator
                            i += 1;
                            terminated = true;
                            break;
                        }
                        if c == 0x1b && i + 1 < len && bytes[i + 1] == b'\\' {
                            // ST terminator (ESC \)
                            i += 2;
                            terminated = true;
                            break;
                        }
                        if c == 0x1b && (i + 1 >= len || bytes[i + 1] != b'\\') {
                            // New ESC that's not ST — OSC is broken, treat as complete
                            terminated = true;
                            break;
                        }
                        i += 1;
                    }
                    if !terminated {
                        last_safe = seq_start;
                    }
                }
                b'P' => {
                    // DCS sequence: ESC P <text> ST
                    i += 1;
                    let mut terminated = false;
                    while i < len {
                        let c = bytes[i];
                        if c == 0x1b && i + 1 < len && bytes[i + 1] == b'\\' {
                            i += 2;
                            terminated = true;
                            break;
                        }
                        if c == 0x1b && (i + 1 >= len || bytes[i + 1] != b'\\') {
                            terminated = true;
                            break;
                        }
                        i += 1;
                    }
                    if !terminated {
                        last_safe = seq_start;
                    }
                }
                _ => {
                    // Simple ESC + single char (e.g., ESC c, ESC 7, ESC 8)
                    // Sequence is complete
                    i += 1;
                }
            }
        } else {
            i += 1;
        }
    }

    last_safe
}

/// Fixed-capacity circular buffer for PTY output, readable by external consumers (MCP bridge).
/// Stores raw terminal output bytes; consumers get the last N bytes on demand.
pub struct OutputRingBuffer {
    buf: Vec<u8>,
    capacity: usize,
    /// Write position (wraps around). When total_written < capacity, data starts at 0.
    write_pos: usize,
    /// Total bytes ever written (monotonic). Consumers use this to detect missed data.
    pub total_written: u64,
    /// Caller-computed secrets of the whole ring, valid while `total_written`
    /// is unchanged.
    secret_cache: Option<(u64, Vec<String>)>,
}

impl OutputRingBuffer {
    pub fn new(capacity: usize) -> Self {
        Self {
            buf: vec![0u8; capacity],
            capacity,
            write_pos: 0,
            total_written: 0,
            secret_cache: None,
        }
    }

    /// `compute()` for the ring's current content, recomputed only after a
    /// write: polling agents read far more often than the ring changes.
    pub fn cached_secrets(&mut self, compute: impl FnOnce() -> Vec<String>) -> Vec<String> {
        if let Some((written, secrets)) = &self.secret_cache
            && *written == self.total_written
        {
            return secrets.clone();
        }
        let secrets = compute();
        self.secret_cache = Some((self.total_written, secrets.clone()));
        secrets
    }

    /// Bytes this ring holds. The buffer is allocated full at construction, so
    /// this — not `len` — is what `memory_report` must count: `len` reads zero
    /// on a ring that has already reserved its whole capacity.
    pub fn capacity(&self) -> usize {
        self.capacity
    }

    /// Append data to the ring buffer using bulk copy to avoid per-byte overhead.
    pub fn write(&mut self, data: &[u8]) {
        let len = data.len();
        if len == 0 {
            return;
        }
        // How many bytes fit from write_pos to end of buffer
        let first_chunk = (self.capacity - self.write_pos).min(len);
        self.buf[self.write_pos..self.write_pos + first_chunk]
            .copy_from_slice(&data[..first_chunk]);
        if first_chunk < len {
            // Wrap around: copy the remainder starting at index 0
            let second_chunk = len - first_chunk;
            self.buf[..second_chunk].copy_from_slice(&data[first_chunk..]);
            self.write_pos = second_chunk;
        } else {
            self.write_pos += first_chunk;
            if self.write_pos == self.capacity {
                self.write_pos = 0;
            }
        }
        self.total_written += len as u64;
    }

    /// Read the last `limit` bytes (or fewer if not enough data).
    /// Returns (bytes, total_written) so consumers can track position.
    pub fn read_last(&self, limit: usize) -> (Vec<u8>, u64) {
        let available = std::cmp::min(self.total_written as usize, self.capacity);
        let to_read = std::cmp::min(limit, available);
        if to_read == 0 {
            return (Vec::new(), self.total_written);
        }

        let mut result = Vec::with_capacity(to_read);
        // Start position: write_pos - to_read, wrapping around
        let start = if self.write_pos >= to_read {
            self.write_pos - to_read
        } else {
            self.capacity - (to_read - self.write_pos)
        };

        // Bulk copy using extend_from_slice (two slices if wrapping)
        let first_chunk = (self.capacity - start).min(to_read);
        result.extend_from_slice(&self.buf[start..start + first_chunk]);
        if first_chunk < to_read {
            result.extend_from_slice(&self.buf[..to_read - first_chunk]);
        }

        (result, self.total_written)
    }

    /// Number of bytes currently readable, which saturates at `capacity` once
    /// the ring wraps — unlike `total_written`, which keeps climbing. Returns a
    /// count rather than making callers do `read_last(usize::MAX).0.len()`,
    /// which copies the whole ring just to measure it.
    ///
    /// Test-only for now, like `total_written()` below: the production readers
    /// all want the bytes, not the count.
    #[cfg(test)]
    pub fn len(&self) -> usize {
        std::cmp::min(self.total_written as usize, self.capacity)
    }

    /// Read bytes written after `since_offset` (based on `total_written`).
    /// Returns (bytes, current_total_written).
    /// If `since_offset` is older than the buffer capacity, returns whatever is still available.
    pub fn read_since(&self, since_offset: u64) -> (Vec<u8>, u64) {
        if since_offset >= self.total_written {
            return (Vec::new(), self.total_written);
        }
        let bytes_behind = (self.total_written - since_offset) as usize;
        // Clamp to available data in the ring buffer
        let available = std::cmp::min(self.total_written as usize, self.capacity);
        let to_read = std::cmp::min(bytes_behind, available);
        self.read_last(to_read)
    }

    /// Current total_written counter (bytes ever written, monotonically increasing).
    #[cfg(test)]
    pub fn total_written(&self) -> u64 {
        self.total_written
    }
}

pub(crate) const OUTPUT_RING_BUFFER_CAPACITY: usize = 2 * 1024 * 1024; // 2 MB

/// Kitty keyboard protocol: actions detected in PTY output.
/// Applications send these sequences to request enhanced key encoding.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum KittyAction {
    /// `CSI > flags u` — push flags onto the stack
    Push(u32),
    /// `CSI < u` — pop one entry from the stack
    Pop,
    /// `CSI ? u` — query current flags (terminal responds with `CSI ? flags u`)
    Query,
}

/// Per-session kitty keyboard protocol state.
/// Tracks a stack of flag values as specified by the protocol.
pub(crate) struct KittyKeyboardState {
    stack: Vec<u32>,
}

impl KittyKeyboardState {
    pub(crate) fn new() -> Self {
        Self { stack: Vec::new() }
    }

    /// Push flags onto the stack.
    pub(crate) fn push(&mut self, flags: u32) {
        self.stack.push(flags);
    }

    /// Pop one entry from the stack. No-op if already empty (underflow safety).
    pub(crate) fn pop(&mut self) {
        self.stack.pop();
    }

    /// Current effective flags (top of stack, or 0 if empty).
    pub(crate) fn current_flags(&self) -> u32 {
        self.stack.last().copied().unwrap_or(0)
    }
}

/// Scan PTY output for kitty keyboard protocol sequences and strip them.
///
/// Detects:
/// - `ESC [ > N u` — push flags (N is one or more digits)
/// - `ESC [ < u`   — pop
/// - `ESC [ ? u`   — query
///
/// Returns the cleaned string (with kitty sequences removed) and a list of actions.
/// Fast path: if the input contains none of the trigger prefixes, returns it unchanged.
pub(crate) fn strip_kitty_sequences(input: &str) -> (Cow<'_, str>, Vec<KittyAction>) {
    // Fast path: skip scanning if no possible kitty sequence prefix exists
    if !input.contains("\x1b[>") && !input.contains("\x1b[<") && !input.contains("\x1b[?") {
        return (Cow::Borrowed(input), Vec::new());
    }

    let bytes = input.as_bytes();
    let len = bytes.len();
    let mut actions = Vec::new();
    // Track ranges of the input to KEEP (everything except kitty sequences).
    // At the end we concatenate these slices to preserve UTF-8 integrity.
    let mut kept_start = 0; // start of current "keep" span
    let mut kept_ranges: Vec<(usize, usize)> = Vec::new();
    let mut i = 0;

    while i < len {
        if bytes[i] == 0x1b && i + 2 < len && bytes[i + 1] == b'[' {
            match bytes[i + 2] {
                b'>' => {
                    // Potential push: ESC [ > digits u
                    let mut j = i + 3;
                    while j < len && bytes[j].is_ascii_digit() {
                        j += 1;
                    }
                    if j > i + 3 && j < len && bytes[j] == b'u' {
                        // Valid push — save preceding text, skip sequence
                        if i > kept_start {
                            kept_ranges.push((kept_start, i));
                        }
                        let digits = &input[i + 3..j];
                        if let Ok(flags) = digits.parse::<u32>() {
                            actions.push(KittyAction::Push(flags));
                        }
                        i = j + 1;
                        kept_start = i;
                        continue;
                    }
                    // Not a kitty push — advance past ESC only
                    i += 1;
                }
                b'<' => {
                    if i + 3 < len && bytes[i + 3] == b'u' {
                        // Valid pop — save preceding text, skip sequence
                        if i > kept_start {
                            kept_ranges.push((kept_start, i));
                        }
                        actions.push(KittyAction::Pop);
                        i += 4;
                        kept_start = i;
                        continue;
                    }
                    i += 1;
                }
                b'?' => {
                    if i + 3 < len && bytes[i + 3] == b'u' {
                        // Valid query — save preceding text, skip sequence
                        if i > kept_start {
                            kept_ranges.push((kept_start, i));
                        }
                        actions.push(KittyAction::Query);
                        i += 4;
                        kept_start = i;
                        continue;
                    }
                    i += 1;
                }
                _ => {
                    i += 1;
                }
            }
        } else {
            i += 1;
        }
    }

    // If no kitty sequences were found, return original string
    if actions.is_empty() {
        return (Cow::Borrowed(input), actions);
    }

    // Flush trailing kept span
    if kept_start < len {
        kept_ranges.push((kept_start, len));
    }

    // Concatenate kept slices (all are valid UTF-8 sub-slices of input)
    let mut output = String::with_capacity(len);
    for (start, end) in &kept_ranges {
        output.push_str(&input[*start..*end]);
    }

    (Cow::Owned(output), actions)
}

pub use tuic_git::worktree::WorktreeInfo;

/// Represents a PTY session with optional worktree
pub type SharedPtyWriter = Arc<Mutex<Box<dyn Write + Send>>>;

pub struct PtySession {
    pub(crate) launch_receipt: Option<crate::prompt_receipt::PromptReceipt>,
    /// Kept outside the session mutex so terminal-generated replies can wait
    /// for an in-flight user write without blocking the reader thread.
    pub writer: SharedPtyWriter,
    pub master: Box<dyn portable_pty::MasterPty + Send>,
    pub(crate) _child: Box<dyn portable_pty::Child + Send + Sync>,
    pub(crate) paused: Arc<AtomicBool>,
    pub worktree: Option<WorktreeInfo>,
    /// Immutable launch directory used for repository ownership, independent of OSC 7.
    pub(crate) initial_cwd: Option<String>,
    pub cwd: Option<String>,
    /// Display name set by the desktop UI, agent launch, or intent title.
    pub display_name: Option<String>,
    /// Only explicit user renames are protected from OSC/intent title updates.
    pub display_name_is_custom: bool,
    /// The name was given by the agent spawn that created the session. The UI
    /// reads it on reload to keep the agent's own OSC title from replacing that
    /// name; the row's other fields cannot tell it apart from a synced OSC title.
    pub display_name_from_spawn: bool,
    /// Created through HTTP/MCP rather than the local desktop UI.
    pub is_remote: bool,
    /// Resolved shell command used to spawn the PTY (e.g. "/bin/zsh",
    /// "C:\\Program Files\\Git\\bin\\bash.exe", "wsl.exe -d Ubuntu").
    /// Kept so `get_session_shell_family` can classify without re-resolving.
    pub shell: String,
}

impl PtySession {
    /// Record the tab name the UI synced, from either transport. A non-custom
    /// name is an OSC or intent title, which refines a spawn name without
    /// replacing it; only a user rename (`is_custom`) ends the spawn origin.
    pub(crate) fn set_display_name(&mut self, name: Option<String>, is_custom: bool) {
        self.display_name = name;
        self.display_name_is_custom = is_custom;
        if is_custom {
            self.display_name_from_spawn = false;
        }
    }
}

/// Configuration for agent orchestration
pub(crate) const MAX_CONCURRENT_SESSIONS: usize = 50;

/// PTY subsystem metrics for observability.
/// All counters use AtomicUsize for lock-free, zero-overhead-when-idle tracking.
pub(crate) struct SessionMetrics {
    pub(crate) total_spawned: AtomicUsize,
    pub(crate) failed_spawns: AtomicUsize,
    pub(crate) active_sessions: AtomicUsize,
    pub(crate) bytes_emitted: AtomicUsize,
    pub(crate) pauses_triggered: AtomicUsize,
}

impl SessionMetrics {
    pub(crate) const fn new() -> Self {
        Self {
            total_spawned: AtomicUsize::new(0),
            failed_spawns: AtomicUsize::new(0),
            active_sessions: AtomicUsize::new(0),
            bytes_emitted: AtomicUsize::new(0),
            pauses_triggered: AtomicUsize::new(0),
        }
    }
}

/// Metadata for an active MCP protocol session.
/// Stored per session_id so tool handlers can check client identity at call time.
#[derive(Debug, Clone)]
pub struct McpSessionMeta {
    pub(crate) prompt_instructions: Option<crate::prompt_receipt::PromptReceipt>,
    /// Last time the session was used (any request); reaper checks this.
    pub last_activity: Instant,
    /// Whether the client identified as Claude Code (or tuic-bridge) at initialize time
    pub is_claude_code: bool,
    /// Whether this client needs the three meta-tool compatibility surface.
    /// Grok rejects nested `server__upstream__tool` identifiers, so its MCP
    /// session uses `search_tools` / `get_tool_schema` / `call_tool` instead.
    pub requires_meta_tools: bool,
    /// Whether this session has an active SSE stream (GET /mcp connected)
    pub has_sse_stream: bool,
    /// Which GET /mcp stream currently owns this session. A reconnect can be
    /// accepted while the previous half-open stream is still being dropped, and
    /// that drop must not tear down its replacement: each stream records the
    /// generation it was given and releases the session only when it still
    /// holds it.
    pub sse_generation: u64,
    /// Repo path extracted from MCP initialize `roots[0].uri` (file:// URI → absolute path).
    /// Used by downstream per-project filtering to scope tool access.
    pub repo_path: Option<String>,
}

/// A registered peer agent in the inter-agent messaging system.
/// Keyed by `tuic_session` (a stable peer UUID from TUIC_SESSION env var).
#[derive(Debug, Clone, Serialize)]
pub struct PeerAgent {
    /// Stable peer UUID (from TUIC_SESSION env var) — primary identifier
    pub tuic_session: String,
    /// MCP session ID (for routing notifications via SSE)
    pub mcp_session_id: String,
    /// Display name (tab name or agent-chosen name)
    pub name: String,
    /// Git repo root this agent is working on (for filtering)
    pub project: Option<String>,
    /// When the agent registered (unix millis for serialization)
    pub registered_at: u64,
}

/// A message in the inter-agent mailbox.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct AgentMessage {
    /// Unique message ID
    pub id: String,
    /// Sender's tuicSession UUID
    pub from_tuic_session: String,
    /// Sender display name
    pub from_name: String,
    /// Message body (max 64 KB)
    pub content: String,
    /// Per-recipient logical unix-millis cursor
    pub timestamp: u64,
    /// Whether this message was pushed via SSE channel notification.
    ///
    /// Server-side forensics only — never serialized. It reports ONE sub-route,
    /// but every reader so far has parsed it as a delivery verdict: it stays
    /// `false` when a waiter or the terminal carried the message, i.e. exactly
    /// when delivery worked best. That ambiguity already removed it from the
    /// `send` response; emitting it on `inbox`/`wait` is the same trap aimed at
    /// the recipient, who is by definition holding the message it describes.
    /// The route lives in `delivery_path` (sender) and the `agent_msg` tracing
    /// line (operator).
    #[serde(skip)]
    pub delivered_via_channel: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum AgentDeliveryOwner {
    Waiter,
    WaiterObserved,
    TerminalPending,
    TerminalDispatched,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum AgentDeliveryAssignment {
    Waiter,
    Terminal,
    InboxOnly,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum UrgentNoticeReservation {
    AlreadyRead,
    InFlight,
    Written,
    Reserved,
}

#[derive(Debug, Clone, Copy)]
struct UrgentNotice {
    first_through: u64,
    through: u64,
    written: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum OrchestratorDeliveryAssignment {
    Waiter,
    WakeSubmitted,
    /// The notice typed the covered payloads themselves, so the recipient owes
    /// no `inbox` round-trip for that window.
    WakeSummarySubmitted,
    WakeCoalesced,
    InboxOnly,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum OrchestratorWakeAttemptOutcome {
    Submitted,
    /// Submitted, and the written line already carried every payload in the
    /// reserved group. Reportable ONLY for a group that is entirely made of
    /// server-authored lifecycle notifications — that is what makes the notice
    /// self-acknowledging instead of a pointer into the inbox.
    SummarySubmitted,
    NotStarted,
    Uncertain,
}

/// The inbox window a single wake notice is reserved to cover, as the
/// half-open range `(observed_through, wake_through]` in per-recipient logical
/// cursor units.
///
/// The generic wake ignores it: one payload-free notice covers an unbounded
/// group because the recipient answers it by reading the whole inbox. A
/// self-acknowledging summary cannot — it only describes what it printed — so
/// it needs the exact bounds both to render the right messages and to advance
/// the cursor by exactly as much as it covered.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct OrchestratorWakeGroup {
    pub(crate) observed_through: u64,
    pub(crate) wake_through: u64,
}

#[cfg(test)]
impl From<bool> for OrchestratorWakeAttemptOutcome {
    fn from(submitted: bool) -> Self {
        if submitted {
            Self::Submitted
        } else {
            Self::Uncertain
        }
    }
}

const ORCHESTRATOR_WAKE_ATTEMPT_LIMIT: u8 = 2;

#[derive(Debug)]
pub(crate) struct AgentDeliveryGate {
    next_lease: u64,
    active_waiters: std::collections::HashSet<u64>,
    owners: HashMap<String, AgentDeliveryOwner>,
    urgent_notices: HashMap<String, UrgentNotice>,
    orchestrator_wake_pending_through: Option<u64>,
    orchestrator_wake_needed_through: Option<u64>,
    orchestrator_observed_through: u64,
    orchestrator_wake_attempt: u64,
    orchestrator_wake_attempts_in_group: u8,
    inbox_revision: u64,
    inbox_events: tokio::sync::watch::Sender<u64>,
}

impl Default for AgentDeliveryGate {
    fn default() -> Self {
        let (inbox_events, _) = tokio::sync::watch::channel(0);
        Self {
            next_lease: 0,
            active_waiters: std::collections::HashSet::new(),
            owners: HashMap::new(),
            urgent_notices: HashMap::new(),
            orchestrator_wake_pending_through: None,
            orchestrator_wake_needed_through: None,
            orchestrator_observed_through: 0,
            orchestrator_wake_attempt: 0,
            orchestrator_wake_attempts_in_group: 0,
            inbox_revision: 0,
            inbox_events,
        }
    }
}

impl AgentDeliveryGate {
    fn reset_orchestrator_wake_budget_if_observed(&mut self) {
        if self.orchestrator_wake_pending_through.is_none()
            && self.orchestrator_wake_needed_through.is_none()
        {
            self.orchestrator_wake_attempts_in_group = 0;
        }
    }
}

#[derive(Debug, Default, PartialEq, Eq)]
pub(crate) struct AgentWaitFinish {
    pub fresh_count: usize,
    pub messages: Vec<AgentMessage>,
    pub terminal_handoff: Vec<String>,
}

/// How often a session emitted each protocol marker, and how many turns it had
/// the chance to.
///
/// `turns` counts submitted turns, not wall-clock time, because that is the only
/// denominator that makes `suggest` comparable across a chatty session and a
/// quiet one. A ratio above 1 is normal and not a bug: an agent may emit
/// `intent:` several times inside one turn as the work changes phase.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, serde::Serialize)]
pub(crate) struct MarkerStats {
    pub(crate) intent: u64,
    pub(crate) suggest: u64,
    pub(crate) turns: u64,
}

/// Which tally to bump. Named rather than three methods so the call sites read
/// as one vocabulary.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum MarkerKind {
    Intent,
    Suggest,
    TurnSubmitted,
}

/// Max messages per agent inbox before FIFO eviction.
pub(crate) const AGENT_INBOX_CAPACITY: usize = 100;

/// Message-id prefix marking auto-generated lifecycle/system notifications.
pub(crate) const LIFECYCLE_MSG_ID_PREFIX: &str = "tuic-auto-";

fn lifecycle_notice_kind(message: &AgentMessage) -> Option<String> {
    if !message.id.starts_with(LIFECYCLE_MSG_ID_PREFIX) || message.from_name != "tuic" {
        return None;
    }
    let payload = serde_json::from_str::<serde_json::Value>(&message.content).ok()?;
    let kind = payload.get("type")?.as_str()?;
    // Each answered question can start a new wait. Keep those notices even if
    // ordinary lifecycle updates from the same child are coalesced.
    if kind == "state_change" && payload.get("state")?.as_str()? == "awaiting_input" {
        return None;
    }
    Some(kind.to_string())
}

/// Max message body size in bytes (64 KB).
pub(crate) const AGENT_MESSAGE_MAX_BYTES: usize = 64 * 1024;

/// Max concurrent *monitoring* git subprocesses (see `monitoring_git_sem`).
/// Tuned to keep background refresh responsive while preventing the FD/CPU
/// storm that a repo-changed burst across many repos would otherwise cause.
pub(crate) const MONITORING_GIT_CONCURRENCY: usize = 8;

/// The GitHub half of [`AppState`], grouped so the 116-field struct reads as
/// subsystems rather than a flat list (#678-9a75).
///
/// Every field is constructible without arguments, so the group carries its own
/// `Default` and `AppState::new` names it once instead of seven times.
pub(crate) struct GitHubState {
    /// GitHub API token — updated on fallback when a 401 triggers candidate rotation
    pub(crate) token: parking_lot::RwLock<Option<String>>,
    /// Where the current GitHub token came from (env, OAuth keyring, gh CLI)
    pub(crate) token_source: parking_lot::RwLock<crate::github_auth::TokenSource>,
    /// Circuit breaker for GitHub API calls
    pub(crate) circuit_breaker: crate::github::GitHubCircuitBreaker,
    /// Background GitHub poller task handle
    pub(crate) poller: parking_lot::Mutex<Option<crate::github_poller::GitHubPoller>>,
    /// Cached GitHub viewer login (authenticated user) for issue filtering.
    pub(crate) viewer_login: parking_lot::RwLock<Option<String>>,
    /// Remaining GraphQL points from last poll — used for proactive throttling.
    /// Initialized to u32::MAX (no constraint). Written by each successful batch poll.
    /// This is the github.com budget; GHE accounts track their own in `ghe_state`.
    pub(crate) rate_limit_remaining: std::sync::atomic::AtomicU32,
    /// Per-account runtime state for non-github.com (GHE) accounts: breaker +
    /// viewer-login cache + rate budget, keyed by account id. github.com uses the
    /// global fields above, so a github.com-only user is byte-for-byte unchanged.
    pub(crate) ghe_state: DashMap<String, crate::github::GheAccountState>,
}

impl Default for GitHubState {
    fn default() -> Self {
        Self {
            token: parking_lot::RwLock::new(None),
            token_source: parking_lot::RwLock::new(Default::default()),
            circuit_breaker: crate::github::GitHubCircuitBreaker::new(),
            poller: parking_lot::Mutex::new(None),
            viewer_login: parking_lot::RwLock::new(None),
            // u32::MAX means "no constraint known yet", not "no budget left".
            rate_limit_remaining: std::sync::atomic::AtomicU32::new(u32::MAX),
            ghe_state: dashmap::DashMap::new(),
        }
    }
}

/// The MCP half of [`AppState`] (#678-9a75): Streamable-HTTP sessions, the
/// upstream proxy registry, OAuth flows, and the tool-search index they feed.
pub struct McpState {
    /// Active MCP Streamable HTTP sessions (session_id -> metadata for TTL reaping + client identity)
    pub sessions: DashMap<String, McpSessionMeta>,
    /// Upstream MCP proxy registry — aggregates tools from all connected upstreams.
    pub(crate) upstream_registry: Arc<crate::mcp_proxy::registry::UpstreamRegistry>,
    /// Orchestrator for in-flight OAuth 2.1 authorization flows. Flows run
    /// concurrently — each one owns its `state` nonce, PKCE verifier and
    /// callback port, so there is nothing to serialize.
    pub(crate) oauth_flow_manager: Arc<crate::mcp_oauth::flow::OAuthFlowManager>,
    /// Broadcast channel for MCP `notifications/tools/list_changed`.
    /// Fired when native tools are toggled or upstream tool lists change.
    pub(crate) tools_changed: tokio::sync::broadcast::Sender<()>,
    /// MCP session → PTY session mapping for caller identity resolution.
    /// Populated at agent spawn time; used by self-close guard in session(close).
    pub to_session: DashMap<String, String>,
    /// Reverse index of `to_session`: tuic_session → list of mcp_session_ids.
    /// Populated alongside `to_session` at agent(register). Lets
    /// `tombstone_transient_cleanup` remove entries in O(1) instead of scanning
    /// every entry of `to_session` on each session exit.
    pub session_to_mcp: DashMap<String, Vec<String>>,
    /// Cached BM25 search index over the full tool corpus (native + upstream),
    /// filtered by `disabled_native_tools`. Used by the MCP `search_tools` /
    /// `get_tool_schema` meta-handlers and the Command Palette. Rebuilt on
    /// every `tools_changed` signal by the updater task in
    /// `mcp_http::mcp_transport::spawn_tool_search_index_updater`.
    pub(crate) tool_search_index: Arc<parking_lot::RwLock<crate::tool_search::ToolSearchIndex>>,
}

impl Default for McpState {
    fn default() -> Self {
        Self {
            sessions: DashMap::new(),
            upstream_registry: Arc::new(crate::mcp_proxy::registry::UpstreamRegistry::new()),
            oauth_flow_manager: Arc::new(crate::mcp_oauth::flow::OAuthFlowManager::new()),
            tools_changed: tokio::sync::broadcast::channel(16).0,
            to_session: DashMap::new(),
            session_to_mcp: DashMap::new(),
            // Empty until the updater task builds the real corpus.
            tool_search_index: Arc::new(parking_lot::RwLock::new(
                crate::tool_search::ToolSearchIndex::build(&[]),
            )),
        }
    }
}

#[cfg(feature = "desktop")]
pub(crate) struct DesktopGridChannel {
    pub(crate) channel: tauri::ipc::Channel<tauri::ipc::Response>,
    pub(crate) webview_label: String,
    pub(crate) epoch: u64,
}

/// The rendering half of [`AppState`] (#678-9a75): one VT grid per session,
/// the raw-byte flight recorder beside it, and the channels and gates that
/// deliver frames to a frontend.
///
/// Every field is an empty map at startup, so the group derives its `Default`.
#[derive(Default)]
pub(crate) struct GridState {
    /// Per-session VT100 log buffers for clean mobile/REST output (session_id → buffer).
    /// Separate DashMap to avoid writer contention on PtySession.
    pub(crate) vt_log_buffers: DashMap<String, Mutex<VtLogBuffer>>,
    /// Per-session ring of the most recent raw PTY output bytes (pre-transform),
    /// capped at `PTY_RAW_RING_CAP`. Always on — memory-only flight recorder for
    /// emulation bugs (story 056-7545): when a rendering corruption shows up in
    /// the wild, GET /sessions/{id}/raw-ring dumps the exact byte stream for
    /// offline replay (terminal_grid.rs `replay_capture_from_env`).
    pub(crate) pty_raw_rings: DashMap<String, Mutex<std::collections::VecDeque<u8>>>,
    /// Binary on purpose: `Channel<Vec<u8>>` serialises to a JSON number array,
    /// `Channel<Response>` keeps the raw bytes. See `send_grid_frame`.
    #[cfg(feature = "desktop")]
    pub(crate) channels: DashMap<String, DesktopGridChannel>,
    /// Watch channel for WebSocket grid streaming (session_id → sender).
    /// Uses latest-frame-wins semantics: slow WS clients skip intermediate frames.
    pub(crate) watch: DashMap<String, crate::grid_watch::GridWatchTx>,
    /// Flow control: frames sent vs frames the frontend reported receiving. While
    /// the gate is closed the ticker skips sending — damage accumulates in
    /// alacritty. See [`crate::grid_gate::GridGate`] for why it counts instead of
    /// holding a bool.
    pub(crate) gates: DashMap<String, Arc<crate::grid_gate::GridGate>>,
    /// Dirty flag: set by PTY reader when new data is processed, cleared by frame ticker.
    /// Decouples read() from frame serialization to coalesce rapid writes (spinners).
    pub(crate) frame_dirty: DashMap<String, Arc<AtomicBool>>,
    /// Hint: true while a DEC 2026 synchronized update is open on this session.
    /// Written by the PTY reader after each `process()`, read by the frame ticker
    /// so an idle tick only takes the vt lock for sessions that can actually have
    /// a stalled update to flush.
    pub(crate) sync_update_active: DashMap<String, Arc<AtomicBool>>,
    /// Pending coalesced scroll target (absolute display offset, -1 = none).
    /// Set by `terminal_scroll_to_offset` without taking the vt lock; applied by the
    /// frame ticker under the lock it already holds, so scroll never blocks on the
    /// PTY output processor.
    pub(crate) pending_scroll: DashMap<String, Arc<AtomicI64>>,
}

/// What a terminal session has learnt about its own commands (#678-9a75).
///
/// This is all that is left of the embedded agent subsystem after the engine was
/// deleted (#784-0aec): no loop, no sandbox, no watcher, no scheduler. The two
/// fields below are written by `pty.rs` from OSC 133 boundaries and read back by
/// the same file to explain a failed command, so they outlived the LLM that used
/// to consume them as well.
///
/// `Default` is derived: both fields start empty.
#[derive(Default)]
pub(crate) struct AiAgentState {
    /// Per-session command outcome + error/fix knowledge store.
    /// Populated by pty.rs OSC 133 hooks and SessionState transitions.
    pub(crate) session_knowledge:
        DashMap<String, Mutex<crate::ai_agent::knowledge::SessionKnowledge>>,
    /// Sessions with unpersisted knowledge changes. Flushed to disk every 2s
    /// by the background knowledge-persist task.
    pub(crate) knowledge_dirty: DashMap<String, ()>,
}

/// The per-session side tables of [`AppState`] (#678-9a75).
///
/// Every one is keyed by session id and starts empty, so the group derives its
/// own `Default` and `AppState::new` names it once instead of 28 times.
#[derive(Default)]
pub struct SessionMaps {
    pub sessions: DashMap<String, Mutex<PtySession>>,
    /// Ring buffers for MCP output access (one per session)
    pub output_buffers: DashMap<String, Mutex<OutputRingBuffer>>,
    /// Per-session kitty keyboard protocol state (session_id → state).
    /// Separate DashMap (not inside PtySession) to avoid writer contention.
    pub(crate) kitty_states: DashMap<String, Mutex<KittyKeyboardState>>,
    /// Per-session input line buffers for reconstructing user input from PTY writes.
    /// Separate DashMap (like kitty_states) to avoid writer lock contention.
    pub(crate) input_buffers: DashMap<String, Mutex<crate::input_line_buffer::InputLineBuffer>>,
    /// Last relevant user prompt per session (>= 10 words).
    /// Updated on each qualifying user input line, read by the Activity Dashboard.
    pub(crate) last_prompts: DashMap<String, String>,
    /// Orchestrator-supplied description of the current PTY task.
    /// Separate from `last_prompts`: one describes assigned work, the other records
    /// the latest user instruction actually submitted to the agent.
    pub(crate) pty_descriptions: DashMap<String, String>,
    /// Per-session silence state for fallback question detection.
    /// Shared between the reader thread and write_pty so user-typed lines can be suppressed.
    pub(crate) silence_states: DashMap<String, Arc<Mutex<crate::pty::SilenceState>>>,
    /// Per-session state accumulated from broadcast events (for REST polling).
    pub(crate) session_states: DashMap<String, SessionState>,
    /// Lossless single-consumer lane for PTY events that mutate `session_states`.
    pub(crate) session_state_events: SessionStateEventQueue,
    /// Per-session slash command mode (true when input starts with `/`).
    /// Used to suppress false-positive slash menu detection on PTY output.
    pub(crate) slash_mode: DashMap<String, std::sync::atomic::AtomicBool>,
    /// Per-session timestamp of last PTY output (epoch ms).
    /// Updated by PTY reader on every non-empty chunk. Used to derive shell_state:
    /// "busy" when now - last < 500ms, "idle" otherwise (matches desktop model).
    pub(crate) last_output_ms: DashMap<String, AtomicU64>,
    /// Per-session timestamp of last user input written to the PTY (epoch ms).
    /// Stamped by `write_pty`. The grid ticker reads it to throttle frame sends
    /// while the user is actively typing AND the system CPU is saturated, so the
    /// WebView main thread stays free to dispatch keystrokes instead of churning
    /// through agent output. Absent until the first keystroke.
    pub(crate) last_input_ms: DashMap<String, AtomicU64>,
    /// Per-session shell activity state (AtomicU8: 0=null, 1=busy, 2=idle).
    /// Updated by the reader thread and silence timer via compare_exchange.
    /// The single source of truth for busy/idle — the frontend consumes events,
    /// it does not derive this state from raw PTY output timing.
    pub(crate) shell_states: DashMap<String, std::sync::atomic::AtomicU8>,
    /// Per-session terminal viewport rows. Updated by resize_pty, read by the
    /// reader thread to clamp cursor-up (ESC[nA) sequences to viewport height.
    pub(crate) terminal_rows: DashMap<String, AtomicU16>,
    /// Per-session resize serialization lock. `resize_session_core` clones the Arc
    /// and holds the mutex across BOTH the grid resize and `master.resize` so two
    /// concurrent differing resizes (Tauri command + HTTP route) cannot interleave
    /// and leave the grid and PTY at mismatched dimensions (CONC-B, story 100-e303).
    /// The stored `(rows, cols)` is the last size that actually reached the PTY —
    /// the no-op guard compares against it, not just the grid, so a grid-resized-
    /// but-PTY-failed retry still re-applies the PTY. `(0, 0)` = nothing applied yet.
    pub(crate) resize_locks: DashMap<String, Arc<Mutex<(u16, u16)>>>,
    /// Exit codes for tombstoned sessions (session_id → code).
    /// Populated by `pty::mark_session_exited` when a PTY process exits so
    /// post-mortem `session action=output` reads can return the real code.
    /// Reaped by `pty::spawn_tombstone_sweeper` alongside the output buffers.
    pub(crate) exit_codes: DashMap<String, i32>,
    /// Epoch-ms timestamp of last shell_state transition per session.
    /// Updated by `pty::try_shell_transition` on every successful CAS.
    /// Used by `session(status)` to compute idle_since_ms / busy_duration_ms.
    pub(crate) shell_state_since_ms: DashMap<String, std::sync::atomic::AtomicU64>,
    /// HTML tab IDs (pluginIds) created by each session (tuic_session → [tab_id]).
    /// Populated by ui(tab) calls from registered agents; cleared on session exit
    /// so orphan tabs can be auto-closed by the frontend.
    pub(crate) session_html_tabs: DashMap<String, Vec<String>>,
    /// `$TUIC_SESSION` → the PTY session key that currently backs it.
    ///
    /// These are two independently minted UUIDs: `create_pty` keys `sessions` by a
    /// fresh `Uuid::new_v4()` while exporting the caller-supplied `tuic_session` to
    /// the agent's environment, and the messaging layer historically assumed they
    /// were the same value. They never are, so a self-registered agent's peer
    /// identity matched no PTY and its wake-ups silently degraded to inbox-only.
    /// Recording the pair here is what lets delivery resolve a stable peer identity
    /// to whatever terminal currently backs it — including after a respawn, which
    /// mints a new session key under the same `$TUIC_SESSION`.
    pub(crate) live_pty_by_tuic_session: DashMap<String, String>,
    /// Parent session for swarm-spawned agents (child_tuic_session → parent_tuic_session).
    /// Populated at spawn time when caller_tuic is set. Used to route auto-notifications
    /// (state_change messages) to the orchestrator's inbox on exit and idle transitions.
    pub(crate) session_parent: DashMap<String, String>,
    /// Per-MCP-session broadcast channels for inter-agent messaging notifications.
    /// Each SSE listener subscribes; `send` action pushes here for real-time delivery.
    pub(crate) messaging_channels: DashMap<String, tokio::sync::broadcast::Sender<String>>,
    /// Per-PTY-session broadcast channels carrying that session's `AppEvent`s
    /// (`PtyParsed`/`PtyExit`/`SessionClosed`). Populated by `emit_pty_event`
    /// ALONGSIDE the global `event_bus` (which still feeds `/events` SSE and the
    /// state accumulator). The session-scoped WS handlers subscribe here instead
    /// of the global bus, so a session's events are no longer cloned+filtered by
    /// every other session's WS receiver. Created on-demand when a WS handler
    /// subscribes; reaped in `cleanup_session`/`tombstone_transient_cleanup`.
    pub(crate) pty_event_channels: DashMap<String, tokio::sync::broadcast::Sender<AppEvent>>,
    /// Sessions whose shell has emitted at least one OSC 133 marker. Presence
    /// here suppresses the Inferred-outcome fallback, since the shell-integration
    /// path is authoritative once wired.
    pub(crate) has_osc133_integration: DashMap<String, ()>,
    /// session_id → human alias (e.g. "tc-1", "nr-2"). Assigned on session creation/restore.
    pub(crate) term_aliases: DashMap<String, String>,
    /// Per-prefix counter for alias numbering (e.g. "tc" → 2 means next is tc-3).
    pub(crate) term_alias_counters: DashMap<String, u32>,
    /// Per-session tab visibility (session_id → visible). Updated by the
    /// frontend on tab focus changes. Read by the watcher engine to evaluate
    /// the Unseen trigger (fires only when the terminal tab is not visible).
    pub(crate) session_visibility: DashMap<String, bool>,
    /// Sessions currently in standby (SIGSTOP'd). session_id → epoch ms when stopped.
    #[cfg(unix)]
    pub(crate) standby_sessions: DashMap<String, u64>,
    /// Per-session marker tallies, so "the agents are ignoring the markers" can be
    /// answered with a number instead of by grepping scrollback — which counts any
    /// mention of the word and is capped by buffer size (#4421).
    pub(crate) marker_stats: DashMap<String, MarkerStats>,
}

/// Global state for managing PTY sessions and worktrees
pub struct AppState {
    pub(crate) workflow_runtime: crate::workflows::WorkflowRuntime,
    pub(crate) secrets: crate::secrets::SecretStore,
    /// Every per-session side table, keyed by session id.
    pub(crate) session_maps: SessionMaps,
    pub(crate) data_dir: PathBuf,
    pub(crate) worktrees_dir: PathBuf,
    pub(crate) metrics: SessionMetrics,
    /// Everything MCP: HTTP sessions, the proxy registry, OAuth flows and
    /// the tool-search index.
    pub mcp: McpState,
    /// WebSocket clients per PTY session for streaming output
    pub ws_clients: DashMap<String, Vec<WsClientTx>>,
    /// Cached AppConfig to avoid re-reading from disk on every request
    pub(crate) config: parking_lot::RwLock<crate::config::AppConfig>,
    /// TTL caches for git and GitHub query results
    pub(crate) git_cache: GitCacheState,
    /// Raw file watchers per repo (keyed by repo path), with per-category
    /// debounce. macOS/Windows: one recursive `notify::RecommendedWatcher` over
    /// the repo root. Linux: pruned non-recursive working-tree watches + targeted
    /// `.git` watches (issue #82). Stored behind `Arc` so the Linux event callback
    /// can clone a stable handle and add watches for newly created dirs without
    /// holding a `DashMap` ref across the blocking `watch()` call.
    pub(crate) repo_watchers: DashMap<String, Arc<crate::repo_watcher::RepoWatchHandle>>,
    /// Last emitted git-state fingerprint per repo path. The repo watcher skips
    /// the `repo-changed` (git-state) emit when the fingerprint is unchanged, so a
    /// no-op `.git` touch (e.g. a `--no-optional-locks` status refreshing the index
    /// stat cache) doesn't trigger the full ~20-panel frontend re-render cascade.
    pub(crate) repo_git_fingerprints: DashMap<String, u64>,
    /// Last emitted resolved-HEAD target per repo path (`resolve_head_target`
    /// output). The repo watcher skips the `head-changed` emit when this is
    /// unchanged, suppressing the Linux inotify storm where `.git/HEAD` events
    /// recur without HEAD actually moving (issue #82).
    pub(crate) repo_head_targets: DashMap<String, String>,
    /// Count of `head-changed` emits suppressed by the `repo_head_targets`
    /// guard — surfaced in diagnostic snapshots to quantify watcher storm
    /// volume in production (issue #82).
    pub(crate) repo_head_emits_suppressed: AtomicU64,
    pub(crate) pending_orphan_cleanup: DashMap<String, crate::worktree::PendingOrphanCleanup>,
    /// File watchers for directory contents (keyed by absolute dir path)
    pub(crate) dir_watchers: DashMap<String, crate::repo_watcher::WatchHandle>,
    /// File watcher for the themes/ directory — kept alive for the app lifetime.
    pub(crate) theme_watcher: parking_lot::Mutex<Option<notify::RecommendedWatcher>>,
    /// Byte cursors and per-subagent summaries behind the Progress Flow view,
    /// so each refresh parses only what Claude appended to a subagent
    /// transcript since the last one. Stays empty until Flow is first shown.
    pub(crate) subagent_map_cache: parking_lot::Mutex<crate::subagent_map::MapCache>,
    /// Per-terminal chat view tails, created on first read and dropped when no
    /// client has read for a while.
    pub(crate) chat_views: crate::chat_view::ChatViews,
    /// Shared mdkb daemon client for AST navigation (outline, goto-def, references).
    pub(crate) mdkb_daemon: crate::mdkb_daemon::SharedMdkbDaemon,
    /// Shared async HTTP client for GitHub API requests.
    /// Built by [`build_http_client`] — always with timeouts, never
    /// `reqwest::Client::new()`.
    pub(crate) http_client: reqwest::Client,
    /// Everything GitHub: credentials, the poller, and per-account budgets.
    pub(crate) github: GitHubState,
    /// Shutdown sender for the HTTP server — send () to gracefully stop it.
    /// Only the TCP listener + TLS renewal task listen to this signal now;
    /// IPC listeners (Unix socket / named pipe) and the session reaper live
    /// for the whole app lifetime regardless of TCP restarts.
    pub(crate) server_shutdown: Mutex<Option<tokio::sync::oneshot::Sender<()>>>,
    /// One-shot guard: IPC listeners (Unix socket / named pipe), MCP session
    /// reaper, and upstream health checker are spawned the first time
    /// `start_server` runs and persist across restarts. Prevents a ~200ms
    /// socket-rebind window on `save_config` / TLS state changes that was
    /// tripping the MCP bridge's 3-failure / 9s health threshold and flipping
    /// it offline.
    pub(crate) ipc_started: std::sync::atomic::AtomicBool,
    /// Random session token for browser cookie auth — generated once when empty,
    /// then persisted in config and reused across server starts (not regenerated
    /// per start). Browsers auto-send cookies in fetch(), unlike stored Basic Auth.
    /// Behind RwLock so it can be regenerated at runtime (invalidating all sessions).
    pub(crate) session_token: parking_lot::RwLock<String>,
    pub(crate) auth_rate_limits:
        DashMap<std::net::IpAddr, Arc<crate::mcp_http::auth::AuthRateLimit>>,
    #[cfg(feature = "desktop")]
    pub(crate) app_handle: parking_lot::RwLock<Option<AppHandle>>,
    #[cfg(feature = "desktop")]
    pub(crate) design_mode: tokio::sync::OnceCell<crate::design_mode::manager::DesignModeManager>,
    /// Last time the desktop WebView's main JS thread proved it was running.
    /// Read by the diagnostics thread; see `frontend_liveness`.
    pub(crate) frontend_liveness: crate::frontend_liveness::FrontendLiveness,
    /// The last URL the main WebView was seen holding while healthy — the only
    /// address a recovery can aim at once the frame is on `about:`. Written by
    /// the webview-recovery thread; see `webview_recovery`.
    pub(crate) webview_boot_url: parking_lot::RwLock<Option<url::Url>>,
    /// Plugin filesystem watchers: watch_id → (plugin_id, watcher)
    pub plugin_watchers: DashMap<String, (String, notify::RecommendedWatcher)>,
    /// Current ANSI color overrides from the frontend theme (indices 0-15).
    /// Applied to new VtLogBuffers at creation time.
    pub(crate) ansi_colors: parking_lot::RwLock<Option<[[u8; 3]; 16]>>,
    /// Everything the renderer reads: the VT grids, the raw-byte flight
    /// recorder, and the per-session frame delivery channels and gates.
    pub(crate) grid: GridState,
    /// Incremental cache for Claude session transcript parsing.
    /// Loaded from disk on startup, persisted after each scan.
    ///
    /// NOT desktop-gated: `/claude/usage/timeline` is mounted unconditionally in
    /// `build_router`, so `tuic-remote` needs this field to compile at all.
    pub(crate) claude_usage_cache: Mutex<crate::claude_usage::SessionStatsCache>,
    /// Centralized application log ring buffer (1000 entries).
    /// Frontend pushes via push_log, reads via get_logs.
    /// Wrapped in Arc so the tracing subscriber layer can share the same buffer.
    pub(crate) log_buffer: Arc<Mutex<crate::app_logger::LogRingBuffer>>,
    /// Broadcast channel for all backend events (SSE, WebSocket, live consumers).
    /// Capacity 256 — lagged receivers get `RecvError::Lagged` and should reconnect.
    pub(crate) event_bus: tokio::sync::broadcast::Sender<AppEvent>,
    /// Idle lifetime advertised by a deployed remote daemon.
    pub(crate) remote_survive_secs: Option<u64>,
    pub(crate) remote_update: Option<crate::remote_update::RemoteUpdateState>,
    /// Open HTTP event streams. Unlike `event_bus.receiver_count()`, this does
    /// not include permanent backend subscribers such as repo watchers.
    pub(crate) sse_client_count: AtomicUsize,
    /// Advances whenever an SSE or terminal WebSocket client arrives. The
    /// lifetime timer observes this even when the client connects and leaves
    /// between two polling ticks.
    pub(crate) remote_client_generation: AtomicU64,
    /// Monotonic counter for SSE event IDs.
    pub(crate) event_counter: Arc<AtomicU64>,
    /// Live type filters of the open `/events` streams, so a browser that starts
    /// listening for a new event type widens its stream in place instead of
    /// reconnecting — a reconnect drops every event published between the close
    /// and the new subscription, and nothing replays them.
    pub(crate) sse_filters: crate::mcp_http::sse_routes::SseFilters,
    /// Per-repo BM25 content index for sub-millisecond file content search.
    /// Built in background on first search or repo load, rebuilt on `RepoChanged`.
    pub(crate) content_indices:
        DashMap<String, Arc<parking_lot::RwLock<crate::content_index::ContentIndex>>>,
    /// Cooperative CPU throttle for content-index builders. Search handlers
    /// acquire a guard here so indexers pause and yield priority to the user.
    pub(crate) indexer_throttle: Arc<crate::content_index::IndexerThrottle>,
    /// Repos whose content index build is currently in-flight (shared by
    /// `ensure_index` and `rebuild_index` to prevent duplicate concurrent builds).
    pub(crate) index_in_flight: Arc<DashSet<String>>,
    /// Global semaphore limiting concurrent index builds to 1. Prevents startup
    /// pre-warm from spawning N simultaneous BM25 builds that saturate the CPU.
    pub(crate) index_build_sem: Arc<tokio::sync::Semaphore>,
    /// Global semaphore bounding concurrent *monitoring* git subprocesses
    /// (repo summary/structure/diff-stats fan-out, poller batch). On a
    /// repo-changed burst these would otherwise spawn hundreds of git pipes at
    /// once → FD spikes (EMFILE) and CPU/IPC storms that stall the WebView.
    /// Operational (user-initiated) git is NEVER gated by this.
    pub(crate) monitoring_git_sem: Arc<tokio::sync::Semaphore>,
    /// Loaded plugin capabilities: plugin_id → list of capability strings.
    /// Populated by the frontend via `register_loaded_plugin` on plugin load.
    /// Used by Rust plugin commands to enforce capability checks server-side.
    pub(crate) loaded_plugins: DashMap<String, Vec<String>>,
    /// Compiled plugin OutputWatcher patterns, one set per connected frontend,
    /// pushed via `set_plugin_output_watchers`. The PTY reader assembles lines
    /// and matches them here, so the WebView main thread only hears about the
    /// lines that matched — and, while some pattern could not be compiled,
    /// about every line, which it then scans itself.
    ///
    /// Whether raw lines are needed lives inside the set, not beside it: two
    /// pieces of state published separately let a line slip through the window
    /// between them and be seen by neither matcher.
    pub(crate) plugin_output_watchers:
        parking_lot::RwLock<crate::output_watchers::OutputWatcherRegistry>,
    /// Cloud relay client state
    pub(crate) relay: RelayState,
    /// Registered peer agents for inter-agent messaging (tuic_session → PeerAgent)
    pub peer_agents: DashMap<String, PeerAgent>,
    /// Explicit opt-out for managed children; removed with the PTY.
    pub(crate) keep_open_sessions: DashSet<String>,
    /// Managed children whose last mail to their parent starts with BLOCKED and
    /// that nothing has mailed since. Kept outside the bounded inboxes so
    /// overflow cannot drop the hold; removed by the idle sweep with the PTY.
    pub(crate) blocked_children: DashSet<String>,
    /// Message inbox per agent (tuic_session → VecDeque<AgentMessage>).
    /// Capped at AGENT_INBOX_CAPACITY messages per agent. Matching lifecycle
    /// notices coalesce; other messages evict FIFO at capacity.
    pub agent_inbox: DashMap<String, VecDeque<AgentMessage>>,
    /// Unread eviction count per agent since last inbox read (tuic_session → count).
    /// Consumed and reset by the inbox action.
    pub(crate) agent_inbox_evictions: DashMap<String, u64>,
    /// Last read position per agent (tuic_session → logical unix-millis cursor).
    ///
    /// `since` used to be entirely the caller's problem, and a wait that timed out
    /// answered without a `next_since` — leaving `since=0` as the only recoverable
    /// value and replaying the whole inbox on the next call. The server remembers
    /// the position instead: an omitted `since` resumes from here, an explicit one
    /// overrides it, and `since=0` stays the deliberate replay escape hatch.
    pub(crate) agent_read_cursor: DashMap<String, u64>,
    /// Server notices, initial prompts and Compose commands waiting for a
    /// recipient's next safe idle window. Entries share one typed FIFO so
    /// delivery order is global. Peer `send` payloads are never in here — see
    /// `PendingInjection`. The inbox is the authoritative copy of every message.
    pub(crate) pending_injections: DashMap<String, VecDeque<PendingInjection>>,
    /// Last 128 accepted queue keys per PTY, including entries already drained.
    pub(crate) recent_queue_keys: DashMap<String, VecDeque<String>>,
    /// Initial prompts awaiting successful PTY submission. Successful delivery
    /// removes the marker; the delivery watchdog notifies the parent once and
    /// leaves the prompt in place so a child that was blocked on a startup
    /// dialog still receives it when it becomes ready.
    pub(crate) pending_initial_prompts: DashMap<String, PendingInitialPrompt>,
    /// Claude MCP children allowed to answer their one startup trust dialog.
    pub(crate) managed_trust_dialogs: DashSet<String>,
    /// Per-peer atomic handoff between blocking waiters and terminal delivery.
    /// Each message has exactly one wake-up owner while remaining visible in
    /// the authoritative inbox for backward-compatible reads.
    pub(crate) active_agent_waiters: DashMap<String, Mutex<AgentDeliveryGate>>,
    /// Peers that have successfully spawned at least one managed child during
    /// their current registration lifetime. Only these registered parents use
    /// inbox-only delivery while working and generic, coalesced wake notices
    /// while idle; ordinary managed agents retain direct message delivery.
    pub(crate) orchestrator_peers: DashSet<String>,
    /// Actual bound socket path (may differ from default if another instance holds mcp.sock).
    /// Updated by `start_server` after successful bind.
    #[cfg(unix)]
    pub(crate) bound_socket_path: parking_lot::RwLock<std::path::PathBuf>,
    /// Tailscale daemon state (detected at server startup)
    pub(crate) tailscale_state: parking_lot::RwLock<crate::tailscale::TailscaleState>,
    /// Push notification subscription store
    pub(crate) push_store: crate::push::PushStore,
    /// Live ACP connections to ego, one supervised child process each.
    ///
    /// Not keyed by PTY session and deliberately unrelated to one: an ACP
    /// connection is a JSON-RPC peer this host drives, and nothing about it
    /// belongs in terminal state. The executable it may launch is not held
    /// here — it is read from configuration at each connect, so a changed
    /// setting takes effect without a restart.
    pub(crate) acp: crate::acp::AcpClientManager,
    /// Mobile alert windows for ACP conversations, separate from PTY state.
    acp_push_last_ms: DashMap<String, Option<u64>>,
    /// When true, the desktop window is currently focused and the user is at
    /// their machine — suppress mobile push notifications to avoid duplicate
    /// alerts. Set to true on focus and at startup; set to false on blur or
    /// window minimize. Push notifications fire only when this is false.
    pub(crate) desktop_window_focused: std::sync::atomic::AtomicBool,
    /// Server start time for uptime calculation in health endpoint.
    pub(crate) server_start_time: std::time::Instant,
    /// TUIC's own AI agent: per-session knowledge, sandboxes, the watcher
    /// engine, the cron scheduler and the suggestion triggers.
    pub(crate) ai: AiAgentState,
    /// SSH tunnel manager — owns running tunnel supervisors.
    /// No outer Mutex needed: `TunnelManager` uses `DashMap` for interior mutability
    /// and all its methods take `&self`. Wrapping in `Mutex` would prevent holding
    /// a reference across the `start()` `.await` point.
    pub(crate) tunnel_manager: Arc<crate::tunnels::manager::TunnelManager>,
    /// SSH tunnel audit log — persisted event history for all tunnels.
    pub(crate) tunnel_audit: Arc<parking_lot::Mutex<crate::tunnels::audit::AuditLog>>,
    /// Live state of every remote connection: status, base URL, session token.
    /// `connections.json` says what is configured; this says what is up.
    pub(crate) remote: crate::remote_runtime::RemoteRuntime,
    /// Sessions running on connected remote machines, mirrored from their own
    /// `GET /sessions` and `/events` so they raise the same badges as local ones.
    pub(crate) remote_sessions: crate::remote_mirror::RemoteSessions,
    /// Duplex, authenticated agent mail links through this desktop hub.
    pub(crate) remote_mail: crate::mcp_http::remote_peer::RemoteMail,
    /// Task registry for long-running MCP orchestration. Survives client
    /// reconnects, so an orchestrator is not bound by the 300s wait ceiling.
    pub(crate) tasks: Arc<crate::tasks::TaskRegistry>,
    /// Serializes load-modify-save on `connections.json` to prevent TOCTOU races.
    pub(crate) connections_lock: tokio::sync::Mutex<()>,
    /// Pending screenshot requests: request_id → oneshot sender for base64 image data.
    /// Populated by MCP `ui(action=screenshot)`, consumed by `screenshot_response` command.
    pub(crate) screenshot_responses: DashMap<String, tokio::sync::oneshot::Sender<Option<String>>>,
    /// Pending suspend requests: request_id → oneshot sender for the tab's verdict
    /// (`Ok(())` suspended, `Err(reason)` refused). Populated by MCP `session action=suspend`,
    /// consumed by `session_suspend_response`.
    pub(crate) suspend_responses: DashMap<String, tokio::sync::oneshot::Sender<Result<(), String>>>,
    /// Pending confirmation requests: request_id → oneshot sender for the human's answer.
    /// Populated by MCP `ui(action=confirm)`, consumed by `mcp_confirm_response`.
    ///
    /// Every connected client — desktop WebView, browser, mobile PWA — is offered
    /// the same request and the first answer wins, because a remote human must be
    /// able to unblock an agent that a native desktop dialog would have pinned to
    /// whoever is sitting at the machine.
    pub(crate) confirm_responses: DashMap<String, tokio::sync::oneshot::Sender<bool>>,
    /// App-wide process-tree snapshot shared by agent lifecycle polling.
    pub(crate) process_snapshot_cache: crate::pty::ProcessSnapshotCache,
    /// Repos with active terminals — used to throttle watcher/polling for cold repos.
    pub(crate) hot_repo_paths: parking_lot::RwLock<std::collections::HashSet<String>>,
    /// Owns the fixture's data directory through normal drop and panic unwind.
    #[cfg(test)]
    _test_data_dir: Option<tempfile::TempDir>,
}

impl AppState {
    /// Clone a session's independently serialized PTY writer.
    ///
    /// The session mutex is held only long enough to clone the handle. Callers
    /// must release it before locking the writer so a blocked kernel write can
    /// never prevent the reader from queuing a mandatory terminal reply.
    pub(crate) fn pty_writer(&self, session_id: &str) -> Option<SharedPtyWriter> {
        self.session_maps
            .sessions
            .get(session_id)
            .map(|session| session.lock().writer.clone())
    }

    /// Write one atomic sequence of byte slices and flush it before another
    /// user-input or terminal-reply writer can interleave.
    pub(crate) fn write_pty_parts(&self, session_id: &str, parts: &[&[u8]]) -> Result<(), String> {
        let writer = self
            .pty_writer(session_id)
            .ok_or_else(|| "Session not found".to_string())?;
        let mut writer = writer.lock();
        if parts.iter().any(|part| !part.is_empty()) {
            self.retire_turn_interrupt(session_id);
        }
        for part in parts {
            writer
                .write_all(part)
                .map_err(|error| format!("Write failed: {error}"))?;
        }
        writer
            .flush()
            .map_err(|error| format!("Flush failed: {error}"))
    }

    /// Called only under the PTY writer lock, before any user input escapes.
    pub(crate) fn retire_turn_interrupt(&self, session_id: &str) {
        if let Some(mut session) = self.session_maps.session_states.get_mut(session_id) {
            session.turn_interrupt = None;
            session.turn_interrupt_retired_epoch = Some(session.turn_epoch);
        }
    }

    /// Emit a PTY-scoped lifecycle event to
    /// all three routes: the lossless state lane, per-session channel (so the session-scoped WS
    /// handlers receive it directly, without every other session's receiver
    /// cloning+filtering it), and the global `event_bus` (which still feeds
    /// `/events` SSE, relay, watcher, etc.).
    ///
    /// The per-session send is best-effort: it only fires when a WS handler has
    /// created the channel (via `subscribe`) — we never create it on the emit
    /// side, so sessions no client ever attaches to don't leak a channel. A send
    /// with no live receivers is a harmless no-op (`Err` dropped), exactly like
    /// the global bus with no subscribers.
    ///
    /// Non-PTY events (repo/dir/github/UI/...) must keep using `event_bus.send`
    /// directly; `pty_session_id()` returns `None` for them so they'd never reach
    /// a per-session channel anyway.
    pub(crate) fn emit_pty_event(&self, event: AppEvent) {
        // State is authoritative and sticky, so it gets a lossless lane. The
        // broadcast copies remain best-effort transports for live consumers.
        // Titles are presentation only; do not grow the lossless state lane
        // for an agent that animates its OSC title.
        if !matches!(&event, AppEvent::PtyTitle { .. }) {
            self.session_maps.session_state_events.send(event.clone());
        }
        if let Some(sid) = event.pty_session_id()
            && let Some(tx) = self.session_maps.pty_event_channels.get(sid)
        {
            let _ = tx.send(event.clone());
        }
        let _ = self.event_bus.send(event);
    }

    /// Tell every UI a terminal's chat view has entries past `seq`. Bus only
    /// for non-desktop clients, plus the window event: there is no bus->window
    /// forwarder, so the producer dual-emits.
    pub(crate) fn notify_chat_view_changed(&self, session_id: &str, seq: u64) {
        let _ = self.event_bus.send(AppEvent::ChatViewChanged {
            session_id: session_id.to_string(),
            seq,
        });
        #[cfg(feature = "desktop")]
        if let Some(app) = self.app_handle.read().as_ref() {
            let _ = app.emit(
                "chat-view-changed",
                serde_json::json!({ "session_id": session_id, "seq": seq }),
            );
        }
    }

    /// Rename a tab from the backend and tell every UI. Only for renames that
    /// start here (MCP `session action=rename`): IPC/HTTP renames come from the
    /// frontend, and emitting for them would echo every OSC title back. Returns
    /// false when the session does not exist.
    pub(crate) fn rename_session_from_backend(
        &self,
        session_id: &str,
        name: String,
        is_custom: bool,
    ) -> bool {
        let Some(entry) = self.session_maps.sessions.get(session_id) else {
            return false;
        };
        entry.lock().set_display_name(Some(name.clone()), is_custom);
        drop(entry);
        self.emit_pty_event(AppEvent::SessionRenamed {
            session_id: session_id.to_string(),
            name: name.clone(),
            is_custom,
        });
        #[cfg(feature = "desktop")]
        if let Some(app) = self.app_handle.read().as_ref() {
            let _ = app.emit(
                "session-renamed",
                serde_json::json!({ "session_id": session_id, "name": name, "is_custom": is_custom }),
            );
        }
        true
    }

    /// Ask the UI to suspend the tab that owns this session. Dual-emitted like a
    /// rename: Tauri listeners on desktop, the event bus for browser/SSE clients.
    /// The tab answers through `resolve_session_suspend` with `request_id`.
    pub(crate) fn request_session_suspend(&self, session_id: &str, request_id: &str) {
        self.emit_pty_event(AppEvent::SessionSuspendRequested {
            session_id: session_id.to_string(),
            request_id: request_id.to_string(),
        });
        #[cfg(feature = "desktop")]
        if let Some(app) = self.app_handle.read().as_ref() {
            let _ = app.emit(
                "session-suspend-requested",
                serde_json::json!({ "session_id": session_id, "request_id": request_id }),
            );
        }
    }

    /// Set or clear the orchestrator-owned description shown above a PTY.
    /// The event is dual-emitted for desktop Tauri listeners and browser/SSE
    /// clients, and is suppressed when the value did not change.
    pub(crate) fn set_pty_description(&self, session_id: &str, description: Option<String>) {
        let changed = match description.as_deref() {
            Some(value) if !value.is_empty() => {
                self.session_maps
                    .pty_descriptions
                    .insert(session_id.to_string(), value.to_string())
                    .as_deref()
                    != Some(value)
            }
            _ => self
                .session_maps
                .pty_descriptions
                .remove(session_id)
                .is_some(),
        };
        if !changed {
            return;
        }
        let description = self
            .session_maps
            .pty_descriptions
            .get(session_id)
            .map(|value| value.value().clone());
        self.emit_pty_event(AppEvent::PtyDescriptionChanged {
            session_id: session_id.to_string(),
            description: description.clone(),
        });
        #[cfg(feature = "desktop")]
        if let Some(app) = self.app_handle.read().as_ref() {
            let _ = app.emit(
                "pty-description-changed",
                serde_json::json!({
                    "session_id": session_id,
                    "description": description,
                }),
            );
        }
    }

    /// Subscribe to lifecycle events for one PTY session. Subscription happens
    /// before callers inspect current state, closing the check-then-sleep race:
    /// a transition after subscription is retained by the receiver, while a
    /// transition before it is visible in the initial state check.
    ///
    /// CALLER CONTRACT: validate `session_id` first. This CREATES the channel for
    /// whatever id it is given (entry API), and session teardown only reaps ids
    /// that were real sessions — so subscribing to an id that never existed
    /// leaves an entry nothing will remove.
    pub(crate) fn subscribe_pty_events(
        &self,
        session_id: &str,
    ) -> tokio::sync::broadcast::Receiver<AppEvent> {
        const CHANNEL_CAPACITY: usize = 256;
        self.session_maps
            .pty_event_channels
            .entry(session_id.to_string())
            .or_insert_with(|| tokio::sync::broadcast::channel(CHANNEL_CAPACITY).0)
            .subscribe()
    }

    /// Buffer a message into `recipient`'s bounded inbox. Replace an older
    /// lifecycle notice for the same child and kind, except question waits;
    /// otherwise evict FIFO at capacity.
    /// Mail from its parent ends a child's BLOCKED hold; the child's own BLOCKED mail to
    /// its parent starts one. Lifecycle notices are TUIC's, not either party's.
    fn track_blocked_hold(&self, recipient: &str, msg: &AgentMessage) {
        if msg.id.starts_with(LIFECYCLE_MSG_ID_PREFIX) {
            return;
        }
        if self
            .session_maps
            .session_parent
            .get(recipient)
            .is_some_and(|parent| parent.value() == &msg.from_tuic_session)
        {
            self.blocked_children.remove(recipient);
        }
        let to_parent = self
            .session_maps
            .session_parent
            .get(&msg.from_tuic_session)
            .is_some_and(|parent| parent.value() == recipient);
        if !to_parent {
            return;
        }
        if msg.content.trim_start().starts_with("BLOCKED") {
            self.blocked_children.insert(msg.from_tuic_session.clone());
        } else {
            self.blocked_children.remove(&msg.from_tuic_session);
        }
    }

    pub(crate) fn push_agent_inbox(&self, recipient: &str, msg: AgentMessage) -> u64 {
        self.track_blocked_hold(recipient, &msg);
        let timestamp = self.store_agent_inbox(recipient, msg);
        crate::mcp_http::remote_peer::notice_stored(self, recipient);
        timestamp
    }

    /// `push_agent_inbox` without touching BLOCKED holds, for replaying mail that
    /// was already seen once (identity handoff).
    pub(crate) fn store_agent_inbox(&self, recipient: &str, mut msg: AgentMessage) -> u64 {
        let gate_entry = self
            .active_agent_waiters
            .entry(recipient.to_string())
            .or_default();
        let mut gate = gate_entry.lock();
        let read_cursor = self
            .agent_read_cursor
            .get(recipient)
            .map(|entry| *entry.value());
        let (evicted, stored_timestamp) = {
            let mut inbox = self.agent_inbox.entry(recipient.to_string()).or_default();
            if let Some(last_timestamp) = inbox.back().map(|message| message.timestamp)
                && msg.timestamp <= last_timestamp
            {
                // Millisecond wall-clock timestamps can collide. Treat the serialized
                // timestamp as a per-recipient logical cursor while avoiding wrap at
                // the theoretical u64 ceiling.
                msg.timestamp = last_timestamp.saturating_add(1);
            }
            let replaced = lifecycle_notice_kind(&msg).and_then(|kind| {
                let index = inbox.iter().position(|message| {
                    message.from_tuic_session == msg.from_tuic_session
                        && lifecycle_notice_kind(message).as_deref() == Some(kind.as_str())
                })?;
                inbox
                    .remove(index)
                    .map(|message| (message.id, message.timestamp, false))
            });
            let evicted = replaced.or_else(|| {
                if inbox.len() < AGENT_INBOX_CAPACITY {
                    return None;
                }
                inbox.pop_front().map(|message| {
                    let observed = read_cursor.is_some_and(|cursor| message.timestamp <= cursor)
                        || gate.owners.get(&message.id)
                            == Some(&AgentDeliveryOwner::WaiterObserved);
                    (message.id, message.timestamp, !observed)
                })
            });
            let stored_timestamp = msg.timestamp;
            inbox.push_back(msg);
            (evicted, stored_timestamp)
        };
        if let Some((evicted_id, evicted_through, missed)) = evicted {
            if missed {
                *self
                    .agent_inbox_evictions
                    .entry(recipient.to_string())
                    .or_insert(0) += 1;
            }
            gate.owners.remove(&evicted_id);
            gate.urgent_notices
                .retain(|_, notice| notice.through > evicted_through);
        }
        gate.inbox_revision = gate.inbox_revision.wrapping_add(1);
        let revision = gate.inbox_revision;
        gate.inbox_events.send_replace(revision);
        stored_timestamp
    }

    /// Hear every change to one peer's inbox without waiting on it.
    ///
    /// Unlike [`Self::begin_agent_wait_with_events`] this takes no waiter
    /// lease, so it never becomes the owner that delivery assigns mail to: it
    /// only learns that the inbox moved.
    pub(crate) fn subscribe_agent_inbox(
        &self,
        tuic_session: &str,
    ) -> tokio::sync::watch::Receiver<u64> {
        self.active_agent_waiters
            .entry(tuic_session.to_string())
            .or_default()
            .lock()
            .inbox_events
            .subscribe()
    }

    #[cfg(test)]
    pub(crate) fn begin_agent_wait(&self, tuic_session: &str) -> u64 {
        self.begin_agent_wait_with_events(tuic_session).0
    }

    pub(crate) fn begin_agent_wait_with_events(
        &self,
        tuic_session: &str,
    ) -> (u64, tokio::sync::watch::Receiver<u64>) {
        let gate = self
            .active_agent_waiters
            .entry(tuic_session.to_string())
            .or_default();
        let mut gate = gate.lock();
        gate.next_lease = gate.next_lease.wrapping_add(1).max(1);
        let lease = gate.next_lease;
        gate.active_waiters.insert(lease);
        (lease, gate.inbox_events.subscribe())
    }

    pub(crate) fn finish_agent_wait(
        &self,
        tuic_session: &str,
        lease: u64,
        since: u64,
        observed: bool,
    ) -> AgentWaitFinish {
        let Some(gate) = self.active_agent_waiters.get(tuic_session) else {
            return AgentWaitFinish::default();
        };
        let mut gate = gate.lock();
        gate.active_waiters.remove(&lease);
        let last_waiter = gate.active_waiters.is_empty();
        let fresh_messages: Vec<AgentMessage> = self
            .agent_inbox
            .get(tuic_session)
            .map(|inbox| {
                inbox
                    .iter()
                    .filter(|message| message.timestamp > since)
                    .cloned()
                    .collect()
            })
            .unwrap_or_default();
        let fresh_ids: std::collections::HashSet<&str> = fresh_messages
            .iter()
            .map(|message| message.id.as_str())
            .collect();
        let mut finish = AgentWaitFinish::default();
        let mut observed_ids = std::collections::HashSet::new();
        for (message_id, owner) in &mut gate.owners {
            if !fresh_ids.contains(message_id.as_str()) || *owner != AgentDeliveryOwner::Waiter {
                continue;
            }
            if observed {
                *owner = AgentDeliveryOwner::WaiterObserved;
                observed_ids.insert(message_id.clone());
            } else if last_waiter {
                *owner = AgentDeliveryOwner::TerminalPending;
                finish.terminal_handoff.push(message_id.clone());
            }
        }
        finish.messages = fresh_messages
            .into_iter()
            .filter(|message| observed_ids.contains(&message.id))
            .collect();
        finish.fresh_count = finish.messages.len();
        if observed {
            let read_through = finish
                .messages
                .iter()
                .map(|message| message.timestamp)
                .max()
                .unwrap_or(since);
            gate.urgent_notices
                .retain(|_, notice| notice.through > read_through);
            gate.orchestrator_observed_through =
                gate.orchestrator_observed_through.max(read_through);
            if read_through == 0
                || gate
                    .orchestrator_wake_pending_through
                    .is_some_and(|pending| read_through >= pending)
            {
                gate.orchestrator_wake_pending_through = None;
            }
            if read_through == 0
                || gate
                    .orchestrator_wake_needed_through
                    .is_some_and(|needed| read_through >= needed)
            {
                gate.orchestrator_wake_needed_through = None;
            }
            gate.reset_orchestrator_wake_budget_if_observed();
        }
        finish
    }

    /// Reserve one unread urgent notice per sender and recipient. This shares
    /// the inbox delivery gate so reads and concurrent sends agree on the
    /// covered logical cursor.
    pub(crate) fn reserve_urgent_notice(
        &self,
        recipient: &str,
        sender: &str,
        through: u64,
    ) -> UrgentNoticeReservation {
        let gate_entry = self
            .active_agent_waiters
            .entry(recipient.to_string())
            .or_default();
        let mut gate = gate_entry.lock();
        if self
            .agent_read_cursor
            .get(recipient)
            .is_some_and(|cursor| *cursor >= through)
        {
            return UrgentNoticeReservation::AlreadyRead;
        }
        match gate.urgent_notices.get_mut(sender) {
            Some(notice) => {
                notice.through = notice.through.max(through);
                if notice.written {
                    UrgentNoticeReservation::Written
                } else {
                    UrgentNoticeReservation::InFlight
                }
            }
            None => {
                gate.urgent_notices.insert(
                    sender.to_string(),
                    UrgentNotice {
                        first_through: through,
                        through,
                        written: false,
                    },
                );
                UrgentNoticeReservation::Reserved
            }
        }
    }

    pub(crate) fn finish_urgent_notice(
        &self,
        recipient: &str,
        sender: &str,
        first_through: u64,
        written: bool,
    ) {
        let Some(gate_entry) = self.active_agent_waiters.get(recipient) else {
            return;
        };
        let mut gate = gate_entry.lock();
        if gate
            .urgent_notices
            .get(sender)
            .is_some_and(|notice| notice.first_through == first_through)
        {
            if written {
                gate.urgent_notices.get_mut(sender).unwrap().written = true;
            } else {
                gate.urgent_notices.remove(sender);
            }
        }
    }

    #[cfg(test)]
    pub(crate) fn has_active_agent_waiter(&self, tuic_session: &str) -> bool {
        self.active_agent_waiters
            .get(tuic_session)
            .is_some_and(|gate| !gate.lock().active_waiters.is_empty())
    }

    pub(crate) fn assign_agent_delivery(
        &self,
        tuic_session: &str,
        message_id: &str,
        terminal_delivery_available: bool,
    ) -> AgentDeliveryAssignment {
        let gate = self
            .active_agent_waiters
            .entry(tuic_session.to_string())
            .or_default();
        let mut gate = gate.lock();
        if let Some(owner) = gate.owners.get(message_id) {
            return match owner {
                AgentDeliveryOwner::Waiter | AgentDeliveryOwner::WaiterObserved => {
                    AgentDeliveryAssignment::Waiter
                }
                AgentDeliveryOwner::TerminalPending | AgentDeliveryOwner::TerminalDispatched => {
                    AgentDeliveryAssignment::Terminal
                }
            };
        }
        if !gate.active_waiters.is_empty() {
            gate.owners
                .insert(message_id.to_string(), AgentDeliveryOwner::Waiter);
            AgentDeliveryAssignment::Waiter
        } else if terminal_delivery_available {
            gate.owners
                .insert(message_id.to_string(), AgentDeliveryOwner::TerminalPending);
            AgentDeliveryAssignment::Terminal
        } else {
            AgentDeliveryAssignment::InboxOnly
        }
    }

    pub(crate) fn assign_agent_delivery_with_channel_attempt<F>(
        &self,
        tuic_session: &str,
        message_id: &str,
        terminal_fallback_available: bool,
        attempt_channel_delivery: F,
    ) -> (AgentDeliveryAssignment, bool)
    where
        F: FnOnce() -> bool,
    {
        let gate = self
            .active_agent_waiters
            .entry(tuic_session.to_string())
            .or_default();
        let mut gate = gate.lock();
        if let Some(owner) = gate.owners.get(message_id) {
            return match owner {
                AgentDeliveryOwner::Waiter | AgentDeliveryOwner::WaiterObserved => {
                    (AgentDeliveryAssignment::Waiter, false)
                }
                AgentDeliveryOwner::TerminalPending => (AgentDeliveryAssignment::Terminal, false),
                AgentDeliveryOwner::TerminalDispatched => (AgentDeliveryAssignment::Terminal, true),
            };
        }
        if !gate.active_waiters.is_empty() {
            gate.owners
                .insert(message_id.to_string(), AgentDeliveryOwner::Waiter);
            return (AgentDeliveryAssignment::Waiter, false);
        }
        if attempt_channel_delivery() {
            // An SSE channel push is a best-effort notification, not an inbox
            // observation. Leave the retained message unowned so a later wait
            // can claim and return it. Treating the push as terminal delivery
            // hid it from wait and let an unrelated lifecycle message advance
            // the implicit cursor past the unread mail.
            return (AgentDeliveryAssignment::Terminal, true);
        }
        if terminal_fallback_available {
            gate.owners
                .insert(message_id.to_string(), AgentDeliveryOwner::TerminalPending);
            return (AgentDeliveryAssignment::Terminal, false);
        }
        (AgentDeliveryAssignment::InboxOnly, false)
    }

    #[cfg(test)]
    pub(crate) fn assign_orchestrator_delivery_with_wake_attempt<F>(
        &self,
        tuic_session: &str,
        message_id: &str,
        message_timestamp: u64,
        wake_allowed: bool,
        attempt_wake: F,
    ) -> OrchestratorDeliveryAssignment
    where
        F: FnOnce() -> bool,
    {
        self.assign_orchestrator_delivery_with_wake_outcome(
            tuic_session,
            message_id,
            message_timestamp,
            wake_allowed,
            |_group| attempt_wake().into(),
        )
    }

    /// Assign delivery for a registered orchestrator without ever exposing the
    /// peer payload to its active turn or composer. An active waiter retains
    /// first ownership. Otherwise an authoritative idle/completed lifecycle may
    /// submit one generic wake plus at most one retry after an uncertain write.
    /// Later mail joins the same bounded group until an inbox read acknowledges
    /// the covered logical cursor.
    ///
    /// `attempt_wake` receives the reserved [`OrchestratorWakeGroup`] and may
    /// answer `SummarySubmitted` when it typed that whole window's payloads
    /// itself; see the settle arm below for what that buys and what it costs.
    pub(crate) fn assign_orchestrator_delivery_with_wake_outcome<F>(
        &self,
        tuic_session: &str,
        message_id: &str,
        message_timestamp: u64,
        wake_allowed: bool,
        attempt_wake: F,
    ) -> OrchestratorDeliveryAssignment
    where
        F: FnOnce(OrchestratorWakeGroup) -> OrchestratorWakeAttemptOutcome,
    {
        let (attempt, group) = {
            let gate_entry = self
                .active_agent_waiters
                .entry(tuic_session.to_string())
                .or_default();
            let mut gate = gate_entry.lock();
            if matches!(
                gate.owners.get(message_id),
                Some(AgentDeliveryOwner::Waiter | AgentDeliveryOwner::WaiterObserved)
            ) {
                return OrchestratorDeliveryAssignment::Waiter;
            }
            // A cancelled wait may have prepared an ordinary terminal handoff
            // before learning that this peer uses orchestrator routing. That
            // ownership cannot hide an inbox payload behind a generic notice.
            if matches!(
                gate.owners.get(message_id),
                Some(AgentDeliveryOwner::TerminalPending | AgentDeliveryOwner::TerminalDispatched)
            ) {
                gate.owners.remove(message_id);
            }
            if message_timestamp <= gate.orchestrator_observed_through {
                return OrchestratorDeliveryAssignment::InboxOnly;
            }
            if !gate.active_waiters.is_empty() {
                gate.owners
                    .insert(message_id.to_string(), AgentDeliveryOwner::Waiter);
                return OrchestratorDeliveryAssignment::Waiter;
            }

            let needed = gate
                .orchestrator_wake_needed_through
                .get_or_insert(message_timestamp);
            *needed = (*needed).max(message_timestamp);
            if let Some(pending_through) = gate.orchestrator_wake_pending_through.as_mut() {
                *pending_through = (*pending_through).max(message_timestamp);
                return OrchestratorDeliveryAssignment::WakeCoalesced;
            }
            if !wake_allowed {
                return OrchestratorDeliveryAssignment::InboxOnly;
            }
            if gate.orchestrator_wake_attempts_in_group >= ORCHESTRATOR_WAKE_ATTEMPT_LIMIT {
                return OrchestratorDeliveryAssignment::InboxOnly;
            }

            gate.orchestrator_wake_attempt = gate.orchestrator_wake_attempt.wrapping_add(1).max(1);
            gate.orchestrator_wake_attempts_in_group += 1;
            let attempt = gate.orchestrator_wake_attempt;
            // Reserve the logical notice before dropping the lock. Concurrent mail
            // coalesces into this cursor while terminal I/O happens without holding
            // either the delivery mutex or its DashMap guard.
            gate.orchestrator_wake_pending_through = gate.orchestrator_wake_needed_through;
            let group = OrchestratorWakeGroup {
                observed_through: gate.orchestrator_observed_through,
                wake_through: gate
                    .orchestrator_wake_needed_through
                    .unwrap_or(message_timestamp),
            };
            (attempt, group)
        };

        let outcome = attempt_wake(group);
        let Some(gate_entry) = self.active_agent_waiters.get(tuic_session) else {
            return OrchestratorDeliveryAssignment::InboxOnly;
        };
        let mut gate = gate_entry.lock();
        if gate.orchestrator_wake_attempt != attempt {
            return OrchestratorDeliveryAssignment::InboxOnly;
        }
        match outcome {
            OrchestratorWakeAttemptOutcome::Submitted => {
                gate.orchestrator_wake_needed_through = None;
                OrchestratorDeliveryAssignment::WakeSubmitted
            }
            OrchestratorWakeAttemptOutcome::SummarySubmitted => {
                // The payload is already on the recipient's screen, so this
                // notice acknowledges itself: advance the observed cursor by
                // exactly what an inbox read of the same window would have.
                //
                // Only `wake_through` is acknowledged, never `needed`. A generic
                // wake may clear `needed` wholesale because "go read your inbox"
                // covers messages that landed after the reservation; a summary
                // describes only what it printed, so mail that coalesced during
                // the write stays outstanding and must earn its own notice
                // (chased by the caller — see `route_registered_orchestrator_mail`).
                gate.orchestrator_observed_through =
                    gate.orchestrator_observed_through.max(group.wake_through);
                gate.orchestrator_wake_pending_through = None;
                if gate
                    .orchestrator_wake_needed_through
                    .is_some_and(|needed_through| needed_through <= group.wake_through)
                {
                    gate.orchestrator_wake_needed_through = None;
                }
                gate.reset_orchestrator_wake_budget_if_observed();
                OrchestratorDeliveryAssignment::WakeSummarySubmitted
            }
            OrchestratorWakeAttemptOutcome::NotStarted => {
                gate.orchestrator_wake_pending_through = None;
                // No PTY byte was written, so this is not an ambiguous delivery.
                // Leave the mail inbox-only instead of repeatedly reclaiming an
                // authoritative idle lifecycle that could not start a write.
                gate.orchestrator_wake_attempts_in_group = ORCHESTRATOR_WAKE_ATTEMPT_LIMIT;
                OrchestratorDeliveryAssignment::InboxOnly
            }
            OrchestratorWakeAttemptOutcome::Uncertain => {
                gate.orchestrator_wake_pending_through = None;
                OrchestratorDeliveryAssignment::InboxOnly
            }
        }
    }

    #[cfg(test)]
    pub(crate) fn acknowledge_orchestrator_wake(&self, tuic_session: &str, read_through: u64) {
        if let Some(gate) = self.active_agent_waiters.get(tuic_session) {
            let mut gate = gate.lock();
            // Zero is the cursor of an authoritative empty-inbox snapshot.
            // It acknowledges a stale notice even though there is no returned
            // message timestamp to compare with its covered cursor.
            if read_through == 0
                || gate
                    .orchestrator_wake_pending_through
                    .is_some_and(|pending_through| read_through >= pending_through)
            {
                gate.orchestrator_wake_pending_through = None;
            }
            if read_through == 0
                || gate
                    .orchestrator_wake_needed_through
                    .is_some_and(|needed_through| read_through >= needed_through)
            {
                gate.orchestrator_wake_needed_through = None;
            }
            gate.orchestrator_observed_through =
                gate.orchestrator_observed_through.max(read_through);
            gate.reset_orchestrator_wake_budget_if_observed();
        }
    }

    /// Snapshot an inbox and acknowledge exactly that snapshot under the same
    /// delivery gate used by wake assignment. A sender that buffered before this
    /// read but has not assigned delivery yet therefore observes the advanced
    /// cursor and cannot wake already-read mail.
    pub(crate) fn observe_agent_inbox(
        &self,
        tuic_session: &str,
        since: u64,
        limit: usize,
    ) -> (Vec<AgentMessage>, bool, u64) {
        let gate_entry = self
            .active_agent_waiters
            .entry(tuic_session.to_string())
            .or_default();
        let mut gate = gate_entry.lock();
        let mut messages: Vec<_> = self
            .agent_inbox
            .get(tuic_session)
            .map(|inbox| {
                inbox
                    .iter()
                    .filter(|message| message.timestamp > since)
                    .take(limit.saturating_add(1))
                    .cloned()
                    .collect()
            })
            .unwrap_or_default();
        let has_more = messages.len() > limit;
        if has_more {
            messages.pop();
        }
        let missed_count = self
            .agent_inbox_evictions
            .remove(tuic_session)
            .map(|(_, count)| count)
            .unwrap_or(0);
        let read_through = messages
            .iter()
            .map(|message| message.timestamp)
            .max()
            .unwrap_or(since);
        // Publish the read position while holding the delivery gate. A sender
        // cannot see a full inbox between this observation and cursor advancement.
        let mut cursor = self
            .agent_read_cursor
            .entry(tuic_session.to_string())
            .or_insert(0);
        if read_through > *cursor {
            *cursor = read_through;
        }
        // Reading is delivery, even when a terminal wake was queued first or a
        // sender has buffered mail but has not yet assigned its wake owner.
        for message in &messages {
            gate.owners
                .insert(message.id.clone(), AgentDeliveryOwner::WaiterObserved);
        }
        gate.urgent_notices
            .retain(|_, notice| notice.through > read_through);
        let no_pending_mail = self.agent_inbox.get(tuic_session).is_none_or(|inbox| {
            inbox.iter().all(|message| {
                gate.owners.get(&message.id) != Some(&AgentDeliveryOwner::TerminalPending)
            })
        });
        if no_pending_mail
            && let Some(pty_session) = self.live_pty_for_peer(tuic_session)
            && let Some(mut queue) = self.pending_injections.get_mut(&pty_session)
        {
            queue.retain(|entry| {
                !matches!(entry, PendingInjection::Notice { text, .. } if text == crate::pty::PEER_MAIL_WAKE)
            });
        }
        gate.orchestrator_observed_through = gate.orchestrator_observed_through.max(read_through);
        if read_through == 0
            || gate
                .orchestrator_wake_pending_through
                .is_some_and(|pending| read_through >= pending)
        {
            gate.orchestrator_wake_pending_through = None;
        }
        if read_through == 0
            || gate
                .orchestrator_wake_needed_through
                .is_some_and(|needed| read_through >= needed)
        {
            gate.orchestrator_wake_needed_through = None;
        }
        gate.reset_orchestrator_wake_budget_if_observed();
        (messages, has_more, missed_count)
    }

    /// Record one marker emission or one submitted turn for `session_id`.
    pub(crate) fn note_marker(&self, session_id: &str, kind: MarkerKind) {
        let mut stats = self
            .session_maps
            .marker_stats
            .entry(session_id.to_string())
            .or_default();
        let counter = match kind {
            MarkerKind::Intent => &mut stats.intent,
            MarkerKind::Suggest => &mut stats.suggest,
            MarkerKind::TurnSubmitted => &mut stats.turns,
        };
        *counter = counter.saturating_add(1);
    }

    /// Tallies for one session. Absent means "no turn observed yet", which is
    /// deliberately the same shape as all-zero: a session that never ran is not
    /// evidence of an agent ignoring anything.
    pub(crate) fn marker_stats_for(&self, session_id: &str) -> MarkerStats {
        self.session_maps
            .marker_stats
            .get(session_id)
            .map(|entry| *entry.value())
            .unwrap_or_default()
    }

    pub(crate) fn orchestrator_wake_needed_through(&self, tuic_session: &str) -> Option<u64> {
        self.active_agent_waiters
            .get(tuic_session)
            .and_then(|gate| {
                let gate = gate.lock();
                gate.orchestrator_wake_needed_through
                    .or(gate.orchestrator_wake_pending_through)
            })
    }

    /// Grant the outstanding wake group a fresh attempt budget because the
    /// recipient reached a new idle edge.
    ///
    /// A `NotStarted` attempt burns the whole budget on purpose: repeating it
    /// against an unchanged lifecycle writes no byte and costs a reclaim each
    /// time. But `agent_state` reading idle is not the same fact as
    /// `should_inject_now`, so the first attempt can fail on a composer draft,
    /// an open question or an unconfirmed idle — and the burn then outlives its
    /// cause. Without this, `reevaluate_orchestrator_mail_wake` is a no-op for
    /// the rest of the session and the mail is never announced at all, which is
    /// the one thing the idle-edge retry exists to prevent.
    ///
    /// A reservation in flight is never disturbed: a notice being typed still
    /// owns its attempt.
    pub(crate) fn rearm_orchestrator_wake_budget(&self, tuic_session: &str) {
        if let Some(gate) = self.active_agent_waiters.get(tuic_session) {
            let mut gate = gate.lock();
            if gate.orchestrator_wake_pending_through.is_none() {
                gate.orchestrator_wake_attempts_in_group = 0;
            }
        }
    }

    pub(crate) fn clear_orchestrator_delivery(&self, tuic_session: &str) {
        if let Some(gate) = self.active_agent_waiters.get(tuic_session) {
            let mut gate = gate.lock();
            gate.orchestrator_wake_attempt = gate.orchestrator_wake_attempt.wrapping_add(1).max(1);
            gate.orchestrator_wake_pending_through = None;
            gate.orchestrator_wake_needed_through = None;
            gate.orchestrator_wake_attempts_in_group = 0;
        }
    }

    pub(crate) fn waiter_fresh_message_count(&self, tuic_session: &str, since: u64) -> usize {
        let gate = self
            .active_agent_waiters
            .entry(tuic_session.to_string())
            .or_default();
        let mut gate = gate.lock();
        let fresh: Vec<String> = self
            .agent_inbox
            .get(tuic_session)
            .map(|inbox| {
                inbox
                    .iter()
                    .filter(|message| message.timestamp > since)
                    .map(|message| message.id.clone())
                    .collect()
            })
            .unwrap_or_default();
        let waiter_active = !gate.active_waiters.is_empty();
        fresh
            .into_iter()
            .filter(|message_id| match gate.owners.get(message_id).copied() {
                Some(
                    AgentDeliveryOwner::TerminalPending | AgentDeliveryOwner::TerminalDispatched,
                ) => false,
                Some(AgentDeliveryOwner::Waiter) => true,
                Some(AgentDeliveryOwner::WaiterObserved) => false,
                None if waiter_active => {
                    gate.owners
                        .insert(message_id.clone(), AgentDeliveryOwner::Waiter);
                    true
                }
                None => false,
            })
            .count()
    }

    pub(crate) fn mark_terminal_delivery_dispatched(&self, tuic_session: &str, message_id: &str) {
        if let Some(gate) = self.active_agent_waiters.get(tuic_session) {
            let mut gate = gate.lock();
            if gate.owners.get(message_id) == Some(&AgentDeliveryOwner::TerminalPending) {
                gate.owners.insert(
                    message_id.to_string(),
                    AgentDeliveryOwner::TerminalDispatched,
                );
            }
        }
    }

    /// The PTY that currently backs a peer identity, if any.
    ///
    /// Resolution, not equality — that distinction is the whole point. Two kinds of
    /// peer reach us: a spawn-registered child, whose identity the server stamped
    /// with the real PTY key, and a self-registering agent, which announces its
    /// `$TUIC_SESSION` and has no idea what key its PTY got. Checking
    /// `sessions.contains_key(peer_id)` only ever answered for the first kind, and
    /// answered "no PTY" for the second — which is how an orchestrator running in a
    /// TUIC tab ended up unreachable through its own terminal.
    ///
    /// Resolved per call rather than cached on the peer, so a respawn under the same
    /// `$TUIC_SESSION` is picked up without anyone re-registering.
    pub(crate) fn live_pty_for_peer(&self, peer_id: &str) -> Option<String> {
        if self.session_maps.sessions.contains_key(peer_id) {
            return Some(peer_id.to_string());
        }
        self.session_maps
            .live_pty_by_tuic_session
            .get(peer_id)
            .map(|entry| entry.value().clone())
            .filter(|session_id| self.session_maps.sessions.contains_key(session_id))
    }

    /// Whether a peer identity may be dropped when its MCP protocol session is
    /// reaped for inactivity.
    ///
    /// The protocol session and the identity have different lifetimes, and the
    /// reaper conflated them. Reaping is about a transport nobody has used for an
    /// hour. The identity is the address other agents send to, and
    /// `refresh_mcp_session` re-asserts it on the owner's next request — so an
    /// agent that spends a long turn thinking, without calling a TUIC tool, had
    /// its address deleted out from under it while it was still running.
    ///
    /// Two things keep an identity addressable:
    ///
    /// - it owns a live PTY. The agent is sitting right there mid-turn and will
    ///   call `agent action=send` when the turn ends.
    /// - a live session still records it as its parent. Dropping it strands that
    ///   child's handoff permanently: re-registering a headerless caller mints a
    ///   fresh UUID, and nothing tells the child what the new one is.
    ///
    /// Both conditions are bounded by live sessions, so retention cannot grow
    /// without bound — a reaped identity with neither is genuinely unreachable.
    pub(crate) fn peer_identity_is_reapable(&self, peer_id: &str) -> bool {
        self.live_pty_for_peer(peer_id).is_none()
            && !self
                .session_maps
                .session_parent
                .iter()
                .any(|entry| entry.value() == peer_id)
    }

    /// Record the PTY now backing a `$TUIC_SESSION`. Called at spawn.
    pub(crate) fn bind_live_pty(&self, tuic_session: &str, session_id: &str) {
        self.session_maps
            .live_pty_by_tuic_session
            .insert(tuic_session.to_string(), session_id.to_string());
    }

    /// Drop every `$TUIC_SESSION` pointing at a PTY that is going away, returning
    /// the identities that just lost their terminal so the caller can retire their
    /// peer registrations too.
    ///
    /// Scans rather than reverse-indexing: the map holds at most one entry per live
    /// session, so this is bounded by the session cap, and a reverse index would be
    /// one more thing to keep consistent for no measurable gain.
    pub(crate) fn unbind_live_pty(&self, session_id: &str) -> Vec<String> {
        let orphaned: Vec<String> = self
            .session_maps
            .live_pty_by_tuic_session
            .iter()
            .filter(|entry| entry.value() == session_id)
            .map(|entry| entry.key().clone())
            .collect();
        for identity in &orphaned {
            self.session_maps.live_pty_by_tuic_session.remove(identity);
        }
        orphaned
    }

    /// Drop the waiter leases of a bridge that a reconnect just replaced.
    ///
    /// Leases carry no owning MCP session, but on a reconnect the distinction is
    /// not needed: any wait registered before the rebind belongs to the connection
    /// that just went away, because the replacement bridge has not issued one yet.
    /// Leaving them would strand the peer — `assign_agent_delivery` gives any
    /// non-empty waiter set priority over terminal delivery, so a half-dead wait
    /// keeps winning and the freshly bound PTY is never woken until it times out.
    ///
    /// Messages that lease had merely claimed (`Waiter`) are released so the
    /// terminal can take them. `WaiterObserved` is kept: the old wait already
    /// returned those to the agent, and re-delivering them would duplicate.
    pub(crate) fn revoke_waiters_for_reconnect(&self, tuic_session: &str) {
        if let Some(gate) = self.active_agent_waiters.get(tuic_session) {
            let mut gate = gate.lock();
            gate.active_waiters.clear();
            gate.owners
                .retain(|_, owner| *owner != AgentDeliveryOwner::Waiter);
        }
    }

    pub(crate) fn release_terminal_delivery(&self, tuic_session: &str, message_id: &str) {
        if let Some(gate) = self.active_agent_waiters.get(tuic_session) {
            let mut gate = gate.lock();
            if gate.owners.get(message_id) == Some(&AgentDeliveryOwner::TerminalPending) {
                // The cursor may already have advanced past this message because a
                // newer waiter-owned message returned first. Put the same durable
                // mail at the end of the logical stream before releasing terminal
                // ownership, so an omitted-since wait can recover this failed PTY
                // delivery. The id stays stable for recipient-side deduplication.
                // Beyond the stored read cursor as well as the other entries: a
                // plain inbox poll may already have read this very message.
                let cursor = self
                    .agent_read_cursor
                    .get(tuic_session)
                    .map(|entry| *entry.value());
                let requeued = self
                    .agent_inbox
                    .get_mut(tuic_session)
                    .and_then(|mut inbox| {
                        let index = inbox.iter().position(|message| message.id == message_id)?;
                        let mut message = inbox.remove(index)?;
                        let after_newest = inbox.back().map(|m| m.timestamp.saturating_add(1));
                        let after_cursor = cursor.map(|c| c.saturating_add(1));
                        message.timestamp = [Some(message.timestamp), after_newest, after_cursor]
                            .into_iter()
                            .flatten()
                            .max()
                            .unwrap_or(message.timestamp);
                        inbox.push_back(message);
                        Some(())
                    })
                    .is_some();
                gate.owners.remove(message_id);
                if requeued {
                    gate.inbox_revision = gate.inbox_revision.wrapping_add(1);
                    let revision = gate.inbox_revision;
                    gate.inbox_events.send_replace(revision);
                } else {
                    // The inbox no longer holds it (evicted at capacity, or the
                    // recipient is gone): the failed delivery cannot be recovered.
                    tracing::warn!(
                        source = "agent",
                        recipient = %tuic_session,
                        message_id = %message_id,
                        "failed terminal mail could not be requeued; it is no longer in the inbox"
                    );
                }
            }
        }
    }

    #[cfg(test)]
    pub(crate) fn agent_delivery_owner(
        &self,
        tuic_session: &str,
        message_id: &str,
    ) -> Option<AgentDeliveryOwner> {
        self.active_agent_waiters
            .get(tuic_session)
            .and_then(|gate| gate.lock().owners.get(message_id).copied())
    }

    /// Acquire one slot in the monitoring-git concurrency limit. Hold the
    /// returned permit for the lifetime of the background git subprocess, then
    /// drop it. Operational (user-initiated) git must NOT call this.
    pub(crate) async fn monitoring_git_permit(&self) -> tokio::sync::OwnedSemaphorePermit {
        self.monitoring_git_sem
            .clone()
            .acquire_owned()
            .await
            .expect("monitoring_git_sem is never closed")
    }

    pub fn new(
        data_dir: PathBuf,
        worktrees_dir: PathBuf,
        config: crate::config::AppConfig,
        log_buffer: Arc<Mutex<crate::app_logger::LogRingBuffer>>,
    ) -> Self {
        let session_token = config.services.auth.session_token.clone();
        let push_store = crate::push::PushStore::load(&data_dir);
        let audit_path = data_dir.join("tunnel_audit.db");
        let tunnel_audit = Arc::new(parking_lot::Mutex::new(
            crate::tunnels::audit::AuditLog::open(&audit_path)
                .expect("Failed to open tunnel audit DB"),
        ));
        let tunnel_manager = Arc::new(crate::tunnels::manager::TunnelManager::new(
            tunnel_audit.clone(),
        ));
        Self {
            workflow_runtime: Default::default(),
            secrets: crate::secrets::SecretStore::default(),
            session_maps: SessionMaps::default(),
            data_dir,
            worktrees_dir,
            metrics: SessionMetrics::new(),
            mcp: McpState::default(),
            ws_clients: DashMap::new(),
            config: parking_lot::RwLock::new(config),
            git_cache: GitCacheState::new(),
            repo_watchers: DashMap::new(),
            repo_git_fingerprints: DashMap::new(),
            repo_head_targets: DashMap::new(),
            repo_head_emits_suppressed: AtomicU64::new(0),
            pending_orphan_cleanup: DashMap::new(),
            dir_watchers: DashMap::new(),
            theme_watcher: parking_lot::Mutex::new(None),
            subagent_map_cache: parking_lot::Mutex::new(Default::default()),
            chat_views: Default::default(),
            mdkb_daemon: crate::mdkb_daemon::create_shared_daemon(),
            http_client: build_http_client(),
            github: GitHubState::default(),
            server_shutdown: parking_lot::Mutex::new(None),
            ipc_started: std::sync::atomic::AtomicBool::new(false),
            session_token: parking_lot::RwLock::new(session_token),
            auth_rate_limits: DashMap::new(),
            #[cfg(feature = "desktop")]
            app_handle: parking_lot::RwLock::new(None),
            #[cfg(feature = "desktop")]
            design_mode: tokio::sync::OnceCell::new(),
            frontend_liveness: Default::default(),
            webview_boot_url: parking_lot::RwLock::new(None),
            plugin_watchers: DashMap::new(),
            ansi_colors: parking_lot::RwLock::new(None),
            grid: GridState::default(),
            claude_usage_cache: parking_lot::Mutex::new(crate::claude_usage::load_cache_from_disk()),
            log_buffer,
            event_bus: tokio::sync::broadcast::channel(256).0,
            remote_survive_secs: None,
            remote_update: None,
            sse_client_count: AtomicUsize::new(0),
            remote_client_generation: AtomicU64::new(0),
            event_counter: Arc::new(std::sync::atomic::AtomicU64::new(0)),
            sse_filters: Default::default(),
            content_indices: DashMap::new(),
            indexer_throttle: Arc::new(crate::content_index::IndexerThrottle::default()),
            index_in_flight: Arc::new(DashSet::new()),
            index_build_sem: Arc::new(tokio::sync::Semaphore::new(1)),
            monitoring_git_sem: Arc::new(tokio::sync::Semaphore::new(MONITORING_GIT_CONCURRENCY)),
            loaded_plugins: DashMap::new(),
            plugin_output_watchers: parking_lot::RwLock::new(Default::default()),
            relay: RelayState::new(),
            peer_agents: DashMap::new(),
            keep_open_sessions: DashSet::new(),
            blocked_children: DashSet::new(),
            agent_inbox: DashMap::new(),
            agent_inbox_evictions: DashMap::new(),
            agent_read_cursor: DashMap::new(),
            pending_injections: DashMap::new(),
            recent_queue_keys: DashMap::new(),
            pending_initial_prompts: DashMap::new(),
            managed_trust_dialogs: DashSet::new(),
            active_agent_waiters: DashMap::new(),
            orchestrator_peers: DashSet::new(),
            ai: AiAgentState::default(),
            #[cfg(unix)]
            bound_socket_path: parking_lot::RwLock::new(std::path::PathBuf::new()),
            tailscale_state: parking_lot::RwLock::new(
                crate::tailscale::TailscaleState::NotInstalled,
            ),
            acp: crate::acp::AcpClientManager::new(),
            acp_push_last_ms: DashMap::new(),
            push_store,
            desktop_window_focused: std::sync::atomic::AtomicBool::new(cfg!(feature = "desktop")),
            server_start_time: std::time::Instant::now(),
            tunnel_manager,
            remote: Default::default(),
            remote_sessions: Default::default(),
            remote_mail: Default::default(),
            tunnel_audit,
            tasks: Arc::new(crate::tasks::TaskRegistry::new()),
            connections_lock: tokio::sync::Mutex::new(()),
            screenshot_responses: DashMap::new(),
            suspend_responses: DashMap::new(),
            confirm_responses: DashMap::new(),
            process_snapshot_cache: crate::pty::ProcessSnapshotCache::default(),
            hot_repo_paths: parking_lot::RwLock::new(std::collections::HashSet::new()),
            #[cfg(test)]
            _test_data_dir: None,
        }
    }

    /// Wire event bus and MCP registry after construction.
    /// Must be called after wrapping in `Arc`.
    pub fn wire_event_bus(self: &Arc<Self>) {
        self.mcp
            .upstream_registry
            .set_event_bus(self.event_bus.clone());
        self.mcp
            .upstream_registry
            .set_mcp_tools_tx(self.mcp.tools_changed.clone());
        self.mcp
            .upstream_registry
            .set_oauth_flow_manager(self.mcp.oauth_flow_manager.clone());
    }

    /// Assign a human-friendly alias based on the repo/cwd name.
    /// E.g. `tuicommander` → `tu-1`, `night-recovery` → `nr-1`, no repo → `sh-1`.
    ///
    /// If the same repo name already has a prefix in `term_alias_counters`, reuse it.
    /// Collision resolution only runs for new repo names whose base acronym clashes.
    ///
    /// `requested` is the alias a restored tab was saved with. It is honoured
    /// verbatim and the prefix counter is moved past its number, so the next fresh
    /// tab in that repo cannot be handed the same address. A malformed request, or
    /// one a live session already holds, is dropped — see
    /// [`reservable_alias`]. The alias is a routable address, so an untrusted
    /// value must never be able to point two tabs at one name.
    pub(crate) fn assign_term_alias(&self, session_id: &str, requested: Option<&str>) -> String {
        if let Some((prefix, number)) = requested.and_then(|alias| self.reservable_alias(alias)) {
            let alias = format!("{prefix}-{number}");
            let mut counter = self
                .session_maps
                .term_alias_counters
                .entry(prefix)
                .or_insert(0);
            *counter = (*counter).max(number);
            drop(counter);
            self.record_term_alias(session_id, alias.clone());
            return alias;
        }
        let cwd = self
            .session_maps
            .sessions
            .get(session_id)
            .and_then(|s| s.lock().cwd.clone());
        let repo_name = cwd
            .as_deref()
            .and_then(|p| std::path::Path::new(p).file_name())
            .and_then(|n| n.to_str())
            .unwrap_or("")
            .to_string();

        // Check if we already have a prefix for this exact repo name
        let prefix = if let Some(existing) = self.find_prefix_for_repo(&repo_name) {
            existing
        } else {
            repo_name_to_prefix(&repo_name, &self.session_maps.term_aliases)
        };

        let mut counter = self
            .session_maps
            .term_alias_counters
            .entry(prefix.clone())
            .or_insert(0);
        *counter += 1;
        let alias = format!("{prefix}-{}", *counter);
        drop(counter);
        self.record_term_alias(session_id, alias.clone());
        alias
    }

    /// Split a requested alias into `(prefix, number)` when it is a shape this
    /// map can address and nothing live already answers to it.
    ///
    /// `prefix` must be non-empty and carry no `-` (the separator the resolver
    /// splits on) and the number must be a positive `u32`, because that is
    /// exactly what [`assign_term_alias`] generates and what the counter counts.
    fn reservable_alias(&self, alias: &str) -> Option<(String, u32)> {
        let (prefix, number) = alias.rsplit_once('-')?;
        if prefix.is_empty() || prefix.contains('-') {
            return None;
        }
        let number: u32 = number.parse().ok()?;
        if number == 0 {
            return None;
        }
        if self.resolve_alias(alias).is_some() {
            return None;
        }
        Some((prefix.to_string(), number))
    }

    /// File an alias against a session and tell the UI, so the tab shows the
    /// address other agents can reach it by. Dual-emitted: the bus carries it
    /// to browser/PWA clients over `/events`, the window emit to the desktop.
    fn record_term_alias(&self, session_id: &str, alias: String) {
        self.session_maps
            .term_aliases
            .insert(session_id.to_string(), alias.clone());
        self.emit_pty_event(AppEvent::TermAliasAssigned {
            session_id: session_id.to_string(),
            alias: alias.clone(),
        });
        #[cfg(feature = "desktop")]
        if let Some(ref app) = *self.app_handle.read() {
            let _ = app.emit(
                "term-alias-assigned",
                serde_json::json!({
                    "session_id": session_id,
                    "alias": alias,
                }),
            );
        }
    }

    /// Find an existing prefix used by a session with the same repo name.
    fn find_prefix_for_repo(&self, repo_name: &str) -> Option<String> {
        for entry in self.session_maps.term_aliases.iter() {
            let sid = entry.key();
            let alias = entry.value();
            let other_cwd = self
                .session_maps
                .sessions
                .get(sid.as_str())
                .and_then(|s| s.lock().cwd.clone());
            let other_name = other_cwd
                .as_deref()
                .and_then(|p| std::path::Path::new(p).file_name())
                .and_then(|n| n.to_str())
                .unwrap_or("");
            if other_name == repo_name
                && let Some(dash) = alias.rfind('-')
            {
                return Some(alias[..dash].to_string());
            }
        }
        None
    }

    /// Look up session_id by alias (e.g. "tc-1" → UUID).
    pub(crate) fn resolve_alias(&self, alias: &str) -> Option<String> {
        self.session_maps
            .term_aliases
            .iter()
            .find(|e| e.value() == alias)
            .map(|e| e.key().clone())
    }

    /// Resolve any address a caller may hold for a terminal into the live PTY key.
    ///
    /// Three names reach the same session and callers do not get to know which one
    /// they were handed: the PTY key the server minted, the `$TUIC_SESSION` a
    /// desktop tab persists across restarts, and the short repo alias the tab menu
    /// shows. Every tool that takes a `session_id` goes through here, so "notify
    /// tu-1" works without a UUID lookup.
    ///
    /// `None` means no live session answers to that name — never a guess.
    ///
    /// Exact addresses win. A short UUID prefix and a display name are accepted
    /// only when they identify exactly one live session.
    pub(crate) fn resolve_session_ref_checked(
        &self,
        reference: &str,
    ) -> Result<Option<String>, String> {
        let exact = self
            .session_maps
            .sessions
            .contains_key(reference)
            .then(|| reference.to_string())
            .or_else(|| self.live_pty_for_peer(reference))
            .or_else(|| self.resolve_alias(reference));
        if let Some(session_id) = exact.filter(|id| self.session_maps.sessions.contains_key(id)) {
            return Ok(Some(session_id));
        }

        let matches: Vec<String> = self
            .session_maps
            .sessions
            .iter()
            .filter_map(|entry| {
                let session_id = entry.key();
                let session = entry.value().lock();
                (session_id.starts_with(reference)
                    || session.display_name.as_deref() == Some(reference))
                .then(|| session_id.clone())
            })
            .collect();
        match matches.as_slice() {
            [] => Ok(None),
            [session_id] => Ok(Some(session_id.clone())),
            _ => Err(format!(
                "Session reference '{reference}' is ambiguous; matches {}",
                matches.join(", ")
            )),
        }
    }

    /// Resolve any address into the key a peer's mail is filed under.
    ///
    /// [`resolve_session_ref_checked`] travels towards the terminal; mail travels the other
    /// way, because `peer_agents` is keyed by `tuic_session`. An alias or a PTY key
    /// therefore has to be walked back to the peer that owns that terminal.
    /// Resolve a peer address while retaining an ambiguity error from the
    /// terminal address resolver.
    pub(crate) fn resolve_peer_ref_checked(
        &self,
        reference: &str,
    ) -> Result<Option<String>, String> {
        if reference.is_empty() {
            return Ok(None);
        }
        if self.peer_agents.contains_key(reference) {
            return Ok(Some(reference.to_string()));
        }
        // Terminal addresses win over the register name: a peer must not be able
        // to capture another terminal's alias, PTY key or display name by
        // registering it as its name.
        if let Some(session_id) = self.resolve_session_ref_checked(reference)?
            && let Some(peer) = self.peer_agents.iter().find(|entry| {
                self.live_pty_for_peer(entry.key()).as_deref() == Some(session_id.as_str())
            })
        {
            return Ok(Some(peer.key().clone()));
        }
        self.resolve_peer_name(reference)
    }

    /// The register `name` list_peers prints, as an address. Live peers (a live
    /// PTY or a known MCP session) shadow dead ones, so a restart that reuses
    /// the default name is not made ambiguous by its dead predecessor; with no
    /// live peer the dead ones still answer. Identities of one PTY are one
    /// owner. Two owners with one name are refused with the candidates.
    fn resolve_peer_name(&self, name: &str) -> Result<Option<String>, String> {
        let mut live = std::collections::BTreeSet::new();
        let mut dead = std::collections::BTreeSet::new();
        for entry in self.peer_agents.iter().filter(|e| e.value().name == name) {
            let key = entry.key();
            if self.live_pty_for_peer(key).is_some() {
                live.extend(self.peer_identity_for_live_pty(key));
            } else if self
                .mcp
                .sessions
                .contains_key(&entry.value().mcp_session_id)
            {
                live.insert(key.clone());
            } else {
                dead.insert(key.clone());
            }
        }
        let owners: Vec<String> = if live.is_empty() { dead } else { live }
            .into_iter()
            .collect();
        match owners.as_slice() {
            [] => Ok(None),
            [owner] => Ok(Some(owner.clone())),
            _ => Err(format!(
                "Peer name '{name}' is ambiguous; matches {}",
                owners.join(", ")
            )),
        }
    }

    /// A managed PTY has one mailbox even if two bridges assert different UUIDs
    /// for it (for example, a persisted tab UUID and the PTY key). Choose the
    /// first registered peer so the mailbox address stays stable on reconnect.
    /// Callers that create a binding hold PEER_IDENTITY_BIND_LOCK while using it.
    pub(crate) fn peer_identity_for_live_pty(&self, asserted: &str) -> Option<String> {
        let pty = self.live_pty_for_peer(asserted)?;
        self.peer_agents
            .iter()
            .filter(|peer| self.live_pty_for_peer(peer.key()).as_deref() == Some(pty.as_str()))
            .map(|peer| (peer.registered_at, peer.key().clone()))
            .min()
            .map(|(_, identity)| identity)
    }

    /// This session's knowledge record, read off disk when it is not resident.
    ///
    /// The startup load is capped at `MAX_RESIDENT_SESSIONS`, so a session with
    /// a file on disk is not necessarily in memory. Starting a blank record for
    /// it is how resuming an older session erased its own history: the next
    /// flush persists what is in memory, over a file nothing had read.
    pub(crate) fn knowledge_entry(
        &self,
        session_id: &str,
    ) -> dashmap::mapref::one::RefMut<'_, String, Mutex<crate::ai_agent::knowledge::SessionKnowledge>>
    {
        self.ai
            .session_knowledge
            .entry(session_id.to_string())
            .or_insert_with(|| {
                Mutex::new(crate::ai_agent::knowledge::load_or_start_fresh(session_id))
            })
    }

    /// Record a command outcome into this session's knowledge store and mark it
    /// dirty for the background persister. Creates the entry on first use.
    pub(crate) fn record_outcome(
        &self,
        session_id: &str,
        outcome: crate::ai_agent::knowledge::CommandOutcome,
    ) -> u64 {
        let id = self.knowledge_entry(session_id).lock().record(outcome);
        self.ai.knowledge_dirty.insert(session_id.to_string(), ());
        id
    }
}

/// Derive a short prefix from a repo directory name.
///
/// Split by `-`, `_`, `.`, and camelCase boundaries, then take initials.
/// - Multi-word: `tuiCommander` → `tc` (camel split), `night-recovery` → `nr`
/// - Single word < 3 chars: use as-is (`go` → `go`)
/// - Single word >= 3 chars: first 2 letters (`server` → `se`)
/// - Empty/no repo: `sh`
///
/// If the result collides with an existing alias prefix, progressively add
/// letters from the longest segment until unique (max 5 chars).
fn repo_name_to_prefix(name: &str, existing: &DashMap<String, String>) -> String {
    if name.is_empty() {
        return "sh".to_string();
    }

    let segments = split_name_segments(name);
    let base = match segments.as_slice() {
        // split_name_segments never returns empty (it pushes "sh"), but guard
        // defensively so a future change can't reintroduce an OOB index.
        [] => "sh".to_string(),
        [single] => {
            if single.chars().count() < 3 {
                single.clone()
            } else {
                single.chars().take(2).collect()
            }
        }
        // Multi-word: take the first char of each segment. chars() (not byte
        // slicing) so multibyte directory names never panic mid-codepoint.
        multi => multi.iter().filter_map(|s| s.chars().next()).collect(),
    };

    let used_prefixes: std::collections::HashSet<String> = existing
        .iter()
        .filter_map(|e| {
            let v = e.value();
            v.rfind('-').map(|i| v[..i].to_string())
        })
        .collect();

    if !used_prefixes.contains(&base) {
        return base;
    }

    // Collision: extend with letters from the longest segment
    let longest = segments.iter().max_by_key(|s| s.len()).unwrap();
    for len in (base.len() + 1)..=5.min(longest.len() + base.len()) {
        let extended = if segments.len() >= 2 {
            let mut ext = base.clone();
            let extra_needed = len - base.len();
            let chars_available: Vec<char> = longest.chars().skip(1).collect();
            for c in chars_available.iter().take(extra_needed) {
                ext.push(*c);
            }
            ext
        } else {
            name.chars().take(len).collect()
        };
        if !used_prefixes.contains(&extended) {
            return extended;
        }
    }

    // Last resort: append digit to base
    for i in 2..=9 {
        let fallback = format!("{base}{i}");
        if !used_prefixes.contains(&fallback) {
            return fallback;
        }
    }
    base
}

/// Split a name into segments by `-`, `_`, `.`, and camelCase transitions.
fn split_name_segments(name: &str) -> Vec<String> {
    let mut segments = Vec::new();
    let mut current = String::new();

    for c in name.chars() {
        if c == '-' || c == '_' || c == '.' {
            if !current.is_empty() {
                segments.push(current.to_lowercase());
                current.clear();
            }
        } else if c.is_uppercase() && !current.is_empty() {
            segments.push(current.to_lowercase());
            current.clear();
            current.push(c);
        } else {
            current.push(c);
        }
    }
    if !current.is_empty() {
        segments.push(current.to_lowercase());
    }
    if segments.is_empty() {
        segments.push("sh".to_string());
    }
    segments
}

/// Cloud relay client state (connection + shutdown handle).
pub(crate) struct RelayState {
    /// Shutdown sender — send () to gracefully stop the relay supervisor
    pub(crate) shutdown: Mutex<Option<tokio::sync::oneshot::Sender<()>>>,
    /// Whether the relay client is currently connected
    pub(crate) connected: std::sync::atomic::AtomicBool,
    /// Bumped after every committed config change so the relay supervisor
    /// re-reads its settings without an app restart. A `watch` rather than a
    /// broadcast: a burst of saves needs one re-read, not one per save, and a
    /// subscriber that was busy cannot lag out of a coalescing channel.
    pub(crate) config_revision: tokio::sync::watch::Sender<u64>,
}

impl RelayState {
    pub(crate) fn new() -> Self {
        Self {
            shutdown: parking_lot::Mutex::new(None),
            connected: std::sync::atomic::AtomicBool::new(false),
            config_revision: tokio::sync::watch::Sender::new(0),
        }
    }
}

/// A TTL + bounded + coalescing cache keyed by repo path.
///
/// `moka::sync::Cache` is used (not `future::Cache`) because every loader is
/// blocking work (git subprocess / in-process gix). `get_with`/`try_get_with`
/// coalesce concurrent identical loads to a single computation — this is what
/// collapses the `repo-changed` fan-out. Values are wrapped in `Arc` so cache
/// hits and the coalesced result are cheap to share.
pub(crate) type GitCache<T> = moka::sync::Cache<String, Arc<T>>;

/// Max entries per git cache. Repo count is small; this is a safety bound.
const GIT_CACHE_CAPACITY: u64 = 256;

/// Build a git cache with the standard capacity and the given TTL.
///
/// `ttl_fallbacks` counts entries evicted by TTL expiry (`RemovalCause::Expired`).
/// This is NOT a reliable "watcher missed an event" signal: an idle repo with no
/// file changes produces no watcher event, so its entry is never invalidated and
/// always lives out the full TTL before expiring — the normal, correct end of a
/// cached entry's life. Because expiry is keyed on write time, a single fan-out
/// load of N repos makes all N entries expire together one TTL later. The counter
/// is therefore dominated by benign idle expiry; it's surfaced in the watchdog
/// snapshot only as a coarse aggregate eviction trend, never logged per entry
/// (that produced one DEBUG line per repo every TTL — pure noise).
pub(crate) fn build_git_cache<T: Send + Sync + 'static>(
    ttl: Duration,
    ttl_fallbacks: Arc<AtomicU64>,
) -> GitCache<T> {
    moka::sync::Cache::builder()
        .max_capacity(GIT_CACHE_CAPACITY)
        .time_to_live(ttl)
        .eviction_listener(move |_key, _v, cause| {
            if cause == moka::notification::RemovalCause::Expired {
                ttl_fallbacks.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
            }
        })
        .build()
}

/// A full review-thread walk of one PR, valid for the PR state it was taken at.
#[derive(Clone)]
pub(crate) struct SettledReviewThreads {
    pub(crate) updated_at: String,
    pub(crate) head_ref_oid: String,
    pub(crate) unresolved: u32,
    pub(crate) complete: bool,
    /// The walk failed: the entry only stops a re-walk on every poll, it carries no count.
    pub(crate) failed: bool,
    pub(crate) walked_at: Instant,
}

/// TTL caches for git and GitHub query results, keyed by repo path.
pub(crate) struct GitCacheState {
    pub(crate) repo_info: GitCache<crate::git::RepoInfo>,
    pub(crate) merged_branches: GitCache<Vec<String>>,
    pub(crate) repo_diff_stats: GitCache<crate::git::RepoDiffStats>,
    pub(crate) branches_detail: GitCache<Vec<crate::git::BranchDetail>>,
    pub(crate) github_status: GitCache<Vec<crate::github::BranchPrStatus>>,
    pub(crate) git_status: GitCache<crate::github::GitHubStatus>,
    pub(crate) git_panel_context: GitCache<crate::git::GitPanelContext>,
    pub(crate) worktree_paths:
        GitCache<std::collections::HashMap<String, crate::worktree::WorkspaceWorktree>>,
    /// Repos that returned null from GitHub GraphQL (not found / no access).
    /// Keyed by "owner/name", value is the cooldown expiry time.
    /// Excluded from batch queries until the cooldown expires (1 hour).
    /// NOT a TTL value cache — kept as a plain `DashMap` set with custom expiry.
    pub(crate) github_repo_cooldown: DashMap<String, Instant>,
    /// Settled review-thread totals of PRs with more threads than the batch poll reads, keyed by
    /// "host/owner/name#number". Not a TTL cache: an entry is valid while the PR's `updatedAt` and head
    /// are unchanged and it is younger than `SETTLED_THREADS_TTL`.
    pub(crate) settled_review_threads: DashMap<String, SettledReviewThreads>,
    /// Count of entries evicted by TTL expiry (watcher-miss observability).
    /// Shared across all git caches; surfaced in the cpu_watchdog snapshot.
    pub(crate) ttl_fallbacks: Arc<AtomicU64>,
}

impl GitCacheState {
    pub(crate) fn new() -> Self {
        let ttl_fallbacks = Arc::new(AtomicU64::new(0));
        Self {
            repo_info: build_git_cache(GIT_CACHE_TTL, Arc::clone(&ttl_fallbacks)),
            merged_branches: build_git_cache(GIT_CACHE_TTL, Arc::clone(&ttl_fallbacks)),
            repo_diff_stats: build_git_cache(GIT_CACHE_TTL, Arc::clone(&ttl_fallbacks)),
            branches_detail: build_git_cache(GIT_CACHE_TTL, Arc::clone(&ttl_fallbacks)),
            github_status: build_git_cache(GITHUB_CACHE_TTL, Arc::clone(&ttl_fallbacks)),
            git_status: build_git_cache(GIT_CACHE_TTL, Arc::clone(&ttl_fallbacks)),
            git_panel_context: build_git_cache(GIT_CACHE_TTL, Arc::clone(&ttl_fallbacks)),
            worktree_paths: build_git_cache(GIT_CACHE_TTL, Arc::clone(&ttl_fallbacks)),
            github_repo_cooldown: DashMap::new(),
            settled_review_threads: DashMap::new(),
            ttl_fallbacks,
        }
    }

    /// Invalidate all caches.
    /// Note: `github_repo_cooldown` is intentionally NOT cleared here — cooldowns
    /// must survive cache invalidation to prevent repeated queries for repos that
    /// don't exist on GitHub.  Only explicit user actions (OAuth login, full reset)
    /// should clear cooldowns.
    pub(crate) fn clear_all(&self) {
        self.repo_info.invalidate_all();
        self.merged_branches.invalidate_all();
        self.repo_diff_stats.invalidate_all();
        self.branches_detail.invalidate_all();
        self.github_status.invalidate_all();
        self.git_status.invalidate_all();
        self.git_panel_context.invalidate_all();
        self.worktree_paths.invalidate_all();
    }

    /// Invalidate caches for a specific repo path.
    pub(crate) fn invalidate_repo(&self, path: &str) {
        self.repo_info.invalidate(path);
        self.merged_branches.invalidate(path);
        self.repo_diff_stats.invalidate(path);
        self.branches_detail.invalidate(path);
        // github_status (remote PR/CI data) is NOT invalidated here — local git
        // changes don't affect remote PRs. The poller and head-changed → pollRepo
        // handle remote refreshes on their own cadence.
        self.git_status.invalidate(path);
        self.git_panel_context.invalidate(path);
        self.worktree_paths.invalidate(path);
    }
}

/// Remove dead (closed-receiver) WebSocket senders for a session.
///
/// Called on WS close so that disconnected clients don't accumulate
/// on idle PTY sessions that produce no output (which would otherwise
/// be the only trigger for retain-based cleanup).
pub(crate) fn purge_dead_ws_clients(
    ws_clients: &DashMap<String, Vec<WsClientTx>>,
    session_id: &str,
) {
    if let Some(mut clients) = ws_clients.get_mut(session_id) {
        clients.retain(|tx| !tx.is_closed());
        let emptied = clients.is_empty();
        drop(clients);
        if emptied {
            reap_empty_ws_entry(ws_clients, session_id);
        }
    }
}

/// Drop the map entry once its last client is gone.
///
/// An empty `Vec` left behind is not free: it makes the reader's lookup succeed,
/// so every chunk is copied for a session nobody is watching. Separate fn
/// because the `RefMut` must be dropped before touching the same map again.
fn reap_empty_ws_entry(ws_clients: &DashMap<String, Vec<WsClientTx>>, session_id: &str) {
    ws_clients.remove_if(session_id, |_, clients| clients.is_empty());
}

/// How many output chunks may be queued for one raw-output WebSocket client.
///
/// The raw lane was the only one of the three stream types without a bound: the
/// log lane paces itself on the sink it awaits, the grid lane is a `watch` that
/// keeps one frame, and this one grew for as long as a stalled browser stayed
/// connected. 256 matches the broadcast lanes.
pub(crate) const WS_CLIENT_QUEUE_CAPACITY: usize = 256;

pub(crate) type WsClientTx = tokio::sync::mpsc::Sender<String>;

/// A bounded queue for one raw-output WebSocket client.
pub(crate) fn new_ws_client_channel() -> (WsClientTx, tokio::sync::mpsc::Receiver<String>) {
    tokio::sync::mpsc::channel(WS_CLIENT_QUEUE_CAPACITY)
}

/// Fan one chunk out to a session's WebSocket clients.
///
/// The single place that copies PTY output for browser clients — both the
/// streaming path and the EOF flush go through it. It allocates only when
/// somebody is actually listening, and reaps the entry when the last client
/// leaves, so a browser that connected once stops costing the reader a copy of
/// every chunk for the rest of the session's life.
///
/// Delivery is `try_send`, never `send`. The caller holds the output ring lock,
/// which the PTY reader and every connecting client also take — awaiting a slow
/// socket there would stall the session itself. A client whose queue is full has
/// stopped draining, so it is dropped; reconnecting replays from the ring, which
/// is the mechanism that exists for exactly this.
pub(crate) fn broadcast_to_ws_clients(
    ws_clients: &DashMap<String, Vec<WsClientTx>>,
    session_id: &str,
    data: &str,
) {
    let Some(mut clients) = ws_clients.get_mut(session_id) else {
        return;
    };
    if !clients.is_empty() {
        let owned = data.to_owned();
        clients.retain(|tx| tx.try_send(owned.clone()).is_ok());
    }
    let emptied = clients.is_empty();
    drop(clients);
    if emptied {
        reap_empty_ws_entry(ws_clients, session_id);
    }
}

const MOBILE_PUSH_HID_IDLE_SECS: f64 = 120.0;

pub(crate) fn mobile_push_away(window_focused: bool, hid_idle_secs: Option<f64>) -> bool {
    !window_focused
        || hid_idle_secs.is_some_and(|secs| secs.is_finite() && secs >= MOBILE_PUSH_HID_IDLE_SECS)
}

#[cfg(target_os = "macos")]
pub(crate) fn hid_idle_seconds() -> Option<f64> {
    #[link(name = "CoreGraphics", kind = "framework")]
    unsafe extern "C" {
        fn CGEventSourceSecondsSinceLastEventType(state_id: i32, event_type: u32) -> f64;
    }
    // CoreGraphics defines HIDSystemState as 1 and AnyInputEventType as ~0.
    // SAFETY: this OS API reads global input idle time and retains no pointers.
    let seconds = unsafe { CGEventSourceSecondsSinceLastEventType(1, u32::MAX) };
    (seconds.is_finite() && seconds >= 0.0).then_some(seconds)
}

#[cfg(not(target_os = "macos"))]
pub(crate) fn hid_idle_seconds() -> Option<f64> {
    None
}

impl AppState {
    /// Invalidate all operation caches (git + GitHub).
    /// Build a session's VT log buffer with the settings that apply to every
    /// session.
    ///
    /// The palette override and the scrollback-reflow flag were both being
    /// applied per creation site — the palette at three of the five, the reflow
    /// flag at none of them, which is why `scrollback_reflow` was a setting with
    /// no consumer (#660-d087). One constructor means a sixth site cannot be
    /// added that silently misses either.
    pub(crate) fn new_vt_log_buffer(&self, rows: u16, cols: u16, capacity: usize) -> VtLogBuffer {
        let mut vt = VtLogBuffer::new(rows, cols, capacity);
        if let Some(colors) = self.ansi_colors.read().as_ref() {
            vt.set_ansi_colors(colors);
        }
        vt.set_reflow_history(self.config.read().scrollback_reflow);
        vt
    }

    /// Push a changed `scrollback_reflow` into every session that already exists.
    ///
    /// Called from `config::commit_config_change`, not from the `save_config`
    /// callers: `ConfigSaveEffects` is only actioned by the callers that remember
    /// to, and a toggle that silently needs a restart is the failure this whole
    /// story is about.
    pub(crate) fn apply_reflow_history(&self, on: bool) {
        for entry in self.grid.vt_log_buffers.iter() {
            entry.value().lock().set_reflow_history(on);
        }
    }

    pub(crate) fn clear_caches(&self) {
        self.git_cache.clear_all();
    }

    /// Invalidate caches for a specific repo path.
    pub(crate) fn invalidate_repo_caches(&self, path: &str) {
        self.git_cache.invalidate_repo(path);
        crate::prompt::invalidate_repo_vars(path);
    }

    /// Announce that the workspace `workspace_id` names is gone, so the sidebar
    /// drops its row.
    ///
    /// Addressed by workspace id, not branch: the row the frontend must drop is
    /// keyed by id, and two workspaces may share a branch — so a branch-keyed
    /// event cannot say which of them died (#726-5ac7).
    ///
    /// Every removal path must call this — UI, MCP `repo worktree_remove`, the HTTP
    /// route, and merge&archive. Without it a backend-initiated removal leaves a
    /// ghost row forever: the repo-watcher's git fingerprint covers HEAD, the index
    /// and the working tree, so removing a worktree changes nothing it observes and
    /// no `repo-changed` prune is ever emitted for the repo.
    ///
    /// Caches are invalidated first, so any refresh the event triggers reads
    /// post-removal worktree state.
    pub(crate) fn notify_worktree_removed(&self, payload: WorktreeRemovedPayload) {
        self.invalidate_repo_caches(&payload.repo_path);
        #[cfg(feature = "desktop")]
        if let Some(ref app) = *self.app_handle.read() {
            let _ = app.emit("worktree-removed", &payload);
        }
        let _ = self.event_bus.send(AppEvent::WorktreeRemoved(payload));
    }

    /// Announce persisted caller placement over both transports.
    pub(crate) fn notify_session_worktree_declared(&self, payload: WorktreeCreatedPayload) {
        #[cfg(feature = "desktop")]
        if let Some(app) = self.app_handle.read().as_ref() {
            use tauri::Emitter;
            let _ = app.emit("session-worktree-declared", &payload);
        }
        let _ = self
            .event_bus
            .send(AppEvent::SessionWorktreeDeclared(payload));
    }

    /// Announce a newly created workspace, so the frontend can offer to switch
    /// to it.
    ///
    /// The mirror of `notify_worktree_removed`, and it exists for the same
    /// reason: creation used to emit the bus event and the desktop event by hand
    /// at each producer (the HTTP/MCP route and the session-with-worktree route),
    /// which is two implementations of one transition — one of them could forget
    /// the cache invalidation, mint a different id, or fire at a different moment.
    pub(crate) fn notify_worktree_created(&self, payload: WorktreeCreatedPayload) {
        self.invalidate_repo_caches(&payload.repo_path);
        #[cfg(feature = "desktop")]
        if let Some(ref app) = *self.app_handle.read() {
            let _ = app.emit("worktree-created", &payload);
        }
        let _ = self.event_bus.send(AppEvent::WorktreeCreated(payload));
    }

    /// Announce that `repositories.json` was written, so every other client
    /// re-reads disk instead of overwriting it from a stale baseline.
    ///
    /// Call this only when the save actually changed the document — see
    /// `config::save_repositories_request`. Dual-emitted because there is no
    /// bus-to-window forwarder: the desktop WebView listens on the Tauri event,
    /// browser and PWA clients on the `/events` SSE bus.
    pub(crate) fn notify_repositories_changed(&self) {
        let _ = self.event_bus.send(AppEvent::RepositoriesChanged);
        #[cfg(feature = "desktop")]
        if let Some(ref app) = *self.app_handle.read() {
            let _ = app.emit("repositories-changed", serde_json::json!({}));
        }
    }

    /// Default rate limit expiry when no retry_after_ms is provided (120s).
    const RATE_LIMIT_DEFAULT_EXPIRY_MS: u64 = 120_000;

    /// Get a SessionState snapshot with shell_state from the PTY reader's state machine.
    /// Also expires stale rate limits based on retry_after_ms + timestamp.
    pub(crate) fn session_state_with_shell(&self, session_id: &str) -> Option<SessionState> {
        // Expire stale rate limits in-place before building the snapshot.
        if let Some(mut entry) = self.session_maps.session_states.get_mut(session_id)
            && entry.rate_limited
            && entry.rate_limit_set_ms > 0
        {
            let now = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_millis() as u64;
            let ttl = entry
                .retry_after_ms
                .unwrap_or(Self::RATE_LIMIT_DEFAULT_EXPIRY_MS);
            if now.saturating_sub(entry.rate_limit_set_ms) > ttl {
                entry.rate_limited = false;
                entry.retry_after_ms = None;
                entry.rate_limit_set_ms = 0;
            }
        }
        // Clone before consulting SilenceState so no session_states shard guard
        // is held across that mutex. Completion emission uses the inverse order
        // to serialize against a newly submitted input epoch.
        let mut state = self
            .session_maps
            .session_states
            .get(session_id)
            .map(|s| s.clone())?;
        state.queued_commands = crate::pty::queued_command_count(self, session_id) as u32;
        state.shell_state = self
            .session_maps
            .shell_states
            .get(session_id)
            .and_then(|atom| {
                crate::pty::shell_state_wire(atom.load(std::sync::atomic::Ordering::Relaxed))
                    .map(str::to_string)
            });
        let completion_declared = state.suggested_actions.is_some()
            || self
                .session_maps
                .silence_states
                .get(session_id)
                .is_some_and(|silence| {
                    silence
                        .lock()
                        .completion_declared_for_epoch(state.turn_epoch)
                });
        let background_work = state.has_pending_background_probe() || state.background_work;
        // A current-turn completion marker is stronger than a stale BUSY atom
        // (for example a completed Codex screen that still contains its last
        // Working row). Keep real background work authoritative, but normalize
        // the terminal state once the agent has explicitly ended the turn.
        if completion_declared && !background_work {
            state.shell_state = Some("idle".to_string());
        }
        state.agent_state = if state.agent_type.is_none() {
            None
        } else if state.foreground_input_blocked {
            // A direct program's child owns the terminal even if the retained
            // ready screen or completion marker still describes the parent.
            Some("working".to_string())
        } else if state.awaiting_input || state.choice_prompt.is_some() {
            Some("awaiting_input".to_string())
        } else if background_work {
            Some("working".to_string())
        } else if completion_declared {
            Some("completed".to_string())
        } else if state.shell_state.as_deref() == Some("busy") {
            Some("working".to_string())
        } else if state.shell_state.as_deref() == Some("idle") {
            Some("idle".to_string())
        } else {
            Some("starting".to_string())
        };
        Some(state)
    }

    /// Spawn a background task that turns ACP wake signals into `AppEvent`s.
    ///
    /// The ACP client cannot emit them itself: it is a field of this state, so
    /// it can hold neither the event bus nor the desktop window without a
    /// cycle. It publishes notices on its own bus instead, and this is the one
    /// place that mirrors them onto both delivery routes — so a phone on
    /// `/events` and the desktop window are told the same thing at the same
    /// time. Call once at startup, after constructing AppState.
    pub(crate) fn spawn_acp_notice_pump(state: Arc<AppState>) -> tokio::task::AbortHandle {
        let mut notices = state.acp.notices();
        let task = tokio::spawn(async move {
            loop {
                match notices.recv().await {
                    Ok(notice) => {
                        #[cfg(feature = "desktop")]
                        if let Some(app) = state.app_handle.read().as_ref() {
                            use tauri::Emitter;
                            let _ = app.emit("acp-notice", &notice);
                        }
                        let _ = state.event_bus.send(AppEvent::AcpNotice(notice.clone()));
                        if notice.kind == crate::acp::AcpNoticeKind::InteractionPending {
                            let pending = state
                                .acp
                                .pending_interactions(notice.connection_id)
                                .await
                                .ok()
                                .and_then(|interactions| {
                                    interactions.into_iter().find(|interaction| {
                                        Some(interaction.request_id()) == notice.request_id
                                            && Some(interaction.session_id())
                                                == notice.session_id.as_ref()
                                    })
                                })
                                .and_then(|interaction| {
                                    let snapshot = state.acp.snapshot(notice.connection_id).ok()?;
                                    let attachment =
                                        snapshot.attachments.into_iter().find(|attachment| {
                                            attachment.session_id == *interaction.session_id()
                                                && (attachment
                                                    .pending_permission_ids
                                                    .contains(&interaction.request_id())
                                                    || attachment
                                                        .pending_elicitation_ids
                                                        .contains(&interaction.request_id()))
                                        })?;
                                    Some((
                                        interaction.request_id(),
                                        attachment.cwd.to_str()?.to_owned(),
                                    ))
                                });
                            if let Some((url, body)) = Self::mobile_push_for_acp_notice(
                                &state,
                                &notice,
                                pending
                                    .as_ref()
                                    .map(|(request_id, repo)| (*request_id, repo.as_str())),
                            ) {
                                Self::send_mobile_push_url(&state, url, &body);
                            }
                        } else if notice.kind == crate::acp::AcpNoticeKind::Card {
                            let repo = state
                                .acp
                                .snapshot(notice.connection_id)
                                .ok()
                                .filter(|snapshot| snapshot.generation == notice.generation)
                                .and_then(|snapshot| {
                                    snapshot.attachments.into_iter().find(|attachment| {
                                        Some(&attachment.session_id) == notice.session_id.as_ref()
                                    })
                                })
                                .and_then(|attachment| attachment.cwd.to_str().map(str::to_owned));
                            if let Some((url, body)) = repo.as_deref().and_then(|repo| {
                                Self::mobile_push_for_acp_session(&state, &notice, repo)
                            }) {
                                Self::send_mobile_push_url(&state, url, &body);
                            }
                        }
                    }
                    // A notice carries nothing that cannot be re-read: a client
                    // that missed one still finds the truth in the connection
                    // snapshot and the pending interactions.
                    Err(tokio::sync::broadcast::error::RecvError::Lagged(n)) => {
                        tracing::warn!(source = "acp", lagged = n, "ACP notice bus lagged");
                    }
                    Err(tokio::sync::broadcast::error::RecvError::Closed) => break,
                }
            }
        });
        task.abort_handle()
    }

    fn mobile_push_for_acp_notice(
        state: &Arc<AppState>,
        notice: &crate::acp::AcpNotice,
        pending: Option<(crate::acp::AcpHostRequestId, &str)>,
    ) -> Option<(String, String)> {
        if notice.kind != crate::acp::AcpNoticeKind::InteractionPending
            || notice.request_id.is_none()
            || notice.request_id != pending.map(|(request_id, _)| request_id)
        {
            return None;
        }
        let (_, repo) = pending?;
        Self::mobile_push_for_acp_session(state, notice, repo)
    }

    /// Questions and ego cards spend the same conversation budget.
    fn mobile_push_for_acp_session(
        state: &Arc<AppState>,
        notice: &crate::acp::AcpNotice,
        repo: &str,
    ) -> Option<(String, String)> {
        let session_id = notice.session_id.as_ref()?;
        let ready = {
            let config = state.config.read();
            config.services.push.enabled
                && !config.services.push.vapid_private_key.is_empty()
                && !state.push_store.is_empty()
        };
        let focused = state
            .desktop_window_focused
            .load(std::sync::atomic::Ordering::Relaxed);
        let away = mobile_push_away(focused, if focused { hid_idle_seconds() } else { None });
        if !ready || !away {
            return None;
        }
        let now_ms = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_millis() as u64;
        let mut last = state
            .acp_push_last_ms
            .entry(session_id.to_string())
            .or_default();
        if !crate::push::reserve_push_slot(&mut last, now_ms, true) {
            return None;
        }
        let query = url::form_urlencoded::Serializer::new(String::new())
            .append_pair("repo", repo)
            .append_pair("session", &session_id.to_string())
            .finish();
        Some((
            format!("/mobile?{query}"),
            if notice.kind == crate::acp::AcpNoticeKind::Card {
                "AI Chat: new notice"
            } else {
                "AI Chat: response needed"
            }
            .to_string(),
        ))
    }

    /// Spawn a background task that subscribes to the event bus and updates
    /// `session_states`. Call once at startup after constructing AppState.
    pub(crate) fn spawn_session_state_accumulator(state: Arc<AppState>) {
        let mut broadcast_rx = state.event_bus.subscribe();
        let mut state_rx = state
            .session_maps
            .session_state_events
            .rx
            .lock()
            .take()
            .expect("session state accumulator must be spawned exactly once");
        tokio::spawn(async move {
            // Last state published per session, so a repaint that changes nothing
            // publishes nothing. Task-local because this task is the sole writer
            // of `session_states` and therefore the sole source of these pushes.
            let mut published: HashMap<String, SessionState> = HashMap::new();
            loop {
                tokio::select! {
                    event = state_rx.recv() => match event {
                        Some(event) => {
                            Self::apply_event_to_session_state(&state, &event);
                            Self::publish_session_state_change(&state, &event, &mut published);
                            // After the apply, not before: the depth counts events the
                            // authoritative state has not absorbed yet, and an event
                            // being applied right now is still one of them.
                            state.session_maps.session_state_events.applied();
                        }
                        None => break,
                    },
                    event = broadcast_rx.recv() => match event {
                        // PTY-scoped events arrive on the lossless lane above.
                        Ok(event) if event.pty_session_id().is_none() => {
                            Self::apply_event_to_session_state(&state, &event);
                        }
                        Ok(_) => {}
                        Err(tokio::sync::broadcast::error::RecvError::Lagged(n)) => {
                            tracing::warn!(source = "session_state", lagged = n, "Event bus lagged");
                        }
                        Err(tokio::sync::broadcast::error::RecvError::Closed) => break,
                    }
                }
            }
        });
    }

    /// Publish `SessionStateChanged` for the session `event` just mutated, when
    /// the state a client renders actually moved.
    ///
    /// Called from the accumulator and nowhere else, because only the
    /// accumulator can see a transition: it owns every write to `session_states`,
    /// and `session_state_with_shell` folds in the three fields derived at read
    /// time (`shell_state`, `queued_commands`, `agent_state`). Dedup is
    /// `SessionState`'s `PartialEq`, which excludes `last_activity_ms` — the one
    /// field a silent repaint does move.
    ///
    /// Only PTY-scoped events are considered: they are the only variants
    /// `apply_event_to_session_state` writes state for, so a global event can
    /// never hide a transition here.
    fn publish_session_state_change(
        state: &Arc<AppState>,
        event: &AppEvent,
        published: &mut HashMap<String, SessionState>,
    ) {
        let Some(session_id) = event.pty_session_id() else {
            return;
        };
        let Some(current) = state.session_state_with_shell(session_id) else {
            // The row is gone (`SessionClosed`). Drop the baseline too: a reused
            // id must not be deduped against the state of a dead session.
            published.remove(session_id);
            return;
        };
        if published.get(session_id) == Some(&current) {
            return;
        }
        published.insert(session_id.to_string(), current.clone());
        // Dual-emit. Nothing forwards the bus to the desktop window, so the
        // window listener is fed here and the bus feeds `/events` SSE — the
        // IPC/HTTP parity rule in AGENTS.md. Both carry the same payload.
        #[cfg(feature = "desktop")]
        if let Some(app) = state.app_handle.read().as_ref() {
            let _ = app.emit(
                "session-state-changed",
                session_state_payload(session_id, &current),
            );
        }
        let _ = state.event_bus.send(AppEvent::SessionStateChanged {
            session_id: session_id.to_string(),
            state: Box::new(current),
        });
    }

    /// Send a push notification to all mobile subscribers, deep-linking a session.
    fn send_mobile_push(state: &Arc<AppState>, session_id: &str, body: &str) {
        Self::send_mobile_push_url(state, format!("/mobile/session/{session_id}"), body);
    }

    /// Send a push notification to all mobile subscribers at an arbitrary deep link.
    ///
    /// A blocked confirmation is the reason this is not private to session events:
    /// a human who is away from the machine learns about it only through push.
    pub(crate) fn send_mobile_push_url(state: &Arc<AppState>, url: String, body: &str) {
        let config = state.config.read().clone();
        let subs = state.push_store.list();
        let http_client = state.http_client.clone();
        let push_state = Arc::clone(state);
        let body = body.to_owned();
        tokio::spawn(async move {
            let result = crate::push::send_push_batch(
                subs,
                &config,
                &http_client,
                "TUICommander",
                &body,
                &url,
            )
            .await;
            for endpoint in &result.stale_endpoints {
                push_state.push_store.remove(endpoint);
            }
        });
    }

    /// Apply a single event to the session state accumulator.
    fn apply_event_to_session_state(state: &Arc<AppState>, event: &AppEvent) {
        // #744-138c: awaiting-evidence side effect on the session's
        // SilenceState, applied outside the session_states entry lock (see
        // the PtyParsed arm below) to preserve the SilenceState → SessionState
        // lock order used throughout pty.rs.
        enum AwaitingEvidenceOp {
            Clear,
            RecordChoicePrompt,
        }
        let now_ms = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_millis() as u64;

        match event {
            AppEvent::SessionCreated {
                session_id,
                agent_type,
                ..
            } => {
                state
                    .session_maps.session_states
                    .entry(session_id.clone())
                    .and_modify(|session| {
                        session.last_activity_ms = now_ms;
                        // Creation may be applied after foreground discovery.
                        // Never re-arm a preset whose agent was already seen.
                        if session.agent_type.is_none() && !session.agent_foreground_observed {
                            session.seed_configured_agent(agent_type.clone());
                        }
                    })
                    .or_insert_with(|| {
                        let mut session = SessionState { last_activity_ms: now_ms, ..Default::default() };
                        session.seed_configured_agent(agent_type.clone());
                        session
                    });
            }
            AppEvent::PtyDescriptionChanged { .. } => {}
            AppEvent::ChatViewChanged { .. } => {}
            AppEvent::SessionRenamed { .. } => {}
            AppEvent::SessionSuspendRequested { .. } => {}
            AppEvent::TermAliasAssigned { .. } => {}
            // A watcher hit says nothing about the session's own state — it is a
            // plugin-facing signal that rides the bus for browser clients only.
            AppEvent::PluginWatcherLines { .. } => {}
            // Deliberately does NOT stamp `last_activity_ms`. That field answers
            // "when did this session last do something notable" — it moves on
            // SessionCreated/PtyParsed/PtyExit, i.e. on semantic events, and the
            // mobile client renders it as such (`SessionCard.tsx`). This pulse
            // answers the different question "are bytes flowing right now", which
            // is true throughout a `tail -f` that produces no semantic event at
            // all. Folding the two would silently redefine the mobile column.
            AppEvent::PtyActivity { .. } | AppEvent::PtyTitle { .. } => {}
            // Shell-integration markers and the OSC 7 cwd are terminal-rendering
            // signals, not session state. The cwd that state cares about is
            // written straight onto the `sessions` entry at the emit site; this
            // event exists to reach clients, not to be accumulated.
            AppEvent::PtyOsc133 { .. } | AppEvent::PtyCwd { .. } => {}
            AppEvent::SessionClosed { session_id, .. } => {
                state.session_maps.session_states.remove(session_id);
            }
            AppEvent::PtyParsed { session_id, parsed } => {
                let event_type = parsed.get("type").and_then(|t| t.as_str()).unwrap_or("");
                let event_turn_epoch = parsed.get("_turn_epoch").and_then(|v| v.as_u64());

                // Collect push notification data outside the DashMap lock
                let mut push_data: Option<(String, String)> = None;
                // Wait metadata for a session that just parked, routed to the
                // spawning orchestrator's inbox outside the lock below.
                let mut parked_wait: Option<(String, bool, &'static str)> = None;

                // Ranked awaiting evidence (#744-138c): resolved through the
                // session's SilenceState BEFORE the session_states entry is
                // taken below. Lock order is SilenceState → SessionState
                // everywhere in pty.rs (see note_submitted_input_with_hook);
                // taking session_states first and reaching into silence_states
                // while still holding it would invert that order against a
                // thread that already follows it, and could deadlock.
                let current_turn_epoch = state
                    .session_maps
                    .session_states
                    .get(session_id)
                    .map(|s| s.turn_epoch);
                let epoch_matches =
                    event_turn_epoch.is_none_or(|epoch| epoch == current_turn_epoch.unwrap_or(0));
                // Don't let a low-confidence (silence-heuristic) question
                // overwrite an already-active high-confidence one — e.g. grok
                // signals an approval prompt via its "Action Required" title
                // (confident) while its on-screen status line is also parsed as
                // a low-confidence question. The generic rank gate reproduces
                // this: a confident (`Protocol`-rank) verdict rejects a
                // heuristic (`Screen`-rank) one, same-or-higher rank updates.
                let question_admitted = (event_type == "question" && epoch_matches).then(|| {
                    let new_confident = parsed
                        .get("confident")
                        .and_then(|v| v.as_bool())
                        .unwrap_or(false);
                    let (rank, source) = if new_confident {
                        // Kept apart so `progress-superseded` can tell a
                        // blocked report from a dialog it must never clear.
                        let source = if parsed.get("source").and_then(|v| v.as_str())
                            == Some("progress-blocked")
                        {
                            "progress-blocked"
                        } else {
                            "question-confident"
                        };
                        (crate::pty::EvidenceRank::Protocol, source)
                    } else {
                        (crate::pty::EvidenceRank::Screen, "question-heuristic")
                    };
                    state
                        .session_maps
                        .silence_states
                        .get(session_id)
                        .map(|sl| sl.lock().record_awaiting(rank, source))
                        .unwrap_or(true)
                });
                let push_ready = matches!(event_type, "question" | "choice-prompt") && {
                    let config = state.config.read();
                    config.services.push.enabled
                        && !config.services.push.vapid_private_key.is_empty()
                        && !state.push_store.is_empty()
                };
                let window_focused = state
                    .desktop_window_focused
                    .load(std::sync::atomic::Ordering::Relaxed);
                let desktop_away = matches!(event_type, "question" | "choice-prompt")
                    && mobile_push_away(
                        window_focused,
                        if window_focused { hid_idle_seconds() } else { None },
                    );
                // "status-line" and "question-cleared" only clear a non-confident
                // awaiting verdict — the old `!question_confident` sticky guard,
                // read off the same ranked evidence instead of a raw bool.
                let awaiting_is_confident = matches!(event_type, "status-line" | "question-cleared")
                    && state
                        .session_maps
                        .silence_states
                        .get(session_id)
                        .is_some_and(|sl| {
                            sl.lock().awaiting_rank() == Some(crate::pty::EvidenceRank::Protocol)
                        });
                // A later progress entry from the same PTY retracts only the
                // badge a `progress blocked` raised; a dialog, a choice prompt
                // or any other confident question records another source.
                let awaiting_from_progress = event_type == "progress-superseded"
                    && state
                        .session_maps
                        .silence_states
                        .get(session_id)
                        .is_some_and(|sl| sl.lock().awaiting_source() == Some("progress-blocked"));
                // Applied to the session's SilenceState AFTER `s` (below) is
                // dropped, for the same lock-order reason.
                let mut awaiting_evidence_op: Option<AwaitingEvidenceOp> = None;

                let mut s = state
                    .session_maps.session_states
                    .entry(session_id.clone())
                    .or_insert_with(|| SessionState {
                        last_activity_ms: now_ms,
                        ..Default::default()
                    });
                s.last_activity_ms = now_ms;
                match event_type {
                    "question" if epoch_matches => {
                        let new_confident = parsed
                            .get("confident")
                            .and_then(|v| v.as_bool())
                            .unwrap_or(false);
                        if question_admitted == Some(true) {
                            let was_awaiting = s.awaiting_input;
                            s.awaiting_input = true;
                            s.question_text = parsed
                                .get("prompt_text")
                                .and_then(|t| t.as_str())
                                .map(|t| t.to_string());
                            s.question_confident = new_confident;
                            tracing::debug!(
                                session_id = %session_id,
                                turn_epoch = s.turn_epoch,
                                confident = new_confident,
                                hook_instrumented = s.hook_instrumented,
                                generic_claude_notify = s.question_text.as_deref()
                                    == Some("Claude is waiting for your input"),
                                "awaiting_input set from question"
                            );

                            // Confidence is metadata, not a routing gate. A
                            // managed child has nobody at its keyboard, so every
                            // transition into a wait must reach its orchestrator;
                            // otherwise plan/skill pickers park invisibly.
                            if !was_awaiting {
                                parked_wait = Some((
                                    s.question_text.clone().unwrap_or_default(),
                                    new_confident,
                                    "question",
                                ));
                            }

                            // Rate limit: skip if last push for this session was < 30s ago
                            let eligible = push_ready
                                && desktop_away
                                && s.question_text.as_deref().is_some_and(|text| !text.trim().is_empty());
                            let should_push = crate::push::reserve_push_slot(
                                &mut s.last_push_ms,
                                now_ms,
                                eligible,
                            );
                            if should_push {
                                let prompt = s.question_text.clone().unwrap_or_default();
                                push_data = Some((session_id.clone(), prompt));
                            }
                        }
                    }
                    "question-cleared" if epoch_matches => {
                        // The silence timer saw the question leave the
                        // screen. It only fires for the heuristic state,
                        // but re-check here: a confident question may
                        // have landed between the check and this event.
                        if !awaiting_is_confident {
                            s.awaiting_input = false;
                            s.question_text = None;
                            awaiting_evidence_op = Some(AwaitingEvidenceOp::Clear);
                        }
                    }
                    "protocol-question-cleared" if epoch_matches => {
                        let expected = parsed
                            .get("expected_question_text")
                            .and_then(|value| value.as_str());
                        if expected.is_none_or(|text| s.question_text.as_deref() == Some(text)) {
                            s.awaiting_input = false;
                            s.question_text = None;
                            s.question_confident = false;
                            s.choice_prompt = None;
                            awaiting_evidence_op = Some(AwaitingEvidenceOp::Clear);
                        }
                    }
                    "progress-superseded" if awaiting_from_progress => {
                        s.awaiting_input = false;
                        s.question_text = None;
                        s.question_confident = false;
                        awaiting_evidence_op = Some(AwaitingEvidenceOp::Clear);
                    }
                    "user-input" => {
                        // User responded — agent will start working
                        s.awaiting_input = false;
                        s.question_text = None;
                        s.question_confident = false;
                        s.slash_menu_items = None;
                        s.choice_prompt = None;
                        awaiting_evidence_op = Some(AwaitingEvidenceOp::Clear);
                        // Capture as last_prompt if >= 10 words
                        if let Some(content) = parsed.get("content").and_then(|v| v.as_str())
                            && content.split_whitespace().count() >= 10
                        {
                            s.last_prompt = Some(content.to_string());
                        }
                    }
                    "rate-limit" if s.current_task.is_some() => {
                        s.rate_limited = true;
                        s.retry_after_ms = parsed.get("retry_after_ms").and_then(|v| v.as_u64());
                        s.rate_limit_set_ms = now_ms;
                    }
                    "usage-limit" => {
                        s.usage_limit_pct = parsed
                            .get("percentage")
                            .and_then(|v| v.as_u64())
                            .map(|v| v as u8);
                    }
                    "api-error" => {
                        s.last_error = parsed
                            .get("matched_text")
                            .and_then(|t| t.as_str())
                            .map(|t| t.to_string());
                    }
                    "status-line" => {
                        // Agent is working — clear error/rate-limit/suggest/question.
                        // Keep slash_menu_items — the agent's status line can tick
                        // while the user is still interacting with the slash menu, and
                        // wiping it here causes the PWA overlay to flash off.
                        //
                        // A *confident* question stays sticky across status-line ticks.
                        // grok keeps its spinner animating (emitting status-line) WHILE
                        // awaiting approval ("⚠ Action Required" title → confident
                        // question), so a busy tick must not clobber it — otherwise
                        // awaiting_input flickers. It clears on user-input (the user
                        // answered, state.rs user-input arm). Low-confidence
                        // silence-heuristic questions still yield to the busy signal.
                        if !awaiting_is_confident {
                            s.awaiting_input = false;
                            s.question_text = None;
                            awaiting_evidence_op = Some(AwaitingEvidenceOp::Clear);
                        }
                        s.rate_limited = false;
                        s.retry_after_ms = None;
                        s.rate_limit_set_ms = 0;
                        s.last_error = None;
                        s.suggested_actions = None;
                        // Only update current_task + activity timestamp when task changes.
                        // Spinner rotations (same task name) are suppressed to avoid
                        // churning the state and flooding WS clients.
                        let new_task = parsed
                            .get("task_name")
                            .and_then(|v| v.as_str())
                            .map(|t| t.to_string());
                        if s.current_task != new_task {
                            s.current_task = new_task;
                        }
                    }
                    "intent" => {
                        s.agent_intent = parsed
                            .get("text")
                            .and_then(|v| v.as_str())
                            .map(|t| t.to_string());
                    }
                    "suggest" if event_turn_epoch.is_none_or(|epoch| epoch == s.turn_epoch) => {
                        s.suggested_actions =
                            parsed.get("items").and_then(|v| v.as_array()).map(|arr| {
                                arr.iter()
                                    .filter_map(|v| v.as_str().map(|s| s.to_string()))
                                    .collect()
                            });
                    }
                    "slash-menu" => {
                        // Borrowed, like the choice-prompt arm below: `from_value`
                        // would deep-clone the items subtree out of the very payload
                        // the Arc exists to stop copying.
                        s.slash_menu_items = parsed.get("items").and_then(|v| {
                            <Vec<crate::output_parser::SlashMenuItem> as serde::Deserialize>::deserialize(v).ok()
                        });
                    }
                    "choice-prompt" => {
                        // Deserialise directly from the parsed JSON — the payload fields
                        // (title/options/dismiss_key/amend_key) are flat at the top level
                        // alongside "type", and serde ignores the unknown "type" field.
                        let was_awaiting = s.awaiting_input;
                        // Deserialize from the borrowed payload: `from_value`
                        // wants it owned, which would deep-clone the very tree the
                        // Arc exists to stop copying.
                        s.choice_prompt = <crate::output_parser::ChoicePromptPayload
                            as serde::Deserialize>::deserialize(&**parsed)
                        .ok();
                        s.awaiting_input = true;
                        if let Some(title) = s.choice_prompt.as_ref().map(|choice| choice.title.clone()) {
                            s.question_text = Some(title.clone());
                            s.question_confident = true;
                            if crate::push::reserve_push_slot(
                                &mut s.last_push_ms,
                                now_ms,
                                push_ready && desktop_away && !title.trim().is_empty(),
                            ) {
                                push_data = Some((session_id.clone(), title));
                            }
                        }
                        awaiting_evidence_op = Some(AwaitingEvidenceOp::RecordChoicePrompt);
                        if !was_awaiting {
                            parked_wait = Some((
                                parsed
                                    .get("title")
                                    .and_then(|value| value.as_str())
                                    .unwrap_or_default()
                                    .to_string(),
                                true,
                                "choice",
                            ));
                        }
                    }
                    "choice-cleared" => {
                        s.choice_prompt = None;
                        s.awaiting_input = false;
                        awaiting_evidence_op = Some(AwaitingEvidenceOp::Clear);
                        s.question_text = None;
                        s.question_confident = false;
                    }
                    "active-subtasks" => {
                        s.active_sub_tasks =
                            parsed.get("count").and_then(|v| v.as_u64()).unwrap_or(0) as u32;
                    }
                    "progress" => {
                        let state_val = parsed.get("state").and_then(|v| v.as_u64()).unwrap_or(0);
                        if state_val == 0 {
                            // state=0 means remove the progress bar
                            s.progress = None;
                        } else {
                            s.progress = parsed
                                .get("value")
                                .and_then(|v| v.as_u64())
                                .map(|v| v as u8);
                        }
                    }
                    _ => {}
                }
                drop(s);

                // Applied outside the session_states entry lock — same
                // lock-order reason as `question_admitted`/`awaiting_is_confident`
                // above (#744-138c).
                if let Some(op) = awaiting_evidence_op
                    && let Some(sl) = state.session_maps.silence_states.get(session_id)
                {
                    let mut sl = sl.lock();
                    match op {
                        AwaitingEvidenceOp::Clear => sl.clear_awaiting(),
                        AwaitingEvidenceOp::RecordChoicePrompt => {
                            sl.record_awaiting(crate::pty::EvidenceRank::Protocol, "choice-prompt");
                        }
                    }
                }

                // Unblock-triggered flush (story 091): user-input just cleared
                // question_confident. If the agent answered a confident question but
                // stays idle, there is no BUSY→IDLE transition to drain its queued
                // peer messages — deliver them now. flush_pending_injections is
                // self-guarded (idle agent, no confident question), and this runs
                // outside the session_states entry lock to avoid re-entrancy.
                if event_type == "user-input" {
                    crate::pty::flush_pending_injections(state, session_id);
                }

                // Route the parked prompt to the spawning orchestrator, outside the
                // session_states entry lock: delivery reaches into the PARENT's
                // SilenceState and re-entering this shard would deadlock.
                if let Some((prompt, confident, source)) = parked_wait {
                    crate::pty::push_state_change_to_parent(
                        state,
                        session_id,
                        serde_json::json!({
                            "type": "state_change",
                            "state": "awaiting_input",
                            "session_id": session_id,
                            "prompt": prompt,
                            "confident": confident,
                            "source": source,
                        }),
                    );
                }

                // Spawn push notification outside the DashMap lock
                if let Some((sid, prompt)) = push_data {
                    let session_name = state
                        .session_maps.sessions
                        .get(&sid)
                        .and_then(|s| s.value().lock().display_name.clone())
                        .unwrap_or_else(|| sid.clone());
                    let body = if prompt.is_empty() {
                        format!("{session_name}: awaiting input")
                    } else {
                        format!("{session_name}: {prompt}")
                    };
                    Self::send_mobile_push(state, &sid, &body);
                }
            }
            AppEvent::PtyExit { session_id } => {
                let push_ready = {
                    let config = state.config.read();
                    config.services.push.enabled
                        && !config.services.push.vapid_private_key.is_empty()
                        && !state.push_store.is_empty()
                };
                let window_focused = state
                    .desktop_window_focused
                    .load(std::sync::atomic::Ordering::Relaxed);
                let desktop_away = mobile_push_away(
                    window_focused,
                    if window_focused { hid_idle_seconds() } else { None },
                );
                let mut should_push = false;
                {
                    let mut entry = state
                        .session_maps
                        .session_states
                        .entry(session_id.clone())
                        .or_default();
                    entry.awaiting_input = false;
                    entry.question_text = None;
                    entry.question_confident = false;
                    entry.rate_limited = false;
                    entry.retry_after_ms = None;
                    entry.rate_limit_set_ms = 0;
                    entry.active_sub_tasks = 0;
                    entry.choice_prompt = None;
                    entry.last_activity_ms = now_ms;
                    // Completion and questions share the same per-session window.
                    if crate::push::reserve_push_slot(
                        &mut entry.last_push_ms,
                        now_ms,
                        push_ready && desktop_away,
                    ) {
                        should_push = true;
                    }
                }
                if should_push {
                    let session_name = state
                        .session_maps.sessions
                        .get(session_id)
                        .and_then(|s| s.value().lock().display_name.clone())
                        .unwrap_or_else(|| session_id.clone());
                    Self::send_mobile_push(
                        state,
                        session_id,
                        &format!("{session_name}: completed"),
                    );
                }
            }
            // Global events don't affect per-session state
            AppEvent::HeadChanged { .. }
            | AppEvent::RepoChanged { .. }
            | AppEvent::PluginChanged { .. }
            | AppEvent::UpstreamStatusChanged { .. }
            | AppEvent::McpOAuthStart { .. }
            | AppEvent::McpToast { .. }
            | AppEvent::McpConfirm { .. }
            | AppEvent::McpConfirmResolved { .. }
            | AppEvent::RepositoriesChanged
            | AppEvent::DirChanged { .. }
            | AppEvent::SessionWorktreeDeclared { .. }
            | AppEvent::WorktreeCreated { .. }
            | AppEvent::WorktreeRemoved { .. }
            | AppEvent::PeerRegistered { .. }
            | AppEvent::PeerUnregistered { .. }
            | AppEvent::UiTab { .. }
            | AppEvent::GitHubPrUpdate { .. }
            | AppEvent::GitHubTransition { .. }
            | AppEvent::GitHubIssuesUpdate { .. }
            | AppEvent::CloseHtmlTabs { .. }
            | AppEvent::DesignModeChanged { .. }
            | AppEvent::ConflictAssistStatus { .. }
            | AppEvent::ProgressRecorded { .. }
            | AppEvent::WorkflowRunChanged { .. }
            | AppEvent::ReviewProgress { .. }
            | AppEvent::ProposalsReady { .. }
            // This accumulator's own output. Feeding it back in would make the
            // session state a function of itself; it is a report, not an input.
            | AppEvent::SessionStateChanged { .. }
            // An ACP connection is not a PTY session and has no row here.
            | AppEvent::AcpNotice(_)
            // A remote daemon coming up or going down says nothing about any
            // session: the sessions it holds report themselves, over the bridge
            // that connection carries.
            | AppEvent::RemoteConnectionStatusChanged { .. }
            // A mirrored event is the far end's accumulator output. Feeding it
            // in here would build a second, local row for a session this
            // machine does not run.
            | AppEvent::RemoteMirrored { .. } => {}
            // Dictation is bound to a session but says nothing about it: a
            // download belongs to the installation, and a spoken reply belongs
            // to the conversation rather than to the terminal it will reach.
            #[cfg(feature = "dictation")]
            AppEvent::DictationDownloadProgress { .. }
            | AppEvent::SpeechDownloadProgress { .. }
            | AppEvent::SpeechUtterance { .. } => {}
        }
    }

    /// Build orchestrator stats snapshot from current state.
    pub(crate) fn orchestrator_stats(&self) -> OrchestratorStats {
        let active = self.session_maps.sessions.len();
        OrchestratorStats {
            active_sessions: active,
            max_sessions: MAX_CONCURRENT_SESSIONS,
            available_slots: MAX_CONCURRENT_SESSIONS.saturating_sub(active),
        }
    }

    /// Build session metrics JSON from current atomic counters.
    pub(crate) fn session_metrics_json(&self) -> serde_json::Value {
        use std::sync::atomic::Ordering;
        serde_json::json!({
            "total_spawned": self.metrics.total_spawned.load(Ordering::Relaxed),
            "failed_spawns": self.metrics.failed_spawns.load(Ordering::Relaxed),
            "active_sessions": self.metrics.active_sessions.load(Ordering::Relaxed),
            "bytes_emitted": self.metrics.bytes_emitted.load(Ordering::Relaxed),
            "pauses_triggered": self.metrics.pauses_triggered.load(Ordering::Relaxed),
        })
    }
}

/// Agent orchestration stats
#[derive(Clone, Serialize)]
pub(crate) struct OrchestratorStats {
    pub(crate) active_sessions: usize,
    pub(crate) max_sessions: usize,
    pub(crate) available_slots: usize,
}

#[derive(Clone, Serialize, Deserialize)]
pub(crate) struct PtyConfig {
    pub(crate) rows: u16,
    pub(crate) cols: u16,
    pub(crate) shell: Option<String>,
    pub(crate) cwd: Option<String>,
    /// Pre-generated stable session UUID — injected as `TUIC_SESSION` env var.
    /// Persists across app restarts so agents can resume the same session.
    pub(crate) tuic_session: Option<String>,
    /// Extra environment variables injected into the PTY process (e.g. agent feature flags).
    #[serde(default)]
    pub(crate) env: HashMap<String, String>,
    /// Pre-set agent type for sessions that will launch a known agent (e.g. from
    /// a run config). Enables intent/suggest parsing from the first output line,
    /// even when the binary name doesn't match `classify_agent` (custom aliases,
    /// symlinks, wrapper scripts).
    #[serde(default)]
    pub(crate) agent_type: Option<String>,
    /// Terminal alias this tab held before the restart (e.g. `tu-3`). Reserved
    /// verbatim so the address other agents already know keeps working; a
    /// malformed or taken value is ignored. See `AppState::assign_term_alias`.
    #[serde(default)]
    pub(crate) alias: Option<String>,
}

#[derive(Clone, Serialize, Deserialize)]
pub(crate) struct AgentConfig {
    pub(crate) prompt: String,
    pub(crate) cwd: Option<String>,
    pub(crate) model: Option<String>,
    pub(crate) print_mode: bool,
    pub(crate) output_format: Option<String>,
    pub(crate) agent_type: Option<String>,
    pub(crate) binary_path: Option<String>,
    pub(crate) args: Option<Vec<String>>,
}

/// Test helper: construct a minimal `AppState` for unit tests in other modules.
#[cfg(test)]
pub(crate) mod tests_support {
    use super::*;

    /// Replay the production accumulator synchronously, without scheduling races.
    pub(crate) fn apply_replay_event(state: &Arc<AppState>, event: &AppEvent) {
        AppState::apply_event_to_session_state(state, event);
    }

    /// A live `sessions` entry backed by a real PTY. `live_pty_for_peer` filters on
    /// liveness, so a resolver test needs a session that genuinely exists rather
    /// than a stub the filter would reject.
    #[cfg(unix)]
    pub fn insert_dummy_session(state: &AppState, session_id: &str) {
        use portable_pty::{CommandBuilder, PtySize, native_pty_system};
        let pair = native_pty_system()
            .openpty(PtySize {
                rows: 24,
                cols: 80,
                pixel_width: 0,
                pixel_height: 0,
            })
            .expect("openpty");
        let mut command = CommandBuilder::new("/bin/sh");
        command.args(["-c", "sleep 30"]);
        let child = pair.slave.spawn_command(command).expect("spawn shell");
        let writer = pair.master.take_writer().expect("writer");
        state.session_maps.sessions.insert(
            session_id.to_string(),
            parking_lot::Mutex::new(PtySession {
                launch_receipt: None,
                writer: std::sync::Arc::new(parking_lot::Mutex::new(writer)),
                master: pair.master,
                _child: child,
                paused: std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false)),
                worktree: None,
                initial_cwd: None,
                cwd: None,
                display_name: None,
                display_name_is_custom: false,
                display_name_from_spawn: false,
                is_remote: false,
                shell: "/bin/sh".to_string(),
            }),
        );
    }

    /// Give a live test session a working directory, which is what the alias
    /// prefix is derived from.
    #[cfg(unix)]
    pub fn set_session_cwd(state: &AppState, session_id: &str, cwd: &str) {
        state
            .session_maps
            .sessions
            .get(session_id)
            .expect("session exists")
            .lock()
            .cwd = Some(cwd.to_string());
    }

    pub fn make_test_app_state() -> AppState {
        make_test_app_state_in(&crate::test_support::test_temp_root())
    }

    pub fn make_test_app_state_in(root: &std::path::Path) -> AppState {
        // Unique data dir per call: AppState::new eagerly opens
        // `data_dir/tunnel_audit.db`, so a shared path makes parallel tests
        // collide on the SQLite file (concurrent opens → SQLITE_BUSY
        // "database is locked"). The guard keeps every DB isolated and
        // removes it when the state drops, including during panic unwind.
        let data_guard = tempfile::Builder::new()
            .prefix("test-tuic-data-")
            .tempdir_in(root)
            .expect("create test AppState data dir");
        let data_dir = data_guard.path().to_path_buf();
        let log_buffer = Arc::new(parking_lot::Mutex::new(
            crate::app_logger::LogRingBuffer::new(crate::app_logger::LOG_RING_CAPACITY),
        ));
        // `AppState::new` takes `data_dir` explicitly for exactly this reason
        // (an isolated path per test), but `claude_usage::load_cache_from_disk`
        // reads `config::config_dir()` instead of the directory it was handed
        // — in production the two are the same value (`lib.rs` passes
        // `config::config_dir()` as `data_dir`), so this only diverges in
        // tests. `config_dir()`'s own process-safe test fallback (never the
        // real platform directory — see `config.rs`) covers it without this
        // function needing its own override: setting one here deadlocked
        // every test that first calls `isolated_config()`-style helpers and
        // THEN `make_test_app_state()` on the same thread, since
        // `set_config_dir_override`'s exclusive lock is not reentrant
        // (`finalize_deletes_once_the_user_confirms` and six siblings in
        // `worktree.rs` hung until nextest's timeout before this was reverted).
        let mut state = AppState::new(
            data_dir,
            data_guard.path().join("worktrees"),
            crate::config::AppConfig::default(),
            log_buffer,
        );
        // Override session_token for deterministic tests
        state.session_token = parking_lot::RwLock::new(String::from("test-token"));
        // Skip disk I/O for claude_usage in tests
        state.claude_usage_cache = parking_lot::Mutex::new(std::collections::HashMap::new());
        state._test_data_dir = Some(data_guard);
        state
    }
}

#[cfg(test)]
mod worktree_event_payloads {
    use super::*;

    fn created() -> WorktreeCreatedPayload {
        WorktreeCreatedPayload {
            creator_session: Some("creator-pty".into()),
            spawn_session: false,
            repo_path: "/repo".to_string(),
            workspace_id: "feature/x".to_string(),
            branch: "feature/x".to_string(),
            worktree_path: "/repo__wt/feature-x".to_string(),
            kind: crate::worktree::WorkspaceKind::Worktree,
        }
    }

    fn removed() -> WorktreeRemovedPayload {
        WorktreeRemovedPayload {
            repo_path: "/repo".to_string(),
            workspace_id: "feature/x".to_string(),
            branch: "feature/x".to_string(),
        }
    }

    /// The wire contract, spelled out. This is the load-bearing assertion: the
    /// two transports now serialize one struct, so comparing them to each other
    /// can no longer catch a rename — comparing both to a literal can.
    #[test]
    fn the_created_payload_spells_its_wire_fields() {
        assert_eq!(
            serde_json::to_value(created()).unwrap(),
            serde_json::json!({
                "repo_path": "/repo",
                "workspace_id": "feature/x",
                "branch": "feature/x",
                "worktree_path": "/repo__wt/feature-x",
                "creator_session": "creator-pty",
                "spawn_session": false,
                "kind": "worktree",
            })
        );
    }

    #[test]
    fn the_removed_payload_spells_its_wire_fields() {
        assert_eq!(
            serde_json::to_value(removed()).unwrap(),
            serde_json::json!({
                "repo_path": "/repo",
                "workspace_id": "feature/x",
                "branch": "feature/x",
            })
        );
    }

    /// The desktop Tauri payload and the SSE payload are the SAME object for the
    /// same event — the frontend store reads one field name on both transports.
    #[test]
    fn the_tauri_payload_and_the_sse_payload_agree() {
        assert_eq!(
            serde_json::to_value(created()).unwrap(),
            crate::mcp_http::sse_routes::event_payload_for_test(&AppEvent::WorktreeCreated(
                created()
            )),
        );
        assert_eq!(
            serde_json::to_value(removed()).unwrap(),
            crate::mcp_http::sse_routes::event_payload_for_test(&AppEvent::WorktreeRemoved(
                removed()
            )),
        );
    }

    /// The event names are the strings the frontend narrows on. A rename here is
    /// a silent no-op at every listener, so it is asserted rather than reviewed.
    #[test]
    fn the_event_names_are_the_kebab_case_ones_the_frontend_listens_for() {
        let created_event = serde_json::to_value(AppEvent::WorktreeCreated(created())).unwrap();
        let removed_event = serde_json::to_value(AppEvent::WorktreeRemoved(removed())).unwrap();
        assert_eq!(
            created_event["event"],
            serde_json::json!("worktree-created")
        );
        assert_eq!(
            removed_event["event"],
            serde_json::json!("worktree-removed")
        );
        // The tagged form nests the very same payload under `payload`, so a
        // WebSocket consumer of the bus and an SSE consumer see one shape.
        assert_eq!(
            created_event["payload"],
            serde_json::to_value(created()).unwrap()
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_msg(id: &str) -> AgentMessage {
        AgentMessage {
            id: id.to_string(),
            from_tuic_session: "sender".to_string(),
            from_name: "sender".to_string(),
            content: "x".to_string(),
            timestamp: 1,
            delivered_via_channel: false,
        }
    }

    fn lifecycle_msg(child: &str, kind: &str, state: &str, id: &str) -> AgentMessage {
        AgentMessage {
            id: format!("tuic-auto-{id}"),
            from_tuic_session: child.to_string(),
            from_name: "tuic".to_string(),
            content: serde_json::json!({"type": kind, "state": state}).to_string(),
            timestamp: 1,
            delivered_via_channel: false,
        }
    }

    // ── scrollback_reflow: the config toggle reaches the grids ──

    /// Fill a buffer's scrollback, then shrink it. With reflow the wrapped rows
    /// are re-split and history grows; without it they are truncated in place.
    fn history_after_shrink(vt: &mut VtLogBuffer) -> (usize, usize) {
        vt.process(b"AAAAAAAAAABBBBBBBBBB\r\n");
        vt.process(b"CCCCCCCCCCDDDDDDDDDD\r\n");
        vt.process(b"EEEEEEEEEEFFFFFFFFFF\r\n");
        vt.process(b"line4\r\nline5\r\nline6");
        let before = vt.grid_history_size();
        vt.resize(3, 10);
        (before, vt.grid_history_size())
    }

    #[test]
    fn new_vt_log_buffer_honors_scrollback_reflow_off() {
        let state = tests_support::make_test_app_state();
        state.config.write().scrollback_reflow = false;

        let mut vt = state.new_vt_log_buffer(3, 20, 4096);
        let (before, after) = history_after_shrink(&mut vt);

        assert_eq!(
            after, before,
            "with the toggle off a resize must truncate history, not reflow it"
        );
    }

    #[test]
    fn new_vt_log_buffer_honors_scrollback_reflow_on() {
        let state = tests_support::make_test_app_state();
        state.config.write().scrollback_reflow = true;

        let mut vt = state.new_vt_log_buffer(3, 20, 4096);
        let (before, after) = history_after_shrink(&mut vt);

        assert!(
            after > before,
            "with the toggle on a shrink must re-split history rows: {before} -> {after}"
        );
    }

    /// The toggle has to reach sessions that already exist — a config change that
    /// only affects the next session is the bug this story was opened for.
    #[test]
    fn apply_reflow_history_reaches_live_buffers() {
        let state = tests_support::make_test_app_state();
        state.config.write().scrollback_reflow = true;
        state.grid.vt_log_buffers.insert(
            "s1".to_string(),
            Mutex::new(state.new_vt_log_buffer(3, 20, 4096)),
        );

        state.apply_reflow_history(false);

        let buffer = state.grid.vt_log_buffers.get("s1").expect("buffer exists");
        let (before, after) = history_after_shrink(&mut buffer.lock());
        assert_eq!(
            after, before,
            "a live buffer must pick up the toggle without being recreated"
        );
    }

    // ── push_agent_inbox: one FIFO for peer and lifecycle mail ──

    #[test]
    fn lifecycle_churn_for_three_children_preserves_unread_peer_result() {
        let state = tests_support::make_test_app_state();
        let recipient = "orchestrator";
        state.push_agent_inbox(recipient, make_msg("peer-result"));

        for index in 0..150 {
            let child = format!("child-{}", index % 3);
            let notice = lifecycle_msg(&child, "state_change", "working", &index.to_string());
            state.push_agent_inbox(recipient, notice);
        }

        let (messages, has_more, missed_count) = state.observe_agent_inbox(recipient, 0, 100);
        assert!(!has_more);
        assert_eq!(missed_count, 0);
        assert_eq!(messages.len(), 4);
        assert_eq!(messages[0].id, "peer-result");
        for child in 0..3 {
            assert!(messages.iter().any(|message| {
                message.from_tuic_session == format!("child-{child}")
                    && message.id == format!("tuic-auto-{}", 147 + child)
            }));
        }
    }

    #[test]
    fn lifecycle_replacement_is_scoped_to_child_and_kind() {
        let state = tests_support::make_test_app_state();
        let recipient = "orchestrator";
        for notice in [
            lifecycle_msg("child-a", "state_change", "working", "a-working"),
            lifecycle_msg("child-b", "state_change", "idle", "b-idle"),
            lifecycle_msg("child-a", "prompt_delivered", "done", "a-prompt"),
            lifecycle_msg("child-a", "state_change", "idle", "a-idle"),
        ] {
            state.push_agent_inbox(recipient, notice);
        }

        let (messages, _, missed_count) = state.observe_agent_inbox(recipient, 0, 100);
        assert_eq!(missed_count, 0);
        assert_eq!(messages.len(), 3);
        assert_eq!(messages[0].id, "tuic-auto-b-idle");
        assert_eq!(messages[1].id, "tuic-auto-a-prompt");
        assert_eq!(messages[2].id, "tuic-auto-a-idle");
    }

    #[test]
    fn separate_confident_questions_remain_visible_after_state_churn() {
        let state = tests_support::make_test_app_state();
        let recipient = "orchestrator";
        for notice in [
            lifecycle_msg(
                "child-a",
                "state_change",
                "awaiting_input",
                "first-question",
            ),
            lifecycle_msg("child-a", "state_change", "working", "working"),
            lifecycle_msg(
                "child-a",
                "state_change",
                "awaiting_input",
                "second-question",
            ),
        ] {
            state.push_agent_inbox(recipient, notice);
        }

        let (messages, has_more, missed_count) = state.observe_agent_inbox(recipient, 0, 100);
        assert!(!has_more);
        assert_eq!(missed_count, 0);
        assert_eq!(
            messages
                .iter()
                .map(|message| message.id.as_str())
                .collect::<Vec<_>>(),
            [
                "tuic-auto-first-question",
                "tuic-auto-working",
                "tuic-auto-second-question",
            ],
            "a later state change must not erase either question"
        );
    }

    #[test]
    fn peer_only_overflow_keeps_fifo_and_counts_unread_loss() {
        let state = tests_support::make_test_app_state();
        let recipient = "orchestrator";
        for index in 0..101 {
            state.push_agent_inbox(recipient, make_msg(&format!("peer-{index}")));
        }

        let (messages, has_more, missed_count) = state.observe_agent_inbox(recipient, 0, 100);
        assert!(!has_more);
        assert_eq!(missed_count, 1);
        assert_eq!(messages.len(), 100);
        assert_eq!(messages[0].id, "peer-1");
        assert_eq!(messages[99].id, "peer-100");
    }

    #[test]
    fn replaced_lifecycle_notice_releases_existing_delivery_owners() {
        let state = tests_support::make_test_app_state();
        let recipient = "orchestrator";
        let first = lifecycle_msg("child-a", "state_change", "working", "first");
        state.push_agent_inbox(recipient, first);
        let lease = state.begin_agent_wait(recipient);
        assert_eq!(state.waiter_fresh_message_count(recipient, 0), 1);
        assert_eq!(
            state.finish_agent_wait(recipient, lease, 0, true).messages[0].id,
            "tuic-auto-first"
        );
        state.push_agent_inbox(
            recipient,
            lifecycle_msg("child-a", "state_change", "idle", "second"),
        );
        assert_eq!(
            state.agent_delivery_owner(recipient, "tuic-auto-first"),
            None
        );

        assert_eq!(
            state.assign_agent_delivery(recipient, "tuic-auto-second", true),
            AgentDeliveryAssignment::Terminal
        );
        state.mark_terminal_delivery_dispatched(recipient, "tuic-auto-second");
        state.push_agent_inbox(
            recipient,
            lifecycle_msg("child-a", "state_change", "completed", "third"),
        );

        let (messages, _, missed_count) = state.observe_agent_inbox(recipient, 0, 100);
        assert_eq!(messages.len(), 1);
        assert_eq!(messages[0].id, "tuic-auto-third");
        assert_eq!(missed_count, 0);
        assert_eq!(
            state.agent_delivery_owner(recipient, "tuic-auto-second"),
            None
        );
    }

    #[test]
    fn push_agent_inbox_evicts_oldest_across_peer_and_lifecycle_mail() {
        let state = tests_support::make_test_app_state();
        let rcpt = "orchestrator";

        state.push_agent_inbox(rcpt, make_msg("oldest-peer"));
        for i in 0..(AGENT_INBOX_CAPACITY - 1) {
            state.push_agent_inbox(rcpt, make_msg(&format!("tuic-auto-state-{i}")));
        }
        state.push_agent_inbox(rcpt, make_msg("tuic-auto-state-overflow"));

        let inbox = state.agent_inbox.get(rcpt).expect("inbox exists");
        assert_eq!(inbox.len(), AGENT_INBOX_CAPACITY);
        assert!(
            !inbox.iter().any(|m| m.id == "oldest-peer"),
            "the oldest peer message must be evicted before newer lifecycle mail"
        );
        assert!(
            inbox.iter().any(|m| m.id == "tuic-auto-state-0"),
            "a newer lifecycle notice must remain"
        );
        assert!(
            *state.agent_inbox_evictions.get(rcpt).unwrap() > 0,
            "genuine evictions must bump the missed-count counter"
        );
    }

    #[test]
    fn push_agent_inbox_evicts_oldest_when_all_lifecycle() {
        let state = tests_support::make_test_app_state();
        let rcpt = "orchestrator";

        // Inbox entirely lifecycle: fallback evicts the oldest to keep the bound.
        for i in 0..=AGENT_INBOX_CAPACITY {
            state.push_agent_inbox(rcpt, make_msg(&format!("tuic-auto-{i}")));
        }

        let inbox = state.agent_inbox.get(rcpt).expect("inbox exists");
        assert_eq!(inbox.len(), AGENT_INBOX_CAPACITY);
        // Oldest (index 0) evicted; the newest is retained.
        assert_eq!(inbox.front().unwrap().id, "tuic-auto-1");
        assert_eq!(
            inbox.back().unwrap().id,
            format!("tuic-auto-{AGENT_INBOX_CAPACITY}")
        );
    }

    #[test]
    fn push_agent_inbox_evicts_oldest_terminal_pending_mail() {
        let state = tests_support::make_test_app_state();
        let recipient = "peer";

        state.push_agent_inbox(recipient, make_msg("terminal-pending"));
        assert_eq!(
            state.assign_agent_delivery(recipient, "terminal-pending", true),
            AgentDeliveryAssignment::Terminal
        );
        for index in 0..(AGENT_INBOX_CAPACITY - 1) {
            state.push_agent_inbox(recipient, make_msg(&format!("returned-{index}")));
        }

        state.push_agent_inbox(recipient, make_msg("overflow"));

        assert!(
            !state
                .agent_inbox
                .get(recipient)
                .is_some_and(|inbox| inbox.iter().any(|message| message.id == "terminal-pending")),
            "the oldest message must leave the bounded inbox"
        );
        assert_eq!(
            state.agent_delivery_owner(recipient, "terminal-pending"),
            None
        );
    }

    #[test]
    fn push_agent_inbox_evicts_peer_only_overflow_after_waiter_delivery() {
        let state = tests_support::make_test_app_state();
        let recipient = "peer";

        state.push_agent_inbox(recipient, make_msg("terminal-pending"));
        assert_eq!(
            state.assign_agent_delivery(recipient, "terminal-pending", true),
            AgentDeliveryAssignment::Terminal
        );
        state.push_agent_inbox(recipient, make_msg("waiter-observed"));
        let lease = state.begin_agent_wait(recipient);
        assert_eq!(state.waiter_fresh_message_count(recipient, 0), 1);
        assert_eq!(
            state.finish_agent_wait(recipient, lease, 0, true).messages[0].id,
            "waiter-observed"
        );
        for index in 0..(AGENT_INBOX_CAPACITY - 2) {
            state.push_agent_inbox(recipient, make_msg(&format!("returned-{index}")));
        }

        state.push_agent_inbox(recipient, make_msg("overflow"));

        let inbox = state.agent_inbox.get(recipient).expect("inbox exists");
        assert!(!inbox.iter().any(|message| message.id == "terminal-pending"));
        assert!(
            inbox.iter().any(|message| message.id == "waiter-observed"),
            "newer mail remains after the oldest is evicted"
        );
    }

    #[test]
    fn push_agent_inbox_accepts_overflow_when_every_message_is_in_flight() {
        let state = tests_support::make_test_app_state();
        let recipient = "peer";

        for index in 0..AGENT_INBOX_CAPACITY {
            let message_id = format!("pending-{index}");
            state.push_agent_inbox(recipient, make_msg(&message_id));
            assert_eq!(
                state.assign_agent_delivery(recipient, &message_id, true),
                AgentDeliveryAssignment::Terminal
            );
        }

        state.push_agent_inbox(recipient, make_msg("overflow"));
        let inbox = state.agent_inbox.get(recipient).expect("inbox exists");
        assert_eq!(inbox.len(), AGENT_INBOX_CAPACITY);
        assert_eq!(inbox.front().unwrap().id, "pending-1");
        assert_eq!(inbox.back().unwrap().id, "overflow");
    }

    #[test]
    fn inbox_eviction_is_per_recipient_and_paging_skips_only_evicted_mail() {
        let state = tests_support::make_test_app_state();
        state.push_agent_inbox("other", make_msg("other-oldest"));
        for index in 0..AGENT_INBOX_CAPACITY {
            state.push_agent_inbox("peer", make_msg(&format!("mail-{index}")));
        }
        let (first, has_more, missed_count) = state.observe_agent_inbox("peer", 0, 2);
        assert!(has_more);
        assert_eq!(missed_count, 0);
        assert_eq!(
            first.iter().map(|m| m.id.as_str()).collect::<Vec<_>>(),
            ["mail-0", "mail-1"]
        );
        for index in AGENT_INBOX_CAPACITY..(AGENT_INBOX_CAPACITY + 3) {
            state.push_agent_inbox("peer", make_msg(&format!("mail-{index}")));
        }
        let cursor = first.last().unwrap().timestamp;
        let (second, has_more, missed_count) = state.observe_agent_inbox("peer", cursor, 2);
        assert!(has_more);
        assert_eq!(missed_count, 1);
        assert_eq!(
            second.iter().map(|m| m.id.as_str()).collect::<Vec<_>>(),
            ["mail-3", "mail-4"]
        );
        assert_eq!(
            state.agent_inbox.get("other").unwrap()[0].id,
            "other-oldest"
        );
        assert!(!state.agent_inbox_evictions.contains_key("peer"));
        assert!(!state.agent_inbox_evictions.contains_key("other"));
    }

    #[test]
    fn urgent_notice_reservation_survives_partial_read_and_clears_after_eviction() {
        let state = tests_support::make_test_app_state();
        let recipient = "peer";
        let first_through = state.push_agent_inbox(recipient, make_msg("urgent-0"));
        assert_eq!(
            state.reserve_urgent_notice(recipient, "sender", first_through),
            UrgentNoticeReservation::Reserved
        );
        state.finish_urgent_notice(recipient, "sender", first_through, true);
        let second_through = state.push_agent_inbox(recipient, make_msg("urgent-1"));
        assert_eq!(
            state.reserve_urgent_notice(recipient, "sender", second_through),
            UrgentNoticeReservation::Written
        );
        state.observe_agent_inbox(recipient, 0, 1);
        let third_through = state.push_agent_inbox(recipient, make_msg("urgent-2"));
        assert_eq!(
            state.reserve_urgent_notice(recipient, "sender", third_through),
            UrgentNoticeReservation::Written
        );
        state.observe_agent_inbox(recipient, first_through, 2);
        let fourth_through = state.push_agent_inbox(recipient, make_msg("urgent-3"));
        assert_eq!(
            state.reserve_urgent_notice(recipient, "sender", fourth_through),
            UrgentNoticeReservation::Reserved
        );

        for index in 0..AGENT_INBOX_CAPACITY {
            state.push_agent_inbox(recipient, make_msg(&format!("filler-{index}")));
        }
        let after_eviction = state.push_agent_inbox(recipient, make_msg("urgent-after-eviction"));
        assert_eq!(
            state.reserve_urgent_notice(recipient, "sender", after_eviction),
            UrgentNoticeReservation::Reserved
        );
    }

    #[test]
    fn push_agent_inbox_reclaims_read_mail_and_settles_delivery_leases() {
        let state = tests_support::make_test_app_state();
        let recipient = "peer";
        state.push_agent_inbox("other-peer", make_msg("other-mail"));

        state.push_agent_inbox(recipient, make_msg("terminal-pending"));
        assert_eq!(
            state.assign_agent_delivery(recipient, "terminal-pending", true),
            AgentDeliveryAssignment::Terminal
        );
        let lease = state.begin_agent_wait(recipient);
        state.push_agent_inbox(recipient, make_msg("waiter-owned"));
        assert_eq!(
            state.assign_agent_delivery(recipient, "waiter-owned", true),
            AgentDeliveryAssignment::Waiter
        );
        state.push_agent_inbox(recipient, make_msg("consumed"));
        let (observed, has_more, missed_count) = state.observe_agent_inbox(recipient, 0, 3);
        assert!(!has_more);
        assert_eq!(missed_count, 0);
        assert_eq!(observed.len(), 3);
        assert_eq!(observed[2].id, "consumed");

        for index in 0..(AGENT_INBOX_CAPACITY - 3) {
            state.push_agent_inbox(recipient, make_msg(&format!("unread-{index}")));
        }
        state.push_agent_inbox(recipient, make_msg("new-mail"));

        let inbox = state.agent_inbox.get(recipient).expect("inbox exists");
        assert_eq!(inbox.len(), AGENT_INBOX_CAPACITY);
        assert!(inbox.iter().all(|message| message.id != "terminal-pending"));
        assert!(inbox.iter().any(|message| message.id == "waiter-owned"));
        assert!(inbox.iter().any(|message| message.id == "consumed"));
        assert!(inbox.iter().any(|message| message.id == "unread-0"));
        assert!(inbox.iter().any(|message| message.id == "new-mail"));
        assert!(!state.agent_inbox_evictions.contains_key(recipient));
        assert_eq!(
            state.agent_inbox.get("other-peer").unwrap()[0].id,
            "other-mail"
        );
        drop(inbox);
        let finish = state.finish_agent_wait(recipient, lease, 0, false);
        assert!(finish.terminal_handoff.is_empty());
    }

    #[test]
    fn terminal_pending_mail_requeues_after_eviction_pressure() {
        let state = tests_support::make_test_app_state();
        let recipient = "peer";

        state.push_agent_inbox(recipient, make_msg("oldest-lifecycle"));
        state.push_agent_inbox(recipient, make_msg("terminal-pending"));
        assert_eq!(
            state.assign_agent_delivery(recipient, "terminal-pending", true),
            AgentDeliveryAssignment::Terminal
        );
        for index in 0..(AGENT_INBOX_CAPACITY - 2) {
            state.push_agent_inbox(recipient, make_msg(&format!("tuic-auto-state-{index}")));
        }
        state.push_agent_inbox(recipient, make_msg("tuic-auto-state-overflow"));

        state.release_terminal_delivery(recipient, "terminal-pending");

        let inbox = state.agent_inbox.get(recipient).expect("inbox exists");
        assert_eq!(
            inbox.back().map(|message| message.id.as_str()),
            Some("terminal-pending")
        );
        assert_eq!(
            state.agent_delivery_owner(recipient, "terminal-pending"),
            None
        );
    }

    #[test]
    fn push_agent_inbox_assigns_monotonic_logical_timestamps() {
        let state = tests_support::make_test_app_state();
        let recipient = "orchestrator";
        for id in ["first", "second", "third"] {
            state.push_agent_inbox(recipient, make_msg(id));
        }

        let inbox = state.agent_inbox.get(recipient).unwrap();
        let timestamps: Vec<_> = inbox.iter().map(|message| message.timestamp).collect();
        assert_eq!(timestamps, vec![1, 2, 3]);
    }

    #[test]
    fn push_agent_inbox_timestamp_saturation_never_wraps() {
        let state = tests_support::make_test_app_state();
        let recipient = "orchestrator";
        state.push_agent_inbox(
            recipient,
            AgentMessage {
                timestamp: u64::MAX,
                ..make_msg("ceiling")
            },
        );
        state.push_agent_inbox(recipient, make_msg("after-ceiling"));

        let inbox = state.agent_inbox.get(recipient).unwrap();
        assert_eq!(inbox[0].timestamp, u64::MAX);
        assert_eq!(inbox[1].timestamp, u64::MAX);
    }

    #[test]
    fn active_agent_waiters_are_reference_counted() {
        let state = tests_support::make_test_app_state();
        let first = state.begin_agent_wait("peer");
        let second = state.begin_agent_wait("peer");
        assert!(state.has_active_agent_waiter("peer"));
        state.finish_agent_wait("peer", first, 0, true);
        assert!(state.has_active_agent_waiter("peer"));
        state.finish_agent_wait("peer", second, 0, true);
        assert!(!state.has_active_agent_waiter("peer"));
    }

    // ---- marker compliance counters (#4421) ----

    #[test]
    fn marker_tallies_are_independent_per_session() {
        let state = tests_support::make_test_app_state();
        state.note_marker("s1", MarkerKind::TurnSubmitted);
        state.note_marker("s1", MarkerKind::Intent);
        state.note_marker("s1", MarkerKind::Suggest);
        state.note_marker("s1", MarkerKind::Suggest);
        state.note_marker("s2", MarkerKind::TurnSubmitted);

        assert_eq!(
            state.marker_stats_for("s1"),
            MarkerStats {
                intent: 1,
                suggest: 2,
                turns: 1
            }
        );
        assert_eq!(
            state.marker_stats_for("s2"),
            MarkerStats {
                intent: 0,
                suggest: 0,
                turns: 1
            },
            "a turn with no markers is the case this counter exists to see"
        );
    }

    #[test]
    fn an_unseen_session_reports_zeroes_rather_than_nothing() {
        let state = tests_support::make_test_app_state();
        // Absent and all-zero are deliberately the same shape: a session that
        // never ran a turn is not evidence of an agent ignoring the protocol.
        assert_eq!(state.marker_stats_for("never-ran"), MarkerStats::default());
    }

    #[test]
    fn a_malformed_marker_never_reaches_the_tally() {
        let state = tests_support::make_test_app_state();
        state.note_marker("s1", MarkerKind::TurnSubmitted);
        // Nothing else is recorded: the counter is fed from parsed ParsedEvents,
        // so a line the parser rejected (`suggest: A | B` with no brackets, or a
        // 4-item list) contributes no event and therefore no count. This asserts
        // the denominator still moves, which is what makes the miss visible.
        assert_eq!(
            state.marker_stats_for("s1"),
            MarkerStats {
                intent: 0,
                suggest: 0,
                turns: 1
            }
        );
    }

    #[test]
    fn lifecycle_summary_notice_acknowledges_its_own_window() {
        let state = tests_support::make_test_app_state();
        let recipient = "orchestrator";
        assert_eq!(
            state.assign_orchestrator_delivery_with_wake_outcome(
                recipient,
                "first",
                10,
                true,
                |group| {
                    assert_eq!(
                        group,
                        OrchestratorWakeGroup {
                            observed_through: 0,
                            wake_through: 10
                        }
                    );
                    OrchestratorWakeAttemptOutcome::SummarySubmitted
                },
            ),
            OrchestratorDeliveryAssignment::WakeSummarySubmitted
        );
        assert_eq!(
            state.orchestrator_wake_needed_through(recipient),
            None,
            "a summary that printed the whole window leaves nothing to chase"
        );
        assert_eq!(
            state.assign_orchestrator_delivery_with_wake_outcome(
                recipient,
                "first-again",
                10,
                true,
                |_| panic!("an acknowledged window must never wake again"),
            ),
            OrchestratorDeliveryAssignment::InboxOnly
        );
    }

    #[test]
    fn mail_coalesced_during_a_summary_write_stays_outstanding() {
        let state = tests_support::make_test_app_state();
        let recipient = "orchestrator";
        let assignment = state.assign_orchestrator_delivery_with_wake_outcome(
            recipient,
            "first",
            10,
            true,
            |group| {
                assert_eq!(group.wake_through, 10);
                // Lands while the summary is being typed: outside the reserved
                // window, so that notice cannot possibly describe it.
                assert_eq!(
                    state.assign_orchestrator_delivery_with_wake_outcome(
                        recipient,
                        "second",
                        20,
                        true,
                        |_| panic!("a pending notice must cover later mail"),
                    ),
                    OrchestratorDeliveryAssignment::WakeCoalesced
                );
                OrchestratorWakeAttemptOutcome::SummarySubmitted
            },
        );
        assert_eq!(
            assignment,
            OrchestratorDeliveryAssignment::WakeSummarySubmitted
        );
        assert_eq!(
            state.orchestrator_wake_needed_through(recipient),
            Some(20),
            "an uncovered message must not be acknowledged by someone else's summary"
        );
        assert_eq!(
            state.assign_orchestrator_delivery_with_wake_outcome(
                recipient,
                "second-retry",
                20,
                true,
                |group| {
                    assert_eq!(
                        group,
                        OrchestratorWakeGroup {
                            observed_through: 10,
                            wake_through: 20
                        }
                    );
                    OrchestratorWakeAttemptOutcome::Submitted
                },
            ),
            OrchestratorDeliveryAssignment::WakeSubmitted
        );
    }

    #[test]
    fn uncertain_summary_write_never_advances_the_cursor() {
        let state = tests_support::make_test_app_state();
        let recipient = "orchestrator";
        assert_eq!(
            state.assign_orchestrator_delivery_with_wake_outcome(
                recipient,
                "first",
                10,
                true,
                |_| OrchestratorWakeAttemptOutcome::Uncertain,
            ),
            OrchestratorDeliveryAssignment::InboxOnly
        );
        assert_eq!(state.orchestrator_wake_needed_through(recipient), Some(10));
        assert_eq!(
            state.assign_orchestrator_delivery_with_wake_outcome(
                recipient,
                "retry",
                10,
                true,
                |group| {
                    assert_eq!(
                        group.observed_through, 0,
                        "an ambiguous write proves nothing reached the screen"
                    );
                    OrchestratorWakeAttemptOutcome::SummarySubmitted
                },
            ),
            OrchestratorDeliveryAssignment::WakeSummarySubmitted
        );
    }

    #[test]
    fn orchestrator_wake_coalesces_until_inbox_cursor_catches_up() {
        let state = tests_support::make_test_app_state();
        let recipient = "orchestrator";
        assert_eq!(
            state.assign_orchestrator_delivery_with_wake_attempt(
                recipient,
                "first",
                10,
                true,
                || true,
            ),
            OrchestratorDeliveryAssignment::WakeSubmitted
        );
        assert_eq!(
            state.assign_orchestrator_delivery_with_wake_attempt(
                recipient,
                "second",
                20,
                false,
                || panic!("a pending wake must cover later mail"),
            ),
            OrchestratorDeliveryAssignment::WakeCoalesced
        );

        state.acknowledge_orchestrator_wake(recipient, 10);
        assert_eq!(
            state.assign_orchestrator_delivery_with_wake_attempt(
                recipient,
                "third",
                30,
                false,
                || panic!("a partial inbox read must not clear the pending wake"),
            ),
            OrchestratorDeliveryAssignment::WakeCoalesced
        );

        state.acknowledge_orchestrator_wake(recipient, 30);
        assert_eq!(
            state.assign_orchestrator_delivery_with_wake_attempt(
                recipient,
                "fourth",
                40,
                true,
                || true,
            ),
            OrchestratorDeliveryAssignment::WakeSubmitted
        );
    }

    #[test]
    fn orchestrator_mail_while_working_is_retried_at_idle_without_payload_wake() {
        let state = tests_support::make_test_app_state();
        let recipient = "orchestrator";
        state.push_agent_inbox(recipient, make_msg("working-mail"));
        assert_eq!(
            state.assign_orchestrator_delivery_with_wake_attempt(
                recipient,
                "working-mail",
                1,
                false,
                || panic!("busy must not wake")
            ),
            OrchestratorDeliveryAssignment::InboxOnly
        );
        let wakes = std::cell::Cell::new(0);
        assert_eq!(
            state.assign_orchestrator_delivery_with_wake_attempt(
                recipient,
                "working-mail",
                1,
                true,
                || {
                    wakes.set(wakes.get() + 1);
                    true
                }
            ),
            OrchestratorDeliveryAssignment::WakeSubmitted
        );
        assert_eq!(wakes.get(), 1);
    }

    /// A wake that could not start must not silence the idle-edge retry.
    ///
    /// `wake_allowed` is decided from the canonical lifecycle, which reads idle
    /// while the composer still refuses an injection. Burning the group budget
    /// on that failure is right for the unchanged lifecycle and wrong forever
    /// after: nothing else announces the mail.
    #[test]
    fn a_wake_that_never_started_is_retried_at_the_next_idle_edge() {
        let state = tests_support::make_test_app_state();
        let recipient = "orchestrator";
        state.push_agent_inbox(recipient, make_msg("stranded"));
        assert_eq!(
            state.assign_orchestrator_delivery_with_wake_outcome(
                recipient,
                "stranded",
                1,
                true,
                |_group| OrchestratorWakeAttemptOutcome::NotStarted
            ),
            OrchestratorDeliveryAssignment::InboxOnly
        );
        assert_eq!(
            state.assign_orchestrator_delivery_with_wake_outcome(
                recipient,
                "stranded",
                1,
                true,
                |_group| panic!("an unchanged lifecycle must not be reclaimed")
            ),
            OrchestratorDeliveryAssignment::InboxOnly
        );

        state.rearm_orchestrator_wake_budget(recipient);
        let wakes = std::cell::Cell::new(0);
        assert_eq!(
            state.assign_orchestrator_delivery_with_wake_outcome(
                recipient,
                "stranded",
                1,
                true,
                |_group| {
                    wakes.set(wakes.get() + 1);
                    OrchestratorWakeAttemptOutcome::Submitted
                }
            ),
            OrchestratorDeliveryAssignment::WakeSubmitted,
            "a new idle edge must be able to announce mail a failed attempt left stranded"
        );
        assert_eq!(wakes.get(), 1);
    }

    /// The re-arm may not steal a reservation from the notice being typed.
    #[test]
    fn rearming_never_disturbs_a_wake_in_flight() {
        let state = tests_support::make_test_app_state();
        let recipient = "orchestrator";
        state.push_agent_inbox(recipient, make_msg("in-flight"));
        let assignment = state.assign_orchestrator_delivery_with_wake_outcome(
            recipient,
            "in-flight",
            1,
            true,
            |_group| {
                // Mid-write: a concurrent idle edge lands here.
                state.rearm_orchestrator_wake_budget(recipient);
                OrchestratorWakeAttemptOutcome::NotStarted
            },
        );
        assert_eq!(assignment, OrchestratorDeliveryAssignment::InboxOnly);
        assert_eq!(
            state.assign_orchestrator_delivery_with_wake_outcome(
                recipient,
                "in-flight",
                1,
                true,
                |_group| panic!("the in-flight attempt still owns the budget")
            ),
            OrchestratorDeliveryAssignment::InboxOnly
        );
    }

    #[test]
    fn empty_and_cursor_inbox_reads_acknowledge_pending_wake() {
        let state = tests_support::make_test_app_state();
        let recipient = "orchestrator";
        assert_eq!(
            state.assign_orchestrator_delivery_with_wake_attempt(recipient, "m", 10, true, || true),
            OrchestratorDeliveryAssignment::WakeSubmitted
        );
        state.acknowledge_orchestrator_wake(recipient, 0);
        assert_eq!(
            state.assign_orchestrator_delivery_with_wake_attempt(
                recipient,
                "new",
                20,
                true,
                || true
            ),
            OrchestratorDeliveryAssignment::WakeSubmitted
        );
    }

    #[test]
    fn coalesced_mail_remains_visible_to_a_later_wait() {
        let state = tests_support::make_test_app_state();
        let recipient = "orchestrator";
        state.push_agent_inbox(recipient, make_msg("coalesced"));
        assert_eq!(
            state.assign_orchestrator_delivery_with_wake_attempt(
                recipient,
                "coalesced",
                1,
                true,
                || true
            ),
            OrchestratorDeliveryAssignment::WakeSubmitted
        );
        let lease = state.begin_agent_wait(recipient);
        assert_eq!(
            state.waiter_fresh_message_count(recipient, 0),
            1,
            "wake ownership must not hide inbox mail"
        );
        state.finish_agent_wait(recipient, lease, 0, true);
    }

    #[test]
    fn failed_wake_attempt_can_be_retried_after_stale_pending_state() {
        let state = tests_support::make_test_app_state();
        let recipient = "orchestrator";
        assert_eq!(
            state.assign_orchestrator_delivery_with_wake_attempt(recipient, "m", 1, true, || false),
            OrchestratorDeliveryAssignment::InboxOnly
        );
        assert_eq!(
            state.assign_orchestrator_delivery_with_wake_attempt(recipient, "m", 1, true, || true),
            OrchestratorDeliveryAssignment::WakeSubmitted
        );
    }

    #[test]
    fn uncertain_payload_free_wake_has_one_retry_until_mail_is_acknowledged() {
        let state = tests_support::make_test_app_state();
        let recipient = "orchestrator";
        let attempts = std::cell::Cell::new(0);
        let uncertain_wake = || {
            attempts.set(attempts.get() + 1);
            false
        };

        // Initial wake and the first deterministic expiry retry are allowed.
        assert_eq!(
            state.assign_orchestrator_delivery_with_wake_attempt(
                recipient,
                "mail-1",
                10,
                true,
                uncertain_wake,
            ),
            OrchestratorDeliveryAssignment::InboxOnly
        );
        assert_eq!(
            state.assign_orchestrator_delivery_with_wake_attempt(
                recipient,
                "mail-1",
                10,
                true,
                || {
                    attempts.set(attempts.get() + 1);
                    false
                },
            ),
            OrchestratorDeliveryAssignment::InboxOnly
        );
        assert_eq!(attempts.get(), 2);

        // A second uncertain result exhausts the budget. Later idle/expiry
        // reevaluations, including coalesced mail, must remain inbox-only.
        assert_eq!(
            state.assign_orchestrator_delivery_with_wake_attempt(
                recipient,
                "mail-1",
                10,
                true,
                || panic!("uncertain wake retry budget must be exhausted"),
            ),
            OrchestratorDeliveryAssignment::InboxOnly
        );
        assert_eq!(
            state.assign_orchestrator_delivery_with_wake_attempt(
                recipient,
                "mail-2",
                20,
                true,
                || panic!("coalesced mail must not reset uncertain wake budget"),
            ),
            OrchestratorDeliveryAssignment::InboxOnly
        );
        assert_eq!(attempts.get(), 2);

        // An authoritative observation clears the group; later mail gets a
        // fresh initial wake plus one retry budget.
        state.acknowledge_orchestrator_wake(recipient, 20);
        assert_eq!(
            state.assign_orchestrator_delivery_with_wake_attempt(
                recipient,
                "mail-3",
                30,
                true,
                || {
                    attempts.set(attempts.get() + 1);
                    false
                },
            ),
            OrchestratorDeliveryAssignment::InboxOnly
        );
        assert_eq!(attempts.get(), 3);
    }

    #[test]
    fn active_waiter_owns_orchestrator_mail_without_wake_attempt() {
        let state = tests_support::make_test_app_state();
        let recipient = "orchestrator";
        let lease = state.begin_agent_wait(recipient);
        state.push_agent_inbox(recipient, make_msg("message"));
        assert_eq!(
            state.assign_orchestrator_delivery_with_wake_attempt(
                recipient,
                "message",
                10,
                true,
                || panic!("an active waiter must suppress the wake"),
            ),
            OrchestratorDeliveryAssignment::Waiter
        );
        state.finish_agent_wait(recipient, lease, 0, true);
    }

    #[test]
    fn waiter_send_handoff_assigns_exactly_one_owner_in_both_deadline_orders() {
        let state = tests_support::make_test_app_state();
        let recipient = "peer";

        let lease = state.begin_agent_wait(recipient);
        state.push_agent_inbox(recipient, make_msg("during-wait"));
        assert_eq!(
            state.assign_agent_delivery(recipient, "during-wait", true),
            AgentDeliveryAssignment::Waiter
        );
        assert_eq!(state.waiter_fresh_message_count(recipient, 0), 1);
        let finish = state.finish_agent_wait(recipient, lease, 0, true);
        assert_eq!(finish.fresh_count, 1);
        assert_eq!(finish.messages, vec![make_msg("during-wait")]);
        assert!(finish.terminal_handoff.is_empty());
        assert_eq!(
            state.agent_delivery_owner(recipient, "during-wait"),
            Some(AgentDeliveryOwner::WaiterObserved)
        );

        let lease = state.begin_agent_wait(recipient);
        assert!(
            state
                .finish_agent_wait(recipient, lease, 1, false)
                .terminal_handoff
                .is_empty()
        );
        state.push_agent_inbox(
            recipient,
            AgentMessage {
                timestamp: 2,
                ..make_msg("after-timeout")
            },
        );
        assert_eq!(
            state.assign_agent_delivery(recipient, "after-timeout", true),
            AgentDeliveryAssignment::Terminal
        );
        assert_eq!(
            state.agent_delivery_owner(recipient, "after-timeout"),
            Some(AgentDeliveryOwner::TerminalPending)
        );
    }

    #[test]
    /// A bridge that reconnects leaves its old `agent wait` lease behind. Because
    /// any non-empty waiter set outranks terminal delivery, that dead lease kept
    /// winning `assign_agent_delivery` and the reconnected peer's PTY was never
    /// woken until the stale wait timed out.
    fn stale_waiter_lease_stops_blocking_terminal_delivery_after_reconnect() {
        let state = tests_support::make_test_app_state();
        let recipient = "peer";
        let _stale_lease = state.begin_agent_wait(recipient);

        assert_eq!(
            state.assign_agent_delivery(recipient, "before", true),
            AgentDeliveryAssignment::Waiter,
            "precondition: the stale lease wins while it is still registered"
        );

        state.revoke_waiters_for_reconnect(recipient);

        assert_eq!(
            state.assign_agent_delivery(recipient, "after", true),
            AgentDeliveryAssignment::Terminal,
            "the reconnected peer must be reachable through its terminal again"
        );
        assert_eq!(
            state.agent_delivery_owner(recipient, "before"),
            None,
            "a message the dead lease only claimed is handed back, not stranded"
        );
    }

    #[test]
    fn reconnect_revocation_keeps_messages_the_old_wait_already_returned() {
        let state = tests_support::make_test_app_state();
        let recipient = "peer";
        let lease = state.begin_agent_wait(recipient);
        state.push_agent_inbox(recipient, make_msg("already-seen"));
        // Observing marks it WaiterObserved — the agent has it.
        let _ = state.waiter_fresh_message_count(recipient, 0);
        state.finish_agent_wait(recipient, lease, 0, true);

        let observed = state.agent_delivery_owner(recipient, "already-seen");
        state.revoke_waiters_for_reconnect(recipient);

        assert_eq!(
            state.agent_delivery_owner(recipient, "already-seen"),
            observed,
            "re-delivering what the previous wait already returned would duplicate it"
        );
    }

    #[cfg(unix)]
    #[test]
    /// Boss's case: a Codex orchestrator running in a TUIC tab registers with its
    /// `$TUIC_SESSION`, which is not the key `create_pty` filed its PTY under. The
    /// old `sessions.contains_key(peer_id)` therefore reported "no terminal" and
    /// every subagent message came back `delivery_path: inbox_only` while still
    /// answering ok/accepted.
    fn peer_resolves_to_the_pty_backing_its_tuic_session() {
        let state = tests_support::make_test_app_state();
        let pty_key = "pty-key";
        let announced = "tuic-session-uuid";
        tests_support::insert_dummy_session(&state, pty_key);
        state.bind_live_pty(announced, pty_key);

        assert_eq!(
            state.live_pty_for_peer(announced),
            Some(pty_key.to_string()),
            "the identity an agent announces must resolve to the terminal behind it"
        );
        // A spawn-registered child is stamped with the PTY key itself; that must
        // keep working without a binding.
        assert_eq!(state.live_pty_for_peer(pty_key), Some(pty_key.to_string()));
        assert_eq!(state.live_pty_for_peer("never-seen"), None);
    }

    #[cfg(unix)]
    #[test]
    fn teardown_reports_the_identities_that_lost_their_terminal() {
        let state = tests_support::make_test_app_state();
        tests_support::insert_dummy_session(&state, "pty-key");
        state.bind_live_pty("announced-a", "pty-key");
        state.bind_live_pty("announced-b", "pty-key");
        state.bind_live_pty("other", "unrelated-pty");

        let mut orphaned = state.unbind_live_pty("pty-key");
        orphaned.sort();

        assert_eq!(
            orphaned,
            vec!["announced-a".to_string(), "announced-b".to_string()],
            "teardown must name the peer identities to retire — they are filed under \
             the announced identity, not the PTY key, so the existing removal missed them"
        );
        assert_eq!(state.live_pty_for_peer("other"), None);
    }

    #[cfg(unix)]
    #[test]
    fn a_binding_whose_pty_died_stops_resolving() {
        let state = tests_support::make_test_app_state();
        tests_support::insert_dummy_session(&state, "pty-key");
        state.bind_live_pty("tuic-session-uuid", "pty-key");

        state.unbind_live_pty("pty-key");

        assert_eq!(
            state.live_pty_for_peer("tuic-session-uuid"),
            None,
            "a stale binding must not offer a terminal that is gone"
        );
    }

    #[cfg(unix)]
    #[test]
    fn respawn_under_the_same_identity_resolves_to_the_new_pty() {
        let state = tests_support::make_test_app_state();
        let announced = "tuic-session-uuid";
        tests_support::insert_dummy_session(&state, "pty-old");
        state.bind_live_pty(announced, "pty-old");

        // Tab respawns: teardown unbinds, the new PTY binds the same identity.
        state.unbind_live_pty("pty-old");
        tests_support::insert_dummy_session(&state, "pty-new");
        state.bind_live_pty(announced, "pty-new");

        assert_eq!(
            state.live_pty_for_peer(announced),
            Some("pty-new".to_string()),
            "resolving per call is what lets a respawn be picked up with no re-register"
        );
    }

    #[test]
    fn delayed_delivery_assignment_preserves_waiter_observation() {
        let state = tests_support::make_test_app_state();
        let recipient = "peer";
        let lease = state.begin_agent_wait(recipient);
        state.push_agent_inbox(recipient, make_msg("claimed-before-assignment"));

        assert_eq!(state.waiter_fresh_message_count(recipient, 0), 1);
        let finish = state.finish_agent_wait(recipient, lease, 0, true);
        assert_eq!(finish.messages, vec![make_msg("claimed-before-assignment")]);
        assert_eq!(
            state.assign_agent_delivery(recipient, "claimed-before-assignment", true),
            AgentDeliveryAssignment::Waiter
        );
        assert_eq!(
            state.agent_delivery_owner(recipient, "claimed-before-assignment"),
            Some(AgentDeliveryOwner::WaiterObserved)
        );
    }

    #[test]
    fn channel_attempt_keeps_mail_available_for_wait_after_a_successful_push() {
        let state = tests_support::make_test_app_state();
        state.push_agent_inbox("failed-peer", make_msg("failed-sse"));
        assert_eq!(
            state.assign_agent_delivery_with_channel_attempt(
                "failed-peer",
                "failed-sse",
                false,
                || false,
            ),
            (AgentDeliveryAssignment::InboxOnly, false)
        );
        assert_eq!(
            state.agent_delivery_owner("failed-peer", "failed-sse"),
            None
        );

        let lease = state.begin_agent_wait("failed-peer");
        assert_eq!(state.waiter_fresh_message_count("failed-peer", 0), 1);
        let finish = state.finish_agent_wait("failed-peer", lease, 0, true);
        assert_eq!(finish.messages, vec![make_msg("failed-sse")]);

        state.push_agent_inbox("live-peer", make_msg("live-sse"));
        assert_eq!(
            state.assign_agent_delivery_with_channel_attempt(
                "live-peer",
                "live-sse",
                false,
                || true,
            ),
            (AgentDeliveryAssignment::Terminal, true)
        );
        assert_eq!(
            state.agent_delivery_owner("live-peer", "live-sse"),
            None,
            "an SSE push is not an inbox read and must remain visible to wait"
        );

        let waiter = state.begin_agent_wait("waiting-peer");
        state.push_agent_inbox("waiting-peer", make_msg("waiter-owned"));
        let terminal_attempted = std::cell::Cell::new(false);
        assert_eq!(
            state.assign_agent_delivery_with_channel_attempt(
                "waiting-peer",
                "waiter-owned",
                true,
                || {
                    terminal_attempted.set(true);
                    true
                },
            ),
            (AgentDeliveryAssignment::Waiter, false)
        );
        assert!(!terminal_attempted.get());
        assert_eq!(
            state
                .finish_agent_wait("waiting-peer", waiter, 0, true)
                .messages,
            vec![make_msg("waiter-owned")]
        );

        state.push_agent_inbox("vanished-peer", make_msg("pty-race"));
        assert_eq!(
            state.assign_agent_delivery("vanished-peer", "pty-race", true),
            AgentDeliveryAssignment::Terminal
        );
        state.release_terminal_delivery("vanished-peer", "pty-race");
        assert_eq!(
            state.agent_delivery_owner("vanished-peer", "pty-race"),
            None
        );
        let waiter = state.begin_agent_wait("vanished-peer");
        assert_eq!(state.waiter_fresh_message_count("vanished-peer", 0), 1);
        assert_eq!(
            state
                .finish_agent_wait("vanished-peer", waiter, 0, true)
                .messages,
            vec![make_msg("pty-race")]
        );
    }

    #[test]
    fn concurrent_waiter_does_not_report_message_observed_by_another_lease() {
        let state = tests_support::make_test_app_state();
        let recipient = "peer";
        let first = state.begin_agent_wait(recipient);
        let second = state.begin_agent_wait(recipient);
        state.push_agent_inbox(recipient, make_msg("one-owner"));
        assert_eq!(
            state.assign_agent_delivery(recipient, "one-owner", true),
            AgentDeliveryAssignment::Waiter
        );

        assert_eq!(state.waiter_fresh_message_count(recipient, 0), 1);
        let observed = state.finish_agent_wait(recipient, first, 0, true);
        assert_eq!(observed.messages, vec![make_msg("one-owner")]);

        assert_eq!(
            state.waiter_fresh_message_count(recipient, 0),
            0,
            "a second lease must not report readiness for an already observed message"
        );
        assert_eq!(
            state.finish_agent_wait(recipient, second, 0, true),
            AgentWaitFinish::default()
        );
        assert_eq!(
            state.agent_delivery_owner(recipient, "one-owner"),
            Some(AgentDeliveryOwner::WaiterObserved),
            "exactly-once wake ownership must remain with the first waiter"
        );
    }

    #[test]
    fn cancelled_last_waiter_hands_unobserved_message_to_terminal_once() {
        let state = tests_support::make_test_app_state();
        let recipient = "peer";
        let lease = state.begin_agent_wait(recipient);
        state.push_agent_inbox(recipient, make_msg("cancel-race"));
        assert_eq!(
            state.assign_agent_delivery(recipient, "cancel-race", true),
            AgentDeliveryAssignment::Waiter
        );

        assert_eq!(
            state
                .finish_agent_wait(recipient, lease, 0, false)
                .terminal_handoff,
            vec!["cancel-race".to_string()]
        );
        assert_eq!(
            state.agent_delivery_owner(recipient, "cancel-race"),
            Some(AgentDeliveryOwner::TerminalPending)
        );
        assert!(
            state
                .finish_agent_wait(recipient, lease, 0, false)
                .terminal_handoff
                .is_empty(),
            "releasing the same lease twice must not duplicate terminal handoff"
        );
    }

    /// Catches: title animation re-stamps semantic activity or enters the lossless state queue.
    #[test]
    fn remote_title_keeps_semantic_activity_and_state_queue_unchanged() {
        let state = Arc::new(tests_support::make_test_app_state());
        state.session_maps.session_states.insert(
            "title-session".into(),
            SessionState {
                last_activity_ms: 123,
                agent_state: Some("idle".into()),
                ..Default::default()
            },
        );
        let event = AppEvent::PtyTitle {
            session_id: "title-session".into(),
            title: "Claude Code".into(),
        };
        let mut bus = state.event_bus.subscribe();
        let before = state.session_maps.session_state_events.depth();
        state.emit_pty_event(event.clone());
        assert!(matches!(bus.try_recv().unwrap(), AppEvent::PtyTitle { .. }));
        assert_eq!(state.session_maps.session_state_events.depth(), before);
        AppState::apply_event_to_session_state(&state, &event);
        let session = state
            .session_maps
            .session_states
            .get("title-session")
            .unwrap();
        assert_eq!(session.last_activity_ms, 123);
        assert_eq!(session.agent_state.as_deref(), Some("idle"));
    }

    // ── emit_pty_event: per-session channel + global-bus parity (story 140) ──

    #[tokio::test]
    async fn emit_pty_event_isolates_per_session_and_mirrors_global_bus() {
        let state = tests_support::make_test_app_state();

        // A session-scoped WS handler for "a" creates + subscribes to its channel.
        let mut rx_a = state
            .session_maps
            .pty_event_channels
            .entry("a".to_string())
            .or_insert_with(|| tokio::sync::broadcast::channel(16).0)
            .subscribe();
        // A global-bus consumer (e.g. /events SSE, state accumulator) sees all.
        let mut bus_rx = state.event_bus.subscribe();

        // One event for "a" (has a channel) and one for "b" (no channel attached).
        state.emit_pty_event(AppEvent::PtyParsed {
            session_id: "a".to_string(),
            parsed: serde_json::json!({ "type": "x" }).into(),
        });
        state.emit_pty_event(AppEvent::PtyExit {
            session_id: "b".to_string(),
        });

        // "a"'s channel receives ONLY a's event — no cross-session leakage, and
        // no per-session filter block needed on the handler side.
        match rx_a.try_recv() {
            Ok(AppEvent::PtyParsed { session_id, .. }) => assert_eq!(session_id, "a"),
            other => panic!("expected PtyParsed for a, got {other:?}"),
        }
        assert!(
            rx_a.try_recv().is_err(),
            "session a's channel must not receive session b's event"
        );

        // The global bus still received BOTH events (parity for SSE/accumulator).
        let mut seen = Vec::new();
        while let Ok(e) = bus_rx.try_recv() {
            seen.push(e.pty_session_id().map(str::to_string));
        }
        assert_eq!(seen, vec![Some("a".to_string()), Some("b".to_string())]);
    }

    #[tokio::test]
    async fn a_parsed_payload_is_shared_with_every_subscriber_not_copied() {
        // emit_pty_event fans one event out to three lanes, and every broadcast
        // receiver clones again on recv. With the payload held inline that is a
        // deep JSON copy per lane per subscriber — paid on every PTY chunk, for
        // consumers that mostly read one `type` field and drop the rest.
        let state = tests_support::make_test_app_state();
        let mut rx_session = state
            .session_maps
            .pty_event_channels
            .entry("a".to_string())
            .or_insert_with(|| tokio::sync::broadcast::channel(16).0)
            .subscribe();
        let mut rx_bus = state.event_bus.subscribe();

        let parsed: Arc<serde_json::Value> =
            Arc::new(serde_json::json!({ "type": "status-line", "content": "x" }));
        state.emit_pty_event(AppEvent::PtyParsed {
            session_id: "a".to_string(),
            parsed: Arc::clone(&parsed),
        });

        for rx in [&mut rx_session, &mut rx_bus] {
            match rx.try_recv() {
                Ok(AppEvent::PtyParsed { parsed: got, .. }) => assert!(
                    Arc::ptr_eq(&parsed, &got),
                    "every lane must share the payload, not copy it"
                ),
                other => panic!("expected PtyParsed, got {other:?}"),
            }
        }
    }

    #[tokio::test]
    async fn session_closed_survives_channel_reap() {
        // Critical invariant: the final "closed" frame must reach the client even
        // though cleanup reaps the per-session channel right after emitting it.
        // broadcast drains buffered messages before signalling Closed.
        let state = tests_support::make_test_app_state();
        let mut rx = state
            .session_maps
            .pty_event_channels
            .entry("s".to_string())
            .or_insert_with(|| tokio::sync::broadcast::channel(16).0)
            .subscribe();

        state.emit_pty_event(AppEvent::SessionClosed {
            session_id: "s".to_string(),
            reason: "process_exit".to_string(),
        });
        // Simulate cleanup_session/tombstone_transient_cleanup dropping the sender.
        state.session_maps.pty_event_channels.remove("s");

        match rx.recv().await {
            Ok(AppEvent::SessionClosed { reason, .. }) => assert_eq!(reason, "process_exit"),
            other => panic!("closed frame must survive channel reap, got {other:?}"),
        }
        assert!(matches!(
            rx.recv().await,
            Err(tokio::sync::broadcast::error::RecvError::Closed)
        ));
    }

    // ── data_dir ─────────────────────────────────────────────

    #[test]
    fn test_state_has_data_dir() {
        let state = tests_support::make_test_app_state();
        // The test fixture belongs to this checkout, even when the caller
        // did not set TMPDIR before invoking Cargo.
        assert!(
            state
                .data_dir
                .starts_with(crate::test_support::test_temp_root())
        );
        assert!(
            state
                .data_dir
                .file_name()
                .and_then(|n| n.to_str())
                .is_some_and(|n| n.starts_with("test-tuic-data")),
            "unexpected data_dir: {:?}",
            state.data_dir
        );
    }

    #[test]
    fn test_state_data_dir_is_removed_on_drop_and_panic() {
        let scratch =
            tempfile::tempdir_in(crate::test_support::test_temp_root()).expect("scratch test root");
        let entries = || {
            std::fs::read_dir(scratch.path())
                .expect("read scratch root")
                .count()
        };
        let before = entries();
        let state = tests_support::make_test_app_state_in(scratch.path());
        let normal_path = state.data_dir.clone();
        let worktrees_path = state.worktrees_dir.clone();
        std::fs::create_dir_all(&worktrees_path).expect("create fixture worktrees");
        std::fs::write(worktrees_path.join("probe"), b"fixture").expect("write fixture worktree");
        assert!(normal_path.exists());
        drop(state);
        assert!(
            !normal_path.exists(),
            "fixture survived normal drop: {normal_path:?}"
        );
        assert!(!worktrees_path.exists(), "fixture worktree survived drop");
        assert_eq!(entries(), before, "fixture left an entry after normal drop");

        let mut panic_path = None;
        // The panic is the point of the test; keep it off stderr.
        let hook = std::panic::take_hook();
        std::panic::set_hook(Box::new(|_| {}));
        let outcome = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            let state = tests_support::make_test_app_state_in(scratch.path());
            panic_path = Some(state.data_dir.clone());
            panic!("exercise fixture cleanup during unwind");
        }));
        std::panic::set_hook(hook);
        assert!(outcome.is_err());
        let panic_path = panic_path.expect("fixture was created before panic");
        assert!(
            !panic_path.exists(),
            "fixture survived panic: {panic_path:?}"
        );
        assert_eq!(entries(), before, "fixture left an entry after panic");
    }

    // ── split_name_segments ──────────────────────────────────

    #[test]
    fn segments_hyphen() {
        assert_eq!(
            split_name_segments("night-recovery"),
            vec!["night", "recovery"]
        );
    }

    #[test]
    fn segments_underscore() {
        assert_eq!(split_name_segments("my_app"), vec!["my", "app"]);
    }

    #[test]
    fn segments_camel_case() {
        assert_eq!(
            split_name_segments("tuiCommander"),
            vec!["tui", "commander"]
        );
    }

    #[test]
    fn segments_dot() {
        assert_eq!(split_name_segments("my.project"), vec!["my", "project"]);
    }

    #[test]
    fn segments_single_word() {
        assert_eq!(split_name_segments("server"), vec!["server"]);
    }

    #[test]
    fn segments_mixed() {
        assert_eq!(
            split_name_segments("my-AwesomeProject_v2"),
            vec!["my", "awesome", "project", "v2"]
        );
    }

    #[test]
    fn segments_empty() {
        assert_eq!(split_name_segments(""), vec!["sh"]);
    }

    // ── repo_name_to_prefix ─────────────────────────────────

    #[test]
    fn prefix_multi_word_hyphen() {
        let m = DashMap::new();
        assert_eq!(repo_name_to_prefix("night-recovery", &m), "nr");
    }

    #[test]
    fn prefix_multi_word_camel() {
        let m = DashMap::new();
        assert_eq!(repo_name_to_prefix("tuiCommander", &m), "tc");
    }

    #[test]
    fn prefix_single_short() {
        let m = DashMap::new();
        assert_eq!(repo_name_to_prefix("go", &m), "go");
    }

    #[test]
    fn prefix_single_long() {
        let m = DashMap::new();
        assert_eq!(repo_name_to_prefix("server", &m), "se");
    }

    #[test]
    fn prefix_empty() {
        let m = DashMap::new();
        assert_eq!(repo_name_to_prefix("", &m), "sh");
    }

    #[test]
    fn prefix_collision_extends() {
        let m: DashMap<String, String> = DashMap::new();
        m.insert("s1".into(), "ma-1".into()); // "ma" prefix is taken
        let p = repo_name_to_prefix("my-app", &m);
        assert_ne!(p, "ma", "should not collide");
        assert!(p.starts_with("ma"), "should extend from base: {p}");
    }

    #[test]
    fn prefix_three_word() {
        let m = DashMap::new();
        assert_eq!(repo_name_to_prefix("my-awesome-project", &m), "map");
    }

    #[test]
    fn prefix_separator_only_no_panic() {
        // A directory literally named "---" or "..." splits into no segments;
        // split_name_segments falls back to ["sh"]. Must not panic.
        let m = DashMap::new();
        assert_eq!(repo_name_to_prefix("---", &m), "sh");
        assert_eq!(repo_name_to_prefix("...", &m), "sh");
        assert_eq!(repo_name_to_prefix("_._", &m), "sh");
    }

    #[test]
    fn prefix_multibyte_single_word_no_panic() {
        // Non-ASCII single-word name: old &s[..2] byte slice panicked on a
        // multibyte char boundary. chars().take(2) is safe.
        let m = DashMap::new();
        assert_eq!(repo_name_to_prefix("über", &m), "üb");
        // 2-char multibyte word stays as-is (< 3 chars).
        assert_eq!(repo_name_to_prefix("ök", &m), "ök");
    }

    #[test]
    fn prefix_multibyte_multi_word_no_panic() {
        // Multi-word name whose first segment starts with a multibyte char:
        // old &s[..1] byte slice panicked. chars().next() is safe.
        let m = DashMap::new();
        assert_eq!(repo_name_to_prefix("über-café", &m), "üc");
        assert_eq!(repo_name_to_prefix("日本-project", &m), "日p");
    }

    // ── assign_term_alias (prefix + counter) ──────────────

    #[test]
    fn prefix_no_collision_when_map_empty() {
        let m: DashMap<String, String> = DashMap::new();
        let p = repo_name_to_prefix("night-recovery", &m);
        assert_eq!(p, "nr");
    }

    #[test]
    fn prefix_extends_on_self_collision() {
        // repo_name_to_prefix doesn't know about repo identity — it only sees
        // existing aliases. assign_term_alias handles same-repo reuse via
        // find_prefix_for_repo. This test verifies the raw collision path.
        let m: DashMap<String, String> = DashMap::new();
        m.insert("s1".into(), "nr-1".into());
        let p = repo_name_to_prefix("night-recorder", &m);
        assert_ne!(p, "nr", "different repo should get extended prefix");
    }

    #[test]
    fn alias_different_repos_different_prefixes() {
        let m: DashMap<String, String> = DashMap::new();
        let p1 = repo_name_to_prefix("night-recovery", &m);
        m.insert("s1".into(), format!("{p1}-1"));
        let p2 = repo_name_to_prefix("tuiCommander", &m);
        assert_ne!(p1, p2, "different repos should get different prefixes");
    }

    // ── acronym collision tests ─────────────────────────────

    #[test]
    fn collision_my_app_vs_my_api() {
        let m: DashMap<String, String> = DashMap::new();
        let p1 = repo_name_to_prefix("my-app", &m);
        assert_eq!(p1, "ma");
        m.insert("s1".into(), format!("{p1}-1"));
        let p2 = repo_name_to_prefix("my-api", &m);
        assert_ne!(p2, "ma", "my-api must not collide with my-app: got {p2}");
        assert!(
            p2.starts_with("ma"),
            "should extend from base 'ma': got {p2}"
        );
    }

    #[test]
    fn collision_three_way_ma_prefix() {
        let m: DashMap<String, String> = DashMap::new();
        let p1 = repo_name_to_prefix("my-app", &m);
        m.insert("s1".into(), format!("{p1}-1"));
        let p2 = repo_name_to_prefix("my-api", &m);
        m.insert("s2".into(), format!("{p2}-1"));
        let p3 = repo_name_to_prefix("my-auth", &m);
        assert_ne!(p3, p1, "third collision must resolve: {p3} vs {p1}");
        assert_ne!(p3, p2, "third collision must resolve: {p3} vs {p2}");
    }

    #[test]
    fn collision_single_word_same_start() {
        let m: DashMap<String, String> = DashMap::new();
        let p1 = repo_name_to_prefix("server", &m);
        assert_eq!(p1, "se");
        m.insert("s1".into(), format!("{p1}-1"));
        let p2 = repo_name_to_prefix("service", &m);
        assert_ne!(p2, "se", "service must not collide with server: got {p2}");
    }

    #[test]
    fn collision_two_char_repos() {
        let m: DashMap<String, String> = DashMap::new();
        let p1 = repo_name_to_prefix("go", &m);
        assert_eq!(p1, "go");
        m.insert("s1".into(), format!("{p1}-1"));
        let p2 = repo_name_to_prefix("go-tools", &m);
        // go-tools = "gt", different from "go" — no collision expected
        assert_ne!(p2, p1, "go-tools should differ from go");
    }

    #[test]
    fn collision_resolved_uniquely_for_five_similar_repos() {
        let m: DashMap<String, String> = DashMap::new();
        let mut prefixes = Vec::new();
        for name in &["my-app", "my-api", "my-auth", "my-admin", "my-agent"] {
            let p = repo_name_to_prefix(name, &m);
            assert!(
                !prefixes.contains(&p),
                "duplicate prefix {p} for {name}, existing: {prefixes:?}"
            );
            m.insert(format!("s{}", prefixes.len()), format!("{p}-1"));
            prefixes.push(p);
        }
    }

    // ── alias reservation + session reference resolution ──

    /// A tab that comes back from disk brings its alias with it. Without a
    /// reservation the restored tab is handed `tc-1` while an older tab in the
    /// same repo already answers to that address, so a `send` reaches the wrong
    /// terminal — and the counter has to move past the restored number, or the
    /// next fresh tab collides with it.
    #[cfg(unix)]
    #[test]
    fn a_restored_alias_is_reserved_and_the_counter_moves_past_it() {
        let state = tests_support::make_test_app_state();
        tests_support::insert_dummy_session(&state, "restored");
        tests_support::set_session_cwd(&state, "restored", "/repos/tuicommander");

        assert_eq!(
            state.assign_term_alias("restored", Some("tc-3")),
            "tc-3",
            "a restored tab keeps the alias it was saved with"
        );

        tests_support::insert_dummy_session(&state, "fresh");
        tests_support::set_session_cwd(&state, "fresh", "/repos/tuicommander");

        assert_eq!(
            state.assign_term_alias("fresh", None),
            "tc-4",
            "the counter must resume past the restored alias, not re-issue it"
        );
    }

    /// The requested alias arrives from persisted client state, so it is input:
    /// a shape the resolver cannot address, or one another live tab already owns,
    /// is dropped in favour of a generated alias rather than trusted.
    #[cfg(unix)]
    #[test]
    fn a_requested_alias_that_is_malformed_or_taken_is_not_honoured() {
        let state = tests_support::make_test_app_state();
        tests_support::insert_dummy_session(&state, "first");
        tests_support::set_session_cwd(&state, "first", "/repos/tuicommander");
        let first = state.assign_term_alias("first", Some("tc-1"));
        assert_eq!(first, "tc-1");

        for rejected in ["garbage", "tc-", "-1", "tc-0", "tc-x", "", "tc-1"] {
            tests_support::insert_dummy_session(&state, rejected);
            tests_support::set_session_cwd(&state, rejected, "/repos/tuicommander");
            let assigned = state.assign_term_alias(rejected, Some(rejected));
            assert_ne!(
                assigned, rejected,
                "'{rejected}' is not a reservable alias and must not be honoured"
            );
            assert!(
                assigned.starts_with("tc-"),
                "the fallback must still be a generated alias: got {assigned}"
            );
        }
    }

    /// One address book for everything a human or an agent can name a terminal
    /// by: the PTY key, the `$TUIC_SESSION` the tab persists, or the short alias
    /// shown in the tab menu.
    #[cfg(unix)]
    #[test]
    fn resolve_session_ref_checked_accepts_pty_id_tuic_session_and_alias() {
        let state = tests_support::make_test_app_state();
        let session_id = "01234567-89ab-cdef-0123-456789abcdef";
        tests_support::insert_dummy_session(&state, session_id);
        state.bind_live_pty("tuic-uuid", session_id);
        state
            .session_maps
            .sessions
            .get(session_id)
            .expect("test session")
            .lock()
            .set_display_name(Some("reviewer".to_string()), true);
        let alias = state.assign_term_alias(session_id, None);

        for reference in [
            session_id,
            "tuic-uuid",
            alias.as_str(),
            "01234567",
            "reviewer",
        ] {
            assert_eq!(
                state.resolve_session_ref_checked(reference).unwrap(),
                Some(session_id.to_string()),
                "'{reference}' must address the terminal behind it"
            );
        }
        assert_eq!(
            state.resolve_session_ref_checked("never-seen").unwrap(),
            None,
            "an unknown reference resolves to nothing rather than to a guess"
        );
    }

    #[cfg(unix)]
    #[test]
    fn resolve_session_ref_reports_ambiguous_display_names() {
        let state = tests_support::make_test_app_state();
        for session_id in [
            "11111111-89ab-cdef-0123-456789abcdef",
            "11111111-89ab-cdef-0123-456789abcdef0",
        ] {
            tests_support::insert_dummy_session(&state, session_id);
            state
                .session_maps
                .sessions
                .get(session_id)
                .expect("test session")
                .lock()
                .set_display_name(Some("reviewer".to_string()), true);
        }

        let error = state
            .resolve_session_ref_checked("reviewer")
            .expect_err("duplicate display names must not pick a session");
        assert!(error.contains("ambiguous"), "{error}");
        assert!(error.contains("reviewer"), "{error}");

        let error = state
            .resolve_session_ref_checked("11111111")
            .expect_err("duplicate short IDs must not pick a session");
        assert!(error.contains("ambiguous"), "{error}");
    }

    /// Browser/PWA clients learn a tab's alias from the bus (`/events` SSE),
    /// not from the desktop-only window emit — a spawn seen live in browser mode
    /// otherwise shows no alias until the next reload lists it.
    #[test]
    fn assign_term_alias_publishes_the_alias_on_the_event_bus() {
        let state = tests_support::make_test_app_state();
        let mut bus = state.event_bus.subscribe();

        let alias = state.assign_term_alias("pty-key", Some("tu-9"));

        assert_eq!(alias, "tu-9");
        match bus.try_recv() {
            Ok(AppEvent::TermAliasAssigned { session_id, alias }) => {
                assert_eq!(session_id, "pty-key");
                assert_eq!(alias, "tu-9");
            }
            other => panic!("expected term-alias-assigned on the bus, got {other:?}"),
        }
    }

    /// Mail is filed under the peer key, so an address that names the terminal
    /// has to travel back the other way before a message can be delivered.
    #[cfg(unix)]
    #[test]
    fn resolve_peer_ref_checked_maps_a_terminal_address_back_to_its_peer() {
        let state = tests_support::make_test_app_state();
        tests_support::insert_dummy_session(&state, "pty-key");
        state.bind_live_pty("tuic-uuid", "pty-key");
        let alias = state.assign_term_alias("pty-key", None);
        state.peer_agents.insert(
            "tuic-uuid".to_string(),
            PeerAgent {
                tuic_session: "tuic-uuid".to_string(),
                mcp_session_id: "mcp-1".to_string(),
                name: "worker".to_string(),
                project: None,
                registered_at: 0,
            },
        );

        for reference in ["tuic-uuid", "pty-key", alias.as_str()] {
            assert_eq!(
                state.resolve_peer_ref_checked(reference).unwrap(),
                Some("tuic-uuid".to_string()),
                "'{reference}' must resolve to the peer that owns that terminal"
            );
        }
        assert_eq!(state.resolve_peer_ref_checked("never-seen").unwrap(), None);
    }

    /// list_peers prints the register `name`; a name that cannot be sent to is a
    /// trap. Catches: name resolving to nothing, or silently picking one of two
    /// peers that share it.
    #[test]
    fn resolve_peer_ref_checked_accepts_the_register_name_and_refuses_duplicates() {
        let state = tests_support::make_test_app_state();
        for (key, name) in [("peer-a", "alpha"), ("peer-b", "twin"), ("peer-c", "twin")] {
            state.peer_agents.insert(
                key.to_string(),
                PeerAgent {
                    tuic_session: key.to_string(),
                    mcp_session_id: format!("mcp-{key}"),
                    name: name.to_string(),
                    project: None,
                    registered_at: 0,
                },
            );
        }

        assert_eq!(
            state.resolve_peer_ref_checked("alpha").unwrap(),
            Some("peer-a".to_string())
        );
        let error = state
            .resolve_peer_ref_checked("twin")
            .expect_err("a shared name must not pick a peer");
        assert!(error.contains("ambiguous"), "{error}");
        assert!(
            error.contains("peer-b") && error.contains("peer-c"),
            "{error}"
        );
    }

    #[cfg(test)]
    fn insert_named_peer(state: &AppState, key: &str, name: &str, registered_at: u64) {
        state.peer_agents.insert(
            key.to_string(),
            PeerAgent {
                tuic_session: key.to_string(),
                mcp_session_id: format!("mcp-{key}"),
                name: name.to_string(),
                project: None,
                registered_at,
            },
        );
    }

    /// Critic 1372. Catches: a peer that registers the name `tu-1` captures mail
    /// meant for the terminal whose alias is `tu-1` (name checked before alias,
    /// silently, with no ambiguity error).
    #[cfg(unix)]
    #[test]
    fn critic_1372_peer_name_equal_to_another_terminals_alias_is_not_silently_preferred() {
        let state = tests_support::make_test_app_state();
        tests_support::insert_dummy_session(&state, "pty-owner");
        state.bind_live_pty("owner-uuid", "pty-owner");
        let alias = state.assign_term_alias("pty-owner", None);
        insert_named_peer(&state, "owner-uuid", "worker", 1);
        insert_named_peer(&state, "squatter-uuid", &alias, 2);

        let resolved = state.resolve_peer_ref_checked(&alias);

        assert_ne!(
            resolved,
            Ok(Some("squatter-uuid".to_string())),
            "alias '{alias}' of the owner's terminal was captured by a peer that merely registered that name"
        );
    }

    /// Critic 1372. Catches: a peer named like another terminal's PTY key captures
    /// mail addressed to that PTY key.
    #[cfg(unix)]
    #[test]
    fn critic_1372_peer_name_equal_to_another_terminals_pty_key_is_not_silently_preferred() {
        let state = tests_support::make_test_app_state();
        tests_support::insert_dummy_session(&state, "pty-owner");
        state.bind_live_pty("owner-uuid", "pty-owner");
        insert_named_peer(&state, "owner-uuid", "worker", 1);
        insert_named_peer(&state, "squatter-uuid", "pty-owner", 2);

        assert_ne!(
            state.resolve_peer_ref_checked("pty-owner"),
            Ok(Some("squatter-uuid".to_string())),
            "PTY key of the owner's terminal was captured by a peer that merely registered that name"
        );
    }

    /// Critic 1372. Catches: an exact peer key losing to a name, i.e. a peer named
    /// with another peer's full UUID redirecting UUID-addressed mail.
    #[test]
    fn critic_1372_exact_peer_key_beats_a_name_equal_to_it() {
        let state = tests_support::make_test_app_state();
        insert_named_peer(&state, "uuid-real", "real", 1);
        insert_named_peer(&state, "uuid-squat", "uuid-real", 2);

        assert_eq!(
            state.resolve_peer_ref_checked("uuid-real"),
            Ok(Some("uuid-real".to_string()))
        );
    }

    /// Critic 1372. Catches: the name branch matching an empty reference against a
    /// peer registered with an empty name (workflow wake path passes whatever
    /// string the run log holds).
    #[test]
    fn critic_1372_empty_reference_never_matches_an_empty_peer_name() {
        let state = tests_support::make_test_app_state();
        insert_named_peer(&state, "peer-empty", "", 1);

        assert_ne!(
            state.resolve_peer_ref_checked(""),
            Ok(Some("peer-empty".to_string()))
        );
    }

    /// Critic 1372. Catches: names compared case-insensitively or trimmed, which
    /// would merge `Coordinator` and `coordinator` into one ambiguous address, or
    /// route to the wrong one.
    #[test]
    fn critic_1372_name_match_is_exact_not_case_folded() {
        let state = tests_support::make_test_app_state();
        insert_named_peer(&state, "peer-upper", "Coordinator", 1);
        insert_named_peer(&state, "peer-lower", "coordinator", 2);

        assert_eq!(
            state.resolve_peer_ref_checked("coordinator"),
            Ok(Some("peer-lower".to_string()))
        );
        assert_eq!(
            state.resolve_peer_ref_checked("Coordinator"),
            Ok(Some("peer-upper".to_string()))
        );
        assert_eq!(state.resolve_peer_ref_checked("coordinator "), Ok(None));
    }

    /// Critic 1372. Catches: two identities of one live terminal (persisted tab
    /// UUID + PTY key, 1246-46e3) sharing a register name being reported as an
    /// ambiguous address although both files lead to the same mailbox owner.
    #[cfg(unix)]
    #[test]
    fn critic_1372_two_identities_of_one_terminal_with_one_name_are_not_ambiguous() {
        let state = tests_support::make_test_app_state();
        tests_support::insert_dummy_session(&state, "pty-shared");
        state.bind_live_pty("tab-uuid", "pty-shared");
        insert_named_peer(&state, "tab-uuid", "coordinator", 1);
        insert_named_peer(&state, "pty-shared", "coordinator", 2);

        let resolved = state.resolve_peer_ref_checked("coordinator");

        assert!(
            resolved.is_ok(),
            "same terminal, same name: not ambiguous, got {resolved:?}"
        );
    }

    /// Critic 1372. Catches: a dead peer (its terminal and MCP session gone, entry
    /// not yet reaped) making the live peer's name ambiguous after a restart that
    /// reuses the name.
    #[cfg(unix)]
    #[test]
    fn critic_1372_stale_peer_with_the_same_name_does_not_make_the_live_one_ambiguous() {
        let state = tests_support::make_test_app_state();
        tests_support::insert_dummy_session(&state, "pty-live");
        state.bind_live_pty("live-uuid", "pty-live");
        insert_named_peer(&state, "live-uuid", "coordinator", 2);
        // Dead: no live PTY, MCP session unknown to the state.
        insert_named_peer(&state, "dead-uuid", "coordinator", 1);

        assert_eq!(
            state.resolve_peer_ref_checked("coordinator"),
            Ok(Some("live-uuid".to_string()))
        );
    }

    /// Critic 1372. Catches: the ambiguity error leaking when a name matches a
    /// single peer but the same string is also a unique session display name of a
    /// different terminal (name silently wins, display name unreachable).
    #[cfg(unix)]
    #[test]
    fn critic_1372_name_equal_to_another_terminals_display_name_is_not_silently_preferred() {
        let state = tests_support::make_test_app_state();
        tests_support::insert_dummy_session(&state, "pty-display");
        state.bind_live_pty("display-uuid", "pty-display");
        state
            .session_maps
            .sessions
            .get("pty-display")
            .unwrap()
            .lock()
            .set_display_name(Some("build".to_string()), true);
        insert_named_peer(&state, "display-uuid", "worker", 1);
        insert_named_peer(&state, "squatter-uuid", "build", 2);

        assert_ne!(
            state.resolve_peer_ref_checked("build"),
            Ok(Some("squatter-uuid".to_string())),
            "display name of another terminal captured by a registered name"
        );
    }

    #[test]
    fn test_utf8_buffer_ascii() {
        let mut buf = Utf8ReadBuffer::new();
        assert_eq!(buf.push(b"hello world"), "hello world");
        assert!(buf.remainder.is_empty());
    }

    #[test]
    fn test_utf8_buffer_complete_multibyte() {
        let mut buf = Utf8ReadBuffer::new();
        assert_eq!(buf.push("€100".as_bytes()), "€100");
        assert!(buf.remainder.is_empty());
    }

    #[test]
    fn test_utf8_buffer_split_multibyte() {
        let mut buf = Utf8ReadBuffer::new();
        let result1 = buf.push(&[0xE2]);
        assert_eq!(result1, "");
        assert_eq!(buf.remainder.len(), 1);

        let result2 = buf.push(&[0x82, 0xAC]);
        assert_eq!(result2, "€");
        assert!(buf.remainder.is_empty());
    }

    #[test]
    fn test_utf8_buffer_split_4byte_emoji() {
        let mut buf = Utf8ReadBuffer::new();
        let crab = "🦀";
        let bytes = crab.as_bytes();
        assert_eq!(bytes.len(), 4);

        let result1 = buf.push(&bytes[..2]);
        assert_eq!(result1, "");
        assert_eq!(buf.remainder.len(), 2);

        let result2 = buf.push(&bytes[2..]);
        assert_eq!(result2, "🦀");
        assert!(buf.remainder.is_empty());
    }

    #[test]
    fn test_utf8_buffer_ascii_then_split() {
        let mut buf = Utf8ReadBuffer::new();
        let mut chunk1 = b"hello".to_vec();
        chunk1.push(0xE2);
        let result1 = buf.push(&chunk1);
        assert_eq!(result1, "hello");
        assert_eq!(buf.remainder.len(), 1);

        let mut chunk2 = vec![0x82, 0xAC];
        chunk2.extend_from_slice(b" world");
        let result2 = buf.push(&chunk2);
        assert_eq!(result2, "€ world");
        assert!(buf.remainder.is_empty());
    }

    #[test]
    fn test_utf8_buffer_flush_incomplete() {
        let mut buf = Utf8ReadBuffer::new();
        let result = buf.push(&[0xE2]);
        assert_eq!(result, "");

        let flushed = buf.flush();
        assert!(flushed.contains('\u{FFFD}'));
    }

    #[test]
    fn test_utf8_buffer_flush_empty() {
        let mut buf = Utf8ReadBuffer::new();
        assert_eq!(buf.flush(), "");
    }

    #[test]
    fn test_utf8_buffer_cjk_characters() {
        let mut buf = Utf8ReadBuffer::new();
        let han = "漢字";
        let bytes = han.as_bytes();
        let split = 4;
        let result1 = buf.push(&bytes[..split]);
        assert_eq!(result1, "漢");
        let result2 = buf.push(&bytes[split..]);
        assert_eq!(result2, "字");
    }

    #[test]
    fn test_utf8_buffer_invalid_bytes_replaced() {
        let mut buf = Utf8ReadBuffer::new();
        // Single invalid byte between valid ASCII → one U+FFFD, surrounding text kept.
        assert_eq!(buf.push(b"a\xffb"), "a\u{FFFD}b");
        // Consecutive invalid bytes → one U+FFFD each (matches from_utf8_lossy).
        assert_eq!(buf.push(b"x\xff\xffy"), "x\u{FFFD}\u{FFFD}y");
        // Invalid byte immediately before a valid multibyte char.
        assert_eq!(buf.push("\u{FF}".as_bytes()), "\u{FF}"); // 0xC3 0xBF is valid UTF-8 (ÿ)
    }

    #[test]
    fn test_utf8_buffer_large_binary_no_stack_overflow() {
        // Regression: the old recursive push() recursed once per invalid byte, so a
        // large all-invalid (binary) chunk overflowed the reader thread's stack. The
        // iterative version must process it flat. 64 KB of 0xFF → 64 K replacement chars.
        let mut buf = Utf8ReadBuffer::new();
        let binary = vec![0xffu8; 64 * 1024];
        let out = buf.push(&binary);
        assert_eq!(out.chars().count(), 64 * 1024);
        assert!(out.chars().all(|c| c == '\u{FFFD}'));
        assert!(buf.remainder.is_empty());
    }

    /// `len()` must track what `read_last` would actually return, which stops
    /// short of `total_written` the moment the ring wraps. A naive accessor
    /// returning the monotonic counter passes the pre-wrap half of this and
    /// fails the rest.
    #[test]
    fn ring_buffer_len_matches_readable_bytes_across_the_wrap() {
        let mut rb = OutputRingBuffer::new(8);
        assert_eq!(rb.len(), 0);

        rb.write(b"abc");
        assert_eq!(rb.len(), 3, "below capacity, len is everything written");
        assert_eq!(rb.len(), rb.read_last(usize::MAX).0.len());

        rb.write(b"defgh");
        assert_eq!(rb.len(), 8, "exactly full");
        assert_eq!(rb.len(), rb.read_last(usize::MAX).0.len());

        rb.write(b"ijklm");
        assert_eq!(rb.len(), 8, "past the wrap, len saturates at capacity");
        assert_eq!(rb.total_written, 13, "the monotonic counter keeps climbing");
        assert_eq!(rb.len(), rb.read_last(usize::MAX).0.len());
    }

    #[test]
    fn test_ring_buffer_basic() {
        let mut rb = OutputRingBuffer::new(16);
        rb.write(b"hello");
        let (data, total) = rb.read_last(16);
        assert_eq!(&data, b"hello");
        assert_eq!(total, 5);
    }

    #[test]
    fn test_ring_buffer_wraps_around() {
        let mut rb = OutputRingBuffer::new(8);
        rb.write(b"12345678");
        rb.write(b"AB");
        let (data, total) = rb.read_last(8);
        assert_eq!(&data, b"345678AB");
        assert_eq!(total, 10);
    }

    #[test]
    fn test_ring_buffer_read_less_than_available() {
        let mut rb = OutputRingBuffer::new(16);
        rb.write(b"hello world");
        let (data, _) = rb.read_last(5);
        assert_eq!(&data, b"world");
    }

    #[test]
    fn test_ring_buffer_empty() {
        let rb = OutputRingBuffer::new(16);
        let (data, total) = rb.read_last(16);
        assert!(data.is_empty());
        assert_eq!(total, 0);
    }

    #[test]
    fn test_ring_buffer_large_write() {
        let mut rb = OutputRingBuffer::new(4);
        rb.write(b"abcdefgh");
        let (data, total) = rb.read_last(4);
        assert_eq!(&data, b"efgh");
        assert_eq!(total, 8);
    }

    #[test]
    fn test_ring_buffer_read_last_bulk_copy_wrap() {
        // Wrap-around: read_last must stitch two slices correctly.
        let mut rb = OutputRingBuffer::new(8);
        rb.write(b"ABCDEFGH"); // fills buffer, write_pos = 0
        rb.write(b"XY"); // overwrites A,B → buf = [X,Y,C,D,E,F,G,H], write_pos = 2

        // Read all 8 bytes — should produce CDEFGHXY (oldest to newest)
        let (data, total) = rb.read_last(8);
        assert_eq!(&data, b"CDEFGHXY");
        assert_eq!(total, 10);

        // Read last 3 — should produce HXY (straddles the wrap boundary)
        let (data, _) = rb.read_last(3);
        assert_eq!(&data, b"HXY");

        // Read last 6 — wraps from before write_pos to after.
        // buf = [X,Y,C,D,E,F,G,H], write_pos = 2, available = 8
        // start = 2 + 8 - 6 = 4 → indices 4,5,6,7,0,1 → E,F,G,H,X,Y
        let (data, _) = rb.read_last(6);
        assert_eq!(&data, b"EFGHXY");
    }

    #[test]
    fn test_ring_buffer_read_last_no_wrap() {
        // No wrap: all data sits in a contiguous region.
        let mut rb = OutputRingBuffer::new(16);
        rb.write(b"hello world!");
        let (data, total) = rb.read_last(5);
        assert_eq!(&data, b"orld!");
        assert_eq!(total, 12);

        let (data, _) = rb.read_last(12);
        assert_eq!(&data, b"hello world!");

        // Request more than available
        let (data, _) = rb.read_last(100);
        assert_eq!(&data, b"hello world!");
    }

    #[test]
    fn test_ring_buffer_read_last_2mb_performance() {
        // Verify that read_last on a full 2MB buffer completes quickly.
        // With bulk copy this should be sub-millisecond; the old byte-per-byte
        // loop took ~2M iterations with modulo on each.
        let cap = 2 * 1024 * 1024; // 2 MB
        let mut rb = OutputRingBuffer::new(cap);

        // Fill the buffer with pattern data that wraps
        let chunk: Vec<u8> = (0..=255u8).cycle().take(cap + 1024).collect();
        rb.write(&chunk);

        let start = std::time::Instant::now();
        let iterations = 100;
        for _ in 0..iterations {
            let (data, _) = rb.read_last(cap);
            assert_eq!(data.len(), cap);
            // Prevent optimizing away
            std::hint::black_box(&data);
        }
        let elapsed = start.elapsed();
        let per_call = elapsed / iterations;
        eprintln!(
            "read_last(2MB) x {}: total {:?}, per call {:?}",
            iterations, elapsed, per_call
        );
        // Bulk copy should finish each call well under 5ms on any modern machine.
        assert!(
            per_call.as_millis() < 5,
            "read_last(2MB) took {:?} per call — too slow",
            per_call
        );
    }

    // --- read_since tests ---

    #[test]
    fn test_ring_buffer_read_since_basic() {
        let mut rb = OutputRingBuffer::new(16);
        rb.write(b"hello");
        // offset 0 → get everything
        let (data, total) = rb.read_since(0);
        assert_eq!(&data, b"hello");
        assert_eq!(total, 5);

        rb.write(b" world");
        // offset 5 → only " world"
        let (data, total) = rb.read_since(5);
        assert_eq!(&data, b" world");
        assert_eq!(total, 11);
    }

    #[test]
    fn test_ring_buffer_read_since_at_current() {
        let mut rb = OutputRingBuffer::new(16);
        rb.write(b"abc");
        let (data, _) = rb.read_since(3);
        assert!(data.is_empty());
    }

    #[test]
    fn test_ring_buffer_read_since_future_offset() {
        let mut rb = OutputRingBuffer::new(16);
        rb.write(b"abc");
        let (data, total) = rb.read_since(999);
        assert!(data.is_empty());
        assert_eq!(total, 3);
    }

    #[test]
    fn test_ring_buffer_read_since_old_offset_clamped() {
        // Offset is so old that data has been overwritten — return what's available
        let mut rb = OutputRingBuffer::new(8);
        rb.write(b"12345678"); // total=8, buf full
        rb.write(b"ABCD"); // total=12, oldest is 5678ABCD
        // offset 2 would want 10 bytes, but only 8 available
        let (data, total) = rb.read_since(2);
        assert_eq!(&data, b"5678ABCD");
        assert_eq!(total, 12);
    }

    #[test]
    fn test_ring_buffer_total_written() {
        let mut rb = OutputRingBuffer::new(8);
        assert_eq!(rb.total_written(), 0);
        rb.write(b"abc");
        assert_eq!(rb.total_written(), 3);
        rb.write(b"defghij");
        assert_eq!(rb.total_written(), 10);
    }

    // --- EscapeAwareBuffer tests ---

    #[test]
    fn test_escape_buffer_plain_text() {
        let mut buf = EscapeAwareBuffer::new();
        assert_eq!(buf.push("hello world"), "hello world");
    }

    #[test]
    fn test_escape_buffer_complete_csi() {
        let mut buf = EscapeAwareBuffer::new();
        // Complete CSI sequence: ESC[31m (set red)
        assert_eq!(buf.push("\x1b[31mRed\x1b[0m"), "\x1b[31mRed\x1b[0m");
    }

    #[test]
    fn test_escape_buffer_split_csi() {
        let mut buf = EscapeAwareBuffer::new();
        // ESC[31 is incomplete — missing final byte
        let out1 = buf.push("Hello\x1b[31");
        assert_eq!(out1, "Hello");
        // Now complete it
        let out2 = buf.push("mRed\x1b[0m");
        assert_eq!(out2, "\x1b[31mRed\x1b[0m");
    }

    #[test]
    fn test_escape_buffer_split_esc_alone() {
        let mut buf = EscapeAwareBuffer::new();
        // Bare ESC at end
        let out1 = buf.push("text\x1b");
        assert_eq!(out1, "text");
        // Complete on next chunk
        let out2 = buf.push("[Cmore");
        assert_eq!(out2, "\x1b[Cmore");
    }

    #[test]
    fn test_escape_buffer_complete_osc() {
        let mut buf = EscapeAwareBuffer::new();
        // Complete OSC with BEL terminator
        assert_eq!(
            buf.push("\x1b]0;My Title\x07text"),
            "\x1b]0;My Title\x07text"
        );
    }

    #[test]
    fn test_escape_buffer_split_osc() {
        let mut buf = EscapeAwareBuffer::new();
        // OSC without terminator
        let out1 = buf.push("before\x1b]0;My Title");
        assert_eq!(out1, "before");
        // Complete with BEL
        let out2 = buf.push("\x07after");
        assert_eq!(out2, "\x1b]0;My Title\x07after");
    }

    #[test]
    fn test_escape_buffer_cursor_forward_split() {
        let mut buf = EscapeAwareBuffer::new();
        // This is the actual bug case: ESC[C (cursor forward) split as ESC[ then C
        let out1 = buf.push("content\x1b[");
        assert_eq!(out1, "content");
        let out2 = buf.push("Cmore");
        assert_eq!(out2, "\x1b[Cmore");
    }

    #[test]
    fn test_escape_buffer_flush_at_eof() {
        let mut buf = EscapeAwareBuffer::new();
        let out = buf.push("text\x1b[31");
        assert_eq!(out, "text");
        // Flush sends remaining even if incomplete
        let flushed = buf.flush();
        assert_eq!(flushed, "\x1b[31");
    }

    #[test]
    fn test_escape_buffer_multiple_sequences() {
        let mut buf = EscapeAwareBuffer::new();
        let out = buf.push("\x1b[1m\x1b[31mBold Red\x1b[0m normal");
        assert_eq!(out, "\x1b[1m\x1b[31mBold Red\x1b[0m normal");
    }

    #[test]
    fn test_escape_buffer_osc_with_st_terminator() {
        let mut buf = EscapeAwareBuffer::new();
        // OSC with ST (ESC \) terminator
        assert_eq!(
            buf.push("\x1b]8;;https://example.com\x1b\\Click\x1b]8;;\x1b\\"),
            "\x1b]8;;https://example.com\x1b\\Click\x1b]8;;\x1b\\"
        );
    }

    #[test]
    fn test_escape_buffer_empty_input() {
        let mut buf = EscapeAwareBuffer::new();
        assert_eq!(buf.push(""), "");
    }

    #[test]
    fn test_escape_buffer_cap_prevents_unbounded_growth() {
        let mut buf = EscapeAwareBuffer::new();
        // Fake "incomplete escape" that's really garbage — over 256 bytes
        let long_fake = format!("\x1b]{}", "x".repeat(300));
        let out = buf.push(&long_fake);
        // Should emit raw since it exceeds cap
        assert_eq!(out, long_fake);
    }

    // --- Cached config in AppState tests ---

    // The 116-field literal that used to sit here was a strictly worse copy of
    // `tests_support::make_test_app_state`: same values everywhere it mattered,
    // but a SHARED `test-tuic-data` dir, which is the SQLITE_BUSY collision the
    // shared helper documents and avoids. Deleted rather than kept in sync
    // (#678-9a75).
    use super::tests_support::make_test_app_state;

    /// The registry is reached through `AppState` by every MCP task handler, so a
    /// state built without a live one would fail at request time, not at boot.
    #[test]
    fn test_state_carries_a_live_task_registry() {
        use crate::tasks::{TaskKind, TaskStatus, TaskUpdate};

        let state = make_test_app_state();
        let id = state
            .tasks
            .create(TaskKind::AgentSpawn, "owner", Some("sess-1"));
        assert_eq!(state.tasks.get(&id).unwrap().status, TaskStatus::Working);

        state
            .tasks
            .set_status(&id, TaskStatus::Completed, TaskUpdate::default())
            .expect("transition");
        assert!(
            state
                .tasks
                .set_status(&id, TaskStatus::Working, TaskUpdate::default())
                .is_err(),
            "terminal immutability must hold through AppState too"
        );
    }

    #[test]
    fn test_cached_config_returns_default() {
        let state = make_test_app_state();
        let config = state.config.read();
        assert_eq!(config.font_family, "JetBrains Mono");
        assert_eq!(config.theme, "commander");
        assert!(config.mcp_server_enabled);
    }

    #[test]
    fn test_cached_config_write_updates_cache() {
        let state = make_test_app_state();
        {
            let mut config = state.config.write();
            config.font_size = 20;
            config.theme = "dracula".to_string();
        }
        let config = state.config.read();
        assert_eq!(config.font_size, 20);
        assert_eq!(config.theme, "dracula");
    }

    #[test]
    fn test_cached_config_full_replacement() {
        let state = make_test_app_state();
        let new_config = crate::config::AppConfig {
            mcp_server_enabled: true,
            font_family: "Fira Code".to_string(),
            ..crate::config::AppConfig::default()
        };
        *state.config.write() = new_config;

        let config = state.config.read();
        assert!(config.mcp_server_enabled);
        assert_eq!(config.font_family, "Fira Code");
    }

    // --- moka git cache tests (Step 1) ---

    fn sample_repo_info(path: &str, name: &str) -> crate::git::RepoInfo {
        crate::git::RepoInfo {
            path: path.to_string(),
            name: name.to_string(),
            initials: name[..1].to_uppercase(),
            branch: "main".to_string(),
            status: "clean".to_string(),
            is_git_repo: true,
        }
    }

    /// MANDATORY Step 1 behavior: 50 concurrent `get_with` on one missing key
    /// invoke the loader exactly once (coalescing collapses the fan-out).
    #[test]
    fn git_cache_get_with_coalesces_concurrent_loads() {
        let cache: GitCache<String> = build_git_cache(GIT_CACHE_TTL, Arc::new(AtomicU64::new(0)));
        let calls = Arc::new(AtomicUsize::new(0));

        let handles: Vec<_> = (0..50)
            .map(|_| {
                let cache = cache.clone();
                let calls = Arc::clone(&calls);
                std::thread::spawn(move || {
                    cache.get_with("k".to_string(), || {
                        calls.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
                        // Hold the load long enough that the other 49 threads pile
                        // up on the same key and must wait for this single compute.
                        std::thread::sleep(Duration::from_millis(50));
                        Arc::new("value".to_string())
                    })
                })
            })
            .collect();

        for h in handles {
            assert_eq!(&*h.join().unwrap(), "value");
        }
        assert_eq!(
            calls.load(std::sync::atomic::Ordering::SeqCst),
            1,
            "loader must run exactly once for 50 concurrent get_with on a cold key"
        );
    }

    /// After the TTL elapses the entry expires and the next `get_with` recomputes.
    #[test]
    fn git_cache_ttl_expiry_recomputes() {
        let cache: moka::sync::Cache<String, Arc<u32>> = moka::sync::Cache::builder()
            .max_capacity(8)
            .time_to_live(Duration::from_millis(80))
            .build();
        let calls = Arc::new(AtomicUsize::new(0));

        let load = |n: u32| {
            let calls = Arc::clone(&calls);
            move || {
                calls.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
                Arc::new(n)
            }
        };

        let _ = cache.get_with("k".to_string(), load(1));
        let _ = cache.get_with("k".to_string(), load(1)); // hit — no recompute
        assert_eq!(calls.load(std::sync::atomic::Ordering::SeqCst), 1);

        std::thread::sleep(Duration::from_millis(120));
        cache.run_pending_tasks();
        let v = cache.get_with("k".to_string(), load(2)); // expired — recompute
        assert_eq!(*v, 2);
        assert_eq!(calls.load(std::sync::atomic::Ordering::SeqCst), 2);
    }

    /// Step 4: only TTL-expiry evictions (`RemovalCause::Expired`) bump the
    /// watcher-miss counter; explicit invalidations do not.
    #[test]
    fn eviction_observability_counts_only_ttl_expiry() {
        let counter = Arc::new(AtomicU64::new(0));
        let cache: GitCache<u32> = build_git_cache(Duration::from_millis(60), Arc::clone(&counter));

        // Expired path: insert, let it age out, force maintenance.
        cache.insert("expired".to_string(), Arc::new(1));
        std::thread::sleep(Duration::from_millis(120));
        cache.run_pending_tasks();
        assert_eq!(
            counter.load(std::sync::atomic::Ordering::Relaxed),
            1,
            "TTL expiry must increment the watcher-miss counter"
        );

        // Explicit path: insert then invalidate — must NOT increment.
        cache.insert("explicit".to_string(), Arc::new(2));
        cache.run_pending_tasks();
        cache.invalidate("explicit");
        cache.run_pending_tasks();
        assert_eq!(
            counter.load(std::sync::atomic::Ordering::Relaxed),
            1,
            "explicit invalidation must NOT increment the watcher-miss counter"
        );
    }

    #[test]
    fn test_clear_caches_empties_all() {
        let state = make_test_app_state();
        state.git_cache.repo_info.insert(
            "/some/path".to_string(),
            Arc::new(sample_repo_info("/some/path", "test")),
        );
        state
            .git_cache
            .github_status
            .insert("/some/path".to_string(), Arc::new(vec![]));

        assert!(state.git_cache.repo_info.get("/some/path").is_some());
        assert!(state.git_cache.github_status.get("/some/path").is_some());

        state.clear_caches();

        assert!(state.git_cache.repo_info.get("/some/path").is_none());
        assert!(state.git_cache.github_status.get("/some/path").is_none());
    }

    #[test]
    fn test_invalidate_repo_caches_removes_specific_path() {
        let state = make_test_app_state();
        state.git_cache.repo_info.insert(
            "/repo/a".to_string(),
            Arc::new(sample_repo_info("/repo/a", "a")),
        );
        state.git_cache.repo_info.insert(
            "/repo/b".to_string(),
            Arc::new(sample_repo_info("/repo/b", "b")),
        );

        state.invalidate_repo_caches("/repo/a");

        assert!(state.git_cache.repo_info.get("/repo/a").is_none());
        assert!(state.git_cache.repo_info.get("/repo/b").is_some());
    }

    // --- KittyKeyboardState tests ---

    #[test]
    fn test_kitty_state_default_flags_zero() {
        let state = KittyKeyboardState::new();
        assert_eq!(state.current_flags(), 0);
    }

    #[test]
    fn test_kitty_state_push_sets_flags() {
        let mut state = KittyKeyboardState::new();
        state.push(1);
        assert_eq!(state.current_flags(), 1);
    }

    #[test]
    fn test_kitty_state_push_pop_stack() {
        let mut state = KittyKeyboardState::new();
        state.push(1);
        state.push(3);
        assert_eq!(state.current_flags(), 3);
        state.pop();
        assert_eq!(state.current_flags(), 1);
        state.pop();
        assert_eq!(state.current_flags(), 0);
    }

    #[test]
    fn test_kitty_state_pop_underflow_safe() {
        let mut state = KittyKeyboardState::new();
        state.pop(); // Should not panic
        assert_eq!(state.current_flags(), 0);
        state.pop(); // Still safe
        assert_eq!(state.current_flags(), 0);
    }

    // --- strip_kitty_sequences tests ---

    #[test]
    fn test_strip_kitty_plain_text_passthrough() {
        let (out, actions) = strip_kitty_sequences("hello world");
        assert_eq!(out, "hello world");
        assert!(actions.is_empty());
    }

    #[test]
    fn test_strip_kitty_push_single_digit() {
        let (out, actions) = strip_kitty_sequences("\x1b[>1u");
        assert_eq!(out, "");
        assert_eq!(actions, vec![KittyAction::Push(1)]);
    }

    #[test]
    fn test_strip_kitty_push_multi_digit() {
        let (out, actions) = strip_kitty_sequences("\x1b[>15u");
        assert_eq!(out, "");
        assert_eq!(actions, vec![KittyAction::Push(15)]);
    }

    #[test]
    fn test_strip_kitty_pop() {
        let (out, actions) = strip_kitty_sequences("\x1b[<u");
        assert_eq!(out, "");
        assert_eq!(actions, vec![KittyAction::Pop]);
    }

    #[test]
    fn test_strip_kitty_query() {
        let (out, actions) = strip_kitty_sequences("\x1b[?u");
        assert_eq!(out, "");
        assert_eq!(actions, vec![KittyAction::Query]);
    }

    #[test]
    fn test_strip_kitty_embedded_in_text() {
        let (out, actions) = strip_kitty_sequences("before\x1b[>1uafter");
        assert_eq!(out, "beforeafter");
        assert_eq!(actions, vec![KittyAction::Push(1)]);
    }

    #[test]
    fn test_strip_kitty_multiple_actions() {
        let (out, actions) = strip_kitty_sequences("\x1b[>1u\x1b[?u\x1b[<u");
        assert_eq!(out, "");
        assert_eq!(
            actions,
            vec![KittyAction::Push(1), KittyAction::Query, KittyAction::Pop,]
        );
    }

    #[test]
    fn test_strip_kitty_sgr_mouse_passthrough() {
        // SGR mouse: ESC [ < 0 ; 35 ; 16 M — starts with ESC[< but next byte is digit, not 'u'
        let input = "\x1b[<0;35;16M";
        let (out, actions) = strip_kitty_sequences(input);
        assert_eq!(out, input);
        assert!(actions.is_empty());
    }

    #[test]
    fn test_strip_kitty_dec_private_mode_passthrough() {
        // DEC private mode: ESC [ ? 1049 h — starts with ESC[? but next byte is digit, not 'u'
        let input = "\x1b[?1049h";
        let (out, actions) = strip_kitty_sequences(input);
        assert_eq!(out, input);
        assert!(actions.is_empty());
    }

    #[test]
    fn test_strip_kitty_normal_csi_passthrough() {
        // Normal CSI (SGR color): should pass through unchanged
        let input = "\x1b[31mRed\x1b[0m";
        let (out, actions) = strip_kitty_sequences(input);
        assert_eq!(out, input);
        assert!(actions.is_empty());
    }

    #[test]
    fn test_strip_kitty_fast_path_no_trigger() {
        // No ESC[> or ESC[< or ESC[? — should take fast path
        let input = "\x1b[31m\x1b[0mhello";
        let (out, actions) = strip_kitty_sequences(input);
        assert_eq!(out, input);
        assert!(actions.is_empty());
    }

    #[test]
    fn test_strip_kitty_push_zero_flags() {
        let (out, actions) = strip_kitty_sequences("\x1b[>0u");
        assert_eq!(out, "");
        assert_eq!(actions, vec![KittyAction::Push(0)]);
    }

    #[test]
    fn test_strip_kitty_incomplete_push_no_digits() {
        // ESC [ > u (no digits) — not a valid push, should pass through
        let input = "\x1b[>u";
        let (out, actions) = strip_kitty_sequences(input);
        // The ESC is emitted, then [>u follows as normal text
        assert_eq!(out, input);
        assert!(actions.is_empty());
    }

    #[test]
    fn test_strip_kitty_preserves_utf8_box_drawing() {
        // Box-drawing chars (╭│╰) are 3-byte UTF-8 — must not be corrupted
        let input = "╭──────╮\n│ hello │\n╰──────╯";
        let (out, actions) = strip_kitty_sequences(input);
        assert_eq!(out, input);
        assert!(actions.is_empty());
    }

    #[test]
    fn test_strip_kitty_utf8_mixed_with_sequences() {
        // Kitty sequence embedded between multi-byte UTF-8 text
        let input = "╭──╮\x1b[>1u╰──╯";
        let (out, actions) = strip_kitty_sequences(input);
        assert_eq!(out, "╭──╮╰──╯");
        assert_eq!(actions, vec![KittyAction::Push(1)]);
    }

    #[test]
    fn test_strip_kitty_emoji_passthrough() {
        let input = "🦀 hello \x1b[?u 🎉";
        let (out, actions) = strip_kitty_sequences(input);
        assert_eq!(out, "🦀 hello  🎉");
        assert_eq!(actions, vec![KittyAction::Query]);
    }

    #[test]
    fn test_strip_kitty_cjk_passthrough() {
        let input = "漢字\x1b[<u日本語";
        let (out, actions) = strip_kitty_sequences(input);
        assert_eq!(out, "漢字日本語");
        assert_eq!(actions, vec![KittyAction::Pop]);
    }

    #[test]
    fn test_strip_kitty_fast_path_returns_borrowed() {
        use std::borrow::Cow;
        let input = "hello world";
        let (out, actions) = strip_kitty_sequences(input);
        assert!(actions.is_empty());
        assert!(
            matches!(out, Cow::Borrowed(_)),
            "fast path should return Cow::Borrowed"
        );
        assert_eq!(&*out, input);
    }

    #[test]
    fn test_strip_kitty_slow_path_returns_owned() {
        use std::borrow::Cow;
        let input = "before\x1b[>1uafter";
        let (out, actions) = strip_kitty_sequences(input);
        assert!(!actions.is_empty());
        assert!(
            matches!(out, Cow::Owned(_)),
            "slow path should return Cow::Owned"
        );
        assert_eq!(&*out, "beforeafter");
    }

    // Helpers for session-state accumulator tests
    fn make_parsed(type_: &str, extra: serde_json::Value) -> AppEvent {
        let mut obj = serde_json::json!({ "type": type_ });
        if let (serde_json::Value::Object(m), serde_json::Value::Object(extra_m)) =
            (&mut obj, extra)
        {
            m.extend(extra_m);
        }
        AppEvent::PtyParsed {
            session_id: "s1".to_string(),
            parsed: obj.into(),
        }
    }

    fn apply(state: &Arc<AppState>, event: &AppEvent) -> SessionState {
        AppState::apply_event_to_session_state(state, event);
        state
            .session_maps
            .session_states
            .get("s1")
            .map(|s| s.clone())
            .unwrap_or_default()
    }

    fn fresh_state() -> Arc<AppState> {
        let s = Arc::new(make_test_app_state());
        s.session_maps
            .session_states
            .insert("s1".to_string(), SessionState::default());
        // Initialize last_output_ms for shell_state derivation
        s.session_maps
            .last_output_ms
            .insert("s1".to_string(), AtomicU64::new(0));
        let mut silence = crate::pty::SilenceState::new();
        silence.confirm_idle();
        s.session_maps
            .silence_states
            .insert("s1".to_string(), Arc::new(parking_lot::Mutex::new(silence)));
        s
    }

    fn session_created(session_id: &str, agent_type: &str) -> AppEvent {
        AppEvent::SessionCreated {
            session_id: session_id.to_string(),
            cwd: None,
            agent_type: Some(agent_type.to_string()),
            display_name: None,
            parent_session: None,
        }
    }

    /// Catches: a delayed creation event re-arms a preset after the agent was
    /// observed and exited, reopening unattended input into the returned shell.
    #[test]
    fn delayed_session_created_cannot_rearm_an_observed_agent_after_exit() {
        let state = Arc::new(make_test_app_state());
        let mut session = SessionState::default();
        session.seed_configured_agent(Some("claude".into()));
        session.agent_foreground_observed = true;
        session.agent_type = None;
        session.agent_type_from_run_config = false;
        state
            .session_maps
            .session_states
            .insert("late-created".into(), session);
        AppState::apply_event_to_session_state(&state, &session_created("late-created", "claude"));
        let row = state
            .session_maps
            .session_states
            .get("late-created")
            .unwrap();
        assert_eq!(row.agent_type, None);
        assert!(!row.agent_type_from_run_config);
        assert!(row.agent_foreground_observed);
    }

    /// Catches: SessionCreated building the row without `last_activity_ms` or
    /// `agent_type` (new-entry path), or not refreshing them on a row that a
    /// PtyParsed event created first (existing-entry path).
    #[test]
    fn session_created_stamps_activity_time_and_agent_type_on_new_and_existing_rows() {
        let state = Arc::new(make_test_app_state());

        AppState::apply_event_to_session_state(&state, &session_created("fresh", "claude"));
        let row = state
            .session_maps
            .session_states
            .get("fresh")
            .unwrap()
            .clone();
        assert!(row.last_activity_ms > 0, "new row lost its activity time");
        assert_eq!(row.agent_type.as_deref(), Some("claude"));

        state
            .session_maps
            .session_states
            .insert("existing".to_string(), SessionState::default());
        AppState::apply_event_to_session_state(&state, &session_created("existing", "codex"));
        let row = state
            .session_maps
            .session_states
            .get("existing")
            .unwrap()
            .clone();
        assert!(
            row.last_activity_ms > 0,
            "existing row kept a zero activity time"
        );
        assert_eq!(row.agent_type.as_deref(), Some("codex"));
    }

    /// Catches: `event_type == "question" && epoch_matches` turned into `||`, so a
    /// question from a previous turn still records awaiting evidence on the
    /// session's SilenceState even though its own state update is rejected.
    #[test]
    fn stale_epoch_question_records_no_awaiting_evidence() {
        let state = fresh_state();
        let stale = make_parsed(
            "question",
            serde_json::json!({ "prompt_text": "old?", "_turn_epoch": 99 }),
        );
        let row = apply(&state, &stale);
        assert!(!row.awaiting_input, "a stale question parked the session");
        assert_eq!(
            state
                .session_maps
                .silence_states
                .get("s1")
                .unwrap()
                .lock()
                .awaiting_rank(),
            None,
            "a stale question recorded awaiting evidence"
        );

        let current = make_parsed("question", serde_json::json!({ "prompt_text": "now?" }));
        assert!(apply(&state, &current).awaiting_input);
        assert!(
            state
                .session_maps
                .silence_states
                .get("s1")
                .unwrap()
                .lock()
                .awaiting_rank()
                .is_some(),
            "a current question must record awaiting evidence"
        );
    }

    // Catches: admitting a current non-question as awaiting evidence when && becomes ||.
    #[test]
    fn current_non_question_does_not_record_awaiting_evidence() {
        let state = fresh_state();
        apply(
            &state,
            &make_parsed(
                "intent",
                serde_json::json!({"text": "Working", "_turn_epoch": 0}),
            ),
        );
        assert!(!state.session_state_with_shell("s1").unwrap().awaiting_input);
        assert_eq!(
            state
                .session_maps
                .silence_states
                .get("s1")
                .unwrap()
                .lock()
                .awaiting_rank(),
            None
        );
    }

    // Catches: a same-epoch clear for another question retracts the current approval.
    #[test]
    fn protocol_clear_for_another_question_preserves_the_current_approval() {
        let state = fresh_state();
        apply(
            &state,
            &make_parsed(
                "question",
                serde_json::json!({"prompt_text": "Approve deploy?", "confident": true, "_turn_epoch": 0}),
            ),
        );
        let row = apply(
            &state,
            &make_parsed(
                "protocol-question-cleared",
                serde_json::json!({"expected_question_text": "Approve delete?", "_turn_epoch": 0}),
            ),
        );
        assert!(row.awaiting_input);
        assert_eq!(row.question_text.as_deref(), Some("Approve deploy?"));
        let row = apply(
            &state,
            &make_parsed(
                "protocol-question-cleared",
                serde_json::json!({"expected_question_text": "Approve deploy?", "_turn_epoch": 0}),
            ),
        );
        assert!(!row.awaiting_input);
        assert_eq!(row.question_text, None);
    }

    /// Catches: deleting a parsed-state arm silently drops usage, errors, menu
    /// entries or subtask counts from the snapshot consumed by clients.
    #[test]
    fn parsed_agent_details_reach_the_client_snapshot_and_can_be_replaced() {
        let state = fresh_state();
        for (kind, payload) in [
            ("usage-limit", serde_json::json!({ "percentage": 81 })),
            (
                "api-error",
                serde_json::json!({ "matched_text": "authentication failed" }),
            ),
            (
                "slash-menu",
                serde_json::json!({ "items": [{
                "command": "/help", "description": "Show commands", "highlighted": true
            }] }),
            ),
            ("active-subtasks", serde_json::json!({ "count": 3 })),
        ] {
            apply(&state, &make_parsed(kind, payload));
        }
        let snapshot = state.session_state_with_shell("s1").unwrap();
        assert_eq!(snapshot.usage_limit_pct, Some(81));
        assert_eq!(
            snapshot.last_error.as_deref(),
            Some("authentication failed")
        );
        assert_eq!(
            snapshot.slash_menu_items,
            Some(vec![crate::output_parser::SlashMenuItem {
                command: "/help".into(),
                description: "Show commands".into(),
                highlighted: true,
            }])
        );
        assert_eq!(snapshot.active_sub_tasks, 3);

        apply(
            &state,
            &make_parsed("usage-limit", serde_json::json!({ "percentage": 0 })),
        );
        apply(
            &state,
            &make_parsed("slash-menu", serde_json::json!({ "items": [] })),
        );
        apply(
            &state,
            &make_parsed("active-subtasks", serde_json::json!({ "count": 0 })),
        );
        apply(
            &state,
            &make_parsed("status-line", serde_json::json!({ "task_name": "Working" })),
        );
        let snapshot = state.session_state_with_shell("s1").unwrap();
        assert_eq!(snapshot.usage_limit_pct, Some(0));
        assert_eq!(snapshot.last_error, None);
        assert_eq!(snapshot.slash_menu_items, Some(vec![]));
        assert_eq!(snapshot.active_sub_tasks, 0);
    }

    /// Catches: an inverted or removed epoch guard retracts the current prompt
    /// on a stale clear, or an always-false guard prevents its real answer.
    #[test]
    fn question_clear_preserves_current_prompt_until_matching_epoch_arrives() {
        for (clear_kind, confident) in [
            ("question-cleared", false),
            ("protocol-question-cleared", true),
        ] {
            let state = fresh_state();
            state
                .session_maps
                .session_states
                .get_mut("s1")
                .unwrap()
                .turn_epoch = 2;
            apply(
                &state,
                &make_parsed(
                    "question",
                    serde_json::json!({
                        "prompt_text": "Proceed with the current operation?",
                        "confident": confident,
                        "_turn_epoch": 2,
                    }),
                ),
            );
            let before = state.session_state_with_shell("s1").unwrap();
            assert!(before.awaiting_input);
            let rank = state
                .session_maps
                .silence_states
                .get("s1")
                .unwrap()
                .lock()
                .awaiting_rank();
            assert!(rank.is_some());
            apply(
                &state,
                &make_parsed(
                    clear_kind,
                    serde_json::json!({
                        "expected_question_text": "Proceed with the current operation?",
                        "_turn_epoch": 1,
                    }),
                ),
            );
            let stale = state.session_state_with_shell("s1").unwrap();
            assert!(
                stale.awaiting_input,
                "{clear_kind} cleared a newer question"
            );
            assert_eq!(
                stale.question_text.as_deref(),
                Some("Proceed with the current operation?")
            );
            assert_eq!(
                state
                    .session_maps
                    .silence_states
                    .get("s1")
                    .unwrap()
                    .lock()
                    .awaiting_rank(),
                rank
            );

            apply(
                &state,
                &make_parsed(
                    clear_kind,
                    serde_json::json!({
                        "expected_question_text": "Proceed with the current operation?",
                        "_turn_epoch": 2,
                    }),
                ),
            );
            let answered = state.session_state_with_shell("s1").unwrap();
            assert!(
                !answered.awaiting_input,
                "{clear_kind} ignored the current answer"
            );
            assert_eq!(answered.question_text, None);
            assert_eq!(
                state
                    .session_maps
                    .silence_states
                    .get("s1")
                    .unwrap()
                    .lock()
                    .awaiting_rank(),
                None
            );
        }
    }

    #[test]
    fn pending_acp_question_alerts_the_chat_once_while_desktop_is_away() {
        let state = fresh_state();
        let (private, _) = crate::push::generate_vapid_keys().unwrap();
        {
            let mut config = state.config.write();
            config.services.push.enabled = true;
            config.services.push.vapid_private_key = private;
        }
        state.push_store.upsert(crate::push::PushSubscription {
            endpoint: "https://fcm.googleapis.com/fcm/send/example".to_string(),
            keys: crate::push::PushSubscriptionKeys {
                p256dh: "unused".to_string(),
                auth: "unused".to_string(),
            },
            created_at: chrono::Utc::now(),
        });
        state
            .desktop_window_focused
            .store(false, std::sync::atomic::Ordering::Relaxed);
        let request_id = crate::acp::AcpHostRequestId::new();
        let notice = crate::acp::AcpNotice {
            connection_id: crate::acp::AcpConnectionId::new(),
            generation: 1,
            session_id: Some(agent_client_protocol::schema::v1::SessionId::new(
                "conversation-1",
            )),
            request_id: Some(request_id),
            sequence: 7,
            kind: crate::acp::AcpNoticeKind::InteractionPending,
        };

        assert_eq!(
            AppState::mobile_push_for_acp_notice(&state, &notice, Some((request_id, "/repo"))),
            Some((
                "/mobile?repo=%2Frepo&session=conversation-1".to_string(),
                "AI Chat: response needed".to_string()
            ))
        );
        assert_eq!(
            AppState::mobile_push_for_acp_notice(&state, &notice, Some((request_id, "/repo"))),
            None,
            "a repeated notice must not alert within 30 seconds"
        );
        let mut other_conversation = notice.clone();
        other_conversation.session_id = Some(agent_client_protocol::schema::v1::SessionId::new(
            "conversation-2",
        ));
        assert!(
            AppState::mobile_push_for_acp_notice(
                &state,
                &other_conversation,
                Some((request_id, "/repo"))
            )
            .is_some(),
            "one conversation must not spend another's push budget"
        );
        let mut special_conversation = notice.clone();
        special_conversation.session_id = Some(agent_client_protocol::schema::v1::SessionId::new(
            "conversation-special",
        ));
        assert_eq!(
            AppState::mobile_push_for_acp_notice(
                &state,
                &special_conversation,
                Some((request_id, "/repo/a & b")),
            ),
            Some((
                "/mobile?repo=%2Frepo%2Fa+%26+b&session=conversation-special".to_string(),
                "AI Chat: response needed".to_string(),
            )),
            "repository paths must stay inside the deep-link query value"
        );
        let mut card = notice.clone();
        card.kind = crate::acp::AcpNoticeKind::Card;
        card.request_id = None;
        assert_eq!(
            AppState::mobile_push_for_acp_session(&state, &card, "/repo"),
            None,
            "a card must not bypass the question's 30-second conversation budget"
        );
        card.session_id = Some(agent_client_protocol::schema::v1::SessionId::new(
            "card-only",
        ));
        assert_eq!(
            AppState::mobile_push_for_acp_session(&state, &card, "/repo"),
            Some((
                "/mobile?repo=%2Frepo&session=card-only".to_string(),
                "AI Chat: new notice".to_string()
            ))
        );
        assert_eq!(
            AppState::mobile_push_for_acp_session(&state, &card, "/repo"),
            None,
            "repeated cards must not flood the phone"
        );
        let first_card = state
            .acp_push_last_ms
            .get("card-only")
            .unwrap()
            .value()
            .unwrap();
        *state.acp_push_last_ms.get_mut("card-only").unwrap() =
            Some(first_card.saturating_sub(31_000));
        assert!(AppState::mobile_push_for_acp_session(&state, &card, "/repo").is_some());
        let key = "conversation-1";
        let first = state.acp_push_last_ms.get(key).unwrap().value().unwrap();
        *state.acp_push_last_ms.get_mut(key).unwrap() = Some(first.saturating_sub(31_000));
        assert!(
            AppState::mobile_push_for_acp_notice(&state, &notice, Some((request_id, "/repo")))
                .is_some()
        );
        state.acp_push_last_ms.remove(key);
        let winners = std::thread::scope(|scope| {
            let calls: Vec<_> = (0..16)
                .map(|_| {
                    scope.spawn(|| {
                        AppState::mobile_push_for_acp_notice(
                            &state,
                            &notice,
                            Some((request_id, "/repo")),
                        )
                    })
                })
                .collect();
            calls
                .into_iter()
                .map(|call| call.join().unwrap().is_some())
                .filter(|sent| *sent)
                .count()
        });
        assert_eq!(
            winners, 1,
            "concurrent notices may spend one slot only once"
        );
    }

    #[test]
    fn answered_or_unrelated_acp_notice_cannot_alert_the_phone() {
        let state = fresh_state();
        let (private, _) = crate::push::generate_vapid_keys().unwrap();
        {
            let mut config = state.config.write();
            config.services.push.enabled = true;
            config.services.push.vapid_private_key = private;
        }
        state.push_store.upsert(crate::push::PushSubscription {
            endpoint: "https://fcm.googleapis.com/fcm/send/example".to_string(),
            keys: crate::push::PushSubscriptionKeys {
                p256dh: "unused".to_string(),
                auth: "unused".to_string(),
            },
            created_at: chrono::Utc::now(),
        });
        state
            .desktop_window_focused
            .store(false, std::sync::atomic::Ordering::Relaxed);
        let request_id = crate::acp::AcpHostRequestId::new();
        let mut notice = crate::acp::AcpNotice {
            connection_id: crate::acp::AcpConnectionId::new(),
            generation: 1,
            session_id: Some(agent_client_protocol::schema::v1::SessionId::new(
                "conversation-1",
            )),
            request_id: Some(request_id),
            sequence: 7,
            kind: crate::acp::AcpNoticeKind::InteractionPending,
        };
        assert_eq!(
            AppState::mobile_push_for_acp_notice(&state, &notice, None),
            None
        );
        assert_eq!(
            AppState::mobile_push_for_acp_notice(
                &state,
                &notice,
                Some((crate::acp::AcpHostRequestId::new(), "/repo"))
            ),
            None
        );
        notice.kind = crate::acp::AcpNoticeKind::Ready;
        assert_eq!(
            AppState::mobile_push_for_acp_notice(&state, &notice, Some((request_id, "/repo"))),
            None
        );
        notice.kind = crate::acp::AcpNoticeKind::InteractionSettled;
        assert_eq!(
            AppState::mobile_push_for_acp_notice(&state, &notice, Some((request_id, "/repo"))),
            None
        );
        state
            .push_store
            .remove("https://fcm.googleapis.com/fcm/send/example");
        notice.kind = crate::acp::AcpNoticeKind::InteractionPending;
        assert_eq!(
            AppState::mobile_push_for_acp_notice(&state, &notice, Some((request_id, "/repo"))),
            None,
            "no subscribed phone must leave the budget free"
        );
        state.push_store.upsert(crate::push::PushSubscription {
            endpoint: "https://fcm.googleapis.com/fcm/send/example".to_string(),
            keys: crate::push::PushSubscriptionKeys {
                p256dh: "unused".to_string(),
                auth: "unused".to_string(),
            },
            created_at: chrono::Utc::now(),
        });
        state.config.write().services.push.enabled = false;
        assert_eq!(
            AppState::mobile_push_for_acp_notice(&state, &notice, Some((request_id, "/repo"))),
            None,
            "disabled push must not alert or spend a slot"
        );
        assert!(state.acp_push_last_ms.get("conversation-1").is_none());
    }

    #[tokio::test]
    async fn live_acp_permission_reaches_one_subscribed_push_service() {
        use base64ct::Encoding;

        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let endpoint = format!("http://{}/push", listener.local_addr().unwrap());
        let (accepted_tx, mut accepted_rx) = tokio::sync::mpsc::unbounded_channel();
        let service = tokio::spawn(async move {
            axum::serve(
                listener,
                axum::Router::new().route(
                    "/push",
                    axum::routing::post(move || {
                        let accepted_tx = accepted_tx.clone();
                        async move {
                            accepted_tx.send(()).unwrap();
                            axum::http::StatusCode::CREATED
                        }
                    }),
                ),
            )
            .await
        });

        let state = fresh_state();
        let (private, public) = crate::push::generate_vapid_keys().unwrap();
        {
            let mut config = state.config.write();
            config.services.push.enabled = true;
            config.services.push.vapid_private_key = private;
            config.services.push.vapid_public_key = public;
        }
        let client_key = p256::ecdsa::SigningKey::random(&mut rand_core::OsRng);
        state.push_store.upsert(crate::push::PushSubscription {
            endpoint,
            keys: crate::push::PushSubscriptionKeys {
                p256dh: base64ct::Base64UrlUnpadded::encode_string(
                    client_key
                        .verifying_key()
                        .to_encoded_point(false)
                        .as_bytes(),
                ),
                auth: base64ct::Base64UrlUnpadded::encode_string(&[7u8; 16]),
            },
            created_at: chrono::Utc::now(),
        });
        state
            .desktop_window_focused
            .store(false, std::sync::atomic::Ordering::Relaxed);
        let pump = AppState::spawn_acp_notice_pump(state.clone());

        let root = tempfile::Builder::new()
            .prefix("acp-push-")
            .tempdir_in(crate::test_support::test_temp_root())
            .unwrap();
        for (source, target) in [
            ("permission-turn.jsonl", "scenario.jsonl"),
            ("ego-initialize.json", "ego-initialize.json"),
        ] {
            std::fs::copy(
                format!("tests/fixtures/acp/{source}"),
                root.path().join(target),
            )
            .unwrap();
        }
        // The nextest `fixture-bins-unix`/`-windows` setup scripts
        // (`.config/nextest.toml`) build this [[bin]] once before the run
        // starts; `cargo nextest run --lib` never builds it on its own.
        let executable = std::env::current_exe()
            .unwrap()
            .parent()
            .unwrap()
            .parent()
            .unwrap()
            .join(format!(
                "tuic-acp-fixture-agent{}",
                std::env::consts::EXE_SUFFIX
            ));
        assert!(
            executable.is_file(),
            "fixture agent missing: {}",
            executable.display()
        );
        let connection = state
            .acp
            .connect(
                &crate::acp::EgoAcpConfig {
                    executable,
                    profile: String::new(),
                },
                crate::acp::AcpConnectRequest {
                    root: root.path().to_path_buf(),
                },
            )
            .await
            .unwrap();
        let session = state
            .acp
            .new_session(
                connection.connection_id,
                crate::acp::AcpSessionAuthority {
                    cwd: root.path().to_path_buf(),
                    additional_directories: Vec::new(),
                    mcp_servers: Vec::new(),
                },
            )
            .await
            .unwrap();
        state
            .acp
            .prompt(
                connection.connection_id,
                session.session_id.clone(),
                vec![agent_client_protocol::schema::v1::ContentBlock::Text(
                    agent_client_protocol::schema::v1::TextContent::new("hello"),
                )],
            )
            .await
            .unwrap();

        tokio::time::timeout(std::time::Duration::from_secs(60), accepted_rx.recv())
            .await
            .expect("pending ACP permission did not reach push service")
            .expect("push service stopped");
        let pending = state
            .acp
            .pending_interactions(connection.connection_id)
            .await
            .unwrap();
        let request_id = pending[0].request_id();
        state
            .acp
            .respond_permission(
                connection.connection_id,
                request_id,
                agent_client_protocol::schema::v1::RequestPermissionOutcome::Selected(
                    agent_client_protocol::schema::v1::SelectedPermissionOutcome::new(
                        agent_client_protocol::schema::v1::PermissionOptionId::new("allow"),
                    ),
                ),
            )
            .await
            .unwrap();
        assert!(
            tokio::time::timeout(std::time::Duration::from_millis(200), accepted_rx.recv())
                .await
                .is_err(),
            "settling a permission must not send another push"
        );
        state
            .acp
            .disconnect(connection.connection_id)
            .await
            .unwrap();
        pump.abort();
        service.abort();
    }

    #[test]
    fn test_session_state_intent_sets_agent_intent() {
        let state = fresh_state();
        let event = make_parsed(
            "intent",
            serde_json::json!({ "text": "fixing the bug", "title": null }),
        );
        let s = apply(&state, &event);
        assert_eq!(s.agent_intent.as_deref(), Some("fixing the bug"));
    }

    /// `PtyActivity` (story 625-56b0) says bytes are flowing; `last_activity_ms`
    /// says when the session last did something notable, and the mobile client
    /// renders it as such (`SessionCard.tsx`). A `tail -f` produces the first
    /// continuously and the second never, so the accumulator must ignore the
    /// pulse — otherwise that column silently becomes "always just now" for any
    /// session with output.
    #[test]
    fn test_session_state_pty_activity_does_not_restamp_last_activity() {
        let state = fresh_state();
        state
            .session_maps
            .session_states
            .get_mut("s1")
            .expect("fresh_state seeds s1")
            .last_activity_ms = 1_000;

        let s = apply(
            &state,
            &AppEvent::PtyActivity {
                session_id: "s1".to_string(),
            },
        );

        assert_eq!(
            s.last_activity_ms, 1_000,
            "the activity pulse must not restamp last_activity_ms"
        );
    }

    #[test]
    fn test_session_state_status_line_sets_current_task() {
        let state = fresh_state();
        let event = make_parsed(
            "status-line",
            serde_json::json!({ "task_name": "Reading files", "full_line": "⏺ Reading files" }),
        );
        let s = apply(&state, &event);
        assert_eq!(s.current_task.as_deref(), Some("Reading files"));
        // status-line also clears error and rate-limit
        assert!(!s.rate_limited);
        assert!(s.last_error.is_none());
    }

    #[test]
    fn test_session_state_user_input_short_does_not_set_last_prompt() {
        let state = fresh_state();
        let event = make_parsed("user-input", serde_json::json!({ "content": "yes" }));
        let s = apply(&state, &event);
        assert!(s.last_prompt.is_none());
    }

    #[test]
    fn test_session_state_user_input_long_sets_last_prompt() {
        let state = fresh_state();
        let long = "please refactor this function to use the new API correctly and efficiently";
        let event = make_parsed("user-input", serde_json::json!({ "content": long }));
        let s = apply(&state, &event);
        assert_eq!(s.last_prompt.as_deref(), Some(long));
    }

    #[test]
    fn test_session_state_user_input_exactly_10_words_sets_last_prompt() {
        let state = fresh_state();
        let ten_words = "one two three four five six seven eight nine ten";
        let event = make_parsed("user-input", serde_json::json!({ "content": ten_words }));
        let s = apply(&state, &event);
        assert_eq!(s.last_prompt.as_deref(), Some(ten_words));
    }

    #[test]
    fn pty_description_is_independent_and_emits_only_on_change() {
        let state = make_test_app_state();
        let session_id = "session-1";
        state
            .session_maps
            .last_prompts
            .insert(session_id.to_string(), "the last user prompt".to_string());
        let mut events = state.event_bus.subscribe();

        state.set_pty_description(session_id, Some("Run validation".to_string()));
        assert_eq!(
            state
                .session_maps
                .pty_descriptions
                .get(session_id)
                .map(|value| value.value().clone()),
            Some("Run validation".to_string())
        );
        assert_eq!(
            state
                .session_maps
                .last_prompts
                .get(session_id)
                .unwrap()
                .value(),
            "the last user prompt"
        );
        assert!(matches!(
            events.try_recv().unwrap(),
            AppEvent::PtyDescriptionChanged { session_id: id, description: Some(value) }
                if id == session_id && value == "Run validation"
        ));

        state.set_pty_description(session_id, Some("Run validation".to_string()));
        assert!(events.try_recv().is_err());

        state.set_pty_description(session_id, None);
        assert!(!state.session_maps.pty_descriptions.contains_key(session_id));
        assert!(matches!(
            events.try_recv().unwrap(),
            AppEvent::PtyDescriptionChanged { session_id: id, description: None }
                if id == session_id
        ));
    }

    /// The desktop learns a session's lifecycle from this push, not from a 1 Hz
    /// `list_active_sessions` poll (#687-be9d). Two properties make that safe:
    /// a real transition produces exactly one push, and a repaint that leaves
    /// the derived state identical produces none. The second is not a nicety —
    /// an agent redrawing its spinner restamps `last_activity_ms` on every
    /// chunk, so pushing on every applied event would be the poll again, at a
    /// higher rate, with the whole `SessionState` attached.
    #[tokio::test]
    async fn session_state_changed_fires_on_a_transition_and_not_on_an_unchanged_repaint() {
        let state = fresh_state();
        let mut bus = state.event_bus.subscribe();
        AppState::spawn_session_state_accumulator(Arc::clone(&state));

        let question = make_parsed(
            "question",
            serde_json::json!({ "prompt_text": "Proceed?", "confident": true }),
        );
        state.emit_pty_event(question.clone());
        // The same screen parsed twice: nothing a client renders has moved.
        state.emit_pty_event(question);
        state.emit_pty_event(make_parsed(
            "user-input",
            serde_json::json!({ "content": "yes" }),
        ));

        // Read until the answer's push arrives and assert on the whole sequence.
        // A silence window would only prove the repaint was slow; the ordered
        // lossless lane lets a leaked repaint show up as an extra `true`.
        let mut awaiting_pushes: Vec<bool> = Vec::new();
        tokio::time::timeout(std::time::Duration::from_secs(5), async {
            loop {
                if let Ok(AppEvent::SessionStateChanged {
                    session_id,
                    state: pushed,
                }) = bus.recv().await
                {
                    assert_eq!(session_id, "s1");
                    let awaiting = pushed.awaiting_input;
                    awaiting_pushes.push(awaiting);
                    if !awaiting {
                        break;
                    }
                }
            }
        })
        .await
        .expect("the accumulator never pushed the answered state back");

        assert_eq!(
            awaiting_pushes,
            vec![true, false],
            "one push per real transition; the identical repaint must add none"
        );
    }

    #[test]
    fn test_session_state_progress_normal_sets_value() {
        let state = fresh_state();
        let event = make_parsed("progress", serde_json::json!({ "state": 1, "value": 42 }));
        let s = apply(&state, &event);
        assert_eq!(s.progress, Some(42));
    }

    #[test]
    fn test_session_state_progress_remove_clears_value() {
        let state = fresh_state();
        // First set a value
        let set_event = make_parsed("progress", serde_json::json!({ "state": 1, "value": 75 }));
        apply(&state, &set_event);
        // Then remove it
        let remove_event = make_parsed("progress", serde_json::json!({ "state": 0, "value": 0 }));
        let s = apply(&state, &remove_event);
        assert!(s.progress.is_none());
    }

    #[test]
    fn test_session_state_suggest_sets_suggested_actions() {
        let state = fresh_state();
        let event = make_parsed(
            "suggest",
            serde_json::json!({ "items": ["Run tests", "Review diff"] }),
        );
        let s = apply(&state, &event);
        assert_eq!(
            s.suggested_actions,
            Some(vec!["Run tests".to_string(), "Review diff".to_string()])
        );
    }

    #[tokio::test]
    async fn queued_prior_turn_suggest_cannot_restore_completion_after_submission() {
        let state = fresh_state();
        state
            .session_maps
            .session_states
            .get_mut("s1")
            .unwrap()
            .agent_type = Some("codex".to_string());
        state.session_maps.shell_states.insert(
            "s1".to_string(),
            std::sync::atomic::AtomicU8::new(crate::pty::SHELL_IDLE),
        );
        let queued_suggest = make_parsed(
            "suggest",
            serde_json::json!({
                "items": ["stale action"],
                "_turn_epoch": 0,
            }),
        );

        AppState::spawn_session_state_accumulator(state.clone());
        state.emit_pty_event(queued_suggest);
        // No await above: the Suggest is queued in the async accumulator when
        // the new input advances the turn epoch.
        crate::pty::note_submitted_input(&state, "s1");
        tokio::time::timeout(std::time::Duration::from_secs(1), async {
            loop {
                if state
                    .session_maps
                    .session_states
                    .get("s1")
                    .is_some_and(|session| session.last_activity_ms > 0)
                {
                    break;
                }
                tokio::task::yield_now().await;
            }
        })
        .await
        .expect("queued Suggest must drain through the accumulator");
        state
            .session_maps
            .shell_states
            .get("s1")
            .unwrap()
            .store(crate::pty::SHELL_IDLE, std::sync::atomic::Ordering::Release);

        let snapshot = state.session_state_with_shell("s1").unwrap();
        assert_eq!(snapshot.turn_epoch, 1);
        assert!(snapshot.suggested_actions.is_none());
        assert_eq!(snapshot.agent_state.as_deref(), Some("idle"));
    }

    #[tokio::test]
    async fn sticky_state_transitions_survive_global_broadcast_lag() {
        let state = fresh_state();
        AppState::spawn_session_state_accumulator(state.clone());

        for index in 0..600 {
            state.emit_pty_event(make_parsed(
                "question",
                serde_json::json!({
                    "prompt_text": format!("transient {index}"),
                    "confident": false,
                }),
            ));
            state.emit_pty_event(make_parsed("question-cleared", serde_json::json!({})));
        }
        state.emit_pty_event(make_parsed(
            "question",
            serde_json::json!({
                "prompt_text": "final prompt",
                "confident": false,
            }),
        ));

        tokio::time::timeout(std::time::Duration::from_secs(2), async {
            loop {
                if state
                    .session_maps
                    .session_states
                    .get("s1")
                    .is_some_and(|session| {
                        session.awaiting_input
                            && session.question_text.as_deref() == Some("final prompt")
                    })
                {
                    break;
                }
                tokio::task::yield_now().await;
            }
        })
        .await
        .expect("lossless state lane must preserve the final sticky transition");
    }

    #[test]
    fn first_pty_event_creates_and_updates_session_state() {
        let state = Arc::new(make_test_app_state());
        let snapshot = apply(
            &state,
            &make_parsed(
                "question",
                serde_json::json!({ "prompt_text": "first prompt", "confident": false }),
            ),
        );

        assert!(snapshot.awaiting_input);
        assert_eq!(snapshot.question_text.as_deref(), Some("first prompt"));
    }

    #[test]
    fn test_session_state_status_line_clears_suggested_actions() {
        let state = fresh_state();
        // Set suggestions
        let suggest = make_parsed("suggest", serde_json::json!({ "items": ["Deploy"] }));
        apply(&state, &suggest);
        // Status-line should clear them
        let status = make_parsed("status-line", serde_json::json!({ "task_name": "Working" }));
        let s = apply(&state, &status);
        assert!(s.suggested_actions.is_none());
    }

    #[test]
    fn test_session_state_question_sets_awaiting_input() {
        let state = fresh_state();
        let event = make_parsed(
            "question",
            serde_json::json!({ "prompt_text": "Do you want to proceed?" }),
        );
        let s = apply(&state, &event);
        assert!(s.awaiting_input);
        assert_eq!(s.question_text.as_deref(), Some("Do you want to proceed?"));
        assert!(
            !s.question_confident,
            "silence-based question should not be confident"
        );
    }

    #[tokio::test]
    async fn a_question_without_a_phone_subscription_does_not_consume_the_away_push_budget() {
        let state = fresh_state();
        let (private, public) = crate::push::generate_vapid_keys().unwrap();
        {
            let mut config = state.config.write();
            config.services.push.enabled = true;
            config.services.push.vapid_private_key = private;
            config.services.push.vapid_public_key = public;
        }
        let question = make_parsed(
            "question",
            serde_json::json!({ "prompt_text": "Should I deploy now?", "confident": true }),
        );
        let at_desk = apply(&state, &question);
        assert!(
            at_desk.last_push_ms.is_none(),
            "a question without a subscribed phone must not spend the 30-second limit"
        );

        state.push_store.upsert(crate::push::PushSubscription {
            endpoint: "https://fcm.googleapis.com/fcm/send/example".to_string(),
            keys: crate::push::PushSubscriptionKeys {
                p256dh: "unused".to_string(),
                auth: "unused".to_string(),
            },
            created_at: chrono::Utc::now(),
        });
        state
            .desktop_window_focused
            .store(false, std::sync::atomic::Ordering::Relaxed);
        let away = apply(&state, &question);
        assert!(
            away.last_push_ms.is_some(),
            "the same question can alert once Boss leaves"
        );

        let first = away.last_push_ms.unwrap();
        let recent = first.saturating_sub(1_000);
        state
            .session_maps
            .session_states
            .get_mut("s1")
            .unwrap()
            .last_push_ms = Some(recent);
        let rate_limited = apply(&state, &question);
        assert_eq!(
            rate_limited.last_push_ms,
            Some(recent),
            "a repeated question within 30 seconds must not alert twice"
        );

        let old = first.saturating_sub(31_000);
        state
            .session_maps
            .session_states
            .get_mut("s1")
            .unwrap()
            .last_push_ms = Some(old);
        let eligible = apply(&state, &question);
        assert!(
            eligible.last_push_ms.unwrap() > old,
            "an older question alert must not block a new one"
        );
    }

    #[tokio::test]
    async fn completion_push_shares_per_session_window_with_question() {
        use base64ct::Encoding;

        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let endpoint = format!("http://{}/push", listener.local_addr().unwrap());
        let (sent, mut accepted) = tokio::sync::mpsc::unbounded_channel();
        let server = tokio::spawn(async move {
            axum::serve(
                listener,
                axum::Router::new().route(
                    "/push",
                    axum::routing::post(move || {
                        let sent = sent.clone();
                        async move {
                            sent.send(()).unwrap();
                            axum::http::StatusCode::CREATED
                        }
                    }),
                ),
            )
            .await
            .unwrap();
        });

        let state = fresh_state();
        let (private, public) = crate::push::generate_vapid_keys().unwrap();
        {
            let mut config = state.config.write();
            config.services.push.enabled = true;
            config.services.push.vapid_private_key = private;
            config.services.push.vapid_public_key = public;
        }
        state
            .desktop_window_focused
            .store(false, std::sync::atomic::Ordering::Relaxed);
        let exit = AppEvent::PtyExit {
            session_id: "s1".to_string(),
        };
        let without_phone = apply(&state, &exit);
        assert!(
            without_phone.last_push_ms.is_none(),
            "completion without a subscriber must leave the push budget available"
        );
        let client_key = p256::ecdsa::SigningKey::random(&mut rand_core::OsRng);
        state.push_store.upsert(crate::push::PushSubscription {
            endpoint,
            keys: crate::push::PushSubscriptionKeys {
                p256dh: base64ct::Base64UrlUnpadded::encode_string(
                    client_key
                        .verifying_key()
                        .to_encoded_point(false)
                        .as_bytes(),
                ),
                auth: base64ct::Base64UrlUnpadded::encode_string(&[7u8; 16]),
            },
            created_at: chrono::Utc::now(),
        });
        let question = make_parsed(
            "question",
            serde_json::json!({ "prompt_text": "Done?", "confident": true }),
        );
        let asked = apply(&state, &question);
        let first = asked.last_push_ms.expect("question spent the push budget");
        tokio::time::timeout(std::time::Duration::from_secs(60), accepted.recv())
            .await
            .expect("question push never reached the local service")
            .expect("local service closed");

        apply(&state, &exit);
        assert!(
            tokio::time::timeout(std::time::Duration::from_secs(2), accepted.recv())
                .await
                .is_err(),
            "completion bypassed the question's 30-second push limit"
        );

        state
            .session_maps
            .session_states
            .get_mut("s1")
            .unwrap()
            .last_push_ms = Some(first.saturating_sub(31_000));
        apply(&state, &exit);
        tokio::time::timeout(std::time::Duration::from_secs(60), accepted.recv())
            .await
            .expect("completion stayed blocked after the window")
            .expect("local service closed");
        let completed_at = state
            .session_maps
            .session_states
            .get("s1")
            .unwrap()
            .last_push_ms;
        apply(&state, &exit);
        assert_eq!(
            state
                .session_maps
                .session_states
                .get("s1")
                .unwrap()
                .last_push_ms,
            completed_at,
            "a repeated completion must not reserve a second push"
        );
        assert!(
            tokio::time::timeout(std::time::Duration::from_secs(2), accepted.recv())
                .await
                .is_err(),
            "a repeated completion reached the push service"
        );

        state
            .session_maps
            .session_states
            .insert("s2".to_string(), SessionState::default());
        AppState::apply_event_to_session_state(
            &state,
            &AppEvent::PtyExit {
                session_id: "s2".to_string(),
            },
        );
        tokio::time::timeout(std::time::Duration::from_secs(60), accepted.recv())
            .await
            .expect("another session inherited the first session's limit")
            .expect("local service closed");
        AppState::apply_event_to_session_state(
            &state,
            &AppEvent::PtyExit {
                session_id: "s3".to_string(),
            },
        );
        tokio::time::timeout(std::time::Duration::from_secs(2), accepted.recv())
            .await
            .expect("completion without an existing state row lost its push")
            .expect("local service closed");
        server.abort();
    }

    #[test]
    fn a_focused_but_idle_desktop_can_alert_the_phone() {
        assert!(super::mobile_push_away(true, Some(120.0)));
        assert!(!super::mobile_push_away(true, Some(119.9)));
        assert!(!super::mobile_push_away(true, Some(0.0)));
        assert!(!super::mobile_push_away(true, Some(f64::NAN)));
        assert!(!super::mobile_push_away(true, None));
        assert!(super::mobile_push_away(false, None));
    }

    #[tokio::test]
    async fn ask_user_question_waits_for_its_title_before_spending_the_push_limit() {
        let state = fresh_state();
        state.push_store.upsert(crate::push::PushSubscription {
            endpoint: "https://fcm.googleapis.com/fcm/send/example".to_string(),
            keys: crate::push::PushSubscriptionKeys {
                p256dh: "unused".to_string(),
                auth: "unused".to_string(),
            },
            created_at: chrono::Utc::now(),
        });
        let (private, public) = crate::push::generate_vapid_keys().unwrap();
        {
            let mut config = state.config.write();
            config.services.push.enabled = true;
            config.services.push.vapid_private_key = private;
            config.services.push.vapid_public_key = public;
        }
        state
            .desktop_window_focused
            .store(false, std::sync::atomic::Ordering::Relaxed);

        let hook = apply(
            &state,
            &make_parsed(
                "question",
                serde_json::json!({ "prompt_text": "", "confident": true }),
            ),
        );
        assert!(hook.awaiting_input);
        assert!(
            hook.last_push_ms.is_none(),
            "an empty hook signal must leave room for the real question"
        );

        let titled = apply(
            &state,
            &make_parsed(
                "choice-prompt",
                serde_json::json!({
                    "title": "Should I deploy now?",
                    "options": [
                        { "key": "1", "label": "Yes", "highlighted": true, "destructive": false },
                        { "key": "2", "label": "No", "highlighted": false, "destructive": true }
                    ]
                }),
            ),
        );
        assert_eq!(
            titled.question_text.as_deref(),
            Some("Should I deploy now?")
        );
        assert!(
            titled.last_push_ms.is_some(),
            "the titled question must be eligible for one push"
        );
    }

    #[test]
    fn test_session_state_confident_question_sets_flag() {
        let state = fresh_state();
        let event = make_parsed(
            "question",
            serde_json::json!({
                "prompt_text": "Enter to select",
                "confident": true,
            }),
        );
        let s = apply(&state, &event);
        assert!(s.awaiting_input);
        assert!(
            s.question_confident,
            "Ink menu question should be confident"
        );
    }

    /// Read the state_change payloads TUIC auto-posted to a parent's inbox.
    fn parent_state_changes(state: &Arc<AppState>, parent: &str) -> Vec<serde_json::Value> {
        state
            .agent_inbox
            .get(parent)
            .map(|inbox| {
                inbox
                    .iter()
                    .filter(|m| m.from_name == "tuic")
                    .filter_map(|m| serde_json::from_str::<serde_json::Value>(&m.content).ok())
                    .collect()
            })
            .unwrap_or_default()
    }

    /// A spawned peer has nobody at its keyboard, so an interactive prompt parks it
    /// forever. Whoever spawned it must be told, or the whole branch of the
    /// orchestration silently stalls.
    #[test]
    fn confident_question_routes_awaiting_input_to_the_spawning_parent() {
        let state = fresh_state();
        state
            .session_maps
            .session_parent
            .insert("s1".to_string(), "parent-1".to_string());
        state.agent_inbox.entry("parent-1".to_string()).or_default();

        apply(
            &state,
            &make_parsed(
                "question",
                serde_json::json!({
                    "prompt_text": "Enter to select · ↑/↓ to navigate · Esc to cancel",
                    "confident": true,
                }),
            ),
        );

        let posted = parent_state_changes(&state, "parent-1");
        assert_eq!(posted.len(), 1, "parent must be told exactly once");
        assert_eq!(posted[0]["state"], "awaiting_input");
        assert_eq!(posted[0]["session_id"], "s1");
        assert_eq!(
            posted[0]["prompt"], "Enter to select · ↑/↓ to navigate · Esc to cancel",
            "the parent needs the prompt text to answer without reading the pane"
        );
    }

    /// Confidence controls how callers present/debounce a wait; it cannot hide a
    /// genuinely blocked managed child from its parent.
    #[test]
    fn low_confidence_question_notifies_parent_with_confidence_metadata() {
        let state = fresh_state();
        state
            .session_maps
            .session_parent
            .insert("s1".to_string(), "parent-1".to_string());
        state.agent_inbox.entry("parent-1".to_string()).or_default();

        apply(
            &state,
            &make_parsed(
                "question",
                serde_json::json!({ "prompt_text": "Did that work?" }),
            ),
        );

        let posted = parent_state_changes(&state, "parent-1");
        assert_eq!(posted.len(), 1);
        assert_eq!(posted[0]["state"], "awaiting_input");
        assert_eq!(posted[0]["confident"], false);
        assert_eq!(posted[0]["source"], "question");
    }

    /// Only the transition into awaiting_input notifies: an Ink menu that re-emits
    /// while already parked must not flood the parent's inbox.
    #[test]
    fn repeated_confident_question_notifies_the_parent_only_once() {
        let state = fresh_state();
        state
            .session_maps
            .session_parent
            .insert("s1".to_string(), "parent-1".to_string());
        state.agent_inbox.entry("parent-1".to_string()).or_default();

        let q = make_parsed(
            "question",
            serde_json::json!({ "prompt_text": "Enter to select", "confident": true }),
        );
        apply(&state, &q);
        apply(&state, &q);

        assert_eq!(parent_state_changes(&state, "parent-1").len(), 1);

        // Answered, then a NEW prompt appears: that is a fresh transition and must
        // notify again, otherwise the second question of a session is invisible.
        apply(
            &state,
            &make_parsed("user-input", serde_json::json!({ "content": "1" })),
        );
        apply(&state, &q);
        assert_eq!(parent_state_changes(&state, "parent-1").len(), 2);
    }

    /// A session nobody spawned (a tab Boss opened by hand) has no parent to notify.
    #[test]
    fn confident_question_without_a_parent_notifies_nobody() {
        let state = fresh_state();
        apply(
            &state,
            &make_parsed(
                "question",
                serde_json::json!({ "prompt_text": "Enter to select", "confident": true }),
            ),
        );
        assert!(state.agent_inbox.is_empty());
    }

    #[test]
    fn test_session_state_user_input_clears_confident() {
        let state = fresh_state();
        let q = make_parsed(
            "question",
            serde_json::json!({
                "prompt_text": "Enter to select",
                "confident": true,
            }),
        );
        apply(&state, &q);
        let event = make_parsed("user-input", serde_json::json!({ "content": "yes" }));
        let s = apply(&state, &event);
        assert!(
            !s.question_confident,
            "user-input should clear question_confident"
        );
    }

    #[test]
    fn test_user_input_unblock_requeues_when_test_pty_is_missing() {
        // A peer message queued while a confident question blocked injection must
        // drain when the user answers (user-input) and the agent stays idle —
        // there is no BUSY→IDLE transition on that path (story 091 unblock flush).
        use std::collections::VecDeque;
        use std::sync::atomic::AtomicU8;
        let state = fresh_state();
        state
            .session_maps
            .session_states
            .get_mut("s1")
            .unwrap()
            .agent_type = Some("claude".to_string());
        state
            .session_maps
            .shell_states
            .insert("s1".to_string(), AtomicU8::new(crate::pty::SHELL_IDLE));
        let q = make_parsed(
            "question",
            serde_json::json!({ "prompt_text": "Enter to select", "confident": true }),
        );
        apply(&state, &q);
        let mut pending = VecDeque::new();
        pending.push_back(PendingInjection::notice(crate::pty::PEER_MAIL_WAKE));
        state.pending_injections.insert("s1".to_string(), pending);

        let ui = make_parsed("user-input", serde_json::json!({ "content": "yes" }));
        apply(&state, &ui);
        assert_eq!(
            state.pending_injections.get("s1").map(|q| q.len()),
            Some(1),
            "missing PTY lookup must roll back and keep the unblocked message retryable"
        );
    }

    #[test]
    fn test_session_state_user_input_clears_awaiting() {
        let state = fresh_state();
        // First go to question state
        let q = make_parsed("question", serde_json::json!({ "prompt_text": "Ready?" }));
        apply(&state, &q);
        // User responds → no longer awaiting
        let event = make_parsed("user-input", serde_json::json!({ "content": "yes" }));
        let s = apply(&state, &event);
        assert!(!s.awaiting_input);
    }

    #[test]
    fn test_session_state_status_line_clears_awaiting_input() {
        let state = fresh_state();
        // Set question state
        let q = make_parsed(
            "question",
            serde_json::json!({ "prompt_text": "Install gopls?" }),
        );
        apply(&state, &q);
        // Status-line means agent is working → question answered
        let status = make_parsed(
            "status-line",
            serde_json::json!({ "task_name": "Reading files" }),
        );
        let s = apply(&state, &status);
        assert!(!s.awaiting_input, "status-line should clear awaiting_input");
        assert!(
            s.question_text.is_none(),
            "status-line should clear question_text"
        );
    }

    #[test]
    fn test_session_state_status_line_keeps_confident_question() {
        let state = fresh_state();
        // Confident question — e.g. grok's "⚠ Action Required" approval prompt.
        let q = make_parsed(
            "question",
            serde_json::json!({ "prompt_text": "Run echo x", "confident": true }),
        );
        apply(&state, &q);
        // grok keeps its spinner animating (emitting status-line) WHILE awaiting
        // approval. A confident question must survive the busy tick — otherwise
        // awaiting_input flickers and the approval notification is lost.
        let status = make_parsed("status-line", serde_json::json!({ "task_name": "Running" }));
        let s = apply(&state, &status);
        assert!(
            s.awaiting_input,
            "confident question must survive a status-line tick"
        );
        assert_eq!(s.question_text.as_deref(), Some("Run echo x"));
        // The user answering (user-input) clears it.
        let ui = make_parsed("user-input", serde_json::json!({ "content": "yes" }));
        let s2 = apply(&state, &ui);
        assert!(
            !s2.awaiting_input,
            "user-input clears the confident question"
        );
    }

    #[test]
    fn stale_turn_question_cannot_rearm_awaiting_after_input() {
        let state = fresh_state();
        state
            .session_maps
            .session_states
            .get_mut("s1")
            .unwrap()
            .turn_epoch = 2;
        let stale = make_parsed(
            "question",
            serde_json::json!({
                "prompt_text": "Confermi questa rimozione?",
                "confident": false,
                "_turn_epoch": 1,
            }),
        );
        let session = apply(&state, &stale);
        assert!(!session.awaiting_input);
        assert!(session.question_text.is_none());
    }

    /// The bug this arm exists for: a heuristic question answered with a bare
    /// Enter. No `user-input` (that needs a non-empty typed line), no
    /// `status-line` (the agent went busy through a screen-movement signal), no
    /// `choice-prompt` to resolve. Nothing cleared awaiting_input, so the tab
    /// stayed badged "question" for the rest of the session.
    #[test]
    fn question_cleared_retracts_a_heuristic_question() {
        let state = fresh_state();
        apply(
            &state,
            &make_parsed(
                "question",
                serde_json::json!({ "prompt_text": "Would you like to make the following edits?" }),
            ),
        );
        let s = apply(
            &state,
            &make_parsed("question-cleared", serde_json::json!({})),
        );
        assert!(
            !s.awaiting_input,
            "question-cleared must retract the heuristic awaiting state"
        );
        assert!(s.question_text.is_none(), "and drop the stale prompt text");
    }

    #[test]
    fn choice_cleared_retracts_choice_and_awaiting_state() {
        let state = fresh_state();
        apply(
            &state,
            &make_parsed(
                "choice-prompt",
                serde_json::json!({
                    "title": "Apply edits?",
                    "options": [
                        { "key": "1", "label": "Yes", "highlighted": true, "destructive": false },
                        { "key": "2", "label": "No", "highlighted": false, "destructive": true }
                    ]
                }),
            ),
        );
        let waiting = state.session_maps.session_states.get("s1").unwrap().clone();
        assert!(waiting.awaiting_input);
        assert!(waiting.choice_prompt.is_some());

        let cleared = apply(
            &state,
            &make_parsed("choice-cleared", serde_json::json!({})),
        );
        assert!(!cleared.awaiting_input);
        assert!(cleared.choice_prompt.is_none());
    }

    /// grok repaints while it waits, so "not on screen right now" is not proof
    /// that a confident prompt was answered. Retracting it here would drop a
    /// real approval request — the same flicker the status-line arm guards.
    #[test]
    fn question_cleared_leaves_a_confident_question_alone() {
        let state = fresh_state();
        apply(
            &state,
            &make_parsed(
                "question",
                serde_json::json!({ "prompt_text": "Run echo x", "confident": true }),
            ),
        );
        let s = apply(
            &state,
            &make_parsed("question-cleared", serde_json::json!({})),
        );
        assert!(
            s.awaiting_input,
            "a confident question must survive question-cleared"
        );
        assert_eq!(s.question_text.as_deref(), Some("Run echo x"));
    }

    #[test]
    fn test_session_state_status_line_keeps_choice_prompt() {
        let state = fresh_state();
        let choice = make_parsed(
            "choice-prompt",
            serde_json::json!({
                "title": "Which approach should I use?",
                "options": [{
                    "key": "1",
                    "label": "Proceed",
                    "highlighted": true,
                    "destructive": false
                }],
                "dismiss_key": "cancel",
                "amend_key": "amend"
            }),
        );
        apply(&state, &choice);

        let status = make_parsed("status-line", serde_json::json!({ "task_name": "Waiting" }));
        let session = apply(&state, &status);
        assert!(
            session.choice_prompt.is_some(),
            "an animated status row must not erase a visible choice prompt"
        );
        state
            .session_maps
            .session_states
            .get_mut("s1")
            .unwrap()
            .agent_type = Some("claude".into());
        let snapshot = state.session_state_with_shell("s1").unwrap();
        assert_eq!(snapshot.agent_state.as_deref(), Some("awaiting_input"));
    }

    #[test]
    fn test_non_hook_choice_prompt_clears_on_single_key_reply() {
        let state = fresh_state();
        let choice = make_parsed(
            "choice-prompt",
            serde_json::json!({
                "title": "Which approach should I use?",
                "options": [{
                    "key": "1",
                    "label": "Proceed",
                    "highlighted": true,
                    "destructive": false
                }],
                "dismiss_key": "cancel",
                "amend_key": "amend"
            }),
        );
        apply(&state, &choice);
        assert!(!resolve_choice_prompt_input(&state, "s1", "\x1b[B"));
        assert!(
            state
                .session_maps
                .session_states
                .get("s1")
                .unwrap()
                .choice_prompt
                .is_some()
        );

        assert!(resolve_choice_prompt_input(&state, "s1", "1"));
        let session = state.session_maps.session_states.get("s1").unwrap();
        assert!(session.choice_prompt.is_none());
        assert!(!session.awaiting_input);
    }

    #[test]
    fn test_codex_choice_prompt_uses_its_own_exit_keys() {
        let state = fresh_state();
        apply(
            &state,
            &make_parsed(
                "choice-prompt",
                serde_json::json!({
                    "title": "Boss, scegli rosso o blu?",
                    "options": [{"key": "2", "label": "Blu", "highlighted": false, "destructive": false}],
                    "dismiss_key": "ctrl+]",
                    "amend_key": "alt+down"
                }),
            ),
        );
        assert!(!resolve_choice_prompt_input(&state, "s1", "\x1b"));
        assert!(!resolve_choice_prompt_input(&state, "s1", "\t"));
        assert!(
            state
                .session_maps
                .session_states
                .get("s1")
                .unwrap()
                .choice_prompt
                .is_some()
        );
        assert!(resolve_choice_prompt_input(&state, "s1", "\x1d"));
        assert!(
            state
                .session_maps
                .session_states
                .get("s1")
                .unwrap()
                .choice_prompt
                .is_none()
        );
    }

    #[test]
    fn claude_navigation_choice_stays_open_until_enter() {
        let state = fresh_state();
        apply(
            &state,
            &make_parsed(
                "choice-prompt",
                serde_json::json!({
                    "title": "Which color do you prefer?",
                    "options": [
                        {"key": "1", "label": "Red", "highlighted": true, "destructive": false},
                        {"key": "2", "label": "Green", "highlighted": false, "destructive": false}
                    ],
                    "selection_mode": "navigate-enter",
                    "dismiss_key": "cancel"
                }),
            ),
        );
        assert!(!resolve_choice_prompt_input(&state, "s1", "\x1b[B"));
        assert!(!resolve_choice_prompt_input(&state, "s1", "2"));
        assert!(
            state
                .session_maps
                .session_states
                .get("s1")
                .unwrap()
                .choice_prompt
                .is_some()
        );
        assert!(resolve_choice_prompt_input(&state, "s1", "\r"));
        assert!(
            state
                .session_maps
                .session_states
                .get("s1")
                .unwrap()
                .choice_prompt
                .is_none()
        );
    }

    #[test]
    fn test_session_state_low_confidence_question_does_not_downgrade_confident() {
        let state = fresh_state();
        // Confident approval prompt active (grok's "Action Required" title).
        apply(
            &state,
            &make_parsed(
                "question",
                serde_json::json!({ "prompt_text": "Run echo x", "confident": true }),
            ),
        );
        // grok's on-screen status line is also parsed as a low-confidence question
        // with a different text — it must NOT downgrade the active confident one,
        // or the next status-line would clear awaiting (flicker).
        let s = apply(
            &state,
            &make_parsed(
                "question",
                serde_json::json!({ "prompt_text": "Run echo x 12s", "confident": false }),
            ),
        );
        assert!(
            s.question_confident,
            "confident flag must not be downgraded"
        );
        assert_eq!(
            s.question_text.as_deref(),
            Some("Run echo x"),
            "confident question text must be preserved"
        );
        assert!(s.awaiting_input);
    }

    #[test]
    fn test_session_state_with_shell_reads_shell_states_not_output_timing() {
        let state = fresh_state();
        // Set shell_states to BUSY (1) — this is the source of truth from PTY reader
        state
            .session_maps
            .shell_states
            .insert("s1".to_string(), std::sync::atomic::AtomicU8::new(1));
        // Intentionally leave last_output_ms at 0 (stale) — should NOT matter
        let ss = state.session_state_with_shell("s1").unwrap();
        assert_eq!(
            ss.shell_state.as_deref(),
            Some("busy"),
            "shell_state must come from shell_states, not last_output_ms"
        );

        // Set shell_states to IDLE (2)
        state
            .session_maps
            .shell_states
            .get("s1")
            .unwrap()
            .store(2, std::sync::atomic::Ordering::Relaxed);
        let ss = state.session_state_with_shell("s1").unwrap();
        assert_eq!(ss.shell_state.as_deref(), Some("idle"));

        // No shell_states entry → None
        state.session_maps.shell_states.remove("s1");
        let ss = state.session_state_with_shell("s1").unwrap();
        assert!(
            ss.shell_state.is_none(),
            "no shell_states entry should produce None, not derive from timing"
        );

        // The explicit null sentinel also means unobserved, not idle.
        state.session_maps.shell_states.insert(
            "s1".to_string(),
            std::sync::atomic::AtomicU8::new(crate::pty::SHELL_NULL),
        );
        state
            .session_maps
            .session_states
            .get_mut("s1")
            .unwrap()
            .agent_type = Some("codex".to_string());
        let ss = state.session_state_with_shell("s1").unwrap();
        assert!(ss.shell_state.is_none());
        assert_eq!(ss.agent_state.as_deref(), Some("starting"));
    }

    #[test]
    fn test_current_turn_completion_normalizes_stale_busy_shell() {
        let state = fresh_state();
        {
            let mut session = state.session_maps.session_states.get_mut("s1").unwrap();
            session.agent_type = Some("codex".into());
            session.suggested_actions = Some(vec!["Review diff".into()]);
        }
        state.session_maps.shell_states.insert(
            "s1".into(),
            std::sync::atomic::AtomicU8::new(crate::pty::SHELL_BUSY),
        );

        let snapshot = state.session_state_with_shell("s1").unwrap();
        assert_eq!(snapshot.shell_state.as_deref(), Some("idle"));
        assert_eq!(snapshot.agent_state.as_deref(), Some("completed"));
    }

    #[test]
    fn test_completion_does_not_override_confirmed_or_pending_background_work() {
        for pending_probe in [false, true] {
            let state = fresh_state();
            {
                let mut session = state.session_maps.session_states.get_mut("s1").unwrap();
                session.agent_type = Some("claude".into());
                session.suggested_actions = Some(vec!["Review result".into()]);
                session.background_work = !pending_probe;
                if pending_probe {
                    session.background_probe_turn_epoch = Some(session.turn_epoch);
                }
            }
            state.session_maps.shell_states.insert(
                "s1".into(),
                std::sync::atomic::AtomicU8::new(crate::pty::SHELL_BUSY),
            );

            let snapshot = state.session_state_with_shell("s1").unwrap();
            assert_eq!(snapshot.shell_state.as_deref(), Some("busy"));
            assert_eq!(snapshot.agent_state.as_deref(), Some("working"));
        }
    }

    #[test]
    fn test_session_state_repeated_status_line_same_task_no_change() {
        let state = fresh_state();
        let e1 = make_parsed(
            "status-line",
            serde_json::json!({ "task_name": "Twisting" }),
        );
        let s1 = apply(&state, &e1);
        assert_eq!(s1.current_task.as_deref(), Some("Twisting"));
        // Same task again — state should be identical (PartialEq)
        let e2 = make_parsed(
            "status-line",
            serde_json::json!({ "task_name": "Twisting" }),
        );
        let s2 = apply(&state, &e2);
        assert_eq!(s1, s2);
    }

    #[test]
    fn test_session_state_status_line_different_task_updates() {
        let state = fresh_state();
        let e1 = make_parsed(
            "status-line",
            serde_json::json!({ "task_name": "Twisting" }),
        );
        let s1 = apply(&state, &e1);
        let e2 = make_parsed(
            "status-line",
            serde_json::json!({ "task_name": "Reading files" }),
        );
        let s2 = apply(&state, &e2);
        assert_ne!(s1, s2);
        assert_eq!(s2.current_task.as_deref(), Some("Reading files"));
    }

    // Real PTY tests keep the private drain_pty harness here.

    /// Helper: extract plain text from log lines for easy assertion.
    fn log_texts(buf: &VtLogBuffer) -> Vec<String> {
        buf.lines().iter().map(|ll| ll.text()).collect()
    }

    /// Integration test: spawn a real PTY process, feed its output through
    /// VtLogBuffer, and verify that clean log lines are extracted.
    #[test]
    fn test_vt_log_real_pty_echo() {
        use portable_pty::{PtySize, native_pty_system};

        let pty_system = native_pty_system();
        let pair = match pty_system.openpty(PtySize {
            rows: 24,
            cols: 80,
            pixel_width: 0,
            pixel_height: 0,
        }) {
            Ok(p) => p,
            Err(e) if e.to_string().contains("Operation not permitted") => {
                eprintln!("Skipping test: PTY not available in sandbox");
                return;
            }
            Err(e) => panic!("open pty: {e}"),
        };

        // Replay 30 numbered lines through the PTY. CRLF because the file is
        // written as bytes rather than produced by the tty's own `\n`
        // translation, and a bare LF would leave the cursor off column 0.
        let dir = tempfile::tempdir().expect("temp dir");
        let stream = dir.path().join("lines.txt");
        let sent: Vec<String> = (1..=30).map(|i| format!("test-line-{i}")).collect();
        std::fs::write(&stream, format!("{}\r\n", sent.join("\r\n"))).expect("write stream");
        let cmd = crate::test_support::replay_file_command(&stream);
        let mut child = pair.slave.spawn_command(cmd).expect("spawn");
        // Unix only: closing the slave is what makes the master report EOF
        // there. A ConPTY has no such handshake, and dropping the slave while
        // the console host is still starting loses the child's output.
        #[cfg(unix)]
        drop(pair.slave);

        let reader = pair.master.try_clone_reader().expect("reader");
        let terminal = pair.master.take_writer().expect("writer");
        let mut buf = VtLogBuffer::new(24, 80, 1000);
        // Kept so a failure can name what the tty actually sent. A ConPTY is a
        // VT interpreter rather than a pass-through, so "no lines" there could
        // be a shell that never ran or a re-render that never scrolls, and the
        // two need different fixes.
        let mut seen = Vec::new();
        crate::test_support::drain_pty(reader, terminal, |chunk| {
            seen.extend_from_slice(chunk);
            buf.process(chunk);
        });
        let _ = child.kill();
        let _ = child.wait();

        let lines = log_texts(&buf);
        // We should have captured at least some of our "test-line-N" lines
        let matching: Vec<&String> = lines
            .iter()
            .filter(|l| l.starts_with("test-line-"))
            .collect();
        assert!(
            !matching.is_empty(),
            "expected some 'test-line-N' lines in log, got 0 out of {} total lines: {:?}\n\
             screen: {:?}\nthe tty sent {} bytes: {}",
            lines.len(),
            lines,
            buf.screen_rows(),
            seen.len(),
            String::from_utf8_lossy(&seen).escape_debug(),
        );
        // Verify the captured lines cover a reasonable range.
        let nums: Vec<u32> = matching
            .iter()
            .filter_map(|l| l.strip_prefix("test-line-").and_then(|n| n.parse().ok()))
            .collect();
        let max_num = nums.iter().copied().max().unwrap_or(0);
        assert!(
            max_num >= 5,
            "should capture lines up to at least 5, max was {max_num}, nums: {nums:?}",
        );
    }

    /// Integration test: verify that alternate-screen content (TUI) is NOT
    /// captured by VtLogBuffer when using a real PTY.
    #[test]
    fn test_vt_log_real_pty_alternate_screen_suppressed() {
        use portable_pty::{PtySize, native_pty_system};

        let pty_system = native_pty_system();
        let pair = match pty_system.openpty(PtySize {
            rows: 24,
            cols: 80,
            pixel_width: 0,
            pixel_height: 0,
        }) {
            Ok(p) => p,
            Err(e) if e.to_string().contains("Operation not permitted") => {
                eprintln!("Skipping test: PTY not available in sandbox");
                return;
            }
            Err(e) => panic!("open pty: {e}"),
        };

        // Normal lines, enter alternate screen, TUI garbage, exit alternate
        // screen, more normal lines — written as the bytes themselves rather
        // than as a shell script, since `cmd` has no `printf` to emit an ESC.
        let stream = concat!(
            "before-alt-1\r\nbefore-alt-2\r\n",
            "\x1b[?1049h", // enter alternate screen
            "TUI-GARBAGE-LINE\r\n",
            "\x1b[?1049l", // exit alternate screen
            "after-alt-1\r\nafter-alt-2\r\n",
        );
        let dir = tempfile::tempdir().expect("temp dir");
        let path = dir.path().join("alt.raw");
        std::fs::write(&path, stream).expect("write stream");
        let cmd = crate::test_support::replay_file_command(&path);
        let mut child = pair.slave.spawn_command(cmd).expect("spawn");
        // See `test_vt_log_real_pty_echo`: unix only.
        #[cfg(unix)]
        drop(pair.slave);

        let reader = pair.master.try_clone_reader().expect("reader");
        let terminal = pair.master.take_writer().expect("writer");
        let mut buf = VtLogBuffer::new(24, 80, 1000);
        crate::test_support::drain_pty(reader, terminal, |chunk| {
            buf.process(chunk);
        });
        let _ = child.kill();
        let _ = child.wait();

        let lines = log_texts(&buf);
        // TUI-GARBAGE-LINE should not appear in the log
        let has_garbage = lines.iter().any(|l| l.contains("TUI-GARBAGE"));
        assert!(
            !has_garbage,
            "alternate-screen content should be suppressed, but found TUI-GARBAGE in: {:?}",
            lines,
        );
        // An absence proves nothing on its own: this assertion passed on
        // Windows while the PTY had delivered zero bytes. What the test is
        // really about is that the alt screen is suppressed *and* the stream
        // around it survives. The survivors are on the screen, not in `lines`
        // — four rows of a 24-row grid never scroll into the log.
        let screen = buf.screen_rows();
        for expected in ["before-alt-1", "before-alt-2", "after-alt-1", "after-alt-2"] {
            assert!(
                screen.iter().any(|l| l.contains(expected)),
                "{expected} must survive the alternate screen, got {screen:?}",
            );
        }
    }

    /// End-to-end through a real PTY: replaying the recorded `gh run watch`
    /// stream must build alt-screen scrollback (the scrollbar's precondition)
    /// while the log/agent-hook pipeline stays suppressed. Those two must hold
    /// *together* — scrollback the user can scroll, no alt noise in the logs.
    #[test]
    fn test_vt_log_real_pty_gh_run_watch_builds_alt_scrollback() {
        use portable_pty::{PtySize, native_pty_system};

        let fixture = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("src/fixtures/alt_screen/gh-run-watch.raw");

        let pty_system = native_pty_system();
        let pair = match pty_system.openpty(PtySize {
            rows: 24,
            cols: 120,
            pixel_width: 0,
            pixel_height: 0,
        }) {
            Ok(p) => p,
            Err(e) if e.to_string().contains("Operation not permitted") => {
                eprintln!("Skipping test: PTY not available in sandbox");
                return;
            }
            Err(e) => panic!("open pty: {e}"),
        };

        let cmd = crate::test_support::replay_file_command(&fixture);
        let mut child = pair.slave.spawn_command(cmd).expect("spawn");
        // See `test_vt_log_real_pty_echo`: unix only.
        #[cfg(unix)]
        drop(pair.slave);

        let reader = pair.master.try_clone_reader().expect("reader");
        let terminal = pair.master.take_writer().expect("writer");
        let mut buf = VtLogBuffer::new(24, 120, 1000);
        // See `test_vt_log_real_pty_echo`: kept for the failure message.
        let mut seen = Vec::new();
        crate::test_support::drain_pty(reader, terminal, |chunk| {
            seen.extend_from_slice(chunk);
            buf.process(chunk);
        });
        let _ = child.kill();
        let _ = child.wait();

        assert!(
            buf.is_alternate_screen(),
            "stream leaves us in alt screen; the tty sent {} bytes: {}",
            seen.len(),
            String::from_utf8_lossy(&seen[..seen.len().min(4000)]).escape_debug(),
        );
        assert!(
            buf.grid_history_size() > 0,
            "alt-screen scrollback must exist — 0 is the bug (no scrollbar, no scrollback)"
        );

        // The harness contract: none of that alt output may reach the log buffer
        // that feeds agent hooks and log extraction.
        let lines = log_texts(&buf);
        assert!(
            !lines.iter().any(|l| l.contains("Refreshing run status")),
            "alt-screen content must stay out of the log buffer, got: {lines:?}",
        );
    }

    #[test]
    fn test_rate_limit_ignored_without_agent_activity() {
        let state = fresh_state();
        // No agent activity (no status-line received) → rate-limit should be ignored
        let event = make_parsed("rate-limit", serde_json::json!({ "retry_after_ms": 5000 }));
        let s = apply(&state, &event);
        assert!(
            !s.rate_limited,
            "rate-limit must be ignored on non-agent session"
        );
        assert!(s.retry_after_ms.is_none());
    }

    #[test]
    fn test_rate_limit_accepted_with_agent_activity() {
        let state = fresh_state();
        // Establish agent presence via status-line
        let status = make_parsed("status-line", serde_json::json!({ "task_name": "Working" }));
        apply(&state, &status);
        // Now rate-limit should be accepted
        let event = make_parsed("rate-limit", serde_json::json!({ "retry_after_ms": 5000 }));
        let s = apply(&state, &event);
        assert!(
            s.rate_limited,
            "rate-limit must be accepted on agent session"
        );
        assert_eq!(s.retry_after_ms, Some(5000));
    }

    #[test]
    fn test_rate_limit_expires_after_retry_after_ms() {
        let state = fresh_state();
        // Establish agent presence
        let status = make_parsed("status-line", serde_json::json!({ "task_name": "Working" }));
        apply(&state, &status);
        // Set rate limited with 5s retry
        let event = make_parsed("rate-limit", serde_json::json!({ "retry_after_ms": 5000 }));
        let s = apply(&state, &event);
        assert!(s.rate_limited);
        assert_eq!(s.retry_after_ms, Some(5000));
        assert!(s.rate_limit_set_ms > 0);

        // Manually backdate the timestamp to simulate expiry
        if let Some(mut entry) = state.session_maps.session_states.get_mut("s1") {
            entry.rate_limit_set_ms = entry.rate_limit_set_ms.saturating_sub(6000);
        }

        // session_state_with_shell should auto-expire the stale rate limit
        let ss = state.session_state_with_shell("s1").unwrap();
        assert!(
            !ss.rate_limited,
            "rate limit should have expired after retry_after_ms"
        );
        assert!(ss.retry_after_ms.is_none());
    }

    #[test]
    fn test_rate_limit_not_expired_within_retry_window() {
        let state = fresh_state();
        // Establish agent presence
        let status = make_parsed("status-line", serde_json::json!({ "task_name": "Working" }));
        apply(&state, &status);
        let event = make_parsed("rate-limit", serde_json::json!({ "retry_after_ms": 60000 }));
        let s = apply(&state, &event);
        assert!(s.rate_limited);

        // Still within the window — should remain rate limited
        let ss = state.session_state_with_shell("s1").unwrap();
        assert!(
            ss.rate_limited,
            "rate limit should still be active within retry window"
        );
    }

    #[test]
    fn test_rate_limit_expires_with_default_timeout_when_no_retry_after() {
        let state = fresh_state();
        // Establish agent presence
        let status = make_parsed("status-line", serde_json::json!({ "task_name": "Working" }));
        apply(&state, &status);
        // Rate limit with no retry_after_ms
        let event = make_parsed("rate-limit", serde_json::json!({}));
        let s = apply(&state, &event);
        assert!(s.rate_limited);
        assert!(s.retry_after_ms.is_none());

        // Backdate past the default expiry (120s)
        if let Some(mut entry) = state.session_maps.session_states.get_mut("s1") {
            entry.rate_limit_set_ms = entry.rate_limit_set_ms.saturating_sub(121_000);
        }

        let ss = state.session_state_with_shell("s1").unwrap();
        assert!(
            !ss.rate_limited,
            "rate limit should expire with default timeout"
        );
    }
}

#[cfg(test)]
mod http_client_tests {
    use super::*;

    /// A `reqwest::Client` with no timeout waits forever on a peer that accepts
    /// the connection and then goes silent — the half-open socket a VPN drop
    /// leaves behind. The GitHub poller awaits that request inline in its
    /// `select!`, so the hang takes the poller's Stop handling down with it.
    #[tokio::test]
    async fn request_timeout_fires_when_the_peer_accepts_and_never_answers() {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
            .await
            .expect("bind stalling listener");
        let addr = listener.local_addr().expect("local addr");
        // Accept and hold the socket open without ever writing a response.
        tokio::spawn(async move {
            let mut held = Vec::new();
            while let Ok((sock, _)) = listener.accept().await {
                held.push(sock);
            }
        });

        let client = http_client_with_timeouts(Duration::from_secs(2), Duration::from_millis(150));
        let err = client
            .get(format!("http://{addr}/"))
            .send()
            .await
            .expect_err("a stalled peer must not resolve");

        assert!(err.is_timeout(), "expected a timeout error, got: {err}");
    }

    /// The shared client must bound *both* phases: a route that black-holes SYN
    /// packets never reaches the request phase, so a request-only timeout still
    /// leaves the connect hanging for the OS default (~75s on macOS).
    #[test]
    fn shared_client_timeouts_are_bounded_and_connect_is_the_tighter_one() {
        assert!(
            HTTP_CONNECT_TIMEOUT < HTTP_REQUEST_TIMEOUT,
            "connect ({HTTP_CONNECT_TIMEOUT:?}) must be tighter than the whole \
             request ({HTTP_REQUEST_TIMEOUT:?})"
        );
        assert!(
            HTTP_REQUEST_TIMEOUT <= Duration::from_secs(30),
            "a GitHub API call slower than 30s is a wedge, not a slow response"
        );
    }
}
