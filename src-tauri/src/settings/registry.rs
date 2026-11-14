//! Routes a request to the one owner service registered for its store id.
//! Each adapter story adds one `register` call; the registry knows no store itself.
use super::{
    policy::{Caller, Policy},
    request::*,
    service::{Service, Store},
};
use std::{collections::BTreeMap, future::Future, pin::Pin, sync::Arc};

type Outcome = Result<Reply, Failure>;

trait Owner: Send + Sync {
    fn execute<'a>(
        &'a self,
        request: Request,
        caller: &'a Caller,
        approval: Option<&'a str>,
    ) -> Pin<Box<dyn Future<Output = Outcome> + Send + 'a>>;
}

impl<S: Store> Owner for Service<S> {
    fn execute<'a>(
        &'a self,
        request: Request,
        caller: &'a Caller,
        approval: Option<&'a str>,
    ) -> Pin<Box<dyn Future<Output = Outcome> + Send + 'a>> {
        Box::pin(Service::execute_as(self, request, caller, approval))
    }
}

#[derive(Default)]
pub struct Registry {
    owners: BTreeMap<String, Arc<dyn Owner>>,
    policy: Arc<Policy>,
}

impl Registry {
    /// All owners share the pool used by the host's confirm dialog integration.
    pub fn with_policy(policy: Arc<Policy>) -> Self {
        Self {
            owners: BTreeMap::new(),
            policy,
        }
    }

    /// A store id has exactly one owner; a second registration is a wiring bug.
    pub fn register<S: Store>(&mut self, service: Service<S>) -> Result<(), Failure> {
        let id = service.store_id().to_owned();
        if self.owners.contains_key(&id) {
            return Err(Error::new(&id, "duplicate"));
        }
        self.owners
            .insert(id, Arc::new(service.with_policy(self.policy.clone())));
        Ok(())
    }

    pub async fn execute(&self, request: Request) -> Outcome {
        self.execute_as(request, &Caller::native(), None).await
    }

    pub async fn execute_as(
        &self,
        request: Request,
        caller: &Caller,
        approval: Option<&str>,
    ) -> Outcome {
        caller.check()?;
        // The caller's string is never echoed: an unknown store reports under "settings".
        let owner = self
            .owners
            .get(request.store())
            .ok_or_else(|| Error::new("settings", "not_found"))?;
        owner.execute(request, caller, approval).await
    }
}
