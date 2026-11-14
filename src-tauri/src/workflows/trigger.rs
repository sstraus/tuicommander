//! Source-neutral trigger bindings. Only backend normalizers submit events.
use super::catalog::ObjectSchema;
use super::neutral_publication::PublicationRef;
use super::run::neutral::NeutralLimits;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::{BTreeMap, BTreeSet};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "snake_case")]
pub(crate) enum TriggerSource {
    Github,
    Gitlab,
    Scheduled,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub(crate) struct TriggerEvent {
    pub source: TriggerSource,
    pub kind: String,
    pub resource: String,
    pub generation: String,
    pub event: String,
    pub delivery_id: Option<String>,
    pub schema: ObjectSchema,
    pub values: BTreeMap<String, Value>,
    pub actor: Option<String>,
}

impl TriggerEvent {
    pub(crate) fn validate(&self) -> Result<(), String> {
        if [&self.kind, &self.resource, &self.generation, &self.event]
            .iter()
            .any(|s| s.is_empty() || s.len() > 1024)
            || self.values.len() > 100
            || serde_json::to_vec(self).map_err(|e| e.to_string())?.len() > 262144
        {
            return Err("invalid bounded normalized trigger event".into());
        }
        if self.schema != trigger_schema(self.source, &self.kind)? {
            return Err("trigger event schema differs from installed source".into());
        }
        self.schema.check_schema("trigger source")?;
        self.schema.resolve(&self.values, "trigger event")?;
        Ok(())
    }
}

/// Constructed only after existing local-owner transport authentication.
/// It is deliberately not deserializable from request or graph JSON.
pub(crate) struct ReplayAuthority {
    project: String,
}
impl ReplayAuthority {
    pub(crate) fn authenticated_owner(project: &str) -> Self {
        Self {
            project: project.into(),
        }
    }
    pub(crate) fn require(&self, project: &str) -> Result<(), String> {
        if self.project != project {
            return Err("replay authority project mismatch".into());
        }
        Ok(())
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub(crate) struct TriggerMapping {
    pub inputs: BTreeMap<String, String>,
    pub subject: BTreeMap<String, String>,
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub(crate) struct TriggerBindingDraft {
    pub id: String,
    pub publication: PublicationRef,
    pub source: TriggerSource,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub schedule: Option<super::scheduled_trigger::TriggerSchedule>,
    pub kind: String,
    pub resource_prefix: String,
    pub events: BTreeSet<String>,
    /// Scalar equality or membership in an array-valued snapshot field.
    pub filters: BTreeMap<String, Value>,
    pub ignored_actors: BTreeSet<String>,
    pub mapping: TriggerMapping,
    pub limits: NeutralLimits,
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub(crate) struct NeutralTriggerBinding {
    pub draft: TriggerBindingDraft,
    pub revision: i64,
    pub enabled: bool,
    /// Requested enablement can be held by an existing installed-source prerequisite.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub refusal_reason: Option<String>,
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub(crate) struct TriggerPreview {
    pub inputs: BTreeMap<String, Value>,
    pub subject: Option<BTreeMap<String, Value>>,
    /// Exact enabled binding id -> revision, acknowledged on enable.
    pub overlaps: BTreeMap<String, i64>,
}

pub(crate) fn map_values(
    mapping: &BTreeMap<String, String>,
    values: &BTreeMap<String, Value>,
) -> BTreeMap<String, Value> {
    mapping
        .iter()
        .filter_map(|(target, source)| values.get(source).map(|v| (target.clone(), v.clone())))
        .collect()
}

pub(crate) fn matches(draft: &TriggerBindingDraft, event: &TriggerEvent) -> bool {
    draft.source == event.source
        && if draft.source == TriggerSource::Scheduled {
            event.resource == draft.resource_prefix
        } else {
            event.resource.starts_with(&draft.resource_prefix)
        }
        && draft.events.contains(&event.event)
        && draft.kind == event.kind
        && !event.actor.as_ref().is_some_and(|actor| {
            draft
                .ignored_actors
                .iter()
                .any(|a| a.eq_ignore_ascii_case(actor))
        })
        && draft.filters.iter().all(|(name, expected)| {
            event.values.get(name).is_some_and(|actual| {
                actual == expected
                    || actual
                        .as_array()
                        .is_some_and(|items| items.contains(expected))
            })
        })
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub(crate) struct TriggerAdmission {
    pub binding_id: String,
    pub resource: String,
    pub generation: String,
    pub run_id: Option<String>,
    pub disposition: String,
}

/// Closed installed-source match; source schemas never come from caller JSON.
pub(crate) fn trigger_schema(source: TriggerSource, kind: &str) -> Result<ObjectSchema, String> {
    match source {
        TriggerSource::Scheduled => super::scheduled_trigger::schema(kind),
        TriggerSource::Gitlab => super::gitlab::event::snapshot_schema(kind),
        TriggerSource::Github => {
            let kind = match kind {
                "issue" => super::github_trigger::GithubResource::Issue,
                "pull_request" => super::github_trigger::GithubResource::PullRequest,
                _ => return Err("unknown GitHub trigger kind".into()),
            };
            Ok(super::github_trigger::snapshot_schema(kind))
        }
    }
}
