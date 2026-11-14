//! Loaded-plugin settings owners. Catalogue lifetime and write authority move together.
use super::{
    catalogue::Catalogue,
    plugin_schema::Definition,
    request::*,
    service::{Service, Store},
    transaction::Prepared,
};
use crate::config::ConfigFile;
use parking_lot::RwLock;
use serde_json::Value;
use std::{collections::BTreeMap, path::PathBuf, sync::Arc};

type Notify = Arc<dyn Fn() + Send + Sync>;

pub struct PluginStore {
    id: String,
    plugin: String,
    root: PathBuf,
    definition: Definition,
    catalogue: Arc<Catalogue>,
    active: RwLock<bool>,
    notify: Notify,
}

impl PluginStore {
    pub(crate) fn new(
        plugin: &str,
        definition: Definition,
        root: PathBuf,
        notify: Notify,
    ) -> Result<Self, Failure> {
        let catalogue = Arc::new(definition.catalogue(plugin)?);
        Ok(Self {
            id: format!("plugins.{plugin}"),
            plugin: plugin.into(),
            root,
            definition,
            catalogue,
            active: RwLock::new(true),
            notify,
        })
    }

    /// Same strict lock and atomic private-file writer as the other settings owners.
    fn update<T>(
        &self,
        write: impl FnOnce(&mut Value) -> Result<(T, bool), Failure>,
    ) -> Result<T, Failure> {
        let active = self.active.read();
        if !*active {
            return Err(Error::new(&self.id, "not_found"));
        }
        let mut failure = None;
        let path = crate::plugin_settings::checked_config_path(&self.root)
            .map_err(|_| Error::new(&self.id, "store_failed"))?;
        ConfigFile::at_path(path)
            .update_with_strict(|value| {
                let result = self
                    .definition
                    .normalize(value, &self.id)
                    .and_then(|()| write(value));
                result.map_err(|error| {
                    failure = Some(error);
                    "rejected".into()
                })
            })
            .map_err(|_| failure.unwrap_or_else(|| Error::new(&self.id, "store_failed")))
    }

    /// Materialize complete defaults and migrate existing credentials into a private atomic file.
    pub(crate) fn migrate(&self) -> Result<(), Failure> {
        self.update(|_| Ok(((), true)))
    }

    pub(crate) fn replace(&self, mut value: Value) -> Result<(), Failure> {
        self.definition.normalize(&mut value, &self.id)?;
        self.update(|latest| {
            let changed = *latest != value;
            *latest = value;
            Ok(((), changed))
        })
    }
}

impl Store for PluginStore {
    fn id(&self) -> &str {
        &self.id
    }
    fn target(&self) -> Target {
        Target::Plugin {
            id: self.plugin.clone(),
        }
    }
    fn read(&self) -> Result<Value, Failure> {
        self.update(|value| Ok((value.clone(), false)))
    }
    fn commit(
        &self,
        prepare: &mut dyn FnMut(&Value) -> Result<Prepared, Failure>,
    ) -> Result<CommitReply, Failure> {
        self.update(|latest| {
            let Prepared { value, mut reply } = prepare(latest)?;
            super::validation::validate(&self.definition.schema, &value, &self.id)?;
            let changed = value != *latest;
            if changed {
                *latest = value;
            }
            reply.persisted = changed;
            Ok((reply, changed))
        })
    }
    fn apply(
        &self,
        _: &CommitReply,
    ) -> impl std::future::Future<Output = Result<bool, Failure>> + Send {
        if *self.active.read() {
            (self.notify)();
        }
        // The frontend unloads/reloads and reads config.json; dispatch is not an acknowledgement.
        std::future::ready(Ok(false))
    }
}

#[derive(Default)]
pub struct PluginSettings {
    owners: RwLock<BTreeMap<String, Arc<PluginStore>>>,
}

impl PluginSettings {
    pub(crate) fn register(&self, store: PluginStore) {
        let mut owners = self.owners.write();
        if let Some(old) = owners.insert(store.plugin.clone(), Arc::new(store)) {
            *old.active.write() = false;
        }
    }

    pub(crate) fn unregister(&self, id: &str) {
        if let Some(old) = self.owners.write().remove(id) {
            *old.active.write() = false;
        }
    }

    fn owner(&self, id: &str) -> Result<Arc<PluginStore>, Failure> {
        self.owners
            .read()
            .get(id)
            .cloned()
            .ok_or_else(|| Error::new("plugins", "not_found"))
    }

    /// Runtime descriptors contain definitions only, never plugin values or credentials.
    pub fn catalogue(&self) -> Result<Catalogue, Failure> {
        let owners = self.owners.read();
        let mut catalogue = Catalogue::default();
        for owner in owners.values() {
            let cat = &owner.catalogue;
            catalogue.fields.extend(cat.fields.clone());
            catalogue.collections.extend(cat.collections.clone());
        }
        Ok(catalogue)
    }

    /// Cutover can route the standard request directly to its registered scoped owner.
    pub async fn execute(&self, request: Request) -> Result<Reply, Failure> {
        self.execute_as(
            request,
            &super::policy::Caller::native(),
            Arc::new(super::policy::Policy::default()),
            None,
        )
        .await
    }

    pub async fn execute_as(
        &self,
        request: Request,
        caller: &super::policy::Caller,
        policy: Arc<super::policy::Policy>,
        approval: Option<&str>,
    ) -> Result<Reply, Failure> {
        caller.check()?;
        let id = request
            .store()
            .strip_prefix("plugins.")
            .ok_or_else(|| Error::new("plugins", "not_found"))?;
        let owner = self.owner(id)?;
        Service::new(owner.catalogue.clone(), owner)
            .with_policy(policy)
            .execute_as(request, caller, approval)
            .await
    }

    pub async fn action(&self, id: &str, action: PluginAction) -> Result<Value, Failure> {
        let owner = self.owner(id)?;
        if matches!(action, PluginAction::Describe) {
            return serde_json::to_value(&owner.catalogue)
                .map_err(|_| Error::new("plugins", "store_failed"));
        }
        let request = action.request(owner.target(), owner.id.clone())?;
        // Validate scope before revision comparison: an unknown path never becomes a host write.
        let paths: Vec<&str> = match &request {
            Request::Get { paths, .. }
            | Request::Describe { paths, .. }
            | Request::Refresh { paths, .. } => paths.iter().map(String::as_str).collect(),
            Request::Set { path, .. } | Request::Reset { path, .. } => vec![path],
            Request::Patch { operations, .. } => operations.iter().map(Edit::path).collect(),
        };
        for path in paths {
            let parts = super::pointer::segments(path, &owner.id)?;
            super::pointer::field(&owner.catalogue, &owner.id, &parts)?;
        }
        let reply = Service::new(owner.catalogue.clone(), owner)
            .execute(request)
            .await?;
        match reply {
            Reply::Snapshot(snapshot) => serde_json::to_value(snapshot),
            Reply::Commit(reply) => serde_json::to_value(reply),
        }
        .map_err(|_| Error::new("plugins", "store_failed"))
    }
}

#[derive(Clone, Debug, serde::Deserialize, serde::Serialize, schemars::JsonSchema)]
#[serde(tag = "action", rename_all = "snake_case", deny_unknown_fields)]
pub enum PluginAction {
    Describe,
    Get {
        paths: Vec<String>,
    },
    Set {
        path: String,
        value: Value,
        #[serde(rename = "expectedRevision")]
        expected_revision: String,
    },
    Reset {
        path: String,
        #[serde(rename = "expectedRevision")]
        expected_revision: String,
    },
    Patch {
        operations: Vec<Edit>,
        #[serde(rename = "expectedRevision")]
        expected_revision: String,
    },
}

impl PluginAction {
    fn request(self, target: Target, store: String) -> Result<Request, Failure> {
        Ok(match self {
            Self::Describe => return Err(Error::new("plugins", "invalid_type")),
            Self::Get { paths } => Request::Get {
                target,
                store,
                paths,
            },
            Self::Set {
                path,
                value,
                expected_revision,
            } => Request::Set {
                target,
                store,
                path,
                value,
                expected_revision,
            },
            Self::Reset {
                path,
                expected_revision,
            } => Request::Reset {
                target,
                store,
                path,
                expected_revision,
            },
            Self::Patch {
                operations,
                expected_revision,
            } => Request::Patch {
                target,
                store,
                operations,
                expected_revision,
            },
        })
    }
}
