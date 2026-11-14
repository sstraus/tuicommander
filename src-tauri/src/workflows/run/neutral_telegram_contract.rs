//! Telegram Bot API 10.4 projection. Credentials never occur in these values.
use super::neutral_effects::EffectKey;
use crate::workflows::catalog::content_digest;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct MessageReference {
    pub bot: String,
    pub chat: i64,
    pub message: i64,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct NotificationReference {
    pub effect: EffectKey,
    pub message: MessageReference,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct ActionReference {
    pub effect: EffectKey,
    pub revision: u64,
    pub approved: bool,
    pub label: String,
}

impl ActionReference {
    pub(crate) fn token(&self, notification_run: &str) -> Result<String, String> {
        let run = uuid::Uuid::parse_str(notification_run)
            .map_err(|_| "Telegram action requires a run UUID")?;
        // 2 + 32 + 1 + 28 = 63 bytes, under callback_data's 64-byte limit.
        // The digest binds the FULL key (arguments/subject), revision and answer.
        let digest =
            content_digest(&(notification_run, &self.effect, self.revision, self.approved))?;
        Ok(format!("w:{}:{}", run.simple(), &digest[..28]))
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum TelegramOperation {
    Send,
    Update,
    Reply,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct TelegramRequest {
    pub chat: i64,
    pub text: String,
    #[serde(default)]
    pub message: Option<MessageReference>,
    #[serde(default)]
    pub actions: Vec<ActionReference>,
}

impl TelegramRequest {
    pub(crate) fn wire(
        &self,
        bot: &str,
        op: TelegramOperation,
        notification_run: &str,
    ) -> Result<(&'static str, Value), String> {
        if self.chat <= 0 || self.text.is_empty() || self.text.encode_utf16().count() > 4096 {
            return Err("Telegram requires a private chat and 1..4096 UTF-16 units".into());
        }
        match (&self.message, op) {
            (None, TelegramOperation::Send) => {}
            (Some(m), TelegramOperation::Update | TelegramOperation::Reply)
                if m.bot == bot && m.chat == self.chat && m.message > 0 => {}
            _ => {
                return Err(
                    "Telegram operation requires its exact bot/chat/message reference".into(),
                );
            }
        }
        let mut body = json!({"chat_id": self.chat, "text": self.text});
        if let Some(message) = &self.message {
            match op {
                TelegramOperation::Update => body["message_id"] = json!(message.message),
                TelegramOperation::Reply => {
                    body["reply_parameters"] =
                        json!({"message_id": message.message, "allow_sending_without_reply": false})
                }
                TelegramOperation::Send => unreachable!(),
            }
        }
        let mut buttons = Vec::new();
        for action in &self.actions {
            if action.revision == 0 || action.label.trim().is_empty() {
                return Err("Telegram action requires a revision and label".into());
            }
            buttons.push(vec![
                json!({"text": action.label, "callback_data": action.token(notification_run)?}),
            ]);
        }
        if !buttons.is_empty() || op == TelegramOperation::Update {
            body["reply_markup"] = json!({"inline_keyboard": buttons});
        }
        Ok((
            if op == TelegramOperation::Update {
                "editMessageText"
            } else {
                "sendMessage"
            },
            body,
        ))
    }

    pub(crate) fn receipt(
        &self,
        bot: &str,
        op: TelegramOperation,
        value: &Value,
    ) -> Result<MessageReference, String> {
        let id = value["message_id"]
            .as_i64()
            .filter(|id| *id > 0)
            .ok_or("Telegram response lacks a message ID")?;
        if value["chat"]["id"].as_i64() != Some(self.chat)
            || (op == TelegramOperation::Update
                && self.message.as_ref().map(|m| m.message) != Some(id))
        {
            return Err("Telegram response differs from requested message".into());
        }
        Ok(MessageReference {
            bot: bot.into(),
            chat: self.chat,
            message: id,
        })
    }
}
