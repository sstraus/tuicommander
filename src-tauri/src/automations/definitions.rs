//! Per-definition edits under ConfigFile's process and cross-process locks.

use super::model::{AutomationDefinition, AutomationsConfig};
use crate::config::ConfigFile;
use std::path::PathBuf;

pub struct DefinitionStore {
    file: ConfigFile<AutomationsConfig>,
}

impl DefinitionStore {
    /// Use the selected application instance's configuration directory.
    pub fn new() -> Self {
        Self {
            file: ConfigFile::new("automations.json"),
        }
    }

    /// Use an explicit document, including isolated test documents.
    pub fn at_path(path: PathBuf) -> Self {
        Self {
            file: ConfigFile::at_path(path),
        }
    }

    /// Read validated definitions without creating or rewriting the document.
    pub fn load(&self) -> Result<AutomationsConfig, String> {
        self.file.update_with_strict(|latest| {
            latest.validate()?;
            Ok((latest.clone(), false))
        })
    }

    /// Create one definition without replacing another writer's edits.
    pub fn create(&self, mut definition: AutomationDefinition) -> Result<(), String> {
        if definition.timezone.is_empty() {
            definition.timezone = super::schedule::timezone_for_creation(None)?;
        }
        definition.validate()?;
        self.file.update_with_strict(|latest| {
            latest.validate()?;
            if latest
                .definitions
                .iter()
                .any(|item| item.id == definition.id)
            {
                return Err("Automation id already exists".into());
            }
            definition.validate_schedule_at(chrono::Utc::now())?;
            latest.definitions.push(definition);
            Ok(((), true))
        })
    }

    /// Replace only the named definition; identity cannot change in an edit.
    pub fn update(&self, id: &str, mut definition: AutomationDefinition) -> Result<(), String> {
        definition.validate()?;
        if definition.id != id {
            return Err("Automation id cannot change".into());
        }
        self.file.update_with_strict(|latest| {
            latest.validate()?;
            let item = latest
                .definitions
                .iter_mut()
                .find(|item| item.id == id)
                .ok_or("Automation not found")?;
            definition
                .created_by_session
                .clone_from(&item.created_by_session);
            let changed = *item != definition;
            if item.cron != definition.cron
                || item.once_local != definition.once_local
                || item.timezone != definition.timezone
            {
                definition.validate_schedule_at(chrono::Utc::now())?;
            }
            *item = definition;
            Ok(((), changed))
        })
    }

    /// Remove one definition; run history is owned by a separate store.
    pub fn remove(&self, id: &str) -> Result<(), String> {
        self.file.update_with_strict(|latest| {
            latest.validate()?;
            let position = latest
                .definitions
                .iter()
                .position(|item| item.id == id)
                .ok_or("Automation not found")?;
            latest.definitions.remove(position);
            Ok(((), true))
        })
    }

    /// Find one definition using the same validated read as list.
    pub fn get(&self, id: &str) -> Result<AutomationDefinition, String> {
        self.load()?
            .definitions
            .into_iter()
            .find(|item| item.id == id)
            .ok_or_else(|| "Automation not found".into())
    }

    /// Pause or resume only the latest definition, retaining concurrent edits.
    pub fn set_enabled(&self, id: &str, enabled: bool) -> Result<(), String> {
        self.file.update_with_strict(|latest| {
            latest.validate()?;
            let item = latest
                .definitions
                .iter_mut()
                .find(|item| item.id == id)
                .ok_or("Automation not found")?;
            let changed = item.enabled != enabled;
            item.enabled = enabled;
            Ok(((), changed))
        })
    }

    /// Change the global limit while retaining the latest definitions.
    pub fn set_concurrency(&self, limit: u32) -> Result<(), String> {
        if limit == 0 {
            return Err("Automation concurrency must be greater than zero".into());
        }
        self.file.update_with_strict(|latest| {
            latest.validate()?;
            let changed = latest.max_concurrent_runs != limit;
            latest.max_concurrent_runs = limit;
            Ok(((), changed))
        })
    }
}
