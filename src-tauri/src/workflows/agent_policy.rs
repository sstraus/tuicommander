//! Typed host routing authority. Import is an explicit operator declaration;
//! a source label is provenance, not an inferred installation/approval claim.
use super::{AgentExecution, AgentRole, NodeKind, PublishedWorkflow};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

#[derive(Clone, Copy, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum AgentScope {
    IsolatedWorkspace,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct AcpRuntimeRoute {
    pub executable: std::path::PathBuf,
    pub machine_profile: String,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct RoleRoute {
    pub role: AgentRole,
    pub execution: AgentExecution,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub acp_runtime: Option<AcpRuntimeRoute>,
    pub model: String,
    pub effort: String,
    pub capabilities: Vec<String>,
    pub scope: AgentScope,
    pub output_contract_version: u16,
    #[serde(default)]
    pub default_for_role: bool,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct AgentPolicyInput {
    pub source: String,
    pub source_revision: String,
    pub routes: Vec<RoleRoute>,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct PinnedRoleRoute {
    pub route: RoleRoute,
    /// Detect changed saved argv, env, binary and launch-policy settings. No
    /// secret-bearing environment value is copied into the workflow ledger.
    pub settings_sha256: Option<String>,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct AgentPolicy {
    pub revision: i64,
    pub source: String,
    pub source_revision: String,
    pub sha256: String,
    pub routes: Vec<PinnedRoleRoute>,
}

pub(crate) fn settings_fingerprint(profile: &str) -> Result<String, String> {
    let config = crate::config::load_agents_config();
    let (agent, saved) = config.require_named_run_config(profile)?;
    let settings = &config.agents[agent];
    // Only the selected launch envelope is pinned. Saved defaults, other
    // profiles and idle/lifecycle preferences do not change this launch.
    let launch_text = if crate::launch_instructions::AGENTS.contains(&agent) {
        crate::launch_instructions::effective_text_for(
            &crate::config::load_app_config(),
            &config,
            agent,
            false,
        )
    } else {
        String::new()
    };
    let inputs = serde_json::json!({
        "agent": agent,
        "command": saved.command,
        "args": saved.args,
        "model": saved.model,
        "env": saved.env,
        "native_status_signals": settings.native_status_signals.unwrap_or(true),
        "prevent_alt_screen": settings.prevent_alt_screen.unwrap_or(true),
        "skip_trust_dialog": settings.skip_trust_dialog.unwrap_or(true),
        "ego_mode": if agent == "ego" { settings.ego_mode } else { None },
        "ego_sandbox": if agent == "ego" { settings.ego_sandbox } else { None },
        "launch_instructions": launch_text,
    });
    // Saved env maps must have stable ordering across process loads.
    let bytes = serde_json::to_vec(&canonical(inputs)).map_err(|e| e.to_string())?;
    Ok(hex::encode(Sha256::digest(bytes)))
}

fn canonical(value: serde_json::Value) -> serde_json::Value {
    match value {
        serde_json::Value::Object(map) => {
            let sorted: std::collections::BTreeMap<_, _> = map.into_iter().collect();
            serde_json::Value::Object(sorted.into_iter().map(|(k, v)| (k, canonical(v))).collect())
        }
        serde_json::Value::Array(items) => {
            serde_json::Value::Array(items.into_iter().map(canonical).collect())
        }
        other => other,
    }
}

/// PTY has no config-option acknowledgement. Import therefore requires the
/// saved launch envelope to state the exact model and an explicit effort flag;
/// a friendly profile name is not evidence of either setting.
fn validate_pty_settings(profile: &str, route: &RoleRoute) -> Result<(), String> {
    let config = crate::config::load_agents_config();
    let (agent, saved) = config.require_named_run_config(profile)?;
    if saved.model.as_deref() != Some(route.model.as_str()) {
        return Err("PTY policy model must equal the saved explicit model setting".into());
    }
    let effort = &route.effort;
    let explicit = saved
        .args
        .windows(2)
        .any(|pair| pair[0] == "--effort" && pair[1] == *effort)
        || saved
            .args
            .iter()
            .any(|arg| arg == &format!("--effort={effort}"))
        || (agent == "codex"
            && saved.args.windows(2).any(|pair| {
                matches!(pair[0].as_str(), "-c" | "--config")
                    && pair[1] == format!("model_reasoning_effort=\"{effort}\"")
            }));
    if !explicit {
        return Err("PTY policy effort must be explicit in the saved launch arguments".into());
    }
    Ok(())
}

impl PinnedRoleRoute {
    pub(crate) fn validate_live_settings(&self) -> Result<(), String> {
        if let AgentExecution::Pty { profile } = &self.route.execution
            && self.settings_sha256.as_deref() != Some(settings_fingerprint(profile)?.as_str())
        {
            return Err("saved launch settings differ from the run-pinned host routing policy; no model/profile substitution".into());
        }
        Ok(())
    }
}

impl AgentPolicy {
    pub(crate) fn prepare(input: AgentPolicyInput, revision: i64) -> Result<Self, String> {
        if input.source.trim().is_empty()
            || input.source.len() > 4096
            || input.source_revision.trim().is_empty()
            || input.source_revision.len() > 256
            || input.routes.is_empty()
            || input.routes.len() > 32
        {
            return Err("invalid host agent routing policy".into());
        }
        let mut routes: Vec<PinnedRoleRoute> = vec![];
        for route in input.routes {
            if route.model.trim().is_empty()
                || route.model.len() > 256
                || route.effort.trim().is_empty()
                || route.effort.len() > 80
                || route.output_contract_version != super::PROMPT_CONTRACT_VERSION
                || route.capabilities.len() > 32
                || route
                    .capabilities
                    .iter()
                    .any(|c| c.trim().is_empty() || c.len() > 80)
                || routes.iter().any(|p| {
                    p.route.role == route.role
                        && (p.route.execution == route.execution
                            || (p.route.default_for_role && route.default_for_role))
                })
            {
                return Err("invalid or ambiguous role routing contract".into());
            }
            let settings_sha256 = match &route.execution {
                AgentExecution::Pty { profile } => {
                    if route.acp_runtime.is_some() {
                        return Err("PTY route cannot carry an ACP runtime".into());
                    }
                    validate_pty_settings(profile, &route)?;
                    Some(settings_fingerprint(profile)?)
                }
                AgentExecution::Acp {
                    profile,
                    model,
                    effort,
                } => {
                    if profile.trim().is_empty() || *model != route.model || *effort != route.effort
                    {
                        return Err("ACP route must pin its exact profile/model/effort".into());
                    }
                    if route
                        .acp_runtime
                        .as_ref()
                        .is_none_or(|runtime| !runtime.executable.is_absolute())
                    {
                        return Err(
                            "ACP host policy requires its exact absolute runtime executable".into(),
                        );
                    }
                    None
                }
            };
            routes.push(PinnedRoleRoute {
                route,
                settings_sha256,
            });
        }
        let mut result = Self {
            revision,
            source: input.source,
            source_revision: input.source_revision,
            sha256: String::new(),
            routes,
        };
        result.sha256 = hex::encode(Sha256::digest(
            serde_json::to_vec(&result).map_err(|e| e.to_string())?,
        ));
        Ok(result)
    }

    pub(crate) fn select(
        &self,
        role: AgentRole,
        requested: Option<&AgentExecution>,
        capabilities: &[String],
    ) -> Result<&PinnedRoleRoute, String> {
        let candidates = self
            .routes
            .iter()
            .filter(|p| {
                p.route.role == role
                    && requested.is_none_or(|execution| execution == &p.route.execution)
            })
            .collect::<Vec<_>>();
        let selected = match candidates.as_slice() {
            [only] => *only,
            several if requested.is_none() => {
                let defaults = several
                    .iter()
                    .filter(|p| p.route.default_for_role)
                    .collect::<Vec<_>>();
                match defaults.as_slice() {
                    [only] => **only,
                    _ => return Err(format!("no unique approved route for {role:?}")),
                }
            }
            _ => {
                return Err(format!(
                    "exact requested route is not approved for {role:?}"
                ));
            }
        };
        if capabilities
            .iter()
            .any(|c| !selected.route.capabilities.contains(c))
        {
            return Err("Agent tool contract exceeds its pinned host policy".into());
        }
        Ok(selected)
    }

    pub(crate) fn validate_definition(&self, definition: &PublishedWorkflow) -> Result<(), String> {
        for node in &definition.graph.nodes {
            if let NodeKind::Agent {
                role,
                execution,
                capabilities,
                ..
            } = &node.kind
            {
                self.select(*role, execution.as_ref(), capabilities)?;
            }
        }
        Ok(())
    }
}

/// Never re-resolve a role against the latest policy after a run has started.
pub(crate) fn pinned_agent_route<'a>(
    run: &'a super::RunSnapshot,
    attempt: &super::NodeAttempt,
) -> Result<(&'a AgentPolicy, &'a PinnedRoleRoute), String> {
    let graph = run
        .graph_executions
        .iter()
        .find(|graph| {
            graph.target_id == attempt.story_id
                && graph
                    .definition
                    .graph
                    .nodes
                    .iter()
                    .any(|n| n.id == attempt.node_id)
        })
        .ok_or("attempt has no pinned graph execution")?;
    let policy = graph
        .contract
        .as_ref()
        .and_then(|c| c.agent_policy.as_ref())
        .ok_or(
            "effective host routing policy is not pinned; import a policy and start a new run",
        )?;
    let node = graph
        .definition
        .graph
        .nodes
        .iter()
        .find(|node| node.id == attempt.node_id)
        .ok_or("pinned Agent node missing")?;
    let NodeKind::Agent {
        role,
        execution,
        capabilities,
        ..
    } = &node.kind
    else {
        return Err("attempt is not an Agent node".into());
    };
    Ok((
        policy,
        policy.select(*role, execution.as_ref(), capabilities)?,
    ))
}
