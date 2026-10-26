//! MCP permission domains. Catalogue names are not the persisted policy contract.
use schemars::JsonSchema;
use serde::{Deserialize, Deserializer, Serialize};

#[derive(Clone, Debug, PartialEq, Eq, Serialize, JsonSchema)]
pub(crate) struct McpCapabilities {
    #[schemars(extend("x-setting" = {"sensitivity":"public","apply":"live","effect":"Updates MCP capability denials for the current owner.","operations":["get","set"],"confirmationClass":"ordinary"}))]
    pub denied: Vec<String>,
    /// Retained until the user resolves them; never silently discard old policy.
    #[schemars(extend("x-not-setting" = "Unresolved legacy MCP policy migration diagnostics"))]
    pub migration_unknown_tools: Vec<String>,
}

impl Default for McpCapabilities {
    fn default() -> Self {
        Self {
            denied: vec!["settings".into(), "diagnostics".into()],
            migration_unknown_tools: Vec::new(),
        }
    }
}

impl<'de> Deserialize<'de> for McpCapabilities {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        #[derive(Deserialize)]
        #[serde(untagged)]
        enum Input {
            Current {
                denied: Vec<String>,
                #[serde(default)]
                migration_unknown_tools: Vec<String>,
            },
            Legacy(Vec<String>),
        }
        Ok(match Input::deserialize(deserializer)? {
            Input::Current {
                denied,
                migration_unknown_tools,
            } => Self {
                denied,
                migration_unknown_tools,
            },
            Input::Legacy(names) => Self::from_legacy(&names),
        })
    }
}

impl McpCapabilities {
    pub(crate) fn from_legacy(names: &[String]) -> Self {
        let mut result = Self {
            denied: Vec::new(),
            migration_unknown_tools: Vec::new(),
        };
        for name in names {
            if let Some(capability) = legacy_capability(name) {
                if !result.denied.iter().any(|denied| denied == capability) {
                    result.denied.push(capability.into());
                }
            } else if !result.migration_unknown_tools.contains(name) {
                tracing::warn!(tool = %name, "Unknown legacy MCP tool denial; retained in migration diagnostics");
                result.migration_unknown_tools.push(name.clone());
            }
        }
        result
    }

    pub(crate) fn allows(&self, tool: &str, action: Option<&str>) -> bool {
        let Some(parent) = tool_capability(tool) else {
            return true;
        };
        !self.denied.iter().any(|denied| {
            denied == parent
                || action
                    .is_some_and(|action| action_capability(tool, action) == Some(denied.as_str()))
        })
    }
}

fn legacy_capability(name: &str) -> Option<&'static str> {
    match name {
        "task" => Some("agent_tasks"),
        "story" => Some("workflow.story"),
        "workflow_run" => Some("workflow.run"),
        "workflow_story_create" => Some("workflow.proposals"),
        "workflow_report" => Some("workflow.reports"),
        "workflow_launch" => Some("workflow.launch"),
        "design_mode" => Some("design"),
        other => tool_capability(other),
    }
}

fn tool_capability(tool: &str) -> Option<&'static str> {
    match tool {
        "session" => Some("session"),
        "agent" => Some("agent"),
        "repo" => Some("repo"),
        "workflow" => Some("workflow"),
        "automations" => Some("automations"),
        "ui" => Some("ui"),
        "secret" => Some("secret"),
        "settings" => Some("settings"),
        "progress" => Some("progress"),
        "debug" => Some("diagnostics"),
        "plugin_dev_guide" => Some("plugin_reference"),
        "remote" => Some("remote_deployment"),
        "voice" => Some("voice"),
        "telegram" => Some("telegram"),
        _ => None,
    }
}

fn action_capability(tool: &str, action: &str) -> Option<&'static str> {
    match (tool, action) {
        ("agent", "task_get" | "task_cancel") => Some("agent_tasks"),
        ("workflow", "story") => Some("workflow.story"),
        ("workflow", "run") => Some("workflow.run"),
        ("workflow", "proposal_create") => Some("workflow.proposals"),
        ("workflow", "report") => Some("workflow.reports"),
        ("workflow", "launch") => Some("workflow.launch"),
        ("ui", action) if action.starts_with("design_") => Some("design"),
        _ => None,
    }
}

/// Settings inventory. Domains own their nested permission switches; no gate is hidden.
pub(crate) fn catalog() -> Vec<serde_json::Value> {
    [
        ("session", "Terminal sessions", "Create and control terminal sessions.", "session", "always"),
        ("agent", "Peers and mail", "Launch peers and exchange durable messages, including task actions.", "agent", "always"),
        ("agent_tasks", "Background task outcomes", "Read and cancel task outcomes, without killing terminals.", "agent", "always"),
        ("repo", "Repositories", "Manage repositories and worktrees.", "repo", "always"),
        ("workflow", "Project workflows", "All story, run, proposal, report and launch actions.", "workflow", "always"),
        ("workflow.story", "Stories and plans", "Read and update native stories and plans.", "workflow", "always"),
        ("workflow.run", "Workflow runs", "Start and inspect published workflow graphs.", "workflow", "always"),
        ("workflow.proposals", "Workflow story proposals", "Create idempotent coordinator proposals.", "workflow", "always"),
        ("workflow.reports", "Workflow reports", "Submit caller-bound attempt outcomes.", "workflow", "always"),
        ("workflow.launch", "Workflow agent launch", "Launch pinned workflow attempts.", "workflow", "always"),
        ("automations", "Scheduled automations", "Manage scheduled definitions and runs.", "automations", "always"),
        ("ui", "User interaction", "Tabs, notices and confirmation, including Design Mode actions.", "ui", "always"),
        ("design", "Design Mode", "Browser inspection; existing extension host grants remain required.", "ui", "always"),
        ("secret", "Secrets", "Collect and use secrets through private forms.", "secret", "always"),
        ("settings", "Settings access", "Read and propose settings; trusted UI opt-in is also required.", "settings", "always"),
        ("progress", "Progress tracking", "Record outcomes; global Progress tracking must also be enabled.", "progress", "always"),
        ("diagnostics", "Diagnostics / JS inspection", "Inspect diagnostics and execute frontend inspection scripts.", "debug", "rare"),
        ("plugin_reference", "Plugin authoring reference", "Read the plugin development guide.", "plugin_dev_guide", "rare"),
        ("voice", "Spoken replies", "Caller-bound speech; voice configuration remains required.", "voice", "conditional"),
        ("telegram", "Telegram", "Send external messages; requires Telegram configuration.", "telegram", "conditional"),
        ("remote_deployment", "Remote daemon deployment", "Preview and update connected remote daemons with deployment consent.", "remote", "conditional"),
    ].into_iter().map(|(id, title, description, domain, section)| serde_json::json!({"name":id,"title":title,"description":description,"domain":domain,"section":section,"parent": match id { "agent_tasks" => Some("agent"), "design" => Some("ui"), id if id.starts_with("workflow.") => Some("workflow"), _ => None }})).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    // Catches: a new capability escaping its approved domain as a stray flat row.
    #[test]
    fn every_capability_has_an_approved_domain_and_section() {
        let sections = [
            (
                "always",
                vec![
                    "session",
                    "agent",
                    "repo",
                    "workflow",
                    "automations",
                    "ui",
                    "secret",
                    "settings",
                    "progress",
                ],
            ),
            ("rare", vec!["debug", "plugin_dev_guide"]),
            ("conditional", vec!["voice", "telegram", "remote"]),
        ];
        let catalog = catalog();
        for (section, domains) in &sections {
            let actual: Vec<_> = catalog
                .iter()
                .filter(|c| c["section"] == *section && c["parent"].is_null())
                .map(|c| c["domain"].as_str().unwrap())
                .collect();
            assert_eq!(&actual, domains);
        }
        for capability in &catalog {
            let domain = capability["domain"]
                .as_str()
                .expect("capability has no domain");
            let section = capability["section"]
                .as_str()
                .expect("capability has no section");
            assert!(
                sections
                    .iter()
                    .any(|(name, domains)| *name == section && domains.contains(&domain)),
                "{capability}"
            );
            if let Some(parent) = capability["parent"].as_str() {
                assert!(
                    catalog.iter().any(|c| c["name"] == parent
                        && c["domain"] == domain
                        && c["parent"].is_null()),
                    "orphan: {capability}"
                );
            }
        }
    }

    // Catches: merging a former tool silently widens a saved denial, or drops unknown names.
    #[test]
    fn legacy_denials_migrate_once_without_authority_expansion() {
        for (old, tool, denied_action, sibling) in [
            ("task", "agent", "task_get", "list_peers"),
            ("story", "workflow", "story", "run"),
            ("workflow_run", "workflow", "run", "story"),
            (
                "workflow_story_create",
                "workflow",
                "proposal_create",
                "run",
            ),
            ("workflow_report", "workflow", "report", "run"),
            ("workflow_launch", "workflow", "launch", "run"),
            ("design_mode", "ui", "design_status", "toast"),
        ] {
            let mut legacy = serde_json::to_value(crate::config::AppConfig::default()).unwrap();
            legacy.as_object_mut().unwrap().remove("mcp_capabilities");
            legacy["disabled_native_tools"] = serde_json::json!([old, "unknown_tool"]);
            let config: crate::config::AppConfig = serde_json::from_value(legacy).unwrap();
            assert!(
                !config.mcp_capabilities.allows(tool, Some(denied_action)),
                "{old}"
            );
            assert!(config.mcp_capabilities.allows(tool, Some(sibling)), "{old}");
            assert_eq!(
                config.mcp_capabilities.migration_unknown_tools,
                ["unknown_tool"]
            );
            let persisted = serde_json::to_value(&config).unwrap();
            assert!(persisted.get("disabled_native_tools").is_none());
            let again: crate::config::AppConfig = serde_json::from_value(persisted).unwrap();
            assert_eq!(again.mcp_capabilities, config.mcp_capabilities);
        }
        for (old, tool, action) in [
            ("agent", "agent", "task_cancel"),
            ("workflow", "workflow", "report"),
            ("ui", "ui", "design_start"),
            ("debug", "debug", "invoke_js"),
            ("remote", "remote", "update"),
        ] {
            let policy = McpCapabilities::from_legacy(&[old.into()]);
            assert!(!policy.allows(tool, Some(action)), "{old}");
        }
    }
}
