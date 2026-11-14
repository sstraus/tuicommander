//! Local installed-family discovery. This never resolves accounts over the network.
use crate::state::AppState;
use crate::workflows::catalog::{ActivityDescriptor, HandlerRef, WorkflowCatalog};
use crate::workflows::gitlab::{GitLabInstall, Operation};
use crate::workflows::jira::{JiraBinding, JiraOperation};
use serde_json::Value;
use std::collections::BTreeMap;

#[derive(Clone)]
pub(super) enum FamilyBinding {
    Jira(Box<JiraBinding>),
    GitLab(Box<GitLabInstall>),
    Telegram(String),
    /// The run's project path selects the account; no installed configuration.
    GitHub,
    Model,
    Approval,
}

#[derive(Clone, Default)]
pub(super) struct InstalledFamilies {
    pub catalog: WorkflowCatalog,
    pub bindings: BTreeMap<HandlerRef, FamilyBinding>,
}

impl InstalledFamilies {
    fn add(
        &mut self,
        descriptor: ActivityDescriptor,
        binding: FamilyBinding,
    ) -> Result<(), String> {
        self.catalog.register(descriptor.clone())?;
        if self.bindings.insert(descriptor.handler, binding).is_some() {
            return Err("installed family handler collision".into());
        }
        Ok(())
    }

    pub fn load(state: &AppState) -> Result<Self, String> {
        let mut installed = Self::default();
        installed.add(
            super::native_approval::descriptor(),
            FamilyBinding::Approval,
        )?;
        for op in super::neutral_github_review::OPERATIONS {
            installed.add(
                super::neutral_github_review::descriptor(op),
                FamilyBinding::GitHub,
            )?;
        }
        for plugin in ["jira", "gitlab"] {
            if crate::plugins::read_single_manifest(plugin).is_err() {
                continue;
            }
            let data = plugin_data(plugin, "work-items.json")?;
            let defaults = plugin_data(plugin, "config.json")?;
            let Some(accounts) = data.get("accounts").and_then(Value::as_array) else {
                continue;
            };
            let metadata = crate::accounts::installed_accounts()?;
            for account in accounts {
                let (Some(id), Some(host)) = (text(account, "id"), text(account, "host")) else {
                    continue;
                };
                if !metadata.iter().any(|a| {
                    a.id == id
                        && a.provider == plugin
                        && a.host.trim_end_matches('/') == host.trim_end_matches('/')
                }) {
                    continue;
                }
                if plugin == "jira" {
                    let Some(project) =
                        text(account, "projectId").or_else(|| text(&defaults, "projectId"))
                    else {
                        continue;
                    };
                    for op in [
                        JiraOperation::Lookup,
                        JiraOperation::Search,
                        JiraOperation::Create,
                        JiraOperation::Update,
                        JiraOperation::Comment,
                        JiraOperation::Transitions,
                        JiraOperation::Transition,
                    ] {
                        let binding = JiraBinding::new(
                            format!("{}/", host.trim_end_matches('/')),
                            plugin.into(),
                            id.into(),
                            project.into(),
                            op,
                        )?;
                        installed.add(
                            binding.descriptor.clone(),
                            FamilyBinding::Jira(Box::new(binding)),
                        )?;
                    }
                } else {
                    let Some(path) =
                        text(account, "project").or_else(|| text(&defaults, "project"))
                    else {
                        continue;
                    };
                    let resolved = &account["resolvedProject"];
                    if text(resolved, "path") != Some(path) {
                        continue;
                    }
                    let Some(project) = resolved["id"].as_u64().filter(|id| *id > 0) else {
                        continue;
                    };
                    let binding = GitLabInstall::new(
                        format!("{}/api/v4/", host.trim_end_matches('/')),
                        project,
                        id.into(),
                    )?;
                    for op in [
                        Operation::Issue,
                        Operation::MergeRequest,
                        Operation::DiffVersion,
                        Operation::Pipeline,
                        Operation::ReviewNote,
                    ] {
                        installed.add(
                            binding.descriptor(op),
                            FamilyBinding::GitLab(Box::new(binding.clone())),
                        )?;
                    }
                }
            }
        }
        let paths = crate::telegram::settings::paths().map_err(|e| e.to_string())?;
        if let Some(config) = crate::telegram::Config::load(&paths).map_err(|e| e.to_string())? {
            let host = super::neutral_telegram::telegram_host(config.bot_alias.clone())?;
            for descriptor in host.descriptors.into_values() {
                installed.add(
                    descriptor,
                    FamilyBinding::Telegram(config.bot_alias.clone()),
                )?;
            }
        }
        let model_configured = {
            let config = state.config.read();
            !config.ego_executable.trim().is_empty() && !config.ego_profile.trim().is_empty()
        };
        if model_configured {
            let runtime = crate::acp_commands::ego_config(state).map_err(|e| e.message)?;
            let descriptor = super::native_model::descriptor(&runtime)?;
            installed.add(descriptor, FamilyBinding::Model)?;
            installed.add(
                super::native_agent::descriptor(&runtime)?,
                FamilyBinding::Model,
            )?;
        }
        Ok(installed)
    }
}

fn plugin_data(plugin: &str, path: &str) -> Result<Value, String> {
    let Some(raw) = crate::plugins::read_plugin_data(plugin.into(), path.into())? else {
        return Ok(Value::Null);
    };
    if raw.len() > 1_048_576 {
        return Err("installed plugin configuration exceeds discovery bound".into());
    }
    serde_json::from_str(&raw).map_err(|e| format!("invalid {plugin} configuration: {e}"))
}

fn text<'a>(value: &'a Value, field: &str) -> Option<&'a str> {
    value.get(field)?.as_str().filter(|s| !s.trim().is_empty())
}
