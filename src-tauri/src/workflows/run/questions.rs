//! Branch-local decisions live in the run ledger, not in a coordinator inbox.
use super::*;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq, JsonSchema)]
#[serde(tag = "type", rename_all = "snake_case", deny_unknown_fields)]
pub enum AnswerSchema {
    Text,
    Boolean,
    SingleChoice { options: Vec<String> },
    MultipleChoice { options: Vec<String> },
}

impl AnswerSchema {
    fn validate(&self) -> Result<(), String> {
        if let Self::SingleChoice { options } | Self::MultipleChoice { options } = self
            && (options.is_empty()
                || options.len() > 16
                || options
                    .iter()
                    .enumerate()
                    .any(|(i, s)| s.trim().is_empty() || s.len() > 256 || options[..i].contains(s)))
        {
            return Err("question choices must be nonempty, unique and bounded".into());
        }
        Ok(())
    }

    fn accepts(&self, answer: &serde_json::Value) -> bool {
        match self {
            Self::Text => answer
                .as_str()
                .is_some_and(|s| !s.trim().is_empty() && s.len() <= 4000),
            Self::Boolean => answer.is_boolean(),
            Self::SingleChoice { options } => answer
                .as_str()
                .is_some_and(|s| options.iter().any(|o| o == s)),
            Self::MultipleChoice { options } => answer.as_array().is_some_and(|items| {
                !items.is_empty()
                    && items.len() <= options.len()
                    && items.iter().enumerate().all(|(i, v)| {
                        v.as_str().is_some_and(|s| options.iter().any(|o| o == s))
                            && !items[..i].contains(v)
                    })
            }),
        }
    }
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum QuestionActorKind {
    Human,
    Ai,
    Daemon,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct QuestionActor {
    pub kind: QuestionActorKind,
    pub id: String,
}
impl QuestionActor {
    fn validate(&self) -> Result<(), String> {
        if self.id.trim().is_empty() || self.id.len() > 256 {
            return Err("question actor must be bounded and nonempty".into());
        }
        Ok(())
    }
}

#[derive(Clone, Copy, Debug, Deserialize, Serialize, PartialEq, Eq, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum QuestionDeadlineOutcome {
    TimedOut,
    Incomplete,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct QuestionDeadline {
    pub at_ms: i64,
    pub outcome: QuestionDeadlineOutcome,
}

#[derive(Clone, Copy, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum QuestionState {
    Pending,
    Answered,
    TimedOut,
    Incomplete,
    Superseded,
    Cancelled,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct QuestionAnswer {
    pub actor: QuestionActor,
    pub value: serde_json::Value,
    pub at_ms: i64,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Question {
    pub id: String,
    pub revision: u64,
    pub attempt_id: String,
    pub generation: u64,
    pub graph_binding: Option<GraphAttemptBinding>,
    pub subject_digest: String,
    pub request: InputRequest,
    pub schema: AnswerSchema,
    pub requested_by: QuestionActor,
    pub opened_ms: i64,
    pub state: QuestionState,
    pub answer: Option<QuestionAnswer>,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct QuestionInboxItem {
    pub run_id: String,
    pub run_status: RunStatus,
    pub story_id: String,
    pub question: Question,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct QuestionInbox {
    pub unanswered_count: usize,
    pub items: Vec<QuestionInboxItem>,
}

pub(super) fn subject_digest(
    snapshot: &RunSnapshot,
    attempt: &NodeAttempt,
) -> Result<String, String> {
    // Source comes from the real store/workspace at command time, never an answer's assertion.
    let revision = if attempt.story_id == snapshot.plan_id {
        crate::stories::StoryStore::open()?
            .get_plan(&snapshot.plan_id)?
            .revision
    } else {
        crate::stories::StoryStore::open()?
            .get_story(&attempt.story_id)?
            .revision
    };
    let artifact = snapshot
        .stories
        .iter()
        .find(|s| s.story_id == attempt.story_id)
        .and_then(|s| s.worktree_path.as_deref())
        .map(|path| workspace_digest(std::path::Path::new(path)))
        .transpose()?;
    let bytes = serde_json::to_vec(&(
        &snapshot.id,
        &attempt.id,
        attempt.generation,
        revision,
        artifact,
    ))
    .map_err(|e| e.to_string())?;
    Ok(hex::encode(Sha256::digest(bytes)))
}

/// Questions may be asked during dirty work. Bind both staged/working bytes and
/// untracked files rather than requiring the implementer to commit before asking.
fn workspace_digest(path: &std::path::Path) -> Result<String, String> {
    use std::io::Read;
    let mut hash = Sha256::new();
    for args in [
        vec!["rev-parse", "HEAD"],
        vec![
            "diff",
            "--no-ext-diff",
            "--no-textconv",
            "--binary",
            "HEAD",
            "--",
        ],
        vec![
            "diff",
            "--cached",
            "--no-ext-diff",
            "--no-textconv",
            "--binary",
            "HEAD",
            "--",
        ],
    ] {
        let output = crate::git_cli::git_cmd(path)
            .args(args)
            .run_raw()
            .map_err(|e| e.to_string())?;
        if !output.status.success() {
            return Err("cannot bind question workspace subject".into());
        }
        hash.update((output.stdout.len() as u64).to_le_bytes());
        hash.update(output.stdout);
    }
    let output = crate::git_cli::git_cmd(path)
        .args(["ls-files", "--others", "--exclude-standard", "-z"])
        .run_raw()
        .map_err(|e| e.to_string())?;
    if !output.status.success() {
        return Err("cannot inventory question workspace subject".into());
    }
    for name in output
        .stdout
        .split(|b| *b == 0)
        .filter(|name| !name.is_empty())
    {
        let name = std::str::from_utf8(name).map_err(|_| "question workspace path is not UTF-8")?;
        hash.update((name.len() as u64).to_le_bytes());
        hash.update(name.as_bytes());
        let file = path.join(name);
        let metadata = std::fs::symlink_metadata(&file).map_err(|e| e.to_string())?;
        if metadata.file_type().is_symlink() {
            let target = std::fs::read_link(file).map_err(|e| e.to_string())?;
            hash.update(b"symlink");
            hash.update(target.to_string_lossy().as_bytes());
        } else {
            hash.update(b"file");
            hash.update(metadata.len().to_le_bytes());
            let mut input = std::fs::File::open(file).map_err(|e| e.to_string())?;
            let mut buffer = [0; 65536];
            loop {
                let read = input.read(&mut buffer).map_err(|e| e.to_string())?;
                if read == 0 {
                    break;
                }
                hash.update(&buffer[..read]);
            }
        }
    }
    Ok(hex::encode(hash.finalize()))
}

fn schema(request: &InputRequest) -> AnswerSchema {
    request.answer_schema.clone().unwrap_or_else(|| {
        if request.options.is_empty() {
            AnswerSchema::Text
        } else {
            AnswerSchema::SingleChoice {
                options: request.options.clone(),
            }
        }
    })
}

pub(super) fn make(
    snapshot: &RunSnapshot,
    attempt: &NodeAttempt,
    request: InputRequest,
    actor: QuestionActor,
    at_ms: i64,
) -> Result<Question, String> {
    if snapshot.questions.len() >= 4096
        || request.question.trim().is_empty()
        || request.question.len() > 2000
        || request.options.len() > 16
        || request
            .options
            .iter()
            .any(|s| s.trim().is_empty() || s.len() > 256)
    {
        return Err("invalid or exhausted question request".into());
    }
    actor.validate()?;
    let schema = schema(&request);
    schema.validate()?;
    if request
        .deadline
        .as_ref()
        .is_some_and(|d| d.at_ms <= at_ms || d.at_ms > at_ms.saturating_add(604_800_000))
    {
        return Err("question deadline must be within the next seven days".into());
    }
    let revision = snapshot
        .questions
        .iter()
        .filter(|q| q.attempt_id == attempt.id)
        .map(|q| q.revision)
        .max()
        .unwrap_or(0)
        .checked_add(1)
        .ok_or("question revision exhausted")?;
    Ok(Question {
        id: format!("input:{}", attempt.id),
        revision,
        attempt_id: attempt.id.clone(),
        generation: attempt.generation,
        graph_binding: attempt.graph_binding.clone(),
        subject_digest: subject_digest(snapshot, attempt)?,
        request,
        schema,
        requested_by: actor,
        opened_ms: at_ms,
        state: QuestionState::Pending,
        answer: None,
    })
}

pub(super) fn reported(
    snapshot: &RunSnapshot,
    attempt_id: String,
    generation: u64,
    outcome: AttemptOutcome,
    report: Option<AttemptReport>,
    at_ms: i64,
) -> Result<RunEventKind, String> {
    if snapshot.event_contract_version < 5 || outcome != AttemptOutcome::NeedsInput {
        return Ok(RunEventKind::AttemptReported {
            attempt_id,
            generation,
            outcome,
            report,
        });
    }
    let attempt = snapshot
        .attempts
        .iter()
        .find(|a| a.id == attempt_id)
        .ok_or("question attempt missing")?;
    let request = report
        .as_ref()
        .and_then(|r| r.input_request.clone())
        .unwrap_or(InputRequest {
            question: "Input required".into(),
            options: vec![],
            answer_schema: None,
            deadline: None,
        });
    let actor = attempt.agent.as_ref().map_or(
        QuestionActor {
            kind: QuestionActorKind::Daemon,
            id: "workflow".into(),
        },
        |a| QuestionActor {
            kind: QuestionActorKind::Ai,
            id: a.session_id.clone(),
        },
    );
    let question = make(snapshot, attempt, request, actor, at_ms)?;
    Ok(RunEventKind::QuestionRequested {
        question: Box::new(question),
        report,
    })
}

pub(super) fn choose(
    snapshot: &RunSnapshot,
    command: &RunCommand,
    at_ms: i64,
) -> Result<RunEventKind, String> {
    if snapshot.event_contract_version < 5 {
        return Err("branch questions require event contract 5".into());
    }
    let (id, revision) = match command {
        RunCommand::AnswerQuestion {
            question_id,
            expected_revision,
            ..
        }
        | RunCommand::ReviseQuestion {
            question_id,
            expected_revision,
            ..
        } => (question_id, *expected_revision),
        _ => return Err("not a question command".into()),
    };
    let question = snapshot
        .questions
        .iter()
        .rev()
        .find(|q| q.id == *id)
        .ok_or("question not found")?;
    if question.revision != revision
        || !matches!(
            question.state,
            QuestionState::Pending | QuestionState::TimedOut | QuestionState::Incomplete
        )
    {
        return Err("question revision or state is stale".into());
    }
    let attempt = eligible(snapshot, question)?;
    match command {
        RunCommand::AnswerQuestion {
            subject_digest: digest,
            actor,
            answer,
            ..
        } => {
            actor.validate()?;
            if question.state != QuestionState::Pending
                || question
                    .request
                    .deadline
                    .as_ref()
                    .is_some_and(|d| at_ms >= d.at_ms)
            {
                return Err("question deadline has expired".into());
            }
            if *digest != question.subject_digest
                || subject_digest(snapshot, attempt)? != question.subject_digest
            {
                return Err("question subject is stale; revise before answering".into());
            }
            if !question.schema.accepts(answer) {
                return Err("answer does not match question schema".into());
            }
            Ok(RunEventKind::QuestionAnswered {
                question_id: id.clone(),
                revision,
                subject_digest: digest.clone(),
                answer: QuestionAnswer {
                    actor: actor.clone(),
                    value: answer.clone(),
                    at_ms,
                },
            })
        }
        RunCommand::ReviseQuestion { request, actor, .. } => Ok(RunEventKind::QuestionRevised {
            question: Box::new(make(
                snapshot,
                attempt,
                request.clone(),
                actor.clone(),
                at_ms,
            )?),
        }),
        _ => unreachable!(),
    }
}

fn eligible<'a>(snapshot: &'a RunSnapshot, q: &Question) -> Result<&'a NodeAttempt, String> {
    let attempt = snapshot
        .attempts
        .iter()
        .find(|a| {
            a.id == q.attempt_id
                && a.generation == q.generation
                && a.state == AttemptState::Reported
                && a.outcome == Some(AttemptOutcome::NeedsInput)
                && a.input_answer.is_none()
        })
        .ok_or("question attempt is no longer eligible")?;
    if let Some(binding) = &q.graph_binding {
        let graph = snapshot
            .graph_executions
            .iter()
            .find(|g| g.id == binding.execution_id && !g.completed)
            .ok_or("question execution departed")?;
        let activation = graph
            .activations
            .iter()
            .find(|a| {
                a.id == binding.activation_id && a.state == super::graph::ActivationState::Suspended
            })
            .ok_or("question activation is no longer suspended")?;
        if activation.tokens.iter().any(|t| {
            !graph.forks.iter().any(|f| {
                f.id == t.fork_id
                    && f.generation == t.generation
                    && f.state == super::parallel::ForkState::Open
            })
        }) {
            return Err("question fork generation departed".into());
        }
    }
    Ok(attempt)
}

fn timer_key(q: &Question) -> String {
    format!("question:{}:{}", q.attempt_id, q.revision)
}

fn retire_timer(snapshot: &mut RunSnapshot, q: &Question) {
    if let Some(timer) = snapshot
        .timers
        .iter_mut()
        .find(|t| t.key == timer_key(q) && t.state == super::timer::TimerState::Pending)
    {
        timer.state = super::timer::TimerState::Cancelled;
    }
}

pub(super) fn open(
    snapshot: &mut RunSnapshot,
    question: &Question,
    report: Option<AttemptReport>,
    revised: bool,
) -> Result<(), String> {
    if snapshot.event_contract_version < 5
        || question.revision == 0
        || question.state != QuestionState::Pending
        || question.answer.is_some()
        || question.subject_digest.len() != 64
        || snapshot.questions.len() >= 4096
    {
        return Err("invalid question event".into());
    }
    question.schema.validate()?;
    question.requested_by.validate()?;
    let attempt = snapshot
        .attempts
        .iter_mut()
        .find(|a| a.id == question.attempt_id && a.generation == question.generation)
        .ok_or("question attempt missing")?;
    if attempt.graph_binding != question.graph_binding {
        return Err("question activation binding changed".into());
    }
    if revised {
        eligible(snapshot, question)?;
        if let Some(report) = snapshot
            .attempts
            .iter_mut()
            .find(|a| a.id == question.attempt_id)
            .and_then(|a| a.report.as_mut())
        {
            report.input_request = Some(question.request.clone());
        }
    } else {
        if attempt.state != AttemptState::Running {
            return Err("question attempt is not running".into());
        }
        attempt.state = AttemptState::Reported;
        attempt.outcome = Some(AttemptOutcome::NeedsInput);
        attempt.report = report;
        attempt.input_answer = None;
        if let Some(binding) = &question.graph_binding {
            let activation = snapshot
                .graph_executions
                .iter_mut()
                .find(|g| g.id == binding.execution_id)
                .and_then(|g| {
                    g.activations.iter_mut().find(|a| {
                        a.id == binding.activation_id
                            && a.state == super::graph::ActivationState::Running
                    })
                })
                .ok_or("question activation is not running")?;
            activation.state = super::graph::ActivationState::Suspended;
        }
    }
    if let Some(old) = snapshot
        .questions
        .iter()
        .rev()
        .find(|q| q.id == question.id)
        .cloned()
    {
        if old.revision + 1 != question.revision {
            return Err("question revision gap".into());
        }
        retire_timer(snapshot, &old);
        if matches!(
            old.state,
            QuestionState::Pending | QuestionState::TimedOut | QuestionState::Incomplete
        ) {
            snapshot
                .questions
                .iter_mut()
                .rev()
                .find(|q| q.id == old.id)
                .expect("old question")
                .state = QuestionState::Superseded;
        }
    } else if question.revision != 1 {
        return Err("question revision gap".into());
    }
    if let Some(deadline) = &question.request.deadline {
        use super::timer::*;
        let timer = DurableTimer {
            key: timer_key(question),
            scope: TimerScope::Service {
                owner: format!("question:{}", question.attempt_id),
            },
            purpose: TimerPurpose::QuestionDeadline {
                question_id: question.id.clone(),
            },
            due_ms: deadline.at_ms,
            expires_ms: deadline.at_ms,
            max_attempts: 1,
            base_backoff_ms: 1,
            max_backoff_ms: 1,
            expiry_outcome: match deadline.outcome {
                QuestionDeadlineOutcome::TimedOut => TimerOutcome::TimedOut,
                QuestionDeadlineOutcome::Incomplete => TimerOutcome::Incomplete,
            },
            attempt: 1,
            state: TimerState::Pending,
            outcome: None,
        };
        super::timer::apply(
            snapshot,
            &TimerEvent::Scheduled { timer },
            question.opened_ms,
        )?;
    }
    snapshot.questions.push(question.clone());
    Ok(())
}

pub(super) fn answered(
    snapshot: &mut RunSnapshot,
    id: &str,
    revision: u64,
    digest: &str,
    answer: &QuestionAnswer,
) -> Result<(), String> {
    let question = snapshot
        .questions
        .iter()
        .rev()
        .find(|q| q.id == id)
        .cloned()
        .ok_or("question missing")?;
    if question.revision != revision
        || question.subject_digest != digest
        || question.state != QuestionState::Pending
        || question
            .request
            .deadline
            .as_ref()
            .is_some_and(|d| answer.at_ms >= d.at_ms)
        || !question.schema.accepts(&answer.value)
    {
        return Err("stale or invalid question answer event".into());
    }
    answer.actor.validate()?;
    eligible(snapshot, &question)?;
    let text = answer
        .value
        .as_str()
        .map(str::to_owned)
        .unwrap_or_else(|| answer.value.to_string());
    super::reducer::resume_input_attempt(snapshot, &question.attempt_id, &text)?;
    if let Some(binding) = &question.graph_binding {
        snapshot
            .graph_executions
            .iter_mut()
            .find(|g| g.id == binding.execution_id)
            .and_then(|g| {
                g.activations
                    .iter_mut()
                    .find(|a| a.id == binding.activation_id)
            })
            .ok_or("question activation missing")?
            .state = super::graph::ActivationState::Running;
    }
    retire_timer(snapshot, &question);
    let current = snapshot
        .questions
        .iter_mut()
        .rev()
        .find(|q| q.id == id)
        .expect("question");
    current.state = QuestionState::Answered;
    current.answer = Some(answer.clone());
    Ok(())
}

/// Deadline receipts never resume work. Departed branches cannot keep actionable questions.
pub(super) fn reconcile(snapshot: &mut RunSnapshot) {
    for index in 0..snapshot.questions.len() {
        let q = snapshot.questions[index].clone();
        if !matches!(
            q.state,
            QuestionState::Pending | QuestionState::TimedOut | QuestionState::Incomplete
        ) {
            continue;
        }
        let state = if snapshot.status == RunStatus::Cancelled {
            Some(QuestionState::Cancelled)
        } else if eligible(snapshot, &q).is_err() {
            Some(QuestionState::Superseded)
        } else {
            snapshot
                .timers
                .iter()
                .find(|t| t.key == timer_key(&q) && t.state == super::timer::TimerState::Fired)
                .map(|t| match t.outcome {
                    Some(super::timer::TimerOutcome::TimedOut) => QuestionState::TimedOut,
                    _ => QuestionState::Incomplete,
                })
        };
        if let Some(state) = state {
            retire_timer(snapshot, &q);
            snapshot.questions[index].state = state;
        }
    }
}

pub(super) fn inbox(runs: impl IntoIterator<Item = RunSnapshot>) -> QuestionInbox {
    let mut items = vec![];
    for run in runs {
        for question in run.questions.iter().filter(|q| {
            matches!(
                q.state,
                QuestionState::Pending | QuestionState::TimedOut | QuestionState::Incomplete
            )
        }) {
            if let Some(attempt) = run.attempts.iter().find(|a| a.id == question.attempt_id) {
                items.push(QuestionInboxItem {
                    run_id: run.id.clone(),
                    run_status: run.status,
                    story_id: attempt.story_id.clone(),
                    question: question.clone(),
                });
            }
        }
    }
    QuestionInbox {
        unanswered_count: items.len(),
        items,
    }
}
