use super::{BotApi, Config, Error, Paths, mail::PendingMail, outbound::Outbound, tool::Input};
use crate::state::{AppEvent, AppState};
use serde_json::{Value, json};
use std::{
    collections::VecDeque,
    sync::{Arc, OnceLock},
};
use tokio::sync::{mpsc, oneshot};

pub(super) static HANDLE: OnceLock<mpsc::Sender<Command>> = OnceLock::new();
pub(super) enum Command {
    SetupReply {
        chat: i64,
        message: super::settings::SetupReply,
        reply: oneshot::Sender<Result<Value, Error>>,
    },
    Automation {
        store: crate::automations::store::RunStore,
        run: Box<crate::automations::run::AutomationRun>,
        text: String,
    },
    Tool {
        caller: String,
        sid: String,
        input: Input,
        reply: oneshot::Sender<Result<Value, Error>>,
    },
    Deliver {
        mail: PendingMail,
        reply: oneshot::Sender<Result<Value, Error>>,
    },
    Update {
        value: Value,
        reply: oneshot::Sender<Result<Value, Error>>,
    },
}

pub(super) struct Runtime {
    pub state: Arc<AppState>,
    pub config: Config,
    config_watch: super::config::WatchedConfig,
    pub outbound: Outbound,
    pub pending: VecDeque<(String, i64)>,
    pub(super) requests: VecDeque<super::streaming::Request>,
    pub callbacks: super::callbacks::Callbacks,
    pub(super) sender_limits: std::collections::HashMap<String, tokio::time::Instant>,
    pub adapter_sid: String,
    pub(super) registration: Option<super::registration::Registration>,
}
impl Runtime {
    pub fn new(
        state: Arc<AppState>,
        config: Config,
        paths: Paths,
        adapter_sid: String,
    ) -> Result<Self, Error> {
        let config_watch = super::config::WatchedConfig::new(paths.clone())?;
        let mut outbound = Outbound::new(paths.clone(), BotApi::new(paths)?);
        outbound.recent = state.telegram_recent.clone();
        Ok(Self {
            state,
            config,
            config_watch,
            outbound,
            pending: VecDeque::new(),
            requests: VecDeque::new(),
            callbacks: Default::default(),
            sender_limits: Default::default(),
            adapter_sid,
            registration: None,
        })
    }
    pub fn live(&self, peer: &str, pty: &str, epoch: u64) -> bool {
        // Draft ownership follows the correlated recipient, including a reply sender.
        // Registration takeover already retires the active draft and its token.
        live(&self.state, peer, pty, epoch)
    }
    pub fn current(&self, peer: &str) -> Result<(String, u64), Error> {
        let pty = self.state.live_pty_for_peer(peer).ok_or(Error::State)?;
        let state = self
            .state
            .session_state_with_shell(&pty)
            .ok_or(Error::State)?;
        if !matches!(
            state.agent_state.as_deref(),
            Some("working" | "awaiting_input")
        ) {
            return Err(Error::State);
        }
        Ok((pty.clone(), state.turn_epoch))
    }
    pub fn destination(&self) -> Result<i64, Error> {
        self.outbound
            .paths
            .allowlist()?
            .into_iter()
            .next()
            .ok_or(Error::Config)
    }
    pub async fn tool(&mut self, caller: &str, sid: &str, input: Input) -> Result<Value, Error> {
        self.retire_registration();
        // Check opt-in at use, including after a queued command.
        let config = Config::load(&self.outbound.paths)?.ok_or(Error::Config)?;
        if config.bot_alias != self.config.bot_alias {
            return Err(Error::State);
        }
        if matches!(input, Input::Register {}) {
            self.register(caller, sid).await?;
            return Ok(json!({"registered":true}));
        }
        let bound = self
            .state
            .mcp
            .to_session
            .get(sid)
            .map(|entry| entry.value().clone());
        // Inbound selection can change while an earlier recipient is replying.
        // Explicit takeover revokes pending correlations in clear_registration.
        let request = match &input {
            Input::Begin { request_id }
            | Input::Activity { request_id, .. }
            | Input::Finish { request_id, .. } => self
                .requests
                .iter()
                .find(|request| &request.id == request_id),
            _ => None,
        };
        let authorized = request.map_or_else(
            || self.recipient_peer().as_deref() == Some(caller),
            |request| {
                request.peer == caller
                    && (request.completed.is_some()
                        || self.pending.iter().any(|(id, _)| id == &request.id))
            },
        );
        if (!authorized && !matches!(input, Input::Send { .. }))
            || bound
                .as_deref()
                .and_then(|peer| self.state.resolve_peer_ref_checked(peer).ok().flatten())
                .as_deref()
                != Some(caller)
        {
            return Err(Error::NotRegistered);
        }
        match input {
            Input::Register {} => unreachable!(),
            Input::Unregister {} => {
                if self.registered_peer() != Some(caller) {
                    return Err(Error::NotRegistered);
                }
                self.clear_registration();
                Ok(json!({"registered":false}))
            }
            Input::Begin { request_id } => {
                let chat = self
                    .pending
                    .iter()
                    .find(|(id, _)| id == &request_id)
                    .map(|(_, chat)| *chat)
                    .ok_or(Error::State)?;
                if let Some(active) = &self.outbound.active {
                    if active.request == request_id {
                        return Ok(json!({"draft_id":active.draft}));
                    }
                    return Err(Error::State);
                }
                let current = self.current(caller).ok();
                let (pty, epoch) = current.clone().unwrap_or_default();
                let interrupt = if current.is_some() {
                    Some(super::stop::arm(&self.state, caller, &pty, epoch)?)
                } else {
                    None
                };
                let draft = self
                    .outbound
                    .begin(request_id.clone(), caller.into(), pty, epoch, chat)
                    .await?;
                self.outbound.active.as_mut().ok_or(Error::State)?.interrupt = interrupt;
                Ok(json!({"draft_id":draft}))
            }
            Input::Activity { request_id, text } => {
                self.outbound.activity(&request_id, &text)?;
                Ok(json!({"accepted":true}))
            }
            Input::Finish { request_id, text } => {
                let ids = self.finish_request(&request_id, &text).await?;
                Ok(json!({"message_ids":ids}))
            }
            Input::Send { text, buttons } => {
                let chat = self.destination()?;
                if self.recipient_peer().as_deref() == Some(caller) {
                    self.send_buttons(chat, &text, buttons).await
                } else {
                    if !buttons.is_empty() {
                        return Err(Error::NonOwnerButtons);
                    }
                    let prefix = self.sender_prefix(caller)?;
                    // Reject empty input before charging the sender's interval.
                    super::outbound::chunks(&text)?;
                    self.limit_sender(caller)?;
                    let ids = self
                        .outbound
                        .send_tagged(chat, &text, None, &prefix)
                        .await?;
                    Ok(json!({"message_ids":ids}))
                }
            }
        }
    }
    pub fn track(
        &mut self,
        mail: PendingMail,
        recipient: super::streaming::Recipient,
    ) -> Result<Value, Error> {
        let envelope: Value = serde_json::from_str(&mail.content).map_err(|_| Error::Protocol)?;
        let chat = envelope["chat_id"]
            .as_str()
            .and_then(|s| s.parse::<i64>().ok())
            .ok_or(Error::Protocol)?;
        if !self.pending.iter().any(|(id, _)| id == &mail.id) {
            // Correlation follows the bounded native inbox; no retained text.
            if self.pending.len() >= crate::state::AGENT_INBOX_CAPACITY {
                self.pending.pop_front();
            }
            self.pending.push_back((mail.id.clone(), chat));
            if self.requests.len() >= crate::state::AGENT_INBOX_CAPACITY {
                self.requests.pop_front();
            }
            let cursor = self
                .state
                .agent_inbox
                .get(&mail.recipient)
                .and_then(|inbox| {
                    inbox
                        .iter()
                        .find(|entry| entry.id == mail.id)
                        .map(|entry| entry.timestamp)
                })
                .unwrap_or(u64::MAX);
            self.requests.push_back(super::streaming::Request {
                id: mail.id,
                chat,
                message: envelope["message_id"].as_i64(),
                peer: mail.recipient.clone(),
                recipient,
                cursor,
                turn: None,
                final_error: None,
                final_epoch: None,
                received: tokio::time::Instant::now(),
                read: None,
                working: false,
                waiting: false,
                failed: false,
                acknowledged: false,
                noticed: false,
                queued_notice: false,
                completed: None,
                reaction: 0,
                reaction_sent: None,
            });
        }
        Ok(json!({"accepted":true}))
    }
    pub async fn deliver(&mut self, mut mail: PendingMail) -> Result<Value, Error> {
        self.retire_registration();
        if !self.enabled() {
            return Err(Error::Config);
        }
        let envelope: Value = serde_json::from_str(&mail.content).map_err(|_| Error::Protocol)?;
        let chat = envelope["chat_id"]
            .as_str()
            .and_then(|s| s.parse::<i64>().ok())
            .ok_or(Error::Protocol)?;
        if !self.outbound.paths.allowlist()?.contains(&chat) {
            self.state.telegram_recent.lock().record(
                "in",
                "message",
                "rejected: not authorized",
                None,
            );
            return Ok(json!({"accepted":false}));
        }
        if self.requests.iter().any(|request| request.id == mail.id) {
            return Ok(json!({"accepted":true}));
        }
        let message = envelope["message_id"].as_i64().ok_or(Error::Protocol)?;
        let received = self
            .outbound
            .received_reaction(chat, message)
            .await
            .is_ok_and(|result| result == Value::Bool(true));
        self.stage("received");
        let text = envelope["text"].as_str();
        let Some(peer) = self
            .reply_recipient(&envelope)
            .or_else(|| self.recipient_peer())
        else {
            self.state
                .telegram_recent
                .lock()
                .record("in", "message", "dropped: no agent", text);
            self.outbound
                .send(
                    chat,
                    "No agent is listening. Ask an agent in TUICommander to register for Telegram.",
                    None,
                )
                .await?;
            return Ok(json!({"accepted":false}));
        };
        mail.recipient = peer;
        // Subscribe before delivery: ACP may wake as soon as the mail is stored.
        let recipient = super::streaming::Recipient::capture(&self.state, &mail.recipient);
        let result = super::native::offer(&self.state, &self.adapter_sid, &mail).await;
        self.state.telegram_recent.lock().record(
            "in",
            "message",
            result
                .as_ref()
                .map(|_| "delivered".into())
                .unwrap_or_else(|error| format!("failed: {error}")),
            text,
        );
        let delivery = result?;
        let id = mail.id.clone();
        let accepted = self.track(mail, recipient)?;
        if let Some(request) = self.requests.iter_mut().find(|r| r.id == id) {
            request.reaction_sent = received.then_some(0);
        }
        self.show_oldest().await;
        if delivery["delivery_path"] == "inbox_only" {
            self.stage("queued");
        }
        Ok(accepted)
    }
    pub async fn update(&mut self, value: Value) -> Result<Value, Error> {
        self.retire_registration();
        if !self.enabled() {
            return Err(Error::Config);
        }
        if let Some(message) = value.get("message") {
            let id = message["chat"]["id"].as_i64();
            if message["chat"]["type"] != "private"
                || id.is_none_or(|id| {
                    !self
                        .outbound
                        .paths
                        .allowlist_entries()
                        .is_ok_and(|ids| ids.contains(&id))
                })
            {
                self.state.telegram_recent.lock().record(
                    "in",
                    "message",
                    "rejected: not authorized",
                    None,
                );
            }
        }
        if !self.stop_acp(&value).await? {
            self.stop(&value)?;
        }
        let callback = self.callback(&value).await;
        if value.get("callback_query").is_some() {
            let outcome = match &callback {
                Ok(true) => "delivered".into(),
                Ok(false) => "rejected: not authorized".into(),
                Err(error) => format!("failed: {error}"),
            };
            self.state
                .telegram_recent
                .lock()
                .record("in", "button", outcome, None);
        }
        callback?;
        Ok(json!({"accepted":true}))
    }
    pub(super) fn enabled(&self) -> bool {
        self.config_watch.enabled(&self.config.bot_alias)
    }
    pub async fn event(&mut self, event: AppEvent) {
        self.retire_registration();
        if !self.enabled() {
            self.outbound.active = None;
            return;
        }
        let Some(pty) = self
            .state
            .live_pty_for_peer(&self.recipient_peer().unwrap_or_default())
        else {
            return;
        };
        if let Some(text) = super::notifications::notice(&event, &pty) {
            if let Some(request) = self.requests.iter().find(|r| {
                r.completed.is_none()
                    && r.working
                    && r.peer == self.recipient_peer().unwrap_or_default()
            }) && let AppEvent::ProgressRecorded { payload, .. } = &event
            {
                if payload["entry"]["type"] == "done" {
                    let id = request.id.clone();
                    if let Err(error) = self.finish_request(&id, &text).await {
                        alert(&self.state, error);
                    }
                }
                // Blocked is a status, not an answer; avoid a second final.
                return;
            }
            // A completed request owns its progress outcome as well as finish.
            let epoch = self
                .state
                .session_state_with_shell(&pty)
                .map(|s| s.turn_epoch);
            if self
                .requests
                .iter()
                .any(|r| r.completed.is_some() && r.final_epoch == epoch)
            {
                return;
            }
            match self.destination() {
                Ok(chat) => {
                    self.callbacks.clear();
                    if let Err(error) = self.outbound.send(chat, &text, None).await {
                        alert(&self.state, error);
                    }
                }
                Err(error) => alert(&self.state, error),
            }
        }
        if let AppEvent::PtyParsed { session_id, parsed } = event
            && let Some(a) = &self.outbound.active
            && session_id == a.pty
            && parsed["_turn_epoch"].as_u64() == Some(a.epoch)
            && parsed["type"] == "intent"
            && let Some(text) = parsed["text"].as_str()
        {
            let request = a.request.clone();
            if let Err(error) = self.outbound.activity(&request, text) {
                alert(&self.state, error);
            }
        }
    }
    pub async fn tick(&mut self) {
        self.retire_registration();
        if !self.enabled() {
            self.outbound.active = None;
            return;
        }
        self.stream_tick().await;
        if let Err(error) = self.outbound.refresh().await {
            alert(&self.state, error);
        }
    }
    pub(super) fn events_lagged(&mut self) {
        // Recipient snapshots and the ACP reader retain the request. Only
        // nonessential transient intent/progress notifications may be missed.
        tracing::warn!(
            source = "telegram",
            "Telegram events lagged; open requests retained"
        );
    }
    #[cfg(test)]
    pub async fn run(self, mut commands: mpsc::Receiver<Command>) {
        self.run_ref(&mut commands).await;
    }
    pub async fn run_ref(mut self, commands: &mut mpsc::Receiver<Command>) {
        let mut events = self.state.event_bus.subscribe();
        let mut interval = tokio::time::interval(std::time::Duration::from_secs(1));
        loop {
            tokio::select! {
                command = commands.recv() => {
                    let Some(command) = command else { break; };
                    if let Command::Automation { store, run, text } = command {
                        self.automation_notice(&store, &run, &text).await;
                        continue;
                    }
                    let (reply,result) = match command {
                        Command::Automation { .. } => unreachable!(),
                        Command::Tool { caller,sid,input,reply } => (reply,self.tool(&caller,&sid,input).await),
                        Command::Deliver { mail,reply } => (reply,self.deliver(mail).await),
                        Command::Update { value,reply } => (reply,self.update(value).await),
                        Command::SetupReply { chat,message,reply } => {
                            let (kind, outcome) = match message {
                                super::settings::SetupReply::Start => ("/start", "delivered"),
                                super::settings::SetupReply::Linked => ("pairing", "delivered"),
                                super::settings::SetupReply::InvalidCode => ("pairing", "rejected: not authorized"),
                            };
                            self.state.telegram_recent.lock().record("in", kind, outcome, None);
                            let result = if self.enabled() { self.outbound.setup_reply(chat, message).await } else { Err(Error::Config) };
                            (reply, result)
                        },
                    };
                    let _ = reply.send(result);
                },
                event = events.recv() => match event {
                    Ok(event) => self.event(event).await,
                    Err(tokio::sync::broadcast::error::RecvError::Lagged(_)) => {
                        self.events_lagged();
                    },
                    Err(_) => break,
                },
                _ = interval.tick() => self.tick().await,
            }
        }
    }
}

pub(super) fn live(state: &AppState, peer: &str, pty: &str, epoch: u64) -> bool {
    state.live_pty_for_peer(peer).as_deref() == Some(pty)
        && !state.session_maps.exit_codes.contains_key(pty)
        && state.session_state_with_shell(pty).is_some_and(|s| {
            s.turn_epoch == epoch
                && matches!(s.agent_state.as_deref(), Some("working" | "awaiting_input"))
        })
}
pub(super) fn alert(state: &AppState, error: Error) {
    let _ = state.event_bus.send(AppEvent::McpToast {
        title: "Telegram adapter unavailable".into(),
        message: Some(error.to_string()),
        level: "error".into(),
        sound: None,
        urgency: "normal".into(),
        origin_repo_path: None,
        origin_session_id: None,
    });
    tracing::error!(source="telegram",error=%error,"Telegram adapter unavailable; inspect daemon logs");
}
