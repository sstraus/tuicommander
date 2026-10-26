//! WebSocket link to the relay room: the byte pipe under the Noise channel.
//!
//! Binary messages are sealed records and go to the multiplexer unchanged. Text
//! messages come from the relay itself (`relay:status`) and never reach the
//! multiplexer: the relay is not authenticated, so nothing it says can be
//! trusted as more than a hint about the other peer's presence.

use std::time::Duration;

use futures_util::{SinkExt, StreamExt};
use serde::Deserialize;
use tokio::sync::mpsc;
use tokio_tungstenite::tungstenite::Message;
use tokio_tungstenite::tungstenite::protocol::WebSocketConfig;

use super::bundle::RelayBearer;

/// Larger than any sealed record (`secure::MAX_RECORD_PLAINTEXT` plus the tag);
/// the relay closes a connection above 1 MiB anyway.
const MAX_WS_MESSAGE: usize = 64 * 1024;
const CONNECT_TIMEOUT: Duration = Duration::from_secs(15);
const LINK_QUEUE: usize = 32;

#[derive(Debug)]
pub(crate) enum LinkError {
    Connect(String),
    Timeout,
}

impl std::fmt::Display for LinkError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Connect(why) => write!(f, "relay connection failed: {why}"),
            Self::Timeout => f.write_str("relay connection timed out"),
        }
    }
}

impl std::error::Error for LinkError {}

/// What the relay says about the other end of the room.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Presence {
    Waiting,
    Connected,
    /// The other peer left, or was reaped.
    Gone,
}

/// What a link delivers. Presence travels in the same queue as the records so a
/// consumer sees "peer joined" strictly before that peer's first record, and
/// "peer left" strictly after its last.
#[derive(Debug, PartialEq, Eq)]
pub(crate) enum LinkItem {
    Record(Vec<u8>),
    Peer(Presence),
}

#[derive(Deserialize)]
#[serde(tag = "type")]
enum RelayText {
    #[serde(rename = "relay:status")]
    Status { peer: String },
    #[serde(other)]
    Other,
}

pub(crate) struct WsLink {
    /// Sealed records to send. Dropping it closes the WebSocket.
    pub tx: mpsc::Sender<Vec<u8>>,
    /// Records and peer presence. Closes when the WebSocket ends.
    pub rx: mpsc::Receiver<LinkItem>,
}

/// `{relay_url}/ws/{room}`, authenticated by `Bearer R` as the first text frame.
pub(crate) async fn connect(
    relay_url: &url::Url,
    room: &str,
    bearer: &RelayBearer,
) -> Result<WsLink, LinkError> {
    let ws_url = format!("{}/ws/{room}", relay_url.as_str().trim_end_matches('/'));
    let config = WebSocketConfig::default()
        .max_message_size(Some(MAX_WS_MESSAGE))
        .max_frame_size(Some(MAX_WS_MESSAGE));
    // The error text of a failed connect can echo the URL; the URL holds the
    // room id only, never R or the pairing secret.
    let (ws, _) = tokio::time::timeout(
        CONNECT_TIMEOUT,
        tokio_tungstenite::connect_async_with_config(ws_url, Some(config), false),
    )
    .await
    .map_err(|_| LinkError::Timeout)?
    .map_err(|e| LinkError::Connect(e.to_string()))?;
    let (mut sink, mut source) = ws.split();
    sink.send(Message::Text(format!("Bearer {}", bearer.expose()).into()))
        .await
        .map_err(|e| LinkError::Connect(e.to_string()))?;

    let (out_tx, mut out_rx) = mpsc::channel::<Vec<u8>>(LINK_QUEUE);
    let (in_tx, in_rx) = mpsc::channel::<LinkItem>(LINK_QUEUE);

    tokio::spawn(async move {
        loop {
            tokio::select! {
                outgoing = out_rx.recv() => match outgoing {
                    Some(record) => {
                        if sink.send(Message::Binary(record.into())).await.is_err() {
                            break;
                        }
                    }
                    None => {
                        let _ = sink.close().await;
                        break;
                    }
                },
                incoming = source.next() => match incoming {
                    Some(Ok(Message::Binary(record))) => {
                        if in_tx.send(LinkItem::Record(record.to_vec())).await.is_err() {
                            break;
                        }
                    }
                    Some(Ok(Message::Text(text))) => {
                        if let Ok(RelayText::Status { peer }) = serde_json::from_str(&text) {
                            let presence = match peer.as_str() {
                                "connected" => Presence::Connected,
                                "waiting" => Presence::Waiting,
                                _ => Presence::Gone,
                            };
                            if in_tx.send(LinkItem::Peer(presence)).await.is_err() {
                                break;
                            }
                        }
                    }
                    Some(Ok(Message::Ping(_) | Message::Pong(_) | Message::Frame(_))) => {}
                    Some(Ok(Message::Close(_))) | Some(Err(_)) | None => break,
                },
            }
        }
    });

    Ok(WsLink {
        tx: out_tx,
        rx: in_rx,
    })
}

#[cfg(test)]
mod tests;
