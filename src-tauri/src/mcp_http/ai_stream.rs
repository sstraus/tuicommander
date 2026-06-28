//! Dedicated per-id WebSocket bridges for the high-frequency AI token streams —
//! event-bridge plan Steps 3 (conversation) & 4 (chat). These deliberately do
//! NOT ride the global `event_bus`/SSE: a single conversation emits 20+ events/sec,
//! which would exhaust the 256-cap broadcast and Lag unrelated SSE consumers.
//! Each connection taps the same engine stream the desktop Tauri Channel uses, so
//! browser/PWA clients get byte-identical `ConversationEvent`/`ChatEvent` frames.

use std::sync::Arc;

use axum::extract::ws::{Message, WebSocket, WebSocketUpgrade};
use axum::extract::{Path, State};
use axum::response::Response;
use futures_util::stream::{SplitSink, StreamExt};
use serde::{Deserialize, Serialize};

use crate::AppState;
use crate::ai_agent::conversation_engine::{
    ConversationEvent, batched_conversation_stream, build_config,
    start_conversation as engine_start, subscribe_conversation,
};
use crate::ai_chat_registry::{ChatEvent, chat_registry, validate_id};

/// Serialize a frame to JSON and push it down the socket. Returns false if the
/// client has disconnected. Shared by the conversation and chat bridges.
async fn send_json<T: Serialize>(sink: &mut SplitSink<WebSocket, Message>, frame: &T) -> bool {
    let Ok(json) = serde_json::to_string(frame) else {
        // Skip an unserializable frame but keep the stream alive — log it so a
        // silent gap in the client's stream is at least visible server-side.
        tracing::warn!("ai_stream: failed to serialize frame, skipping");
        return true;
    };
    futures_util::SinkExt::send(sink, Message::Text(json.into()))
        .await
        .is_ok()
}

/// Start params for a conversation stream — mirrors the `start_conversation`
/// Tauri command args (minus `sessionId`, which is in the path, and `onEvent`,
/// which is the WebSocket itself). Sent by the client as the first text frame.
#[derive(Deserialize)]
struct StartConversationParams {
    message: String,
    autonomy: Option<String>,
    #[serde(rename = "maxSteps")]
    max_steps: Option<usize>,
    temperature: Option<f32>,
    #[serde(rename = "modelOverride")]
    model_override: Option<String>,
    #[serde(rename = "bypassedTools")]
    bypassed_tools: Option<Vec<String>>,
    #[serde(rename = "reasoningEffort")]
    reasoning_effort: Option<String>,
}

/// `GET /ai/conversation/{session_id}/stream` — WebSocket upgrade.
pub(super) async fn conversation_ws(
    ws: WebSocketUpgrade,
    Path(session_id): Path<String>,
    State(state): State<Arc<AppState>>,
) -> Response {
    ws.on_upgrade(move |socket| bridge_conversation(socket, session_id, state))
}

async fn bridge_conversation(socket: WebSocket, session_id: String, state: Arc<AppState>) {
    let (mut sink, mut stream) = socket.split();

    // Validate the session id up front (mirrors `bridge_chat`'s `validate_id`).
    if let Err(e) = validate_id(&session_id) {
        let _ = send_json(&mut sink, &ConversationEvent::Error { message: e }).await;
        return;
    }

    // First frame carries the start params (atomic start+subscribe — no
    // POST-then-subscribe race where early TextChunks would be missed).
    let params: StartConversationParams = match stream.next().await {
        Some(Ok(Message::Text(t))) => match serde_json::from_str(&t) {
            Ok(p) => p,
            Err(e) => {
                let _ = send_json(
                    &mut sink,
                    &ConversationEvent::Error {
                        message: format!("invalid start params: {e}"),
                    },
                )
                .await;
                return;
            }
        },
        _ => return, // closed before sending params
    };

    let config = build_config(
        params.autonomy,
        params.max_steps,
        params.temperature,
        params.model_override,
        params.bypassed_tools,
        params.reasoning_effort,
    )
    .await;

    let rx = match engine_start(state, session_id.clone(), params.message, config).await {
        Ok(rx) => rx,
        // A conversation is already running on this session — re-attach to its
        // live stream instead of erroring. The reconnecting client keeps its own
        // transcript, so live events (no backfill) resume the stream. If the
        // conversation ended in the race between start and subscribe, surface the
        // original start error.
        Err(e) => match subscribe_conversation(&session_id) {
            Some(rx) => rx,
            None => {
                let _ = send_json(&mut sink, &ConversationEvent::Error { message: e }).await;
                return;
            }
        },
    };

    // Same 50ms batcher the desktop Channel bridge uses. A client disconnect
    // surfaces as a send error; we stop forwarding but leave the conversation
    // running (matches desktop: closing the panel doesn't cancel — use the
    // explicit cancel endpoint). The batcher task ends when its mpsc is dropped.
    let mut batched = batched_conversation_stream(rx);
    while let Some(ev) = batched.recv().await {
        if !send_json(&mut sink, &ev).await {
            break;
        }
    }
}

// ── Chat registry stream (event-bridge plan Step 4) ────────────────────

/// `GET /ai/chat/{chat_id}/stream` — WebSocket upgrade. Mirrors the desktop
/// `chat_subscribe` command: the first frame is a `ChatEvent::Snapshot`, then
/// live `ChatEvent`s (chunk/error/cleared/snapshot) as they are fanned out.
/// Closing the socket unsubscribes (no explicit `chat_unsubscribe` needed).
pub(super) async fn chat_ws(ws: WebSocketUpgrade, Path(chat_id): Path<String>) -> Response {
    ws.on_upgrade(move |socket| bridge_chat(socket, chat_id))
}

async fn bridge_chat(socket: WebSocket, chat_id: String) {
    let (mut sink, mut stream) = socket.split();

    if let Err(e) = validate_id(&chat_id) {
        let _ = send_json(&mut sink, &ChatEvent::Error { message: e }).await;
        return;
    }

    let reg = chat_registry();
    let (result, mut rx) = reg.subscribe_ws(&chat_id).await;

    // First frame: the state snapshot (carries `kind: "snapshot"` so the client's
    // applyRegistryEvent handles it like any other event).
    if !send_json(&mut sink, &ChatEvent::Snapshot(result.snapshot)).await {
        reg.unsubscribe(&chat_id, result.subscription_id).await;
        return;
    }

    // Forward fan-out events; also watch the socket so an idle chat still detects
    // a client disconnect promptly (send-failure alone wouldn't fire until the
    // next event, which may be far off).
    loop {
        tokio::select! {
            maybe = rx.recv() => match maybe {
                Some(ev) => {
                    if !send_json(&mut sink, &ev).await {
                        break;
                    }
                }
                None => break, // registry dropped the sender
            },
            incoming = stream.next() => match incoming {
                Some(Ok(Message::Close(_))) | None => break,
                Some(Err(_)) => break,
                Some(Ok(_)) => {} // ignore other client→server frames
            },
        }
    }

    reg.unsubscribe(&chat_id, result.subscription_id).await;
}
