//! Pure normalization. The shared intake owner must authenticate and admit this
//! snapshot; this module creates neither a binding nor a scheduling loop.
use super::{GitLabInstall, sha};
use crate::workflows::catalog::content_digest;
use serde::{Deserialize, Serialize};
use serde_json::Value;

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct GitLabEvent {
    pub instance: String,
    pub project: u64,
    pub issue: Option<u64>,
    pub mr: Option<u64>,
    pub head: Option<String>,
    /// Webhooks do not promise a merge-base. Absent is not an empty SHA.
    pub base: Option<String>,
    pub pipeline: Option<u64>,
    pub kind: String,
    pub action: Option<String>,
    pub actor: Option<String>,
    pub generation: String,
    pub delivery: String,
    pub resource: String,
}

pub(crate) fn normalize(
    install: &GitLabInstall,
    delivery: &str,
    raw: &Value,
) -> Result<GitLabEvent, String> {
    install.validate()?;
    if delivery.trim().is_empty() || delivery.len() > 256 {
        return Err("GitLab intake requires original delivery identity".into());
    }
    if raw.pointer("/project/id").and_then(Value::as_u64) != Some(install.project) {
        return Err("GitLab event belongs to another installed project".into());
    }
    let kind = raw
        .get("object_kind")
        .and_then(Value::as_str)
        .ok_or("GitLab event kind missing")?;
    let attrs = raw
        .get("object_attributes")
        .ok_or("GitLab event attributes missing")?;
    let subject_project = match kind {
        "issue" => attrs.get("project_id"),
        "merge_request" => attrs.get("target_project_id"),
        _ => None,
    };
    if subject_project.is_some_and(|v| v.as_u64() != Some(install.project)) {
        return Err("GitLab event subject contradicts project identity".into());
    }
    let positive = |v: Option<&Value>| {
        v.and_then(Value::as_u64)
            .filter(|v| *v > 0)
            .ok_or("GitLab event subject missing")
    };
    let mut event = GitLabEvent {
        instance: install.base.clone(),
        project: install.project,
        issue: None,
        mr: None,
        head: None,
        base: None,
        pipeline: None,
        kind: kind.into(),
        action: attrs
            .get("action")
            .or_else(|| attrs.get("status"))
            .and_then(Value::as_str)
            .map(str::to_owned),
        generation: content_digest(&(&install.base, install.project, kind, attrs))?,
        delivery: delivery.into(),
        actor: raw
            .pointer("/user/username")
            .and_then(Value::as_str)
            .filter(|s| !s.is_empty() && s.len() <= 256)
            .map(str::to_owned),
        resource: String::new(),
    };
    let subject = match kind {
        "issue" => {
            let iid = positive(attrs.get("iid"))?;
            event.issue = Some(iid);
            format!("issue:{iid}")
        }
        "merge_request" => {
            let iid = positive(attrs.get("iid"))?;
            event.mr = Some(iid);
            event.head = Some(
                attrs
                    .pointer("/last_commit/id")
                    .and_then(Value::as_str)
                    .ok_or("GitLab MR event has no head")?
                    .into(),
            );
            format!("mr:{iid}")
        }
        "pipeline" => {
            let id = positive(attrs.get("id"))?;
            event.pipeline = Some(id);
            event.head = Some(
                attrs
                    .get("sha")
                    .and_then(Value::as_str)
                    .ok_or("GitLab pipeline event has no head")?
                    .into(),
            );
            if let Some(mr) = raw.get("merge_request").filter(|v| !v.is_null()) {
                if mr.get("target_project_id").and_then(Value::as_u64) != Some(install.project) {
                    return Err("GitLab pipeline MR belongs to another project".into());
                }
                event.mr = Some(positive(mr.get("iid"))?);
            }
            format!("pipeline:{id}")
        }
        _ => return Err("GitLab event kind unsupported by selected flow".into()),
    };
    if let Some(head) = &event.head {
        sha(head)?;
    }
    event.resource = format!("{}:{subject}", install.connector.id);
    Ok(event)
}

/// Installed schema, not a shape supplied by an incoming webhook or graph.
pub(crate) fn snapshot_schema(kind: &str) -> Result<super::ObjectSchema, String> {
    use super::{ValueType, field};
    use std::collections::BTreeMap;
    if !matches!(kind, "issue" | "merge_request" | "pipeline") {
        return Err("unknown GitLab trigger kind".into());
    }
    let mut fields = BTreeMap::from([
        ("instance".into(), field(ValueType::String)),
        ("project".into(), field(ValueType::Integer)),
        ("generation".into(), field(ValueType::String)),
        ("action".into(), field(ValueType::String)),
    ]);
    for (name, ty, required) in [
        ("issue", ValueType::Integer, kind == "issue"),
        ("mr", ValueType::Integer, kind == "merge_request"),
        ("pipeline", ValueType::Integer, kind == "pipeline"),
        ("head", ValueType::String, kind != "issue"),
        ("base", ValueType::String, false),
        ("actor", ValueType::String, false),
    ] {
        let mut value = field(ty);
        value.required = required;
        fields.insert(name.into(), value);
    }
    Ok(super::ObjectSchema { fields })
}
