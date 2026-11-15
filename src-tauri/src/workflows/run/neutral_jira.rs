//! Installed Jira activities share the neutral admission/effect owner and HTTP authority.
#![cfg_attr(
    not(test),
    expect(dead_code, reason = "011 awaits host composition in 008")
)]
use super::neutral_effects::{EffectObservation, EffectPreparation, NeutralEffect};
use super::neutral_http::{HttpAuthority, HttpContext};
use super::neutral_jira_client::JiraClient;
use super::neutral_runtime::{NeutralHost, NeutralInvocation, ObservationFuture};
use crate::workflows::catalog::{ActivityDescriptor, ActivityInstance, HandlerRef};
use crate::workflows::jira::*;
use crate::workflows::neutral_admission::{HostAdmission, HostContext, HostRegistry};
use crate::workflows::neutral_publication::NeutralPublication;
use serde_json::Value;
use std::collections::{BTreeMap, BTreeSet};
use std::sync::Arc;

pub(crate) struct JiraHost {
    pub approval: Option<super::neutral_effects::EffectQuestion>,
    pub state: Arc<crate::AppState>,
    pub bindings: BTreeMap<HandlerRef, JiraBinding>,
    pub authority: Arc<HttpAuthority>,
}
impl JiraHost {
    fn check(
        &self,
        context: &dyn HostContext,
        descriptor: &ActivityDescriptor,
        instance: &ActivityInstance,
        inputs: Option<&BTreeMap<String, Value>>,
        write: bool,
    ) -> Result<(&JiraBinding, Option<JiraRequest>), String> {
        let binding = self
            .bindings
            .get(&descriptor.handler)
            .ok_or("Jira binding not installed")?;
        if binding.descriptor != *descriptor
            || instance.descriptor != descriptor.id
            || descriptor.target.is_some()
        {
            return Err("Jira descriptor differs from installed binding".into());
        }
        let config = descriptor.config.resolve(&instance.config, "Jira config")?;
        if config.get("connector")
            != descriptor
                .config
                .fields
                .get("connector")
                .and_then(|f| f.default.as_ref())
        {
            return Err("Jira cannot override installed connector".into());
        }
        let context = (context as &dyn std::any::Any)
            .downcast_ref::<HttpContext>()
            .ok_or("Jira authority unavailable")?;
        if !context.network.contains(&binding.contract.id)
            || write
                && binding.operation.mutation()
                && !context.writes.contains(&binding.contract.id)
        {
            return Err("Jira network or write grant unavailable".into());
        }
        Ok((binding, inputs.map(|i| binding.request(i)).transpose()?))
    }
    fn client_request(
        &self,
        effect: &NeutralEffect,
        write: bool,
    ) -> Result<(JiraClient, JiraRequest), String> {
        let context = (self.authority)(&effect.invocation)?;
        let (binding, request) = self.check(
            &context,
            &effect.invocation.descriptor,
            &effect.invocation.instance,
            Some(&effect.invocation.inputs),
            write,
        )?;
        if !effect.dispatched || !effect.approved() {
            return Err("Jira original effect is not dispatched and approved".into());
        }
        if self.preparation(binding) != effect.preparation {
            return Err("Jira preparation changed".into());
        }
        Ok((
            JiraClient {
                state: self.state.clone(),
                binding: binding.clone(),
            },
            request.ok_or("Jira request absent")?,
        ))
    }
    fn preparation(&self, binding: &JiraBinding) -> EffectPreparation {
        EffectPreparation {
            mutation: binding.operation.mutation(),
            resources: if binding.operation.mutation() {
                // Serialize project writes, including key/id aliases and unknown create IDs.
                BTreeSet::from([binding.contract.id.clone()])
            } else {
                BTreeSet::new()
            },
            approval: binding
                .operation
                .mutation()
                .then(|| self.approval.clone())
                .flatten(),
            model_tools: None,
        }
    }
}
impl HostAdmission for JiraHost {
    fn publication(
        &self,
        context: &dyn HostContext,
        _: &NeutralPublication,
        descriptor: &ActivityDescriptor,
        instance: &ActivityInstance,
    ) -> Result<(), String> {
        self.check(context, descriptor, instance, None, true)
            .map(|_| ())
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
        self.check(context, descriptor, instance, Some(inputs), true)
            .map(|_| ())
    }
}
impl NeutralHost for JiraHost {
    fn context(&self, invocation: &NeutralInvocation) -> Result<Box<dyn HostContext>, String> {
        Ok(Box::new((self.authority)(invocation)?))
    }
    fn registry(&self) -> HostRegistry<'_> {
        let mut registry = HostRegistry::default();
        registry.handlers = self
            .bindings
            .keys()
            .cloned()
            .map(|key| (key, self as &dyn HostAdmission))
            .collect();
        registry
            .capabilities
            .insert("jira.network".into(), self as &dyn HostAdmission);
        registry
    }
    fn prepare(&self, invocation: &NeutralInvocation) -> Result<EffectPreparation, String> {
        let context = (self.authority)(invocation)?;
        let (binding, _) = self.check(
            &context,
            &invocation.descriptor,
            &invocation.instance,
            Some(&invocation.inputs),
            true,
        )?;
        Ok(self.preparation(binding))
    }
    fn invoke_effect(&self, effect: NeutralEffect) -> ObservationFuture {
        let prepared = self.client_request(&effect, true);
        Box::pin(async move {
            let (client, request) = prepared?;
            completed(
                super::neutral_jira_operation::execute(&client, &request, &effect.key.digest)
                    .await?,
            )
        })
    }
    fn reconcile(&self, effect: NeutralEffect) -> ObservationFuture {
        let prepared = self.client_request(&effect, false);
        Box::pin(async move {
            let (client, request) = prepared?;
            completed(
                super::neutral_jira_reconcile::reconcile(&client, &request, &effect.key.digest)
                    .await?,
            )
        })
    }
}
fn completed(result: JiraResult) -> Result<EffectObservation, String> {
    Ok(super::neutral_http::completed(BTreeMap::from([(
        "result".into(),
        serde_json::to_value(result).map_err(|e| e.to_string())?,
    )])))
}
