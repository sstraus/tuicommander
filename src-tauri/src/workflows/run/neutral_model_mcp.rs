//! A per-model catalogue and dispatch boundary; never forwards to ordinary MCP.
use super::neutral_effects::{EffectKey, EffectOutcome};
use super::neutral_execution::NeutralResult;
use super::neutral_model_tools::{ModelSession, ModelToolCall, ModelToolPolicy};
use super::runtime::RuntimeOwner;
use crate::acp::{AcpMcpOrigin, McpNotify, McpOverAcpError, McpOverAcpHost, McpReply};
use crate::workflows::catalog::{ObjectSchema, ValueType};
use parking_lot::Mutex;
use serde_json::{Map, Value, json};
use std::collections::BTreeMap;
use std::sync::{Arc, Weak};

#[derive(Clone)]
pub(super) struct ModelMcpHost {
    owner: Weak<RuntimeOwner>,
    parent: EffectKey,
    policy: ModelToolPolicy,
    connections: Arc<Mutex<BTreeMap<String, AcpMcpOrigin>>>,
}
fn refused(message: impl Into<String>) -> McpOverAcpError {
    McpOverAcpError::new(-32602, message)
}
impl ModelMcpHost {
    pub fn new(owner: &Arc<RuntimeOwner>, parent: EffectKey) -> Result<Arc<Self>, String> {
        let run = owner.store.neutral_execution(&parent.fence.run)?;
        let effect = run.effect(&parent)?;
        if !effect.dispatched || effect.tool.is_some() {
            return Err("model MCP requires its dispatched parent effect".into());
        }
        let policy = effect
            .preparation
            .model_tools
            .clone()
            .ok_or("model tool policy missing")?;
        policy.validate()?;
        Ok(Arc::new(Self {
            owner: Arc::downgrade(owner),
            parent,
            policy,
            connections: Arc::new(Mutex::new(BTreeMap::new())),
        }))
    }

    fn session(&self, origin: &AcpMcpOrigin) -> Result<ModelSession, McpOverAcpError> {
        let owner = self
            .owner
            .upgrade()
            .ok_or_else(|| refused("model owner unavailable"))?;
        let run = owner
            .store
            .neutral_execution(&self.parent.fence.run)
            .map_err(refused)?;
        let parent = run.effect(&self.parent).map_err(refused)?;
        let session = parent
            .model_session
            .clone()
            .ok_or_else(|| refused("model session not bound"))?;
        if session.connection != origin.connection_id.to_string()
            || session.generation != origin.generation
        {
            return Err(refused("model MCP origin differs from bound session"));
        }
        Ok(session)
    }

    async fn call(
        &self,
        origin: &AcpMcpOrigin,
        params: Map<String, Value>,
    ) -> Result<Value, McpOverAcpError> {
        if params
            .keys()
            .any(|key| !matches!(key.as_str(), "name" | "arguments" | "_meta"))
            || serde_json::to_vec(&params)
                .map_err(|e| refused(e.to_string()))?
                .len()
                > 1_048_576
        {
            return Err(refused(
                "model tool arguments exceed the strict request contract",
            ));
        }
        let name = params
            .get("name")
            .and_then(Value::as_str)
            .ok_or_else(|| refused("tool name required"))?;
        if !self.policy.tools.contains_key(name) {
            return Err(refused("tool not admitted by model policy"));
        }
        let id = params
            .get("_meta")
            .and_then(|v| v.get("tuic/call_id"))
            .and_then(Value::as_str)
            .filter(|id| {
                id.len() == 64
                    && id
                        .bytes()
                        .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
            })
            .ok_or_else(|| refused("stable lowercase SHA256 tuic/call_id required"))?;
        let arguments = serde_json::from_value::<BTreeMap<String, Value>>(
            params
                .get("arguments")
                .cloned()
                .ok_or_else(|| refused("tool arguments object required"))?,
        )
        .map_err(|e| refused(e.to_string()))?;
        let call = ModelToolCall {
            parent: self.parent.clone(),
            session: self.session(origin)?,
            id: id.into(),
            name: name.into(),
        };
        let owner = self
            .owner
            .upgrade()
            .ok_or_else(|| refused("model owner unavailable"))?;
        let key = owner
            .start_model_tool(call, arguments)
            .map_err(refused)?
            .key;
        loop {
            // Register before reading SQLite, so a commit between read and await cannot be lost.
            let changed = owner.neutral_effect_updates.notified();
            tokio::pin!(changed);
            changed.as_mut().enable();
            let run = owner
                .store
                .neutral_execution(&self.parent.fence.run)
                .map_err(refused)?;
            let effect = run.effect(&key).map_err(refused)?;
            if let Some(observation) = &effect.observation {
                match &observation.outcome {
                    EffectOutcome::Completed { result } if observation.stopped => {
                        return Ok(
                            json!({"content":[{"type":"text","text":serde_json::to_string(result)
                            .map_err(|e| refused(e.to_string()))?}],"isError":matches!(result, NeutralResult::Failed { .. })}),
                        );
                    }
                    EffectOutcome::Uncertain { diagnostic } => {
                        return Err(refused(format!(
                            "original tool effect uncertain: {diagnostic}"
                        )));
                    }
                    _ => {}
                }
            }
            changed.await;
            self.session(origin)?;
        }
    }
}

impl McpOverAcpHost for ModelMcpHost {
    fn has_inbox_subscriber(&self, _: &str) -> bool {
        false
    }
    fn connect(&self, _: Option<&str>, _: McpNotify) -> Result<String, McpOverAcpError> {
        Err(refused("model MCP requires host-issued ACP origin"))
    }
    fn connect_scoped(
        &self,
        peer: Option<&str>,
        _: McpNotify,
        origin: Option<&AcpMcpOrigin>,
    ) -> Result<String, McpOverAcpError> {
        let origin = origin
            .filter(|o| peer.is_some() && o.peer_id.as_deref() == peer && o.generation != 0)
            .ok_or_else(|| refused("model MCP origin missing or inconsistent"))?;
        let mut connections = self.connections.lock();
        if connections.values().any(|old| old != origin) {
            return Err(refused("model MCP cannot change connection generation"));
        }
        let id = uuid::Uuid::new_v4().to_string();
        connections.insert(id.clone(), origin.clone());
        Ok(id)
    }
    fn message(
        &self,
        connection: &str,
        method: String,
        params: Option<Map<String, Value>>,
    ) -> McpReply {
        let origin = self.connections.lock().get(connection).cloned();
        let host = self.clone();
        Box::pin(async move {
            let origin = origin.ok_or_else(|| refused("unknown model MCP connection"))?;
            // ego speaks only the 2026-07-28 lifecycle and refuses a server that does not
            // answer `server/discover` or omits the result envelope.
            match method.as_str() {
                "server/discover" => Ok(json!({
                    "resultType":"complete","supportedVersions":["2026-07-28"],
                    "capabilities":{"tools":{"listChanged":false}},
                    "ttlMs":0,"cacheScope":"private",
                })),
                "tools/list" => {
                    let mut list = json!({"tools":host.policy.tools.iter().map(|(name,binding)| json!({
                        "name":name,"description":binding.descriptor.purpose,
                        "inputSchema":input_schema(&binding.descriptor.inputs),
                    })).collect::<Vec<_>>(),"nextCursor":null});
                    crate::mcp_http::mcp_transport::add_result_envelope(&mut list);
                    Ok(list)
                }
                "tools/call" => {
                    let mut result = host
                        .call(
                            &origin,
                            params.ok_or_else(|| refused("tool call parameters required"))?,
                        )
                        .await?;
                    result["resultType"] = json!("complete");
                    Ok(result)
                }
                _ => Err(McpOverAcpError::new(
                    -32601,
                    "method unavailable on model MCP",
                )),
            }
        })
    }
    fn notification(&self, _: &str, _: String, _: Option<Map<String, Value>>) {}
    fn disconnect(&self, connection: &str) {
        self.connections.lock().remove(connection);
    }
    /// Every call goes through start_model_tool, which enforces the retained policy.
    fn preapproved_tools(&self) -> Vec<String> {
        self.policy.tools.keys().cloned().collect()
    }
}

fn input_schema(schema: &ObjectSchema) -> Value {
    let properties: Map<String, Value> = schema
        .fields
        .iter()
        .map(|(name, field)| {
            let kind = match field.value_type {
                ValueType::String => "string",
                ValueType::Boolean => "boolean",
                ValueType::Integer => "integer",
                ValueType::Number => "number",
                ValueType::Null => "null",
                ValueType::Array => "array",
                ValueType::Object
                | ValueType::WorkspaceReference
                | ValueType::ArtifactReference
                | ValueType::CredentialReference => "object",
            };
            (name.clone(), json!({"type":kind}))
        })
        .collect();
    json!({"type":"object","properties":properties,"additionalProperties":false,
        "required":schema.fields.iter().filter(|(_,f)| f.required && f.default.is_none()).map(|(n,_)| n).collect::<Vec<_>>()})
}
