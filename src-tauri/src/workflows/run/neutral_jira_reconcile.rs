//! Recovery only observes the original effect. Absence never authorizes replay.
use super::neutral_jira_client::{JiraClient, quote};
use super::neutral_jira_operation::{marker, observed_fields};
use crate::workflows::jira::*;
use serde_json::json;
use std::collections::BTreeSet;

pub(super) async fn reconcile(
    client: &JiraClient,
    request: &JiraRequest,
    digest: &str,
) -> Result<JiraResult, String> {
    match request {
        JiraRequest::Create { .. } => {
            // Enhanced search is eventually consistent: zero matches is uncertainty.
            let result = client
                .search(&format!("labels = {}", quote(&marker(digest))), None, 2)
                .await?;
            let JiraResult::Search {
                issues,
                next_page_token,
            } = result
            else {
                return Err("invalid Jira reconciliation page".into());
            };
            if issues.len() != 1 || next_page_token.is_some() {
                return Err("Jira original create not uniquely observed; no POST repeated".into());
            }
            let issue = client.issue(&issues[0].reference).await?;
            if !issue.fields["labels"]
                .as_array()
                .is_some_and(|labels| labels.iter().any(|v| v.as_str() == Some(&marker(digest))))
            {
                return Err("Jira create correlation missing".into());
            }
            Ok(JiraResult::Issue { issue })
        }
        JiraRequest::Update { issue, fields } => {
            observed_fields(client, issue, &fields.payload()?, None).await
        }
        JiraRequest::Transition {
            issue,
            target_status_id,
            fields,
            ..
        } => observed_fields(client, issue, &fields.payload()?, Some(target_status_id)).await,
        JiraRequest::Comment { issue, .. } => {
            client.issue(issue).await?;
            let mut offset = 0u64;
            let mut ids = BTreeSet::new();
            let mut found = None;
            for _ in 0..20 {
                let page = client
                    .request(
                        "GET",
                        &["issue", &issue.id, "comment"],
                        &[
                            ("startAt", offset.to_string()),
                            ("maxResults", "100".into()),
                        ],
                        None,
                    )
                    .await?;
                if page["startAt"].as_u64() != Some(offset) {
                    return Err("Jira comment page offset mismatch".into());
                }
                let comments = page["comments"]
                    .as_array()
                    .ok_or("Jira comment page missing")?;
                if comments.len() > 100 {
                    return Err("Jira comment page oversized".into());
                }
                let mut page_ids = BTreeSet::new();
                for comment in comments {
                    let id = required_string(comment, "id")?;
                    if !ids.insert(id.clone()) {
                        return Err("Jira comment pagination repeated an id".into());
                    }
                    page_ids.insert(
                        id.parse::<u64>()
                            .map_err(|_| "Jira comment id is not numeric")?,
                    );
                }
                if !page_ids.is_empty() {
                    // Cloud v3 GET comments does not expand properties. This POST
                    // reads existing IDs; it never creates a replacement comment.
                    let details = client
                        .request(
                            "POST",
                            &["comment", "list"],
                            &[("expand", "properties".into())],
                            Some(json!({"ids":page_ids})),
                        )
                        .await?;
                    let values = details["values"]
                        .as_array()
                        .ok_or("Jira comment properties missing")?;
                    let mut observed = BTreeSet::new();
                    for comment in values {
                        let id = required_string(comment, "id")?;
                        let numeric = id
                            .parse::<u64>()
                            .map_err(|_| "Jira comment id is not numeric")?;
                        if !page_ids.contains(&numeric) || !observed.insert(numeric) {
                            return Err(
                                "Jira comment properties returned foreign or duplicate id".into()
                            );
                        }
                        if comment["properties"].as_array().is_some_and(|p| {
                            p.iter().any(|p| {
                                p["key"] == "tuic.workflow.effect"
                                    && p["value"].as_str() == Some(digest)
                            })
                        }) && found.replace(id).is_some()
                        {
                            return Err("Jira comment correlation is not unique".into());
                        }
                    }
                    if observed != page_ids {
                        return Err("Jira comment properties omitted requested ids".into());
                    }
                }
                offset += comments.len() as u64;
                if offset >= page["total"].as_u64().ok_or("Jira comment total missing")? {
                    return Ok(JiraResult::Comment {
                        issue: issue.clone(),
                        comment_id: found
                            .ok_or("Jira original comment not observed; no POST repeated")?,
                    });
                }
                if comments.is_empty() {
                    return Err("Jira comment pagination stalled".into());
                }
            }
            Err("Jira comment reconciliation page bound exceeded".into())
        }
        _ => Err("Jira read activity has no mutation to reconcile".into()),
    }
}
