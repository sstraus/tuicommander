//! Built-in neutral workflow templates. A template is an ordinary catalog
//! definition: the Designer opens it as a new unsaved draft, and the user edits,
//! saves, publishes and binds it like any other draft.
use super::model::EditorMetadata;
use crate::workflows::neutral_publication::NeutralDefinition;
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct NeutralTemplate {
    pub key: String,
    pub name: String,
    pub description: String,
    pub editor: EditorMetadata,
    pub definition: NeutralDefinition,
}

const SOURCES: &[&str] = &[
    include_str!("../templates/github-issue-triage.json"),
    include_str!("../templates/github-pr-review.json"),
];

pub(crate) fn templates() -> Result<Vec<NeutralTemplate>, String> {
    SOURCES
        .iter()
        .map(|source| {
            serde_json::from_str(source).map_err(|e| format!("neutral workflow template: {e}"))
        })
        .collect()
}
