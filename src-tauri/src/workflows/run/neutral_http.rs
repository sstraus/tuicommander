//! Installed HTTP adapter: admission and effects use the existing neutral owner.
#![cfg_attr(
    not(test),
    expect(dead_code, reason = "009 HTTP awaits host composition in 008")
)]

use super::neutral_effects::{EffectObservation, EffectOutcome, EffectPreparation, NeutralEffect};
use super::neutral_execution::NeutralResult;
use super::neutral_runtime::{NeutralHost, NeutralInvocation, ObservationFuture};
use crate::workflows::catalog::{
    ActivityDescriptor, ActivityInstance, FieldSchema, HandlerRef, ValueType, content_digest,
};
use crate::workflows::connector::ConnectorContract;
use crate::workflows::data_reference::DataReference;
use crate::workflows::http_operation::HttpOperation;
use crate::workflows::neutral_admission::{HostAdmission, HostContext, HostRegistry};
use crate::workflows::neutral_publication::NeutralPublication;
use serde_json::Value;
use std::collections::{BTreeMap, BTreeSet};
use std::sync::Arc;

pub(crate) struct HttpConnector {
    pub descriptor: ActivityDescriptor,
    contract: ConnectorContract,
    plugin: String,
    base: String,
}

impl HttpConnector {
    pub(crate) fn new(
        mut descriptor: ActivityDescriptor,
        contract: ConnectorContract,
        plugin: String,
        base: String,
    ) -> Result<Self, String> {
        contract.validate()?;
        let url = url::Url::parse(&base).map_err(|_| "invalid HTTP connector base")?;
        if !matches!(url.scheme(), "http" | "https")
            || !url.username().is_empty()
            || url.password().is_some()
            || url.query().is_some()
            || url.fragment().is_some()
            || !url.path().ends_with('/')
        {
            return Err("HTTP connector requires a credential-free base URL ending in /".into());
        }
        if plugin.is_empty() || !descriptor.outcomes.contains("complete") {
            return Err("HTTP connector requires plugin and complete outcome".into());
        }
        // Catalog pins include every installed boundary. Replacing credentials,
        // pagination or client identity invalidates old pins.
        if descriptor.target.is_none() {
            descriptor.config.fields.insert(
                "connector".into(),
                FieldSchema {
                    value_type: ValueType::Object,
                    required: true,
                    default: Some(
                        serde_json::json!({"contract": contract, "plugin": plugin, "base": base}),
                    ),
                },
            );
        } else if descriptor.target
            != Some(crate::workflows::catalog::ReusableTarget::Http {
                connector: contract.clone(),
                plugin: plugin.clone(),
                base: base.clone(),
            })
        {
            return Err("HTTP reusable target differs from connector binding".into());
        }
        if descriptor
            .config
            .fields
            .get("request")
            .is_some_and(|f| f.default.is_some())
        {
            let config = descriptor
                .config
                .resolve(&BTreeMap::new(), "HTTP defaults")?;
            HttpOperation::parse(&config)?.validate(&base, &descriptor, &contract)?;
        }
        Ok(Self {
            descriptor,
            contract,
            plugin,
            base,
        })
    }
}

#[derive(Clone, Default)]
pub(crate) struct HttpContext {
    pub network: BTreeSet<String>,
    pub writes: BTreeSet<String>,
}
impl HostContext for HttpContext {}

/// Host code looks up current grants; graph values cannot construct this callback.
pub(crate) type HttpAuthority =
    dyn Fn(&NeutralInvocation) -> Result<HttpContext, String> + Send + Sync;

pub(crate) struct HttpHost {
    pub approval: Option<super::neutral_effects::EffectQuestion>,
    pub state: Arc<crate::AppState>,
    pub connectors: BTreeMap<HandlerRef, HttpConnector>,
    pub authority: Arc<HttpAuthority>,
}

impl HttpHost {
    fn binding(&self, descriptor: &ActivityDescriptor) -> Result<&HttpConnector, String> {
        let binding = self
            .connectors
            .get(&descriptor.handler)
            .ok_or("HTTP connector is not installed")?;
        if binding.descriptor != *descriptor {
            return Err("HTTP descriptor differs from installed contract".into());
        }
        Ok(binding)
    }

    fn check(
        &self,
        context: &dyn HostContext,
        descriptor: &ActivityDescriptor,
        instance: &ActivityInstance,
        inputs: Option<&BTreeMap<String, Value>>,
    ) -> Result<(HttpOperation, bool), String> {
        let binding = self.binding(descriptor)?;
        if instance.descriptor != descriptor.id {
            return Err("HTTP instance descriptor mismatch".into());
        }
        let config = descriptor.config.resolve(&instance.config, "HTTP config")?;
        if config.get("connector")
            != descriptor
                .config
                .fields
                .get("connector")
                .and_then(|f| f.default.as_ref())
        {
            return Err("HTTP instance cannot override installed connector metadata".into());
        }
        let context = (context as &dyn std::any::Any)
            .downcast_ref::<HttpContext>()
            .ok_or("HTTP host context unavailable")?;
        let op = HttpOperation::parse(&config)?;
        op.validate(&binding.base, descriptor, &binding.contract)?;
        let mutation = !matches!(op.method.as_str(), "GET" | "HEAD" | "OPTIONS");
        if binding.contract.pagination.is_some() && mutation {
            return Err("pagination cannot repeat a mutating operation".into());
        }
        if !context.network.contains(&binding.contract.id)
            || (mutation && !context.writes.contains(&binding.contract.id))
        {
            return Err("HTTP network or write grant unavailable".into());
        }
        if let Some(inputs) = inputs {
            descriptor.inputs.resolve(inputs, "HTTP inputs")?;
            op.resolve(&binding.base, inputs)?;
            resources(binding, inputs)?;
        }
        Ok((op, mutation))
    }
}

impl HostAdmission for HttpHost {
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

impl NeutralHost for HttpHost {
    fn context(&self, invocation: &NeutralInvocation) -> Result<Box<dyn HostContext>, String> {
        Ok(Box::new((self.authority)(invocation)?))
    }

    fn registry(&self) -> HostRegistry<'_> {
        let mut registry = HostRegistry::default();
        registry.handlers = self
            .connectors
            .keys()
            .cloned()
            .map(|key| (key, self as &dyn HostAdmission))
            .collect();
        registry
    }

    fn prepare(&self, invocation: &NeutralInvocation) -> Result<EffectPreparation, String> {
        let context = (self.authority)(invocation)?;
        let (_, mutation) = self.check(
            &context,
            &invocation.descriptor,
            &invocation.instance,
            Some(&invocation.inputs),
        )?;
        let binding = self.binding(&invocation.descriptor)?;
        Ok(EffectPreparation {
            mutation,
            resources: if mutation {
                BTreeSet::from([format!(
                    "connector:{}:{}",
                    binding.contract.id,
                    content_digest(&resources(binding, &invocation.inputs)?)?
                )])
            } else {
                BTreeSet::new()
            },
            approval: mutation.then(|| self.approval.clone()).flatten(),
            model_tools: None,
        })
    }

    fn invoke_effect(&self, effect: NeutralEffect) -> ObservationFuture {
        // Repeat current admission immediately before calling the installed client.
        let prepared = self.prepare(&effect.invocation);
        let binding = self.binding(&effect.invocation.descriptor);
        let request = prepared.and_then(|prepared| {
            if !effect.dispatched || !effect.approved() || prepared != effect.preparation {
                return Err("HTTP effect lacks its current dispatch preparation".into());
            }
            let binding = binding?;
            Ok((
                HttpOperation::parse(
                    &effect
                        .invocation
                        .descriptor
                        .config
                        .resolve(&effect.invocation.instance.config, "HTTP config")?,
                )?,
                binding.contract.clone(),
                binding.plugin.clone(),
                binding.base.clone(),
            ))
        });
        let state = self.state.clone();
        Box::pin(async move {
            let (op, contract, plugin, base) = request?;
            super::neutral_http_client::execute(state, effect, op, contract, plugin, base).await
        })
    }

    fn reconcile(&self, effect: NeutralEffect) -> ObservationFuture {
        // The ledger receives callbacks with the exact original key. Never issue
        // a GET probe or replacement write without a service reconciliation API.
        Box::pin(async move {
            Ok(EffectObservation {
                stopped: false,
                outcome: EffectOutcome::Uncertain {
                    diagnostic: format!(
                        "original HTTP effect {} requires retained callback/service evidence; no request repeated",
                        effect.key.digest
                    ),
                },
            })
        })
    }
}

fn resources(
    binding: &HttpConnector,
    inputs: &BTreeMap<String, Value>,
) -> Result<BTreeMap<String, Value>, String> {
    let values = inputs
        .iter()
        .filter(|(name, _)| binding.contract.resources.fields.contains_key(*name))
        .map(|(k, v)| (k.clone(), v.clone()))
        .collect();
    binding
        .contract
        .resources
        .resolve(&values, "connector resources")
}

pub(super) fn completed(outputs: BTreeMap<String, Value>) -> EffectObservation {
    EffectObservation {
        stopped: true,
        outcome: EffectOutcome::Completed {
            result: NeutralResult::Completed {
                outcome: "complete".into(),
                outputs,
            },
        },
    }
}

pub(super) fn credential(contract: &ConnectorContract) -> Option<String> {
    match &contract.authentication {
        Some(DataReference::Credential { id }) => Some(id.clone()),
        _ => None,
    }
}
