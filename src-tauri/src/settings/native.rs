//! Native typed-store definitions. Registration performs no disk, CLI or vault I/O.
use super::{catalogue::Catalogue, model::*};
use crate::config::*;

/// Build descriptors even on hosts without the optional native audio backend.
pub fn native_catalogue() -> Result<Catalogue, CatalogueError> {
    native_catalogue_for(cfg!(feature = "dictation"))
}

/// The committed TypeScript is one file for every build, so it is generated with `dictation` on.
pub(super) fn native_catalogue_for(dictation: bool) -> Result<Catalogue, CatalogueError> {
    let mut catalogue = Catalogue::default();
    let available = Availability::Available;
    catalogue.register(
        "app",
        "machine",
        Some(&AppConfig::default()),
        available.clone(),
    )?;
    catalogue.register(
        "notifications",
        "machine",
        Some(&NotificationConfig::default()),
        available.clone(),
    )?;
    catalogue.register(
        "preferences",
        "instance",
        Some(&UIPrefsConfig::default()),
        available.clone(),
    )?;
    catalogue.register(
        "repo_defaults",
        "instance",
        Some(&RepoDefaultsConfig::default()),
        available.clone(),
    )?;
    catalogue.register(
        "repo_local",
        "repository",
        Some(&RepoLocalConfig::default()),
        available.clone(),
    )?;
    catalogue.register(
        "repositories",
        "instance",
        Some(&RepoSettingsMap::default()),
        available.clone(),
    )?;
    catalogue.register(
        "repositories_registration",
        "instance",
        Some(&super::registration::Registration::default()),
        available.clone(),
    )?;
    catalogue.register(
        "prompts",
        "instance",
        Some(&PromptLibraryConfig::default()),
        available.clone(),
    )?;
    catalogue.register(
        "agents",
        "machine",
        Some(&AgentsConfig::default()),
        available.clone(),
    )?;
    catalogue.register_schema(
        "upstreams",
        "machine",
        &super::upstream_credentials::catalogue_schema(),
        None,
        available.clone(),
    )?;
    catalogue.register_schema(
        "circleci",
        "machine",
        &super::circleci::catalogue_schema(),
        None,
        available.clone(),
    )?;
    catalogue.register_schema(
        "github_accounts",
        "machine",
        &super::accounts::GithubSettings::catalogue_schema(),
        None,
        available.clone(),
    )?;
    catalogue.register(
        "github_bindings",
        "instance",
        Some(&crate::github_account::RepoBindingStore::default()),
        available.clone(),
    )?;
    catalogue.register::<super::remote::RemoteConnections>(
        "connections",
        "machine",
        None,
        available.clone(),
    )?;
    let mut credential = Catalogue::default();
    credential.register_schema(
        "connections",
        "machine",
        &super::remote::credential_schema(),
        None,
        available.clone(),
    )?;
    for (_, mut field) in credential.fields {
        field.id = "connections.{id}.credential".into();
        field.path = "/{id}/credential".into();
        catalogue.fields.insert(field.id.clone(), field);
    }
    catalogue.register::<super::remote::TunnelProfiles>(
        "tunnels",
        "machine",
        None,
        available.clone(),
    )?;
    catalogue.register_schema(
        "telegram",
        "machine",
        &super::telegram::owner_schema(),
        None,
        available,
    )?;
    if let Some(mut field) = catalogue.fields.remove("telegram.allowed_chat_ids.{key}") {
        field.id = "telegram.allowed_chat_ids.{chatId}".into();
        field.path = "/allowed_chat_ids/{chatId}".into();
        catalogue.fields.insert(field.id.clone(), field);
    }
    let voice = if dictation {
        Availability::Available
    } else {
        Availability::Unavailable {
            reason: "This build has no native dictation backend".into(),
        }
    };
    catalogue.register(
        "voice",
        "machine",
        Some(&DictationConfig::fresh_defaults()),
        voice.clone(),
    )?;
    catalogue.register_schema(
        "corrections",
        "machine",
        &super::corrections::catalogue_schema(),
        None,
        voice,
    )?;
    super::ego_schema::register(&mut catalogue)?;
    super::keybindings::register(&mut catalogue)?;
    super::layout::register(&mut catalogue);
    super::app_choices::register(&mut catalogue);
    super::registration::classify_runtime(&mut catalogue);
    catalogue.register_gap_ledger(include_str!("inventory-gaps.json"))?;
    Ok(catalogue)
}
