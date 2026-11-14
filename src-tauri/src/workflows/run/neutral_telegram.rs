//! Installed Telegram activities; the neutral owner retains all effect state.
#![cfg_attr(
    not(test),
    expect(dead_code, reason = "013 host composition belongs to 008")
)]
use super::neutral_effects::{EffectObservation, EffectOutcome, EffectPreparation, NeutralEffect};
use super::neutral_runtime::{NeutralHost, NeutralInvocation, ObservationFuture};
use super::neutral_telegram_contract::{NotificationReference, TelegramOperation, TelegramRequest};
use crate::workflows::catalog::{
    ActivityDescriptor, ActivityInstance, DescriptorId, FieldSchema, HandlerRef, ObjectSchema,
    ValueType,
};
use crate::workflows::connector::{ConnectorContract, RateLimit, Reconciliation};
use crate::workflows::data_reference::DataReference;
use crate::workflows::neutral_admission::{HostAdmission, HostContext, HostRegistry};
use crate::workflows::neutral_publication::NeutralPublication;
use serde_json::{Value, json};
use std::collections::{BTreeMap, BTreeSet};

pub(crate) struct TelegramHost {
    bot: String,
    pub descriptors: BTreeMap<HandlerRef, ActivityDescriptor>,
}
struct Context;
impl HostContext for Context {}

/// Host composition installs these three pinned descriptors and this one host.
pub(crate) fn telegram_host(bot: String) -> Result<TelegramHost, String> {
    if bot.trim().is_empty() {
        return Err("Telegram bot alias is empty".into());
    }
    let contract = ConnectorContract {
        id: format!("telegram:{bot}"),
        authentication: Some(DataReference::Credential {
            id: format!("telegram:{bot}"),
        }),
        resources: ObjectSchema {
            fields: BTreeMap::from([("chat".into(), field(ValueType::Integer, true, None))]),
        },
        pagination: None,
        rate_limit: RateLimit::FailWithRetryAfter,
        callback: None,
        reconciliation: Reconciliation::Unavailable,
    };
    contract.validate()?;
    let mut descriptors = BTreeMap::new();
    for (name, operation) in [
        ("send", TelegramOperation::Send),
        ("update", TelegramOperation::Update),
        ("reply", TelegramOperation::Reply),
    ] {
        let handler = HandlerRef {
            name: format!("telegram.{name}"),
            version: 1,
        };
        descriptors.insert(
            handler.clone(),
            ActivityDescriptor {
                id: DescriptorId {
                    namespace: "telegram".into(),
                    name: name.into(),
                    version: 1,
                },
                purpose: format!("Telegram {name} through the installed bot"),
                handler,
                target: None,
                config: ObjectSchema {
                    fields: BTreeMap::from([(
                        "connector".into(),
                        field(
                            ValueType::Object,
                            true,
                            Some(json!({"contract": contract, "bot": bot, "operation": operation})),
                        ),
                    )]),
                },
                inputs: ObjectSchema {
                    fields: BTreeMap::from([
                        ("chat".into(), field(ValueType::Integer, true, None)),
                        ("text".into(), field(ValueType::String, true, None)),
                        ("message".into(), field(ValueType::Object, false, None)),
                        (
                            "actions".into(),
                            field(ValueType::Array, false, Some(json!([]))),
                        ),
                    ]),
                },
                subject: None,
                outputs: ObjectSchema {
                    fields: BTreeMap::from([(
                        "notification".into(),
                        field(ValueType::Object, true, None),
                    )]),
                },
                outcomes: BTreeSet::from(["complete".into()]),
                required_capabilities: BTreeSet::new(),
                execution_policies: BTreeSet::from(["local".into()]),
            },
        );
    }
    Ok(TelegramHost { bot, descriptors })
}

fn field(value_type: ValueType, required: bool, default: Option<Value>) -> FieldSchema {
    FieldSchema {
        value_type,
        required,
        default,
    }
}

impl TelegramHost {
    fn check(
        &self,
        descriptor: &ActivityDescriptor,
        instance: &ActivityInstance,
        inputs: Option<&BTreeMap<String, Value>>,
    ) -> Result<TelegramOperation, String> {
        if self.descriptors.get(&descriptor.handler) != Some(descriptor)
            || instance.descriptor != descriptor.id
        {
            return Err("Telegram descriptor differs from installed contract".into());
        }
        let config = descriptor
            .config
            .resolve(&instance.config, "Telegram config")?;
        if config.get("connector") != descriptor.config.fields["connector"].default.as_ref() {
            return Err("Telegram instance cannot replace installed connector".into());
        }
        let op: TelegramOperation =
            serde_json::from_value(config["connector"]["operation"].clone())
                .map_err(|_| "invalid Telegram operation")?;
        if let Some(inputs) = inputs {
            let request = request(descriptor, inputs)?;
            request.wire(&self.bot, op, "00000000-0000-0000-0000-000000000000")?;
            let paths = crate::telegram::settings::paths().map_err(|e| e.to_string())?;
            let config = crate::telegram::Config::load(&paths)
                .map_err(|e| e.to_string())?
                .ok_or("Telegram disabled")?;
            if config.bot_alias != self.bot
                || !paths
                    .allowlist()
                    .map_err(|e| e.to_string())?
                    .contains(&request.chat)
            {
                return Err("Telegram bot or chat is unavailable".into());
            }
        }
        Ok(op)
    }
}

pub(super) fn request(
    descriptor: &ActivityDescriptor,
    inputs: &BTreeMap<String, Value>,
) -> Result<TelegramRequest, String> {
    let inputs = descriptor.inputs.resolve(inputs, "Telegram inputs")?;
    serde_json::from_value(serde_json::to_value(inputs).map_err(|e| e.to_string())?)
        .map_err(|_| "invalid typed Telegram request".into())
}

impl HostAdmission for TelegramHost {
    fn publication(
        &self,
        _: &dyn HostContext,
        _: &NeutralPublication,
        descriptor: &ActivityDescriptor,
        instance: &ActivityInstance,
    ) -> Result<(), String> {
        self.check(descriptor, instance, None).map(|_| ())
    }
    fn dispatch(
        &self,
        _: &dyn HostContext,
        _: &NeutralPublication,
        descriptor: &ActivityDescriptor,
        instance: &ActivityInstance,
        inputs: &BTreeMap<String, Value>,
        _: Option<&BTreeMap<String, Value>>,
    ) -> Result<(), String> {
        self.check(descriptor, instance, Some(inputs)).map(|_| ())
    }
}
impl NeutralHost for TelegramHost {
    fn context(&self, _: &NeutralInvocation) -> Result<Box<dyn HostContext>, String> {
        Ok(Box::new(Context))
    }
    fn registry(&self) -> HostRegistry<'_> {
        let mut registry = HostRegistry::default();
        registry.handlers = self
            .descriptors
            .keys()
            .cloned()
            .map(|key| (key, self as &dyn HostAdmission))
            .collect();
        registry
    }
    fn prepare(&self, invocation: &NeutralInvocation) -> Result<EffectPreparation, String> {
        self.check(
            &invocation.descriptor,
            &invocation.instance,
            Some(&invocation.inputs),
        )?;
        let request = request(&invocation.descriptor, &invocation.inputs)?;
        Ok(EffectPreparation {
            mutation: true,
            resources: BTreeSet::from([format!(
                "connector:telegram:{}:{}",
                self.bot, request.chat
            )]),
            approval: None,
            model_tools: None,
        })
    }
    fn invoke_effect(&self, effect: NeutralEffect) -> ObservationFuture {
        let bot = self.bot.clone();
        Box::pin(async move {
            if !effect.dispatched || !effect.approved() {
                return Err("Telegram effect lacks current dispatch preparation".into());
            }
            // Dispatch admission checked this retained connector configuration.
            // The serialized worker rechecks alias and chat before the write.
            let op: TelegramOperation = serde_json::from_value(
                effect
                    .invocation
                    .instance
                    .config
                    .get("connector")
                    .and_then(|config| config.get("operation"))
                    .cloned()
                    .ok_or("Telegram operation missing from pinned configuration")?,
            )
            .map_err(|_| "invalid pinned Telegram operation")?;
            let request = request(&effect.invocation.descriptor, &effect.invocation.inputs)?;
            let response = crate::telegram::workflow_execute(
                bot.clone(),
                op,
                request.clone(),
                effect.key.fence.run.clone(),
            )
            .await
            .map_err(|e| e.to_string())?;
            let message = request.receipt(&bot, op, &response)?;
            Ok(super::neutral_http::completed(BTreeMap::from([(
                "notification".into(),
                serde_json::to_value(NotificationReference {
                    effect: effect.key,
                    message,
                })
                .map_err(|e| e.to_string())?,
            )])))
        })
    }
    fn reconcile(&self, _: NeutralEffect) -> ObservationFuture {
        // Bot API has no getMessage/idempotent send lookup. Retain uncertainty.
        Box::pin(async {
            Ok(EffectObservation { stopped: false, outcome: EffectOutcome::Uncertain { diagnostic: "Telegram original send requires retained service evidence; no request repeated".into() } })
        })
    }
}
