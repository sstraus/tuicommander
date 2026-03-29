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

/// The `keys` object inside a browser PushSubscription.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct PushSubscriptionKeys {
    pub p256dh: String,
    pub auth: String,
}

/// Browser push subscription from PushManager.subscribe().toJSON().
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct PushSubscription {
    pub endpoint: String,
    pub keys: PushSubscriptionKeys,
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
