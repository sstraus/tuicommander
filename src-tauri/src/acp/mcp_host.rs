//! The `tuicommander` MCP server, served to ego over the ACP connection itself.
//!
//! ego used to reach it through a stdio `tuic-bridge` entry, which opened a
//! fresh HTTP MCP session for every tool operation — 13,409 initializes from one
//! peer on 2026-09-28. MCP-over-ACP (`mcp/connect`, `mcp/message`,
//! `mcp/disconnect`) carries the same protocol on the connection that already
//! exists, so there is no process and no socket in between.
//!
//! The connection layer does not know what the server *is*: the MCP handler
//! lives with the HTTP routes and needs the whole application state, which the
//! manager must not hold. A host is injected instead, and this module is the
//! contract between the two.

use std::collections::HashSet;
use std::future::Future;
use std::pin::Pin;
use std::sync::{Arc, OnceLock};

use agent_client_protocol::schema::v1;
use agent_client_protocol::{Agent, ConnectionTo, Error};
use parking_lot::Mutex;
use serde_json::value::RawValue;
use serde_json::{Map, Value};

/// The `serverId` every session's `tuicommander` entry names.
///
/// One server per connection, so the id only has to be unique on it. The name
/// and the id are the same word on purpose: ego matches tool permissions on the
/// server name, and nothing is gained by making the id say something else.
pub const TUICOMMANDER_MCP_SERVER_ID: &str = "tuicommander";

/// An MCP error, carried back to ego as the ACP error of its `mcp/message`.
#[derive(Debug, Clone, PartialEq)]
pub struct McpOverAcpError {
    pub code: i32,
    pub message: String,
    pub data: Option<Value>,
}

impl McpOverAcpError {
    #[must_use]
    pub fn new(code: i32, message: impl Into<String>) -> Self {
        Self {
            code,
            message: message.into(),
            data: None,
        }
    }
}

/// Send one MCP notification to ego on the connection it was opened on.
pub type McpNotify = Arc<dyn Fn(String, Option<Map<String, Value>>) + Send + Sync>;

/// What an MCP request settles into, off the connection's dispatch loop.
pub type McpReply = Pin<Box<dyn Future<Output = Result<Value, McpOverAcpError>> + Send>>;

/// Whatever serves the `tuicommander` MCP server for an ACP connection.
pub trait McpOverAcpHost: Send + Sync {
    /// Whether this peer has a live MCP connection subscribed to its inbox.
    fn has_inbox_subscriber(&self, peer_id: &str) -> bool;

    /// Open one MCP connection for `peer_id`, the identity ego was launched
    /// under. `notify` reaches ego for as long as the connection lives.
    fn connect(&self, peer_id: Option<&str>, notify: McpNotify) -> Result<String, McpOverAcpError>;

    /// A managed channel supplies its immutable ACP origin. Hosts that do not
    /// authorize workflow reports can retain the ordinary peer-only adapter.
    fn connect_scoped(
        &self,
        peer_id: Option<&str>,
        notify: McpNotify,
        _origin: Option<&super::AcpMcpOrigin>,
    ) -> Result<String, McpOverAcpError> {
        self.connect(peer_id, notify)
    }

    /// Answer one MCP request with its `result`.
    fn message(
        &self,
        connection_id: &str,
        method: String,
        params: Option<Map<String, Value>>,
    ) -> McpReply;

    /// Take one MCP notification. Nothing is sent back.
    fn notification(&self, connection_id: &str, method: String, params: Option<Map<String, Value>>);

    /// Release everything the connection held. Called once per connection,
    /// whether ego disconnected it or the ACP connection ended.
    fn disconnect(&self, connection_id: &str);

    /// Tools whose every call this host already admits through its own ledger.
    /// A mediated launch pre-approves them in ego, which otherwise asks a person
    /// that no unattended turn has and refuses the call.
    fn preapproved_tools(&self) -> Vec<String> {
        Vec::new()
    }
}

/// The MCP connections one ACP connection has open, and the host behind them.
///
/// Shared by the four SDK callbacks and by the supervisor, which releases
/// whatever is still open when the connection ends: ego is not trusted to
/// disconnect, because an ego that crashed never will.
#[derive(Clone)]
pub(super) struct McpChannel {
    host: Option<Arc<dyn McpOverAcpHost>>,
    peer_id: Option<String>,
    origin: Option<super::AcpMcpOrigin>,
    open: Arc<Mutex<HashSet<String>>>,
}

impl McpChannel {
    pub(super) fn new(host: Option<Arc<dyn McpOverAcpHost>>, peer_id: Option<String>) -> Self {
        Self {
            host,
            peer_id,
            origin: None,
            open: Arc::new(Mutex::new(HashSet::new())),
        }
    }

    pub(super) fn with_origin(
        mut self,
        connection_id: super::AcpConnectionId,
        generation: u64,
    ) -> Self {
        self.origin = Some(super::AcpMcpOrigin {
            peer_id: self.peer_id.clone(),
            connection_id,
            generation,
        });
        self
    }

    fn host(&self) -> Result<&Arc<dyn McpOverAcpHost>, Error> {
        self.host
            .as_ref()
            .ok_or_else(|| Error::method_not_found().data("this client serves no MCP over ACP"))
    }

    /// A connection id ego names must be one it opened here.
    fn known(&self, connection_id: &v1::McpConnectionId) -> Result<(), Error> {
        if self.open.lock().contains(connection_id.0.as_ref()) {
            Ok(())
        } else {
            Err(Error::invalid_params().data(format!("no MCP connection {connection_id}")))
        }
    }

    pub(super) fn connect(
        &self,
        request: &v1::ConnectMcpRequest,
        connection: ConnectionTo<Agent>,
    ) -> Result<v1::ConnectMcpResponse, Error> {
        if request.server_id.0.as_ref() != TUICOMMANDER_MCP_SERVER_ID {
            return Err(Error::invalid_params().data(format!(
                "no MCP server {} on this connection",
                request.server_id
            )));
        }
        let host = self.host()?;
        // The notifier exists before the id does, so it learns the id once the
        // host has chosen one; anything sent before that has nowhere to go.
        let named = Arc::new(OnceLock::<String>::new());
        let notify: McpNotify = {
            let named = Arc::clone(&named);
            Arc::new(move |method, params| {
                if let Some(id) = named.get() {
                    let notification =
                        v1::MessageMcpNotification::new(id.clone(), method).params(params);
                    let _ = connection.send_notification(notification);
                }
            })
        };
        let id = host
            .connect_scoped(self.peer_id.as_deref(), notify, self.origin.as_ref())
            .map_err(into_acp)?;
        let _ = named.set(id.clone());
        self.open.lock().insert(id.clone());
        Ok(v1::ConnectMcpResponse::new(id))
    }

    pub(super) fn message(
        &self,
        request: v1::MessageMcpRequest,
    ) -> Result<impl Future<Output = Result<v1::MessageMcpResponse, Error>> + use<>, Error> {
        self.known(&request.connection_id)?;
        let reply = self
            .host()?
            .message(&request.connection_id.0, request.method, request.params);
        Ok(async move {
            let result = reply.await.map_err(into_acp)?;
            let raw =
                RawValue::from_string(result.to_string()).map_err(Error::into_internal_error)?;
            Ok(v1::MessageMcpResponse::new(Arc::from(raw)))
        })
    }

    pub(super) fn notification(&self, notification: v1::MessageMcpNotification) {
        if self.known(&notification.connection_id).is_err() {
            return;
        }
        if let Ok(host) = self.host() {
            host.notification(
                &notification.connection_id.0,
                notification.method,
                notification.params,
            );
        }
    }

    pub(super) fn disconnect(
        &self,
        request: &v1::DisconnectMcpRequest,
    ) -> Result<v1::DisconnectMcpResponse, Error> {
        self.known(&request.connection_id)?;
        self.open.lock().remove(request.connection_id.0.as_ref());
        self.host()?.disconnect(&request.connection_id.0);
        Ok(v1::DisconnectMcpResponse::new())
    }

    /// Release every MCP connection still open, once the ACP connection is gone.
    pub(super) fn close_all(&self) {
        let open: Vec<String> = self.open.lock().drain().collect();
        if let Some(host) = &self.host {
            for id in open {
                host.disconnect(&id);
            }
        }
    }
}

fn into_acp(error: McpOverAcpError) -> Error {
    Error::new(error.code, error.message).data(error.data)
}
