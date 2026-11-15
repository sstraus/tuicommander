//! Internal owner service; no transport or origin/confirmation authority is granted here.
use super::{
    catalogue::Catalogue,
    model::*,
    pointer,
    request::*,
    transaction::{self, Prepared},
};
use serde_json::{Value, json};
use std::{collections::BTreeMap, future::Future, sync::Arc};

/// An adapter owns strict reads, validation, locking, persistence and runtime effects.
/// `commit` MUST invoke `prepare` on latest state under the same owner/file lock as
/// its one atomic write. It must not call `apply` or publish before persistence.
pub trait Store: Send + Sync + 'static {
    fn id(&self) -> &str;
    fn target(&self) -> Target;
    /// Bind a request to its owner without changing fixed-target adapters.
    fn resolve(self: Arc<Self>, target: &Target) -> Result<Arc<Self>, Failure> {
        if *target != self.target() {
            return Err(Error::new(self.id(), "not_found"));
        }
        Ok(self)
    }
    fn read(&self) -> Result<Value, Failure>;
    /// Refuse paths that are not owned by the resolved target before reading state.
    fn validate_read_paths(&self, _paths: &[String]) -> Result<(), Failure> {
        Ok(())
    }
    /// Discovery only runs on an explicit refresh; describe never calls this method.
    fn refresh_choices(
        &self,
        _resolver: &str,
        _path: &str,
    ) -> impl Future<Output = Result<Vec<Value>, Failure>> + Send {
        std::future::ready(Err(Error::new(self.id(), "unavailable")))
    }
    /// Pure owner validation before an exposure proposal is offered to the human.
    /// `commit` must still repeat domain validation on the locked latest state.
    fn validate_proposal(&self, _latest: &Value, _value: &Value) -> Result<(), Failure> {
        Ok(())
    }
    fn commit(
        &self,
        prepare: &mut dyn FnMut(&Value) -> Result<Prepared, Failure>,
    ) -> Result<CommitReply, Failure>;
    /// `false` means effects were scheduled but completion still needs readback.
    fn apply(&self, reply: &CommitReply) -> impl Future<Output = Result<bool, Failure>> + Send;
}

pub struct Service<S> {
    catalogue: Arc<Catalogue>,
    owner: Arc<S>,
    policy: Arc<super::policy::Policy>,
    choices: parking_lot::Mutex<BTreeMap<String, Choices>>,
}

impl<S: Store> Service<S> {
    pub fn new(catalogue: Arc<Catalogue>, owner: Arc<S>) -> Self {
        Self {
            catalogue,
            owner,
            choices: parking_lot::Mutex::new(BTreeMap::new()),
            policy: Arc::new(super::policy::Policy::default()),
        }
    }

    /// Share the host proposal pool with the real all-client confirmation path.
    pub fn with_policy(mut self, policy: Arc<super::policy::Policy>) -> Self {
        self.policy = policy;
        self
    }

    /// Host-issued caller context is separate from the model's request payload.
    pub async fn execute_as(
        &self,
        request: Request,
        caller: &super::policy::Caller,
        approval: Option<&str>,
    ) -> Result<Reply, Failure> {
        caller.check()?;
        if let Request::Describe {
            target,
            store,
            paths,
        }
        | Request::Refresh {
            target,
            store,
            paths,
        } = &request
        {
            return self
                .choice_snapshot(
                    target.clone(),
                    store.clone(),
                    paths.clone(),
                    matches!(request, Request::Refresh { .. }),
                )
                .await
                .map(Reply::Snapshot);
        }
        if let Request::Get {
            target,
            store,
            paths,
        } = request
        {
            return self.get(target, store, paths).await.map(Reply::Snapshot);
        }
        self.check_owner(request.store())?;
        let owner = self.owner.clone();
        let cat = self.catalogue.clone();
        let policy = self.policy.clone();
        let context = caller.clone();
        let grant = approval.map(str::to_owned);
        let safe_store = self.owner.id().to_owned();
        let request = tokio::task::spawn_blocking(move || {
            let (target, _, _, _) = super::policy::patch(&request)?;
            let owner = owner.resolve(target)?;
            let latest = owner.read()?;
            if grant.is_none() {
                policy.authorize(
                    &cat,
                    &context,
                    &request,
                    &latest,
                    grant.as_deref(),
                    |value| owner.validate_proposal(&latest, value),
                )?;
            }
            Ok::<_, Failure>(request)
        })
        .await
        .map_err(|_| Error::new(&safe_store, "store_failed"))??;
        self.commit_patch(request, caller.clone(), approval.map(str::to_owned))
            .await
            .map(Reply::Commit)
    }

    pub fn store_id(&self) -> &str {
        self.owner.id()
    }

    fn check_owner(&self, store: &str) -> Result<(), Failure> {
        if store != self.owner.id() {
            // Use the registered store, never echo an arbitrary caller string.
            return Err(Error::new(self.owner.id(), "not_found"));
        }
        if !self.catalogue.fields.values().any(|f| f.store == store) {
            return Err(Error::new(self.owner.id(), "unavailable"));
        }
        Ok(())
    }

    pub async fn execute(&self, request: Request) -> Result<Reply, Failure> {
        self.execute_as(request, &super::policy::Caller::native(), None)
            .await
    }

    pub async fn get(
        &self,
        target: Target,
        store: String,
        paths: Vec<String>,
    ) -> Result<Snapshot, Failure> {
        self.check_owner(&store)?;
        if paths.is_empty() || paths.len() > 128 {
            let mut error = Error::new(&store, "out_of_range");
            error.expected = Some("paths array".into());
            error.allowed = vec!["minItems: 1".into(), "maxItems: 128".into()];
            return Err(error);
        }
        let cat = self.catalogue.clone();
        let owner = self.owner.clone();
        let safe_store = store.clone();
        tokio::task::spawn_blocking(move || {
            let owner = owner.resolve(&target)?;
            owner.validate_read_paths(&paths)?;
            snapshot(&cat, &store, paths, owner.read()?)
        })
        .await
        .map_err(|_| Error::new(&safe_store, "store_failed"))?
    }

    async fn choice_snapshot(
        &self,
        target: Target,
        store: String,
        paths: Vec<String>,
        refresh: bool,
    ) -> Result<Snapshot, Failure> {
        self.check_owner(&store)?;
        if paths.is_empty() || paths.len() > 128 {
            return Err(Error::new(&store, "out_of_range"));
        }
        let owner = self.owner.clone().resolve(&target)?;
        let read_owner = owner.clone();
        let revision =
            tokio::task::spawn_blocking(move || transaction::revision(&read_owner.read()?))
                .await
                .map_err(|_| Error::new(&store, "store_failed"))??;
        // Validate the whole request before any discovery; a typo cannot trigger probes.
        let mut resolvers = Vec::new();
        for path in &paths {
            let parts = pointer::segments(path, &store)?;
            let field = pointer::field(&self.catalogue, &store, &parts)?;
            let Some(Choices::Dynamic { resolver, .. }) = &field.choices else {
                return Err(Error::new(&store, "unavailable"));
            };
            if !matches!(field.availability, Availability::Available) {
                return Err(Error::new(&store, "unavailable"));
            }
            resolvers.push(resolver.clone());
        }
        let mut choices = BTreeMap::new();
        for (path, resolver) in paths.into_iter().zip(resolvers) {
            let key = format!(
                "{}:{}:{}",
                serde_json::to_string(&target).unwrap(),
                revision,
                path
            );
            let value = if refresh {
                let (availability, values) = match owner.refresh_choices(&resolver, &path).await {
                    Ok(values) => (Availability::Available, Some(values)),
                    Err(_) => (
                        Availability::Unavailable {
                            reason: "Owner discovery failed; explicitly refresh to retry".into(),
                        },
                        None,
                    ),
                };
                let value = Choices::Dynamic {
                    resolver,
                    availability,
                    values,
                };
                // Bound retained discovery across revisions and targets to 128 entries.
                {
                    let mut cache = self.choices.lock();
                    if cache.len() >= 128 {
                        cache.clear();
                    }
                    cache.insert(key, value.clone());
                }
                value
            } else {
                self.choices
                    .lock()
                    .get(&key)
                    .cloned()
                    .unwrap_or(Choices::Dynamic {
                        resolver,
                        availability: Availability::Unavailable {
                            reason: "Explicit refresh required".into(),
                        },
                        values: None,
                    })
            };
            choices.insert(path, value);
        }
        Ok(Snapshot {
            revision,
            values: BTreeMap::new(),
            choices,
        })
    }

    pub async fn patch(
        &self,
        target: Target,
        store: String,
        expected_revision: String,
        operations: Vec<Edit>,
    ) -> Result<CommitReply, Failure> {
        match self
            .execute(Request::Patch {
                target,
                store,
                expected_revision,
                operations,
            })
            .await?
        {
            Reply::Commit(reply) => Ok(reply),
            Reply::Snapshot(_) => unreachable!("patch returns a commit"),
        }
    }

    async fn commit_patch(
        &self,
        request: Request,
        caller: super::policy::Caller,
        approval: Option<String>,
    ) -> Result<CommitReply, Failure> {
        let store = request.store().to_owned();
        self.check_owner(&store)?;
        let owner = self.owner.clone();
        let cat = self.catalogue.clone();
        let safe_store = store.clone();
        let policy = self.policy.clone();
        let (mut reply, owner) = tokio::task::spawn_blocking(move || {
            let (target, _, expected_revision, operations) = super::policy::patch(&request)?;
            let owner = owner.resolve(target)?;
            owner
                .commit(&mut |latest| {
                    // Retain revocable grants while queued; consume and revalidate only
                    // after the owner lock is acquired, immediately before preparation.
                    policy.authorize(
                        &cat,
                        &caller,
                        &request,
                        latest,
                        approval.as_deref(),
                        |value| owner.validate_proposal(latest, value),
                    )?;
                    transaction::prepare(&cat, &store, latest, expected_revision, &operations)
                })
                .map(|reply| (reply, owner))
        })
        .await
        .map_err(|_| Error::new(&safe_store, "store_failed"))??;
        if !reply.changed_ids.is_empty() {
            match owner.apply(&reply).await {
                Ok(applied) => reply.applied = applied,
                Err(_) => {
                    let mut error = Error::new(&safe_store, "apply_failed");
                    error.persisted = reply.persisted;
                    error.revision = Some(reply.revision);
                    error.next_action = "get".into();
                    // Reread/recover effects; never invite replay of a durable mutation.
                    return Err(error);
                }
            }
        } else {
            reply.applied = true;
        }
        Ok(reply)
    }
}

/// Project an authoritative owner document with the same path and secret rules as file stores.
pub(super) fn snapshot(
    cat: &Catalogue,
    store: &str,
    paths: Vec<String>,
    mut latest: Value,
) -> Result<Snapshot, Failure> {
    if paths.is_empty() || paths.len() > 128 {
        return Err(Error::new(store, "out_of_range"));
    }
    let revision = transaction::revision(&latest)?;
    let mut values = BTreeMap::new();
    for path in paths {
        let parts = pointer::segments(&path, store)?;
        let field = pointer::field(cat, store, &parts)?;
        if !matches!(field.availability, Availability::Available)
            || !field.annotation.operations.contains(&Operation::Get)
        {
            return Err(Error::new(store, "unavailable"));
        }
        let value = match pointer::at_mut(&mut latest, &parts, cat, store, &mut Vec::new()) {
            Ok(value) => value.clone(),
            Err(error)
                if error.code == "not_found"
                    && (field.nullable
                        || matches!(
                            field.annotation.sensitivity,
                            Sensitivity::Secret | Sensitivity::DerivedSecret
                        )) =>
            {
                let (key, parent) = parts.split_last().expect("nonempty pointer");
                let parent = pointer::at_mut(&mut latest, parent, cat, store, &mut Vec::new())?;
                if field.path.ends_with('}') || !parent.is_object() || parent.get(key).is_some() {
                    return Err(error);
                }
                Value::Null
            }
            Err(error) => return Err(error),
        };
        let stored = if matches!(
            field.annotation.sensitivity,
            Sensitivity::Secret | Sensitivity::DerivedSecret
        ) {
            if field.schema["enum"] == json!(["set", "unset", "unavailable"]) {
                if value == "unavailable" {
                    return Err(Error::new(store, "secret_unreadable"));
                }
                json!({"status": value})
            } else {
                let set = !value.is_null() && value != "";
                json!({"status": if set { "set" } else { "unset" }})
            }
        } else {
            value
        };
        values.insert(
            path,
            StoredValue {
                effective: stored.clone(),
                stored,
                source: "stored".into(),
            },
        );
    }
    Ok(Snapshot {
        revision,
        values,
        choices: BTreeMap::new(),
    })
}
