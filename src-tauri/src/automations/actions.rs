//! Shared definition actions for MCP and the HTTP/IPC adapters.

use super::{definitions::DefinitionStore, model::AutomationDefinition};
use serde::Deserialize;
use serde_json::{Value, json};

/// Definition operations, independent of their transport.
#[derive(Debug, Deserialize)]
#[serde(tag = "action", rename_all = "snake_case", deny_unknown_fields)]
pub enum DefinitionAction {
    List {},
    Get {
        id: String,
    },
    Create {
        definition: AutomationDefinition,
    },
    Update {
        id: String,
        definition: AutomationDefinition,
    },
    Pause {
        id: String,
    },
    Resume {
        id: String,
    },
    Delete {
        id: String,
    },
}

/// Apply one action with host-resolved creator identity, never client-supplied provenance.
pub fn execute(
    store: &DefinitionStore,
    action: DefinitionAction,
    creator_session: Option<&str>,
) -> Result<Value, String> {
    match action {
        DefinitionAction::List {} => {
            serde_json::to_value(store.load()?.definitions).map_err(|error| error.to_string())
        }
        DefinitionAction::Get { id } => {
            serde_json::to_value(store.get(&id)?).map_err(|error| error.to_string())
        }
        DefinitionAction::Create { mut definition } => {
            definition.created_by_session = creator_session.map(str::to_owned);
            store.create(definition)?;
            Ok(json!({"ok": true}))
        }
        DefinitionAction::Update { id, definition } => {
            store.update(&id, definition)?;
            Ok(json!({"ok": true}))
        }
        DefinitionAction::Pause { id } => {
            store.set_enabled(&id, false)?;
            Ok(json!({"ok": true}))
        }
        DefinitionAction::Resume { id } => {
            store.set_enabled(&id, true)?;
            Ok(json!({"ok": true}))
        }
        DefinitionAction::Delete { id } => {
            store.remove(&id)?;
            Ok(json!({"ok": true}))
        }
    }
}
