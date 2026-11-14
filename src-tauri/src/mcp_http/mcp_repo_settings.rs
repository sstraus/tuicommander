//! Repo tool `get_settings` / `set_settings`: the same store and delta save as
//! `GET/PUT /config/repo-settings`, scoped to one repository.
use crate::{AppState, config};
use serde_json::{Map, Value, json};
use std::sync::Arc;

fn entry_json(settings: &config::RepoSettingsMap, path: &str) -> Value {
    let entry = settings
        .repos
        .get(path)
        .cloned()
        .unwrap_or_else(|| config::RepoSettingsEntry {
            path: path.to_owned(),
            ..Default::default()
        });
    serde_json::to_value(entry).unwrap_or_else(|_| json!({}))
}

/// Own value wins; a key the repository leaves unset (null or absent) falls
/// back to the global repo default of the same name.
fn effective(own: &Value) -> Value {
    let mut merged = match serde_json::to_value(config::load_repo_defaults()) {
        Ok(Value::Object(defaults)) => defaults,
        _ => Map::new(),
    };
    if let Value::Object(own) = own {
        for (key, value) in own {
            if !value.is_null() {
                merged.insert(key.clone(), value.clone());
            }
        }
    }
    Value::Object(merged)
}

pub(super) fn get(path: &str) -> Value {
    let own = entry_json(&config::load_repo_settings(), path);
    json!({"path": path, "settings": own, "effective": effective(&own)})
}

fn allowed_keys() -> Vec<String> {
    let schema = schemars::schema_for!(config::RepoSettingsEntry);
    schema
        .get("properties")
        .and_then(Value::as_object)
        .map(|props| props.keys().cloned().collect())
        .unwrap_or_default()
}

pub(super) fn set(state: &Arc<AppState>, args: &Value, path: &str) -> Value {
    let Some(patch) = args.get("settings").and_then(Value::as_object) else {
        return json!({"error": "Action 'set_settings' requires 'settings' (object of keys to change; null restores the inherited default)"});
    };
    let allowed = allowed_keys();
    for key in patch.keys() {
        if key == "path" || !allowed.contains(key) {
            return json!({"error": format!("Unknown or read-only repository setting '{key}'")});
        }
    }
    let base = config::load_repo_settings();
    let mut own = match entry_json(&base, path) {
        Value::Object(own) => own,
        _ => Map::new(),
    };
    for (key, value) in patch {
        own.insert(key.clone(), value.clone());
    }
    let entry: config::RepoSettingsEntry = match serde_json::from_value(Value::Object(own)) {
        Ok(entry) => entry,
        Err(error) => return json!({"error": format!("Invalid repository settings: {error}")}),
    };
    let mut updated = base.clone();
    updated.repos.insert(path.to_owned(), entry);
    match config::save_repo_settings(base, updated) {
        Ok(()) => {
            crate::settings::api::publish_change(state, "repositories", true);
            let mut result = get(path);
            result["ok"] = json!(true);
            result
        }
        Err(error) => json!({"error": error}),
    }
}
