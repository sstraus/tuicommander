//! Versioned script protocol. The descriptor owns schemas and the executable;
//! instance configuration owns bounded execution policy, never graph routing.
use super::super::neutral_effects::EffectKey;
use crate::workflows::catalog::{ActivityDescriptor, ActivityInstance, ReusableTarget};
use crate::workflows::command::{CommandDefinition, CommandOutput, CommandTarget, Invocation};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::{BTreeMap, BTreeSet};

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case", deny_unknown_fields)]
pub(crate) enum OutputMode {
    Structured,
    ExitStatus { outcome: String },
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum RetryPolicy {
    Never,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case", deny_unknown_fields)]
pub(crate) enum Reconciliation {
    /// Inspect the original execution/effect using this host-owned procedure.
    Manual {
        procedure: String,
    },
    ReadOnly,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct ScriptPolicy {
    pub version: u16,
    pub timeout_secs: u32,
    pub input_bytes: usize,
    pub stdout_bytes: usize,
    pub stderr_bytes: usize,
    pub artifact_limit: usize,
    /// Resolved names are claimed by the neutral ledger, not a second queue.
    pub resources: BTreeSet<String>,
    pub environment_refs: BTreeMap<String, String>,
    pub secret_refs: BTreeMap<String, String>,
    pub mutation: bool,
    pub retry: RetryPolicy,
    pub reconciliation: Reconciliation,
    pub output: OutputMode,
}

impl ScriptPolicy {
    pub fn resolve(
        descriptor: &ActivityDescriptor,
        instance: &ActivityInstance,
    ) -> Result<(Self, Invocation), String> {
        if descriptor.handler != super::handler() || instance.descriptor != descriptor.id {
            return Err("script handler or descriptor identity mismatch".into());
        }
        let config = descriptor
            .config
            .resolve(&instance.config, "script config")?;
        let policy: Self =
            serde_json::from_value(config.get("script").ok_or("script policy missing")?.clone())
                .map_err(|e| format!("invalid script policy: {e}"))?;
        let Some(ReusableTarget::Script { invocation }) = &descriptor.target else {
            return Err("script handler requires a pinned script target".into());
        };
        if policy.version != 1
            || !(1..=86_400).contains(&policy.timeout_secs)
            || !(1..=1_048_576).contains(&policy.input_bytes)
            // execute_process captures at most 1 MiB per stream. A full capture
            // cannot prove absence of truncation, so accept strictly less.
            || !(1..1_048_576).contains(&policy.stdout_bytes)
            || !(1..1_048_576).contains(&policy.stderr_bytes)
            || policy.artifact_limit > 64
            || policy.resources.len() > 16
            || policy.resources.iter().any(|s| s.trim().is_empty() || s.len() > 4096)
            || policy.environment_refs.len() + policy.secret_refs.len() > 32
        {
            return Err("invalid script execution limits".into());
        }
        if matches!(&policy.reconciliation, Reconciliation::Manual { procedure } if procedure.trim().is_empty() || procedure.len() > 4096)
        {
            return Err("script reconciliation procedure is empty or too large".into());
        }
        if policy.mutation
            && (policy.resources.is_empty()
                || !matches!(&policy.reconciliation, Reconciliation::Manual { .. }))
        {
            return Err(
                "mutating script requires resource ownership and reconciliation procedure".into(),
            );
        }
        if policy
            .environment_refs
            .keys()
            .any(|k| policy.secret_refs.contains_key(k))
        {
            return Err("script environment and secret destinations overlap".into());
        }
        // Reuse command validation for bounded versioned invocation and names.
        let references = policy
            .environment_refs
            .iter()
            .chain(&policy.secret_refs)
            .map(|(k, v)| (k.clone(), v.clone()))
            .collect();
        CommandDefinition {
            version: 1,
            invocation: invocation.clone(),
            workspace: "assigned".into(),
            target: CommandTarget::Local {
                os: std::env::consts::OS.into(),
                arch: std::env::consts::ARCH.into(),
            },
            resources: vec![],
            secret_refs: references,
            stdin_artifact: None,
            timeout_secs: policy.timeout_secs,
            output: CommandOutput::ExitStatus,
        }
        .validate()?;
        if let OutputMode::ExitStatus { outcome } = &policy.output {
            if !descriptor.outcomes.contains(outcome) {
                return Err("exit-status outcome is undeclared".into());
            }
            descriptor
                .outputs
                .resolve(&BTreeMap::new(), "exit-status outputs")?;
        }
        Ok((policy, invocation.clone()))
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct ScriptRequest {
    pub version: u16,
    pub identity: EffectKey,
    pub config: BTreeMap<String, Value>,
    pub inputs: BTreeMap<String, Value>,
    pub subject: Option<BTreeMap<String, Value>>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct ScriptResult {
    pub version: u16,
    pub identity: EffectKey,
    pub outcome: String,
    pub data: BTreeMap<String, Value>,
    /// Named output ports hold typed immutable artifact references.
    pub artifacts: BTreeMap<String, Value>,
}
