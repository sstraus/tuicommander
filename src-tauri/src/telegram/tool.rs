use super::{
    Error,
    runtime::{Command, HANDLE},
};
use crate::state::AppState;
use serde::Deserialize;
use serde_json::{Value, json};
use std::sync::Arc;

#[derive(Deserialize)]
#[serde(tag = "action", rename_all = "snake_case", deny_unknown_fields)]
pub(super) enum Input {
    Register {},
    Unregister {},
    Begin {
        request_id: String,
    },
    Activity {
        request_id: String,
        text: String,
    },
    Finish {
        request_id: String,
        text: String,
    },
    Send {
        text: String,
        #[serde(default)]
        buttons: Vec<Vec<Button>>,
    },
}
#[derive(Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct Button {
    pub label: String,
    pub data: String,
}

pub(crate) fn definition() -> Value {
    json!({"name":"telegram","description":"Call register to opt your own MCP-bound TUIC agent into Telegram; it replaces the previous agent and sends it one TUIC notice. unregister releases your registration. Registration ends on agent exit, PTY close, MCP session end or daemon restart. Request-specific begin/activity/finish stay bound to the peer that received the request, even if another AI Chat connects. Explicit registration takeover revokes pending requests. Any live MCP-bound agent may send without registration or changing inbound ownership. Non-owner sends carry a server-derived [session name] prefix on every chunk, allow no buttons, and require a 30-second per-peer interval (telegram_sender_rate_limited). Owner sends have no prefix and preserve the authored text exactly. Reply to a prefixed message routes to the unique live peer with that exact name; absent, dead or ambiguous names fall back to the inbound owner. Names must be non-empty, at most 128 UTF-16 units, without brackets or control characters. With no registered agent, allowed inbound text goes to the latest-connected live AI Chat agent. Only when neither exists is text dropped after the bot replies No agent is listening. Ask an agent in TUICommander to register for Telegram.. TUIC acknowledges Telegram mail and opens its status draft automatically. begin is optional and idempotent for the current request, for PTY agents and AI Chat without a PTY. activity sends concise safe unapproved preview text. finish is authoritative: it sends the complete exact reply even after the draft or turn ended, and deduplicates completed requests. AI Chat streams only assistant message text; a Telegram-mail-woken turn settles to its own final text unless finish already replied. send uses the single allowlisted outbound destination with optional rows of {label,data} opaque buttons. Buttons return structured peer mail and confer no publish approval. No raw terminal output or hidden reasoning is streamed.","inputSchema":{"type":"object","properties":{
        "action":{"type":"string","enum":["register","unregister","begin","activity","finish","send"]},
        "request_id":{"type":"string"},"text":{"type":"string"},
        "buttons":{"type":"array","items":{"type":"array","items":{"type":"object","properties":{"label":{"type":"string"},"data":{"type":"string"}},"required":["label","data"],"additionalProperties":false}}}
    },"required":["action"],"additionalProperties":false}})
}

pub(crate) async fn handle(state: &Arc<AppState>, args: &Value, sid: Option<&str>) -> Value {
    let result = async {
        let caller = sid
            .and_then(|sid| state.mcp.to_session.get(sid).map(|v| v.value().clone()))
            .ok_or(Error::State)?;
        let caller = state
            .resolve_peer_ref_checked(&caller)
            .map_err(|_| Error::State)?
            .ok_or(Error::State)?;
        let input: Input = serde_json::from_value(args.clone()).map_err(|_| Error::Config)?;
        let sender = HANDLE.get().ok_or(Error::State)?;
        let (reply, receive) = tokio::sync::oneshot::channel();
        sender
            .try_send(Command::Tool {
                caller,
                sid: sid.ok_or(Error::State)?.to_string(),
                input,
                reply,
            })
            .map_err(|_| Error::Capacity)?;
        receive.await.map_err(|_| Error::State)?
    }
    .await;
    match result {
        Ok(value) => value,
        Err(error) => json!({"error":error.to_string()}),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    // Catches: an agent parameter creates approval authority or smuggles an
    // arbitrary callback schema into a send operation.
    #[tokio::test]
    async fn tool_rejects_forged_authority_and_malformed_button_fields() {
        use crate::telegram::tests::{FakeServer, setup};
        use axum::http::StatusCode;
        let config_dir = tempfile::tempdir_in(crate::test_support::test_temp_root()).unwrap();
        let _guard = crate::config::set_config_dir_override(config_dir.path().to_path_buf());
        let (setup_dir, _) = setup();
        // Dispatch availability and the runtime must read the same private setup.
        std::fs::rename(setup_dir.path(), config_dir.path().join("telegram")).unwrap();
        let paths = crate::telegram::settings::paths().unwrap();
        let server = FakeServer::start(vec![
            (StatusCode::OK, json!({"ok":true,"result":{"message_id":7}})),
            (StatusCode::OK, json!({"ok":true,"result":{"message_id":8}})),
        ])
        .await;
        let runtime = crate::telegram::tests::outbound_tests::runtime(paths, server.address).await;
        let state = runtime.state.clone();
        let (commands, receive) = tokio::sync::mpsc::channel(10);
        HANDLE.set(commands).expect("one native Telegram adapter");
        let worker = tokio::spawn(runtime.run(receive));
        let invoke = |value: Value| {
            let state = state.clone();
            async move {
                crate::mcp_http::mcp_transport::handle_mcp_tool_call(
                    &state,
                    "127.0.0.1:0".parse().unwrap(),
                    "telegram",
                    &value,
                    Some("target-mcp"),
                )
                .await
            }
        };
        assert_eq!(
            invoke(json!({"action":"unregister"})).await["registered"],
            false
        );
        assert_eq!(
            invoke(json!({"action":"send","text":"before registration"})).await["message_ids"],
            json!([7])
        );
        assert_eq!(
            invoke(json!({"action":"register"})).await["registered"],
            true
        );
        // The same authenticated dispatch reaches real outbound delivery.
        assert_eq!(
            invoke(json!({"action":"send","text":"hello"})).await["message_ids"],
            json!([8])
        );
        for value in [
            json!({"action":"register","peer":"other"}),
            json!({"action":"unregister","peer":"other"}),
            json!({"action":"register","tuic_session":"other"}),
            json!({"action":"send","text":"hello","approved":true}),
            json!({"action":"begin","request_id":"r","peer":"other"}),
            json!({"action":"send","text":"hello","buttons":[[{"label":"ok","data":"x","approved":true}]]}),
            json!({"action":"send","text":"hello","chat_id":"1111111"}),
        ] {
            let path = if value.get("buttons").is_some() {
                "/buttons/0/0"
            } else {
                ""
            };
            let result = invoke(value).await;
            assert_eq!(
                crate::test_support::mcp_argument_constraint(&result, path, "additionalProperties"),
                &json!(false)
            );
        }
        assert_eq!(server.requests().len(), 2);
        let inbox = crate::mcp_http::mcp_transport::local_peer_call_with_message_id(
            &state,
            &json!({"action":"inbox","since":0}),
            Some("target-mcp"),
            None,
        )
        .await;
        assert_eq!(inbox["count"], 0);
        worker.abort();
    }
}
