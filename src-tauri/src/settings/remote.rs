//! Remote owner seams: `connections.json` and the global `tunnels/*.toml` profiles are written
//! only through their existing owners (`ConfigFile` locks, `ProfileStore`). The catalogue
//! describes each list as a wrapper record keyed by `id`; the files keep their own shapes.
//! Vault secrets (connection password, pairing token) are never read or written here: they are
//! bound to the immutable connection id, so an edit keeps them and only a removal deletes them.
use super::{
    request::*,
    service::Store,
    transaction::{self, Prepared},
};
use crate::config::ConfigFile;
use crate::remote_connection::{RemoteConnection, delete_connection_credentials};
use crate::tunnels::{profile::TunnelProfile, storage::ProfileStore};
use parking_lot::Mutex;
use serde::{Serialize, de::DeserializeOwned};
use serde_json::{Value, json};
use std::{collections::HashMap, path::PathBuf, sync::Arc};

const CONNECTIONS_ID: &str = "connections";
const TUNNELS_ID: &str = "tunnels";
const CONNECTIONS_FILE: &str = "connections.json";

/// Schema of the `connections` store: the records of `connections.json`.
#[derive(Debug, Clone, Default, Serialize, serde::Deserialize, schemars::JsonSchema)]
pub(crate) struct RemoteConnections {
    #[serde(default)]
    #[schemars(extend("x-setting" = {"sensitivity":"public","apply":"live","effect":"Updates connections for the current owner.","operations":["get","set"],"confirmationClass":"ordinary"}, "x-collection-key" = "id"))]
    pub(crate) connections: Vec<RemoteConnection>,
}

/// Schema of the `tunnels` store: the global-scope profiles. Repository-scope profiles live in
/// each repository and are not part of this store.
#[derive(Debug, Clone, Default, Serialize, serde::Deserialize, schemars::JsonSchema)]
pub(crate) struct TunnelProfiles {
    #[serde(default)]
    #[schemars(extend("x-setting" = {"sensitivity":"public","apply":"live","effect":"Updates profiles for the current owner.","operations":["get","set"],"confirmationClass":"ordinary"}, "x-collection-key" = "id"))]
    pub(crate) profiles: Vec<TunnelProfile>,
}

/// Receives the saved record after removal, when disk can no longer supply teardown authority.
pub(crate) type OnConnectionRemoved = Arc<dyn Fn(&RemoteConnection) + Send + Sync>;

/// Tunnel runtime teardown remains keyed by profile id.
pub type OnRemoved = Arc<dyn Fn(&str) + Send + Sync>;

/// Records of one list, as the catalogue sees them.
fn document<T: Serialize>(key: &str, records: &[T], id: &str) -> Result<Value, Failure> {
    let records = serde_json::to_value(records).map_err(|_| Error::new(id, "store_failed"))?;
    Ok(json!({ key: records }))
}

struct Plan<T> {
    next: Vec<T>,
    reply: CommitReply,
    changed: bool,
}

/// Run `prepare` on the latest records and settle the reply. `validate` is the owner's own gate
/// and may normalize (trim) the records before they are compared and persisted.
fn plan<T: Serialize + DeserializeOwned>(
    id: &str,
    key: &str,
    latest: &[T],
    prepare: &mut dyn FnMut(&Value) -> Result<Prepared, Failure>,
    validate: impl FnOnce(&mut [T]) -> Result<(), String>,
) -> Result<Plan<T>, Failure> {
    let latest_document = document(key, latest, id)?;
    let Prepared { value, mut reply } = prepare(&latest_document)?;
    let mut next: Vec<T> = serde_json::from_value(value.get(key).cloned().unwrap_or(Value::Null))
        .map_err(|_| Error::new(id, "invalid_type"))?;
    validate(&mut next).map_err(|_| Error::new(id, "out_of_range"))?;
    let canonical = document(key, &next, id)?;
    reply.revision = transaction::revision(&canonical)?;
    if canonical == latest_document {
        reply.changed_ids.clear();
        reply.effects.clear();
        reply.restart.clear();
    }
    let changed = !reply.changed_ids.is_empty();
    reply.persisted = changed;
    Ok(Plan {
        next,
        reply,
        changed,
    })
}

fn removed_ids<T>(latest: &[T], next: &[T], id_of: impl Fn(&T) -> &str) -> Vec<String> {
    latest
        .iter()
        .map(&id_of)
        .filter(|id| !next.iter().any(|n| id_of(n) == *id))
        .map(str::to_owned)
        .collect()
}

pub struct ConnectionsSettingsStore {
    path: PathBuf,
    on_removed: OnConnectionRemoved,
    /// Removed records of each commit, keyed by the revision it produced, until `apply` runs.
    pending: Mutex<HashMap<String, Vec<RemoteConnection>>>,
}

impl ConnectionsSettingsStore {
    pub(crate) fn new(data_dir: PathBuf, on_removed: OnConnectionRemoved) -> Self {
        Self::at_path(data_dir.join(CONNECTIONS_FILE), on_removed)
    }

    fn at_path(path: PathBuf, on_removed: OnConnectionRemoved) -> Self {
        Self {
            path,
            on_removed,
            pending: Mutex::default(),
        }
    }

    fn file(&self) -> ConfigFile<Vec<RemoteConnection>> {
        ConfigFile::at_path(self.path.clone())
    }
}

impl Store for ConnectionsSettingsStore {
    fn id(&self) -> &str {
        CONNECTIONS_ID
    }
    fn target(&self) -> Target {
        Target::Machine
    }

    fn read(&self) -> Result<Value, Failure> {
        // Strict: a corrupt file is an error, never silently an empty list.
        let records = self
            .file()
            .update_with_strict(|records| Ok((records.clone(), false)))
            .map_err(|_| Error::new(CONNECTIONS_ID, "store_failed"))?;
        document("connections", &records, CONNECTIONS_ID)
    }

    fn commit(
        &self,
        prepare: &mut dyn FnMut(&Value) -> Result<Prepared, Failure>,
    ) -> Result<CommitReply, Failure> {
        let mut failure = None;
        let result = self.file().update_with_strict(|records| {
            let settled = plan(CONNECTIONS_ID, "connections", records, prepare, |next| {
                next.iter().try_for_each(RemoteConnection::validate)
            })
            .map_err(|e| {
                failure = Some(e);
                "rejected".to_owned()
            })?;
            let Plan {
                next,
                reply,
                changed,
            } = settled;
            if changed {
                let removed = records
                    .iter()
                    .filter(|record| !next.iter().any(|next| next.id == record.id))
                    .cloned()
                    .collect();
                self.pending.lock().insert(reply.revision.clone(), removed);
                *records = next;
            }
            Ok((reply, changed))
        });
        result.map_err(|_| failure.unwrap_or_else(|| Error::new(CONNECTIONS_ID, "store_failed")))
    }

    fn apply(
        &self,
        reply: &CommitReply,
    ) -> impl std::future::Future<Output = Result<bool, Failure>> + Send {
        let removed = self.pending.lock().remove(&reply.revision);
        let mut complete = true;
        for connection in removed.into_iter().flatten() {
            (self.on_removed)(&connection);
            // The vault key is the connection id: a secret left behind names nothing reachable.
            complete &= delete_connection_credentials(&connection.id).is_ok();
        }
        std::future::ready(Ok(complete))
    }
}

pub struct TunnelsSettingsStore {
    data_dir: PathBuf,
    on_removed: OnRemoved,
    /// `ProfileStore` has no lock of its own; this one orders settings commits among themselves.
    lock: Mutex<()>,
    pending: Mutex<HashMap<String, Vec<String>>>,
}

impl TunnelsSettingsStore {
    pub fn new(data_dir: PathBuf, on_removed: OnRemoved) -> Self {
        Self {
            data_dir,
            on_removed,
            lock: Mutex::new(()),
            pending: Mutex::default(),
        }
    }

    fn load(&self) -> Result<Vec<TunnelProfile>, Failure> {
        let mut profiles = ProfileStore::load_all(&self.data_dir, None)
            .map_err(|_| Error::new(TUNNELS_ID, "store_failed"))?;
        profiles.sort_by(|a, b| a.id.cmp(&b.id));
        Ok(profiles)
    }
}

impl Store for TunnelsSettingsStore {
    fn id(&self) -> &str {
        TUNNELS_ID
    }
    fn target(&self) -> Target {
        Target::Machine
    }

    fn read(&self) -> Result<Value, Failure> {
        document("profiles", &self.load()?, TUNNELS_ID)
    }

    fn validate_proposal(&self, _latest: &Value, value: &Value) -> Result<(), Failure> {
        let mut next: Vec<TunnelProfile> = serde_json::from_value(value["profiles"].clone())
            .map_err(|_| Error::new(TUNNELS_ID, "invalid_type"))?;
        next.iter_mut()
            .try_for_each(TunnelProfile::validate)
            .map_err(|_| Error::new(TUNNELS_ID, "out_of_range"))
    }

    fn commit(
        &self,
        prepare: &mut dyn FnMut(&Value) -> Result<Prepared, Failure>,
    ) -> Result<CommitReply, Failure> {
        let _ordered = self.lock.lock();
        let latest = self.load()?;
        let Plan {
            mut next,
            reply,
            changed,
        } = plan(TUNNELS_ID, "profiles", &latest, prepare, |next| {
            next.iter_mut().try_for_each(TunnelProfile::validate)
        })?;
        if changed {
            next.sort_by(|a, b| a.id.cmp(&b.id));
            let before = |id: &str| latest.iter().find(|p| p.id == id);
            for profile in &next {
                let same = before(&profile.id).is_some_and(|old| {
                    serde_json::to_value(old).ok() == serde_json::to_value(profile).ok()
                });
                if !same {
                    ProfileStore::save(&self.data_dir, profile)
                        .map_err(|_| Error::new(TUNNELS_ID, "store_failed"))?;
                }
            }
            let removed = removed_ids(&latest, &next, |p| p.id.as_str());
            for id in &removed {
                ProfileStore::delete(&self.data_dir, None, id)
                    .map_err(|_| Error::new(TUNNELS_ID, "store_failed"))?;
            }
            self.pending.lock().insert(reply.revision.clone(), removed);
        }
        Ok(reply)
    }

    fn apply(
        &self,
        reply: &CommitReply,
    ) -> impl std::future::Future<Output = Result<bool, Failure>> + Send {
        for id in self
            .pending
            .lock()
            .remove(&reply.revision)
            .into_iter()
            .flatten()
        {
            (self.on_removed)(&id);
        }
        std::future::ready(Ok(true))
    }
}

// Credentials are input-only; never implement Debug or Serialize on this action.
#[derive(serde::Deserialize, schemars::JsonSchema)]
#[serde(tag = "action", rename_all = "snake_case", deny_unknown_fields)]
pub enum ConnectionCredentialAction {
    SetPassword {
        id: String,
        #[schemars(extend("writeOnly" = true, "x-sensitivity" = "secret"))]
        password: String,
    },
    ClearPassword {
        id: String,
    },
    SetPairingToken {
        id: String,
        #[schemars(extend("writeOnly" = true, "x-sensitivity" = "secret"))]
        token: String,
    },
    ClearPairingToken {
        id: String,
    },
    /// The relay pairing bundle printed by `tuic-remote --relay-pair`.
    SetRelayPairing {
        id: String,
        #[schemars(extend("writeOnly" = true, "x-sensitivity" = "secret"))]
        bundle: String,
    },
    ClearRelayPairing {
        id: String,
    },
}

#[derive(Clone, Copy)]
enum Slot {
    Password,
    PairingToken,
    RelayPairing,
}

/// One vault slot per immutable connection id, not a caller-selected vault reference.
/// The callback retires the old authenticated runtime and schedules owner reconciliation.
#[derive(Clone)]
pub struct ConnectionCredentialSettings {
    path: PathBuf,
    on_changed: OnRemoved,
}
impl ConnectionCredentialSettings {
    pub fn new(data_dir: PathBuf, on_changed: OnRemoved) -> Self {
        Self {
            path: data_dir.join("connections.json"),
            on_changed,
        }
    }
    pub fn action_schema() -> Value {
        super::catalogue::schema::<ConnectionCredentialAction>()
    }
    pub fn read(&self, id: &str) -> Result<Value, Failure> {
        self.with_connection(id, || {
            let password = crate::remote_connection::connection_password_exists(id)
                .map_err(|_| Error::new(CONNECTIONS_ID, "secret_unreadable"))?;
            let pairing = crate::remote_connection::pairing_token(id)
                .map_err(|_| Error::new(CONNECTIONS_ID, "secret_unreadable"))?
                .is_some();
            let relay = crate::remote_connection::relay_bundle(id)
                .map_err(|_| Error::new(CONNECTIONS_ID, "secret_unreadable"))?
                .is_some();
            Ok(
                json!({"password":{"status":if password {"set"} else {"unset"}},
                "pairingToken":{"status":if pairing {"set"} else {"unset"}},
                "relayPairing":{"status":if relay {"set"} else {"unset"}}}),
            )
        })
    }
    fn with_connection<R>(
        &self,
        id: &str,
        action: impl FnOnce() -> Result<R, Failure>,
    ) -> Result<R, Failure> {
        let mut failure = None;
        ConfigFile::<Vec<RemoteConnection>>::at_path(self.path.clone())
            .update_with_strict(|records| {
                let result = if records.iter().any(|record| record.id == id) {
                    action()
                } else {
                    Err(Error::new(CONNECTIONS_ID, "not_found"))
                };
                result.map(|result| (result, false)).map_err(|error| {
                    failure = Some(error);
                    "rejected".to_owned()
                })
            })
            .map_err(|_| failure.unwrap_or_else(|| Error::new(CONNECTIONS_ID, "store_failed")))
    }
    pub fn perform(&self, action: ConnectionCredentialAction) -> Result<Value, Failure> {
        let (id, secret, slot, clear) = match action {
            ConnectionCredentialAction::SetPassword { id, password } => {
                (id, password, Slot::Password, false)
            }
            ConnectionCredentialAction::ClearPassword { id } => {
                (id, String::new(), Slot::Password, true)
            }
            ConnectionCredentialAction::SetPairingToken { id, token } => {
                (id, token, Slot::PairingToken, false)
            }
            ConnectionCredentialAction::ClearPairingToken { id } => {
                (id, String::new(), Slot::PairingToken, true)
            }
            ConnectionCredentialAction::SetRelayPairing { id, bundle } => {
                (id, bundle, Slot::RelayPairing, false)
            }
            ConnectionCredentialAction::ClearRelayPairing { id } => {
                (id, String::new(), Slot::RelayPairing, true)
            }
        };
        let secret = zeroize::Zeroizing::new(secret);
        if !clear && (secret.is_empty() || secret.len() > 16384) {
            return Err(Error::new(CONNECTIONS_ID, "out_of_range"));
        }
        self.with_connection(&id, || {
            let result = match slot {
                Slot::PairingToken => crate::remote_connection::set_pairing_token(&id, &secret),
                Slot::Password => crate::remote_connection::set_connection_password(&id, &secret),
                // A bundle that does not parse is the caller's mistake, not a
                // store failure worth retrying.
                Slot::RelayPairing => {
                    if !clear
                        && crate::relay_tunnel::bundle::PairingBundle::parse(secret.trim()).is_err()
                    {
                        return Err(Error::new(CONNECTIONS_ID, "out_of_range"));
                    }
                    crate::remote_connection::set_relay_pairing(&id, &secret)
                }
            };
            result.map_err(|_| {
                let mut error = Error::new(CONNECTIONS_ID, "store_failed");
                error.retryable = true;
                error.next_action =
                    "read credential presence and retry the same owner action".into();
                error
            })
        })?;
        (self.on_changed)(&id);
        self.read(&id).map_err(|mut error| {
            error.persisted = true;
            error.next_action =
                "read credential presence; runtime reconciliation was requested".into();
            error
        })
    }
}

pub(super) fn credential_schema() -> Value {
    // The schema names an owner capability, not an extra field in connections.json.
    json!({"type":"object","properties":{"credential":{"type":"string","writeOnly":true,
        "x-setting":{"sensitivity":"secret","apply":"live","effect":"Updates the immutable connection credential slot and reconciles its runtime.","operations":["get","set_secret","clear_secret"],"confirmationClass":"ordinary"}}}})
}
