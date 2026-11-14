//! Installed GitLab REST v4 operations over the existing account HTTP client.
#![cfg_attr(
    not(test),
    expect(dead_code, reason = "012 awaits native host composition")
)]
pub(crate) mod event;
mod intake;
mod response;
mod schema;
#[cfg(test)]
mod tests;

use super::catalog::{
    ActivityDescriptor, DescriptorId, FieldSchema, HandlerRef, ObjectSchema, ValueType,
};
use super::connector::{ConnectorContract, RateLimit, Reconciliation};
use super::data_reference::DataReference;
pub(crate) use response::project_response;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::collections::{BTreeMap, BTreeSet};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum Operation {
    Issue,
    MergeRequest,
    DiffVersion,
    Pipeline,
    ReviewNote,
}
impl Operation {
    pub fn name(self) -> &'static str {
        match self {
            Self::Issue => "issue",
            Self::MergeRequest => "merge_request",
            Self::DiffVersion => "diff_version",
            Self::Pipeline => "pipeline",
            Self::ReviewNote => "review_note",
        }
    }
    pub fn mutation(self) -> bool {
        self == Self::ReviewNote
    }
    pub fn inputs(self) -> ObjectSchema {
        use ValueType::{Integer, String};
        let mut fields = BTreeMap::new();
        for (name, ty) in match self {
            Self::Issue => vec![("issue", Integer)],
            Self::MergeRequest => vec![("mr", Integer)],
            Self::DiffVersion => vec![
                ("mr", Integer),
                ("version", Integer),
                ("head", String),
                ("base", String),
                ("start", String),
            ],
            Self::Pipeline => vec![
                ("mr", Integer),
                ("pipeline", Integer),
                ("head", String),
                ("base", String),
            ],
            Self::ReviewNote => vec![
                ("mr", Integer),
                ("head", String),
                ("base", String),
                ("body", String),
            ],
        } {
            fields.insert(name.into(), field(ty));
        }
        ObjectSchema { fields }
    }
}
pub(crate) fn field(value_type: ValueType) -> FieldSchema {
    FieldSchema {
        value_type,
        required: true,
        default: None,
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct GitLabInstall {
    /// Full REST root, including any self-managed relative root, ending /api/v4/.
    pub base: String,
    pub project: u64,
    pub connector: ConnectorContract,
}
impl GitLabInstall {
    pub fn new(base: String, project: u64, account: String) -> Result<Self, String> {
        let install = Self {
            base,
            project,
            connector: ConnectorContract {
                id: format!("gitlab:{account}:{project}"),
                authentication: Some(DataReference::Credential { id: account }),
                resources: ObjectSchema {
                    fields: BTreeMap::from([("project".into(), field(ValueType::Integer))]),
                },
                pagination: None,
                rate_limit: RateLimit::FailWithRetryAfter,
                callback: None,
                reconciliation: Reconciliation::Unavailable,
            },
        };
        install.validate()?;
        Ok(install)
    }
    pub fn validate(&self) -> Result<(), String> {
        self.connector.validate()?;
        let url = url::Url::parse(&self.base).map_err(|_| "invalid GitLab REST root")?;
        if self.project == 0
            || !matches!(url.scheme(), "http" | "https")
            || !url.path().ends_with("/api/v4/")
            || !url.username().is_empty()
            || url.password().is_some()
            || url.query().is_some()
            || url.fragment().is_some()
        {
            return Err(
                "GitLab requires an installed project and credential-free REST v4 root".into(),
            );
        }
        if !matches!(
            self.connector.authentication,
            Some(DataReference::Credential { .. })
        ) || self.connector.pagination.is_some()
            || self.connector.callback.is_some()
            || self.connector.reconciliation != Reconciliation::Unavailable
        {
            return Err(
                "GitLab requires an account reference and no invented service reconciliation"
                    .into(),
            );
        }
        Ok(())
    }
    pub fn descriptor(&self, op: Operation) -> ActivityDescriptor {
        use sha2::Digest;
        let binding = hex::encode(sha2::Sha256::digest(self.connector.id.as_bytes()));
        ActivityDescriptor {
            id: DescriptorId {
                namespace: "gitlab".into(),
                name: format!("{}.{binding}", op.name()),
                version: 1,
            },
            purpose: format!("GitLab REST v4 {} for an installed project", op.name()),
            handler: HandlerRef {
                name: format!("installed.gitlab.{}.{binding}", op.name()),
                version: 1,
            },
            target: None,
            config: ObjectSchema {
                fields: BTreeMap::from([(
                    "installed".into(),
                    FieldSchema {
                        value_type: ValueType::Object,
                        required: true,
                        default: Some(json!(self)),
                    },
                )]),
            },
            inputs: op.inputs(),
            subject: None,
            outputs: op.outputs(),
            outcomes: ["complete", "stale", "incomplete", "pending", "failed"]
                .into_iter()
                .map(str::to_owned)
                .collect(),
            required_capabilities: BTreeSet::from([if op.mutation() {
                "gitlab:write"
            } else {
                "gitlab:read"
            }
            .into()]),
            execution_policies: BTreeSet::from(["local".into()]),
        }
    }
    pub fn request(
        &self,
        op: Operation,
        inputs: &BTreeMap<String, Value>,
        effect: &str,
    ) -> Result<(String, &'static str, Option<String>), String> {
        self.validate()?;
        op.inputs().resolve(inputs, "GitLab inputs")?;
        for name in ["issue", "mr", "version", "pipeline"] {
            if inputs.contains_key(name) {
                number(inputs, name)?;
            }
        }
        for name in ["head", "base", "start"] {
            if inputs.contains_key(name) {
                sha(text(inputs, name)?)?;
            }
        }
        let root = format!("{}projects/{}", self.base, self.project);
        let path = match op {
            Operation::Issue => format!("{root}/issues/{}", number(inputs, "issue")?),
            Operation::MergeRequest | Operation::ReviewNote => {
                format!("{root}/merge_requests/{}", number(inputs, "mr")?)
            }
            Operation::DiffVersion => format!(
                "{root}/merge_requests/{}/versions/{}",
                number(inputs, "mr")?,
                number(inputs, "version")?
            ),
            Operation::Pipeline => format!("{root}/pipelines/{}", number(inputs, "pipeline")?),
        };
        if op != Operation::ReviewNote {
            return Ok((path, "GET", None));
        }
        let body = text(inputs, "body")?;
        if body.trim().is_empty() || body.len() > 60_000 {
            return Err("GitLab review body is empty or too large".into());
        }
        // Notes can execute service quick actions. A note grant must not become
        // a merge/approval command merely because review text contains one.
        if body.lines().any(|line| {
            line.trim_start().strip_prefix('/').is_some_and(|rest| {
                let command = rest.split_whitespace().next().unwrap_or("");
                !command.is_empty()
                    && command
                        .bytes()
                        .all(|b| b.is_ascii_alphabetic() || b == b'_')
            })
        }) {
            return Err("GitLab review notes cannot contain service quick actions".into());
        }
        let body = format!(
            "{body}\n\nReviewed GitLab project {} MR !{}\nHead: {}\nBase: {}\n<!-- tuic-effect:{effect} -->",
            self.project,
            number(inputs, "mr")?,
            text(inputs, "head")?,
            text(inputs, "base")?
        );
        Ok((
            format!("{path}/notes"),
            "POST",
            Some(json!({"body":body}).to_string()),
        ))
    }
}
pub(crate) fn number(inputs: &BTreeMap<String, Value>, name: &str) -> Result<u64, String> {
    inputs
        .get(name)
        .and_then(Value::as_u64)
        .filter(|v| *v > 0)
        .ok_or_else(|| format!("GitLab {name} requires a positive integer"))
}
pub(super) fn text<'a>(inputs: &'a BTreeMap<String, Value>, name: &str) -> Result<&'a str, String> {
    inputs
        .get(name)
        .and_then(Value::as_str)
        .ok_or_else(|| format!("GitLab {name} requires text"))
}
pub(super) fn sha(value: &str) -> Result<(), String> {
    if matches!(value.len(), 40 | 64)
        && value
            .bytes()
            .all(|b| b.is_ascii_hexdigit() && !b.is_ascii_uppercase())
    {
        Ok(())
    } else {
        Err("GitLab requires a full lowercase commit SHA".into())
    }
}
