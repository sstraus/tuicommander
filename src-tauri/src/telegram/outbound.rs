use super::{BotApi, Error, Paths};
use serde_json::{Value, json};
use std::time::Duration;
use tokio::time::Instant;

pub(super) const PREVIEW_LABEL: &str = "Preview — not approved.";

/// Deliberately conservative UTF-16 budget, including astral characters.
pub(super) fn chunks(text: &str) -> Result<Vec<&str>, Error> {
    chunks_with_budget(text, 4096)
}

fn chunks_with_budget(text: &str, budget: usize) -> Result<Vec<&str>, Error> {
    if text.is_empty() || budget < 2 {
        return Err(Error::Config);
    }
    let mut result = Vec::new();
    let mut start = 0;
    let mut units = 0;
    for (index, ch) in text.char_indices() {
        if units + ch.len_utf16() > budget {
            result.push(&text[start..index]);
            start = index;
            units = 0;
        }
        units += ch.len_utf16();
    }
    result.push(&text[start..]);
    Ok(result)
}

pub(super) struct Active {
    pub request: String,
    pub peer: String,
    pub pty: String,
    pub epoch: u64,
    pub chat: i64,
    pub draft: i64,
    pub interrupt: Option<std::sync::Arc<()>>,
    text: String,
    pub authored: bool,
    sent: Instant,
    dirty: bool,
}

/// One worker serializes draft refresh and final delivery. Polling runs separately.
pub(super) struct Outbound {
    pub paths: Paths,
    pub api: BotApi,
    pub active: Option<Active>,
    pub recent: std::sync::Arc<parking_lot::Mutex<super::recent::Recent>>,
    next_send: Instant,
    stopped: Option<Error>,
}
impl Outbound {
    pub fn new(paths: Paths, api: BotApi) -> Self {
        Self {
            paths,
            api,
            active: None,
            recent: Default::default(),
            next_send: Instant::now(),
            stopped: None,
        }
    }

    pub async fn call(&mut self, chat: i64, method: &str, body: Value) -> Result<Value, Error> {
        let preview = body["text"].as_str().unwrap_or("").to_owned();
        let result = self.call_inner(chat, method, body, None).await;
        let kind = match method {
            "sendMessage" => Some("reply"),
            "answerCallbackQuery" => Some("button"),
            _ => None,
        };
        if let Some(kind) = kind {
            self.recent.lock().record(
                "out",
                kind,
                result
                    .as_ref()
                    .map(|_| "sent".into())
                    .unwrap_or_else(|error| format!("failed: {error}")),
                Some(&preview),
            );
        }
        result
    }

    pub async fn setup_reply(
        &mut self,
        chat: i64,
        reply: super::settings::SetupReply,
    ) -> Result<Value, Error> {
        let result = self
            .call_inner(
                chat,
                "sendMessage",
                json!({"chat_id":chat,"text":reply.text()}),
                Some(reply),
            )
            .await;
        self.recent.lock().record(
            "out",
            "reply",
            result
                .as_ref()
                .map(|_| "sent".into())
                .unwrap_or_else(|error| format!("failed: {error}")),
            Some(reply.text()),
        );
        result
    }

    async fn call_inner(
        &mut self,
        chat: i64,
        method: &str,
        body: Value,
        setup: Option<super::settings::SetupReply>,
    ) -> Result<Value, Error> {
        if let Some(error) = self.stopped {
            return Err(error);
        }
        // A long Telegram throttle must not hold the worker asleep.
        if let Some(wait) = self.next_send.checked_duration_since(Instant::now())
            && wait > Duration::from_secs(1)
        {
            return Err(Error::RateLimited(wait.as_secs().saturating_add(1)));
        }
        tokio::time::sleep_until(self.next_send).await;
        // Revocation is checked immediately before every operation, after waiting.
        let ids = self.paths.allowlist_entries()?;
        if chat <= 0
            || (setup.is_none() && !ids.contains(&chat))
            || (matches!(setup, Some(super::settings::SetupReply::Linked)) && !ids.contains(&chat))
        {
            return Err(Error::Config);
        }
        self.next_send = Instant::now() + Duration::from_secs(1);
        let result = match setup {
            Some(reply) => self.api.setup_reply(chat, reply).await,
            None => self.api.request(method, body).await,
        };
        if let Err(Error::RateLimited(seconds)) = result {
            self.next_send = Instant::now() + Duration::from_secs(seconds.clamp(1, 3600));
        }
        if let Err(error) = result
            && error.is_fatal()
        {
            self.stopped = Some(error);
            self.active = None;
        }
        result
    }

    pub async fn received_reaction(&mut self, chat: i64, message: i64) -> Result<Value, Error> {
        self.call(chat, "setMessageReaction", json!({"chat_id":chat,"message_id":message,
            "reaction":[{"type":"emoji","emoji":super::streaming::RECEIVED_REACTION}],"is_big":false})).await
    }

    pub async fn begin(
        &mut self,
        request: String,
        peer: String,
        pty: String,
        epoch: u64,
        chat: i64,
    ) -> Result<i64, Error> {
        self.begin_text(request, peer, pty, epoch, chat, "").await
    }

    pub async fn begin_text(
        &mut self,
        request: String,
        peer: String,
        pty: String,
        epoch: u64,
        chat: i64,
        text: &str,
    ) -> Result<i64, Error> {
        if self.active.is_some() {
            return Err(Error::State);
        }
        let bytes = *uuid::Uuid::new_v4().as_bytes();
        let draft = (i64::from_be_bytes(bytes[..8].try_into().map_err(|_| Error::State)?)
            & i64::MAX)
            .max(1);
        let accepted = self
            .call(
                chat,
                "sendMessageDraft",
                json!({"chat_id":chat,"draft_id":draft,"text":text,"can_stop":true,"keep_on_stop":false}),
            )
            .await?;
        if accepted != Value::Bool(true) {
            return Err(Error::Protocol);
        }
        self.active = Some(Active {
            request,
            peer,
            pty,
            epoch,
            chat,
            draft,
            interrupt: None,
            text: text.into(),
            authored: false,
            sent: Instant::now(),
            dirty: false,
        });
        Ok(draft)
    }

    pub fn activity(&mut self, request: &str, text: &str) -> Result<(), Error> {
        // Agent-authored content remains explicitly unapproved.
        self.status(
            request,
            &format!(
                "{PREVIEW_LABEL}\n{}",
                text.chars().take(500).collect::<String>()
            ),
            true,
        );
        if self.active.as_ref().is_none_or(|a| a.request != request) {
            return Err(Error::State);
        }
        Ok(())
    }

    pub fn status(&mut self, request: &str, text: &str, authored: bool) {
        if let Some(active) = self.active.as_mut().filter(|a| a.request == request) {
            let (prefix, body) = text
                .strip_prefix(PREVIEW_LABEL)
                .map_or(("", text), |body| (PREVIEW_LABEL, body));
            let budget = 4096 - prefix.encode_utf16().count();
            let mut units = 0;
            let tail: String = body
                .chars()
                .rev()
                .take_while(|ch| {
                    units += ch.len_utf16();
                    units <= budget
                })
                .collect();
            let text = format!("{prefix}{}", tail.chars().rev().collect::<String>());
            if active.text != text {
                active.text = text;
                active.dirty = true;
            }
            active.authored = authored;
        }
    }

    pub async fn refresh(&mut self) -> Result<(), Error> {
        let Some(active) = &self.active else {
            return Ok(());
        };
        let elapsed = active.sent.elapsed();
        if elapsed < Duration::from_secs(20) && !(active.dirty && elapsed >= Duration::from_secs(2))
        {
            return Ok(());
        }
        let chat = active.chat;
        let body = json!({"chat_id":chat,"draft_id":active.draft,"text":active.text,"can_stop":true,"keep_on_stop":false});
        if self.call(chat, "sendMessageDraft", body).await? != Value::Bool(true) {
            return Err(Error::Protocol);
        }
        if let Some(active) = &mut self.active {
            active.sent = Instant::now();
            active.dirty = false;
        }
        Ok(())
    }

    pub async fn send(
        &mut self,
        chat: i64,
        text: &str,
        keyboard: Option<Value>,
    ) -> Result<Vec<i64>, Error> {
        self.send_tagged(chat, text, keyboard, "").await
    }

    pub async fn send_tagged(
        &mut self,
        chat: i64,
        text: &str,
        keyboard: Option<Value>,
        prefix: &str,
    ) -> Result<Vec<i64>, Error> {
        let budget = 4096usize
            .checked_sub(prefix.encode_utf16().count())
            .ok_or(Error::Config)?;
        let parts = chunks_with_budget(text, budget)?;
        let mut ids = Vec::new();
        for (index, part) in parts.iter().enumerate() {
            let mut body = json!({"chat_id":chat,"text":format!("{prefix}{part}")});
            if index + 1 == parts.len()
                && let Some(keyboard) = &keyboard
            {
                body["reply_markup"] = keyboard.clone();
            }
            let message = self.call(chat, "sendMessage", body).await?;
            let id = message["message_id"]
                .as_i64()
                .filter(|id| *id > 0)
                .ok_or(Error::Protocol)?;
            ids.push(id);
        }
        Ok(ids)
    }

    pub async fn finish(&mut self, request: &str, text: &str) -> Result<Vec<i64>, Error> {
        chunks(text)?;
        if self.active.as_ref().is_none_or(|a| a.request != request) {
            return Err(Error::State);
        }
        // Retire before any network side effect: a partial/uncertain final cannot
        // be followed by a draft refresh, nor retried as though nothing was sent.
        let active = self.active.take().ok_or(Error::State)?;
        self.send(active.chat, text, None).await
    }
}

#[cfg(all(test, unix))]
#[path = "critic_tests.rs"]
mod critic_tests;
