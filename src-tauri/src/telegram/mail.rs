use super::Error;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::BTreeSet;

/// Match the native inbox boundary (64 KiB), including envelope JSON overhead.
const MAX_MAIL_BYTES: usize = 64 * 1024;

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct PendingMail {
    pub(crate) id: String,
    pub(crate) recipient: String,
    pub(crate) content: String,
}

#[derive(Serialize)]
struct TextMail<'a> {
    channel: &'static str,
    kind: &'static str,
    request_id: &'a str,
    chat_id: String,
    message_id: i64,
    text: &'a str,
    #[serde(skip_serializing_if = "Option::is_none")]
    reply_to_text: Option<&'a str>,
}

pub(super) struct Update {
    pub(super) id: i64,
    pub(super) mail: Option<PendingMail>,
}
impl Update {
    pub(super) fn parse(
        value: &Value,
        ids: &BTreeSet<i64>,
        alias: &str,
        peer: &str,
    ) -> Result<Self, Error> {
        let id = value
            .get("update_id")
            .and_then(Value::as_i64)
            .filter(|id| *id >= 0)
            .ok_or(Error::Protocol)?;
        let Some(message) = value.get("message") else {
            return Ok(Self { id, mail: None });
        };
        let chat = message.get("chat").ok_or(Error::Protocol)?;
        let chat_id = chat
            .get("id")
            .and_then(Value::as_i64)
            .ok_or(Error::Protocol)?;
        if !ids.contains(&chat_id) || chat.get("type").and_then(Value::as_str) != Some("private") {
            return Ok(Self { id, mail: None });
        }
        let Some(text) = message.get("text").and_then(Value::as_str) else {
            return Ok(Self { id, mail: None });
        };
        let message_id = message
            .get("message_id")
            .and_then(Value::as_i64)
            .filter(|id| *id > 0)
            .ok_or(Error::Protocol)?;
        let request_id = format!("tg:{alias}:{id}");
        let content = serde_json::to_string(&TextMail {
            channel: "telegram",
            kind: "text",
            request_id: &request_id,
            chat_id: chat_id.to_string(),
            message_id,
            text,
            reply_to_text: message
                .get("reply_to_message")
                .and_then(|reply| reply["text"].as_str()),
        })
        .map_err(|_| Error::Protocol)?;
        if content.len() > MAX_MAIL_BYTES {
            return Err(Error::Protocol);
        }
        Ok(Self {
            id,
            mail: Some(PendingMail {
                id: request_id,
                recipient: peer.into(),
                content,
            }),
        })
    }
}

/// Native integration offers to the in-memory inbox under the identity lock,
/// then arbitrates safe wake. No adapter persistence or consumption receipts.
pub(crate) trait MailPort {
    async fn offer(&mut self, mail: &PendingMail) -> Result<bool, Error>;
    /// Setup replies share the existing outbound worker, not the agent inbox.
    async fn setup_reply(
        &mut self,
        _chat: i64,
        _reply: super::settings::SetupReply,
    ) -> Result<(), Error> {
        Ok(())
    }
    /// Validated update id; concrete ports authorize non-text update payloads.
    async fn update(&mut self, _value: &Value) -> Result<(), Error> {
        Ok(())
    }
}

#[cfg(test)]
#[derive(Default)]
pub(super) struct TestInbox {
    pub(super) by_id: std::collections::BTreeMap<String, PendingMail>,
}
#[cfg(test)]
impl MailPort for TestInbox {
    fn offer(&mut self, mail: &PendingMail) -> impl Future<Output = Result<bool, Error>> {
        self.by_id
            .entry(mail.id.clone())
            .or_insert_with(|| mail.clone());
        std::future::ready(Ok(true))
    }
}
