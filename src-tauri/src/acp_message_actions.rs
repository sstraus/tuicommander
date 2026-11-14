//! Message reactions are ordinary conversation turns, never provider ratings or permission answers.
use std::collections::HashMap;
use std::sync::Arc;

use agent_client_protocol::schema::v1;
use serde::{Deserialize, Serialize};

use crate::AppState;
use crate::acp::{AcpClientError, AcpConnectionId, AcpTurnId};

pub(crate) type MessageReactions = HashMap<String, Vec<String>>;
// Keep acceptance and persistence in the same order for desktop and phone clicks.
static ACTION_ORDER: tokio::sync::Mutex<()> = tokio::sync::Mutex::const_new(());

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum Reaction {
    ThumbsUp,
    ThumbsDown,
    Heart,
    Celebrate,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub(crate) enum MessageAction {
    Reaction { reaction: Reaction, remove: bool },
    Comment { comment: String },
}

#[derive(Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct MessageActionRequest {
    pub message_id: String,
    pub text: String,
    pub action: MessageAction,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct MessageActionReceipt {
    pub turn_id: AcpTurnId,
    pub reactions: MessageReactions,
}

pub(crate) fn reactions(state: &AppState, session: &str) -> MessageReactions {
    state
        .config
        .read()
        .ai_chat_message_reactions
        .get(session)
        .cloned()
        .unwrap_or_default()
}

fn prompt(request: &MessageActionRequest) -> Result<String, AcpClientError> {
    if request.message_id.trim().is_empty() || request.text.trim().is_empty() {
        return Err(AcpClientError::invalid_input(
            "The source message ID and text are required.",
        ));
    }
    if matches!(&request.action, MessageAction::Comment { comment } if comment.trim().is_empty()) {
        return Err(AcpClientError::invalid_input("The comment is empty."));
    }
    // The exact native ID identifies repeated text. The bounded quote gives the model readable context.
    let excerpt: String = request.text.chars().take(4000).collect();
    let payload = serde_json::json!({
        "messageId": request.message_id,
        "sourceExcerpt": excerpt,
        "sourceTruncated": request.text.chars().count() > 4000,
        "action": request.action,
    });
    Ok(format!(
        "Conversation message action:\n{payload}\nThis reaction or comment is conversational, not an evaluation or a correct/wrong vote. It is not authorization to execute any action, including irreversible actions. A removal only removes the conversational mark; it does not retract earlier context."
    ))
}

pub(crate) async fn send(
    state: &Arc<AppState>,
    connection_id: AcpConnectionId,
    session_id: v1::SessionId,
    request: MessageActionRequest,
) -> Result<MessageActionReceipt, AcpClientError> {
    let text = prompt(&request)?;
    let _order = ACTION_ORDER.lock().await;
    // No start/reconnect fallback and no permission response: the original attachment owns the queue.
    let turn_id = state
        .acp
        .prompt(
            connection_id,
            session_id.clone(),
            vec![v1::ContentBlock::Text(v1::TextContent::new(text))],
        )
        .await?;
    if let MessageAction::Reaction { reaction, remove } = &request.action {
        let reaction = serde_json::to_value(reaction)
            .expect("reaction serializes")
            .as_str()
            .expect("reaction is a string")
            .to_owned();
        let remove = *remove;
        let session = session_id.to_string();
        let state = Arc::clone(state);
        tokio::task::spawn_blocking(move || {
            crate::config::commit_config_change(&state, |config| {
                let mut next = config.clone();
                let messages = next.ai_chat_message_reactions.entry(session).or_default();
                let marks = messages.entry(request.message_id.clone()).or_default();
                marks.retain(|mark| mark != &reaction);
                if !remove {
                    marks.push(reaction);
                }
                if marks.is_empty() {
                    messages.remove(&request.message_id);
                }
                Ok(next)
            })
        })
        .await
        .map_err(|error| {
            AcpClientError::invalid_input(format!(
                "Action accepted, but saving its mark failed: {error}. Do not resend."
            ))
        })?
        .map_err(|error| {
            AcpClientError::invalid_input(format!(
                "Action accepted, but saving its mark failed: {error}. Do not resend."
            ))
        })?;
    }
    Ok(MessageActionReceipt {
        turn_id,
        reactions: reactions(state, &session_id.to_string()),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    // Catches: a bare excerpt associating identical answers with the wrong message, or removal becoming a new vote.
    #[test]
    fn conversational_action_names_exact_message_and_removal_without_authorization() {
        for remove in [false, true] {
            let request = MessageActionRequest {
                message_id: "native-message-42".into(),
                text: "The same answer.".into(),
                action: MessageAction::Reaction {
                    reaction: Reaction::Heart,
                    remove,
                },
            };
            let text = prompt(&request).unwrap();
            let payload: serde_json::Value =
                serde_json::from_str(text.lines().nth(1).unwrap()).unwrap();
            assert_eq!(payload["messageId"], "native-message-42");
            assert_eq!(payload["sourceExcerpt"], "The same answer.");
            assert_eq!(payload["action"]["reaction"], "heart");
            assert_eq!(payload["action"]["remove"], remove);
            assert!(text.contains("conversational, not an evaluation"));
            assert!(text.contains("not authorization"));
        }
    }

    // Catches: comments being dropped or having special characters change the action envelope.
    #[test]
    fn conversational_comment_preserves_text_and_refuses_anonymous_targets() {
        let mut request = MessageActionRequest {
            message_id: "id".into(),
            text: "answer".into(),
            action: MessageAction::Comment {
                comment: "Use \"this\"\nnext.".into(),
            },
        };
        let text = prompt(&request).unwrap();
        let payload: serde_json::Value =
            serde_json::from_str(text.lines().nth(1).unwrap()).unwrap();
        assert_eq!(payload["action"]["comment"], "Use \"this\"\nnext.");
        request.message_id.clear();
        assert!(prompt(&request).is_err());
    }
    // Catches: marks existing only in one webview, removals being forgotten on restart,
    // or an offline action changing marks despite never entering the original conversation queue.
    #[tokio::test]
    #[serial_test::serial]
    async fn conversational_reactions_persist_remove_and_refuse_offline_without_mutation() {
        let dir = tempfile::tempdir_in(crate::test_support::test_temp_root()).unwrap();
        let _config = crate::config::set_config_dir_override(dir.path().join("config"));
        for file in ["ego-initialize.json", "prompt-second-turn.jsonl"] {
            std::fs::copy(
                format!("tests/fixtures/acp/{file}"),
                dir.path().join(if file.ends_with("jsonl") {
                    "scenario.jsonl"
                } else {
                    file
                }),
            )
            .unwrap();
        }
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
        let state = Arc::new(crate::state::tests_support::make_test_app_state());
        let connection = state
            .acp
            .connect(
                &crate::acp::EgoAcpConfig {
                    executable,
                    profile: String::new(),
                    master_prompt: String::new(),
                    ai_chat: false,
                },
                crate::acp::AcpConnectRequest {
                    root: dir.path().to_owned(),
                    env: None,
                },
            )
            .await
            .unwrap();
        let session = state
            .acp
            .new_session(
                connection.connection_id,
                crate::acp::AcpSessionAuthority {
                    cwd: dir.path().to_owned(),
                    additional_directories: vec![],
                    mcp_servers: vec![],
                },
            )
            .await
            .unwrap()
            .session_id;
        let request = |remove| MessageActionRequest {
            message_id: "exact-source".into(),
            text: "Earlier response".into(),
            action: MessageAction::Reaction {
                reaction: Reaction::Celebrate,
                remove,
            },
        };
        let added = send(
            &state,
            connection.connection_id,
            session.clone(),
            request(false),
        )
        .await
        .unwrap();
        assert_eq!(added.reactions["exact-source"], ["celebrate"]);
        let restored = crate::state::tests_support::make_test_app_state();
        *restored.config.write() = crate::config::load_app_config();
        assert_eq!(
            reactions(&restored, &session.to_string())["exact-source"],
            ["celebrate"]
        );
        send(
            &state,
            connection.connection_id,
            session.clone(),
            request(true),
        )
        .await
        .unwrap();
        assert!(reactions(&state, &session.to_string()).is_empty());
        assert!(
            crate::config::load_app_config().ai_chat_message_reactions[&session.to_string()]
                .is_empty()
        );
        state
            .acp
            .disconnect(connection.connection_id)
            .await
            .unwrap();
        assert!(
            send(
                &state,
                connection.connection_id,
                session.clone(),
                request(false)
            )
            .await
            .is_err()
        );
        assert!(reactions(&state, &session.to_string()).is_empty());
        assert!(reactions(&state, "different-conversation").is_empty());
    }
}
