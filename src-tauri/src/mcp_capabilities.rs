//! MCP permission domains. Catalogue names are not the persisted policy contract.
use schemars::JsonSchema;
use serde::{Deserialize, Deserializer, Serialize};

/// Legacy denial of the settings tool. Settings access is the host-only
/// `settings_mcp_enabled` opt-in; this name survives only for migration.
const SETTINGS_DENIAL: &str = "settings";

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
            denied: vec!["diagnostics".into()],
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
                denied: migrate_split_denials(denied),
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

    /// Remove a saved `settings` denial. The caller turns it into the one opt-in
    /// (`settings_mcp_enabled = false`), so the denial is never a hidden second gate.
    pub(crate) fn take_settings_denial(&mut self) -> bool {
        let before = self.denied.len();
        self.denied.retain(|denied| denied != SETTINGS_DENIAL);
        self.denied.len() != before
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

/// Stories left `workflow` for their own tool (#2804-6bd7). A saved member denial keeps
/// denying the same actions under the new parent capability, so no saved denial widens.
fn migrate_split_denials(denied: Vec<String>) -> Vec<String> {
    let mut migrated: Vec<String> = Vec::with_capacity(denied.len());
    for name in denied {
        let name = if name == "workflow.story" {
            "story".to_owned()
        } else {
            name
        };
        if !migrated.contains(&name) {
            migrated.push(name);
        }
    }
    migrated
}

fn legacy_capability(name: &str) -> Option<&'static str> {
    match name {
        "task" => Some("agent_tasks"),
        "design_mode" => Some("design"),
        // Not a capability any more; the load migration turns this denial into the opt-in.
        "settings" => Some(SETTINGS_DENIAL),
        other => tool_capability(other),
    }
}

fn tool_capability(tool: &str) -> Option<&'static str> {
    match tool {
        "session" => Some("session"),
        "agent" => Some("agent"),
        "repo" => Some("repo"),
        "story" => Some("story"),
        "workflow" => Some("workflow"),
        "automations" => Some("automations"),
        "ui" => Some("ui"),
        "secret" => Some("secret"),
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
        ("story", "Stories and plans", "Read and update native stories, plans and Ideas.", "story", "always"),
        ("workflow", "Project workflows", "Neutral workflow authoring, publication and runs.", "workflow", "always"),
        ("automations", "Scheduled automations", "Manage scheduled definitions and runs.", "automations", "always"),
        ("ui", "User interaction", "Tabs, notices and confirmation, including Design Mode actions.", "ui", "always"),
        ("design", "Design Mode", "Browser inspection; sidebar host consent remains required.", "ui", "always"),
        ("secret", "Secrets", "Collect and use secrets through private forms.", "secret", "always"),
        ("settings", "Settings access", "Read and propose settings. Only the trusted Settings UI can turn this on.", "settings", "always"),
        ("progress", "Progress tracking", "Record outcomes; global Progress tracking must also be enabled.", "progress", "always"),
        ("diagnostics", "Diagnostics / JS inspection", "Inspect diagnostics and execute frontend inspection scripts.", "debug", "rare"),
        ("plugin_reference", "Plugin authoring reference", "Read the plugin development guide.", "plugin_dev_guide", "rare"),
        ("voice", "Spoken replies", "Caller-bound speech; voice configuration remains required.", "voice", "conditional"),
        ("telegram", "Telegram", "Send external messages; requires Telegram configuration.", "telegram", "conditional"),
        ("remote_deployment", "Remote daemon deployment", "Preview and update connected remote daemons with deployment consent.", "remote", "conditional"),
    ].into_iter().map(|(id, title, description, domain, section)| serde_json::json!({"name":id,"title":title,"description":description,"domain":domain,"section":section,"parent": match id { "agent_tasks" => Some("agent"), "design" => Some("ui"), _ => None }})).collect()
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
                    "story",
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
            ("story", "story", "list_plans"),
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

    // Catches: the story split widening a saved `workflow.story` denial, or a legacy
    // `story` denial no longer reaching the new tool (#2804-6bd7).
    #[test]
    fn saved_story_denials_follow_stories_into_their_own_tool() {
        let config: crate::config::AppConfig = serde_json::from_value({
            let mut saved = serde_json::to_value(crate::config::AppConfig::default()).unwrap();
            saved["mcp_capabilities"] = serde_json::json!({"denied": ["workflow.story", "story"]});
            saved
        })
        .unwrap();
        let policy = &config.mcp_capabilities;
        assert_eq!(policy.denied, ["story"]);
        assert!(!policy.allows("story", Some("list_plans")));
        assert!(policy.allows("workflow", Some("run")));
        let workflow_only = McpCapabilities::from_legacy(&["workflow".into()]);
        assert!(workflow_only.allows("story", Some("list_plans")));
        assert!(!workflow_only.allows("workflow", Some("run")));
    }
}
