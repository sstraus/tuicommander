use std::{
    collections::HashMap,
    path::{Path, PathBuf},
    sync::{
        Arc, Weak,
        atomic::{AtomicBool, AtomicU64, Ordering},
    },
};

use agent_client_protocol::schema::v1;
use agent_client_protocol::{AcpAgent, AcpAgentConfig, Agent, Client, ConnectionTo, Responder};
use parking_lot::Mutex;
use tokio::{
    sync::{Mutex as AsyncMutex, broadcast, mpsc, oneshot},
    task::JoinHandle,
};

use futures_util::StreamExt;

use super::connection::{
    Accepted, Answer, Command, ConnectionActor, InFlight, Inbound, Interaction, Publish,
};
use super::events::{AcpEventJournal, AcpEventStream};
use super::mcp_host::McpChannel;
use super::{
    AcpAttachKind, AcpAttachmentSnapshot, AcpAttachmentState, AcpCapabilitySnapshot,
    AcpClientError, AcpClientEvent, AcpConnectRequest, AcpConnectionId, AcpConnectionSettlement,
    AcpConnectionSettlementReason, AcpConnectionSnapshot, AcpConnectionState, AcpDetachKind,
    AcpHostRequestId, AcpInteractionSettlement, AcpNotice, AcpPendingInteraction,
    AcpReconnectRequest, AcpSessionAuthority, AcpTurnId, EgoAcpConfig, EgoCompactRequest,
    EgoCompactResponse, EgoHoldRequest, EgoHoldResponse, McpOverAcpHost, build_initialize_request,
    capability_snapshot, launch_spec, tuicommander_acp_mcp_server,
};

const INITIAL_GENERATION: u64 = 1;

/// Where a prompt names the repository on screen (agreed with ego, #1157-1e54).
pub const VIEWED_REPO_META_KEY: &str = "tuicommander/viewedRepo";

/// How many wake signals the shared notice bus holds for a slow subscriber.
///
/// Small because a notice carries no payload worth catching up on: a
/// subscriber that fell behind can read the connection snapshot and the
/// pending interactions and know everything the notices would have told it.
const NOTICE_CAPACITY: usize = 64;

/// How many settled connections stay readable.
///
/// A connection is kept after it ends because ending is exactly when a host
/// wants to read it: the settlement reason, the attachments it had, the tail of
/// its stream. Kept forever it would be a leak instead — each one holds a
/// journal of up to a thousand events, and reconnecting is an ordinary thing to
/// do over and over. Enough to cover any reconnect a person would watch happen,
/// and small enough that a retry loop cannot grow this map without bound.
const SETTLED_RETAINED: usize = 8;

/// How many commands may wait for the actor before a caller is made to wait.
///
/// Bounded rather than unbounded on purpose: an unbounded queue turns a stalled
/// agent into unbounded memory and hides the stall until something else breaks.
/// The number is small because a queue this deep already means the connection
/// is not keeping up, and the honest thing then is backpressure on the caller.
const COMMAND_QUEUE: usize = 32;

/// How many incoming updates may wait for the actor.
///
/// Deep because the producer is the SDK dispatch loop, which cannot be made to
/// wait without stalling the protocol, and a replayed session arrives as a
/// burst of chunks. Overflowing it is a fatal connection error rather than a
/// dropped update: a host that renders a turn with a hole in the middle of it
/// is confidently wrong about what the agent said.
const UPDATE_QUEUE: usize = 4096;

/// How long a freshly launched agent has to answer `initialize`.
///
/// Without it `connect` has no bound at all, and the shape that costs is not a
/// crash: a process that starts and then says nothing keeps its pipe open, so
/// there is no EOF to notice and the supervisor sits in `send_request` holding
/// the child for the life of the app. Every caller above it inherits that —
/// `/acp/one-shot` most of all, whose whole turn has to finish inside the
/// router's `REQUEST_TIMEOUT`.
///
/// Sixty seconds is far above a real launch (ego answers in well under a
/// second on a warm machine, a few seconds cold) and far below "never". The
/// budget is enforced where it can act: inside the supervisor, around the
/// `initialize` request itself, so expiry takes the ordinary initialization
/// failure path that already stops the child — rather than at the caller,
/// where dropping the future would leave the supervisor exactly as stuck.
pub const INITIALIZE_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(60);

pub struct AcpClientManager {
    connections: Arc<Mutex<HashMap<AcpConnectionId, ConnectionHandle>>>,
    peer_connect_locks: Mutex<HashMap<String, Weak<AsyncMutex<()>>>>,
    next_generation: AtomicU64,
    notices: broadcast::Sender<AcpNotice>,
    /// What serves `tuicommander` to ego over MCP-over-ACP, once the
    /// application has one to offer.
    mcp_host: Mutex<Option<Arc<dyn McpOverAcpHost>>>,
}

impl Default for AcpClientManager {
    fn default() -> Self {
        Self::new()
    }
}

struct ConnectionHandle {
    snapshot: AcpConnectionSnapshot,
    peer_id: Option<String>,
    root: PathBuf,
    attachment_directories: HashMap<String, (PathBuf, String)>,
    commands: mpsc::Sender<Command>,
    journal: Arc<AcpEventJournal>,
    shutdown: Option<oneshot::Sender<()>>,
    supervisor: Option<JoinHandle<()>>,
}

struct InitializedConnection {
    agent_info: Option<v1::Implementation>,
    capabilities: AcpCapabilitySnapshot,
}

/// What one pass of the supervisor's select decided to do.
enum Step {
    Command(Command),
    Accept(Accepted),
}

#[derive(Debug, Clone, Copy)]
enum SupervisorExit {
    NotReady,
    Disconnected,
    Eof,
    /// The agent denied a method it had advertised.
    ProtocolViolation,
}

impl AcpClientManager {
    #[must_use]
    pub fn new() -> Self {
        let (notices, _) = broadcast::channel(NOTICE_CAPACITY);
        Self {
            connections: Arc::new(Mutex::new(HashMap::new())),
            peer_connect_locks: Mutex::new(HashMap::new()),
            next_generation: AtomicU64::new(INITIAL_GENERATION),
            notices,
            mcp_host: Mutex::new(None),
        }
    }

    /// Serve the `tuicommander` MCP server on every connection opened from now
    /// on through `host`.
    pub fn set_mcp_host(&self, host: Arc<dyn McpOverAcpHost>) {
        *self.mcp_host.lock() = Some(host);
    }

    pub fn has_acp_inbox_subscriber(&self, peer_id: &str) -> bool {
        self.mcp_host
            .lock()
            .as_ref()
            .is_some_and(|host| host.has_inbox_subscriber(peer_id))
    }

    /// Replace whatever a caller put in `mcp_servers` with what this process
    /// actually serves.
    ///
    /// Replace rather than extend, and here rather than at each route: the
    /// session routes are reachable from a browser and take no spawn guard, so
    /// a list a body could contribute to would let whoever sends one point ego
    /// at any endpoint it liked. `deny_unknown_fields` refuses such a body one
    /// layer up; this is the layer that makes a Rust caller unable to do it
    /// either.
    fn granted(&self, mut authority: AcpSessionAuthority) -> AcpSessionAuthority {
        authority.mcp_servers = vec![tuicommander_acp_mcp_server()];
        authority
    }

    /// Wake signals for every connection this manager holds.
    ///
    /// One bus for all of them on purpose: a notice says where to look, and a
    /// subscriber that had to enumerate connections to hear about a new one
    /// would miss exactly the notice announcing it.
    #[must_use]
    pub fn notices(&self) -> broadcast::Receiver<AcpNotice> {
        self.notices.subscribe()
    }

    /// Launch ego and initialize a connection to it.
    ///
    /// The executable arrives per call rather than being remembered here, and
    /// that is the point: it is a setting a person can change while this
    /// manager is alive, and a copy taken once would keep launching the
    /// previous binary for the rest of the process without ever saying so.
    /// The caller reads it from configuration; no request body can supply it.
    pub async fn connect(
        &self,
        config: &EgoAcpConfig,
        request: AcpConnectRequest,
    ) -> Result<AcpConnectionSnapshot, AcpClientError> {
        self.connect_within_with_peer(config, request, INITIALIZE_TIMEOUT, None)
            .await
    }

    pub async fn connect_with_peer(
        &self,
        config: &EgoAcpConfig,
        request: AcpConnectRequest,
        peer_id: String,
    ) -> Result<AcpConnectionSnapshot, AcpClientError> {
        self.connect_within_with_peer(config, request, INITIALIZE_TIMEOUT, Some(peer_id))
            .await
    }

    /// [`connect`](Self::connect) with the initialize budget as a parameter.
    ///
    /// The deadline IS the subject for the test that proves a silent agent is
    /// abandoned rather than held forever, and that test needs a bound it can
    /// exceed in milliseconds. A `cfg(test)` constant would leave production
    /// and the suite exercising different code (AGENTS.md, "Which timing
    /// assertions are load-bearing").
    pub async fn connect_within(
        &self,
        config: &EgoAcpConfig,
        request: AcpConnectRequest,
        initialize_timeout: std::time::Duration,
    ) -> Result<AcpConnectionSnapshot, AcpClientError> {
        self.connect_within_with_peer(config, request, initialize_timeout, None)
            .await
    }

    async fn connect_within_with_peer(
        &self,
        config: &EgoAcpConfig,
        request: AcpConnectRequest,
        initialize_timeout: std::time::Duration,
        peer_id: Option<String>,
    ) -> Result<AcpConnectionSnapshot, AcpClientError> {
        let peer_lock = if let Some(peer_id) = peer_id.as_deref() {
            validate_peer_id(peer_id)?;
            let mut locks = self.peer_connect_locks.lock();
            locks.retain(|_, lock| lock.strong_count() > 0);
            Some(
                locks
                    .get(peer_id)
                    .and_then(Weak::upgrade)
                    .unwrap_or_else(|| {
                        let lock = Arc::new(AsyncMutex::new(()));
                        locks.insert(peer_id.to_owned(), Arc::downgrade(&lock));
                        lock
                    }),
            )
        } else {
            None
        };
        let _peer_guard = match &peer_lock {
            Some(lock) => Some(lock.lock().await),
            None => None,
        };
        if let Some(peer_id) = peer_id.as_deref() {
            // One identity is one process. A reloaded webview or a second
            // client asking again gets the ego this peer already has; `reconnect`
            // is the way to a fresh one.
            if let Some(connection_id) = self.live_connection_for_peer(peer_id) {
                return self.snapshot(connection_id);
            }
        }
        let executable = canonical_executable(&config.executable).await?;
        let root = canonical_root(&request.root).await?;
        let spec = launch_spec(
            &EgoAcpConfig {
                executable,
                profile: config.profile.clone(),
            },
            &root,
        )?;
        let mut agent_config = AcpAgentConfig::new(spec.program).args(spec.args);
        if let Some(peer_id) = &peer_id {
            agent_config = agent_config.env("TUIC_SESSION", peer_id);
        }
        let agent = AcpAgent::new(agent_config);
        let connection_id = AcpConnectionId::new();
        let generation = self.next_generation.fetch_add(1, Ordering::Relaxed);
        let (initialized_tx, initialized_rx) = oneshot::channel();
        let (shutdown_tx, shutdown_rx) = oneshot::channel();
        let (registered_tx, registered_rx) = oneshot::channel();
        let (commands_tx, commands_rx) = mpsc::channel(COMMAND_QUEUE);
        let (inbound, updates_rx) = mpsc::channel(UPDATE_QUEUE);
        let journal = Arc::new(AcpEventJournal::new(
            connection_id,
            generation,
            self.notices.clone(),
        ));

        let supervisor = tokio::spawn(supervise_connection(
            connection_id,
            generation,
            agent,
            initialize_timeout,
            SupervisorWiring {
                initialized: initialized_tx,
                registered: registered_rx,
                commands: commands_rx,
                inbound,
                updates: updates_rx,
                shutdown: shutdown_rx,
                journal: Arc::clone(&journal),
                mcp: McpChannel::new(self.mcp_host.lock().clone(), peer_id.clone()),
            },
            Arc::clone(&self.connections),
        ));

        let initialized = match initialized_rx.await {
            Ok(Ok(initialized)) => initialized,
            Ok(Err(error)) => {
                let _ = supervisor.await;
                return Err(error);
            }
            Err(_) => {
                let _ = supervisor.await;
                return Err(AcpClientError::initialization_failed(
                    connection_id,
                    "ACP process ended before initialization completed",
                ));
            }
        };

        // Filed before the bounds are read, so the snapshot this returns
        // already covers it: a host that subscribes from `latest_sequence + 1`
        // has been told the connection is usable, and one that replays from
        // zero sees the same first event rather than a stream that starts
        // mid-conversation.
        journal.append(
            None,
            None,
            AcpClientEvent::ConnectionState {
                state: AcpConnectionState::Ready,
            },
        );
        let (earliest_sequence, latest_sequence) = journal.bounds();
        let snapshot = AcpConnectionSnapshot {
            connection_id,
            generation,
            state: AcpConnectionState::Ready,
            agent_info: initialized.agent_info,
            capabilities: Some(initialized.capabilities),
            attachments: Vec::new(),
            earliest_sequence,
            latest_sequence,
            settlement: None,
        };
        self.connections.lock().insert(
            connection_id,
            ConnectionHandle {
                snapshot: snapshot.clone(),
                peer_id,
                root,
                attachment_directories: HashMap::new(),
                commands: commands_tx,
                journal,
                shutdown: Some(shutdown_tx),
                supervisor: Some(supervisor),
            },
        );
        let _ = registered_tx.send(());

        Ok(snapshot)
    }

    pub async fn reconnect(
        &self,
        config: &EgoAcpConfig,
        request: AcpReconnectRequest,
    ) -> Result<AcpConnectionSnapshot, AcpClientError> {
        self.reconnect_with_peer(config, request, None).await
    }

    pub async fn reconnect_with_peer(
        &self,
        config: &EgoAcpConfig,
        request: AcpReconnectRequest,
        peer_id: Option<String>,
    ) -> Result<AcpConnectionSnapshot, AcpClientError> {
        self.snapshot(request.connection_id)?;
        if let Some(peer_id) = peer_id.as_deref() {
            validate_peer_id(peer_id)?;
        }
        // Validate the replacement BEFORE giving up the working connection.
        // Every input check lives inside `connect`, so disconnecting first meant
        // a renamed root or a moved binary destroyed the live connection, every
        // attachment on it and every running turn, and then answered
        // `invalid_input`. There is no rollback and the old id is gone, so a
        // request that could never have succeeded left the caller with nothing.
        let executable = canonical_executable(&config.executable).await?;
        let root = canonical_root(&request.root).await?;
        launch_spec(
            &EgoAcpConfig {
                executable,
                profile: config.profile.clone(),
            },
            &root,
        )?;

        self.disconnect(request.connection_id).await?;
        self.connect_within_with_peer(
            config,
            AcpConnectRequest { root },
            INITIALIZE_TIMEOUT,
            peer_id,
        )
        .await
    }

    /// Open a durable session on a live connection.
    ///
    /// The authority is supplied here rather than remembered, because a session
    /// created against a wider authority than the caller currently holds is a
    /// privilege the caller was never granted.
    pub async fn new_session(
        &self,
        connection_id: AcpConnectionId,
        authority: AcpSessionAuthority,
    ) -> Result<AcpAttachmentSnapshot, AcpClientError> {
        let authority = self.granted(authority);
        self.dispatch(connection_id, |reply| Command::NewSession {
            authority,
            profiles: None,
            reply,
        })
        .await
    }

    /// Apply a repository selection only with a machine ceiling. Ego owns the
    /// policy merge; this host supplies names, never policy values.
    pub async fn new_session_for_repo(
        &self,
        connection_id: AcpConnectionId,
        authority: AcpSessionAuthority,
        machine_profile: &str,
    ) -> Result<AcpAttachmentSnapshot, AcpClientError> {
        let cwd = canonical_root(&authority.cwd).await?;
        let repo = tokio::task::spawn_blocking(move || {
            crate::config::load_repo_local_config_from_path(&cwd)
        })
        .await
        .map_err(|error| {
            AcpClientError::invalid_input(format!("cannot read repo profile: {error}"))
        })?;
        let Some(profile) = repo.and_then(|config| config.ego_profile) else {
            return self.new_session(connection_id, authority).await;
        };
        if machine_profile.is_empty() {
            return Err(AcpClientError::invalid_input(
                "a repository ego_profile requires an explicit machine ego profile in Settings",
            ));
        }
        let profiles = serde_json::json!({"profile": profile, "ceilingProfile": machine_profile});
        let authority = self.granted(authority);
        self.dispatch(connection_id, |reply| Command::NewSession {
            authority,
            profiles: Some(profiles),
            reply,
        })
        .await
    }

    /// Open a session with no host tools, for a turn nobody is watching.
    ///
    /// The only difference from [`new_session`](Self::new_session) is what the
    /// session may reach. Every attended session is given TUICommander's own
    /// MCP server, which is how ego drives terminals and repositories; granting
    /// that to a turn with no one at the keyboard to approve a single call is
    /// an authority nobody asked for. So the list is emptied here rather than
    /// filled by `granted`, and it is emptied rather than left to the caller:
    /// a caller that could choose would be a caller that could choose wrong.
    ///
    /// Ego's own tools are still ego's to offer. `acp::oneshot` refuses each
    /// one as it is asked for.
    pub async fn new_unattended_session(
        &self,
        connection_id: AcpConnectionId,
        authority: AcpSessionAuthority,
    ) -> Result<AcpAttachmentSnapshot, AcpClientError> {
        let authority = unattended(authority);
        self.dispatch(connection_id, |reply| Command::NewSession {
            authority,
            profiles: None,
            reply,
        })
        .await
    }

    /// Attach this connection to a session ego already owns.
    ///
    /// The returned attachment names the session that is now attached, which
    /// for a fork is not the one that was asked for.
    pub async fn attach(
        &self,
        connection_id: AcpConnectionId,
        kind: AcpAttachKind,
        session_id: v1::SessionId,
        authority: AcpSessionAuthority,
    ) -> Result<AcpAttachmentSnapshot, AcpClientError> {
        self.attach_at_message(connection_id, kind, session_id, authority, None)
            .await
    }

    pub async fn attach_at_message(
        &self,
        connection_id: AcpConnectionId,
        kind: AcpAttachKind,
        session_id: v1::SessionId,
        authority: AcpSessionAuthority,
        at_message_id: Option<String>,
    ) -> Result<AcpAttachmentSnapshot, AcpClientError> {
        let authority = self.granted(authority);
        self.dispatch(connection_id, |reply| Command::Attach {
            at_message_id,
            kind,
            session_id,
            authority,
            reply,
        })
        .await
    }

    pub async fn detach(
        &self,
        connection_id: AcpConnectionId,
        kind: AcpDetachKind,
        session_id: v1::SessionId,
    ) -> Result<(), AcpClientError> {
        let attachment_id = session_id.to_string();
        self.dispatch(connection_id, |reply| Command::Detach {
            kind,
            session_id,
            reply,
        })
        .await?;
        let mut connections = self.connections.lock();
        if let Some(connection) = connections.get_mut(&connection_id)
            && let Some((data_dir, key)) = connection.attachment_directories.remove(&attachment_id)
        {
            crate::attachments::cleanup_session(&data_dir, "acp", &key);
        }
        Ok(())
    }

    pub async fn list_sessions(
        &self,
        connection_id: AcpConnectionId,
        request: v1::ListSessionsRequest,
    ) -> Result<v1::ListSessionsResponse, AcpClientError> {
        self.dispatch(connection_id, |reply| Command::ListSessions {
            request,
            reply,
        })
        .await
    }

    /// The picker needs the complete ancestry before it can order paginated rows.
    pub async fn list_sessions_for_display(
        &self,
        connection_id: AcpConnectionId,
        mut request: v1::ListSessionsRequest,
    ) -> Result<v1::ListSessionsResponse, AcpClientError> {
        let mut response = self.list_sessions(connection_id, request.clone()).await?;
        while let Some(cursor) = response.next_cursor.take() {
            request.cursor = Some(cursor);
            let page = self.list_sessions(connection_id, request.clone()).await?;
            response.sessions.extend(page.sessions);
            response.next_cursor = page.next_cursor;
        }
        response.sessions = super::session_tree(response.sessions);
        Ok(response)
    }

    /// Start a turn and get back its id, not its outcome.
    ///
    /// The outcome is an event, because a turn outlives the call that started
    /// it and more than one reader needs to know how it ended.
    /// [`prompt`](Self::prompt) with the repository on screen as a hint for
    /// this turn only, as `_meta.tuicommander/viewedRepo`. The session's cwd is
    /// untouched: the hint names what a person is looking at, never what ego
    /// may reach.
    pub async fn prompt_with_context(
        &self,
        connection_id: AcpConnectionId,
        session_id: v1::SessionId,
        prompt: Vec<v1::ContentBlock>,
        viewed_repo: Option<String>,
    ) -> Result<AcpTurnId, AcpClientError> {
        let meta = viewed_repo.map(|path| {
            let mut meta = v1::Meta::new();
            meta.insert(VIEWED_REPO_META_KEY.to_owned(), path.into());
            meta
        });
        self.dispatch(connection_id, |reply| Command::Prompt {
            session_id,
            prompt,
            meta,
            reply,
        })
        .await
    }

    pub async fn prompt(
        &self,
        connection_id: AcpConnectionId,
        session_id: v1::SessionId,
        prompt: Vec<v1::ContentBlock>,
    ) -> Result<AcpTurnId, AcpClientError> {
        self.prompt_with_context(connection_id, session_id, prompt, None)
            .await
    }

    /// Ask the running turn to stop. It settles on its own response.
    pub async fn cancel(
        &self,
        connection_id: AcpConnectionId,
        session_id: v1::SessionId,
    ) -> Result<(), AcpClientError> {
        self.dispatch(connection_id, |reply| Command::Cancel {
            session_id,
            expected_turn: None,
            reply,
        })
        .await
    }

    /// A delayed external Stop must never cancel a replacement turn.
    pub(crate) async fn cancel_turn(
        &self,
        connection_id: AcpConnectionId,
        session_id: v1::SessionId,
        turn: AcpTurnId,
    ) -> Result<(), AcpClientError> {
        self.dispatch(connection_id, |reply| Command::Cancel {
            session_id,
            expected_turn: Some(turn),
            reply,
        })
        .await
    }

    pub async fn cancel_queued(
        &self,
        connection_id: AcpConnectionId,
        session_id: v1::SessionId,
        turn_id: AcpTurnId,
    ) -> Result<(), AcpClientError> {
        self.dispatch(connection_id, |reply| Command::CancelQueued {
            session_id,
            turn_id,
            reply,
        })
        .await
    }

    /// Set one config option and take the full set back.
    ///
    /// The whole set is the answer because it is what the agent sent: setting
    /// one option can change what the others offer, and a client that returned
    /// only the one it set would leave a host rendering a stale list.
    pub async fn set_config_option(
        &self,
        connection_id: AcpConnectionId,
        request: v1::SetSessionConfigOptionRequest,
    ) -> Result<Vec<v1::SessionConfigOption>, AcpClientError> {
        self.dispatch(connection_id, |reply| Command::SetConfigOption {
            request,
            reply,
        })
        .await
    }

    /// Ask ego to hold this session at the next boundary.
    ///
    /// `pending` is not a failure: it means the request is recorded and the
    /// turn has not reached a boundary yet. Only `paused` says nothing more
    /// runs.
    pub async fn pause_turn(
        &self,
        connection_id: AcpConnectionId,
        request: EgoHoldRequest,
    ) -> Result<EgoHoldResponse, AcpClientError> {
        self.dispatch(connection_id, |reply| Command::Hold {
            release: false,
            request,
            reply,
        })
        .await
    }

    /// Release a hold. Not `session/resume`, which attaches a session.
    pub async fn resume_turn(
        &self,
        connection_id: AcpConnectionId,
        request: EgoHoldRequest,
    ) -> Result<EgoHoldResponse, AcpClientError> {
        self.dispatch(connection_id, |reply| Command::Hold {
            release: true,
            request,
            reply,
        })
        .await
    }

    /// Compact a session into a successor.
    ///
    /// The receipt's publication is the part that matters: a target that may
    /// have been published is never retried as a new compaction, because that
    /// risks a second successor for one source.
    pub async fn compact(
        &self,
        connection_id: AcpConnectionId,
        request: EgoCompactRequest,
    ) -> Result<EgoCompactResponse, AcpClientError> {
        self.dispatch(connection_id, |reply| Command::Compact { request, reply })
            .await
    }

    /// Give a person's decision to the permission request that is waiting on it.
    pub async fn respond_permission(
        &self,
        connection_id: AcpConnectionId,
        request_id: AcpHostRequestId,
        outcome: v1::RequestPermissionOutcome,
    ) -> Result<AcpInteractionSettlement, AcpClientError> {
        self.dispatch(connection_id, |reply| Command::Respond {
            request_id,
            answer: Answer::Permission(outcome),
            reply,
        })
        .await
    }

    /// Give a person's decision to the elicitation that is waiting on it.
    pub async fn respond_elicitation(
        &self,
        connection_id: AcpConnectionId,
        request_id: AcpHostRequestId,
        action: v1::ElicitationAction,
    ) -> Result<AcpInteractionSettlement, AcpClientError> {
        self.dispatch(connection_id, |reply| Command::Respond {
            request_id,
            answer: Answer::Elicitation(action),
            reply,
        })
        .await
    }

    /// Every request this connection is waiting on a person for, in the order
    /// the agent asked.
    pub async fn pending_interactions(
        &self,
        connection_id: AcpConnectionId,
    ) -> Result<Vec<AcpPendingInteraction>, AcpClientError> {
        self.dispatch(connection_id, |reply| Command::PendingInteractions {
            reply,
        })
        .await
    }

    /// Read what happened on a connection, from `from` onwards.
    ///
    /// Works on a settled connection too: what it recorded is still true, and a
    /// host that reconnects to read the end of a turn should not be told the
    /// connection is gone before it has seen how the turn ended.
    pub fn subscribe(
        &self,
        connection_id: AcpConnectionId,
        from: u64,
    ) -> Result<AcpEventStream, AcpClientError> {
        let journal = {
            let connections = self.connections.lock();
            let connection = connections
                .get(&connection_id)
                .ok_or_else(|| AcpClientError::not_found(connection_id))?;
            Arc::clone(&connection.journal)
        };
        journal.subscribe(from)
    }

    #[cfg(test)]
    pub(crate) fn test_peer_journal(&self, peer: &str) -> Arc<AcpEventJournal> {
        let id = AcpConnectionId::new();
        let generation = self.next_generation.fetch_add(1, Ordering::Relaxed);
        let journal = Arc::new(AcpEventJournal::new(id, generation, self.notices.clone()));
        let (commands, _) = mpsc::channel(1);
        self.connections.lock().insert(
            id,
            ConnectionHandle {
                snapshot: AcpConnectionSnapshot {
                    connection_id: id,
                    generation,
                    state: AcpConnectionState::Ready,
                    agent_info: None,
                    capabilities: None,
                    attachments: vec![],
                    earliest_sequence: 1,
                    latest_sequence: 0,
                    settlement: None,
                },
                peer_id: Some(peer.into()),
                root: crate::test_support::test_temp_root(),
                attachment_directories: HashMap::new(),
                commands,
                journal: journal.clone(),
                shutdown: None,
                supervisor: None,
            },
        );
        journal
    }

    /// Latest connected AI Chat wins only when Telegram has no explicit owner.
    pub(crate) fn latest_live_peer(&self, peers: &[String]) -> Option<String> {
        self.connections
            .lock()
            .values()
            .filter(|connection| {
                connection.snapshot.settlement.is_none()
                    && connection.snapshot.state == AcpConnectionState::Ready
                    && connection
                        .peer_id
                        .as_ref()
                        .is_some_and(|peer| peers.contains(peer))
            })
            .max_by_key(|connection| connection.snapshot.generation)
            .and_then(|connection| connection.peer_id.clone())
    }

    /// Observe only events accepted after this call for the peer's current connection.
    pub(crate) fn subscribe_peer_tail(&self, peer: &str) -> Option<(AcpEventStream, bool)> {
        let connections = self.connections.lock();
        let connection = connections
            .values()
            .filter(|connection| {
                connection.peer_id.as_deref() == Some(peer)
                    && connection.snapshot.settlement.is_none()
            })
            .max_by_key(|connection| connection.snapshot.generation)?;
        let (_, latest) = connection.journal.bounds();
        let idle = connection
            .snapshot
            .attachments
            .iter()
            .all(|attachment| attachment.active_turn.is_none());
        Some((
            connection
                .journal
                .subscribe(latest.saturating_add(1))
                .ok()?,
            idle,
        ))
    }

    /// Hand one command to a connection's actor and wait for its answer.
    ///
    /// A connection this manager never had and one that has settled are
    /// different answers on purpose: the first is a caller naming something
    /// that does not exist, the second is a caller holding an id that was real
    /// and no longer is, and only the second means "make a new connection".
    async fn dispatch<T>(
        &self,
        connection_id: AcpConnectionId,
        command: impl FnOnce(oneshot::Sender<Result<T, AcpClientError>>) -> Command,
    ) -> Result<T, AcpClientError> {
        let commands = {
            let connections = self.connections.lock();
            let connection = connections
                .get(&connection_id)
                .ok_or_else(|| AcpClientError::not_found(connection_id))?;
            if connection.snapshot.settlement.is_some() {
                return Err(AcpClientError::transport_closed(connection_id));
            }
            connection.commands.clone()
        };

        let (reply, answer) = oneshot::channel();
        // Both failures below mean the actor is gone: the connection settled
        // between the check above and now, which is exactly the race the
        // settled state describes rather than a failure of the operation.
        commands
            .send(command(reply))
            .await
            .map_err(|_| AcpClientError::transport_closed(connection_id))?;
        answer
            .await
            .map_err(|_| AcpClientError::transport_closed(connection_id))?
    }

    pub fn snapshot(
        &self,
        connection_id: AcpConnectionId,
    ) -> Result<AcpConnectionSnapshot, AcpClientError> {
        self.connections
            .lock()
            .get(&connection_id)
            .map(|connection| {
                // Read from the journal rather than from a copy kept in step
                // with it: the bounds move with every event, and a copy would
                // have to be republished on each one just to stay true.
                let (earliest, latest) = connection.journal.bounds();
                AcpConnectionSnapshot {
                    earliest_sequence: earliest,
                    latest_sequence: latest,
                    ..connection.snapshot.clone()
                }
            })
            .ok_or_else(|| AcpClientError::not_found(connection_id))
    }

    /// Resolve only live ACP connections. A settled connection must never
    /// route a blocked report to a conversation that no longer has a seat.
    pub fn peer_conversation(&self, peer_id: &str) -> Option<(PathBuf, v1::SessionId)> {
        self.connections
            .lock()
            .values()
            .filter(|connection| {
                connection.peer_id.as_deref() == Some(peer_id)
                    && connection.snapshot.settlement.is_none()
                    && !connection.snapshot.attachments.is_empty()
            })
            .max_by_key(|connection| connection.snapshot.generation)
            .and_then(|connection| {
                connection
                    .snapshot
                    .attachments
                    .iter()
                    .rev()
                    .find(|attachment| attachment.active_turn.is_some())
                    .or_else(|| connection.snapshot.attachments.last())
                    .map(|attachment| (connection.root.clone(), attachment.session_id.clone()))
            })
    }

    /// Start a turn carrying `notice` in the peer's conversation, only when no
    /// turn is running on its connection. A busy ego hears about mail at its
    /// next tool boundary instead, through `resources/updated`.
    pub async fn wake_idle_peer(
        &self,
        peer_id: &str,
        notice: &str,
    ) -> Result<Option<AcpTurnId>, AcpClientError> {
        let target = self
            .connections
            .lock()
            .iter()
            .filter(|(_, connection)| {
                connection.peer_id.as_deref() == Some(peer_id)
                    && connection.snapshot.settlement.is_none()
            })
            .max_by_key(|(_, connection)| connection.snapshot.generation)
            .and_then(|(connection_id, connection)| {
                let attachments = &connection.snapshot.attachments;
                let busy = attachments.iter().any(|attachment| {
                    attachment.active_turn.is_some() || attachment.state != AcpAttachmentState::Idle
                });
                let session = attachments.last()?.session_id.clone();
                (!busy).then_some((*connection_id, session))
            });
        let Some((connection_id, session_id)) = target else {
            return Ok(None);
        };
        let notice = v1::ContentBlock::Text(v1::TextContent::new(notice));
        self.prompt(connection_id, session_id, vec![notice])
            .await
            .map(Some)
    }

    fn live_connection_for_peer(&self, peer_id: &str) -> Option<AcpConnectionId> {
        self.connections
            .lock()
            .iter()
            .filter(|(_, connection)| {
                connection.peer_id.as_deref() == Some(peer_id)
                    && connection.snapshot.settlement.is_none()
            })
            .max_by_key(|(_, connection)| connection.snapshot.generation)
            .map(|(connection_id, _)| *connection_id)
    }

    pub fn peer_root(&self, peer_id: &str) -> Option<PathBuf> {
        self.connections
            .lock()
            .values()
            .filter(|connection| {
                connection.peer_id.as_deref() == Some(peer_id)
                    && connection.snapshot.settlement.is_none()
            })
            .max_by_key(|connection| connection.snapshot.generation)
            .map(|connection| connection.root.clone())
    }

    /// Bind binary uploads to one attached conversation, including EOF cleanup.
    pub(crate) fn attachment_directory(
        &self,
        connection_id: AcpConnectionId,
        session_id: &str,
        data_dir: &Path,
    ) -> Result<PathBuf, String> {
        use sha2::{Digest, Sha256};
        let mut connections = self.connections.lock();
        let connection = connections
            .get_mut(&connection_id)
            .filter(|connection| connection.snapshot.settlement.is_none())
            .ok_or("ACP connection not found")?;
        if !connection
            .snapshot
            .attachments
            .iter()
            .any(|attachment| attachment.session_id.to_string() == session_id)
        {
            return Err("ACP session is not attached".into());
        }
        let key = format!(
            "{connection_id}-{}",
            hex::encode(Sha256::digest(session_id.as_bytes()))
        );
        let directory = crate::attachments::prepare_session_directory(data_dir, "acp", &key)?;
        connection
            .attachment_directories
            .insert(session_id.into(), (data_dir.into(), key));
        Ok(directory)
    }

    pub fn connection_root(&self, connection_id: AcpConnectionId) -> Option<PathBuf> {
        self.connections
            .lock()
            .get(&connection_id)
            .filter(|connection| connection.snapshot.settlement.is_none())
            .map(|connection| connection.root.clone())
    }

    /// End every connection, for the app quitting.
    ///
    /// The process exits without running the destructors that kill each ego's
    /// process group, so an ego left to them would outlive the app. Killed
    /// rather than disconnected: nobody is left to read a graceful goodbye.
    pub async fn shutdown_all(&self) {
        for connection_id in self.connection_ids() {
            let _ = self.kill(connection_id).await;
        }
    }

    #[must_use]
    pub fn connection_ids(&self) -> Vec<AcpConnectionId> {
        self.connections.lock().keys().copied().collect()
    }

    pub async fn disconnect(
        &self,
        connection_id: AcpConnectionId,
    ) -> Result<AcpConnectionSettlement, AcpClientError> {
        let (shutdown, supervisor, settled) = {
            let mut connections = self.connections.lock();
            let connection = connections
                .get_mut(&connection_id)
                .ok_or_else(|| AcpClientError::not_found(connection_id))?;
            if let Some(settlement) = connection.snapshot.settlement {
                (None, None, Some(settlement))
            } else {
                connection.snapshot.state = AcpConnectionState::Closing;
                (
                    connection.shutdown.take(),
                    connection.supervisor.take(),
                    None,
                )
            }
        };
        if let Some(settlement) = settled {
            return Ok(settlement);
        }

        if let Some(shutdown) = shutdown {
            let _ = shutdown.send(());
        }
        if let Some(supervisor) = supervisor {
            let _ = supervisor.await;
        }

        self.snapshot(connection_id)?.settlement.ok_or_else(|| {
            AcpClientError::initialization_failed(
                connection_id,
                "ACP supervisor ended without settling the connection",
            )
        })
    }

    pub async fn kill(
        &self,
        connection_id: AcpConnectionId,
    ) -> Result<AcpConnectionSettlement, AcpClientError> {
        let (shutdown, supervisor, generation, settled) = {
            let mut connections = self.connections.lock();
            let connection = connections
                .get_mut(&connection_id)
                .ok_or_else(|| AcpClientError::not_found(connection_id))?;
            (
                connection.shutdown.take(),
                connection.supervisor.take(),
                connection.snapshot.generation,
                connection.snapshot.settlement,
            )
        };
        if let Some(settlement) = settled {
            return Ok(settlement);
        }

        if let Some(supervisor) = supervisor {
            supervisor.abort();
            let _ = supervisor.await;
        }

        drop(shutdown);
        settle_connection(
            &self.connections,
            connection_id,
            generation,
            AcpConnectionSettlementReason::Killed,
        );
        self.snapshot(connection_id)?.settlement.ok_or_else(|| {
            AcpClientError::initialization_failed(connection_id, "ACP kill did not settle")
        })
    }
}

fn validate_peer_id(peer_id: &str) -> Result<(), AcpClientError> {
    if super::valid_peer_id(peer_id) {
        Ok(())
    } else {
        Err(AcpClientError::invalid_input(
            "ACP peer identity must be a canonical UUID",
        ))
    }
}

/// Hand one reverse request to the actor, or refuse it here if it cannot be.
///
/// A full queue is the one case this cannot do: the request is settled on the
/// spot with the answer that grants nothing, out of the order the agent asked
/// in. That is the honest trade — the alternative is an agent waiting forever
/// on a seat that was never taken.
fn queue_interaction(
    inbound: &mpsc::Sender<Inbound>,
    interaction: Interaction,
) -> Result<(), agent_client_protocol::Error> {
    if let Err(error) = inbound.try_send(Inbound::Interaction(Box::new(interaction))) {
        let Inbound::Interaction(interaction) = error.into_inner() else {
            unreachable!("the value returned is the one that was just sent");
        };
        interaction.refuse();
    }
    Ok(())
}

/// Every channel a supervisor is wired to, in the order it uses them.
///
/// They travel together because they are one handshake: the supervisor reports
/// initialization, waits to be registered, then serves commands until told to
/// stop. Passing them separately said nothing extra and only made the call
/// site a list of look-alike channels.
struct SupervisorWiring {
    initialized: oneshot::Sender<Result<InitializedConnection, AcpClientError>>,
    registered: oneshot::Receiver<()>,
    commands: mpsc::Receiver<Command>,
    /// Both halves of the inbound channel: the sender belongs to the SDK
    /// dispatch callbacks, which are registered here rather than by the caller.
    inbound: mpsc::Sender<Inbound>,
    updates: mpsc::Receiver<Inbound>,
    shutdown: oneshot::Receiver<()>,
    journal: Arc<AcpEventJournal>,
    mcp: McpChannel,
}

async fn supervise_connection(
    connection_id: AcpConnectionId,
    generation: u64,
    agent: AcpAgent,
    initialize_timeout: std::time::Duration,
    wiring: SupervisorWiring,
    connections: Arc<Mutex<HashMap<AcpConnectionId, ConnectionHandle>>>,
) {
    let SupervisorWiring {
        initialized,
        registered,
        mut commands,
        inbound,
        mut updates,
        shutdown,
        journal,
        mcp,
    } = wiring;
    let ready = Arc::new(AtomicBool::new(false));
    let closure_ready = Arc::clone(&ready);
    let actor_connections = Arc::clone(&connections);
    // One sender per callback, one channel for all of them: updates and the
    // requests the agent is waiting on arrive interleaved on the wire, and the
    // order between them is the record. Two channels would let a permission
    // request overtake the update that explains why it was asked.
    let updates_in = inbound.clone();
    let permissions_in = inbound.clone();
    let elicitations_in = inbound;
    let (mcp_connect, mcp_message, mcp_notified, mcp_disconnect) =
        (mcp.clone(), mcp.clone(), mcp.clone(), mcp.clone());
    let outcome = Client
        .builder()
        // Deliberately short, because it holds the SDK's dispatch loop: the
        // update is handed to the actor and nothing else happens here.
        .on_receive_notification(
            async move |notification: v1::SessionNotification, _connection| {
                updates_in
                    .try_send(Inbound::Update(Box::new(notification)))
                    .map_err(|error| {
                        // Never a silent drop. A host that renders a turn with
                        // a hole in it is confidently wrong about what the
                        // agent said, which is worse than a connection that
                        // failed.
                        agent_client_protocol::Error::internal_error()
                            .data(format!("ACP update queue: {error}"))
                    })
            },
            agent_client_protocol::on_receive_notification!(),
        )
        // The responder is handed to the actor rather than answered here: a
        // person decides this, and holding the dispatch loop until they do
        // would stop every other message on the connection — including the
        // updates that say what the agent is asking about.
        .on_receive_request(
            async move |request: v1::RequestPermissionRequest,
                        responder: Responder<v1::RequestPermissionResponse>,
                        _connection| {
                queue_interaction(
                    &permissions_in,
                    Interaction::Permission {
                        request: Box::new(request),
                        responder,
                    },
                )
            },
            agent_client_protocol::on_receive_request!(),
        )
        .on_receive_request(
            async move |request: v1::CreateElicitationRequest,
                        responder: Responder<v1::CreateElicitationResponse>,
                        _connection| {
                queue_interaction(
                    &elicitations_in,
                    Interaction::Elicitation {
                        request: Box::new(request),
                        responder,
                    },
                )
            },
            agent_client_protocol::on_receive_request!(),
        )
        // MCP-over-ACP. Answered here rather than by the actor, because none
        // of it touches attachment state: it is the `tuicommander` server
        // reached over this connection. A request is answered off the dispatch
        // loop, since a tool call can take as long as the tool does.
        .on_receive_request(
            async move |request: v1::ConnectMcpRequest,
                        responder: Responder<v1::ConnectMcpResponse>,
                        connection: ConnectionTo<Agent>| {
                responder.respond_with_result(mcp_connect.connect(&request, connection))
            },
            agent_client_protocol::on_receive_request!(),
        )
        .on_receive_request(
            async move |request: v1::MessageMcpRequest,
                        responder: Responder<v1::MessageMcpResponse>,
                        _connection| {
                match mcp_message.message(request) {
                    // `$/cancel_request` names this request by its ACP id, the
                    // only id it has; the reply future is dropped, which stops
                    // the call at its next await.
                    Ok(reply) => {
                        let cancellation = responder.cancellation();
                        tokio::spawn(async move {
                            let outcome = cancellation.run_until_cancelled(reply).await;
                            let _ = responder.respond_with_result(outcome);
                        });
                        Ok(())
                    }
                    Err(error) => responder.respond_with_error(error),
                }
            },
            agent_client_protocol::on_receive_request!(),
        )
        .on_receive_notification(
            async move |notification: v1::MessageMcpNotification, _connection| {
                mcp_notified.notification(notification);
                Ok(())
            },
            agent_client_protocol::on_receive_notification!(),
        )
        .on_receive_request(
            async move |request: v1::DisconnectMcpRequest,
                        responder: Responder<v1::DisconnectMcpResponse>,
                        _connection| {
                responder.respond_with_result(mcp_disconnect.disconnect(&request))
            },
            agent_client_protocol::on_receive_request!(),
        )
        .connect_with(agent, move |connection: ConnectionTo<Agent>| async move {
            // Budgeted here rather than at the caller, because only here can
            // expiry do anything: returning from this closure ends the
            // supervisor and stops the child, while a deadline outside would
            // drop a future that is not holding the process.
            let initialize = tokio::time::timeout(
                initialize_timeout,
                connection.send_request(build_initialize_request()).block_task(),
            )
            .await;
            let response = match initialize {
                Ok(Ok(response)) => response,
                Ok(Err(error)) => {
                    let initialization_error = AcpClientError::initialization_failed(
                        connection_id,
                        format!("ACP initialization failed: {error}"),
                    );
                    let _ = initialized.send(Err(initialization_error));
                    return Err(error);
                }
                Err(_) => {
                    // Alive and silent: no EOF will ever arrive, so this is the
                    // only thing that ends the connection.
                    let _ = initialized.send(Err(AcpClientError::initialization_failed(
                        connection_id,
                        format!(
                            "the agent did not answer initialize within {}s",
                            initialize_timeout.as_secs_f32()
                        ),
                    )));
                    return Ok(SupervisorExit::NotReady);
                }
            };

            let capabilities = match capability_snapshot(&response) {
                Ok(capabilities) => Arc::new(capabilities),
                Err(error) => {
                    let _ = initialized.send(Err(error.with_connection_id(connection_id)));
                    return Ok(SupervisorExit::NotReady);
                }
            };
            let ready = InitializedConnection {
                agent_info: response.agent_info,
                capabilities: AcpCapabilitySnapshot::clone(&capabilities),
            };
            if initialized.send(Ok(ready)).is_err() {
                return Ok(SupervisorExit::NotReady);
            }
            closure_ready.store(true, Ordering::Release);
            if registered.await.is_err() {
                return Ok(SupervisorExit::NotReady);
            }

            // The actor publishes for itself, next to the mutation and before
            // it answers anyone. Doing it out here — after `handle` or `accept`
            // returned — would leave the reply already sent and the settlement
            // already on the stream, so whoever they woke could read a snapshot
            // that has not caught up. The window is narrow, and narrow is not
            // the same as closed.
            let publish: Publish = {
                let connections = Arc::clone(&actor_connections);
                Arc::new(move |attachments| {
                    publish_attachments(&connections, connection_id, attachments);
                })
            };
            let mut actor = ConnectionActor::new(connection_id, capabilities, journal, publish);
            let mut shutdown = shutdown;
            let mut in_flight = InFlight::new();
            loop {
                // What the select produces is decided here and acted on below,
                // because acting inside the select would still hold borrows on
                // everything the other branches are watching.
                let step = tokio::select! {
                    biased;
                    _ = &mut shutdown => return Ok(SupervisorExit::Disconnected),
                    () = connection.incoming_closed() => return Ok(SupervisorExit::Eof),
                    // Updates come first, and specifically before the answers
                    // in flight. The SDK hands us an update before it routes
                    // the response that follows it on the wire, so an update
                    // waiting here is one the agent sent *before* whatever is
                    // now settling. Taking the settlement first would file a
                    // turn's last words after the record of it ending.
                    inbound = updates.recv() => match inbound {
                        Some(inbound) => Step::Accept(Accepted::Inbound(Box::new(inbound))),
                        None => return Ok(SupervisorExit::Disconnected),
                    },
                    // An empty `FuturesUnordered` yields `None`, which fails
                    // this pattern and disables the branch for that pass.
                    Some(pending) = in_flight.next() => Step::Accept(Accepted::Settled(Box::new(pending))),
                    command = commands.recv() => match command {
                        Some(command) => Step::Command(command),
                        // The manager holds the only sender, so this is the
                        // manager itself going away, not a caller hanging up.
                        None => return Ok(SupervisorExit::Disconnected),
                    },
                };

                match step {
                    Step::Command(command) => actor.handle(command, &connection, &in_flight),
                    Step::Accept(accepted) => actor.accept(accepted, &connection, &in_flight),
                }
                // Checked here rather than in the select, because the caller
                // whose request uncovered the contradiction has just been
                // answered and is owed that answer either way. Everything still
                // in flight settles as transport closed, which is what it now
                // is.
                if actor.contradicted() {
                    return Ok(SupervisorExit::ProtocolViolation);
                }
            }
        })
        .await;
    mcp.close_all();

    let reason = match outcome {
        Ok(SupervisorExit::Disconnected) => AcpConnectionSettlementReason::Disconnected,
        Ok(SupervisorExit::Eof) => AcpConnectionSettlementReason::Eof,
        Ok(SupervisorExit::ProtocolViolation) => AcpConnectionSettlementReason::ProtocolViolation,
        Ok(SupervisorExit::NotReady) => return,
        Err(_) if ready.load(Ordering::Acquire) => AcpConnectionSettlementReason::TransportError,
        _ => return,
    };
    settle_connection(&connections, connection_id, generation, reason);
}

/// Republish what a connection is attached to.
///
/// A settled connection is left alone: its attachment list is the last true
/// thing anyone knew about it, and a session recorded after settlement would
/// claim an attachment on a connection that can no longer carry one.
fn publish_attachments(
    connections: &Mutex<HashMap<AcpConnectionId, ConnectionHandle>>,
    connection_id: AcpConnectionId,
    attachments: Vec<AcpAttachmentSnapshot>,
) {
    let mut connections = connections.lock();
    let Some(connection) = connections.get_mut(&connection_id) else {
        return;
    };
    if connection.snapshot.settlement.is_some() {
        return;
    }
    connection.snapshot.attachments = attachments;
}

/// Forget the connections that settled longest ago, past [`SETTLED_RETAINED`].
///
/// Ordered by generation, which is the order they were created in and the only
/// order this map records. Settlement order would be a better answer to "which
/// one is stalest" and is not worth a second counter to get: the two differ
/// only when an old connection outlives a newer one, and either choice then
/// forgets something nobody asked about in a long time.
fn forget_stale_settled(connections: &mut HashMap<AcpConnectionId, ConnectionHandle>) {
    let mut settled: Vec<_> = connections
        .iter()
        .filter(|(_, connection)| connection.snapshot.settlement.is_some())
        .map(|(id, connection)| (connection.snapshot.generation, *id))
        .collect();
    if settled.len() <= SETTLED_RETAINED {
        return;
    }
    settled.sort_unstable_by_key(|(generation, _)| *generation);
    for (_, id) in &settled[..settled.len() - SETTLED_RETAINED] {
        connections.remove(id);
    }
}

fn settle_connection(
    connections: &Mutex<HashMap<AcpConnectionId, ConnectionHandle>>,
    connection_id: AcpConnectionId,
    generation: u64,
    reason: AcpConnectionSettlementReason,
) {
    let mut connections = connections.lock();
    let Some(connection) = connections.get_mut(&connection_id) else {
        return;
    };
    if connection.snapshot.settlement.is_some() {
        return;
    }

    for (_, (data_dir, key)) in connection.attachment_directories.drain() {
        crate::attachments::cleanup_session(&data_dir, "acp", &key);
    }

    connection.snapshot.state = match reason {
        AcpConnectionSettlementReason::Disconnected => AcpConnectionState::Closed,
        AcpConnectionSettlementReason::Killed => AcpConnectionState::Killed,
        AcpConnectionSettlementReason::Eof
        | AcpConnectionSettlementReason::TransportError
        | AcpConnectionSettlementReason::ProtocolViolation => AcpConnectionState::Failed,
    };
    connection.snapshot.settlement = Some(AcpConnectionSettlement {
        connection_id,
        generation,
        reason,
    });

    // Said on the stream as well as in the snapshot. A host watching the
    // stream would otherwise learn that the connection ended only from its
    // subscription going quiet, which is indistinguishable from an agent that
    // is simply thinking. Appended after the lock is released so the journal's
    // subscribers are never woken while this map is held.
    let state = connection.snapshot.state;
    let journal = Arc::clone(&connection.journal);
    // After this one has been recorded, never before: the connection that just
    // settled is the most recent of them and must survive its own settlement.
    forget_stale_settled(&mut connections);
    drop(connections);
    journal.append(None, None, AcpClientEvent::ConnectionState { state });
}

async fn canonical_executable(path: &Path) -> Result<std::path::PathBuf, AcpClientError> {
    if !path.is_absolute() {
        return Err(AcpClientError::invalid_input(
            "ego executable must be an absolute path",
        ));
    }
    let executable = tokio::fs::canonicalize(path).await.map_err(|error| {
        AcpClientError::invalid_input(format!("invalid ego executable: {error}"))
    })?;
    let metadata = tokio::fs::metadata(&executable).await.map_err(|error| {
        AcpClientError::invalid_input(format!("invalid ego executable: {error}"))
    })?;
    if !metadata.is_file() {
        return Err(AcpClientError::invalid_input(
            "ego executable must be a regular file",
        ));
    }
    Ok(executable)
}

async fn canonical_root(path: &Path) -> Result<std::path::PathBuf, AcpClientError> {
    let root = tokio::fs::canonicalize(path)
        .await
        .map_err(|error| AcpClientError::invalid_input(format!("invalid ACP root: {error}")))?;
    let metadata = tokio::fs::metadata(&root)
        .await
        .map_err(|error| AcpClientError::invalid_input(format!("invalid ACP root: {error}")))?;
    if !metadata.is_dir() {
        return Err(AcpClientError::invalid_input(
            "ACP root must be a directory",
        ));
    }
    Ok(root)
}

/// The counterpart of [`AcpClientManager::granted`] for a turn nobody watches:
/// whatever the caller brought, the session gets no MCP server.
///
/// A free function rather than two lines inside the method so the one rule this
/// story rests on can be asserted without a live connection.
fn unattended(authority: AcpSessionAuthority) -> AcpSessionAuthority {
    AcpSessionAuthority {
        mcp_servers: Vec::new(),
        ..authority
    }
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use super::*;

    #[test]
    fn an_unattended_session_is_given_no_mcp_server() {
        let authority = AcpSessionAuthority {
            cwd: PathBuf::from("/repo"),
            additional_directories: vec![PathBuf::from("/repo/docs")],
            // The real one, built the way `granted` builds it — an invented
            // server would prove only that some list was emptied.
            mcp_servers: vec![tuicommander_acp_mcp_server()],
        };
        assert_eq!(
            authority.mcp_servers.len(),
            1,
            "the fixture is the real grant"
        );

        let unattended = unattended(authority);

        // The whole authority of this mode. A turn with nobody at the keyboard
        // cannot be given the server that drives terminals and repositories,
        // whatever the caller asked for.
        assert_eq!(
            unattended.mcp_servers,
            [] as [agent_client_protocol::schema::v1::McpServer; 0]
        );
        // And nothing else moves: the directories are what the prompt runs on.
        assert_eq!(unattended.cwd, PathBuf::from("/repo"));
        assert_eq!(
            unattended.additional_directories,
            vec![PathBuf::from("/repo/docs")]
        );
    }
}
