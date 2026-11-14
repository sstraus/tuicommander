//! Jira Cloud v3 operation contracts. Installed references never confer authority.
#![cfg_attr(
    not(test),
    expect(dead_code, reason = "011 awaits host composition in 008")
)]
mod catalog;
mod request;
pub(crate) use catalog::*;
pub(crate) use request::*;
use serde::{Deserialize, Serialize};
use serde_json::Value;

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct JiraIssueRef {
    pub account: String,
    pub id: String,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct JiraIssue {
    pub reference: JiraIssueRef,
    pub key: String,
    pub fields: Value,
}
impl JiraIssue {
    pub(crate) fn parse(account: &str, value: &Value) -> Result<Self, String> {
        Ok(Self {
            reference: JiraIssueRef {
                account: account.into(),
                id: required_string(value, "id")?,
            },
            key: required_string(value, "key")?,
            fields: value
                .get("fields")
                .cloned()
                .unwrap_or_else(|| serde_json::json!({})),
        })
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case", deny_unknown_fields)]
pub(crate) enum JiraResult {
    Issue {
        issue: JiraIssue,
    },
    Search {
        issues: Vec<JiraIssue>,
        next_page_token: Option<String>,
    },
    Comment {
        issue: JiraIssueRef,
        comment_id: String,
    },
    Transitions {
        issue: JiraIssueRef,
        transitions: Value,
    },
}

pub(crate) fn required_string(value: &Value, field: &str) -> Result<String, String> {
    value
        .get(field)
        .and_then(Value::as_str)
        .filter(|s| !s.trim().is_empty())
        .map(str::to_owned)
        .ok_or_else(|| format!("Jira response missing {field}"))
}

pub(crate) fn adf(text: &str) -> Value {
    serde_json::json!({"version":1,"type":"doc","content":[{"type":"paragraph","content":[{"type":"text","text":text}]}]})
}
