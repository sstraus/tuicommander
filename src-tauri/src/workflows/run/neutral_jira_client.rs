//! Jira uses the installed plugin HTTP/account owner, including credential redaction.
use crate::workflows::jira::*;
use serde_json::{Value, json};
use std::sync::Arc;

pub(super) struct JiraClient {
    pub state: Arc<crate::AppState>,
    pub binding: JiraBinding,
}
impl JiraClient {
    pub async fn request(
        &self,
        method: &str,
        path: &[&str],
        query: &[(&str, String)],
        body: Option<Value>,
    ) -> Result<Value, String> {
        let mut url = url::Url::parse(&self.binding.base).map_err(|_| "invalid Jira site")?;
        {
            let mut segments = url.path_segments_mut().map_err(|_| "invalid Jira path")?;
            segments.pop_if_empty().extend(["rest", "api", "3"]);
            for segment in path {
                segments.push(segment);
            }
        }
        if !query.is_empty() {
            url.query_pairs_mut()
                .extend_pairs(query.iter().map(|(k, v)| (*k, v.as_str())));
        }
        let reply = crate::plugin_http::plugin_http_fetch_impl(
            &self.state,
            url.into(),
            Some(method.into()),
            Some(std::collections::HashMap::from([
                ("Accept".into(), "application/json".into()),
                ("Content-Type".into(), "application/json".into()),
            ])),
            body.map(|body| body.to_string()),
            super::neutral_http::credential(&self.binding.contract),
            self.binding.plugin.clone(),
        )
        .await?;
        if reply.status == 429 {
            let retry = reply
                .headers
                .iter()
                .find(|(k, _)| k.eq_ignore_ascii_case("retry-after"))
                .map(|(_, v)| v.as_str())
                .unwrap_or("unspecified");
            return Err(format!(
                "Jira rate limited; Retry-After={}; no automatic retry",
                retry.chars().take(128).collect::<String>()
            ));
        }
        if reply.status == 204 {
            return Ok(Value::Null);
        }
        if !matches!(reply.status, 200 | 201) {
            return Err(format!(
                "Jira HTTP {}; original effect requires observation",
                reply.status
            ));
        }
        if reply.body.len() > 1_048_576 {
            return Err("Jira response byte bound exceeded".into());
        }
        serde_json::from_str(&reply.body).map_err(|_| "invalid Jira JSON response".into())
    }
    pub async fn issue(&self, reference: &JiraIssueRef) -> Result<JiraIssue, String> {
        let value = self
            .request("GET", &["issue", &reference.id], &[], None)
            .await?;
        let issue = JiraIssue::parse(&self.binding.account, &value)?;
        if value.pointer("/fields/project/id").and_then(Value::as_str)
            != Some(self.binding.project_id.as_str())
        {
            return Err("Jira issue is outside installed project".into());
        }
        // A key is allowed as input; the result always returns the canonical numeric id.
        if issue.reference.id != reference.id && issue.key != reference.id {
            return Err("Jira lookup returned another issue".into());
        }
        Ok(issue)
    }
    pub async fn search(
        &self,
        jql: &str,
        cursor: Option<&str>,
        limit: u32,
    ) -> Result<JiraResult, String> {
        let mut body = json!({"jql":format!("project = {} AND ({jql})", quote(&self.binding.project_id)),"maxResults":limit,"fields":["summary","project","status","labels"]});
        if let Some(cursor) = cursor {
            body["nextPageToken"] = json!(cursor);
        }
        let value = self
            .request("POST", &["search", "jql"], &[], Some(body))
            .await?;
        let items = value["issues"]
            .as_array()
            .ok_or("Jira search issues missing")?;
        if items.len() > limit as usize {
            return Err("Jira search item bound exceeded".into());
        }
        let next = if value["isLast"].as_bool() == Some(true) {
            None
        } else {
            Some(required_string(&value, "nextPageToken")?)
        };
        if next
            .as_ref()
            .is_some_and(|s| s.len() > 4096 || Some(s.as_str()) == cursor)
        {
            return Err("Jira search cursor repeated or oversized".into());
        }
        let issues = items
            .iter()
            .map(|v| {
                if v.pointer("/fields/project/id").and_then(Value::as_str)
                    != Some(self.binding.project_id.as_str())
                {
                    return Err("Jira search returned another project".into());
                }
                JiraIssue::parse(&self.binding.account, v)
            })
            .collect::<Result<Vec<_>, String>>()?;
        Ok(JiraResult::Search {
            issues,
            next_page_token: next,
        })
    }
    pub async fn metadata(&self, issue_type: &str) -> Result<Value, String> {
        let mut all = Vec::new();
        let mut offset = 0;
        for _ in 0..20 {
            let page = self
                .request(
                    "GET",
                    &[
                        "issue",
                        "createmeta",
                        &self.binding.project_id,
                        "issuetypes",
                        issue_type,
                    ],
                    &[
                        ("startAt", offset.to_string()),
                        ("maxResults", "100".into()),
                    ],
                    None,
                )
                .await?;
            let fields = page["fields"]
                .as_array()
                .ok_or("Jira create fields missing")?;
            if page["startAt"].as_u64() != Some(offset) || fields.len() > 100 {
                return Err("invalid Jira metadata page".into());
            }
            let total = page["total"]
                .as_u64()
                .ok_or("Jira metadata total missing")?;
            offset += fields.len() as u64;
            all.extend(fields.iter().cloned());
            if offset >= total {
                return Ok(Value::Array(all));
            }
            if fields.is_empty() {
                return Err("Jira metadata pagination stalled".into());
            }
        }
        Err("Jira metadata page bound exceeded".into())
    }
}
pub(super) fn quote(value: &str) -> String {
    format!("\"{}\"", value.replace('\\', "\\\\").replace('"', "\\\""))
}

pub(super) fn validate_fields(
    fields: &Value,
    metadata: &Value,
    create: bool,
) -> Result<(), String> {
    let object = fields.as_object().ok_or("Jira fields are not an object")?;
    let descriptors: Vec<(&str, &Value)> = if let Some(list) = metadata.as_array() {
        list.iter()
            .map(|v| Ok((v["fieldId"].as_str().ok_or("Jira field id missing")?, v)))
            .collect::<Result<_, String>>()?
    } else {
        metadata
            .as_object()
            .ok_or("Jira field metadata missing")?
            .iter()
            .map(|(k, v)| (k.as_str(), v))
            .collect()
    };
    for key in object.keys() {
        let descriptor = descriptors
            .iter()
            .find(|(id, _)| *id == key)
            .map(|(_, v)| *v)
            .ok_or("Jira field is not available")?;
        if !create
            && !descriptor["operations"]
                .as_array()
                .is_some_and(|ops| ops.iter().any(|v| v == "set"))
        {
            return Err("Jira field is not settable".into());
        }
    }
    if create {
        for (id, field) in descriptors {
            if field["required"] == true
                && field["hasDefaultValue"] != true
                && !object.get(id).is_some_and(|v| {
                    !v.is_null()
                        && v.as_str().is_none_or(|s| !s.trim().is_empty())
                        && v.as_array().is_none_or(|a| !a.is_empty())
                })
            {
                return Err(format!("Jira required field missing: {id}"));
            }
        }
    }
    Ok(())
}
