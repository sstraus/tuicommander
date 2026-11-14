//! GitLab service adapter; scheduling and uncertainty remain in the neutral owner.
#![cfg_attr(
    not(test),
    expect(dead_code, reason = "012 awaits native host composition")
)]
use super::neutral_effects::{EffectObservation, EffectOutcome, EffectPreparation, NeutralEffect};
use super::neutral_runtime::{NeutralHost, NeutralInvocation, ObservationFuture};
use crate::workflows::catalog::{ActivityDescriptor, ActivityInstance, HandlerRef};
use crate::workflows::gitlab::{GitLabInstall, Operation};
use crate::workflows::neutral_admission::{HostAdmission, HostContext, HostRegistry};
use crate::workflows::neutral_publication::NeutralPublication;
use serde_json::Value;
use std::collections::{BTreeMap, BTreeSet};
use std::sync::Arc;

#[derive(Default)]
pub(crate) struct GitLabContext {
    /// Exact installed connector IDs; a read grant never permits publication.
    pub reads: BTreeSet<String>,
    pub writes: BTreeSet<String>,
}
impl HostContext for GitLabContext {}
pub(crate) type GitLabAuthority =
    dyn Fn(&NeutralInvocation) -> Result<GitLabContext, String> + Send + Sync;
pub(crate) struct GitLabHost {
    pub approval: Option<super::neutral_effects::EffectQuestion>,
    pub state: Arc<crate::AppState>,
    pub install: GitLabInstall,
    pub authority: Arc<GitLabAuthority>,
}
const OPERATIONS: [Operation; 5] = [
    Operation::Issue,
    Operation::MergeRequest,
    Operation::DiffVersion,
    Operation::Pipeline,
    Operation::ReviewNote,
];
impl GitLabHost {
    fn check(
        &self,
        context: &dyn HostContext,
        descriptor: &ActivityDescriptor,
        instance: &ActivityInstance,
        inputs: Option<&BTreeMap<String, Value>>,
    ) -> Result<Operation, String> {
        self.install.validate()?;
        let op = OPERATIONS
            .into_iter()
            .find(|op| self.install.descriptor(*op) == *descriptor)
            .ok_or("GitLab descriptor differs from installed contract")?;
        if instance.descriptor != descriptor.id || instance.execution_policy != "local" {
            return Err("GitLab instance differs from installed operation".into());
        }
        let config = descriptor
            .config
            .resolve(&instance.config, "GitLab config")?;
        if config.get("installed")
            != descriptor
                .config
                .fields
                .get("installed")
                .and_then(|f| f.default.as_ref())
        {
            return Err("GitLab instance cannot override installed metadata".into());
        }
        let context = (context as &dyn std::any::Any)
            .downcast_ref::<GitLabContext>()
            .ok_or("GitLab current host context unavailable")?;
        if !context.reads.contains(&self.install.connector.id)
            || (op.mutation() && !context.writes.contains(&self.install.connector.id))
        {
            return Err("GitLab current read/write grant unavailable".into());
        }
        if let Some(inputs) = inputs {
            self.install.request(op, inputs, "preflight")?;
        }
        Ok(op)
    }
}
impl HostAdmission for GitLabHost {
    fn publication(
        &self,
        context: &dyn HostContext,
        _: &NeutralPublication,
        descriptor: &ActivityDescriptor,
        instance: &ActivityInstance,
    ) -> Result<(), String> {
        self.check(context, descriptor, instance, None).map(|_| ())
    }
    fn dispatch(
        &self,
        context: &dyn HostContext,
        _: &NeutralPublication,
        descriptor: &ActivityDescriptor,
        instance: &ActivityInstance,
        inputs: &BTreeMap<String, Value>,
        _: Option<&BTreeMap<String, Value>>,
    ) -> Result<(), String> {
        self.check(context, descriptor, instance, Some(inputs))
            .map(|_| ())
    }
}
impl NeutralHost for GitLabHost {
    fn context(&self, invocation: &NeutralInvocation) -> Result<Box<dyn HostContext>, String> {
        Ok(Box::new((self.authority)(invocation)?))
    }
    fn registry(&self) -> HostRegistry<'_> {
        let mut registry = HostRegistry::default();
        registry.handlers = OPERATIONS
            .into_iter()
            .map(|op| {
                (
                    self.install.descriptor(op).handler,
                    self as &dyn HostAdmission,
                )
            })
            .collect::<BTreeMap<HandlerRef, _>>();
        registry.capabilities = ["gitlab:read", "gitlab:write"]
            .into_iter()
            .map(|name| (name.into(), self as &dyn HostAdmission))
            .collect();
        registry
    }
    fn prepare(&self, invocation: &NeutralInvocation) -> Result<EffectPreparation, String> {
        let context = (self.authority)(invocation)?;
        let op = self.check(
            &context,
            &invocation.descriptor,
            &invocation.instance,
            Some(&invocation.inputs),
        )?;
        let resources = if op.mutation() {
            BTreeSet::from([format!(
                "{}:mr:{}",
                self.install.connector.id,
                crate::workflows::gitlab::number(&invocation.inputs, "mr")?
            )])
        } else {
            BTreeSet::new()
        };
        Ok(EffectPreparation {
            mutation: op.mutation(),
            resources,
            approval: op.mutation().then(|| self.approval.clone()).flatten(),
            model_tools: None,
        })
    }
    fn invoke_effect(&self, effect: NeutralEffect) -> ObservationFuture {
        let request = self.prepare(&effect.invocation).and_then(|prepared| {
            if !effect.dispatched || !effect.approved() || prepared != effect.preparation {
                return Err("GitLab effect lacks current dispatch ownership".into());
            }
            // Preparation already validated the exact installed descriptor.
            OPERATIONS
                .into_iter()
                .find(|op| self.install.descriptor(*op) == effect.invocation.descriptor)
                .ok_or_else(|| "GitLab installed operation missing".into())
        });
        let state = self.state.clone();
        let install = self.install.clone();
        Box::pin(async move {
            let op = request?;
            super::neutral_gitlab_client::execute(state, &install, op, &effect).await
        })
    }
    fn reconcile(&self, _: NeutralEffect) -> ObservationFuture {
        // GitLab notes has no exact idempotency/reconciliation contract here.
        // Retain the original ownership; never turn lost evidence into another POST.
        Box::pin(async {
            Ok(EffectObservation {
                stopped: false,
                outcome: EffectOutcome::Uncertain {
                    diagnostic:
                        "GitLab original note needs authorized service evidence; no write repeated"
                            .into(),
                },
            })
        })
    }
}
