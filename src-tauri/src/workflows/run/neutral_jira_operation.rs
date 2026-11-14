use super::neutral_jira_client::{JiraClient, validate_fields};
use crate::workflows::jira::*;
use serde_json::{Value, json};

pub(super) async fn execute(
    client: &JiraClient,
    request: &JiraRequest,
    digest: &str,
) -> Result<JiraResult, String> {
    match request {
        JiraRequest::Lookup { issue } => Ok(JiraResult::Issue {
            issue: client.issue(issue).await?,
        }),
        JiraRequest::Search {
            jql,
            next_page_token,
            limit,
        } => client.search(jql, next_page_token.as_deref(), *limit).await,
        JiraRequest::Transitions { issue } => {
            client.issue(issue).await?;
            let value = client
                .request(
                    "GET",
                    &["issue", &issue.id, "transitions"],
                    &[("expand", "transitions.fields".into())],
                    None,
                )
                .await?;
            if !value["transitions"].is_array() {
                return Err("Jira transitions missing".into());
            }
            Ok(JiraResult::Transitions {
                issue: issue.clone(),
                transitions: value["transitions"].clone(),
            })
        }
        JiraRequest::Create {
            issue_type_id,
            fields,
        } => {
            let metadata = client.metadata(issue_type_id).await?;
            let mut fields = fields.payload()?;
            fields["project"] = json!({"id":client.binding.project_id});
            fields["issuetype"] = json!({"id":issue_type_id});
            let mut labels = fields
                .get("labels")
                .and_then(Value::as_array)
                .cloned()
                .unwrap_or_default();
            labels.push(json!(marker(digest)));
            fields["labels"] = json!(labels);
            validate_fields(&fields, &metadata, true)?;
            let created = client
                .request("POST", &["issue"], &[], Some(json!({"fields":fields})))
                .await?;
            let created = JiraIssue::parse(&client.binding.account, &created)?;
            Ok(JiraResult::Issue {
                issue: client.issue(&created.reference).await?,
            })
        }
        JiraRequest::Update { issue, fields } => {
            client.issue(issue).await?;
            let fields = fields.payload()?;
            let metadata = client
                .request("GET", &["issue", &issue.id, "editmeta"], &[], None)
                .await?;
            validate_fields(&fields, &metadata["fields"], false)?;
            client
                .request(
                    "PUT",
                    &["issue", &issue.id],
                    &[],
                    Some(json!({"fields":fields})),
                )
                .await?;
            observed_fields(client, issue, &fields, None).await
        }
        JiraRequest::Comment { issue, body } => {
            client.issue(issue).await?;
            let value = client.request("POST", &["issue",&issue.id,"comment"], &[], Some(json!({"body":adf(body),"properties":[{"key":"tuic.workflow.effect","value":digest}]}))).await?;
            Ok(JiraResult::Comment {
                issue: issue.clone(),
                comment_id: required_string(&value, "id")?,
            })
        }
        JiraRequest::Transition {
            issue,
            transition_id,
            target_status_id,
            fields,
        } => {
            client.issue(issue).await?;
            let value = client
                .request(
                    "GET",
                    &["issue", &issue.id, "transitions"],
                    &[("expand", "transitions.fields".into())],
                    None,
                )
                .await?;
            let transition = value["transitions"]
                .as_array()
                .ok_or("Jira transitions missing")?
                .iter()
                .find(|v| v["id"].as_str() == Some(transition_id))
                .ok_or("Jira transition no longer available")?;
            if transition.pointer("/to/id").and_then(Value::as_str) != Some(target_status_id) {
                return Err("Jira transition target changed".into());
            }
            let fields = fields.payload()?;
            validate_fields(&fields, &transition["fields"], true)?;
            client
                .request(
                    "POST",
                    &["issue", &issue.id, "transitions"],
                    &[],
                    Some(json!({"transition":{"id":transition_id},"fields":fields})),
                )
                .await?;
            observed_fields(client, issue, &fields, Some(target_status_id)).await
        }
    }
}

pub(super) fn marker(digest: &str) -> String {
    format!("tuic-effect-{digest}")
}

pub(super) async fn observed_fields(
    client: &JiraClient,
    issue: &JiraIssueRef,
    fields: &Value,
    status: Option<&str>,
) -> Result<JiraResult, String> {
    let actual = client.issue(issue).await?;
    for (key, expected) in fields.as_object().ok_or("invalid Jira fields")? {
        let received = actual
            .fields
            .get(key)
            .ok_or("Jira field absent in read-back")?;
        let matches = if key == "labels" {
            expected.as_array().is_some_and(|wanted| {
                received.as_array().is_some_and(|got| {
                    wanted.len() == got.len() && wanted.iter().all(|v| got.contains(v))
                })
            })
        } else {
            field_matches(expected, received)
        };
        if !matches {
            return Err("Jira desired fields not yet observed; no write repeated".into());
        }
    }
    if status
        .is_some_and(|id| actual.fields.pointer("/status/id").and_then(Value::as_str) != Some(id))
    {
        return Err("Jira desired status not yet observed; no transition repeated".into());
    }
    Ok(JiraResult::Issue { issue: actual })
}

// Jira expands option/reference objects and serializes numeric custom fields as doubles.
fn field_matches(expected: &Value, actual: &Value) -> bool {
    match (expected, actual) {
        (Value::Number(a), Value::Number(b)) => number_matches(a, b),
        (Value::Object(wanted), Value::Object(got)) => wanted.iter().all(|(key, value)| {
            got.get(key)
                .is_some_and(|actual| field_matches(value, actual))
        }),
        (Value::Array(wanted), Value::Array(got)) if wanted.len() == got.len() => {
            if wanted
                .iter()
                .all(|v| v.is_object() && (v.get("id").is_some() || v.get("value").is_some()))
            {
                let mut matched = vec![false; got.len()];
                wanted.iter().all(|value| {
                    let found = got
                        .iter()
                        .enumerate()
                        .position(|(i, actual)| !matched[i] && field_matches(value, actual));
                    if let Some(i) = found {
                        matched[i] = true;
                        true
                    } else {
                        false
                    }
                })
            } else {
                wanted
                    .iter()
                    .zip(got)
                    .all(|(wanted, got)| field_matches(wanted, got))
            }
        }
        _ => expected == actual,
    }
}
fn number_matches(a: &serde_json::Number, b: &serde_json::Number) -> bool {
    if a == b {
        return true;
    }
    if a.is_f64() && !b.is_f64() {
        return number_matches(b, a);
    }
    if !b.is_f64() {
        return false;
    }
    let Some(float) = b.as_f64() else {
        return false;
    };
    if float.fract() != 0.0 {
        return false;
    }
    if let Some(integer) = a.as_i64() {
        return float >= i64::MIN as f64 && float < i64::MAX as f64 && float as i64 == integer;
    }
    if let Some(integer) = a.as_u64() {
        return float >= 0.0 && float < u64::MAX as f64 && float as u64 == integer;
    }
    false
}
