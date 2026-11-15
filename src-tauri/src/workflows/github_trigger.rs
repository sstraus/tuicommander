//! Neutral GitHub bindings. Serialized source data never grants replay authority.
use super::catalog::{FieldSchema, ObjectSchema, ValueType};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::collections::{BTreeMap, BTreeSet};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum GithubResource {
    Issue,
    PullRequest,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum GithubAction {
    Opened,
    Reopened,
    Updated,
    Labeled,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct GithubSnapshot {
    pub repository: String,
    pub number: i64,
    pub kind: GithubResource,
    pub open: bool,
    pub title: String,
    pub material_digest: String,
    pub labels: BTreeSet<String>,
    pub head: Option<String>,
    pub base: Option<String>,
    /// RFC3339 source update clock orders snapshots; it is not event identity.
    pub updated_at: String,
    /// Absent for polls: the author is not proof of who caused an update.
    pub actor: Option<String>,
}

impl GithubSnapshot {
    pub(crate) fn validate(&self) -> Result<(), String> {
        repository(&self.repository)?;
        chrono::DateTime::parse_from_rfc3339(&self.updated_at)
            .map_err(|_| "GitHub snapshot requires source update time")?;
        if self.number <= 0
            || self.title.len() > 65536
            || self.material_digest.is_empty()
            || self.labels.len() > 100
            || self.labels.iter().any(|v| v.len() > 256)
            || self
                .actor
                .as_ref()
                .is_some_and(|s| s.is_empty() || s.len() > 256)
        {
            return Err("invalid bounded GitHub snapshot".into());
        }
        if self.kind == GithubResource::PullRequest
            && (!self.head.as_deref().is_some_and(sha) || !self.base.as_deref().is_some_and(sha))
        {
            return Err("PR snapshot requires exact head/base SHAs".into());
        }
        Ok(())
    }
    pub(crate) fn resource(&self) -> Result<String, String> {
        Ok(format!(
            "github:{}/{}/{}",
            repository(&self.repository)?,
            match self.kind {
                GithubResource::Issue => "issue",
                GithubResource::PullRequest => "pull_request",
            },
            self.number
        ))
    }
    pub(crate) fn values(&self, action: GithubAction) -> BTreeMap<String, Value> {
        let mut values = BTreeMap::from([
            (
                "repository".into(),
                json!(self.repository.to_ascii_lowercase()),
            ),
            ("number".into(), json!(self.number)),
            ("action".into(), json!(action)),
            ("title".into(), json!(self.title)),
            ("material_digest".into(), json!(self.material_digest)),
            ("labels".into(), json!(self.labels)),
        ]);
        for (name, value) in [
            ("actor", &self.actor),
            ("head", &self.head),
            ("base", &self.base),
        ] {
            if let Some(value) = value {
                values.insert(name.into(), json!(value));
            }
        }
        values
    }
}
fn sha(value: &str) -> bool {
    value.len() == 40 && value.bytes().all(|c| c.is_ascii_hexdigit())
}
pub(crate) fn repository(value: &str) -> Result<String, String> {
    let parts: Vec<_> = value.split('/').collect();
    if parts.len() != 2
        || parts.iter().any(|v| {
            v.is_empty()
                || *v == "."
                || *v == ".."
                || !v
                    .bytes()
                    .all(|c| c.is_ascii_alphanumeric() || matches!(c, b'-' | b'_' | b'.'))
        })
    {
        return Err("GitHub repository must be owner/name".into());
    }
    Ok(value.to_ascii_lowercase())
}

pub(crate) fn snapshot_schema(kind: GithubResource) -> ObjectSchema {
    let fields = [
        "repository",
        "number",
        "action",
        "actor",
        "head",
        "base",
        "title",
        "material_digest",
        "labels",
        "label",
    ]
    .into_iter()
    .filter(|name| kind == GithubResource::PullRequest || !matches!(*name, "head" | "base"))
    .map(|name| {
        (
            name.into(),
            FieldSchema {
                value_type: match name {
                    "number" => ValueType::Integer,
                    "labels" => ValueType::Array,
                    _ => ValueType::String,
                },
                required: !matches!(name, "actor" | "label"),
                default: None,
            },
        )
    })
    .collect();
    ObjectSchema { fields }
}

/// A binding selects exactly one repository, never a sibling prefix.
pub(crate) fn resource_prefix(value: &str) -> Result<String, String> {
    let repo = value
        .strip_prefix("github:")
        .ok_or("GitHub resource prefix must start github:")?;
    let repo = repository(repo.strip_suffix('/').unwrap_or(repo))?;
    Ok(format!("github:{repo}/"))
}
