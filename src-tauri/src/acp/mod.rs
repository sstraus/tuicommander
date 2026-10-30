use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use agent_client_protocol::JsonRpcResponse;
use agent_client_protocol::schema::{ProtocolVersion, v1};
use serde::{Deserialize, Serialize};
use serde_json::Value;

const EGO_PAUSE_METHOD: &str = "_ego/pause";
const EGO_RESUME_METHOD: &str = "_ego/resume";
const EGO_COMPACT_METHOD: &str = "_ego/compact";
const EGO_STEER_METHOD: &str = "_ego/steer";

/// The extension metadata ego attaches to its advertised capabilities.
///
/// It hangs off `agentCapabilities`, not off the initialize response, because
/// it *is* a capability. Pause and resume arrive together under `hold` with one
/// version covering both method names — they are two halves of one extension,
/// and separate versions could disagree in a way no agent can actually be in.
const EGO_EXTENSIONS: &str = "ego";
const EGO_HOLD_EXTENSION: &str = "hold";
const EGO_COMPACT_EXTENSION: &str = "compact";

mod connection;
mod ego_ext;
mod events;
mod manager;
mod mcp_host;
pub(crate) mod oneshot;

pub use events::{AcpEventJournal, AcpEventStream};
pub use manager::{AcpClientManager, INITIALIZE_TIMEOUT};
pub use mcp_host::{
    McpNotify, McpOverAcpError, McpOverAcpHost, McpReply, TUICOMMANDER_MCP_SERVER_ID,
};

/// The three ways a connection attaches to a session ego already owns.
///
/// They are one operation with three names because the request is the same —
/// a durable id plus the authority the session is to run under — and only the
/// method and what comes back differ. Fork is the one that answers with an id
/// the caller did not name, because a fork is a second session.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum AcpAttachKind {
    /// Attach and replay the history, so the host can render what happened.
    Load,
    /// Attach a copy that shares the original's history and diverges from it.
    Fork,
    /// Attach without replay, for a host that already has the history.
    Resume,
}

/// The two ways a connection lets a session go.
///
/// Both end the attachment; only one ends the session. Which of the two ego
/// was asked for is not a detail the client may blur, so they never collapse
/// into a single "forget it".
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum AcpDetachKind {
    /// Stop serving the session here. It stays in `session/list`.
    Close,
    /// Remove the session from ego for good.
    Delete,
}

impl AcpAttachKind {
    /// The capability that has to be advertised before this is sent.
    #[must_use]
    pub fn operation(self) -> AcpOperation {
        match self {
            Self::Load => AcpOperation::Load,
            Self::Fork => AcpOperation::Fork,
            Self::Resume => AcpOperation::Resume,
        }
    }

    /// The wire method this is sent as, for the log that attributes an attach.
    #[must_use]
    pub fn method(self) -> &'static str {
        match self {
            Self::Load => "session/load",
            Self::Fork => "session/fork",
            Self::Resume => "session/resume",
        }
    }
}

impl AcpDetachKind {
    /// The capability that has to be advertised before this is sent.
    #[must_use]
    pub fn operation(self) -> AcpOperation {
        match self {
            Self::Close => AcpOperation::Close,
            Self::Delete => AcpOperation::Delete,
        }
    }
}

/// What a session is allowed to reach, resupplied by the caller every time.
///
/// It is never restored from a stored snapshot: an authority that outlived the
/// window in which it was granted is a wider authority than anyone gave, and
/// reconnect is exactly when that would happen unnoticed.
///
/// **`mcp_servers` is not part of the request.** `connect` and `reconnect` take
/// the spawn guard because they launch a process; the session routes underneath
/// them do not, and they are reachable from a browser. A body that could name an
/// MCP server would therefore let whoever sends one point the agent at an
/// arbitrary HTTP endpoint, with arbitrary headers, and ego would egress to it —
/// a hole no guard on `connect` can cover, because by then the connection is
/// already legitimate. So the field is built server-side from configuration and
/// `deny_unknown_fields` refuses a body that tries to supply one, rather than
/// stripping it quietly: a caller that meant to widen the authority learns that
/// it did not.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct AcpSessionAuthority {
    pub cwd: PathBuf,
    pub additional_directories: Vec<PathBuf>,
    #[serde(skip_deserializing)]
    pub mcp_servers: Vec<v1::McpServer>,
}

/// The name TUICommander's own MCP server carries in every ego session.
///
/// It is a public contract, not an implementation detail: it appears in users'
/// ego rule files, so renaming it breaks policy somebody else wrote.
pub const TUICOMMANDER_MCP_SERVER_NAME: &str = "tuicommander";

/// The one MCP server an attended ego session is given: `tuicommander`, served
/// by this process on the ACP connection itself (MCP-over-ACP).
///
/// Synthesised here rather than accepted from a caller — see
/// `AcpSessionAuthority` for why a request body must not be able to name one.
///
/// It used to be a stdio `tuic-bridge` command line, and before that an HTTP
/// URL built from the TCP port. The bridge opened a fresh HTTP MCP session for
/// every ego tool operation (13,409 initializes from one peer on 2026-09-28),
/// and it had to be told which socket to reach and which identity to assert.
/// Over ACP there is no process, no socket and no identity to pass along: the
/// connection is the identity (Boss, 2026-09-28).
///
/// Ego still names itself in the `_meta` `clientInfo` of every MCP request, so
/// `client_requires_meta_tools` gives it the collapsed tool surface, worth
/// 35.104 tokens a turn against 615 at 190 tools. Ego speaks the stateless
/// 2026-07-28 lifecycle, so `initialize` is not on its path.
#[must_use]
pub fn tuicommander_acp_mcp_server() -> v1::McpServer {
    v1::McpServer::Acp(v1::McpServerAcp::new(
        TUICOMMANDER_MCP_SERVER_NAME,
        TUICOMMANDER_MCP_SERVER_ID,
    ))
}

/// The bridge header accepts only the canonical UUID form used for TUIC peers.
pub(crate) fn valid_peer_id(id: &str) -> bool {
    id.len() == 36 && uuid::Uuid::parse_str(id).is_ok()
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EgoAcpConfig {
    pub executable: PathBuf,
    pub profile: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LaunchSpec {
    pub program: PathBuf,
    pub args: Vec<String>,
}

pub fn launch_spec(config: &EgoAcpConfig, root: &Path) -> Result<LaunchSpec, AcpClientError> {
    if !config.executable.is_absolute() {
        return Err(AcpClientError::invalid_input(
            "ego executable must be an absolute path",
        ));
    }
    if !root.is_absolute() {
        return Err(AcpClientError::invalid_input(
            "ACP root must be an absolute path",
        ));
    }
    let root = root
        .to_str()
        .ok_or_else(|| AcpClientError::invalid_input("ACP root must be valid UTF-8"))?;

    let profile = &config.profile;
    if !profile.is_empty()
        && (profile.len() > 64
            || profile.starts_with('-')
            || profile
                .chars()
                .any(|character| character.is_whitespace() || character.is_ascii_control()))
    {
        return Err(AcpClientError::invalid_input("invalid ego profile name"));
    }

    let mut args = vec!["acp".to_string(), "-C".to_string(), root.to_string()];
    if !profile.is_empty() {
        args.extend(["--profile".to_string(), profile.clone()]);
    }

    Ok(LaunchSpec {
        program: config.executable.clone(),
        args,
    })
}

pub fn build_initialize_request() -> v1::InitializeRequest {
    let client_capabilities = v1::ClientCapabilities::new()
        .fs(v1::FileSystemCapabilities::new())
        .terminal(false)
        // Form elicitation is advertised because this client seats an
        // `elicitation/create` and lets a person answer it. Ego treats the
        // absence of this as "no one to ask" and settles its questions
        // `Unavailable` without ever sending one, so leaving it off would make
        // the seat unreachable rather than merely unused.
        .elicitation(
            v1::ElicitationCapabilities::new().form(v1::ElicitationFormCapabilities::new()),
        );

    v1::InitializeRequest::new(ProtocolVersion::V1).client_capabilities(client_capabilities)
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum AcpOperation {
    Load,
    List,
    Resume,
    Fork,
    Delete,
    Close,
    AdditionalDirectories,
    PromptImage,
    PromptAudio,
    PromptEmbeddedContext,
    McpStdio,
    McpHttp,
    McpSse,
    McpAcp,
    ClientFormElicitation,
    ClientBooleanConfig,
    Pause,
    ResumeTurn,
    Compact,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AcpUnavailableReason {
    NotAdvertised,
    WrongExtensionVersion,
    NotOfferedBySession,
    ExcludedByContract,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AcpAvailability {
    pub operation: AcpOperation,
    pub available: bool,
    pub reason: Option<AcpUnavailableReason>,
}

/// Host-issued origin of an MCP channel, not a peer's mutable latest connection.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AcpMcpOrigin {
    pub peer_id: Option<String>,
    pub connection_id: AcpConnectionId,
    pub generation: u64,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Hash)]
#[serde(transparent)]
pub struct AcpConnectionId(uuid::Uuid);

impl AcpConnectionId {
    #[must_use]
    pub fn new() -> Self {
        Self(uuid::Uuid::now_v7())
    }
}

impl Default for AcpConnectionId {
    fn default() -> Self {
        Self::new()
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Hash)]
#[serde(transparent)]
pub struct AcpTurnId(uuid::Uuid);

impl AcpTurnId {
    #[must_use]
    pub fn new() -> Self {
        Self(uuid::Uuid::now_v7())
    }
}

impl Default for AcpTurnId {
    fn default() -> Self {
        Self::new()
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Hash)]
#[serde(transparent)]
pub struct AcpHostRequestId(uuid::Uuid);

impl AcpHostRequestId {
    #[must_use]
    pub fn new() -> Self {
        Self(uuid::Uuid::now_v7())
    }
}

impl Default for AcpHostRequestId {
    fn default() -> Self {
        Self::new()
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Hash)]
#[serde(transparent)]
pub struct AcpOperationId(uuid::Uuid);

impl AcpOperationId {
    #[must_use]
    pub fn new() -> Self {
        Self(uuid::Uuid::now_v7())
    }
}

impl Default for AcpOperationId {
    fn default() -> Self {
        Self::new()
    }
}

#[derive(Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct AcpConnectRequest {
    pub root: PathBuf,
    /// Launch-only overrides. Never retained in connection snapshots or diagnostics.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub env: Option<BTreeMap<String, String>>,
}

impl std::fmt::Debug for AcpConnectRequest {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("AcpConnectRequest")
            .field("root", &self.root)
            .field("env", &self.env.as_ref().map(|_| "[redacted]"))
            .finish()
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct AcpReconnectRequest {
    pub connection_id: AcpConnectionId,
    pub root: PathBuf,
}

/// What a registered connection is, and only that.
///
/// There is no `starting` or `initializing` here, because launching the child
/// and negotiating v1 are phases of the `connect` *operation* rather than
/// states of a connection anyone can hold: the id is minted inside `connect`
/// and does not escape it until initialization has succeeded, so a connection
/// that is still coming up cannot be named, listed, subscribed to, or asked
/// about. An attempt that fails is answered with an `AcpClientError` and
/// registers nothing. Giving those phases a state would mean returning an id
/// before there is a connection behind it — a different, asynchronous connect
/// API, with its own cancellation, retention and ownership rules, and nothing
/// asks for one.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
#[non_exhaustive]
pub enum AcpConnectionState {
    Ready,
    Closing,
    Closed,
    Failed,
    Killed,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
#[non_exhaustive]
pub enum AcpAttachmentState {
    Attaching,
    Idle,
    Prompting,
    Cancelling,
    PausePending,
    Paused,
    Closing,
    Closed,
    Detached,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
#[non_exhaustive]
pub enum AcpTurnState {
    Running,
    Cancelling,
    Settled,
}

/// What this attachment knows about token usage, from two independent signals.
///
/// Both are optional because the agent owes neither. `context` arrives on a
/// `session/update` and `end_turn` on the prompt response, and an agent may
/// send one, the other, or both. Requiring `context` meant the snapshot could
/// only exist once a `UsageUpdate` had arrived, so an agent that reports usage
/// only on the response had every figure dropped from the durable aggregate.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct AcpUsageSnapshot {
    pub context: Option<v1::UsageUpdate>,
    pub end_turn: Option<v1::Usage>,
}

/// One frame on a subscriber's stream, whichever transport carries it.
///
/// A gap is a frame rather than a closed stream, and it is the last one: the
/// events it stands for cannot be reconstructed from anything this client
/// holds, so a subscriber that kept reading would render a turn with a hole in
/// the middle and no way to know. The recovery is a fresh subscription and an
/// ego `session/load`, which replays from the only place that actually knows.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase", tag = "kind")]
#[non_exhaustive]
pub enum AcpStreamFrame {
    /// One journal entry, byte for byte as every other subscriber saw it.
    Event(Box<AcpEventEnvelope>),
    /// The subscriber fell behind and the missing events are gone.
    Gap(AcpClientError),
    /// The connection can produce nothing further.
    End,
}

/// One thing that happened on a connection, in the order it happened.
///
/// Every event carries the generation it belongs to. A late event from a
/// connection that has since been replaced is not a fresher view of the same
/// thing; it is news about a process that is already gone, and the generation
/// is what lets a reader say so rather than apply it to the wrong connection.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct AcpEventEnvelope {
    pub connection_id: AcpConnectionId,
    pub generation: u64,
    pub sequence: u64,
    pub session_id: Option<v1::SessionId>,
    pub turn_id: Option<AcpTurnId>,
    pub event: AcpClientEvent,
}

/// What happened, in the one shape a host reads it in.
///
/// `rename_all` renames the *variants*; `rename_all_fields` renames the fields
/// inside them, and both are needed. Without the second, `permissionSettled`
/// arrives carrying `request_id` while the envelope around it carries
/// `connectionId` — one frame, two casings, and the half a client silently
/// fails to read is the half that dismisses a question already answered.
///
/// The two state variants carry their payload under a named `state` field
/// rather than as a bare newtype for the same reason. Serde's internal tagging
/// has no key to put a plain string under, so it invents one from the value:
/// `ConnectionState(Failed)` went on the wire as `{"kind":"connectionState",
/// "failed":null}`, where the state is the *key* and every state is a
/// differently-shaped object. Nothing errored — it is a valid JSON object, and
/// it is unreadable.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(
    rename_all = "camelCase",
    rename_all_fields = "camelCase",
    tag = "kind"
)]
#[non_exhaustive]
pub enum AcpClientEvent {
    ConnectionState {
        state: AcpConnectionState,
    },
    AttachmentState {
        state: AcpAttachmentState,
    },
    TurnStarted,
    /// The accepted user input was sent to the agent; all views render it once.
    PromptSent {
        text: String,
    },
    /// The complete queue after a change, so every subscriber sees the same order.
    PromptQueueChanged {
        queued_prompts: Vec<AcpQueuedPrompt>,
    },
    /// Ego's own update, forwarded whole rather than reduced.
    ///
    /// The client has no business deciding which parts of what the agent said
    /// a host is allowed to render.
    ///
    /// Boxed because it dwarfs every other variant, and a journal retains a
    /// thousand of these per connection: unboxed, a bare `TurnStarted` would
    /// cost as much to keep as the update it followed.
    /// Ego's own update, carried under a field of its own rather than flattened.
    ///
    /// A newtype variant here would spread the update's fields beside this
    /// enum's `kind` tag, and `ToolCall` has a field called `kind` too: the
    /// object would then hold that key twice, serde_json would write both, and
    /// every reader keeps the last — so a `read` tool call would arrive
    /// announcing itself as an event kind no host knows. Nothing errors on
    /// either side. Pinned by
    /// `a_tool_call_kind_does_not_overwrite_the_event_kind`.
    SessionUpdate {
        update: Box<v1::SessionUpdate>,
    },
    TurnSettled {
        stop_reason: v1::StopReason,
        usage: Option<v1::Usage>,
    },
    /// The prompt answered with an ACP error; preserve its diagnostic for every viewer.
    TurnFailed {
        message: String,
        state: AcpAttachmentState,
    },
    /// The agent is waiting on a person, and this is what it asked.
    ///
    /// It travels on the stream rather than being handed to whoever called
    /// last, because the caller that started the turn is not necessarily the
    /// one at the keyboard, and may not be listening at all by now.
    PermissionRequested {
        request_id: AcpHostRequestId,
        request: Box<v1::RequestPermissionRequest>,
    },
    PermissionSettled {
        request_id: AcpHostRequestId,
        outcome: v1::RequestPermissionOutcome,
    },
    ElicitationRequested {
        request_id: AcpHostRequestId,
        request: Box<v1::CreateElicitationRequest>,
    },
    ElicitationSettled {
        request_id: AcpHostRequestId,
        action: v1::ElicitationAction,
    },
}

/// Why a host might want to come and look, and where.
///
/// Deliberately not the event itself. Every chunk of a turn is an event; only
/// these notices are worth waking a client that is not currently reading the
/// stream, and only they are cheap enough to put on a broadcast that every
/// other subscriber in the app shares.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
#[non_exhaustive]
pub enum AcpNoticeKind {
    /// The connection finished initializing and can be used.
    Ready,
    /// Something that was in flight has finished.
    ///
    /// A turn settling and a connection settling are both this: the host was
    /// waiting on one of them, and the two are told apart by whether the
    /// notice names a session, not by needing a fifth kind.
    Settled,
    /// The agent is waiting on a person.
    InteractionPending,
    /// A pending question was answered — possibly by somebody else's client.
    InteractionSettled,
    /// An ego card has arrived on the ordered session stream.
    Card,
}

/// A low-frequency wake signal about one connection.
///
/// It says where to look and nothing more: the ordered payload stays on the
/// per-connection stream, and the current picture stays in the snapshot and
/// the interactions list. A host that reacts to a notice by fetching one of
/// those two reads the same truth as a host that never missed a frame.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct AcpNotice {
    pub connection_id: AcpConnectionId,
    pub generation: u64,
    pub session_id: Option<v1::SessionId>,
    pub request_id: Option<AcpHostRequestId>,
    /// The sequence of the event this notice was derived from, so a host can
    /// resume its stream from exactly here rather than from the beginning.
    pub sequence: u64,
    pub kind: AcpNoticeKind,
}

impl AcpNotice {
    /// The notice one stamped event deserves, if it deserves one at all.
    ///
    /// Derived at the journal rather than at each producer: every event on a
    /// connection is appended there exactly once, so a notice cannot be
    /// forgotten by a new call site, and it cannot be sent twice.
    #[must_use]
    pub fn from_envelope(envelope: &AcpEventEnvelope) -> Option<Self> {
        let (kind, request_id) = match &envelope.event {
            AcpClientEvent::ConnectionState {
                state: AcpConnectionState::Ready,
            } => (AcpNoticeKind::Ready, None),
            AcpClientEvent::ConnectionState {
                state:
                    AcpConnectionState::Closed | AcpConnectionState::Failed | AcpConnectionState::Killed,
            }
            | AcpClientEvent::TurnSettled { .. }
            | AcpClientEvent::TurnFailed { .. } => (AcpNoticeKind::Settled, None),
            AcpClientEvent::PermissionRequested { request_id, .. }
            | AcpClientEvent::ElicitationRequested { request_id, .. } => {
                (AcpNoticeKind::InteractionPending, Some(*request_id))
            }
            AcpClientEvent::PermissionSettled { request_id, .. }
            | AcpClientEvent::ElicitationSettled { request_id, .. } => {
                (AcpNoticeKind::InteractionSettled, Some(*request_id))
            }
            AcpClientEvent::SessionUpdate { update }
                if matches!(update.as_ref(), v1::SessionUpdate::AgentMessageChunk(chunk)
                    if chunk.meta.as_ref()
                        .and_then(|meta| meta.get("ego"))
                        .and_then(|ego| ego.get("salience"))
                        .and_then(serde_json::Value::as_str) == Some("card")) =>
            {
                (AcpNoticeKind::Card, None)
            }
            _ => return None,
        };
        Some(Self {
            connection_id: envelope.connection_id,
            generation: envelope.generation,
            session_id: envelope.session_id.clone(),
            request_id,
            sequence: envelope.sequence,
            kind,
        })
    }
}

/// One request the agent is waiting on an answer to.
///
/// Carried in snapshots as well as on the stream, so a frontend that was not
/// running when the agent asked still finds the question when it comes back.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(
    rename_all = "camelCase",
    rename_all_fields = "camelCase",
    tag = "kind"
)]
#[non_exhaustive]
pub enum AcpPendingInteraction {
    Permission {
        request_id: AcpHostRequestId,
        session_id: v1::SessionId,
        request: Box<v1::RequestPermissionRequest>,
    },
    Elicitation {
        request_id: AcpHostRequestId,
        session_id: v1::SessionId,
        request: Box<v1::CreateElicitationRequest>,
    },
}

impl AcpPendingInteraction {
    #[must_use]
    pub fn request_id(&self) -> AcpHostRequestId {
        match self {
            Self::Permission { request_id, .. } | Self::Elicitation { request_id, .. } => {
                *request_id
            }
        }
    }

    #[must_use]
    pub fn session_id(&self) -> &v1::SessionId {
        match self {
            Self::Permission { session_id, .. } | Self::Elicitation { session_id, .. } => {
                session_id
            }
        }
    }
}

/// The receipt for an interaction this client answered.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct AcpInteractionSettlement {
    pub request_id: AcpHostRequestId,
}

/// What a `_ego/pause` or `_ego/resume` found or established.
///
/// The two are not distinguished, because ego does not distinguish them and
/// the shared vocabulary is the better contract: a session told `paused` may
/// assume nothing further runs under it, and a session told `pending` may not.
/// That is the distinction the hold boundary exists to make, and it is the
/// same distinction whichever of the two methods asked.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
#[non_exhaustive]
pub enum AcpHoldState {
    /// Nothing is holding this session.
    Running,
    /// A hold is recorded and the turn has not reached a boundary yet.
    Pending,
    /// The hold is durable: nothing more runs until it is released.
    Paused,
}

/// One `_ego/pause` or `_ego/resume`, named by the caller.
///
/// The request id is the caller's, generated once and reused only to retrieve
/// the same idempotent result. Ego rejects a nil UUID, so this client rejects
/// one before the write rather than letting a round trip say it.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct EgoHoldRequest {
    pub session_id: v1::SessionId,
    pub request_id: uuid::Uuid,
}

/// Ego's answer to a hold request, in ego's own field names.
#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Eq, JsonRpcResponse)]
#[serde(rename_all = "camelCase")]
pub struct EgoHoldResponse {
    pub v: u32,
    pub session_id: v1::SessionId,
    pub request_id: uuid::Uuid,
    pub state: AcpHoldState,
}

/// One `_ego/compact`, named by the caller.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct EgoCompactRequest {
    pub session_id: v1::SessionId,
    pub request_id: uuid::Uuid,
}

/// Whether the successor a compaction produced is durably on record.
///
/// The uncertain case is the reason this is three values and not a boolean. A
/// target that may or may not have been published cannot be retried as a new
/// compaction — that would risk a second successor for one source — and it
/// cannot be treated as absent either.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(tag = "kind", rename_all = "snake_case")]
#[non_exhaustive]
pub enum AcpTargetPublication {
    NotPublished { diagnostic: String },
    PublishedDurably { diagnostic: Option<String> },
    PublishedDurabilityUncertain { diagnostic: String },
}

impl AcpTargetPublication {
    /// Whether asking for this compaction again is safe.
    ///
    /// Only a target that was definitely not published may be retried. Both
    /// other answers mean a successor may exist, and asking again could make
    /// a second one.
    #[must_use]
    pub fn retry_safe(&self) -> bool {
        matches!(self, Self::NotPublished { .. })
    }
}

/// Ego's answer to a compaction, in ego's own field names.
#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Eq, JsonRpcResponse)]
#[serde(rename_all = "camelCase")]
pub struct EgoCompactResponse {
    pub v: u32,
    pub source_session_id: v1::SessionId,
    pub source_seq: u64,
    pub target_session_id: v1::SessionId,
    pub publication: AcpTargetPublication,
    pub successor_start_request_id: uuid::Uuid,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct AcpTurnSnapshot {
    pub turn_id: AcpTurnId,
    pub state: AcpTurnState,
    pub stop_reason: Option<v1::StopReason>,
    pub usage: Option<v1::Usage>,
}

/// A prompt accepted by the host but not yet sent to the agent.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct AcpQueuedPrompt {
    pub turn_id: AcpTurnId,
    pub summary: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct AcpAttachmentSnapshot {
    pub session_id: v1::SessionId,
    pub state: AcpAttachmentState,
    pub cwd: PathBuf,
    pub additional_directories: Vec<PathBuf>,
    pub config_options: Vec<v1::SessionConfigOption>,
    #[serde(default)]
    pub available_commands: Vec<v1::AvailableCommand>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub profile_warnings: Vec<String>,
    pub usage: Option<AcpUsageSnapshot>,
    pub active_turn: Option<AcpTurnSnapshot>,
    pub queued_prompts: Vec<AcpQueuedPrompt>,
    pub pending_permission_ids: Vec<AcpHostRequestId>,
    pub pending_elicitation_ids: Vec<AcpHostRequestId>,
}

impl AcpAttachmentSnapshot {
    /// The actor retains the most recently settled turn in active_turn. It is
    /// history, not in-flight work; queued prompts and open seats are work.
    pub(crate) fn is_quiescent(&self) -> bool {
        self.state == AcpAttachmentState::Idle
            && self
                .active_turn
                .as_ref()
                .is_none_or(|turn| turn.state == AcpTurnState::Settled)
            && self.queued_prompts.is_empty()
            && self.pending_permission_ids.is_empty()
            && self.pending_elicitation_ids.is_empty()
    }
}

#[derive(Debug, Clone, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct AcpConnectionSnapshot {
    pub connection_id: AcpConnectionId,
    pub generation: u64,
    pub state: AcpConnectionState,
    pub agent_info: Option<v1::Implementation>,
    pub capabilities: Option<AcpCapabilitySnapshot>,
    pub attachments: Vec<AcpAttachmentSnapshot>,
    pub earliest_sequence: u64,
    pub latest_sequence: u64,
    pub settlement: Option<AcpConnectionSettlement>,
}

/// Why a connection ended, as this client can actually prove it.
///
/// `TransportError` is the umbrella for every non-EOF way the SDK's transport
/// can end, write-side failures included. Naming the writer separately would
/// claim an attribution nobody here can make: a child closing its pipes and a
/// child exiting produce a write failure, a stdout EOF and an SDK shutdown at
/// once, and which of them the supervisor notices first is a scheduling
/// accident. A reason a host cannot rely on is worse than a coarser one it can.
///
/// There is likewise no `initialization_failed`: a connection that never
/// initialized was never registered, so there is nothing to settle. That
/// failure is an `AcpClientErrorCode::InitializationFailed` from `connect`.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
#[non_exhaustive]
pub enum AcpConnectionSettlementReason {
    Disconnected,
    Eof,
    TransportError,
    ProtocolViolation,
    Killed,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct AcpConnectionSettlement {
    pub connection_id: AcpConnectionId,
    pub generation: u64,
    pub reason: AcpConnectionSettlementReason,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
#[non_exhaustive]
pub enum AcpClientErrorCode {
    InvalidInput,
    InitializationFailed,
    NotFound,
    UnsupportedProtocol,
    /// The peer never advertised this operation, so nothing was sent.
    CapabilityUnavailable,
    /// The agent answered, and its answer was a refusal.
    AgentError,
    /// The agent denied a method it had advertised.
    ///
    /// Not a refusal but a contradiction of the snapshot every later decision
    /// on this connection is read from, so it settles the connection rather
    /// than being handed back as one operation's bad luck.
    ProtocolViolation,
    /// The connection has settled. A new one is the only way forward.
    TransportClosed,
    /// The events a subscriber asked for are no longer held.
    ///
    /// Said out loud rather than papered over: the missing chunks cannot be
    /// reconstructed from anything this client holds, and a stream that
    /// silently resumed past a hole would render as a turn that skipped part
    /// of what the model said. The recovery is a fresh connection and
    /// `session/load`, which replays from the one place that actually knows.
    StreamGap,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct AcpClientError {
    pub code: AcpClientErrorCode,
    pub message: String,
    pub connection_id: Option<AcpConnectionId>,
    pub session_id: Option<v1::SessionId>,
    pub operation: Option<AcpOperation>,
    pub retryable: bool,
    #[serde(skip)]
    agent_code: Option<i32>,
}

impl AcpClientError {
    pub(super) fn invalid_input(message: impl Into<String>) -> Self {
        Self::new(AcpClientErrorCode::InvalidInput, message)
    }

    pub(super) fn initialization_failed(
        connection_id: AcpConnectionId,
        message: impl Into<String>,
    ) -> Self {
        Self::new(AcpClientErrorCode::InitializationFailed, message)
            .with_connection_id(connection_id)
    }

    pub(super) fn not_found(connection_id: AcpConnectionId) -> Self {
        Self::new(
            AcpClientErrorCode::NotFound,
            format!("ACP connection {connection_id} was not found"),
        )
        .with_connection_id(connection_id)
    }

    /// Refused before the wire, naming the operation and why it is not there.
    ///
    /// Never retryable: the answer comes from an immutable snapshot taken at
    /// `initialize`, so the identical request on this connection will refuse
    /// identically. A host that wants it must reconnect to an agent that has
    /// it, which is a different action, not a retry.
    pub(super) fn capability_unavailable(
        connection_id: AcpConnectionId,
        operation: AcpOperation,
        reason: Option<AcpUnavailableReason>,
    ) -> Self {
        Self::new(
            AcpClientErrorCode::CapabilityUnavailable,
            match reason {
                Some(AcpUnavailableReason::WrongExtensionVersion) => format!(
                    "the agent advertises {operation:?} at a version this client does not speak"
                ),
                Some(AcpUnavailableReason::NotOfferedBySession) => {
                    format!("this session does not offer {operation:?}")
                }
                Some(AcpUnavailableReason::ExcludedByContract) => {
                    format!("{operation:?} is not part of this client's contract")
                }
                Some(AcpUnavailableReason::NotAdvertised) | None => {
                    format!("the agent did not advertise {operation:?}")
                }
            },
        )
        .with_connection_id(connection_id)
        .with_operation(operation)
    }

    /// The agent answered and its answer was a refusal.
    ///
    /// Not retryable on its own account: the agent decided, and asking again
    /// with the same bytes invites the same decision. Whether a *different*
    /// request would work is the caller's judgement, not this one's.
    pub(super) fn agent_error(
        connection_id: AcpConnectionId,
        operation: Option<AcpOperation>,
        message: impl Into<String>,
    ) -> Self {
        let mut error =
            Self::new(AcpClientErrorCode::AgentError, message).with_connection_id(connection_id);
        error.operation = operation;
        error
    }

    /// The agent denied a method it advertised.
    ///
    /// Never retryable, for the same reason `capability_unavailable` is not:
    /// the request was sent because a snapshot said it could be, and that
    /// snapshot does not change. Repeating it invites the same contradiction
    /// from an agent this client can no longer believe about anything else
    /// either — which is why the connection settles behind this error rather
    /// than staying up for the next caller to rediscover.
    pub(super) fn protocol_violation(
        connection_id: AcpConnectionId,
        operation: Option<AcpOperation>,
        message: impl Into<String>,
    ) -> Self {
        let mut error = Self::new(AcpClientErrorCode::ProtocolViolation, message)
            .with_connection_id(connection_id);
        error.operation = operation;
        error
    }

    /// The connection is gone. Retryable, because a fresh one may not be.
    pub(super) fn transport_closed(connection_id: AcpConnectionId) -> Self {
        let mut error = Self::new(
            AcpClientErrorCode::TransportClosed,
            format!("ACP connection {connection_id} has settled"),
        )
        .with_connection_id(connection_id);
        error.retryable = true;
        error
    }

    /// The caller named a session this connection is not attached to.
    ///
    /// Distinct from an unknown connection: the connection is fine, and it is
    /// the session that was never attached here or has since been let go.
    pub(super) fn not_attached(connection_id: AcpConnectionId, session_id: v1::SessionId) -> Self {
        Self::new(
            AcpClientErrorCode::NotFound,
            format!("ACP connection {connection_id} is not attached to session {session_id}"),
        )
        .with_connection_id(connection_id)
        .with_session_id(session_id)
    }

    /// A load or resume named a session this connection already holds.
    ///
    /// Refused rather than merged. The attachment is where the running turn,
    /// the usage totals and the open interactions live; attaching writes a
    /// fresh one, so a second attach would blank the turn and leave the
    /// response that settles it arriving for a turn nothing names any more.
    ///
    /// Not retryable, and not a race: it stops being true only when the caller
    /// itself detaches, which is a different request rather than the same one
    /// again.
    pub(super) fn already_attached(
        connection_id: AcpConnectionId,
        session_id: v1::SessionId,
    ) -> Self {
        Self::new(
            AcpClientErrorCode::InvalidInput,
            format!("ACP connection {connection_id} is already attached to session {session_id}"),
        )
        .with_connection_id(connection_id)
        .with_session_id(session_id)
    }

    /// A config option, or a value for one, that this session never offered.
    ///
    /// Refused here rather than forwarded because the offer is the whole
    /// contract: ego resolves an incoming value by matching it against the list
    /// it published, so a value it did not publish has no meaning to send.
    pub(super) fn not_offered(
        connection_id: AcpConnectionId,
        session_id: v1::SessionId,
        detail: impl std::fmt::Display,
    ) -> Self {
        Self::new(
            AcpClientErrorCode::CapabilityUnavailable,
            format!("ACP session {session_id} does not offer {detail}"),
        )
        .with_connection_id(connection_id)
        .with_session_id(session_id)
    }

    /// A cancel arrived for a session that has nothing running.
    pub(super) fn no_active_turn(
        connection_id: AcpConnectionId,
        session_id: v1::SessionId,
    ) -> Self {
        Self::new(
            AcpClientErrorCode::InvalidInput,
            format!("ACP session {session_id} has no turn to cancel"),
        )
        .with_connection_id(connection_id)
        .with_session_id(session_id)
    }

    /// The events asked for fell out of the journal before they were read.
    ///
    /// Not retryable: asking again for the same cursor gets the same answer,
    /// and the events are not coming back. The caller has to either accept the
    /// gap and resume from `earliest`, or reload the session from ego.
    pub(super) fn stream_gap(
        connection_id: AcpConnectionId,
        requested: u64,
        earliest: u64,
    ) -> Self {
        Self::new(
            AcpClientErrorCode::StreamGap,
            format!(
                "ACP connection {connection_id} no longer holds event {requested}; \
                 the earliest it still holds is {earliest}"
            ),
        )
        .with_connection_id(connection_id)
    }

    /// An answer arrived for a request nothing is waiting on.
    ///
    /// One code for two histories on purpose: the request was answered already,
    /// or it was never one this connection held. Both mean the same thing to
    /// the caller — the seat is not open — and the client does not keep settled
    /// seats around just so it could tell the two apart.
    pub(super) fn interaction_settled(
        connection_id: AcpConnectionId,
        request_id: AcpHostRequestId,
    ) -> Self {
        Self::new(
            AcpClientErrorCode::NotFound,
            format!("ACP connection {connection_id} is not waiting on request {request_id}"),
        )
        .with_connection_id(connection_id)
    }

    /// The answer named an option the agent never offered.
    ///
    /// Refused here rather than forwarded: the agent decides what a permission
    /// means, and it can only do that for the options it named. An id it does
    /// not know is not a decision it can read.
    pub(super) fn unoffered_option(
        connection_id: AcpConnectionId,
        session_id: v1::SessionId,
        option: &v1::PermissionOptionId,
    ) -> Self {
        Self::new(
            AcpClientErrorCode::InvalidInput,
            format!("the agent did not offer permission option {}", option.0),
        )
        .with_connection_id(connection_id)
        .with_session_id(session_id)
    }

    /// The answer used an elicitation action the protocol does not define.
    ///
    /// The same rule as [`unoffered_option`], on the other seat. ACP defines
    /// accept, decline and cancel; anything else deserialises into the
    /// catch-all it keeps for extensions and for versions this client has never
    /// seen, and forwarding one would mean answering the agent in a vocabulary
    /// this client cannot know it reads.
    ///
    /// Refused before the seat is taken, so the question survives the bad
    /// answer and a person can still give a real one.
    ///
    /// [`unoffered_option`]: Self::unoffered_option
    pub(super) fn undefined_action(
        connection_id: AcpConnectionId,
        session_id: v1::SessionId,
        action: &str,
    ) -> Self {
        Self::new(
            AcpClientErrorCode::InvalidInput,
            format!("{action} is not an elicitation action this protocol defines"),
        )
        .with_connection_id(connection_id)
        .with_session_id(session_id)
    }

    /// A live subscriber fell far enough behind that it missed events.
    pub(super) fn stream_lagged(connection_id: AcpConnectionId) -> Self {
        Self::new(
            AcpClientErrorCode::StreamGap,
            format!("ACP connection {connection_id} produced events faster than this subscriber read them"),
        )
        .with_connection_id(connection_id)
    }

    pub(super) fn unsupported_protocol(message: impl Into<String>) -> Self {
        Self::new(AcpClientErrorCode::UnsupportedProtocol, message)
    }

    fn new(code: AcpClientErrorCode, message: impl Into<String>) -> Self {
        Self {
            code,
            message: message.into(),
            connection_id: None,
            session_id: None,
            operation: None,
            retryable: false,
            agent_code: None,
        }
    }

    pub(super) fn with_connection_id(mut self, connection_id: AcpConnectionId) -> Self {
        self.connection_id = Some(connection_id);
        self
    }

    fn with_operation(mut self, operation: AcpOperation) -> Self {
        self.operation = Some(operation);
        self
    }

    fn with_session_id(mut self, session_id: v1::SessionId) -> Self {
        self.session_id = Some(session_id);
        self
    }
}

impl std::fmt::Display for AcpConnectionId {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.0.fmt(formatter)
    }
}

impl std::fmt::Display for AcpHostRequestId {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.0.fmt(formatter)
    }
}

#[derive(Debug, Clone, Copy, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
enum ExtensionAvailability {
    Absent,
    Available,
    Mismatched,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct AcpCapabilitySnapshot {
    pub protocol: ProtocolVersion,
    pub load: bool,
    pub list: bool,
    pub resume: bool,
    pub fork: bool,
    pub fork_at_message: bool,
    pub delete: bool,
    pub close: bool,
    pub additional_directories: bool,
    pub prompt_image: bool,
    pub prompt_audio: bool,
    pub prompt_embedded_context: bool,
    pub mcp_stdio: bool,
    pub mcp_http: bool,
    pub mcp_sse: bool,
    pub mcp_acp: bool,
    pub client_form_elicitation: bool,
    pub client_boolean_config: bool,
    /// One version for `_ego/pause` and `_ego/resume`, which arrive together.
    pub ego_hold_version: Option<u32>,
    pub ego_compact_version: Option<u32>,
    pub ego_steer_version: Option<u32>,
    hold_availability: ExtensionAvailability,
    compact_availability: ExtensionAvailability,
}

impl AcpCapabilitySnapshot {
    #[must_use]
    pub fn availability(&self, operation: AcpOperation) -> AcpAvailability {
        let reason = match operation {
            AcpOperation::Load => advertised(self.load),
            AcpOperation::List => advertised(self.list),
            AcpOperation::Resume => advertised(self.resume),
            AcpOperation::Fork => advertised(self.fork),
            AcpOperation::Delete => advertised(self.delete),
            AcpOperation::Close => advertised(self.close),
            AcpOperation::AdditionalDirectories => advertised(self.additional_directories),
            AcpOperation::PromptImage => advertised(self.prompt_image),
            AcpOperation::PromptAudio => advertised(self.prompt_audio),
            AcpOperation::PromptEmbeddedContext => advertised(self.prompt_embedded_context),
            AcpOperation::McpStdio => included(self.mcp_stdio),
            AcpOperation::McpHttp => advertised(self.mcp_http),
            AcpOperation::McpSse => advertised(self.mcp_sse),
            AcpOperation::McpAcp => advertised(self.mcp_acp),
            AcpOperation::ClientFormElicitation => included(self.client_form_elicitation),
            AcpOperation::ClientBooleanConfig => included(self.client_boolean_config),
            AcpOperation::Pause | AcpOperation::ResumeTurn => {
                extension_reason(self.hold_availability)
            }
            AcpOperation::Compact => extension_reason(self.compact_availability),
        };

        AcpAvailability {
            operation,
            available: reason.is_none(),
            reason,
        }
    }
}

pub fn capability_snapshot(
    response: &v1::InitializeResponse,
) -> Result<AcpCapabilitySnapshot, AcpClientError> {
    if response.protocol_version != ProtocolVersion::V1 {
        return Err(AcpClientError::unsupported_protocol(format!(
            "unsupported ACP protocol version {}",
            response.protocol_version
        )));
    }

    let agent = &response.agent_capabilities;
    let session = &agent.session_capabilities;
    let prompt = &agent.prompt_capabilities;
    let mcp = &agent.mcp_capabilities;
    let hold = hold_availability(agent);
    let compact = compact_availability(agent);

    Ok(AcpCapabilitySnapshot {
        protocol: response.protocol_version,
        load: agent.load_session,
        list: session.list.is_some(),
        resume: session.resume.is_some(),
        fork: session.fork.is_some(),
        fork_at_message: session
            .fork
            .as_ref()
            .and_then(|fork| fork.meta.as_ref())
            .and_then(|meta| meta.get("ego"))
            .and_then(|ego| ego.get("atMessage"))
            .and_then(Value::as_bool)
            == Some(true),
        delete: session.delete.is_some(),
        close: session.close.is_some(),
        additional_directories: session.additional_directories.is_some(),
        prompt_image: prompt.image,
        prompt_audio: prompt.audio,
        prompt_embedded_context: prompt.embedded_context,
        // Stdio is a fact about this client: the bridge it used to carry is
        // gone (Boss, 2026-09-28), so it is excluded by contract. The ACP
        // transport is the agent's to advertise: an ego that predates
        // `mcpCapabilities.acp` still answers `session/new` with a session id
        // and silently drops the server, which would open a chat with no
        // tuicommander tools and no error (#1270-6d4c).
        mcp_stdio: false,
        mcp_acp: mcp.acp,
        mcp_http: mcp.http,
        mcp_sse: mcp.sse,
        // What this client can do, not what the agent said. It is here rather
        // than beside the request because a host asking "can a question be
        // answered on this connection?" reads one snapshot, and an answer that
        // needs both sides is still one answer.
        client_form_elicitation: true,
        client_boolean_config: false,
        ego_hold_version: exact_extension_version(hold),
        ego_compact_version: exact_extension_version(compact),
        ego_steer_version: ego_extensions(agent)
            .and_then(|ego| ego.get("steer"))
            .filter(|steer| {
                steer.get("version").and_then(Value::as_u64) == Some(1)
                    && steer.get("method").and_then(Value::as_str) == Some(EGO_STEER_METHOD)
                    && steer
                        .get("contentTypes")
                        .and_then(Value::as_array)
                        .is_some_and(|types| types.iter().any(|kind| kind.as_str() == Some("text")))
            })
            .map(|_| 1),
        hold_availability: hold,
        compact_availability: compact,
    })
}

/// The one `ego` object under the agent's advertised capabilities.
fn ego_extensions(agent: &v1::AgentCapabilities) -> Option<&serde_json::Map<String, Value>> {
    agent.meta.as_ref()?.get(EGO_EXTENSIONS)?.as_object()
}

/// Whether `_ego/pause` and `_ego/resume` may be sent on this connection.
///
/// Both method names are checked, not just the one the caller happens to want:
/// a `hold` that names a pause this client knows and a resume it does not is
/// not half-available, it is an agent speaking a version of the extension this
/// client was not written against.
fn hold_availability(agent: &v1::AgentCapabilities) -> ExtensionAvailability {
    let Some(hold) = ego_extensions(agent).and_then(|ego| ego.get(EGO_HOLD_EXTENSION)) else {
        return ExtensionAvailability::Absent;
    };
    let matches = hold.get("version").and_then(Value::as_u64) == Some(1)
        && hold.get("pause").and_then(Value::as_str) == Some(EGO_PAUSE_METHOD)
        && hold.get("resume").and_then(Value::as_str) == Some(EGO_RESUME_METHOD);
    if matches {
        ExtensionAvailability::Available
    } else {
        ExtensionAvailability::Mismatched
    }
}

fn compact_availability(agent: &v1::AgentCapabilities) -> ExtensionAvailability {
    let Some(compact) = ego_extensions(agent).and_then(|ego| ego.get(EGO_COMPACT_EXTENSION)) else {
        return ExtensionAvailability::Absent;
    };
    let matches = compact.get("version").and_then(Value::as_u64) == Some(1)
        && compact.get("method").and_then(Value::as_str) == Some(EGO_COMPACT_METHOD);
    if matches {
        ExtensionAvailability::Available
    } else {
        ExtensionAvailability::Mismatched
    }
}

fn exact_extension_version(availability: ExtensionAvailability) -> Option<u32> {
    (availability == ExtensionAvailability::Available).then_some(1)
}

fn extension_reason(availability: ExtensionAvailability) -> Option<AcpUnavailableReason> {
    match availability {
        ExtensionAvailability::Absent => Some(AcpUnavailableReason::NotAdvertised),
        ExtensionAvailability::Available => None,
        ExtensionAvailability::Mismatched => Some(AcpUnavailableReason::WrongExtensionVersion),
    }
}

fn advertised(available: bool) -> Option<AcpUnavailableReason> {
    (!available).then_some(AcpUnavailableReason::NotAdvertised)
}

fn included(available: bool) -> Option<AcpUnavailableReason> {
    (!available).then_some(AcpUnavailableReason::ExcludedByContract)
}

/// Order durable conversations by ancestry for the host picker. Ego metadata is retained.
pub(super) fn session_tree(mut sessions: Vec<v1::SessionInfo>) -> Vec<v1::SessionInfo> {
    fn lineage(session: &v1::SessionInfo) -> Option<&Value> {
        session.meta.as_ref()?.get("ego")?.get("lineage")
    }
    fn parent(session: &v1::SessionInfo) -> Option<&str> {
        lineage(session)?.get("sourceSessionId")?.as_str()
    }
    sessions.sort_by(|a, b| b.updated_at.cmp(&a.updated_at));
    let mut placeholders = Vec::new();
    for session in &sessions {
        let Some(info) = lineage(session) else {
            continue;
        };
        let Some(source) = parent(session) else {
            continue;
        };
        if info.get("sourceDeleted").and_then(Value::as_bool) != Some(true)
            || sessions
                .iter()
                .chain(placeholders.iter())
                .any(|row: &v1::SessionInfo| row.session_id.to_string() == source)
        {
            continue;
        }
        let mut placeholder = v1::SessionInfo::new(source.to_owned(), session.cwd.clone())
            .title("Deleted conversation");
        let mut meta = serde_json::Map::new();
        meta.insert(
            "tuicommander".to_owned(),
            serde_json::json!({"deleted": true}),
        );
        // A missing immediate ancestor still belongs under the reported root.
        if let Some(root) = info
            .get("rootSessionId")
            .and_then(Value::as_str)
            .filter(|root| *root != source)
        {
            meta.insert(
                "ego".to_owned(),
                serde_json::json!({"lineage": {"sourceSessionId": root}}),
            );
        }
        placeholder.meta = Some(meta);
        placeholders.push(placeholder);
    }
    sessions.extend(placeholders);
    fn visit(
        index: usize,
        depth: usize,
        rows: &[v1::SessionInfo],
        seen: &mut std::collections::HashSet<usize>,
        ordered: &mut Vec<v1::SessionInfo>,
    ) {
        if !seen.insert(index) {
            return;
        }
        let mut row = rows[index].clone();
        let display = row
            .meta
            .get_or_insert_default()
            .entry("tuicommander".to_owned())
            .or_insert_with(|| serde_json::json!({}));
        display["lineageDepth"] = serde_json::json!(depth);
        let id = row.session_id.to_string();
        ordered.push(row);
        for (child, _) in rows
            .iter()
            .enumerate()
            .filter(|(_, row)| parent(row) == Some(id.as_str()))
        {
            visit(child, depth + 1, rows, seen, ordered);
        }
    }
    let mut ordered = Vec::new();
    let mut seen = std::collections::HashSet::new();
    for (index, row) in sessions.iter().enumerate() {
        if parent(row).is_none_or(|source| {
            !sessions
                .iter()
                .any(|candidate| candidate.session_id.to_string() == source)
        }) {
            visit(index, 0, &sessions, &mut seen, &mut ordered);
        }
    }
    // Preserve every row even if a non-ego agent publishes incomplete ancestry.
    for index in 0..sessions.len() {
        visit(index, 0, &sessions, &mut seen, &mut ordered);
    }
    ordered
}

#[cfg(test)]
mod session_tree_tests {
    use super::*;

    // Domain input for TUIC's projection; ego's wire contract is exercised in ego 246-a81d.
    fn row(id: &str, source: Option<&str>, deleted: bool) -> v1::SessionInfo {
        let mut row = v1::SessionInfo::new(id.to_owned(), "/workspace");
        if let Some(source) = source {
            row.meta = Some(serde_json::Map::from_iter([(
                "ego".to_owned(),
                serde_json::json!({
                    "lineage": {"kind": "fork", "sourceSessionId": source, "rootSessionId": "root", "sourceDeleted": deleted}
                }),
            )]));
        }
        row
    }

    // Catches: recency ordering flattening nested fork descendants.
    #[test]
    fn nested_forks_render_two_levels_below_the_root() {
        let rows = session_tree(vec![
            row("grandchild", Some("child"), false),
            row("root", None, false),
            row("child", Some("root"), false),
        ]);
        assert_eq!(
            rows.iter()
                .map(|row| row.session_id.to_string())
                .collect::<Vec<_>>(),
            ["root", "child", "grandchild"]
        );
        assert_eq!(
            rows.iter()
                .map(|row| row
                    .meta
                    .as_ref()
                    .and_then(|meta| meta.get("tuicommander"))
                    .and_then(|meta| meta.get("lineageDepth"))
                    .and_then(Value::as_u64))
                .collect::<Vec<_>>(),
            [Some(0), Some(1), Some(2)]
        );
        assert_eq!(
            rows[2].meta.as_ref().unwrap()["ego"]["lineage"]["sourceSessionId"],
            "child"
        );
    }

    // Catches: an orphan disappearing when its immediate parent is deleted.
    #[test]
    fn deleted_parent_remains_a_disabled_ancestor_of_its_child() {
        let rows = session_tree(vec![
            row("child", Some("deleted"), true),
            row("root", None, false),
        ]);
        assert_eq!(
            rows.iter()
                .map(|row| row.session_id.to_string())
                .collect::<Vec<_>>(),
            ["root", "deleted", "child"]
        );
        assert_eq!(rows[1].title.as_deref(), Some("Deleted conversation"));
        assert_eq!(
            rows[1].meta.as_ref().unwrap()["tuicommander"]["deleted"],
            true
        );
        assert_eq!(
            rows[2].meta.as_ref().unwrap()["tuicommander"]["lineageDepth"],
            2
        );
    }
}
