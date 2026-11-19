//! Ego-owned configuration: only its managed CLI mutates the store. Never read its files.
use super::{
    ego_schema::{self, Managed},
    request::*,
    service::Store,
    transaction::{self, Prepared},
};
use crate::{AppState, ego_cli::ConfigContext};
use parking_lot::Mutex;
use serde::Deserialize;
use serde_json::{Value, json};
use std::sync::Arc;

const ID: &str = "ego";

#[path = "ego_login.rs"]
mod login;

/// Base auth edits and human login lifecycle. No Debug/Serialize: write-only values must not enter diagnostics.
#[derive(Deserialize, schemars::JsonSchema)]
#[serde(tag = "action", rename_all = "snake_case", deny_unknown_fields)]
pub enum EgoAuthAction {
    /// Open the configured Ego login in a human-operated PTY. No credential input is accepted.
    BeginLogin {
        provider: String,
    },
    /// Read terminal liveness and fresh credential presence; exit is not authentication success.
    LoginStatus {
        flow_id: String,
    },
    SetHeaders {
        provider: String,
        #[schemars(extend("writeOnly" = true))]
        headers: std::collections::BTreeMap<String, String>,
    },
    SetCredentialCommand {
        provider: String,
        #[schemars(extend("writeOnly" = true))]
        command: String,
    },
    ClearHeaders {
        provider: String,
    },
    ClearCredentialCommand {
        provider: String,
    },
}

pub struct EgoSettingsStore {
    state: std::sync::Weak<AppState>,
    context: Option<Arc<ConfigContext>>,
    runtime: tokio::runtime::Handle,
    sequence: Arc<Mutex<()>>,
    logins: Arc<login::Logins>,
}

impl EgoSettingsStore {
    /// Construct inside the runtime. `Store` calls run on its blocking pool, while Ego's
    /// existing asynchronous process owner remains the only execution path.
    #[allow(dead_code)] // Registered by the settings facade in Step 4.
    pub(crate) fn new(state: Arc<AppState>) -> Result<Self, Failure> {
        Ok(Self {
            state: Arc::downgrade(&state),
            context: None,
            runtime: tokio::runtime::Handle::try_current()
                .map_err(|_| Error::new(ID, "unavailable"))?,
            sequence: Arc::new(Mutex::new(())),
            logins: Arc::new(login::Logins::default()),
        })
    }

    pub fn auth_action_schema() -> Value {
        super::catalogue::schema::<EgoAuthAction>()
    }

    /// Base-only transaction; a launch profile must never redirect provider auth.
    pub fn auth_edit(
        &self,
        expected_revision: &str,
        action: EgoAuthAction,
    ) -> Result<CommitReply, Failure> {
        let _sequence = self.sequence.lock();
        let mut context = ConfigContext::configured(
            self.state
                .upgrade()
                .ok_or_else(|| Error::new(ID, "unavailable"))?
                .as_ref(),
        )
        .map_err(|_| Error::new(ID, "unavailable"))?;
        context.profile = None;
        let latest = self.snapshot(&context)?;
        if transaction::revision(&latest)? != expected_revision {
            return Err(Error::new(ID, "conflict"));
        }
        let (provider, field, value) = match action {
            EgoAuthAction::BeginLogin { .. } | EgoAuthAction::LoginStatus { .. } => {
                return Err(Error::new(ID, "invalid_type"));
            }
            EgoAuthAction::SetHeaders { provider, headers } => (
                provider,
                "headers",
                Some(serde_json::to_value(headers).map_err(|_| Error::new(ID, "invalid_type"))?),
            ),
            EgoAuthAction::SetCredentialCommand { provider, command } => {
                (provider, "credential_command", Some(json!(command)))
            }
            EgoAuthAction::ClearHeaders { provider } => (provider, "headers", None),
            EgoAuthAction::ClearCredentialCommand { provider } => {
                (provider, "credential_command", None)
            }
        };
        if provider.is_empty()
            || provider.len() > 64
            || !provider
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || b"_-.".contains(&b))
        {
            return Err(Error::new(ID, "unknown_path"));
        }
        let key = format!("auth.{provider}.{field}");
        let edits = match value {
            Some(value) => vec!["--set".into(), format!("{key}={}", toml_value(&value)?)],
            None => vec!["--unset".into(), key],
        };
        self.publish(&context, &latest, &edits)?;
        let after = self.snapshot(&context).map_err(|mut error| {
            error.persisted = true;
            error.next_action = "get".into();
            error
        })?;
        let persisted = latest != after;
        Ok(CommitReply {
            revision: transaction::revision(&after)?,
            persisted,
            applied: true,
            changed_ids: if persisted {
                vec![format!("ego.auth.{{provider}}.{field}")]
            } else {
                vec![]
            },
            effects: vec![],
            restart: if persisted {
                vec!["next_session".into()]
            } else {
                vec![]
            },
        })
    }

    fn context(&self) -> Result<Arc<ConfigContext>, Failure> {
        self.context
            .clone()
            .ok_or_else(|| Error::new(ID, "unavailable"))
    }

    fn snapshot(&self, context: &ConfigContext) -> Result<Value, Failure> {
        #[derive(Deserialize)]
        struct Listing {
            v: u8,
            scope: String,
            profile: Option<String>,
            values: Value,
            revision: String,
        }
        let capabilities = self
            .runtime
            .block_on(context.run(&["config".into(), "capabilities".into()]))
            .map_err(|_| Error::new(ID, "store_failed"))?;
        let capabilities: Value =
            serde_json::from_str(&capabilities).map_err(|_| Error::new(ID, "store_failed"))?;
        if capabilities["v"] != 1
            || capabilities["atomic_batch"] != true
            || capabilities["compare_and_swap"] != true
            || capabilities["max_operations"].as_u64().unwrap_or(0) < 4
        {
            return Err(Error::new(ID, "unavailable"));
        }
        let raw = self
            .runtime
            .block_on(context.managed("snapshot", &[]))
            .map_err(|_| Error::new(ID, "store_failed"))?;
        let listing: Listing =
            serde_json::from_str(&raw).map_err(|_| Error::new(ID, "store_failed"))?;
        if listing.v != 1 || listing.scope != "user" || listing.profile != context.profile {
            return Err(Error::new(ID, "store_failed"));
        }
        let raw = self
            .runtime
            .block_on(context.run(&["auth".into(), "status".into(), "--json".into()]))
            .map_err(|_| Error::new(ID, "store_failed"))?;
        let credentials = ego_schema::credentials(&raw)?;
        let managed: Managed = serde_json::from_value(listing.values.clone())
            .map_err(|_| Error::new(ID, "store_failed"))?;
        let mut value = ego_schema::value(&managed)?;
        let mut auth = serde_json::Map::new();
        for (key, presence) in listing
            .values
            .as_object()
            .ok_or_else(|| Error::new(ID, "store_failed"))?
        {
            if let Some((provider, field)) =
                key.strip_prefix("auth.").and_then(|s| s.rsplit_once('.'))
                && matches!(field, "headers" | "credential_command")
                && presence == "<redacted>"
            {
                auth.entry(provider.to_owned())
                    .or_insert_with(|| json!({"headers":null,"credential_command":null}))[field] =
                    json!("set");
            }
        }
        let mut presence = serde_json::Map::new();
        for credential in credentials {
            if credential.connection.is_none()
                && let Some(provider) = &credential.provider
            {
                auth.entry(provider.clone())
                    .or_insert_with(|| json!({"headers":null,"credential_command":null}));
            }
            presence.insert(
                credential.key(),
                if credential.present {
                    json!("set")
                } else {
                    Value::Null
                },
            );
        }
        value["auth"] = Value::Object(auth);
        value["_owner_revision"] = json!(listing.revision);
        value["credentials"] = Value::Object(presence);
        // Bind optimistic revisions to process authority and workspace too, without exposing
        // this internal metadata as a readable or writable setting.
        value["_context"] = json!({"program":context.program,"workspace":context.workspace,"profile":context.profile});
        Ok(value)
    }
    fn publish(
        &self,
        context: &ConfigContext,
        latest: &Value,
        edits: &[String],
    ) -> Result<(), Failure> {
        let revision = latest["_owner_revision"]
            .as_str()
            .ok_or_else(|| Error::new(ID, "store_failed"))?;
        let mut args = vec!["--if-revision".to_owned(), revision.to_owned()];
        args.extend_from_slice(edits);
        self.runtime
            .block_on(context.managed("batch", &args))
            .map_err(|error| {
                let conflict = serde_json::from_str::<Value>(&error.stdout)
                    .ok()
                    .is_some_and(|v| {
                        v["v"] == 1 && v["code"] == "conflict" && v["persisted"] == false
                    });
                let mut failure =
                    Error::new(ID, if conflict { "conflict" } else { "store_failed" });
                if conflict {
                    failure.retryable = true;
                    failure.next_action = "get".into();
                }
                failure
            })?;
        Ok(())
    }
}

impl Store for EgoSettingsStore {
    fn id(&self) -> &str {
        ID
    }
    fn target(&self) -> Target {
        self.context
            .as_ref()
            .and_then(|context| context.profile.as_ref())
            .map_or(Target::Machine, |id| Target::Profile { id: id.clone() })
    }
    fn resolve(self: Arc<Self>, target: &Target) -> Result<Arc<Self>, Failure> {
        let state = self
            .state
            .upgrade()
            .ok_or_else(|| Error::new(ID, "unavailable"))?;
        let context = Arc::new(
            ConfigContext::configured(state.as_ref()).map_err(|_| Error::new(ID, "unavailable"))?,
        );
        let current = context
            .profile
            .as_ref()
            .map_or(Target::Machine, |id| Target::Profile { id: id.clone() });
        if *target != current && *target != Target::Machine {
            return Err(Error::new(ID, "not_found"));
        }
        let mut context = Arc::try_unwrap(context).map_err(|_| Error::new(ID, "store_failed"))?;
        if *target == Target::Machine {
            context.profile = None;
        }
        let context = Arc::new(context);
        Ok(Arc::new(Self {
            state: self.state.clone(),
            context: Some(context),
            runtime: self.runtime.clone(),
            sequence: self.sequence.clone(),
            logins: self.logins.clone(),
        }))
    }
    fn validate_read_paths(&self, paths: &[String]) -> Result<(), Failure> {
        if self.context()?.profile.is_some() && paths.iter().any(|path| path.starts_with("/auth/"))
        {
            return Err(Error::new(ID, "not_found"));
        }
        Ok(())
    }
    fn read(&self) -> Result<Value, Failure> {
        let _sequence = self.sequence.lock();
        self.snapshot(self.context()?.as_ref())
    }
    fn commit(
        &self,
        prepare: &mut dyn FnMut(&Value) -> Result<Prepared, Failure>,
    ) -> Result<CommitReply, Failure> {
        let _sequence = self.sequence.lock();
        let context = self.context()?;
        let latest = self.snapshot(&context)?;
        let Prepared { value, mut reply } = prepare(&latest)?;
        let next: Managed =
            serde_json::from_value(value.clone()).map_err(|_| Error::new(ID, "invalid_type"))?;
        let next = ego_schema::value(&next)?;
        let changes: Vec<_> = next
            .as_object()
            .ok_or_else(|| Error::new(ID, "invalid_type"))?
            .iter()
            .filter(|(key, value)| latest.get(*key) != Some(*value))
            .collect();
        if changes.is_empty() {
            reply.persisted = false;
            reply.changed_ids.clear();
            reply.effects.clear();
            reply.restart.clear();
            return Ok(reply);
        }
        let mut args = Vec::new();
        for (key, new_value) in changes {
            if key == "model"
                && let Some(model) = new_value.as_str()
            {
                crate::ego_cli::validate_model(model)
                    .map_err(|_| Error::new(ID, "out_of_range"))?;
                if !model
                    .split_once('/')
                    .is_some_and(|(provider, model)| !provider.is_empty() && !model.is_empty())
                {
                    return Err(Error::new(ID, "out_of_range"));
                }
            }
            if new_value.is_null() {
                args.extend(["--unset".to_owned(), key.clone()]);
            } else {
                args.extend([
                    "--set".to_owned(),
                    format!("{key}={}", toml_value(new_value)?),
                ]);
            }
        }
        self.publish(&context, &latest, &args)?;
        let after = self.snapshot(&context).map_err(|mut error| {
            error.persisted = true;
            error.next_action = "get".into();
            error
        })?;
        reply.revision = transaction::revision(&after)?;
        reply.persisted = after != latest;
        if !reply.persisted {
            reply.changed_ids.clear();
            reply.effects.clear();
            reply.restart.clear();
        }
        Ok(reply)
    }
    fn apply(
        &self,
        _: &CommitReply,
    ) -> impl std::future::Future<Output = Result<bool, Failure>> + Send {
        // Settings affect new Ego invocations; existing ACP and PTY sessions are untouched.
        std::future::ready(Ok(true))
    }
}

/// Encode a single closed-key assignment, never shell text or an editor command.
fn toml_value(value: &Value) -> Result<toml_edit::Value, Failure> {
    Ok(match value {
        Value::String(s) => toml_edit::Value::from(s.as_str()),
        Value::Bool(b) => toml_edit::Value::from(*b),
        Value::Array(values) => {
            let mut array = toml_edit::Array::new();
            for value in values {
                array.push(toml_value(value)?);
            }
            toml_edit::Value::Array(array)
        }
        Value::Object(values) => {
            let mut table = toml_edit::InlineTable::new();
            for (key, value) in values {
                table.insert(key, toml_value(value)?);
            }
            toml_edit::Value::InlineTable(table)
        }
        _ => return Err(Error::new(ID, "invalid_type")),
    })
}
