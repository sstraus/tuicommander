use super::*;
use std::collections::BTreeMap;

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct JiraFields {
    pub summary: Option<String>,
    pub description: Option<String>,
    pub labels: Option<Vec<String>>,
    pub priority_id: Option<String>,
    pub assignee_account_id: Option<String>,
    #[serde(default)]
    pub custom: BTreeMap<String, Value>,
}
impl JiraFields {
    pub(crate) fn payload(&self) -> Result<Value, String> {
        if self.custom.keys().any(|key| {
            !key.starts_with("customfield_")
                || !key[12..].bytes().all(|b| b.is_ascii_digit())
                || key.len() == 12
        }) {
            return Err("Jira custom fields require customfield_<numeric id>".into());
        }
        let mut fields: serde_json::Map<String, Value> = self.custom.clone().into_iter().collect();
        if let Some(summary) = &self.summary {
            nonempty(summary)?;
            fields.insert("summary".into(), Value::String(summary.clone()));
        }
        if let Some(description) = &self.description {
            fields.insert(
                "description".into(),
                if description.is_empty() {
                    Value::Null
                } else {
                    adf(description)
                },
            );
        }
        if let Some(labels) = &self.labels {
            for label in labels {
                nonempty(label)?;
            }
            fields.insert("labels".into(), serde_json::json!(labels));
        }
        if let Some(id) = &self.priority_id {
            nonempty(id)?;
            fields.insert("priority".into(), serde_json::json!({"id":id}));
        }
        if let Some(id) = &self.assignee_account_id {
            nonempty(id)?;
            fields.insert("assignee".into(), serde_json::json!({"accountId":id}));
        }
        Ok(Value::Object(fields))
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum JiraOperation {
    Lookup,
    Search,
    Create,
    Update,
    Comment,
    Transitions,
    Transition,
}
impl JiraOperation {
    pub(crate) fn name(self) -> &'static str {
        match self {
            Self::Lookup => "lookup",
            Self::Search => "search",
            Self::Create => "create",
            Self::Update => "update",
            Self::Comment => "comment",
            Self::Transitions => "transitions",
            Self::Transition => "transition",
        }
    }
    pub(crate) fn mutation(self) -> bool {
        matches!(
            self,
            Self::Create | Self::Update | Self::Comment | Self::Transition
        )
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "operation", rename_all = "snake_case", deny_unknown_fields)]
pub(crate) enum JiraRequest {
    Lookup {
        issue: JiraIssueRef,
    },
    Search {
        jql: String,
        next_page_token: Option<String>,
        limit: u32,
    },
    Create {
        issue_type_id: String,
        fields: JiraFields,
    },
    Update {
        issue: JiraIssueRef,
        fields: JiraFields,
    },
    Comment {
        issue: JiraIssueRef,
        body: String,
    },
    Transitions {
        issue: JiraIssueRef,
    },
    Transition {
        issue: JiraIssueRef,
        transition_id: String,
        target_status_id: String,
        fields: JiraFields,
    },
}
impl JiraRequest {
    pub(crate) fn operation(&self) -> JiraOperation {
        match self {
            Self::Lookup { .. } => JiraOperation::Lookup,
            Self::Search { .. } => JiraOperation::Search,
            Self::Create { .. } => JiraOperation::Create,
            Self::Update { .. } => JiraOperation::Update,
            Self::Comment { .. } => JiraOperation::Comment,
            Self::Transitions { .. } => JiraOperation::Transitions,
            Self::Transition { .. } => JiraOperation::Transition,
        }
    }
    pub(crate) fn issue(&self) -> Option<&JiraIssueRef> {
        match self {
            Self::Lookup { issue }
            | Self::Update { issue, .. }
            | Self::Comment { issue, .. }
            | Self::Transitions { issue }
            | Self::Transition { issue, .. } => Some(issue),
            _ => None,
        }
    }
    pub(crate) fn validate(&self, account: &str) -> Result<(), String> {
        if let Some(issue) = self.issue() {
            if issue.account != account {
                return Err("Jira issue belongs to another account".into());
            }
            nonempty(&issue.id)?;
            if !issue
                .id
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_')
            {
                return Err("invalid Jira issue identifier".into());
            }
        }
        match self {
            Self::Search {
                jql,
                next_page_token,
                limit,
            } => {
                nonempty(jql)?;
                if !(1..=100).contains(limit)
                    || next_page_token
                        .as_ref()
                        .is_some_and(|s| s.is_empty() || s.len() > 4096)
                {
                    return Err("invalid Jira search page bound".into());
                }
            }
            Self::Create {
                issue_type_id,
                fields,
            } => {
                nonempty(issue_type_id)?;
                nonempty(fields.summary.as_deref().ok_or("Jira summary required")?)?;
                fields.payload()?;
            }
            Self::Update { fields, .. } | Self::Transition { fields, .. } => {
                fields.payload()?;
            }
            Self::Comment { body, .. } => nonempty(body)?,
            _ => {}
        }
        if let Self::Transition {
            transition_id,
            target_status_id,
            ..
        } = self
        {
            nonempty(transition_id)?;
            nonempty(target_status_id)?;
        }
        Ok(())
    }
}

pub(super) fn nonempty(value: &str) -> Result<(), String> {
    if value.trim().is_empty() {
        Err("Jira value is empty".into())
    } else {
        Ok(())
    }
}

/// Classification chooses configured data, never the operation or its authority.
pub(crate) fn classified_issue_create(
    issue: &crate::github::GitHubIssue,
    classification: &str,
    issue_types: &BTreeMap<String, String>,
) -> Result<JiraRequest, String> {
    let issue_type_id = issue_types
        .get(classification)
        .ok_or("unconfigured Jira classification")?
        .clone();
    Ok(JiraRequest::Create {
        issue_type_id,
        fields: JiraFields {
            summary: Some(issue.title.clone()),
            description: Some(format!(
                "Source: {}\nGitHub issue #{}\nClassification: {}",
                issue.url, issue.number, classification
            )),
            labels: Some(
                issue
                    .labels
                    .iter()
                    .map(|label| {
                        label
                            .name
                            .chars()
                            .map(|c| if c.is_whitespace() { '-' } else { c })
                            .collect()
                    })
                    .collect(),
            ),
            priority_id: None,
            assignee_account_id: None,
            custom: BTreeMap::new(),
        },
    })
}
