//! Conversation persistence types for AI Chat (L1) and Agent loop (L2+).
//!
//! Extracted from `ai_chat.rs` so L2 tool-call extensions live next to the
//! agent code while L1 code can keep importing the same names via
//! `crate::ai_chat::{ChatMessage, Conversation, ConversationMeta}`.

use serde::{Deserialize, Serialize};
use serde_json::Value;

/// Current schema version constant — only referenced by tests that verify
/// round-trip serialization. Production load relies on `default_schema_version`
/// for serde defaults; no runtime migration is needed (v1→v2 is a no-op).
#[cfg(test)]
pub(crate) const CURRENT_SCHEMA_VERSION: u32 = 2;

fn default_schema_version() -> u32 {
    1
}

/// One tool call emitted by an assistant message in an agent turn.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub(crate) struct ToolCallRecord {
    pub id: String,
    pub name: String,
    #[serde(default)]
    pub arguments: Value,
}

/// A single message in a saved conversation.
///
/// L1 chat uses `role` + `content` only. L2 agent turns add tool-call
/// metadata: assistant messages may carry `tool_calls`, and a `tool` role
/// message carries `tool_use_id` + `tool_result` (+ `is_error`).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct ChatMessage {
    pub role: String, // "user" | "assistant" | "system" | "tool"
    pub content: String,
    #[serde(default)]
    pub timestamp: u64, // unix millis

    // -- L2 tool-call extensions (all optional + serde default) --
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tool_calls: Option<Vec<ToolCallRecord>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tool_use_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tool_result: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub is_error: Option<bool>,
}

impl ChatMessage {
    /// Plain L1 text message (user/assistant/system).
    #[cfg_attr(not(test), allow(dead_code))]
    pub fn text(role: impl Into<String>, content: impl Into<String>, timestamp: u64) -> Self {
        Self {
            role: role.into(),
            content: content.into(),
            timestamp,
            tool_calls: None,
            tool_use_id: None,
            tool_result: None,
            is_error: None,
        }
    }
}

/// Metadata for a saved conversation.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct ConversationMeta {
    pub id: String,
    pub title: String,
    /// Session ID of the attached terminal (if any)
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub session_id: Option<String>,
    pub created: u64, // unix millis
    pub updated: u64, // unix millis
    pub message_count: usize,
    /// Provider + model used
    #[serde(default)]
    pub provider: String,
    #[serde(default)]
    pub model: String,
}

/// A full conversation with messages.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct Conversation {
    pub meta: ConversationMeta,
    pub messages: Vec<ChatMessage>,
    /// Schema version. 1 = L1 chat. 2 = L2 with tool calls. Older files
    /// without this field load as 1 via serde default.
    #[serde(default = "default_schema_version")]
    pub schema_version: u32,
}

const TOOL_RESULT_MAX_BYTES: usize = 8192;

impl Conversation {
    pub fn sanitize_for_persist(&mut self) {
        for msg in &mut self.messages {
            if let Some(ref mut result) = msg.tool_result {
                *result = crate::ai_agent::tools::redact_secrets(result);
                if result.len() > TOOL_RESULT_MAX_BYTES {
                    result.truncate(TOOL_RESULT_MAX_BYTES);
                    result.push_str("\n[truncated]");
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn v1_json_loads_with_default_schema_version() {
        let json_v1 = r#"{
            "meta": {
                "id": "old", "title": "Old", "created": 1, "updated": 2,
                "message_count": 1
            },
            "messages": [{"role":"user","content":"hi"}]
        }"#;
        let conv: Conversation = serde_json::from_str(json_v1).unwrap();
        assert_eq!(conv.schema_version, 1);
        assert_eq!(conv.messages.len(), 1);
        assert!(conv.messages[0].tool_calls.is_none());
        assert!(conv.messages[0].tool_use_id.is_none());
    }

    #[test]
    fn agent_conversation_with_tool_calls_roundtrips() {
        let conv = Conversation {
            meta: ConversationMeta {
                id: "a1".into(), title: "Agent".into(), session_id: Some("s1".into()),
                created: 1, updated: 2, message_count: 3,
                provider: "anthropic".into(), model: "claude-sonnet-4-5".into(),
            },
            messages: vec![
                ChatMessage::text("user", "list files", 1),
                ChatMessage {
                    role: "assistant".into(),
                    content: "I'll list them.".into(),
                    timestamp: 2,
                    tool_calls: Some(vec![ToolCallRecord {
                        id: "call_1".into(),
                        name: "ai_terminal_send_input".into(),
                        arguments: json!({"text": "ls\n"}),
                    }]),
                    tool_use_id: None,
                    tool_result: None,
                    is_error: None,
                },
                ChatMessage {
                    role: "tool".into(),
                    content: String::new(),
                    timestamp: 3,
                    tool_calls: None,
                    tool_use_id: Some("call_1".into()),
                    tool_result: Some("Cargo.toml\nsrc/".into()),
                    is_error: Some(false),
                },
            ],
            schema_version: CURRENT_SCHEMA_VERSION,
        };

        let json = serde_json::to_string(&conv).unwrap();
        let loaded: Conversation = serde_json::from_str(&json).unwrap();

        assert_eq!(loaded.schema_version, CURRENT_SCHEMA_VERSION);
        assert_eq!(loaded.messages.len(), 3);
        let tool_calls = loaded.messages[1].tool_calls.as_ref().unwrap();
        assert_eq!(tool_calls.len(), 1);
        assert_eq!(tool_calls[0].id, "call_1");
        assert_eq!(tool_calls[0].name, "ai_terminal_send_input");
        assert_eq!(tool_calls[0].arguments["text"], "ls\n");
        assert_eq!(loaded.messages[2].tool_use_id.as_deref(), Some("call_1"));
        assert_eq!(loaded.messages[2].tool_result.as_deref(), Some("Cargo.toml\nsrc/"));
        assert_eq!(loaded.messages[2].is_error, Some(false));
    }

    #[test]
    fn l1_message_omits_tool_fields_in_json() {
        let msg = ChatMessage::text("user", "hello", 1);
        let json = serde_json::to_string(&msg).unwrap();
        assert!(!json.contains("tool_calls"));
        assert!(!json.contains("tool_use_id"));
        assert!(!json.contains("tool_result"));
        assert!(!json.contains("is_error"));
    }
}
