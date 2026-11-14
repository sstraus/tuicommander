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
        Self::migrated(ConfigFile::new("automations.json"))
    }

    /// Use an explicit document, including isolated test documents.
    pub fn at_path(path: PathBuf) -> Self {
        Self::migrated(ConfigFile::at_path(path))
    }

    fn migrated(file: ConfigFile<AutomationsConfig>) -> Self {
        if let Err(error) = file.rewrite_raw_with_backup(
            crate::sqlite_once::LEGACY_BACKUP_SUFFIX,
            strip_legacy_sections,
        ) {
            tracing::warn!(source = "automations", %error, "Legacy automation sections not removed");
        }
        Self { file }
    }

    /// Read validated definitions without creating or rewriting the document.
    pub fn load(&self) -> Result<AutomationsConfig, String> {
        self.file.update_preserving_invalid(|latest| {
            latest.validate()?;
            Ok((latest.clone(), false))
        })
    }

    /// Create one definition without replacing another writer's edits.
    pub fn create(&self, definition: AutomationDefinition) -> Result<(), String> {
        self.create_saved(definition).map(|_| ())
    }

    pub(crate) fn create_saved(
        &self,
        definition: AutomationDefinition,
    ) -> Result<AutomationDefinition, String> {
        self.insert(definition, false)
    }

    /// Atomically reuse the first saved occurrence for a host-generated request id.
    pub(crate) fn create_idempotent(
        &self,
        definition: AutomationDefinition,
    ) -> Result<AutomationDefinition, String> {
        self.insert(definition, true)
    }

    fn insert(
        &self,
        mut definition: AutomationDefinition,
        idempotent: bool,
    ) -> Result<AutomationDefinition, String> {
        if definition.timezone.is_empty() {
            definition.timezone = super::schedule::timezone_for_creation(None)?;
        }
        definition.validate()?;
        self.file.update_preserving_invalid(|latest| {
            latest.validate()?;
            if let Some(saved) = latest
                .definitions
                .iter()
                .find(|item| item.id == definition.id)
            {
                if idempotent && saved.created_by_session == definition.created_by_session {
                    return Ok((saved.clone(), false));
                }
                return Err("Automation id already exists".into());
            }
            definition.validate_schedule_at(chrono::Utc::now())?;
            latest.definitions.push(definition.clone());
            Ok((definition, true))
        })
    }

    /// Replace only the named definition; identity cannot change in an edit.
    pub fn update(&self, id: &str, mut definition: AutomationDefinition) -> Result<(), String> {
        definition.validate()?;
        if definition.id != id {
            return Err("Automation id cannot change".into());
        }
        self.file.update_preserving_invalid(|latest| {
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
                || item.once_at != definition.once_at
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
        self.file.update_preserving_invalid(|latest| {
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
        self.file.update_preserving_invalid(|latest| {
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
        self.file.update_preserving_invalid(|latest| {
            latest.validate()?;
            let changed = latest.max_concurrent_runs != limit;
            latest.max_concurrent_runs = limit;
            Ok(((), changed))
        })
    }
}

/// Remove the sections of the deleted legacy engines: the PR-review and issue
/// intake subscriptions, and the definitions only they could start (the
/// review-only definition, the triage definitions, any ego triage executor).
/// Prompt schedules stay as they are. Returns whether the document changed.
fn strip_legacy_sections(document: &mut serde_json::Value) -> bool {
    let Some(root) = document.as_object_mut() else {
        return false;
    };
    let mut legacy_ids = std::collections::HashSet::new();
    let mut changed = false;
    if let Some(reviews) = root.remove("pull_request_reviews") {
        changed = true;
        if reviews["repositories"]
            .as_array()
            .is_some_and(|r| !r.is_empty())
            && let Some(id) = reviews["definition_id"].as_str()
        {
            legacy_ids.insert(id.to_owned());
        }
    }
    if let Some(intake) = root.remove("event_intake") {
        changed = true;
        for policy in intake["triage"].as_array().into_iter().flatten() {
            if let Some(id) = policy["definition_id"].as_str() {
                legacy_ids.insert(id.to_owned());
            }
        }
    }
    if let Some(definitions) = root
        .get_mut("definitions")
        .and_then(serde_json::Value::as_array_mut)
    {
        let before = definitions.len();
        definitions.retain(|definition| {
            let id = definition["id"].as_str().unwrap_or_default();
            let ego = definition["triage_executor"]["kind"] == "ego";
            !ego && !legacy_ids.contains(id)
        });
        changed |= definitions.len() != before;
        for definition in definitions.iter_mut() {
            if let Some(fields) = definition.as_object_mut() {
                changed |= fields.remove("triage_executor").is_some();
            }
        }
    }
    changed
}

#[cfg(test)]
mod legacy_section_tests {
    use super::*;

    fn schedule(id: &str) -> serde_json::Value {
        serde_json::json!({
            "id": id, "name": id, "prompt": "Check", "run_config": "codex",
            "repository": "/repo", "workspace": {"mode": "existing"},
            "cron": "0 9 * * *", "timezone": "UTC", "enabled": true,
            "grace_secs": 60, "overlap": "skip", "max_duration_secs": 600, "precheck": null
        })
    }

    // Catches: the one-time removal dropping a prompt schedule, keeping a definition
    // only a deleted engine could start (it would fire on its cron), or leaving a key
    // the strict schema refuses, so every automation read fails after the upgrade.
    #[test]
    fn legacy_sections_are_backed_up_then_removed_and_prompt_schedules_stay() {
        let dir = tempfile::tempdir_in(crate::test_support::test_temp_root()).unwrap();
        let path = dir.path().join("automations.json");
        let mut review = schedule("review");
        review["triage_executor"] = serde_json::json!({"kind": "managed"});
        let mut ego = schedule("ego");
        ego["triage_executor"] = serde_json::json!({"kind": "ego"});
        let original = serde_json::json!({
            "version": 1, "max_concurrent_runs": 2,
            "definitions": [schedule("daily"), review, schedule("triage"), ego, schedule("intake")],
            "pull_request_reviews": {"repositories": ["o/r"], "definition_id": "review"},
            "event_intake": {"repositories": ["o/r"], "label": "run", "definition_id": "intake",
                "triage": [{"repository": "o/r", "definition_id": "triage", "policy_version": "1",
                    "read_only_attested": true, "rejection_label": "no"}]}
        });
        std::fs::write(&path, serde_json::to_vec(&original).unwrap()).unwrap();

        let config = DefinitionStore::at_path(path.clone()).load().unwrap();
        let ids: Vec<_> = config.definitions.iter().map(|d| d.id.as_str()).collect();
        assert_eq!(ids, ["daily", "intake"]);
        assert_eq!(
            config.definitions[0],
            serde_json::from_value(schedule("daily")).unwrap()
        );
        let backup = dir.path().join(format!(
            "automations.json.{}",
            crate::sqlite_once::LEGACY_BACKUP_SUFFIX
        ));
        let saved: serde_json::Value =
            serde_json::from_slice(&std::fs::read(&backup).unwrap()).unwrap();
        assert_eq!(saved, original);

        // A later load leaves a clean document and the first backup alone.
        std::fs::write(&backup, b"first").unwrap();
        DefinitionStore::at_path(path).load().unwrap();
        assert_eq!(std::fs::read(&backup).unwrap(), b"first");
    }
}
