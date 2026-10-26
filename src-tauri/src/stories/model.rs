use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct NewPlan {
    pub project: String,
    pub title: String,
    pub source: String,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct Plan {
    pub revision: i64,
    pub archived: bool,
    pub id: String,
    pub project: String,
    pub title: String,
    pub source: String,
}

/// Mutable plan metadata. Omitted fields retain their stored values.
#[derive(Clone, Debug, Default, Deserialize, Serialize, JsonSchema, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct PlanPatch {
    pub title: Option<String>,
    pub source: Option<String>,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
pub struct PlanSource {
    pub title: String,
    pub source: String,
}

#[derive(Clone, Copy, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum PlanState {
    Draft,
    Active,
    Done,
}

/// Where a story came from: `{"type":"native"}`, `{"type":"plan_step","step":"..."}` or `{"type":"git_hub_issue","number":123}`.
#[derive(Clone, Debug, Deserialize, Serialize, JsonSchema, PartialEq, Eq)]
#[serde(tag = "type", rename_all = "snake_case", from = "StrictStoryOrigin")]
pub enum StoryOrigin {
    Native,
    PlanStep {
        /// The plan step this story implements.
        step: String,
    },
    GitHubIssue {
        /// The GitHub issue number.
        number: u64,
    },
}

// Serde ignores extra fields on internally tagged unit variants, even with deny_unknown_fields.
// An empty struct variant enforces the same strict contract for the native origin.
#[derive(Deserialize, JsonSchema)]
#[serde(tag = "type", rename_all = "snake_case", deny_unknown_fields)]
enum StrictStoryOrigin {
    Native {},
    PlanStep { step: String },
    GitHubIssue { number: u64 },
}

impl From<StrictStoryOrigin> for StoryOrigin {
    fn from(origin: StrictStoryOrigin) -> Self {
        match origin {
            StrictStoryOrigin::Native {} => Self::Native,
            StrictStoryOrigin::PlanStep { step } => Self::PlanStep { step },
            StrictStoryOrigin::GitHubIssue { number } => Self::GitHubIssue { number },
        }
    }
}

/// A new story. Field names are camelCase.
#[derive(Clone, Debug, Deserialize, Serialize, JsonSchema, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct NewStory {
    /// Id of the plan that owns the story (from create_plan or list_plans).
    pub plan_id: String,
    pub title: String,
    /// Acceptance criteria, 1 to 100 entries.
    pub criteria: Vec<String>,
    /// 1 (highest), 2 or 3.
    #[schemars(range(min = 1, max = 3))]
    pub priority: u8,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub story_type: Option<StoryType>,
    pub origin: StoryOrigin,
    /// Repo-relative paths the story may touch; may be empty.
    pub file_scope: Vec<String>,
}

#[derive(Clone, Copy, Debug, Deserialize, Serialize, JsonSchema, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum StoryType {
    Feature,
    Fix,
    Refactor,
    Chore,
    Task,
}

#[derive(Clone, Copy, Debug, Default, Deserialize, Serialize, JsonSchema, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum StoryVisibility {
    #[default]
    All,
    Open,
    Closed,
    Qa,
}

#[derive(Clone, Debug, Default, Deserialize, Serialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct StoryFilter {
    #[serde(default)]
    pub archived: bool,
    #[serde(default)]
    pub visibility: StoryVisibility,
    pub story_type: Option<StoryType>,
    #[serde(default)]
    pub untyped_only: bool,
}

/// Optional conjunctive filters for the list service, separate from dispatch reads.
#[derive(Clone, Debug, Default, Deserialize, Serialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct StoryListFilter {
    #[serde(default)]
    pub archived: bool,
    pub state: Option<StoryStatus>,
    pub priority: Option<u8>,
    pub story_type: Option<StoryType>,
}

/// Keep the historical value array and add a sibling cursor only when more matches remain.
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(tag = "type", rename = "stories")]
pub struct StoryListPage {
    pub value: Vec<Story>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub next_cursor: Option<String>,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct StoryColumn {
    pub status: StoryStatus,
    pub story_ids: Vec<String>,
}

#[derive(Clone, Copy, Debug, Deserialize, Serialize, JsonSchema, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum StoryRisk {
    Low,
    Medium,
    High,
}

/// An amendment, not a whole-record replacement. Omitted fields remain unchanged.
#[derive(Clone, Debug, Default, Deserialize, Serialize, JsonSchema, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct StoryPatch {
    pub title: Option<String>,
    #[schemars(range(min = 1, max = 3))]
    pub priority: Option<u8>,
    pub file_scope: Option<Vec<String>>,
    pub origin: Option<StoryOrigin>,
    /// Null returns the story to the untyped historical category.
    #[serde(
        default,
        deserialize_with = "nullable_patch",
        skip_serializing_if = "Option::is_none"
    )]
    #[expect(
        clippy::option_option,
        reason = "omission preserves, null clears, and a value replaces"
    )]
    pub story_type: Option<Option<StoryType>>,
    /// Null clears the problem; omission preserves it.
    #[serde(
        default,
        deserialize_with = "nullable_patch",
        skip_serializing_if = "Option::is_none"
    )]
    #[expect(
        clippy::option_option,
        reason = "omission preserves, null clears, and a value replaces"
    )]
    pub problem: Option<Option<String>>,
    /// Null clears the risk; omission preserves it.
    #[serde(
        default,
        deserialize_with = "nullable_patch",
        skip_serializing_if = "Option::is_none"
    )]
    #[expect(
        clippy::option_option,
        reason = "omission preserves, null clears, and a value replaces"
    )]
    pub risk: Option<Option<StoryRisk>>,
    /// Ordered final criteria. Only unique, byte-identical criteria retain checks.
    pub criteria: Option<Vec<String>>,
    /// Final same-plan dependency set. Cycles and duplicates are rejected.
    pub dependencies: Option<Vec<String>>,
}

#[expect(
    clippy::option_option,
    reason = "the outer option distinguishes a present null from omission"
)]
fn nullable_patch<'de, D, T>(deserializer: D) -> Result<Option<Option<T>>, D::Error>
where
    D: serde::Deserializer<'de>,
    T: Deserialize<'de>,
{
    Option::<T>::deserialize(deserializer).map(Some)
}

#[derive(Clone, Copy, Debug, Deserialize, Serialize, JsonSchema, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum StoryStatus {
    Backlog,
    Ready,
    InProgress,
    Review,
    Qa,
    Done,
    Blocked,
    WontFix,
}

impl StoryStatus {
    pub(super) fn as_str(self) -> &'static str {
        match self {
            Self::Backlog => "backlog",
            Self::Ready => "ready",
            Self::InProgress => "in_progress",
            Self::Review => "review",
            Self::Qa => "qa",
            Self::Done => "done",
            Self::Blocked => "blocked",
            Self::WontFix => "wontfix",
        }
    }
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct Story {
    #[serde(default)]
    pub archived: bool,
    pub id: String,
    pub plan_id: String,
    pub title: String,
    pub criteria: Vec<String>,
    pub checked: Vec<bool>,
    pub dependencies: Vec<String>,
    pub priority: u8,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub story_type: Option<StoryType>,
    pub origin: StoryOrigin,
    pub file_scope: Vec<String>,
    pub status: StoryStatus,
    pub revision: i64,
    pub claim_session: Option<String>,
    #[serde(default)]
    pub problem: Option<String>,
    #[serde(default)]
    pub risk: Option<StoryRisk>,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct StoryRead {
    #[serde(flatten)]
    pub story: Story,
    pub abandoned: bool,
}

/// Progress over the active plans of a project: plans not yet complete, stories being worked on,
/// and accepted (done) against countable stories (neither won't-fix nor abandoned behind one).
#[derive(Clone, Debug, Default, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct StorySummary {
    pub active_plans: usize,
    pub in_progress: usize,
    pub qa: usize,
    pub done: usize,
    pub total: usize,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct PlanView {
    pub stories: Vec<StoryRead>,
    pub visible_story_ids: Vec<String>,
    pub columns: Vec<StoryColumn>,
    pub state: PlanState,
    pub wont_fix_count: usize,
    pub all_cancelled: bool,
}

/// Error prefixes of mistakes the user can fix; the HTTP route answers them 422, not 500.
pub const PLAN_SOURCE_IN_USE: &str = "plan source is in use by an active plan coordinator: ";
pub const PLAN_DOCUMENT_NOT_FOUND: &str = "plan document not found: ";
pub const CRITERIA_INCOMPLETE: &str = "criteria are incomplete: unchecked indexes ";
pub const STORY_LIST_QUERY_INVALID: &str = "invalid story list query: ";

/// A status command. Unit commands are plain strings (`"submit_review"`); the criterion commands
/// carry a zero-based index (`{"check_criterion":0}`).
#[derive(Clone, Debug, Deserialize, Serialize, JsonSchema, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum StoryCommand {
    /// Ready -> in_progress (or QA for manual-only work) without a session claim.
    StartManual,
    /// Only while in_progress or QA.
    CheckCriterion(usize),
    /// Only while in_progress or QA.
    UncheckCriterion(usize),
    /// Mark a criterion as manual with the [MANUAL] prefix, or remove that prefix.
    SetCriterionManual { index: usize, manual: bool },
    /// In progress/review -> QA, with no open non-manual criteria.
    SubmitQa,
    /// QA -> in progress after a failed manual check.
    RejectQa,
    /// Moves complete work to review, or manual-only pending work to QA.
    SubmitReview,
    /// Review/QA -> done when all criteria pass; releases eligible dependants.
    Approve,
    /// Review -> in_progress (claimed) or ready.
    RejectReview,
    /// Moves an active story to blocked.
    Block,
    /// Blocked -> ready, or backlog while a dependency is unfinished.
    Unblock,
    /// Cancels the story.
    WontFix,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum StoryTransitionActor {
    Human,
    LocalApi,
    ManagedSession { session_id: String },
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct StoryTransition {
    pub story_id: String,
    pub revision: i64,
    pub command: StoryCommand,
    pub actor: StoryTransitionActor,
}

/// Stable identity of an existing workflow event receipt, not a copied payload.
#[derive(Clone, Debug, Deserialize, Serialize, JsonSchema, PartialEq, Eq, Hash)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct StoryReceiptReference {
    pub run_id: String,
    pub command_id: String,
}

/// Append-only commentary against the story revision observed by its author.
#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct StoryNote {
    pub id: String,
    pub story_id: String,
    pub story_revision: i64,
    pub text: String,
    pub actor: StoryTransitionActor,
    /// Backend Unix timestamp in milliseconds.
    pub created_at: i64,
    pub receipts: Vec<StoryReceiptReference>,
}

pub const STORY_NOTE_INVALID: &str = "invalid story note: ";
/// Append-only archival event. Archival never changes status or deletes records.
#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ArchiveEvent {
    pub revision: i64,
    pub archived: bool,
    pub actor: StoryTransitionActor,
}
