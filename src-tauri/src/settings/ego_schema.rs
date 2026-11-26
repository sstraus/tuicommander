//! Typed projection of Ego's closed managed vocabulary, not its complete TOML schema.
use super::{
    catalogue::{Catalogue, schema},
    model::*,
};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::BTreeSet;

#[derive(Clone, Debug, Deserialize, Serialize, JsonSchema)]
#[serde(rename_all = "lowercase")]
pub(super) enum Network {
    On,
    Off,
}

#[derive(Clone, Debug, Deserialize, Serialize, JsonSchema)]
#[serde(rename_all = "kebab-case")]
pub(super) enum Access {
    Read,
    ReadWrite,
}

#[derive(Clone, Debug, Deserialize, Serialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub(super) struct Root {
    pub path: String,
    pub access: Access,
}

#[derive(Default, Deserialize, Serialize, JsonSchema)]
#[serde(default)]
pub(super) struct Managed {
    pub model: Option<String>,
    pub roots: Option<Vec<Root>>,
    pub network: Option<Network>,
    pub exec_paths: Option<Vec<String>>,
}

/// Whole managed keys are atomic leaves, including `roots`. The owner accepts one typed
/// assignment, not individual root-record edits; preserve absence versus explicit `[]`.
pub(super) fn register(catalogue: &mut Catalogue) -> Result<(), CatalogueError> {
    let schema = schema::<Managed>();
    let fields = schema["properties"]
        .as_object()
        .ok_or_else(|| CatalogueError {
            store: "ego".into(),
            path: "/".into(),
            reason: "invalid managed schema".into(),
        })?;
    for (key, schema) in fields {
        let path = format!("/{key}");
        let id = format!("ego.{key}");
        if catalogue.fields.contains_key(&id) {
            return Err(CatalogueError {
                store: "ego".into(),
                path,
                reason: "duplicate managed setting".into(),
            });
        }
        catalogue
            .fields
            .insert(id.clone(), descriptor(id, path, schema.clone(), false));
    }
    for name in ["headers", "credential_command"] {
        let mut field = descriptor(
            format!("ego.auth.{{provider}}.{name}"),
            format!("/auth/{{provider}}/{name}"),
            serde_json::json!({"type":["string","null"]}),
            true,
        );
        field.owner = "machine".into();
        field.annotation.operations = vec![Operation::Get];
        catalogue.fields.insert(field.id.clone(), field);
    }
    let mut credential = descriptor(
        "ego.credentials.{key}".into(),
        "/credentials/{key}".into(),
        serde_json::json!({"type":["string","null"]}),
        true,
    );
    credential.annotation.operations = vec![Operation::Get];
    catalogue.fields.insert(credential.id.clone(), credential);
    Ok(())
}

fn descriptor(id: String, path: String, schema: Value, secret: bool) -> Field {
    Field {
        label_key: format!("settings.catalogue.{id}.label"),
        help_key: format!("settings.catalogue.{id}.help"), label: path[1..].into(),
        id, store: "ego".into(), owner: "profile".into(), area: "ego".into(), group: "managed".into(),
        order: 0, path, schema, nullable: true,
        default: (!secret).then_some(Value::Null), availability: Availability::Available, choices: None,
        annotation: Annotation {
            sensitivity: if secret { Sensitivity::Secret } else { Sensitivity::Private },
            apply: ApplyMode::NextSession, effect: "Used by the next Ego session; existing ACP and PTY sessions keep their configuration.".into(),
            operations: vec![Operation::Get, Operation::Set, Operation::Reset], confirmation_class: "ordinary".into(),
        },
    }
}

pub(super) fn value(managed: &Managed) -> Result<Value, super::request::Failure> {
    serde_json::to_value(managed).map_err(|_| super::request::Error::new("ego", "store_failed"))
}

/// Presence only; vendor and stable account identity stay separate until projection.
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct Credential {
    #[serde(default, deserialize_with = "optional_identifier")]
    pub provider: Option<String>,
    #[serde(default, deserialize_with = "optional_identifier")]
    pub connection: Option<String>,
    pub present: bool,
}

// Optional means omitted, not null: a malformed named identity must not become legacy.
fn optional_identifier<'de, D: serde::Deserializer<'de>>(
    deserializer: D,
) -> Result<Option<String>, D::Error> {
    String::deserialize(deserializer).map(Some)
}

impl Credential {
    pub fn key(&self) -> String {
        match &self.connection {
            Some(connection) => format!("connection:{connection}"),
            None => self.provider.clone().expect("validated legacy identity"),
        }
    }
}

pub(super) fn credentials(raw: &str) -> Result<Vec<Credential>, super::request::Failure> {
    #[derive(Deserialize)]
    #[serde(deny_unknown_fields)]
    struct Inventory {
        v: u8,
        credentials: Vec<Credential>,
    }
    let failure = || super::request::Error::new("ego", "store_failed");
    let inventory: Inventory = serde_json::from_str(raw).map_err(|_| failure())?;
    if inventory.v != 1 {
        return Err(failure());
    }
    // Same grammar as the owner's connection ID/vendor validator; no display-label
    // length limit, truncation, or token/metadata projection.
    let valid = |s: &str| {
        !s.is_empty()
            && s.bytes()
                .all(|b| b.is_ascii_alphanumeric() || b"_-".contains(&b))
    };
    let mut seen = BTreeSet::new();
    for row in &inventory.credentials {
        if row.provider.as_deref().is_some_and(|p| !valid(p))
            || row.connection.as_deref().is_some_and(|c| !valid(c))
            || (row.provider.is_none() && row.connection.is_none())
            || !seen.insert(row.key())
        {
            return Err(failure());
        }
    }
    Ok(inventory.credentials)
}
