use serde::{Deserialize, Serialize};

/// Relay-generated status messages sent as plaintext JSON to connected peers.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(tag = "type")]
pub enum RelayMessage {
    /// Connection status update from relay to peers.
    #[serde(rename = "relay:status")]
    Status { peer: PeerStatus },

    /// Push notification hint from TUICommander to relay.
    /// Not E2E-encrypted — contains only metadata.
    #[serde(rename = "relay:push")]
    Push {
        reason: String,
        session_name: String,
    },
}

/// Peer connection state as seen by the relay.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum PeerStatus {
    Waiting,
    Connected,
    Disconnected,
    Timeout,
}
