//! One generated envelope and host-issued caller context for every settings transport.
use super::{catalogue::Catalogue, policy::Caller, request::*, service::Service};
use crate::AppState;
use schemars::JsonSchema;
use serde::Deserialize;
use serde_json::{Value, json};
use std::sync::{
    Arc,
    atomic::{AtomicU64, Ordering},
};

#[path = "api_owners.rs"]
mod owners;
#[path = "api_reads.rs"]
mod reads;

#[derive(Deserialize, JsonSchema)]
#[serde(untagged)]
pub enum Action {
    Catalogue(Describe),
    Transaction(Request),
    Perform(Perform),
    Refresh(Refresh),
    Frontend(Frontend),
}

#[derive(Deserialize, JsonSchema)]
#[serde(tag = "action", rename_all = "snake_case", deny_unknown_fields)]
pub enum Describe {
    Describe {
        area: Option<String>,
        store: Option<String>,
        id: Option<String>,
        cursor: Option<String>,
        #[serde(default = "page_size")]
        limit: usize,
    },
}
fn page_size() -> usize {
    64
}

#[derive(Deserialize, JsonSchema)]
#[serde(tag = "action", rename_all = "snake_case")]
pub enum Perform {
    Perform {
        target: Target,
        #[serde(flatten)]
        owner: owners::OwnerAction,
    },
}

#[derive(Deserialize, JsonSchema)]
#[serde(tag = "action", rename_all = "snake_case", deny_unknown_fields)]
pub enum Refresh {
    Refresh { store: String },
}

/// Runtime UI registrations contain identifiers only, never callbacks or launch commands.
#[derive(Deserialize, JsonSchema)]
#[serde(tag = "action", rename_all = "snake_case", deny_unknown_fields)]
pub enum Frontend {
    PublishPluginKeybindingActions { actions: Vec<String> },
}

pub fn request_schema() -> Value {
    let mut schema = super::catalogue::schema::<Action>();
    fn approval(schema: &mut Value) {
        if let Some(properties) = schema.get_mut("properties").and_then(Value::as_object_mut) {
            properties.insert("approval".into(), json!({"type":"string","description":"Opaque, one-use proposal approval; bound to this exact request and caller."}));
            properties.insert("confirm".into(), json!({"type":"boolean","description":"With approval=proposalId, ask the human through the existing confirmation dialog, then replay this exact edit in the same request."}));
        }
        for key in ["anyOf", "oneOf"] {
            if let Some(variants) = schema.get_mut(key).and_then(Value::as_array_mut) {
                for variant in variants {
                    approval(variant);
                }
            }
        }
    }
    approval(&mut schema);
    schema
}

static GENERATION: AtomicU64 = AtomicU64::new(0);

/// Shared by facade commits and the pre-existing AppConfig owner save seam.
/// This cursor never fingerprints credentials or changes an owner's CAS revision.
pub(crate) fn publish_change(state: &AppState, section: &str, applied: bool) -> u64 {
    if matches!(section, "repositories" | "repo_defaults") {
        state.git_cache.refresh_worktree_storage_settings();
    }
    let generation = GENERATION.fetch_add(1, Ordering::SeqCst) + 1;
    state.publish_settings_changed(super::changes::SettingsChanged {
        section: section.to_owned(),
        generation,
        persisted: true,
        applied,
    });
    generation
}

pub struct Settings {
    catalogue: Arc<Catalogue>,
    registry: super::registry::Registry,
    github: super::accounts::GithubSettings,
    connections: super::remote::ConnectionCredentialSettings,
    ego: Arc<super::ego::EgoSettingsStore>,
}

impl Settings {
    pub(crate) fn new(state: Arc<AppState>) -> Result<Self, Failure> {
        let catalogue = Arc::new(
            super::native::native_catalogue().map_err(|_| Error::new("settings", "unavailable"))?,
        );
        let mut registry = super::registry::Registry::with_policy(state.settings_policy.clone());
        macro_rules! register {
            ($owner:expr) => {
                registry.register(Service::new(catalogue.clone(), Arc::new($owner)))?
            };
        }
        register!(crate::config::settings_app::AppSettingsStore::new(
            state.clone()
        ));
        register!(super::agents::AgentsSettingsStore::default());
        register!(super::notifications::NotificationSettingsStore::default());
        register!(super::preferences::PreferencesSettingsStore::default());
        register!(super::repo_defaults::RepoDefaultsSettingsStore::default());
        register!(super::repo_local::RepoLocalSettingsStore::default());
        register!(super::repositories::RepositoriesSettingsStore::default());
        register!(super::telegram::TelegramSettingsStore::default());
        let ego = Arc::new(super::ego::EgoSettingsStore::new(state.clone())?);
        registry.register(Service::new(catalogue.clone(), ego.clone()))?;
        register!(super::registration::RegistrationStore::default());
        register!(super::ui_owners::UiSettingsStore::keybindings_with_actions(
            state.plugin_keybinding_actions.clone()
        ));
        register!(super::ui_owners::UiSettingsStore::pane_layout());
        register!(super::upstreams::UpstreamsSettingsStore::new(
            state.mcp.upstream_registry.clone(),
            state.config.read().services.server.port
        ));
        register!(super::prompts::PromptsSettingsStore::default());
        let weak = Arc::downgrade(&state);
        register!(super::remote::ConnectionsSettingsStore::new(
            state.data_dir.clone(),
            Arc::new(move |connection| {
                if let Some(state) = weak.upgrade() {
                    crate::remote_runtime::teardown_deleted_record(&state, connection);
                }
            })
        ));
        let manager = state.tunnel_manager.clone();
        register!(super::remote::TunnelsSettingsStore::new(
            state.data_dir.clone(),
            Arc::new(move |id| manager.stop_if_running(id))
        ));
        #[cfg(feature = "dictation")]
        {
            #[cfg(feature = "desktop")]
            let speaker = {
                use tauri::Manager;
                state.app_handle.read().as_ref().and_then(|app| {
                    app.try_state::<crate::dictation::DictationState>()
                        .map(|dictation| dictation.speaker.clone())
                })
            };
            #[cfg(not(feature = "desktop"))]
            let speaker = None;
            register!(super::voice::VoiceSettingsStore::new(speaker));
            #[cfg(feature = "desktop")]
            let corrections = {
                use tauri::Manager;
                state.app_handle.read().as_ref().and_then(|app| {
                    app.try_state::<crate::dictation::DictationState>()
                        .map(|dictation| dictation.corrections.clone())
                })
            };
            #[cfg(not(feature = "desktop"))]
            let corrections = None;
            register!(super::corrections::CorrectionsStore::new(corrections));
        }
        let weak = Arc::downgrade(&state);
        let connections = super::remote::ConnectionCredentialSettings::new(
            state.data_dir.clone(),
            Arc::new(move |id| {
                if let Some(state) = weak.upgrade() {
                    crate::remote_runtime::credentials_changed(&state, id);
                }
            }),
        );
        Ok(Self {
            catalogue,
            registry,
            github: super::accounts::GithubSettings::new(state),
            connections,
            ego,
        })
    }

    /// Generation is a process-local invalidation cursor, not an owner CAS revision.
    pub async fn execute(
        &self,
        state: &Arc<AppState>,
        action: Action,
        caller: &Caller,
        approval: Option<&str>,
    ) -> Result<Value, Failure> {
        let (section, durable_success) = match &action {
            Action::Transaction(
                Request::Set { store, .. }
                | Request::Reset { store, .. }
                | Request::Patch { store, .. },
            ) => (Some(store.clone()), false),
            Action::Perform(Perform::Perform { owner, .. }) => {
                (Some(owner.section().to_owned()), owner.success_is_durable())
            }
            _ => (None, false),
        };
        let mut outcome = self.execute_inner(state, action, caller, approval).await;
        let persisted = match &outcome {
            Ok(value) => {
                value["persisted"] == true || (durable_success && value.get("error").is_none())
            }
            Err(error) => error.persisted,
        };
        if let Some(section) = section.filter(|_| persisted) {
            let applied = outcome
                .as_ref()
                .is_ok_and(|value| value["applied"].as_bool().unwrap_or(true));
            let generation = publish_change(state, &section, applied);
            match &mut outcome {
                Ok(value) => {
                    if let Some(object) = value.as_object_mut() {
                        object.insert("generation".into(), json!(generation));
                        object.insert("persisted".into(), json!(true));
                        object.insert("applied".into(), json!(applied));
                    }
                }
                Err(error) => error.generation = Some(generation),
            }
        } else if let Ok(value) = &mut outcome
            && let Some(object) = value.as_object_mut()
        {
            object.insert(
                "generation".into(),
                json!(GENERATION.load(Ordering::SeqCst)),
            );
        }
        outcome
    }

    async fn execute_inner(
        &self,
        state: &Arc<AppState>,
        action: Action,
        caller: &Caller,
        approval: Option<&str>,
    ) -> Result<Value, Failure> {
        caller.check()?;
        match action {
            Action::Catalogue(Describe::Describe {
                area,
                store,
                id,
                cursor,
                limit,
            }) => {
                if limit == 0 || limit > 128 {
                    return Err(Error::new("settings", "out_of_range"));
                }
                let mut fields = self.catalogue.fields.clone();
                fields.extend(state.plugin_settings.catalogue()?.fields);
                let mut page: Vec<_> = fields
                    .values()
                    .filter(|field| {
                        store.as_ref().is_none_or(|store| &field.store == store)
                            && id.as_ref().is_none_or(|id| &field.id == id)
                            && area.as_ref().is_none_or(|area| &field.area == area)
                            && cursor.as_ref().is_none_or(|cursor| &field.id > cursor)
                    })
                    .take(limit + 1)
                    .collect();
                let more = page.len() > limit;
                page.truncate(limit);
                let next = more.then(|| page.last().unwrap().id.clone());
                Ok(
                    json!({"schemaVersion":1,"fields":page,"nextCursor":next,"inventoryComplete":self.catalogue.gaps.is_empty(),"actions":request_schema()}),
                )
            }
            Action::Transaction(request) => {
                self.catalogue
                    .require_complete_inventory()
                    .map_err(|_| Error::new("settings", "unavailable"))?;
                if let Request::Get {
                    target,
                    store,
                    paths,
                } = &request
                    && let Some(snapshot) = self.owner_read(state, target, store, paths).await?
                {
                    return serde_json::to_value(snapshot)
                        .map_err(|_| Error::new("settings", "store_failed"));
                }
                let reply = if request.store().starts_with("plugins.") {
                    state
                        .plugin_settings
                        .execute_as(request, caller, state.settings_policy.clone(), approval)
                        .await?
                } else {
                    self.registry.execute_as(request, caller, approval).await?
                };
                serde_json::to_value(reply).map_err(|_| Error::new("settings", "store_failed"))
            }
            Action::Frontend(Frontend::PublishPluginKeybindingActions { actions }) => {
                if actions.len() > 4096
                    || actions.iter().any(|action| {
                        action.len() > 256
                            || !action.starts_with("plugin:")
                            || action.split(':').count() != 3
                            || action
                                .split(':')
                                .any(|part| part.is_empty() || part.chars().any(char::is_control))
                    })
                {
                    return Err(Error::new("keybindings", "out_of_range"));
                }
                let mut actions = actions;
                actions.sort();
                actions.dedup();
                *state.plugin_keybinding_actions.write() = Some(actions);
                Ok(json!({"persisted":false,"applied":true}))
            }
            Action::Refresh(Refresh::Refresh { store }) => {
                if store != "ego" {
                    return Err(Error::new("settings", "unavailable"));
                }
                serde_json::to_value(
                    crate::ego_cli::providers(state, true)
                        .await
                        .map_err(|_| Error::new("ego", "unavailable"))?,
                )
                .map_err(|_| Error::new("ego", "store_failed"))
            }
            Action::Perform(Perform::Perform { target, owner }) => {
                self.catalogue
                    .require_complete_inventory()
                    .map_err(|_| Error::new("settings", "unavailable"))?;
                if target != Target::Machine {
                    return Err(Error::new("settings", "not_found"));
                }
                self.perform(state, owner, caller).await
            }
        }
    }
}

pub(crate) async fn execute(state: &Arc<AppState>, mut input: Value, caller: Caller) -> Value {
    let outcome = async {
        caller.check()?;
        let approval = input
            .as_object_mut()
            .and_then(|input| input.remove("approval"));
        let approval = match approval {
            Some(Value::String(value)) => Some(value),
            None => None,
            _ => return Err(Error::new("settings", "invalid_type")),
        };
        let confirm = input
            .as_object_mut()
            .and_then(|input| input.remove("confirm"));
        let confirm = match confirm {
            Some(Value::Bool(value)) => value,
            None => false,
            _ => return Err(Error::new("settings", "invalid_type")),
        };
        let action: Action =
            serde_json::from_value(input).map_err(|_| Error::new("settings", "invalid_type"))?;
        if confirm {
            let id = approval
                .as_deref()
                .ok_or_else(|| Error::new("settings", "invalid_type"))?;
            if !matches!(
                &action,
                Action::Transaction(
                    Request::Set { .. } | Request::Reset { .. } | Request::Patch { .. }
                )
            ) {
                return Err(Error::new("settings", "invalid_type"));
            }
            let result = crate::mcp_http::confirm_settings(state, &caller, id, None).await;
            if result["confirmed"] != true {
                return Err(Error::new("settings", "confirmation_denied"));
            }
        }
        let settings = state
            .settings_api
            .get_or_try_init(|| async { Settings::new(state.clone()) })
            .await?;
        settings
            .execute(state, action, &caller, approval.as_deref())
            .await
    }
    .await;
    match outcome {
        Ok(value) => value,
        Err(error) => json!({"error":error}),
    }
}

#[cfg(feature = "desktop")]
#[tauri::command]
pub(crate) async fn settings_action(
    state: tauri::State<'_, Arc<AppState>>,
    request: Value,
) -> Result<Value, Failure> {
    Ok(execute(state.inner(), request, Caller::owner_ui()).await)
}
