//! Per-alias GraphQL access failures. A null field alone is not proof of absence.
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RepoAccessReason {
    NotFound,
    Forbidden,
    Unknown,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RepoAccessError {
    pub repo: String,
    pub reason: RepoAccessReason,
    pub message: String,
}

/// Preserve errors belonging to this alias, including nested-field failures.
/// Only an explicit NOT_FOUND may exclude the repository from future polls.
pub fn repo_access_error(
    response: &serde_json::Value,
    alias: &str,
    repo: &str,
) -> Option<RepoAccessError> {
    let errors: Vec<_> = response["errors"]
        .as_array()
        .into_iter()
        .flatten()
        .filter(|e| e["path"][0].as_str() == Some(alias))
        .collect();
    if errors.is_empty() && !response["data"][alias].is_null() {
        return None;
    }
    let message = if errors.is_empty() {
        "GitHub returned no repository data without an access reason.".to_string()
    } else {
        errors
            .iter()
            .map(|e| e["message"].as_str().unwrap_or("Unknown GraphQL error"))
            .collect::<Vec<_>>()
            .join("\n")
    };
    let reason = if errors
        .iter()
        .any(|e| e["type"].as_str() == Some("FORBIDDEN"))
    {
        RepoAccessReason::Forbidden
    } else if !errors.is_empty()
        && errors.iter().all(|e| {
            e["type"].as_str() == Some("NOT_FOUND")
                && e["path"].as_array().is_some_and(|p| p.len() == 1)
        })
    {
        RepoAccessReason::NotFound
    } else {
        RepoAccessReason::Unknown
    };
    Some(RepoAccessError {
        repo: repo.to_string(),
        reason,
        message,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn recorded_forbidden_is_not_a_not_found_or_an_org_restriction() {
        // Catches collapsing every null alias into NOT_FOUND and inventing an org policy.
        let response =
            serde_json::from_str(include_str!("fixtures/github/access-errors.json")).unwrap();
        let missing = repo_access_error(&response, "missing", "caplock").unwrap();
        assert_eq!(missing.reason, RepoAccessReason::NotFound);
        let denied = repo_access_error(&response, "restricted", "audit").unwrap();
        assert_eq!(denied.reason, RepoAccessReason::Forbidden);
        assert_eq!(
            denied.message,
            "sstraus does not have permission to retrieve auditLog information."
        );
    }

    #[test]
    fn null_without_error_is_unknown_not_a_day_long_exclusion() {
        // Catches treating an unexplained null as authoritative repository absence.
        let response = serde_json::json!({"data": {"repo0": null, "repo1": {"name": "ok"}}});
        assert_eq!(
            repo_access_error(&response, "repo0", "unknown")
                .unwrap()
                .reason,
            RepoAccessReason::Unknown
        );
        assert!(repo_access_error(&response, "repo1", "ok").is_none());
    }

    #[test]
    fn nested_not_found_does_not_exclude_a_repository_that_exists() {
        // Catches a missing nested object suppressing the whole repository for 24h.
        let response = serde_json::json!({"data": {"repo0": {"name": "ok"}}, "errors": [{"type": "NOT_FOUND", "path": ["repo0", "object"], "message": "missing object"}]});
        assert_eq!(
            repo_access_error(&response, "repo0", "ok").unwrap().reason,
            RepoAccessReason::Unknown
        );
    }
}
