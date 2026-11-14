use super::{GitLabInstall, Operation, number, text};
use serde_json::{Value, json};
use std::collections::BTreeMap;

/// Response identity is checked before creating a receipt. No API body is a grant.
pub(crate) fn project_response(
    install: &GitLabInstall,
    op: Operation,
    inputs: &BTreeMap<String, Value>,
    raw: &Value,
    effect: &str,
) -> Result<(&'static str, Value), String> {
    let mut receipt = json!({"instance":install.base,"project":install.project,"effect":effect});
    let outcome = match op {
        Operation::Issue | Operation::MergeRequest => {
            let key = if op == Operation::Issue {
                "issue"
            } else {
                "mr"
            };
            equal(raw, "project_id", &json!(install.project))?;
            equal(raw, "iid", &json!(number(inputs, key)?))?;
            receipt[key] = json!(number(inputs, key)?);
            receipt["data"] = raw.clone();
            if op == Operation::MergeRequest {
                for (name, pointer) in [
                    ("head", "/diff_refs/head_sha"),
                    ("base", "/diff_refs/base_sha"),
                ] {
                    if let Some(value) = raw.pointer(pointer).and_then(Value::as_str) {
                        super::sha(value)?;
                        receipt[name] = json!(value);
                    }
                }
            }
            "complete"
        }
        Operation::DiffVersion => {
            equal(raw, "id", &json!(number(inputs, "version")?))?;
            receipt["mr"] = json!(number(inputs, "mr")?);
            receipt["version"] = json!(number(inputs, "version")?);
            for (input, remote) in [
                ("head", "head_commit_sha"),
                ("base", "base_commit_sha"),
                ("start", "start_commit_sha"),
            ] {
                receipt[input] = json!(text(inputs, input)?);
                if raw.get(remote).and_then(Value::as_str) != Some(text(inputs, input)?) {
                    return Ok(("stale", receipt));
                }
            }
            let diffs = raw
                .get("diffs")
                .and_then(Value::as_array)
                .ok_or("GitLab diff response has no files")?;
            if raw.get("state").and_then(Value::as_str) != Some("collected")
                || diffs.iter().any(|d| {
                    d.get("too_large").and_then(Value::as_bool) == Some(true)
                        || d.get("collapsed").and_then(Value::as_bool) == Some(true)
                        || d.get("diff").and_then(Value::as_str).is_none()
                })
                || raw
                    .get("real_size")
                    .and_then(Value::as_str)
                    .and_then(|s| s.parse::<usize>().ok())
                    != Some(diffs.len())
            {
                "incomplete"
            } else {
                receipt["diffs"] = json!(diffs);
                "complete"
            }
        }
        Operation::Pipeline => {
            equal(raw, "id", &json!(number(inputs, "pipeline")?))?;
            equal(raw, "project_id", &json!(install.project))?;
            receipt["mr"] = json!(number(inputs, "mr")?);
            receipt["pipeline"] = json!(number(inputs, "pipeline")?);
            receipt["head"] = json!(text(inputs, "head")?);
            receipt["base"] = json!(text(inputs, "base")?);
            if raw.get("sha").and_then(Value::as_str) != Some(text(inputs, "head")?) {
                return Ok(("stale", receipt));
            }
            let status = raw
                .get("status")
                .and_then(Value::as_str)
                .ok_or("GitLab pipeline status missing")?;
            receipt["status"] = json!(status);
            match status {
                "success" => "complete",
                "failed" | "canceled" | "skipped" => "failed",
                "created"
                | "waiting_for_resource"
                | "preparing"
                | "pending"
                | "running"
                | "manual"
                | "scheduled" => "pending",
                _ => return Err("GitLab pipeline status unsupported".into()),
            }
        }
        Operation::ReviewNote => {
            let (_, _, body) = install.request(op, inputs, effect)?;
            let body: Value =
                serde_json::from_str(body.as_deref().ok_or("review request missing")?)
                    .map_err(|_| "invalid review body")?;
            equal(raw, "body", &body["body"])?;
            equal(raw, "project_id", &json!(install.project))?;
            equal(raw, "noteable_iid", &json!(number(inputs, "mr")?))?;
            equal(raw, "noteable_type", &json!("MergeRequest"))?;
            let note = raw
                .get("id")
                .and_then(Value::as_u64)
                .filter(|id| *id > 0)
                .ok_or("GitLab note id missing")?;
            receipt["mr"] = json!(number(inputs, "mr")?);
            receipt["head"] = json!(text(inputs, "head")?);
            receipt["base"] = json!(text(inputs, "base")?);
            receipt["note"] = json!(note);
            "complete"
        }
    };
    Ok((outcome, receipt))
}
fn equal(raw: &Value, field: &str, expected: &Value) -> Result<(), String> {
    if raw.get(field) == Some(expected) {
        Ok(())
    } else {
        Err(format!(
            "GitLab response {field} differs from requested subject"
        ))
    }
}
