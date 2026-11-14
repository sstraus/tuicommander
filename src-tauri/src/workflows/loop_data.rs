//! Typed loop-carried results. Iteration scheduling remains with the run owner.

// DEFERRED (2026-11-10) — contract entry points await the 004-2809 runtime consumer.
#![cfg_attr(
    not(test),
    expect(dead_code, reason = "003-e678 contract; first consumer 004-2809")
)]

use super::catalog::ObjectSchema;
use super::collection::CollectionIdentity;
use super::data_binding::{
    DataSource, Datum, InputBindings, ResolvedInputs, resolve_bindings, validate_bindings,
};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub(crate) struct LoopContract {
    pub body: usize,
    pub max_iterations: u32,
    pub carried: ObjectSchema,
    pub initial: InputBindings,
    pub result: InputBindings,
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct LoopResult {
    pub identity: CollectionIdentity,
    /// Zero is the initial state; one is the first completed traversal.
    pub iteration: u32,
    pub fields: ResolvedInputs,
}

impl LoopContract {
    pub(crate) fn validate(
        &self,
        workflow: &ObjectSchema,
        outputs: &[&ObjectSchema],
        body_inputs: &ObjectSchema,
        scopes: &super::scope_data::ScopeSchemas,
    ) -> Result<(), String> {
        if self.max_iterations == 0 || self.carried.fields.is_empty() {
            return Err("loop requires a positive iteration bound and typed carried fields".into());
        }
        self.carried.check_schema("loop carried state")?;
        for (name, field) in &self.carried.fields {
            let target = body_inputs
                .fields
                .get(name)
                .ok_or("loop carried input not found")?;
            if field.value_type != target.value_type
                || (target.required
                    && target.default.is_none()
                    && !field.required
                    && field.default.is_none())
            {
                return Err(format!("loop carried input {name} is incompatible"));
            }
        }
        validate_bindings(
            &self.initial,
            workflow,
            outputs,
            &self.carried,
            self.body,
            scopes,
        )?;
        if self.result.values().any(|source| !matches!(source, DataSource::ActivityOutput { activity, .. } if *activity == self.body)) {
            return Err("loop result must project the current body outputs".into());
        }
        validate_bindings(
            &self.result,
            workflow,
            outputs,
            &self.carried,
            usize::MAX,
            scopes,
        )
    }

    pub(crate) fn resolve(
        &self,
        identity: CollectionIdentity,
        iteration: u32,
        workflow: &BTreeMap<String, Datum>,
        outputs: &BTreeMap<usize, BTreeMap<String, Datum>>,
        scopes: &super::scope_data::ScopeValues,
    ) -> Result<LoopResult, String> {
        if identity.run.is_empty() || identity.scope.is_empty() || identity.generation == 0 {
            return Err("loop requires run/scope/generation identity".into());
        }
        if iteration > self.max_iterations {
            return Err("loop iteration bound exhausted".into());
        }
        let bindings = if iteration == 0 {
            &self.initial
        } else {
            &self.result
        };
        let fields = resolve_bindings(bindings, workflow, outputs, &self.carried, scopes)?;
        Ok(LoopResult {
            identity,
            iteration,
            fields,
        })
    }
}
