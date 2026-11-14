//! Typed ego extensions outside standard ACP.
//!
//! ACP has no way to hold a turn at a boundary and no way to compact a session
//! into a successor, so ego adds `_ego/pause`, `_ego/resume` and
//! `_ego/compact` under the extension namespace the protocol reserves for
//! exactly this. They are typed here rather than sent as loose JSON because
//! they are a contract with one specific agent at one specific version, and a
//! contract that only exists as a `json!` literal at the call site is one
//! nothing checks.
//!
//! The wire shapes are ego's, read from `AcpHoldRequest`, `AcpHoldResponse`,
//! `AcpCompactRequest` and `AcpCompactResponse` in that repository's
//! `crates/ego-acp/src/serve.rs`. Ego's request structs carry
//! `deny_unknown_fields`, so an extra field here is a rejected call, not a
//! tolerated one.

use agent_client_protocol::schema::v1;
use agent_client_protocol::{Error, JsonRpcMessage, JsonRpcRequest, UntypedMessage};
use serde::{Deserialize, Serialize};

use super::{
    EGO_COMPACT_METHOD, EGO_PAUSE_METHOD, EGO_RESUME_METHOD, EgoCompactResponse, EgoHoldResponse,
};

/// Ego reports a session it could not set up as a config update with
/// `_meta.ego.configOptions.state = "failed"` and its JSON-RPC error in
/// `_meta.ego.error` (ego-acp serve.rs `failed_config_options`). No offers
/// follow it, so a caller that waits for one only reaches its deadline and
/// loses ego's reason. Returns that reason; internal errors carry it in `data`.
/// The actor keeps it in the attachment snapshot, so a waiter that starts after
/// the update still reads it.
pub(super) fn failed_configuration(options: &v1::ConfigOptionUpdate) -> Option<String> {
    let ego = options.meta.as_ref()?.get("ego")?;
    if ego["configOptions"]["state"] != "failed" {
        return None;
    }
    let error = &ego["error"];
    let mut reason = error["message"]
        .as_str()
        .unwrap_or("no error message")
        .to_owned();
    if let Some(data) = error.get("data").filter(|data| !data.is_null()) {
        let data = data
            .as_str()
            .map_or_else(|| data.to_string(), str::to_owned);
        reason = format!("{reason}: {data}");
    }
    Some(format!("ego session setup failed: {reason}"))
}

/// The version of the ego extensions this client speaks, and only this one.
///
/// Not a range and not a minimum. A host that guessed forwards would be
/// asserting what a version it has never seen means, which is the one thing an
/// extension version exists to stop.
pub(super) const EGO_EXTENSION_VERSION: u32 = 1;

/// One `_ego/pause` or `_ego/resume`.
///
/// One type for both, as in ego, because they carry the same three fields and
/// differ only in which method names them. Two structs would be two places to
/// keep the version rule in step.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct EgoHoldWire<const RELEASE: bool> {
    pub v: u32,
    pub session_id: String,
    pub request_id: String,
}

const fn hold_method(release: bool) -> &'static str {
    if release {
        EGO_RESUME_METHOD
    } else {
        EGO_PAUSE_METHOD
    }
}

impl<const RELEASE: bool> JsonRpcMessage for EgoHoldWire<RELEASE> {
    fn matches_method(method: &str) -> bool {
        method == hold_method(RELEASE)
    }

    fn method(&self) -> &str {
        hold_method(RELEASE)
    }

    fn to_untyped_message(&self) -> Result<UntypedMessage, Error> {
        UntypedMessage::new(self.method(), self)
    }

    fn parse_message(method: &str, params: &impl Serialize) -> Result<Self, Error> {
        if !Self::matches_method(method) {
            return Err(Error::method_not_found());
        }
        agent_client_protocol::util::json_cast_params(params)
    }
}

impl<const RELEASE: bool> JsonRpcRequest for EgoHoldWire<RELEASE> {
    type Response = EgoHoldResponse;
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct EgoCompactWire {
    pub v: u32,
    pub session_id: String,
    pub request_id: String,
}

impl JsonRpcMessage for EgoCompactWire {
    fn matches_method(method: &str) -> bool {
        method == EGO_COMPACT_METHOD
    }

    fn method(&self) -> &str {
        EGO_COMPACT_METHOD
    }

    fn to_untyped_message(&self) -> Result<UntypedMessage, Error> {
        UntypedMessage::new(self.method(), self)
    }

    fn parse_message(method: &str, params: &impl Serialize) -> Result<Self, Error> {
        if !Self::matches_method(method) {
            return Err(Error::method_not_found());
        }
        agent_client_protocol::util::json_cast_params(params)
    }
}

impl JsonRpcRequest for EgoCompactWire {
    type Response = EgoCompactResponse;
}

/// Text submitted to the current turn; acceptance is durable and must not be retried.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct EgoSteerWire {
    pub v: u32,
    pub session_id: String,
    pub prompt: Vec<agent_client_protocol::schema::v1::ContentBlock>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub(super) enum EgoSteerState {
    Accepted,
    NotBusy,
    Rejected,
}

#[derive(Debug, Clone, Serialize, Deserialize, agent_client_protocol::JsonRpcResponse)]
pub(super) struct EgoSteerResponse {
    pub v: u32,
    pub state: EgoSteerState,
}

impl JsonRpcMessage for EgoSteerWire {
    fn matches_method(method: &str) -> bool {
        method == super::EGO_STEER_METHOD
    }
    fn method(&self) -> &str {
        super::EGO_STEER_METHOD
    }
    fn to_untyped_message(&self) -> Result<UntypedMessage, Error> {
        UntypedMessage::new(self.method(), self)
    }
    fn parse_message(method: &str, params: &impl Serialize) -> Result<Self, Error> {
        if !Self::matches_method(method) {
            return Err(Error::method_not_found());
        }
        agent_client_protocol::util::json_cast_params(params)
    }
}
impl JsonRpcRequest for EgoSteerWire {
    type Response = EgoSteerResponse;
}
