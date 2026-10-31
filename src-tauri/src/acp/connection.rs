//! The one place that speaks on a live ACP connection.
//!
//! The SDK's connection handle exists only inside the future passed to
//! `connect_with`, so nothing outside can send on it. Everything the manager
//! offers therefore arrives here as a [`Command`] carrying its own reply
//! channel, and one actor owns the whole of a connection's mutable semantics.
//!
//! That is not a workaround for the SDK's shape, it is the shape the semantics
//! want. Attachment state, in-flight operations and pending human interactions
//! all have to agree with each other and with what is on the wire; a lock
//! around each would let two commands interleave between the capability check
//! and the write.
//!
//! ## Deciding is serial, waiting is not
//!
//! A command is *decided and written* before the next one is read, so
//! "advertised, then sent" is one indivisible step. Waiting for the answer is
//! not: the request goes into [`InFlight`] and the actor returns to its select.
//! A prompt takes as long as a model takes and a load replays a whole history,
//! so an actor that awaited them inline would hold up the very cancel meant to
//! stop them, and the updates they produce.
//!
//! Incoming `session/update` arrives on its own channel because the SDK
//! dispatch callback that produces it blocks message processing until it
//! returns. It therefore never waits for the actor, and a full queue is a fatal
//! connection error rather than a dropped update: a host cannot render a turn
//! with a hole in the middle of it and be right about what happened.
//!
//! Ego owns durable session identity, history, lineage, leases, run state and
//! cost. What lives here is attachment and correlation only — what *this*
//! connection is currently attached to, and which reply belongs to which
//! caller.

use std::collections::HashMap;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use agent_client_protocol::schema::v1;
use agent_client_protocol::{Agent, ConnectionTo, Responder};
use futures_util::future::BoxFuture;
use futures_util::stream::FuturesUnordered;
use tokio::sync::oneshot;

use super::events::AcpEventJournal;
use super::{
    AcpAttachKind, AcpAttachmentSnapshot, AcpAttachmentState, AcpCapabilitySnapshot,
    AcpClientError, AcpClientEvent, AcpConnectionId, AcpDetachKind, AcpHoldState, AcpHostRequestId,
    AcpInteractionSettlement, AcpOperation, AcpPendingInteraction, AcpQueuedPrompt,
    AcpSessionAuthority, AcpTurnId, AcpTurnSnapshot, AcpTurnState, AcpUsageSnapshot,
    EgoCompactRequest, EgoCompactResponse, EgoHoldRequest, EgoHoldResponse, ego_ext,
};

pub(super) type Reply<T> = oneshot::Sender<Result<T, AcpClientError>>;

/// Ego normally acknowledges this durable append in milliseconds. A silent
/// steer must release its FIFO reservation without retrying possibly accepted text.
const EGO_STEER_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(10);

/// One request from the manager, with the channel its answer goes back on.
///
/// A dropped receiver is not an error here: it means the caller went away, and
/// the effect either happened or did not on its own terms.
pub(super) enum Command {
    NewSession {
        authority: AcpSessionAuthority,
        profiles: Option<serde_json::Value>,
        reply: Reply<AcpAttachmentSnapshot>,
    },
    ListSessions {
        request: v1::ListSessionsRequest,
        reply: Reply<v1::ListSessionsResponse>,
    },
    Attach {
        at_message_id: Option<String>,
        kind: AcpAttachKind,
        session_id: v1::SessionId,
        authority: AcpSessionAuthority,
        reply: Reply<AcpAttachmentSnapshot>,
    },
    Detach {
        kind: AcpDetachKind,
        session_id: v1::SessionId,
        reply: Reply<()>,
    },
    /// Start a turn. The answer is the turn's id, not its outcome.
    ///
    /// The outcome arrives on the event stream, because the caller is not the
    /// only one who needs to know how a turn ended and is not guaranteed to
    /// still be listening when it does.
    Prompt {
        session_id: v1::SessionId,
        prompt: Vec<v1::ContentBlock>,
        /// Context for this turn only, such as the repository on screen.
        meta: Option<v1::Meta>,
        reply: Reply<AcpTurnId>,
    },
    Cancel {
        session_id: v1::SessionId,
        expected_turn: Option<AcpTurnId>,
        reply: Reply<()>,
    },
    CancelQueued {
        session_id: v1::SessionId,
        turn_id: AcpTurnId,
        reply: Reply<()>,
    },
    RecallQueued {
        session_id: v1::SessionId,
        turn_id: AcpTurnId,
        reply: Reply<String>,
    },
    /// Answer a request the agent is waiting on a person for.
    ///
    /// One command for both seats because everything about them is the same
    /// except the shape of the answer: find the open seat, refuse if it is not
    /// open, write once, and journal the settlement.
    Respond {
        request_id: AcpHostRequestId,
        answer: Answer,
        reply: Reply<AcpInteractionSettlement>,
    },
    PendingInteractions {
        reply: Reply<Vec<AcpPendingInteraction>>,
    },
    /// Set one of the options this session published.
    SetConfigOption {
        request: v1::SetSessionConfigOptionRequest,
        reply: Reply<Vec<v1::SessionConfigOption>>,
    },
    /// Hold a session at a boundary, or let a held one go.
    ///
    /// One command for both because ego serves them as one: same three fields,
    /// same three answers, and only the method name differs. What a caller
    /// asked for is `release`, and what actually happened is the state that
    /// comes back — which is why the answer is not a boolean.
    Hold {
        release: bool,
        request: EgoHoldRequest,
        reply: Reply<EgoHoldResponse>,
    },
    Compact {
        request: EgoCompactRequest,
        reply: Reply<EgoCompactResponse>,
    },
}

/// What a person decided, in the vocabulary of the seat they decided at.
pub(super) enum Answer {
    Permission(v1::RequestPermissionOutcome),
    Elicitation(v1::ElicitationAction),
}

/// A request that has been written and is waiting for its answer.
///
/// Each variant carries everything needed to finish the job, so the actor holds
/// nothing about a request it has already sent.
pub(super) enum Pending {
    Steer {
        session_id: v1::SessionId,
        turn_id: AcpTurnId,
        queued_turn_id: AcpTurnId,
        outcome: Result<ego_ext::EgoSteerResponse, AcpClientError>,
        reply: Reply<AcpTurnId>,
    },

    Attach {
        /// The session a load or resume holds in `attaching` until it settles.
        claimed: Option<v1::SessionId>,
        outcome: Result<Attached, AcpClientError>,
        authority: AcpSessionAuthority,
        reply: Reply<AcpAttachmentSnapshot>,
    },
    Detach {
        session_id: v1::SessionId,
        outcome: Result<(), AcpClientError>,
        reply: Reply<()>,
    },
    List {
        outcome: Result<v1::ListSessionsResponse, AcpClientError>,
        reply: Reply<v1::ListSessionsResponse>,
    },
    Turn {
        session_id: v1::SessionId,
        turn_id: AcpTurnId,
        outcome: Result<v1::PromptResponse, AcpClientError>,
    },
    Config {
        session_id: v1::SessionId,
        outcome: Result<v1::SetSessionConfigOptionResponse, AcpClientError>,
        reply: Reply<Vec<v1::SessionConfigOption>>,
    },
    Hold {
        session_id: v1::SessionId,
        outcome: Result<EgoHoldResponse, AcpClientError>,
        reply: Reply<EgoHoldResponse>,
    },
    Compact {
        outcome: Result<EgoCompactResponse, AcpClientError>,
        reply: Reply<EgoCompactResponse>,
    },
}

/// What a session request that succeeded says about the session.
pub(super) struct Attached {
    session_id: v1::SessionId,
    config_options: Option<Vec<v1::SessionConfigOption>>,
    profile_warnings: Vec<String>,
}

/// The requests this connection is waiting on, in no particular order.
pub(super) type InFlight = FuturesUnordered<BoxFuture<'static, Pending>>;

struct QueuedContent {
    prompt: Vec<v1::ContentBlock>,
    meta: Option<v1::Meta>,
    steering_pending: bool,
}

type Sent<T> = BoxFuture<'static, Result<T, AcpClientError>>;

/// One request the agent made of this client, and the seat it answers on.
///
/// The SDK responder is held rather than answered inside the dispatch callback
/// that produced it: a person is going to take as long as a person takes, and
/// the dispatch loop cannot read another frame until the callback returns.
pub(super) enum Interaction {
    Permission {
        request: Box<v1::RequestPermissionRequest>,
        responder: Responder<v1::RequestPermissionResponse>,
    },
    Elicitation {
        request: Box<v1::CreateElicitationRequest>,
        responder: Responder<v1::CreateElicitationResponse>,
    },
}

impl Interaction {
    /// The session this request belongs to, as the request itself says.
    ///
    /// An elicitation scoped to a request rather than a session names nothing
    /// this client can seat it against. That scope is for what happens before a
    /// session exists — authentication and configuration — which this client
    /// does not do, so there is no case where guessing an owner would be right.
    fn session_id(&self) -> Option<v1::SessionId> {
        match self {
            Self::Permission { request, .. } => Some(request.session_id.clone()),
            Self::Elicitation { request, .. } => match request.scope() {
                v1::ElicitationScope::Session(scope) => Some(scope.session_id.clone()),
                _ => None,
            },
        }
    }

    /// Whether this is a question this client said it would put to a person.
    ///
    /// `initialize` advertises `elicitation.form` and nothing else, so a `url`
    /// mode — or one from a version this client has never seen — is a question
    /// the agent was told would not be taken. The protocol says a client that
    /// does not understand a mode must not render it as one it does, and
    /// seating is how rendering happens here: the request goes on the
    /// attachment and a frontend draws the form it knows how to draw. A URL
    /// elicitation drawn as a form is worse than a refusal, because the one
    /// thing it is asking — go and look at this address — is what a form has
    /// nowhere to put.
    ///
    /// Permissions have no equivalent: there is one shape of them, and
    /// advertising the capability at all is advertising the whole of it.
    fn advertised(&self) -> bool {
        match self {
            Self::Permission { .. } => true,
            Self::Elicitation { request, .. } => {
                matches!(request.mode, v1::ElicitationMode::Form(_))
            }
        }
    }

    /// The answer that settles this request without granting anything.
    fn refusal(&self) -> Answer {
        match self {
            Self::Permission { .. } => Answer::Permission(v1::RequestPermissionOutcome::Cancelled),
            Self::Elicitation { .. } => Answer::Elicitation(v1::ElicitationAction::Cancel),
        }
    }

    /// Write an answer on the seat it belongs to.
    ///
    /// A write that fails is not reported: the connection is already going, and
    /// there is no one left to tell.
    fn answer(self, answer: Answer) {
        match (self, answer) {
            (Self::Permission { responder, .. }, Answer::Permission(outcome)) => {
                let _ = responder.respond(v1::RequestPermissionResponse::new(outcome));
            }
            (Self::Elicitation { responder, .. }, Answer::Elicitation(action)) => {
                let _ = responder.respond(v1::CreateElicitationResponse::new(action));
            }
            // Unreachable: `respond` checks the pairing before it takes the
            // seat, and every other caller builds the answer from the seat.
            (interaction, _) => interaction.refuse(),
        }
    }

    /// Settle this request with the answer that grants nothing.
    pub(super) fn refuse(self) {
        let refusal = self.refusal();
        self.answer(refusal);
    }

    fn requested(&self, request_id: AcpHostRequestId) -> AcpClientEvent {
        match self {
            Self::Permission { request, .. } => AcpClientEvent::PermissionRequested {
                request_id,
                request: request.clone(),
            },
            Self::Elicitation { request, .. } => AcpClientEvent::ElicitationRequested {
                request_id,
                request: request.clone(),
            },
        }
    }
}

/// Everything the agent sends unbidden, on one channel and in wire order.
///
/// Updates and reverse requests share a channel because they share an order.
/// The SDK hands them to their callbacks one at a time from the same loop, so
/// two channels would preserve each stream's own order and lose the one that
/// matters: whether the agent asked before or after it said something.
pub(super) enum Inbound {
    Update(Box<v1::SessionNotification>),
    Interaction(Box<Interaction>),
}

/// An open seat: what was asked, who it belongs to, and how to answer it.
struct Seat {
    request_id: AcpHostRequestId,
    session_id: v1::SessionId,
    interaction: Interaction,
}

/// Everything one connection knows that is not on the wire.
pub(super) struct ConnectionActor {
    connection_id: AcpConnectionId,
    capabilities: Arc<AcpCapabilitySnapshot>,
    journal: Arc<AcpEventJournal>,
    attachments: HashMap<v1::SessionId, AcpAttachmentSnapshot>,
    /// Pending load/resume sessions and usage received before their reply.
    attaching: HashMap<v1::SessionId, Option<AcpUsageSnapshot>>,
    /// Initial command offers can follow session/new's reply before the actor
    /// settles that reply. Hold at most one session offer per pending attachment
    /// whose id is not yet known, plus offers for explicitly claimed loads.
    initial_commands: HashMap<v1::SessionId, Vec<v1::AvailableCommand>>,
    unidentified_attaches: usize,
    /// Queued wire payloads stay here; snapshots and the journal carry summaries.
    queued_contents: HashMap<AcpTurnId, QueuedContent>,
    /// Open seats in the order the agent asked, which is the order they are
    /// shown in and the order a cancel settles them in. A map keyed by id
    /// would have made that order depend on hashing.
    seats: Vec<Seat>,
    /// Set once, by whichever request first got a denial of something the
    /// snapshot advertised.
    ///
    /// Shared with the futures [`Self::send`] hands to the supervisor, because
    /// that is where an answer actually arrives and it arrives long after the
    /// actor decided to ask. Reading it belongs to the supervisor: the actor
    /// can refuse an operation but cannot end a connection.
    contradicted: Arc<AtomicBool>,
    /// Where a host reads this connection's attachments from.
    ///
    /// A callback rather than the manager's map, because the actor's business
    /// is what the attachments *are* and not where they are kept. What it buys
    /// is the ordering: the actor can publish and only then answer, in one
    /// stretch of its own code, so no caller can be handed a result the
    /// snapshot has not caught up with yet.
    publish: Publish,
}

/// Hand a host the attachments as they now stand.
pub(super) type Publish = Arc<dyn Fn(Vec<AcpAttachmentSnapshot>) + Send + Sync>;

impl ConnectionActor {
    pub(super) fn new(
        connection_id: AcpConnectionId,
        capabilities: Arc<AcpCapabilitySnapshot>,
        journal: Arc<AcpEventJournal>,
        publish: Publish,
    ) -> Self {
        Self {
            connection_id,
            capabilities,
            journal,
            attachments: HashMap::new(),
            attaching: HashMap::new(),
            initial_commands: HashMap::new(),
            unidentified_attaches: 0,
            queued_contents: HashMap::new(),
            seats: Vec::new(),
            contradicted: Arc::new(AtomicBool::new(false)),
            publish,
        }
    }

    /// Show a host what this connection is attached to, as it now stands.
    ///
    /// Called before the answer that made it true is sent, never after: a
    /// caller that reads the snapshot the instant its request returns must not
    /// see the connection as it was before it asked.
    fn publish(&self) {
        (self.publish)(self.attachments());
    }

    /// Whether the agent has denied something it advertised.
    ///
    /// True only after the answer that says so has been handed to the caller
    /// that asked, so a connection ends behind a settled request rather than
    /// instead of one.
    pub(super) fn contradicted(&self) -> bool {
        self.contradicted.load(Ordering::Acquire)
    }

    /// The attachments in a stable order, for the connection snapshot.
    ///
    /// A `HashMap` iterated raw would reorder the list between two reads of an
    /// unchanged connection, which a host would render as sessions jumping
    /// about. Sorting by id is enough to stop that; it also happens to be
    /// creation order against ego, whose session ids are UUIDv7, but the
    /// ordering is chosen for stability and does not depend on that.
    fn attachments(&self) -> Vec<AcpAttachmentSnapshot> {
        let mut attachments: Vec<_> = self.attachments.values().cloned().collect();
        attachments.sort_by(|left, right| left.session_id.0.cmp(&right.session_id.0));
        attachments
    }

    /// Decide one command and write it, in that order.
    ///
    /// Nothing here waits for an answer. What is returned goes into
    /// `in_flight`, and the actor is free to read the next command — which is
    /// what lets a cancel reach a prompt that is still running.
    ///
    /// Most commands change nothing until their answer comes back, but three
    /// change something the moment they are decided: a prompt starts a turn, a
    /// cancel sweeps the seats that turn was waiting on, and an answer takes a
    /// seat away. Those publish here. Without it the connection snapshot would
    /// go on saying a session is idle for as long as a turn takes to produce
    /// its first update — which, for a model thinking, is exactly the stretch
    /// where a host most needs to know otherwise.
    pub(super) fn handle(
        &mut self,
        command: Command,
        connection: &ConnectionTo<Agent>,
        in_flight: &InFlight,
    ) {
        match command {
            Command::NewSession {
                authority,
                profiles,
                reply,
            } => {
                tracing::info!(
                    source = "acp",
                    connection_id = %self.connection_id,
                    method = "session/new",
                    "ACP attach"
                );
                match self.start_new_session(&authority, profiles, connection) {
                    Ok(sent) => {
                        self.unidentified_attaches += 1;
                        in_flight.push(Box::pin(async move {
                            Pending::Attach {
                                claimed: None,
                                outcome: sent.await,
                                authority,
                                reply,
                            }
                        }));
                    }
                    Err(error) => drop(reply.send(Err(error))),
                }
            }
            Command::Attach {
                at_message_id,
                kind,
                session_id,
                authority,
                reply,
            } => match self.start_attach(kind, session_id, &authority, at_message_id, connection) {
                Ok((claimed, sent)) => {
                    if claimed.is_none() {
                        self.unidentified_attaches += 1;
                    }
                    in_flight.push(Box::pin(async move {
                        Pending::Attach {
                            claimed,
                            outcome: sent.await,
                            authority,
                            reply,
                        }
                    }));
                }
                Err(error) => drop(reply.send(Err(error))),
            },
            Command::Detach {
                kind,
                session_id,
                reply,
            } => match self.start_detach(kind, &session_id, connection) {
                Ok(sent) => in_flight.push(Box::pin(async move {
                    Pending::Detach {
                        session_id,
                        outcome: sent.await,
                        reply,
                    }
                })),
                Err(error) => drop(reply.send(Err(error))),
            },
            Command::ListSessions { request, reply } => match self.require(AcpOperation::List) {
                Ok(()) => {
                    let sent = self.send(request, connection, Some(AcpOperation::List));
                    in_flight.push(Box::pin(async move {
                        Pending::List {
                            outcome: sent.await,
                            reply,
                        }
                    }));
                }
                Err(error) => drop(reply.send(Err(error))),
            },
            Command::Prompt {
                session_id,
                prompt,
                meta,
                reply,
            } => {
                if self.capabilities.ego_steer_version == Some(1)
                    && !prompt.is_empty()
                    && prompt.iter().all(|block| matches!(block, v1::ContentBlock::Text(text) if !text.text.trim().is_empty()))
                    && self.attachments.get(&session_id).is_some_and(|attachment| {
                        attachment.state == AcpAttachmentState::Prompting
                            && attachment.queued_prompts.is_empty()
                            && attachment.active_turn.as_ref().is_some_and(|turn| turn.state == AcpTurnState::Running)
                    })
                {
                    let turn_id = self.turn_of(&session_id).expect("running turn checked above");
                    // Reserve the same FIFO position used by normal prompts before awaiting ego.
                    let queued_turn_id = match self.start_prompt(&session_id, prompt.clone(), meta, connection) {
                        Ok((id, _)) => id,
                        Err(error) => {
                            let _ = reply.send(Err(error));
                            return;
                        }
                    };
                    self.queued_contents.get_mut(&queued_turn_id)
                        .expect("prompt queued behind running turn").steering_pending = true;
                    let sent = self.send(ego_ext::EgoSteerWire {
                        v: ego_ext::EGO_EXTENSION_VERSION,
                        session_id: session_id.to_string(),
                        prompt,
                    }, connection, None);
                    let connection_id = self.connection_id;
                    in_flight.push(Box::pin(async move {
                        let outcome = tokio::time::timeout(EGO_STEER_TIMEOUT, sent)
                            .await
                            .unwrap_or_else(|_| Err(AcpClientError::agent_error(
                                connection_id,
                                None,
                                "ego steering timed out; delivery is uncertain and the text was not resent",
                            )));
                        Pending::Steer { session_id, turn_id, queued_turn_id, outcome, reply }
                    }));
                } else {
                    self.handle_prompt(session_id, prompt, meta, reply, connection, in_flight);
                }
            }
            Command::Cancel {
                session_id,
                expected_turn,
                reply,
            } => {
                let _ = reply.send(self.cancel(&session_id, expected_turn, connection));
            }
            Command::CancelQueued {
                session_id,
                turn_id,
                reply,
            } => {
                let _ = reply.send(self.cancel_queued(&session_id, turn_id));
            }
            Command::RecallQueued {
                session_id,
                turn_id,
                reply,
            } => {
                let _ = reply.send(self.recall_queued(&session_id, turn_id));
            }
            Command::Respond {
                request_id,
                answer,
                reply,
            } => {
                let _ = reply.send(self.respond(request_id, answer));
            }
            Command::PendingInteractions { reply } => {
                let _ = reply.send(Ok(self.pending_interactions()));
            }
            Command::SetConfigOption { request, reply } => {
                let session_id = request.session_id.clone();
                match self.start_config(request, connection) {
                    Ok(sent) => in_flight.push(Box::pin(async move {
                        Pending::Config {
                            session_id,
                            outcome: sent.await,
                            reply,
                        }
                    })),
                    Err(error) => drop(reply.send(Err(error))),
                }
            }
            Command::Hold {
                release,
                request,
                reply,
            } => {
                let session_id = request.session_id.clone();
                match self.start_hold(release, &request, connection) {
                    Ok(sent) => in_flight.push(Box::pin(async move {
                        Pending::Hold {
                            session_id,
                            outcome: sent.await,
                            reply,
                        }
                    })),
                    Err(error) => drop(reply.send(Err(error))),
                }
            }
            Command::Compact { request, reply } => match self.start_compact(&request, connection) {
                Ok(sent) => in_flight.push(Box::pin(async move {
                    Pending::Compact {
                        outcome: sent.await,
                        reply,
                    }
                })),
                Err(error) => drop(reply.send(Err(error))),
            },
        }
    }

    /// Set one config option, against the offer this session published.
    ///
    /// Validated here and not only by ego, because the offer is already on the
    /// attachment: a value this session never listed has no meaning to send,
    /// and a round trip to be told so is a round trip that could instead have
    /// been a clear local refusal naming the option.
    fn start_config(
        &mut self,
        request: v1::SetSessionConfigOptionRequest,
        connection: &ConnectionTo<Agent>,
    ) -> Result<Sent<v1::SetSessionConfigOptionResponse>, AcpClientError> {
        let attachment = self.attachment(&request.session_id)?;
        let offered = attachment
            .config_options
            .iter()
            .find(|option| option.id == request.config_id)
            .ok_or_else(|| {
                AcpClientError::not_offered(
                    self.connection_id,
                    request.session_id.clone(),
                    format_args!("a `{}` config option", request.config_id.0),
                )
            })?;

        match (&offered.kind, &request.value) {
            (
                v1::SessionConfigKind::Select(select),
                v1::SessionConfigOptionValue::ValueId { value },
            ) => {
                if !offers_value(select, value) {
                    return Err(AcpClientError::not_offered(
                        self.connection_id,
                        request.session_id.clone(),
                        format_args!("`{}` as a value for `{}`", value.0, request.config_id.0),
                    ));
                }
            }
            // Boolean options are excluded by this client's own contract, not
            // by the agent's: nothing here renders or answers one, so claiming
            // to set it would be claiming a seat that does not exist.
            (v1::SessionConfigKind::Boolean(_), _) => {
                self.require(AcpOperation::ClientBooleanConfig)?;
            }
            (v1::SessionConfigKind::Select(_), v1::SessionConfigOptionValue::Boolean { .. }) => {
                return Err(AcpClientError::invalid_input(format!(
                    "the `{}` config option is a select and takes a value id, not a boolean",
                    request.config_id.0
                )));
            }
            _ => {}
        }

        Ok(self.send(request, connection, None))
    }

    /// Ask ego to hold this session at a boundary, or to let it go.
    fn start_hold(
        &mut self,
        release: bool,
        request: &EgoHoldRequest,
        connection: &ConnectionTo<Agent>,
    ) -> Result<Sent<EgoHoldResponse>, AcpClientError> {
        let operation = if release {
            AcpOperation::ResumeTurn
        } else {
            AcpOperation::Pause
        };
        self.require(operation)?;
        self.attachment(&request.session_id)?;
        let request_id = self.name_request(request.request_id, operation)?;

        let v = ego_ext::EGO_EXTENSION_VERSION;
        let session_id = request.session_id.0.to_string();
        // The const generic is what picks the method name, so the runtime
        // choice is made once, here, rather than by a string passed downwards.
        // Both arms answer with the same type, which is the contract itself:
        // pause and resume differ in what they ask for, not in what they say.
        Ok(if release {
            self.send(
                ego_ext::EgoHoldWire::<true> {
                    v,
                    session_id,
                    request_id,
                },
                connection,
                Some(operation),
            )
        } else {
            self.send(
                ego_ext::EgoHoldWire::<false> {
                    v,
                    session_id,
                    request_id,
                },
                connection,
                Some(operation),
            )
        })
    }

    /// Ask ego to compact this session into a successor.
    fn start_compact(
        &mut self,
        request: &EgoCompactRequest,
        connection: &ConnectionTo<Agent>,
    ) -> Result<Sent<EgoCompactResponse>, AcpClientError> {
        self.require(AcpOperation::Compact)?;
        self.attachment(&request.session_id)?;
        let request_id = self.name_request(request.request_id, AcpOperation::Compact)?;

        Ok(self.send(
            ego_ext::EgoCompactWire {
                v: ego_ext::EGO_EXTENSION_VERSION,
                session_id: request.session_id.0.to_string(),
                request_id,
            },
            connection,
            Some(AcpOperation::Compact),
        ))
    }

    /// The caller's request id, refused here if ego would refuse it.
    ///
    /// A nil UUID is the one value ego rejects outright, and it is also the one
    /// a caller reaches by default-constructing rather than deciding. Catching
    /// it before the write keeps the failure at the caller instead of arriving
    /// as an opaque `invalid_params` a round trip later.
    fn name_request(
        &self,
        request_id: uuid::Uuid,
        operation: AcpOperation,
    ) -> Result<String, AcpClientError> {
        if request_id.is_nil() {
            return Err(AcpClientError::invalid_input(format!(
                "{operation:?} needs a request id that names one attempt, not a nil UUID"
            )));
        }
        Ok(request_id.to_string())
    }

    /// Take in one request the agent is waiting on a person for.
    ///
    /// A request this client has no seat for is answered here and now rather
    /// than held: every seat it has belongs to an attachment, so an elicitation
    /// scoped to something else, or a permission for a session it let go, is one
    /// no person can ever be shown. Holding it would leave the agent waiting on
    /// a human who does not exist.
    ///
    /// A request in a mode this client never advertised gets the same answer,
    /// for a different reason and with the same honesty: see
    /// [`Interaction::advertised`]. Either way the agent is told at once, in the
    /// vocabulary it asked in.
    fn seat(&mut self, interaction: Interaction) {
        let request_id = AcpHostRequestId::new();
        let session_id = match interaction.session_id() {
            Some(session_id)
                if interaction.advertised() && self.attachments.contains_key(&session_id) =>
            {
                session_id
            }
            unseatable => {
                // Both halves are recorded even though no one could have acted
                // on them: a host reading the stream is owed the fact that the
                // agent asked, and that the answer it got was nobody's.
                self.journal
                    .append(unseatable.clone(), None, interaction.requested(request_id));
                let refusal = interaction.refusal();
                let event = Self::settled(request_id, &refusal);
                interaction.answer(refusal);
                self.journal.append(unseatable, None, event);
                return;
            }
        };

        let turn_id = self.turn_of(&session_id);
        let asked = interaction.requested(request_id);
        self.seats.push(Seat {
            request_id,
            session_id: session_id.clone(),
            interaction,
        });
        self.republish(&session_id);
        self.publish();
        self.journal.append(Some(session_id), turn_id, asked);
    }

    /// Answer one open seat, once.
    fn respond(
        &mut self,
        request_id: AcpHostRequestId,
        answer: Answer,
    ) -> Result<AcpInteractionSettlement, AcpClientError> {
        let index = self
            .seats
            .iter()
            .position(|seat| seat.request_id == request_id)
            .ok_or_else(|| AcpClientError::interaction_settled(self.connection_id, request_id))?;
        // Validated before the seat is taken, so a refused answer leaves the
        // seat open for a valid one instead of stranding the agent.
        self.validate(&self.seats[index], &answer)?;

        let seat = self.seats.remove(index);
        let event = Self::settled(request_id, &answer);
        let turn_id = self.turn_of(&seat.session_id);
        seat.interaction.answer(answer);
        self.republish(&seat.session_id);
        self.publish();
        self.journal
            .append(Some(seat.session_id.clone()), turn_id, event);
        Ok(AcpInteractionSettlement { request_id })
    }

    /// Settle every seat one session is waiting on, in the order they were
    /// asked, with the answer that grants nothing.
    fn sweep(&mut self, session_id: &v1::SessionId) {
        let mut swept = Vec::new();
        let mut index = 0;
        while index < self.seats.len() {
            if &self.seats[index].session_id == session_id {
                swept.push(self.seats.remove(index));
            } else {
                index += 1;
            }
        }
        self.republish(session_id);
        self.publish();
        for seat in swept {
            let refusal = seat.interaction.refusal();
            let event = Self::settled(seat.request_id, &refusal);
            let turn_id = self.turn_of(&seat.session_id);
            seat.interaction.answer(refusal);
            self.journal
                .append(Some(seat.session_id.clone()), turn_id, event);
        }
    }

    fn pending_interactions(&self) -> Vec<AcpPendingInteraction> {
        self.seats
            .iter()
            .map(|seat| match &seat.interaction {
                Interaction::Permission { request, .. } => AcpPendingInteraction::Permission {
                    request_id: seat.request_id,
                    session_id: seat.session_id.clone(),
                    request: request.clone(),
                },
                Interaction::Elicitation { request, .. } => AcpPendingInteraction::Elicitation {
                    request_id: seat.request_id,
                    session_id: seat.session_id.clone(),
                    request: request.clone(),
                },
            })
            .collect()
    }

    /// Whether this answer can be given at this seat.
    fn validate(&self, seat: &Seat, answer: &Answer) -> Result<(), AcpClientError> {
        match (&seat.interaction, answer) {
            (Interaction::Permission { request, .. }, Answer::Permission(outcome)) => {
                let v1::RequestPermissionOutcome::Selected(selected) = outcome else {
                    return Ok(());
                };
                if request
                    .options
                    .iter()
                    .any(|option| option.option_id == selected.option_id)
                {
                    return Ok(());
                }
                Err(AcpClientError::unoffered_option(
                    self.connection_id,
                    seat.session_id.clone(),
                    &selected.option_id,
                ))
            }
            (Interaction::Elicitation { .. }, Answer::Elicitation(action)) => {
                let v1::ElicitationAction::Other(other) = action else {
                    return Ok(());
                };
                Err(AcpClientError::undefined_action(
                    self.connection_id,
                    seat.session_id.clone(),
                    &other.action,
                ))
            }
            _ => Err(AcpClientError::invalid_input(
                "this answer does not belong to the request it names",
            )),
        }
    }

    fn settled(request_id: AcpHostRequestId, answer: &Answer) -> AcpClientEvent {
        match answer {
            Answer::Permission(outcome) => AcpClientEvent::PermissionSettled {
                request_id,
                outcome: outcome.clone(),
            },
            Answer::Elicitation(action) => AcpClientEvent::ElicitationSettled {
                request_id,
                action: action.clone(),
            },
        }
    }

    /// The turn a session is running, if it is running one.
    fn turn_of(&self, session_id: &v1::SessionId) -> Option<AcpTurnId> {
        self.attachments
            .get(session_id)?
            .active_turn
            .as_ref()
            .map(|turn| turn.turn_id)
    }

    /// Copy the open seats back onto the attachment a host reads.
    fn republish(&mut self, session_id: &v1::SessionId) {
        let permissions: Vec<_> = self
            .seats
            .iter()
            .filter(|seat| {
                &seat.session_id == session_id
                    && matches!(seat.interaction, Interaction::Permission { .. })
            })
            .map(|seat| seat.request_id)
            .collect();
        let elicitations: Vec<_> = self
            .seats
            .iter()
            .filter(|seat| {
                &seat.session_id == session_id
                    && matches!(seat.interaction, Interaction::Elicitation { .. })
            })
            .map(|seat| seat.request_id)
            .collect();
        if let Some(attachment) = self.attachments.get_mut(session_id) {
            attachment.pending_permission_ids = permissions;
            attachment.pending_elicitation_ids = elicitations;
        }
    }

    /// Take in one answer we asked for, or one piece of news we did not.
    pub(super) fn accept(
        &mut self,
        accepted: Accepted,
        connection: &ConnectionTo<Agent>,
        in_flight: &InFlight,
    ) {
        match accepted {
            Accepted::Settled(pending) => self.settle(*pending, connection, in_flight),
            Accepted::Inbound(inbound) => match *inbound {
                Inbound::Update(notification) => self.project(*notification),
                Inbound::Interaction(interaction) => self.seat(*interaction),
            },
        }
    }

    fn settle(&mut self, pending: Pending, connection: &ConnectionTo<Agent>, in_flight: &InFlight) {
        match pending {
            Pending::Steer {
                session_id,
                turn_id,
                queued_turn_id,
                outcome,
                reply,
            } => {
                if let Some(content) = self.queued_contents.get_mut(&queued_turn_id) {
                    content.steering_pending = false;
                }
                match outcome {
                    Ok(response) if response.v == ego_ext::EGO_EXTENSION_VERSION => {
                        match response.state {
                            ego_ext::EgoSteerState::Accepted => {
                                let _ = self.cancel_queued(&session_id, queued_turn_id);
                                let _ = reply.send(Ok(turn_id));
                            }
                            ego_ext::EgoSteerState::NotBusy | ego_ext::EgoSteerState::Rejected => {
                                let result = self.attachment(&session_id).map(|_| queued_turn_id);
                                let _ = reply.send(result);
                            }
                        }
                    }
                    Ok(_) => {
                        let _ = self.cancel_queued(&session_id, queued_turn_id);
                        let _ = reply.send(Err(AcpClientError::invalid_input(
                            "unsupported ego steering response version; acceptance is unknown",
                        )));
                    }
                    // An uncertain transport outcome must never resend possibly accepted text.
                    Err(error) => {
                        let _ = self.cancel_queued(&session_id, queued_turn_id);
                        let _ = reply.send(Err(error));
                    }
                }
                self.drain_next(&session_id, connection, in_flight);
            }

            Pending::Attach {
                claimed,
                outcome,
                authority,
                reply,
            } => {
                if claimed.is_none() {
                    self.unidentified_attaches -= 1;
                }
                let usage = claimed
                    .as_ref()
                    .and_then(|id| self.attaching.remove(id))
                    .flatten();
                if outcome.is_err()
                    && let Some(id) = claimed
                {
                    self.initial_commands.remove(&id);
                }
                let outcome = outcome.map(|attached| self.record(attached, authority, usage));
                // No unidentified reply can claim the remaining offers now.
                if self.unidentified_attaches == 0 {
                    self.initial_commands
                        .retain(|id, _| self.attaching.contains_key(id));
                }
                let _ = reply.send(outcome);
            }
            Pending::Detach {
                session_id,
                outcome,
                reply,
            } => {
                // A refused detach keeps the attachment on purpose: the agent
                // still has the session, and forgetting it here would leave a
                // live session nothing in this client can reach.
                if outcome.is_ok()
                    && let Some(removed) = self.attachments.remove(&session_id)
                {
                    for queued in removed.queued_prompts {
                        self.queued_contents.remove(&queued.turn_id);
                    }
                    self.publish();
                    self.journal.append(
                        Some(session_id),
                        None,
                        AcpClientEvent::AttachmentState {
                            state: AcpAttachmentState::Detached,
                        },
                    );
                }
                let _ = reply.send(outcome);
            }
            Pending::List { outcome, reply } => drop(reply.send(outcome)),
            Pending::Turn {
                session_id,
                turn_id,
                outcome,
            } => {
                if self.settle_turn(&session_id, turn_id, outcome) {
                    self.drain_next(&session_id, connection, in_flight);
                }
            }
            Pending::Config {
                session_id,
                outcome,
                reply,
            } => {
                // The response carries the full set, so the attachment takes
                // all of it rather than patching the one option that was set.
                // The agent is the authority on what setting one did to the
                // others, and a client that merged would be guessing.
                let outcome = outcome.map(|response| response.config_options);
                if let (Ok(options), Some(attachment)) =
                    (&outcome, self.attachments.get_mut(&session_id))
                {
                    attachment.config_options = options.clone();
                    self.publish();
                }
                let _ = reply.send(outcome);
            }
            Pending::Hold {
                session_id,
                outcome,
                reply,
            } => {
                self.settle_hold(&session_id, outcome, reply);
                self.drain_next(&session_id, connection, in_flight);
            }
            // Nothing on this attachment changes. The successor is a different
            // session that this connection is not attached to, and attaching to
            // it is a separate decision a caller makes.
            Pending::Compact { outcome, reply } => drop(reply.send(outcome)),
        }
    }

    /// Record what the agent said about a session, in the order it said it.
    ///
    /// An update for a session this connection is not attached to is dropped
    /// rather than attached to whichever session happens to be there: it
    /// belongs to a session that was closed or was never ours, and guessing an
    /// owner would put one turn's output into another turn's transcript.
    fn project(&mut self, notification: v1::SessionNotification) {
        let Some(attachment) = self.attachments.get_mut(&notification.session_id) else {
            // Metadata only: this does not admit an unknown session's output.
            // The successful reply must still name this exact id before use.
            if let v1::SessionUpdate::AvailableCommandsUpdate(offer) = &notification.update {
                let claimed = self.attaching.contains_key(&notification.session_id);
                let has_room =
                    self.initial_commands.len() < self.attaching.len() + self.unidentified_attaches;
                if claimed
                    || self.initial_commands.contains_key(&notification.session_id)
                    || (self.unidentified_attaches > 0 && has_room)
                {
                    self.initial_commands.insert(
                        notification.session_id.clone(),
                        offer.available_commands.clone(),
                    );
                }
            }
            // Ego sends replay chunks before answering session/load. This id is
            // known from the request even though its attachment is not yet
            // recorded; other unknown session ids must still be ignored.
            if let Some(usage) = self.attaching.get_mut(&notification.session_id) {
                if let v1::SessionUpdate::UsageUpdate(context) = &notification.update {
                    *usage = Some(AcpUsageSnapshot {
                        context: Some(context.clone()),
                        end_turn: None,
                    });
                }
                self.journal.append(
                    Some(notification.session_id),
                    None,
                    AcpClientEvent::SessionUpdate {
                        update: Box::new(notification.update),
                    },
                );
            }
            return;
        };
        let turn_id = attachment.active_turn.as_ref().map(|turn| turn.turn_id);
        let changed = match &notification.update {
            v1::SessionUpdate::UsageUpdate(usage) => {
                attachment.usage = Some(AcpUsageSnapshot {
                    context: Some(usage.clone()),
                    end_turn: attachment
                        .usage
                        .as_ref()
                        .and_then(|held| held.end_turn.clone()),
                });
                true
            }
            v1::SessionUpdate::AvailableCommandsUpdate(offer) => {
                attachment.available_commands = offer.available_commands.clone();
                true
            }
            v1::SessionUpdate::ConfigOptionUpdate(options) => {
                attachment.config_options = options.config_options.clone();
                true
            }
            _ => false,
        };
        if changed {
            self.publish();
        }
        self.journal.append(
            Some(notification.session_id),
            turn_id,
            AcpClientEvent::SessionUpdate {
                update: Box::new(notification.update),
            },
        );
    }

    fn start_new_session(
        &self,
        authority: &AcpSessionAuthority,
        profiles: Option<serde_json::Value>,
        connection: &ConnectionTo<Agent>,
    ) -> Result<Sent<Attached>, AcpClientError> {
        self.require_authority(authority)?;
        let mut request = v1::NewSessionRequest::new(authority.cwd.clone());
        request
            .additional_directories
            .clone_from(&authority.additional_directories);
        request.mcp_servers.clone_from(&authority.mcp_servers);
        if let Some(profiles) = &profiles {
            request.meta = Some(serde_json::Map::from_iter([(
                "ego".to_owned(),
                profiles.clone(),
            )]));
        }
        let sent = self.send(request, connection, None);
        Ok(Box::pin(async move {
            let response = sent.await?;
            let mut profile_warnings = Vec::new();
            if let Some(profiles) = profiles {
                let meta = response.meta.as_ref().and_then(|meta| meta.get("ego"));
                let refusal = || {
                    AcpClientError::invalid_input(
                        "ego did not acknowledge the repository profile ceiling; update ego before opening this session",
                    )
                };
                let meta = meta.ok_or_else(refusal)?;
                if meta.get("profile") != profiles.get("profile")
                    || meta.get("ceilingProfile") != profiles.get("ceilingProfile")
                    || !meta
                        .get("effective")
                        .is_some_and(serde_json::Value::is_object)
                {
                    return Err(refusal());
                }
                profile_warnings =
                    serde_json::from_value(meta.get("warnings").cloned().ok_or_else(refusal)?)
                        .map_err(|_| refusal())?;
            }
            Ok(Attached {
                session_id: response.session_id,
                config_options: response.config_options,
                profile_warnings,
            })
        }))
    }

    /// Write the request that attaches to a session ego already owns.
    ///
    /// The three kinds differ in the method and in whether the answer names a
    /// new id; the gate, the request body and the resulting attachment are
    /// shared, so they are written once. Fork is the one that comes back with
    /// an id the caller did not name, because a fork is a second session.
    fn start_attach(
        &mut self,
        kind: AcpAttachKind,
        session_id: v1::SessionId,
        authority: &AcpSessionAuthority,
        at_message_id: Option<String>,
        connection: &ConnectionTo<Agent>,
    ) -> Result<(Option<v1::SessionId>, Sent<Attached>), AcpClientError> {
        tracing::info!(
            source = "acp",
            connection_id = %self.connection_id,
            method = kind.method(),
            session_id = %session_id,
            "ACP attach"
        );
        let operation = kind.operation();
        self.require(operation)?;
        self.require_authority(authority)?;
        if at_message_id.is_some() && !self.capabilities.fork_at_message {
            return Err(AcpClientError::invalid_input(
                "agent does not support fork at a message",
            ));
        }
        // Load and resume name the session they attach to; a fork names the one
        // it forks *from* and comes back with an id of its own, so only these
        // two can land on an attachment this connection already holds. Landing
        // on one would overwrite the running turn, the usage and the open
        // interaction ids with the empty state of a fresh attachment — while
        // the seats themselves survive, so the two would then disagree. One
        // still unanswered counts too: each load makes ego admit every MCP
        // server again, and a host that asked twice would pay for both.
        let claimed = if matches!(kind, AcpAttachKind::Fork) {
            None
        } else {
            if self.attachments.contains_key(&session_id)
                || self.attaching.contains_key(&session_id)
            {
                return Err(AcpClientError::already_attached(
                    self.connection_id,
                    session_id,
                ));
            }
            self.attaching.insert(session_id.clone(), None);
            Some(session_id.clone())
        };
        let operation = Some(operation);
        let cwd = authority.cwd.clone();
        let roots = authority.additional_directories.clone();
        let servers = authority.mcp_servers.clone();

        let sent: Sent<Attached> = match kind {
            AcpAttachKind::Load => {
                let mut request = v1::LoadSessionRequest::new(session_id.clone(), cwd);
                request.additional_directories = roots;
                request.mcp_servers = servers;
                let sent = self.send(request, connection, operation);
                Box::pin(async move {
                    sent.await.map(|response| Attached {
                        session_id,
                        config_options: response.config_options,
                        profile_warnings: Vec::new(),
                    })
                })
            }
            AcpAttachKind::Fork => {
                let mut request = v1::ForkSessionRequest::new(session_id, cwd);
                if let Some(message_id) = at_message_id {
                    request.meta = Some(serde_json::Map::from_iter([(
                        "ego".to_owned(),
                        serde_json::json!({ "atMessageId": message_id }),
                    )]));
                }
                request.additional_directories = roots;
                request.mcp_servers = servers;
                let sent = self.send(request, connection, operation);
                Box::pin(async move {
                    sent.await.map(|response| Attached {
                        session_id: response.session_id,
                        config_options: response.config_options,
                        profile_warnings: Vec::new(),
                    })
                })
            }
            AcpAttachKind::Resume => {
                let mut request = v1::ResumeSessionRequest::new(session_id.clone(), cwd);
                request.additional_directories = roots;
                request.mcp_servers = servers;
                let sent = self.send(request, connection, operation);
                Box::pin(async move {
                    sent.await.map(|response| Attached {
                        session_id,
                        config_options: response.config_options,
                        profile_warnings: Vec::new(),
                    })
                })
            }
        };
        Ok((claimed, sent))
    }

    fn start_detach(
        &self,
        kind: AcpDetachKind,
        session_id: &v1::SessionId,
        connection: &ConnectionTo<Agent>,
    ) -> Result<Sent<()>, AcpClientError> {
        let operation = kind.operation();
        self.require(operation)?;
        let operation = Some(operation);
        Ok(match kind {
            AcpDetachKind::Close => {
                let request = v1::CloseSessionRequest::new(session_id.clone());
                let sent = self.send(request, connection, operation);
                Box::pin(async move { sent.await.map(drop) })
            }
            AcpDetachKind::Delete => {
                let request = v1::DeleteSessionRequest::new(session_id.clone());
                let sent = self.send(request, connection, operation);
                Box::pin(async move { sent.await.map(drop) })
            }
        })
    }

    fn handle_prompt(
        &mut self,
        session_id: v1::SessionId,
        prompt: Vec<v1::ContentBlock>,
        meta: Option<v1::Meta>,
        reply: Reply<AcpTurnId>,
        connection: &ConnectionTo<Agent>,
        in_flight: &InFlight,
    ) {
        match self.start_prompt(&session_id, prompt, meta, connection) {
            Ok((turn_id, sent)) => {
                let _ = reply.send(Ok(turn_id));
                if let Some(sent) = sent {
                    in_flight.push(Box::pin(async move {
                        Pending::Turn {
                            session_id,
                            turn_id,
                            outcome: sent.await,
                        }
                    }));
                }
            }
            Err(error) => drop(reply.send(Err(error))),
        }
    }

    /// Accept a turn, queueing it when the session already has one in flight.
    ///
    /// The turn that settled is still held, because a host that has just been
    /// told a turn ended still has to be able to read how it ended. Holding it
    /// is not the same as it being in the way, so what blocks a new prompt is
    /// the turn's state and not its mere presence.
    fn start_prompt(
        &mut self,
        session_id: &v1::SessionId,
        prompt: Vec<v1::ContentBlock>,
        meta: Option<v1::Meta>,
        connection: &ConnectionTo<Agent>,
    ) -> Result<(AcpTurnId, Option<Sent<v1::PromptResponse>>), AcpClientError> {
        let attachment = self.attachment(session_id)?;
        for block in &prompt {
            if let Some(operation) = content_operation(block) {
                self.require(operation)?;
            }
        }

        let turn_id = AcpTurnId::new();
        if attachment.state != AcpAttachmentState::Idle
            || !attachment.queued_prompts.is_empty()
            || attachment
                .active_turn
                .as_ref()
                .is_some_and(|turn| turn.state != AcpTurnState::Settled)
        {
            let attachment = self
                .attachments
                .get_mut(session_id)
                .expect("checked just above");
            if attachment.queued_prompts.len() >= 32 {
                return Err(AcpClientError::invalid_input("ACP prompt queue is full"));
            }
            attachment.queued_prompts.push(AcpQueuedPrompt {
                turn_id,
                summary: prompt_display(&prompt, 200),
            });
            self.queued_contents.insert(
                turn_id,
                QueuedContent {
                    prompt,
                    meta,
                    steering_pending: false,
                },
            );
            let queued_prompts = attachment.queued_prompts.clone();
            self.publish();
            self.journal.append(
                Some(session_id.clone()),
                Some(turn_id),
                AcpClientEvent::PromptQueueChanged { queued_prompts },
            );
            return Ok((turn_id, None));
        }
        Ok((
            turn_id,
            Some(self.send_prompt(session_id, turn_id, prompt, meta, connection)),
        ))
    }

    fn send_prompt(
        &mut self,
        session_id: &v1::SessionId,
        turn_id: AcpTurnId,
        prompt: Vec<v1::ContentBlock>,
        meta: Option<v1::Meta>,
        connection: &ConnectionTo<Agent>,
    ) -> Sent<v1::PromptResponse> {
        let text = prompt_display(&prompt, usize::MAX);
        let request = v1::PromptRequest::new(session_id.clone(), prompt).meta(meta);
        let sent = self.send(request, connection, None);

        let attachment = self
            .attachments
            .get_mut(session_id)
            .expect("checked just above");
        attachment.state = AcpAttachmentState::Prompting;
        attachment.active_turn = Some(AcpTurnSnapshot {
            turn_id,
            state: AcpTurnState::Running,
            stop_reason: None,
            usage: None,
        });
        self.publish();
        self.journal.append(
            Some(session_id.clone()),
            Some(turn_id),
            AcpClientEvent::PromptSent { text },
        );
        self.journal.append(
            Some(session_id.clone()),
            Some(turn_id),
            AcpClientEvent::TurnStarted,
        );
        sent
    }

    /// Send the next accepted prompt only when ego can take a turn again.
    fn drain_next(
        &mut self,
        session_id: &v1::SessionId,
        connection: &ConnectionTo<Agent>,
        in_flight: &InFlight,
    ) {
        loop {
            let Some(attachment) = self.attachments.get_mut(session_id) else {
                return;
            };
            if attachment.state != AcpAttachmentState::Idle {
                return;
            }
            let Some(queued) = attachment.queued_prompts.first().cloned() else {
                return;
            };
            if self
                .queued_contents
                .get(&queued.turn_id)
                .is_some_and(|content| content.steering_pending)
            {
                return;
            }
            attachment.queued_prompts.remove(0);
            let queued_prompts = attachment.queued_prompts.clone();
            self.publish();
            self.journal.append(
                Some(session_id.clone()),
                Some(queued.turn_id),
                AcpClientEvent::PromptQueueChanged { queued_prompts },
            );
            let Some(content) = self.queued_contents.remove(&queued.turn_id) else {
                tracing::warn!(turn_id = ?queued.turn_id, "ACP queued prompt payload missing");
                continue;
            };
            let sent = self.send_prompt(
                session_id,
                queued.turn_id,
                content.prompt,
                content.meta,
                connection,
            );
            let session_id = session_id.clone();
            in_flight.push(Box::pin(async move {
                Pending::Turn {
                    session_id,
                    turn_id: queued.turn_id,
                    outcome: sent.await,
                }
            }));
            return;
        }
    }

    /// Ask for the running turn to stop, exactly once.
    ///
    /// Cancelling does not settle the turn — only the prompt response does, and
    /// the expected one is `StopReason::Cancelled`. A client that settled here
    /// would drop the updates the agent is still entitled to send before it
    /// answers, and would be wrong on the race where the turn had already
    /// finished.
    fn cancel(
        &mut self,
        session_id: &v1::SessionId,
        expected_turn: Option<AcpTurnId>,
        connection: &ConnectionTo<Agent>,
    ) -> Result<(), AcpClientError> {
        let attachment = self.attachment(session_id)?;
        let Some(turn) = attachment.active_turn.as_ref() else {
            return Err(AcpClientError::no_active_turn(
                self.connection_id,
                session_id.clone(),
            ));
        };
        if expected_turn.is_some_and(|expected| turn.turn_id != expected) {
            return Err(AcpClientError::no_active_turn(
                self.connection_id,
                session_id.clone(),
            ));
        }
        if turn.state != AcpTurnState::Running {
            return Ok(());
        }
        let turn_id = turn.turn_id;

        // Before the agent is told to stop, and not after: every seat this
        // session is waiting on is answered with the outcome that grants
        // nothing. A person is not going to answer a question belonging to a
        // turn that is being cancelled, and an unanswered request would keep
        // the agent waiting on the very turn it was asked to abandon.
        self.sweep(session_id);

        // A notification carries no answer, so the only way this fails is the
        // outgoing side of the connection being gone — the agent never saw it.
        // Calling that an agent error would tell a caller its cancel was
        // considered and refused, when nothing was there to consider it.
        connection
            .send_notification(v1::CancelNotification::new(session_id.clone()))
            .map_err(|_| AcpClientError::transport_closed(self.connection_id))?;

        let attachment = self
            .attachments
            .get_mut(session_id)
            .expect("checked just above");
        attachment.state = AcpAttachmentState::Cancelling;
        if let Some(turn) = attachment.active_turn.as_mut() {
            turn.state = AcpTurnState::Cancelling;
        }
        self.publish();
        self.journal.append(
            Some(session_id.clone()),
            Some(turn_id),
            AcpClientEvent::AttachmentState {
                state: AcpAttachmentState::Cancelling,
            },
        );
        Ok(())
    }

    fn recall_queued(
        &mut self,
        session_id: &v1::SessionId,
        turn_id: AcpTurnId,
    ) -> Result<String, AcpClientError> {
        let content = self
            .queued_contents
            .get(&turn_id)
            .ok_or_else(|| AcpClientError::invalid_input("queued prompt not found"))?;
        let mut parts = Vec::new();
        for block in &content.prompt {
            let v1::ContentBlock::Text(text) = block else {
                // DEFERRED (2026-10-31) — the composer needs a lossless multimodal
                // draft contract before attachment-bearing prompts can be recalled.
                return Err(AcpClientError::invalid_input(
                    "only text-only queued prompts can be recalled",
                ));
            };
            parts.push(text.text.clone());
        }
        let text = parts.join("\n\n");
        self.cancel_queued(session_id, turn_id)?;
        Ok(text)
    }

    fn cancel_queued(
        &mut self,
        session_id: &v1::SessionId,
        turn_id: AcpTurnId,
    ) -> Result<(), AcpClientError> {
        let attachment = self
            .attachments
            .get_mut(session_id)
            .ok_or_else(|| AcpClientError::not_attached(self.connection_id, session_id.clone()))?;
        let index = attachment
            .queued_prompts
            .iter()
            .position(|queued| queued.turn_id == turn_id)
            .ok_or_else(|| AcpClientError::invalid_input("queued prompt not found"))?;
        if self
            .queued_contents
            .get(&turn_id)
            .is_some_and(|content| content.steering_pending)
        {
            return Err(AcpClientError::invalid_input(
                "steering acceptance is still pending",
            ));
        }
        attachment.queued_prompts.remove(index);
        self.queued_contents.remove(&turn_id);
        let queued_prompts = attachment.queued_prompts.clone();
        self.publish();
        self.journal.append(
            Some(session_id.clone()),
            Some(turn_id),
            AcpClientEvent::PromptQueueChanged { queued_prompts },
        );
        Ok(())
    }

    /// Record what a hold request found, and tell the caller the same thing.
    ///
    /// The state ego reports is the whole answer, and it is recorded whichever
    /// method asked for it: a pause that came back `running` because the turn
    /// had already ended, and a resume that came back `pending` because the
    /// hold has not landed yet, are both facts a host has to see.
    fn settle_hold(
        &mut self,
        session_id: &v1::SessionId,
        outcome: Result<EgoHoldResponse, AcpClientError>,
        reply: Reply<EgoHoldResponse>,
    ) {
        let Ok(response) = &outcome else {
            let _ = reply.send(outcome);
            return;
        };
        let Some(attachment) = self.attachments.get_mut(session_id) else {
            let _ = reply.send(outcome);
            return;
        };

        let state = match response.state {
            AcpHoldState::Pending => AcpAttachmentState::PausePending,
            AcpHoldState::Paused => AcpAttachmentState::Paused,
            // Not holding says nothing about whether a turn is running; the
            // turn's own state is what decides, so the attachment goes back to
            // whichever of the two it was actually in.
            AcpHoldState::Running => match attachment.active_turn.as_ref() {
                Some(turn) if turn.state != AcpTurnState::Settled => AcpAttachmentState::Prompting,
                _ => AcpAttachmentState::Idle,
            },
        };
        let changed = attachment.state != state;
        attachment.state = state;
        let turn_id = attachment.active_turn.as_ref().map(|turn| turn.turn_id);
        if changed {
            self.publish();
            self.journal.append(
                Some(session_id.clone()),
                turn_id,
                AcpClientEvent::AttachmentState { state },
            );
        }
        let _ = reply.send(outcome);
    }

    /// Close out a turn on the answer that actually settles it.
    fn settle_turn(
        &mut self,
        session_id: &v1::SessionId,
        turn_id: AcpTurnId,
        outcome: Result<v1::PromptResponse, AcpClientError>,
    ) -> bool {
        let Some(attachment) = self.attachments.get_mut(session_id) else {
            return false;
        };
        // A response for a turn that is no longer the active one belongs to a
        // turn that was already settled; applying it would rewrite the outcome
        // of whatever is running now.
        if attachment.active_turn.as_ref().map(|turn| turn.turn_id) != Some(turn_id) {
            return false;
        }

        attachment.state = AcpAttachmentState::Idle;
        let event = match outcome {
            Ok(response) => {
                attachment.active_turn = Some(AcpTurnSnapshot {
                    turn_id,
                    state: AcpTurnState::Settled,
                    stop_reason: Some(response.stop_reason),
                    usage: response.usage.clone(),
                });
                // Create the snapshot rather than only updating one that exists.
                // `PromptResponse.usage` is independent of `UsageUpdate`, so
                // requiring a held snapshot dropped every figure from an agent
                // that reports usage on the response alone. It survived in
                // `active_turn` until the next turn overwrote it, which is
                // exactly long enough to look like it was kept.
                if let Some(usage) = &response.usage {
                    match attachment.usage.as_mut() {
                        Some(held) => held.end_turn = Some(usage.clone()),
                        None => {
                            attachment.usage = Some(AcpUsageSnapshot {
                                context: None,
                                end_turn: Some(usage.clone()),
                            })
                        }
                    }
                }
                AcpClientEvent::TurnSettled {
                    stop_reason: response.stop_reason,
                    usage: response.usage,
                }
            }
            // A failed prompt has no stop reason, but the agent's diagnostic
            // must reach every subscriber before the composer returns to idle.
            Err(error) => {
                attachment.active_turn = None;
                // ego's dedicated pause code means the hold reached its
                // boundary. The message is explanatory, not a state signal.
                if error.agent_code == Some(-32011) {
                    attachment.state = AcpAttachmentState::Paused;
                }
                AcpClientEvent::TurnFailed {
                    message: error.message,
                    state: attachment.state,
                }
            }
        };
        self.publish();
        self.journal
            .append(Some(session_id.clone()), Some(turn_id), event);
        true
    }

    /// Remember what this connection is now attached to.
    fn record(
        &mut self,
        attached: Attached,
        authority: AcpSessionAuthority,
        usage: Option<AcpUsageSnapshot>,
    ) -> AcpAttachmentSnapshot {
        let attachment = AcpAttachmentSnapshot {
            available_commands: self
                .initial_commands
                .remove(&attached.session_id)
                .unwrap_or_default(),
            session_id: attached.session_id,
            state: AcpAttachmentState::Idle,
            cwd: authority.cwd,
            additional_directories: authority.additional_directories,
            config_options: attached.config_options.unwrap_or_default(),
            profile_warnings: attached.profile_warnings,
            usage,
            active_turn: None,
            queued_prompts: Vec::new(),
            pending_permission_ids: Vec::new(),
            pending_elicitation_ids: Vec::new(),
        };
        self.attachments
            .insert(attachment.session_id.clone(), attachment.clone());
        self.publish();
        self.journal.append(
            Some(attachment.session_id.clone()),
            None,
            AcpClientEvent::AttachmentState {
                state: AcpAttachmentState::Idle,
            },
        );
        attachment
    }

    fn attachment(
        &self,
        session_id: &v1::SessionId,
    ) -> Result<&AcpAttachmentSnapshot, AcpClientError> {
        self.attachments
            .get(session_id)
            .ok_or_else(|| AcpClientError::not_attached(self.connection_id, session_id.clone()))
    }

    /// Refuse an operation this connection's peer never advertised.
    fn require(&self, operation: AcpOperation) -> Result<(), AcpClientError> {
        let availability = self.capabilities.availability(operation);
        if availability.available {
            return Ok(());
        }
        Err(AcpClientError::capability_unavailable(
            self.connection_id,
            operation,
            availability.reason,
        ))
    }

    /// Refuse extra roots, and MCP servers on a transport, an agent never said
    /// it honours.
    ///
    /// The session request carrying them is baseline, which is the danger: an
    /// agent that does not know the field answers with a session anyway, and
    /// the caller is left believing it spans directories the agent will never
    /// touch. `mcp_servers` rides the same baseline requests and had no gate at
    /// all, so a caller could hand an agent with `mcpCapabilities.http == false`
    /// a set of HTTP tools, get a session back, and never be told the agent has
    /// none of them.
    ///
    /// Stdio is the one transport with nothing to check on the agent's side: v1
    /// publishes no `mcpCapabilities.stdio`, so it is baseline and the gate
    /// reads what this client carries instead. That is safe only because the
    /// list is replaced with our own entry before it reaches here — a stdio
    /// server is a command line the agent runs, so the property that matters is
    /// whose, not which transport.
    fn require_authority(&self, authority: &AcpSessionAuthority) -> Result<(), AcpClientError> {
        if !authority.additional_directories.is_empty() {
            self.require(AcpOperation::AdditionalDirectories)?;
        }
        for server in &authority.mcp_servers {
            let operation = mcp_server_operation(server).ok_or_else(|| {
                AcpClientError::invalid_input(
                    "MCP server uses a transport this build does not know, so no capability \
                     covers it",
                )
            })?;
            self.require(operation)?;
        }
        Ok(())
    }

    /// Write one request and hand back the future that resolves to its answer.
    ///
    /// Every request this client sends passes through here, and so does every
    /// answer, which is what makes this the one place a denial can be told
    /// apart from a refusal: nothing reaches the wire that the capability
    /// snapshot did not allow, so a `method_not_found` coming back is the agent
    /// disowning what it published rather than declining what it was asked.
    fn send<Request>(
        &self,
        request: Request,
        connection: &ConnectionTo<Agent>,
        operation: Option<AcpOperation>,
    ) -> Sent<Request::Response>
    where
        Request: agent_client_protocol::JsonRpcRequest,
        Request::Response: Send + 'static,
    {
        let sent = connection.send_request(request);
        let connection_id = self.connection_id;
        let contradicted = Arc::clone(&self.contradicted);
        Box::pin(async move {
            sent.block_task().await.map_err(|error| {
                if error.code == v1::ErrorCode::MethodNotFound {
                    // Flagged before the error is handed over, so the
                    // supervisor that reads it after this request settles
                    // cannot see the answer without also seeing why it is the
                    // last one this connection will give.
                    contradicted.store(true, Ordering::Release);
                    return AcpClientError::protocol_violation(
                        connection_id,
                        operation,
                        error.to_string(),
                    );
                }
                let mut client_error =
                    AcpClientError::agent_error(connection_id, operation, error.to_string());
                client_error.agent_code = Some(i32::from(error.code));
                client_error
            })
        })
    }
}

/// The two ways state changes: an answer we asked for, and news we did not.
/// Both arms are boxed, so what the supervisor's select moves on every event
/// is a pointer rather than the largest thing either arm can carry.
pub(super) enum Accepted {
    Settled(Box<Pending>),
    Inbound(Box<Inbound>),
}

fn prompt_display(prompt: &[v1::ContentBlock], limit: usize) -> String {
    let text = prompt
        .iter()
        .map(|block| match block {
            v1::ContentBlock::Text(text) => text.text.as_str(),
            v1::ContentBlock::Image(_) => "Image",
            _ => "Attachment",
        })
        .collect::<Vec<_>>()
        .join(" ");
    let mut chars = text.chars();
    let shown: String = chars.by_ref().take(limit).collect();
    if chars.next().is_some() {
        format!("{shown}…")
    } else {
        shown
    }
}

/// The capability a content block needs before it may be sent.
///
/// Text and resource links are baseline. The rest are refused here rather than
/// on the wire, because an agent that cannot read an image has no way to say so
/// other than by answering as if the image were not there.
fn content_operation(block: &v1::ContentBlock) -> Option<AcpOperation> {
    match block {
        v1::ContentBlock::Image(_) => Some(AcpOperation::PromptImage),
        v1::ContentBlock::Audio(_) => Some(AcpOperation::PromptAudio),
        v1::ContentBlock::Resource(_) => Some(AcpOperation::PromptEmbeddedContext),
        _ => None,
    }
}

/// The capability an MCP server entry needs before it may be sent, or `None`
/// when this build has no operation for its transport.
///
/// `v1::McpServer` is `#[non_exhaustive]`, so the wildcard is forced rather than
/// chosen — an SDK upgrade can add a transport without touching this file. It
/// therefore answers `None` instead of guessing: an unknown transport has no
/// capability to check, and the caller refuses it. Inheriting permission from a
/// neighbouring arm would let a server through on an agent that never
/// advertised it, which is the whole failure this function exists to prevent.
fn mcp_server_operation(server: &v1::McpServer) -> Option<AcpOperation> {
    match server {
        v1::McpServer::Http(_) => Some(AcpOperation::McpHttp),
        v1::McpServer::Sse(_) => Some(AcpOperation::McpSse),
        v1::McpServer::Stdio(_) => Some(AcpOperation::McpStdio),
        v1::McpServer::Acp(_) => Some(AcpOperation::McpAcp),
        _ => None,
    }
}

/// Whether a select currently offers this value, grouped or not.
///
/// Grouping is presentation: which header a value renders under says nothing
/// about whether it can be chosen, so both shapes are searched the same way.
fn offers_value(select: &v1::SessionConfigSelect, value: &v1::SessionConfigValueId) -> bool {
    match &select.options {
        v1::SessionConfigSelectOptions::Ungrouped(options) => {
            options.iter().any(|option| &option.value == value)
        }
        v1::SessionConfigSelectOptions::Grouped(groups) => groups
            .iter()
            .flat_map(|group| &group.options)
            .any(|option| &option.value == value),
        // An option shape this client does not know is one it cannot check a
        // value against, so it does not claim to have checked.
        _ => false,
    }
}
