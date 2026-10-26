use super::check::CheckReceipt;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum RunStatus {
    Running,
    Paused,
    Completed,
    Cancelled,
}

impl RunStatus {
    pub(super) fn as_str(self) -> &'static str {
        match self {
            Self::Running => "running",
            Self::Paused => "paused",
            Self::Completed => "completed",
            Self::Cancelled => "cancelled",
        }
    }
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct RunLimits {
    pub max_loops: u16,
    pub max_story_creations: u16,
    pub max_spawns: u16,
    #[serde(default = "default_max_parallel_stories")]
    pub max_parallel_stories: u16,
    pub max_duration_secs: u32,
}

const fn default_max_parallel_stories() -> u16 {
    2
}

impl Default for RunLimits {
    fn default() -> Self {
        Self {
            max_loops: 8,
            max_story_creations: 100,
            max_spawns: 24,
            max_parallel_stories: default_max_parallel_stories(),
            max_duration_secs: 86_400,
        }
    }
}

impl RunLimits {
    pub fn validate(&self) -> Result<(), String> {
        if self.max_loops == 0
            || self.max_loops > 100
            || self.max_story_creations == 0
            || self.max_story_creations > 100
            || self.max_spawns == 0
            || self.max_spawns > 100
            || self.max_parallel_stories == 0
            || self.max_parallel_stories > 8
            || self.max_duration_secs == 0
            || self.max_duration_secs > 604_800
        {
            return Err("run limits are outside supported bounds".into());
        }
        Ok(())
    }
}

#[derive(Clone, Copy, Debug, Deserialize, Serialize, PartialEq, Eq, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum AttemptOutcome {
    Completed,
    Failed,
    NeedsInput,
    Interrupted,
}

#[derive(Clone, Copy, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum AttemptState {
    Running,
    Reported,
    Interrupted,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct NodeAttempt {
    pub id: String,
    pub story_id: String,
    pub node_id: String,
    pub generation: u64,
    pub state: AttemptState,
    pub outcome: Option<AttemptOutcome>,
    #[serde(default)]
    pub agent: Option<AgentBinding>,
    #[serde(default)]
    pub report: Option<AttemptReport>,
    #[serde(default)]
    pub input_answer: Option<String>,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct AgentBinding {
    pub session_id: String,
    pub task_id: Option<String>,
    pub effect_id: String,
    pub prompt_contract_version: u16,
    pub prompt_sha256: String,
    pub audit_preview: String,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct CriterionResult {
    pub index: usize,
    pub satisfied: bool,
    pub evidence: String,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct InputRequest {
    pub question: String,
    #[serde(default)]
    pub options: Vec<String>,
}

#[derive(Clone, Copy, Debug, Deserialize, Serialize, PartialEq, Eq, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum ReviewDecision {
    Approved,
    ChangesRequested,
}

#[derive(Clone, Copy, Debug, Deserialize, Serialize, PartialEq, Eq, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum ReviewSeverity {
    Minor,
    Major,
    Blocker,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ReviewFinding {
    pub criterion_index: usize,
    pub severity: ReviewSeverity,
    pub summary: String,
    pub evidence: String,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ReviewAssessment {
    pub decision: ReviewDecision,
    pub artifact_digest: String,
    pub findings: Vec<ReviewFinding>,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct AttemptReport {
    pub contract_version: u16,
    pub run_id: String,
    pub story_id: String,
    pub story_revision: i64,
    pub attempt_id: String,
    pub generation: u64,
    pub outcome: AttemptOutcome,
    pub summary: String,
    pub criterion_results: Vec<CriterionResult>,
    pub evidence: Vec<String>,
    #[serde(default)]
    pub input_request: Option<InputRequest>,
    #[serde(default)]
    pub review: Option<ReviewAssessment>,
}

#[derive(Clone, Copy, Debug, Deserialize, Serialize, PartialEq, Eq, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum EffectKind {
    SpawnAgent,
    CreateStory,
    Notify,
    Integrate,
}

#[derive(Clone, Copy, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum EffectState {
    Intended,
    Succeeded,
    Failed,
    Uncertain,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct EffectIntent {
    pub id: String,
    pub key: String,
    pub kind: EffectKind,
    pub state: EffectState,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct StoryExecution {
    pub story_id: String,
    pub accepted: bool,
    #[serde(default)]
    pub accepted_revision: Option<i64>,
    #[serde(default)]
    pub worktree_path: Option<String>,
    pub attempt_ids: Vec<String>,
    #[serde(default)]
    pub check_receipts: Vec<CheckReceipt>,
    #[serde(default)]
    pub integration_receipt: Option<IntegrationReceipt>,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct IntegrationReceipt {
    pub story_revision: i64,
    pub canonical_ref: String,
    pub base_commit: String,
    pub source_commit: String,
    pub source_tree: String,
    pub merge_commit: String,
    pub merge_tree: String,
    pub post_checks: Vec<CheckReceipt>,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct CanonicalReceipt {
    pub canonical_ref: String,
    pub commit: String,
    pub tree: String,
    pub post_checks: Vec<CheckReceipt>,
}

/// The native root is distinct from its plan relationship and graph target.
#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq, JsonSchema)]
#[serde(tag = "type", content = "id", rename_all = "snake_case")]
pub enum RunTarget {
    Plan(String),
    Story(String),
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct RunSnapshot {
    #[serde(default)]
    pub event_contract_version: u16,
    pub id: String,
    pub project: String,
    #[serde(default)]
    pub canonical_ref: Option<String>,
    pub plan_id: String,
    #[serde(default)]
    pub root_target: Option<RunTarget>,
    pub definition_id: String,
    pub definition_revision: i64,
    pub story_definition_id: String,
    pub story_definition_revision: i64,
    pub status: RunStatus,
    pub sequence: i64,
    pub started_ms: i64,
    #[serde(default)]
    pub paused_since_ms: Option<i64>,
    #[serde(default)]
    pub paused_duration_ms: i64,
    /// Why the daemon paused the run; cleared on resume. Absent for manual pauses.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub pause_cause: Option<String>,
    pub limits: RunLimits,
    pub loops: u16,
    pub story_creations: u16,
    pub spawns: u16,
    pub planning_fingerprint: Option<String>,
    pub verification_fingerprint: Option<String>,
    pub stories: Vec<StoryExecution>,
    #[serde(default)]
    pub canonical_recertification: Option<CanonicalReceipt>,
    pub attempts: Vec<NodeAttempt>,
    pub effects: Vec<EffectIntent>,
    #[serde(default)]
    pub graph_executions: Vec<super::graph::GraphExecution>,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum RunEventKind {
    Graph {
        event: super::graph::GraphEvent,
    },
    Started {
        initial: Box<RunSnapshot>,
    },
    PlanningClosed {
        fingerprint: String,
    },
    PlanningReopened,
    AttemptStarted {
        attempt: Box<NodeAttempt>,
    },
    WorktreeAssigned {
        story_id: String,
        path: String,
    },
    AttemptReported {
        attempt_id: String,
        generation: u64,
        outcome: AttemptOutcome,
        #[serde(default)]
        report: Option<AttemptReport>,
    },
    InputAnswered {
        attempt_id: String,
        answer: String,
    },
    AgentBound {
        attempt_id: String,
        binding: AgentBinding,
    },
    LateReportIgnored {
        attempt_id: String,
        generation: u64,
    },
    AttemptInterrupted {
        attempt_id: String,
    },
    EffectReserved {
        effect: EffectIntent,
    },
    EffectChanged {
        effect_id: String,
        state: EffectState,
    },
    LoopAdvanced,
    StoryAccepted {
        story_id: String,
        #[serde(default)]
        revision: i64,
    },
    CheckRecorded {
        story_id: String,
        receipt: CheckReceipt,
    },
    StoryIntegrated {
        story_id: String,
        receipt: IntegrationReceipt,
    },
    CanonicalRecertified {
        receipt: CanonicalReceipt,
    },
    VerificationPassed {
        fingerprint: String,
    },
    Paused,
    PausedFor {
        cause: String,
    },
    DeadlineExpired {
        deadline_ms: i64,
    },
    Resumed,
    GraphResumed {
        execution_id: String,
        activation_id: String,
        resolution: String,
    },
    Cancelled,
    Completed,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct RunEvent {
    pub sequence: i64,
    pub command_id: String,
    #[serde(default)]
    pub command_hash: Option<String>,
    pub at_ms: i64,
    pub kind: RunEventKind,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq, JsonSchema)]
#[serde(tag = "action", rename_all = "snake_case", deny_unknown_fields)]
pub enum RunCommand {
    /// Internal only: graph transitions are not operator-supplied reports.
    #[schemars(skip)]
    Graph {
        transition: super::graph::GraphTransition,
    },
    /// Internal daemon primitive: only a reached running Agent may start an attempt.
    #[schemars(skip)]
    StartGraphAgent {
        execution_id: String,
        activation_id: String,
    },
    ClosePlanning,
    StartAttempt {
        story_id: String,
        node_id: String,
    },
    /// Internal only: a managed coordinator binds a registered worktree before spawn.
    #[schemars(skip)]
    AssignWorktree {
        story_id: String,
        path: String,
    },
    StartPlanAgent {
        node_id: String,
    },
    ReportAttempt {
        attempt_id: String,
        generation: u64,
        outcome: AttemptOutcome,
    },
    /// Internal only: the public run transport rejects this command.
    #[schemars(skip)]
    BindAgent {
        attempt_id: String,
        binding: AgentBinding,
    },
    /// Internal only: the public run transport rejects this command.
    #[schemars(skip)]
    ReportBoundAttempt {
        caller_session: String,
        report: AttemptReport,
    },
    AnswerInput {
        attempt_id: String,
        answer: String,
    },
    ReserveEffect {
        key: String,
        kind: EffectKind,
    },
    MarkEffect {
        effect_id: String,
        succeeded: bool,
    },
    ResolveUncertainEffect {
        effect_id: String,
        succeeded: bool,
    },
    AdvanceLoop,
    AcceptStory {
        story_id: String,
    },
    /// Internal only: receipts are computed by the backend check runner.
    #[schemars(skip)]
    RecordCheck {
        story_id: String,
        receipt: CheckReceipt,
    },
    /// Internal only: integration receipts are computed by the backend.
    #[schemars(skip)]
    RecordIntegration {
        story_id: String,
        receipt: IntegrationReceipt,
    },
    /// Internal only: canonical receipts are computed by the backend.
    #[schemars(skip)]
    RecordRecertification {
        receipt: CanonicalReceipt,
    },
    FinalVerificationPassed,
    Complete,
    Pause,
    /// Internal: the daemon pauses a run and keeps the cause it already knows.
    #[schemars(skip)]
    PauseFor {
        cause: String,
    },
    /// Internal daemon timer command, never a caller-supplied timeout assertion.
    #[schemars(skip)]
    ExpireDeadline,
    Resume,
    ResumeGraph {
        execution_id: String,
        activation_id: String,
        resolution: String,
    },
    Cancel,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct RunReceipt {
    pub sequence: i64,
    pub event: RunEvent,
    pub snapshot: RunSnapshot,
}
