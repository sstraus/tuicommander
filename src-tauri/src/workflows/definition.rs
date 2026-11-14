use serde::{Deserialize, Serialize};

// One host-owned write to the repository or its pull requests.
#[derive(Clone, Copy, Debug, Deserialize, Serialize, schemars::JsonSchema, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum PublicationAction {
    Push,
    CreateDraftPr,
    MarkReady,
}

#[derive(Clone, Copy, Debug, Deserialize, Serialize, schemars::JsonSchema, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum AgentRole {
    Coordinator,
    Planner,
    Implementer,
    Reviewer,
    Validator,
}
