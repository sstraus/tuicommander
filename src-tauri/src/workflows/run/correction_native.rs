//! Consume real native review/check services, never caller-supplied verdicts.
use super::RunStore;
use super::native_operations::NativeContext;
use super::neutral_effects::NeutralEffect;
use crate::workflows::catalog::{
    ActivityDescriptor, DescriptorId, HandlerRef, WorkflowCatalog, content_digest,
};
use crate::workflows::correction::*;
use crate::workflows::correction_region::{correction_input_schema, correction_state_schema};
use crate::workflows::review_verdict::CodeVerdict;
use std::collections::BTreeSet;

pub(super) fn register(catalog: &mut WorkflowCatalog) -> Result<(), String> {
    let template = catalog
        .descriptors()
        .next()
        .ok_or("native catalog empty")?
        .clone();
    catalog.register(ActivityDescriptor {
        id: DescriptorId {
            namespace: "native".into(),
            name: "correction.assess".into(),
            version: 1,
        },
        handler: HandlerRef {
            name: "native.correction.assess".into(),
            version: 1,
        },
        purpose: "Assess current independent review and every captured verification check".into(),
        inputs: correction_input_schema(),
        outputs: correction_state_schema(),
        outcomes: BTreeSet::from(["assessed".into()]),
        ..template
    })
}

pub(super) fn register_policy(catalog: &mut WorkflowCatalog) -> Result<(), String> {
    use crate::workflows::catalog::{FieldSchema, ObjectSchema, ValueType};
    let mut descriptor = catalog
        .descriptors()
        .find(|d| d.handler.name == "native.correction.assess")
        .ok_or("native correction descriptor absent")?
        .clone();
    descriptor.id.name = "correction.policy.assess".into();
    descriptor.handler.name = super::correction_policy::HANDLER.into();
    descriptor.purpose="Assess source role findings and proposal receipts under separate review/correction budgets".into();
    let field = |value_type, default| FieldSchema {
        value_type,
        required: true,
        default,
    };
    descriptor.config = ObjectSchema {
        fields: std::collections::BTreeMap::from([(
            "policy".into(),
            field(ValueType::Object, None),
        )]),
    };
    descriptor
        .inputs
        .fields
        .insert("report".into(), field(ValueType::Object, None));
    descriptor.inputs.fields.insert(
        "proposals".into(),
        field(ValueType::Array, Some(serde_json::json!([]))),
    );
    descriptor
        .outputs
        .fields
        .insert("accounting".into(), field(ValueType::Object, None));
    catalog.register(descriptor)
}

pub(super) fn validate(
    inputs: &std::collections::BTreeMap<String, serde_json::Value>,
) -> Result<(), String> {
    if inputs
        .keys()
        .any(|key| !["context", "state", "repeat", "criteria"].contains(&key.as_str()))
        || !inputs.get("context").is_some_and(|v| v.is_object())
    {
        return Err("correction requires immutable context and carried state".into());
    }
    serde_json::from_value::<CorrectionState>(
        inputs
            .get("state")
            .ok_or("correction state absent")?
            .clone(),
    )
    .map(|_| ())
    .map_err(|e| e.to_string())
}

pub(super) fn assess(
    context: &NativeContext,
    store: &RunStore,
    effect: &NeutralEffect,
) -> Result<CorrectionEvaluation, String> {
    let admission_policy =
        if effect.invocation.descriptor.handler.name == super::correction_policy::HANDLER {
            super::correction_policy::validate(&effect.invocation.inputs)?;
            Some(super::native_review_policy::policy(
                &effect.invocation.instance,
            )?)
        } else {
            validate(&effect.invocation.inputs)?;
            None
        };
    context.current(None)?;
    let state: CorrectionState = serde_json::from_value(effect.invocation.inputs["state"].clone())
        .map_err(|e| e.to_string())?;
    let artifact = &state.artifact;
    super::correction_generation::require_artifact(store, &effect.key.fence.run, artifact)?;
    context.require_artifact(&artifact.head, &artifact.material_digest)?;
    let checks = context.verification_check_names();
    let policy = CorrectionPolicy {
        criteria: checks
            .iter()
            .map(|id| format!("check:{id}"))
            .chain(["independent_review".into()])
            .collect(),
    };
    let declared: CorrectionPolicy = serde_json::from_value(
        effect
            .invocation
            .inputs
            .get("criteria")
            .ok_or("correction criteria absent")?
            .clone(),
    )
    .map_err(|e| e.to_string())?;
    if declared != policy || artifact.criteria_digest != content_digest(&policy)? {
        return Err("correction criteria differ from captured native checks/review".into());
    }
    let run = store.neutral_execution(&effect.key.fence.run)?;
    let owned = run.effect(&effect.key)?;
    if run.status != super::neutral_execution::NeutralStatus::Running
        || run.deadline_ms <= super::store::now_ms()
        || !owned.dispatched
        || owned.cancel_started
        || owned.observation.is_some()
        || owned.invocation != effect.invocation
    {
        return Err("correction assessment lacks current owned effect".into());
    }
    if admission_policy.is_some()
        && super::native_role_budget::exhausted(
            &super::native_host::NativeBinding {
                context: context.clone(),
                store: store.clone(),
            },
            effect,
        )?
    {
        return Ok(CorrectionEvaluation::Ready {
            state: Box::new(state),
            repeat: None,
        });
    }
    // Only actual stopped native effects in this run are eligible. An old head
    // or unexecuted review cannot be supplied as a passing JSON verdict.
    let reviews = run
        .effects
        .values()
        .filter(|e| {
            if let Some(policy) = &admission_policy {
                e.invocation.descriptor.handler.name == super::native_review_policy::HANDLER
                    && super::native_review_policy::policy(&e.invocation.instance).as_ref()
                        == Ok(policy)
            } else {
                e.invocation.descriptor.handler.name == "native.review.change"
            }
        })
        .collect::<Vec<_>>();
    let mut current = reviews
        .into_iter()
        .filter_map(|e| {
            super::observed_review(
                context,
                store,
                &e.key,
                &artifact.base,
                &artifact.head,
                &artifact.material_digest,
            )
            .ok()
        })
        .collect::<Vec<_>>();
    if let Some(policy) = &admission_policy {
        let latest = run
            .effects
            .values()
            .filter(|e| {
                e.invocation.descriptor.handler.name == super::native_review_policy::HANDLER
            })
            .filter_map(|e| {
                let receipt = super::observed_review(
                    context,
                    store,
                    &e.key,
                    &artifact.base,
                    &artifact.head,
                    &artifact.material_digest,
                )
                .ok()?;
                Some((
                    super::native_review_policy::ordinal(store, &e.key, policy).ok()?,
                    receipt,
                ))
            })
            .max_by_key(|(ordinal, _)| *ordinal)
            .map(|(_, receipt)| receipt);
        current = latest.into_iter().collect();
        if run.effects.values().any(|e|e.invocation.descriptor.handler.name==super::native_review_policy::HANDLER
            && super::native_review_policy::policy(&e.invocation.instance).as_ref()==Ok(policy)
            && e.invocation.inputs.get("request").is_some_and(|r|r["tree"]==artifact.material_digest && r["head"]==artifact.head)
            && matches!(&e.observation,Some(super::neutral_effects::EffectObservation{stopped:true,outcome:super::neutral_effects::EffectOutcome::Completed{
                result:super::neutral_execution::NeutralResult::Completed{outcome,..}
            }}) if outcome=="exhausted")) {
            return Ok(CorrectionEvaluation::Ready{state:Box::new(state),repeat:None});
        }
    }
    if current.len() != 1 {
        return Err(
            "correction requires one unambiguous current stopped independent review".into(),
        );
    }
    let review: super::ReviewReceipt = current.pop().expect("one current review");
    let source = |provider: &str, resource: String, revision: String| FindingSource {
        provider: provider.into(),
        resource,
        revision,
    };
    let mut receipts = vec![CorrectionReceipt {
        source: source(
            "native_review",
            context.workspace_path().to_string_lossy().into_owned(),
            content_digest(&review)?,
        ),
        artifact: artifact.clone(),
        criterion: "independent_review".into(),
        verdict: match review.verdict {
            CodeVerdict::Pass => EvidenceVerdict::Passed,
            CodeVerdict::Findings => EvidenceVerdict::CorrectionRequired,
            CodeVerdict::Uncertain => EvidenceVerdict::NeedsDecision,
        },
        reason: review.body,
    }];
    for check in checks {
        let receipt =
            match super::native_policy_checks::assessment(context, store, effect, &check, artifact)
            {
                Ok(receipt) => receipt,
                Err(super::native_git::NativeFailure::Refused(reason)) => return Err(reason),
                Err(super::native_git::NativeFailure::Uncertain(reason)) => {
                    return Ok(CorrectionEvaluation::Reconcile {
                        effect: effect.key.digest.clone(),
                        reason,
                    });
                }
            };
        receipts.push(CorrectionReceipt {
            source: source("native_check", check.clone(), content_digest(&receipt)?),
            artifact: artifact.clone(),
            criterion: format!("check:{check}"),
            verdict: if receipt.exit_code == 0 {
                EvidenceVerdict::Passed
            } else {
                EvidenceVerdict::CorrectionRequired
            },
            reason: format!("configured check {check} exited {}", receipt.exit_code),
        });
    }
    context.require_artifact(&artifact.head, &artifact.material_digest)?;
    super::correction_generation::require_artifact(store, &effect.key.fence.run, artifact)?;
    let findings = if let Some(admission_policy) = &admission_policy {
        super::correction_policy::findings(
            &super::native_host::NativeBinding {
                context: context.clone(),
                store: store.clone(),
            },
            effect,
            &state,
            admission_policy,
        )?
    } else {
        Vec::new()
    };
    policy.assess(
        &state,
        CorrectionObservation::Evaluation {
            artifact: artifact.clone(),
            receipts,
            findings,
        },
    )
}
