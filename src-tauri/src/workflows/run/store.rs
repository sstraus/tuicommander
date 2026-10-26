use super::check::{
    CheckReceipt, cancel_checks, clean_artifact, execute_run_check, git_output,
    require_merge_tree_support,
};
use super::model::*;
use super::reducer::apply_event;
use crate::stories::{NewStory, Story, StoryOrigin, StoryStatus, StoryStore};
use crate::workflows::{CheckDefinition, NodeKind, PublishedWorkflow, WorkflowKind, WorkflowStore};
use rusqlite::{Connection, OptionalExtension, TransactionBehavior, params};
use serde::{Serialize, de::DeserializeOwned};
use sha2::{Digest, Sha256};
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::Mutex;
use std::time::{Duration, SystemTime, UNIX_EPOCH};
use uuid::Uuid;

#[derive(Clone, Debug)]
pub struct RunStore {
    db_path: PathBuf,
}

/// Domain refusal at the atomic run-start boundary. Transports render Display.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum RunStartError {
    AlreadyActive {
        plan_id: String,
        active_run_id: String,
    },
    Other(String),
}

impl std::fmt::Display for RunStartError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::AlreadyActive {
                plan_id,
                active_run_id,
            } => write!(
                f,
                "workflow run already active for plan {plan_id}: {active_run_id}"
            ),
            Self::Other(message) => f.write_str(message),
        }
    }
}

impl std::error::Error for RunStartError {}

impl From<String> for RunStartError {
    fn from(message: String) -> Self {
        Self::Other(message)
    }
}

impl From<&str> for RunStartError {
    fn from(message: &str) -> Self {
        Self::Other(message.into())
    }
}

impl From<RunStartError> for String {
    fn from(error: RunStartError) -> Self {
        error.to_string()
    }
}

/// Layout version recorded in `PRAGMA user_version`. Stores created before the
/// version was recorded report 0 and already have the version 1 layout.
/// Version 2 adds graph events and additive JSON snapshot fields. Legacy JSON
/// is decoded with defaults, without inventing graph positions. Version 3
/// records explicit graph-root targets; older binaries must refuse that contract.
// 4: paused_for events; a build that supports 3 refuses the store instead of failing to decode them.
const RUN_STORE_SCHEMA_VERSION: i64 = 4;

mod approval;
mod runtime_start;
pub(crate) use runtime_start::GraphStartRequest;

/// Serialize a manual claim against graph-root reservations across databases.
/// The returned connection holds the run writer lock until the story commits.
pub(crate) fn guard_manual_story_start(
    project: &str,
    story_id: &str,
) -> Result<Option<Connection>, String> {
    let path = crate::config::config_dir().join("workflow_runs.sqlite3");
    if !path.exists() {
        return Ok(None);
    }
    let conn = RunStore::open_at(&path)?.connect()?;
    conn.execute_batch("BEGIN IMMEDIATE")
        .map_err(|e| format!("lock workflow reservations: {e}"))?;
    runtime_start::require_available_story(&conn, project, story_id)?;
    Ok(Some(conn))
}

/// Story and plan amendments must share the lock-only open contract, including the first write.
fn lock_run_amendment(path: &Path) -> Result<(Connection, bool), String> {
    // This is a lock-only connection. RunStore::connect changes journal mode and initializes
    // schema, which races a concurrent first amendment at PRAGMA journal_mode=WAL.
    let conn = Connection::open(path)
        .map_err(|error| format!("open story amendment reservations: {error}"))?;
    conn.busy_timeout(Duration::from_secs(5))
        .map_err(|error| format!("story amendment reservation timeout: {error}"))?;
    conn.execute_batch("BEGIN IMMEDIATE")
        .map_err(|error| format!("lock story amendment reservations: {error}"))?;
    let version: i64 = conn
        .pragma_query_value(None, "user_version", |row| row.get(0))
        .map_err(|error| format!("read story amendment run version: {error}"))?;
    if version > RUN_STORE_SCHEMA_VERSION {
        return Err(
            "workflow run store is newer than this build supports; update TUICommander".into(),
        );
    }
    let has_runs: bool = conn.query_row(
        "SELECT EXISTS(SELECT 1 FROM sqlite_master WHERE type='table' AND name='workflow_runs')",
        [], |row| row.get(0),
    ).map_err(|error| format!("read story amendment run schema: {error}"))?;
    Ok((conn, has_runs))
}

/// Fence amendments against both graph reservations and legacy dispatched story executions.
/// Always open the database so a concurrent first run cannot race an absent-file probe.
pub(crate) fn guard_story_amendment(
    path: &Path,
    project: &str,
    story_id: &str,
) -> Result<Connection, String> {
    let (conn, has_runs) = lock_run_amendment(path)?;
    if !has_runs {
        return Ok(conn);
    }
    {
        let mut stmt = conn.prepare(
            "SELECT snapshot_json FROM workflow_runs WHERE project=?1 AND status IN ('running','paused')"
        ).map_err(|error| format!("prepare story amendment ownership: {error}"))?;
        for row in stmt
            .query_map([project], |row| row.get::<_, String>(0))
            .map_err(|error| format!("read story amendment ownership: {error}"))?
        {
            let run: RunSnapshot =
                decode(&row.map_err(|error| format!("read story owner: {error}"))?)?;
            if run
                .graph_executions
                .iter()
                .any(|graph| graph.target_id == story_id)
            {
                return Err("story is reserved by an active workflow root".into());
            }
            if run
                .stories
                .iter()
                .any(|execution| execution.story_id == story_id)
            {
                return Err("story is owned by an active workflow run".into());
            }
        }
    }
    Ok(conn)
}

/// Keep a source correction serialized with plan attempt start/bind and report writes.
/// Open even an unused run store so a first run cannot race the source transaction.
/// The caller holds this writer before acquiring the story writer, matching coordinator creation.
pub(crate) fn guard_plan_source_update_in(
    db_path: &Path,
    plan_id: &str,
) -> Result<Connection, String> {
    let (conn, has_runs) = lock_run_amendment(db_path)?;
    if !has_runs {
        return Ok(conn);
    }
    {
        let mut stmt = conn.prepare(
            "SELECT snapshot_json FROM workflow_runs WHERE plan_id=?1 AND status IN ('running','paused')"
        ).map_err(|e| format!("find source-dependent plan attempts: {e}"))?;
        let rows = stmt
            .query_map([plan_id], |row| row.get::<_, String>(0))
            .map_err(|e| format!("read source-dependent plan attempts: {e}"))?;
        for row in rows {
            let snapshot: RunSnapshot = decode(&row.map_err(|e| format!("read plan run: {e}"))?)?;
            if snapshot.attempts.iter().any(|attempt| {
                attempt.story_id == plan_id && attempt.state == AttemptState::Running
            }) {
                return Err(format!(
                    "{}run {}; finish the coordinator or cancel the run before changing the source",
                    crate::stories::PLAN_SOURCE_IN_USE,
                    snapshot.id,
                ));
            }
        }
    }
    Ok(conn)
}

/// Hold the workflow writer lock until the native archive transaction commits.
/// Plan roots own all children; story roots own only their target/executions.
pub(crate) fn guard_native_archive(
    path: &Path,
    project: &str,
    plan_id: &str,
    story_id: Option<&str>,
) -> Result<Connection, String> {
    let (conn, has_runs) = lock_run_amendment(path)?;
    if has_runs {
        let mut stmt = conn.prepare(
            "SELECT snapshot_json FROM workflow_runs WHERE project=?1 AND plan_id=?2 AND status IN ('running','paused')"
        ).map_err(|e| format!("read archive reservations: {e}"))?;
        for row in stmt
            .query_map(params![project, plan_id], |row| row.get::<_, String>(0))
            .map_err(|e| format!("read archive reservations: {e}"))?
        {
            let run: RunSnapshot = decode(&row.map_err(|e| format!("read archive owner: {e}"))?)?;
            let owned = story_id.is_none_or(|id| match &run.root_target {
                Some(RunTarget::Story(target)) => {
                    target == id
                        || run.stories.iter().any(|execution| execution.story_id == id)
                        || run
                            .graph_executions
                            .iter()
                            .any(|graph| graph.target_id == id)
                }
                Some(RunTarget::Plan(_)) | None => true,
            });
            if owned {
                return Err("plan or story is owned by an active workflow run".into());
            }
        }
    }
    Ok(conn)
}

static SERVICE_RECEIPT_LOCK: Mutex<()> = Mutex::new(());
#[cfg(test)]
thread_local! {
    pub(super) static BEFORE_COMMIT: std::cell::RefCell<Option<Box<dyn FnOnce()>>> = const { std::cell::RefCell::new(None) };
}

impl RunStore {
    pub(crate) fn duration_expired(&self, run_id: &str) -> Result<bool, String> {
        Ok(now_ms() >= super::runtime::deadline_ms(&self.snapshot(run_id)?))
    }

    pub fn open() -> Result<Self, String> {
        Self::open_at(&crate::config::config_dir().join("workflow_runs.sqlite3"))
    }

    pub(crate) fn open_at(path: &Path) -> Result<Self, String> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)
                .map_err(|e| format!("create workflow run directory: {e}"))?;
        }
        let store = Self {
            db_path: path.to_path_buf(),
        };
        store.connect()?;
        Ok(store)
    }

    fn connect(&self) -> Result<Connection, String> {
        let conn =
            Connection::open(&self.db_path).map_err(|e| format!("open workflow run store: {e}"))?;
        conn.busy_timeout(Duration::from_secs(5))
            .map_err(|e| format!("workflow run busy timeout: {e}"))?;
        conn.pragma_update(None, "journal_mode", "WAL")
            .map_err(|e| format!("workflow run WAL: {e}"))?;
        conn.pragma_update(None, "foreign_keys", "ON")
            .map_err(|e| format!("workflow run foreign keys: {e}"))?;
        let stored_version: i64 = conn
            .pragma_query_value(None, "user_version", |row| row.get(0))
            .map_err(|e| format!("read workflow run schema version: {e}"))?;
        if stored_version > RUN_STORE_SCHEMA_VERSION {
            return Err(format!(
                "workflow run store schema version {stored_version} is newer than this build supports ({RUN_STORE_SCHEMA_VERSION}); update TUICommander"
            ));
        }
        conn.execute_batch(
            "CREATE TABLE IF NOT EXISTS workflow_runs (
                id TEXT PRIMARY KEY, project TEXT NOT NULL, plan_id TEXT NOT NULL,
                status TEXT NOT NULL, snapshot_json TEXT NOT NULL
            );
            CREATE UNIQUE INDEX IF NOT EXISTS one_active_workflow_per_plan
                ON workflow_runs(project,plan_id) WHERE status IN ('running','paused');
            CREATE TABLE IF NOT EXISTS workflow_events (
                run_id TEXT NOT NULL REFERENCES workflow_runs(id), sequence INTEGER NOT NULL,
                command_id TEXT NOT NULL, event_json TEXT NOT NULL, receipt_json TEXT NOT NULL,
                PRIMARY KEY(run_id,sequence), UNIQUE(run_id,command_id)
            );
            CREATE TABLE IF NOT EXISTS workflow_story_executions (
                run_id TEXT NOT NULL REFERENCES workflow_runs(id), story_id TEXT NOT NULL,
                document_json TEXT NOT NULL, PRIMARY KEY(run_id,story_id)
            );
            CREATE TABLE IF NOT EXISTS workflow_node_attempts (
                run_id TEXT NOT NULL REFERENCES workflow_runs(id), attempt_id TEXT NOT NULL,
                document_json TEXT NOT NULL, PRIMARY KEY(run_id,attempt_id)
            );
            CREATE TABLE IF NOT EXISTS workflow_effects (
                run_id TEXT NOT NULL REFERENCES workflow_runs(id), effect_id TEXT NOT NULL,
                effect_key TEXT NOT NULL, document_json TEXT NOT NULL,
                PRIMARY KEY(run_id,effect_id), UNIQUE(run_id,effect_key)
            );",
        )
        .map_err(|e| format!("prepare workflow run schema: {e}"))?;
        if stored_version < RUN_STORE_SCHEMA_VERSION {
            conn.pragma_update(None, "user_version", RUN_STORE_SCHEMA_VERSION)
                .map_err(|e| format!("record workflow run schema version: {e}"))?;
        }
        Ok(conn)
    }

    pub fn start_plan(
        &self,
        project: &str,
        plan_id: &str,
        definition_id: &str,
        definition_revision: i64,
        limits: RunLimits,
    ) -> Result<RunSnapshot, RunStartError> {
        self.start_plan_with(
            project,
            plan_id,
            definition_id,
            definition_revision,
            limits,
            true,
        )
    }

    /// Starts a run the way a pre-policy build did, so tests can model an
    /// in-flight run pinned to an unchecked story definition.
    #[cfg(test)]
    pub(crate) fn start_plan_pre_policy(
        &self,
        project: &str,
        plan_id: &str,
        definition_id: &str,
        definition_revision: i64,
        limits: RunLimits,
    ) -> Result<RunSnapshot, RunStartError> {
        self.start_plan_with(
            project,
            plan_id,
            definition_id,
            definition_revision,
            limits,
            false,
        )
    }

    fn start_plan_with(
        &self,
        project: &str,
        plan_id: &str,
        definition_id: &str,
        definition_revision: i64,
        limits: RunLimits,
        enforce_check_policy: bool,
    ) -> Result<RunSnapshot, RunStartError> {
        limits.validate()?;
        let owner = crate::progress::resolve_owning_project(Some(project))?
            .to_string_lossy()
            .to_string();
        let plan = StoryStore::open()?.require_unarchived_plan(plan_id)?;
        if plan.project != owner {
            return Err("plan does not belong to project".into());
        }
        let published = WorkflowStore::open()?.get_published(definition_id, definition_revision)?;
        if published.project != owner || published.kind != WorkflowKind::Plan {
            return Err("published plan workflow does not belong to project".into());
        }
        let dispatches: Vec<_> = published
            .graph
            .nodes
            .iter()
            .filter_map(|node| {
                if let NodeKind::StoryDispatch {
                    story_template_id,
                    story_revision,
                } = &node.kind
                {
                    Some((story_template_id.clone(), *story_revision))
                } else {
                    None
                }
            })
            .collect();
        if dispatches.len() != 1 {
            return Err("plan workflow requires one Story Dispatch node".into());
        }
        let (story_definition_id, story_definition_revision) = dispatches[0].clone();
        let story_definition = WorkflowStore::open()?
            .get_published(&story_definition_id, story_definition_revision)?;
        if story_definition.kind != WorkflowKind::Story || story_definition.project != owner {
            return Err("pinned story workflow does not belong to project".into());
        }
        if enforce_check_policy {
            require_nonempty_policy(&story_definition)?;
        }
        let initial = RunSnapshot {
            event_contract_version: super::graph::RUN_EVENT_CONTRACT_VERSION,
            id: Uuid::now_v7().to_string(),
            canonical_ref: git_output(Path::new(&owner), &["symbolic-ref", "HEAD"]).ok(),
            project: owner,
            plan_id: plan_id.into(),
            root_target: None,
            definition_id: definition_id.into(),
            definition_revision,
            story_definition_id,
            story_definition_revision,
            status: RunStatus::Running,
            sequence: 0,
            started_ms: now_ms(),
            paused_since_ms: None,
            paused_duration_ms: 0,
            pause_cause: None,
            limits,
            loops: 0,
            story_creations: 0,
            spawns: 0,
            planning_fingerprint: None,
            verification_fingerprint: None,
            stories: vec![],
            canonical_recertification: None,
            attempts: vec![],
            effects: vec![],
            graph_executions: vec![],
        };
        let event = RunEvent {
            sequence: 1,
            command_id: format!("start:{}", initial.id),
            command_hash: None,
            at_ms: initial.started_ms,
            kind: RunEventKind::Started {
                initial: Box::new(initial.clone()),
            },
        };
        let snapshot = apply_event(None, &event)?;
        let receipt = RunReceipt {
            sequence: 1,
            event: event.clone(),
            snapshot: snapshot.clone(),
        };
        let mut conn = self.connect()?;
        let tx = conn
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .map_err(|e| format!("begin run start: {e}"))?;
        StoryStore::open()?.require_unarchived_plan(plan_id)?;
        insert_started_run(&tx, &snapshot, "start plan run")?;
        insert_event(&tx, &snapshot.id, &receipt)?;
        tx.commit()
            .map_err(|e| format!("commit plan run start: {e}"))?;
        StoryStore::open()?.reconcile_integrated_dependencies(plan_id)?;
        Ok(snapshot)
    }

    /// Retain role authority after reports, exit, cancellation and manual claim release.
    pub(crate) fn is_story_implementer(
        &self,
        story_id: &str,
        session: &str,
    ) -> Result<bool, String> {
        let conn = self.connect()?;
        let mut stmt = conn
            .prepare("SELECT r.snapshot_json FROM workflow_runs r JOIN workflow_story_executions s ON s.run_id=r.id WHERE s.story_id=?1")
            .map_err(|e| format!("read workflow role authority: {e}"))?;
        for row in stmt
            .query_map([story_id], |row| row.get::<_, String>(0))
            .map_err(|e| format!("read workflow role authority: {e}"))?
        {
            let run: RunSnapshot = decode(&row.map_err(|e| format!("read workflow role: {e}"))?)?;
            for attempt in run.attempts.iter().filter(|a| {
                a.story_id == story_id && a.agent.as_ref().is_some_and(|a| a.session_id == session)
            }) {
                let definition = WorkflowStore::open()?
                    .get_published(&run.story_definition_id, run.story_definition_revision)?;
                if definition.graph.nodes.iter().any(|n| {
                    n.id == attempt.node_id
                        && matches!(
                            n.kind,
                            NodeKind::Agent {
                                role: crate::workflows::AgentRole::Implementer,
                                ..
                            }
                        )
                }) {
                    return Ok(true);
                }
            }
        }
        Ok(false)
    }

    pub fn snapshot(&self, run_id: &str) -> Result<RunSnapshot, String> {
        read_snapshot(&self.connect()?, run_id)
    }

    pub fn list_plan_runs(
        &self,
        project: &str,
        plan_id: &str,
        limit: usize,
    ) -> Result<Vec<RunSnapshot>, String> {
        if limit == 0 || limit > 100 {
            return Err("workflow run list limit must be between 1 and 100".into());
        }
        let conn = self.connect()?;
        let mut stmt = conn
            .prepare("SELECT snapshot_json FROM workflow_runs WHERE project=?1 AND plan_id=?2 ORDER BY rowid DESC LIMIT ?3")
            .map_err(|error| format!("prepare plan runs: {error}"))?;
        stmt.query_map(params![project, plan_id, limit as i64], |row| {
            row.get::<_, String>(0)
        })
        .map_err(|error| format!("list plan runs: {error}"))?
        .map(|row| decode(&row.map_err(|error| format!("read plan run: {error}"))?))
        .collect()
    }

    /// Validate immutable receipt identity and ownership without copying its payload.
    pub(crate) fn receipt_belongs_to_plan(
        &self,
        run_id: &str,
        command_id: &str,
        project: &str,
        plan_id: &str,
    ) -> Result<bool, String> {
        self.connect()?.query_row(
            "SELECT EXISTS(SELECT 1 FROM workflow_events e JOIN workflow_runs r ON r.id=e.run_id
             WHERE e.run_id=?1 AND e.command_id=?2 AND r.project=?3 AND r.plan_id=?4)",
            params![run_id, command_id, project, plan_id],
            |row| row.get(0),
        ).map_err(|error| format!("read workflow receipt identity: {error}"))
    }

    pub fn events_after(
        &self,
        run_id: &str,
        after_sequence: i64,
        limit: usize,
    ) -> Result<Vec<RunEvent>, String> {
        self.snapshot(run_id)?;
        if after_sequence < 0 || limit == 0 || limit > 500 {
            return Err("invalid workflow event cursor or limit".into());
        }
        let conn = self.connect()?;
        let mut stmt = conn.prepare("SELECT event_json FROM workflow_events WHERE run_id=?1 AND sequence>?2 ORDER BY sequence LIMIT ?3")
            .map_err(|e| format!("prepare workflow event replay: {e}"))?;
        stmt.query_map(params![run_id, after_sequence, limit as i64], |row| {
            row.get::<_, String>(0)
        })
        .map_err(|e| format!("read workflow events: {e}"))?
        .map(|row| decode(&row.map_err(|e| format!("read workflow event: {e}"))?))
        .collect()
    }

    #[cfg(test)]
    pub fn replay(&self, run_id: &str) -> Result<RunSnapshot, String> {
        replay_events(&self.connect()?, run_id)
    }

    pub fn command(
        &self,
        run_id: &str,
        command_id: &str,
        command: RunCommand,
    ) -> Result<RunReceipt, String> {
        self.command_at(run_id, command_id, command, now_ms())
    }

    /// Complete a previously reserved spawn effect in the same event that binds
    /// the live managed session to its attempt.
    pub fn bind_agent(
        &self,
        run_id: &str,
        attempt_id: &str,
        binding: AgentBinding,
    ) -> Result<RunReceipt, String> {
        self.command(
            run_id,
            &format!("bind-agent:{attempt_id}"),
            RunCommand::BindAgent {
                attempt_id: attempt_id.into(),
                binding,
            },
        )
    }

    /// The caller session is resolved from the MCP connection, never from an
    /// agent-supplied field. A stable command ID makes retries idempotent.
    pub fn report_bound_agent(
        &self,
        report: AttemptReport,
        caller_session: &str,
    ) -> Result<RunReceipt, String> {
        let run_id = report.run_id.clone();
        let command_id = format!("agent-report:{}", report.attempt_id);
        self.command(
            &run_id,
            &command_id,
            RunCommand::ReportBoundAttempt {
                caller_session: caller_session.into(),
                report,
            },
        )
    }

    /// Create plan work only for the run's bound coordinator. The reservation
    /// and the story database each have a durable key, so a lost tool response
    /// can be retried without creating another story.
    pub fn create_story_from_coordinator(
        &self,
        run_id: &str,
        caller_session: &str,
        proposal_key: &str,
        input: NewStory,
    ) -> Result<Story, String> {
        validate_key("agent session", caller_session)?;
        if proposal_key.trim().is_empty() || proposal_key.len() > 64 || proposal_key.contains('\0')
        {
            return Err("invalid story proposal key".into());
        }
        if !matches!(&input.origin, StoryOrigin::PlanStep { .. }) {
            return Err("workflow stories require a plan-step origin".into());
        }
        let snapshot = self.snapshot(run_id)?;
        if input.plan_id != snapshot.plan_id
            || !snapshot.attempts.iter().any(|attempt| {
                attempt.story_id == snapshot.plan_id
                    && attempt.state == AttemptState::Running
                    && attempt
                        .agent
                        .as_ref()
                        .is_some_and(|agent| agent.session_id == caller_session)
            })
        {
            return Err("only the active run coordinator may create plan stories".into());
        }
        let effect_key = format!("create-story:{proposal_key}");
        let effect = if let Some(effect) = snapshot
            .effects
            .iter()
            .find(|effect| effect.key == effect_key)
        {
            if effect.kind != EffectKind::CreateStory || effect.state == EffectState::Failed {
                return Err("story proposal effect cannot be retried".into());
            }
            effect.clone()
        } else {
            if snapshot.status != RunStatus::Running {
                return Err("workflow run is not running".into());
            }
            let reserved = self.command(
                run_id,
                &format!("create-story-intent:{proposal_key}"),
                RunCommand::ReserveEffect {
                    key: effect_key,
                    kind: EffectKind::CreateStory,
                },
            )?;
            let RunEventKind::EffectReserved { effect } = reserved.event.kind else {
                return Err("workflow state changed before story proposal".into());
            };
            effect
        };
        let story_store = StoryStore::open()?;
        let creation = match effect.state {
            EffectState::Intended => {
                // Hold the run writer lock across the other database's story
                // transaction. Cancel must either win first (no story is
                // created) or observe a story committed before cancellation.
                let mut conn = self.connect()?;
                let tx = conn
                    .transaction_with_behavior(TransactionBehavior::Immediate)
                    .map_err(|error| format!("begin workflow story creation: {error}"))?;
                let current = read_snapshot(&tx, run_id)?;
                if current.status != RunStatus::Running
                    || !current
                        .effects
                        .iter()
                        .any(|entry| entry.id == effect.id && entry.state == EffectState::Intended)
                    || !current.attempts.iter().any(|attempt| {
                        attempt.story_id == current.plan_id
                            && attempt.state == AttemptState::Running
                            && attempt
                                .agent
                                .as_ref()
                                .is_some_and(|agent| agent.session_id == caller_session)
                    })
                {
                    return Err("workflow stopped before story creation".into());
                }
                let created = story_store.create_story_once(run_id, proposal_key, input);
                tx.commit()
                    .map_err(|error| format!("commit workflow story reservation: {error}"))?;
                created
            }
            EffectState::Uncertain => {
                return Err(
                    "story proposal outcome is uncertain; operator reconciliation required".into(),
                );
            }
            EffectState::Succeeded => story_store
                .existing_story_for_proposal(run_id, proposal_key, &input)?
                .ok_or("completed story proposal has no story receipt".into()),
            EffectState::Failed => unreachable!(),
        };
        let story = match creation {
            Ok(story) => story,
            Err(error) => {
                if effect.state == EffectState::Intended {
                    let _ = self.command(
                        run_id,
                        &format!("create-story-failed:{proposal_key}"),
                        RunCommand::MarkEffect {
                            effect_id: effect.id,
                            succeeded: false,
                        },
                    );
                }
                return Err(error);
            }
        };
        let completed = match effect.state {
            EffectState::Intended => Some(RunCommand::MarkEffect {
                effect_id: effect.id,
                succeeded: true,
            }),
            EffectState::Succeeded => None,
            EffectState::Uncertain | EffectState::Failed => unreachable!(),
        };
        if let Some(command) = completed {
            self.command(
                run_id,
                &format!("create-story-complete:{proposal_key}"),
                command,
            )?;
        }
        Ok(story)
    }

    /// Process exit is an observation, never an outcome report. Fence it in the
    /// same transaction as the event so a racing semantic report wins or loses
    /// cleanly without converting an exit code into success.
    pub fn interrupt_agent_session(&self, session_id: &str) -> Result<Vec<RunSnapshot>, String> {
        validate_key("agent session", session_id)?;
        let mut conn = self.connect()?;
        let mut stmt = conn
            .prepare("SELECT id FROM workflow_runs WHERE status IN ('running','paused')")
            .map_err(|error| format!("prepare agent exit lookup: {error}"))?;
        let run_ids: Vec<String> = stmt
            .query_map([], |row| row.get(0))
            .map_err(|error| format!("read agent exit runs: {error}"))?
            .collect::<Result<_, _>>()
            .map_err(|error| format!("read agent exit run: {error}"))?;
        drop(stmt);
        let mut changed = Vec::new();
        for run_id in run_ids {
            let tx = conn
                .transaction_with_behavior(TransactionBehavior::Immediate)
                .map_err(|error| format!("begin agent exit: {error}"))?;
            let snapshot = read_snapshot(&tx, &run_id)?;
            if matches!(snapshot.status, RunStatus::Completed | RunStatus::Cancelled) {
                continue;
            }
            let attempts: Vec<String> = snapshot
                .attempts
                .iter()
                .filter(|attempt| {
                    attempt.state == AttemptState::Running
                        && attempt
                            .agent
                            .as_ref()
                            .is_some_and(|agent| agent.session_id == session_id)
                })
                .map(|attempt| attempt.id.clone())
                .collect();
            let had_pending = !attempts.is_empty();
            let mut current = snapshot;
            for attempt_id in attempts {
                let command_id = format!("agent-exit:{attempt_id}");
                current = persist_event(
                    &tx,
                    current,
                    &command_id,
                    None,
                    now_ms(),
                    RunEventKind::AttemptInterrupted { attempt_id },
                )?
                .snapshot;
            }
            tx.commit()
                .map_err(|error| format!("commit agent exit: {error}"))?;
            if had_pending {
                changed.push(current);
            }
        }
        Ok(changed)
    }

    pub fn command_expected(
        &self,
        run_id: &str,
        command_id: &str,
        expected_sequence: i64,
        command: RunCommand,
    ) -> Result<RunReceipt, String> {
        if expected_sequence < 1 {
            return Err("invalid expected workflow sequence".into());
        }
        self.command_at_checked(
            run_id,
            command_id,
            Some(expected_sequence),
            command,
            now_ms(),
        )
    }

    #[cfg(test)]
    pub(crate) fn record_check_receipt(
        &self,
        run_id: &str,
        story_id: &str,
        command_id: &str,
        expected_sequence: i64,
        receipt: CheckReceipt,
    ) -> Result<RunReceipt, String> {
        self.command_expected(
            run_id,
            command_id,
            expected_sequence,
            RunCommand::RecordCheck {
                story_id: story_id.into(),
                receipt,
            },
        )
    }

    pub(crate) fn record_integration_receipt(
        &self,
        run_id: &str,
        story_id: &str,
        command_id: &str,
        expected_sequence: i64,
        receipt: IntegrationReceipt,
    ) -> Result<RunReceipt, String> {
        self.command_expected(
            run_id,
            command_id,
            expected_sequence,
            RunCommand::RecordIntegration {
                story_id: story_id.into(),
                receipt,
            },
        )
    }

    /// Record a merge already present on the canonical branch. The backend
    /// verifies the source and merge parents, then executes the pinned checks
    /// against the merged commit before committing an event.
    pub fn record_integrated_story(
        &self,
        run_id: &str,
        story_id: &str,
        command_id: &str,
        expected_sequence: i64,
    ) -> Result<RunReceipt, String> {
        let _service_guard = SERVICE_RECEIPT_LOCK
            .lock()
            .map_err(|_| "workflow receipt service lock is poisoned")?;
        if let Some(prior) = self.existing_service_receipt(run_id, command_id, expected_sequence)? {
            match &prior.event.kind {
                RunEventKind::StoryIntegrated {
                    story_id: prior_story,
                    ..
                } if prior_story == story_id => {
                    StoryStore::open()?
                        .reconcile_integrated_dependencies(&prior.snapshot.plan_id)?;
                    return Ok(prior);
                }
                _ => return Err("workflow command id was reused with a different payload".into()),
            }
        }
        let snapshot = self.snapshot(run_id)?;
        if snapshot.sequence != expected_sequence {
            return Err("stale workflow sequence".into());
        }
        let execution = snapshot
            .stories
            .iter()
            .find(|item| item.story_id == story_id)
            .ok_or("story execution not found")?;
        let revision = execution.accepted_revision.ok_or("story is not accepted")?;
        let story = StoryStore::open()?.get_story(story_id)?;
        if story.status != StoryStatus::Done
            || !story_work_revision_current(story_id, revision, story.revision)?
        {
            return Err("integration requires the accepted story revision".into());
        }
        let worktree = Path::new(
            execution
                .worktree_path
                .as_deref()
                .ok_or("story worktree is missing")?,
        );
        let (source_commit, source_tree) = clean_artifact(worktree)?;
        let source_ref = git_output(worktree, &["symbolic-ref", "HEAD"])?;
        let definition = WorkflowStore::open()?.get_published(
            &snapshot.story_definition_id,
            snapshot.story_definition_revision,
        )?;
        require_nonempty_policy(&definition)?;
        require_current_checks(
            &definition.required_checks,
            &execution.check_receipts,
            &source_ref,
            &source_commit,
            &source_tree,
        )?;
        let canonical = Path::new(&snapshot.project);
        let canonical_ref = git_output(canonical, &["symbolic-ref", "HEAD"])?;
        if snapshot.canonical_ref.as_deref() != Some(canonical_ref.as_str()) {
            return Err("canonical branch moved since the run started".into());
        }
        let (merge_commit, merge_tree) = clean_artifact(canonical)?;
        let parents = git_output(canonical, &["rev-list", "--parents", "-n", "1", "HEAD"])?;
        let parts: Vec<_> = parents.split_whitespace().collect();
        if parts.len() != 3 || parts[0] != merge_commit || parts[2] != source_commit {
            return Err("canonical HEAD is not a merge of the checked story commit".into());
        }
        let base_commit = parts[1].to_owned();
        require_merge_tree_support(canonical)?;
        let expected_tree = git_output(
            canonical,
            &["merge-tree", "--write-tree", "--no-messages", &base_commit, &source_commit],
        ).map_err(|error| format!(
            "clean merge could not be verified; conflict resolution requires explicit human review or a separately verified artifact: {error}"
        ))?;
        if expected_tree != merge_tree {
            return Err("canonical merge tree differs from the verified clean merge".into());
        }
        let mut post_checks = Vec::with_capacity(definition.required_checks.len());
        for check in &definition.required_checks {
            let receipt = execute_run_check(check, canonical, &self.db_path, run_id, false)?;
            if receipt.exit_code != 0 {
                return Err(format!("post-integration check {} failed", check.id));
            }
            post_checks.push(receipt);
        }
        if git_output(canonical, &["symbolic-ref", "HEAD"])? != canonical_ref
            || clean_artifact(canonical)? != (merge_commit.clone(), merge_tree.clone())
            || git_output(worktree, &["symbolic-ref", "HEAD"])? != source_ref
            || clean_artifact(worktree)? != (source_commit.clone(), source_tree.clone())
        {
            return Err("integration ref or tree moved after checks".into());
        }
        let receipt = IntegrationReceipt {
            story_revision: revision,
            canonical_ref,
            base_commit,
            source_commit,
            source_tree,
            merge_commit,
            merge_tree,
            post_checks,
        };
        let result = self.record_integration_receipt(
            run_id,
            story_id,
            command_id,
            expected_sequence,
            receipt,
        )?;
        StoryStore::open()?.reconcile_integrated_dependencies(&snapshot.plan_id)?;
        Ok(result)
    }

    /// Recheck a clean canonical tip after an unrelated commit advanced it.
    /// The event certifies only previously accepted integrations whose source
    /// commits still belong to the branch; it does not integrate a new story.
    pub fn recertify_canonical(
        &self,
        run_id: &str,
        command_id: &str,
        expected_sequence: i64,
    ) -> Result<RunReceipt, String> {
        let _service_guard = SERVICE_RECEIPT_LOCK
            .lock()
            .map_err(|_| "workflow receipt service lock is poisoned")?;
        if let Some(prior) = self.existing_service_receipt(run_id, command_id, expected_sequence)? {
            return match &prior.event.kind {
                RunEventKind::CanonicalRecertified { .. } => {
                    StoryStore::open()?
                        .reconcile_integrated_dependencies(&prior.snapshot.plan_id)?;
                    Ok(prior)
                }
                _ => Err("workflow command id was reused with a different payload".into()),
            };
        }
        let snapshot = self.snapshot(run_id)?;
        if snapshot.sequence != expected_sequence {
            return Err("stale workflow sequence".into());
        }
        let canonical = Path::new(&snapshot.project);
        let canonical_ref = git_output(canonical, &["symbolic-ref", "HEAD"])?;
        if snapshot.canonical_ref.as_deref() != Some(canonical_ref.as_str()) {
            return Err("canonical branch moved since the run started".into());
        }
        let (commit, tree) = clean_artifact(canonical)?;
        let stories = StoryStore::open()?;
        let integrated: Vec<_> = snapshot
            .stories
            .iter()
            .filter_map(|item| {
                item.integration_receipt
                    .as_ref()
                    .map(|receipt| (item, receipt))
            })
            .collect();
        if integrated.is_empty() {
            return Err("run has no story integration to recertify".into());
        }
        for (execution, receipt) in integrated {
            let story = stories.get_story(&execution.story_id)?;
            if story.status != StoryStatus::Done
                || !execution.accepted
                || execution.accepted_revision != Some(receipt.story_revision)
                || !story_work_revision_current(&story.id, receipt.story_revision, story.revision)?
                || receipt.canonical_ref != canonical_ref
                || !source_is_ancestor(canonical, &receipt.source_commit, &commit)?
            {
                return Err("integrated story is stale or absent from canonical HEAD".into());
            }
        }
        let definition = WorkflowStore::open()?.get_published(
            &snapshot.story_definition_id,
            snapshot.story_definition_revision,
        )?;
        require_nonempty_policy(&definition)?;
        let mut post_checks = Vec::with_capacity(definition.required_checks.len());
        for check in &definition.required_checks {
            let receipt = execute_run_check(
                check,
                canonical,
                &self.db_path,
                run_id,
                snapshot.status == RunStatus::Cancelled,
            )?;
            if receipt.exit_code != 0 {
                return Err(format!("post-integration check {} failed", check.id));
            }
            post_checks.push(receipt);
        }
        if git_output(canonical, &["symbolic-ref", "HEAD"])? != canonical_ref
            || clean_artifact(canonical)? != (commit.clone(), tree.clone())
        {
            return Err("canonical ref or tree moved after checks".into());
        }
        let receipt = CanonicalReceipt {
            canonical_ref,
            commit,
            tree,
            post_checks,
        };
        let result = self.command_expected(
            run_id,
            command_id,
            expected_sequence,
            RunCommand::RecordRecertification { receipt },
        )?;
        stories.reconcile_integrated_dependencies(&snapshot.plan_id)?;
        Ok(result)
    }

    /// Execute a check pinned by the run's published story definition. The
    /// receipt is recorded only while its story revision and Git artifact stay current.
    pub fn execute_check(
        &self,
        run_id: &str,
        story_id: &str,
        check_id: &str,
        command_id: &str,
        expected_sequence: i64,
    ) -> Result<RunReceipt, String> {
        if expected_sequence < 1 {
            return Err("invalid expected workflow sequence".into());
        }
        validate_key("command id", command_id)?;
        let prior = read_command_receipt(&self.connect()?, run_id, command_id)?;
        if let Some(prior) = prior {
            return check_receipt_retry(prior, story_id, check_id, expected_sequence);
        }
        let snapshot = self.snapshot(run_id)?;
        if matches!(snapshot.status, RunStatus::Completed | RunStatus::Cancelled) {
            return Err("terminal workflow cannot execute a check".into());
        }
        if snapshot.sequence != expected_sequence {
            return Err("stale workflow sequence".into());
        }
        let execution = snapshot
            .stories
            .iter()
            .find(|story| story.story_id == story_id)
            .ok_or("story execution not found")?;
        let path = execution
            .worktree_path
            .as_ref()
            .ok_or("story worktree is missing")?;
        let definition = WorkflowStore::open()?.get_published(
            &snapshot.story_definition_id,
            snapshot.story_definition_revision,
        )?;
        let check = definition
            .required_checks
            .iter()
            .find(|check| check.id == check_id)
            .ok_or("check is not pinned by the story definition")?;
        let receipt = execute_run_check(check, Path::new(path), &self.db_path, run_id, false)?;
        self.commit_completed_check(run_id, execution, command_id, expected_sequence, receipt)
    }

    /// Commit against the story inputs rather than the run-wide cursor: another
    /// worker may have appended an unrelated event while the check was running.
    pub(super) fn commit_completed_check(
        &self,
        run_id: &str,
        execution: &StoryExecution,
        command_id: &str,
        expected_sequence: i64,
        receipt: CheckReceipt,
    ) -> Result<RunReceipt, String> {
        validate_key("command id", command_id)?;
        if command_id.starts_with("start:") || command_id.starts_with("reconcile-") {
            return Err("reserved workflow command id".into());
        }
        let command = RunCommand::RecordCheck {
            story_id: execution.story_id.clone(),
            receipt: receipt.clone(),
        };
        let hash = command_hash(Some(expected_sequence), &command)?;
        let conn = self.connect()?;
        if let Some(prior) = read_command_receipt(&conn, run_id, command_id)? {
            return check_receipt_retry(
                prior,
                &execution.story_id,
                &receipt.check_id,
                expected_sequence,
            );
        }
        let snapshot = read_snapshot(&conn, run_id)?;
        drop(conn);
        let revisions = fingerprint(&snapshot.plan_id, &plan_stories(&snapshot)?, true)?;
        let current = snapshot
            .stories
            .iter()
            .find(|item| item.story_id == execution.story_id)
            .ok_or("story execution not found")?;
        if current.accepted != execution.accepted
            || current.accepted_revision != execution.accepted_revision
            || current.worktree_path != execution.worktree_path
        {
            return Err("workflow check inputs changed before receipt commit".into());
        }
        let path = current
            .worktree_path
            .as_deref()
            .ok_or("story worktree is missing")?;
        if clean_artifact(Path::new(path))? != (receipt.commit.clone(), receipt.tree.clone())
            || git_output(Path::new(path), &["symbolic-ref", "HEAD"])? != receipt.ref_name
        {
            return Err("workflow check artifact moved before receipt commit".into());
        }
        let at_ms = now_ms();
        let kind = choose_event(&snapshot, command, at_ms)?;
        // A policy-triggered pause or planning reopen must not look like a check receipt.
        if !matches!(kind, RunEventKind::CheckRecorded { .. }) {
            return Err("workflow no longer accepts the completed check".into());
        }
        let mut conn = self.connect()?;
        let tx = conn
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .map_err(|e| format!("begin workflow check receipt: {e}"))?;
        if let Some(prior) = read_command_receipt(&tx, run_id, command_id)? {
            return check_receipt_retry(
                prior,
                &execution.story_id,
                &receipt.check_id,
                expected_sequence,
            );
        }
        validate_preflight(&tx, &snapshot, &revisions)?;
        let result = persist_event(&tx, snapshot, command_id, Some(hash), at_ms, kind)?;
        tx.commit()
            .map_err(|e| format!("commit workflow check receipt: {e}"))?;
        Ok(result)
    }

    fn existing_service_receipt(
        &self,
        run_id: &str,
        command_id: &str,
        expected_sequence: i64,
    ) -> Result<Option<RunReceipt>, String> {
        validate_key("command id", command_id)?;
        let conn = self.connect()?;
        let prior = read_command_receipt(&conn, run_id, command_id)?;
        let next_sequence = expected_sequence
            .checked_add(1)
            .ok_or("invalid expected workflow sequence")?;
        if prior
            .as_ref()
            .is_some_and(|item| item.sequence != next_sequence)
        {
            return Err("workflow command id was reused with a different expected sequence".into());
        }
        Ok(prior)
    }

    pub(crate) fn command_at(
        &self,
        run_id: &str,
        command_id: &str,
        command: RunCommand,
        at_ms: i64,
    ) -> Result<RunReceipt, String> {
        self.command_at_checked(run_id, command_id, None, command, at_ms)
    }

    fn command_at_checked(
        &self,
        run_id: &str,
        command_id: &str,
        expected_sequence: Option<i64>,
        command: RunCommand,
        at_ms: i64,
    ) -> Result<RunReceipt, String> {
        self.command_at_checked_with_precommit(
            run_id,
            command_id,
            expected_sequence,
            command,
            at_ms,
            || {
                #[cfg(test)]
                BEFORE_COMMIT.with(|slot| {
                    let hook = slot.borrow_mut().take();
                    if let Some(hook) = hook {
                        hook();
                    }
                });
            },
        )
    }

    fn command_at_checked_with_precommit(
        &self,
        run_id: &str,
        command_id: &str,
        expected_sequence: Option<i64>,
        command: RunCommand,
        at_ms: i64,
        precommit: impl FnOnce(),
    ) -> Result<RunReceipt, String> {
        validate_key("command id", command_id)?;
        if command_id.starts_with("start:") || command_id.starts_with("reconcile-") {
            return Err("reserved workflow command id".into());
        }
        let command_hash = command_hash(expected_sequence, &command)?;
        let conn = self.connect()?;
        if let Some(receipt) = read_command_receipt(&conn, run_id, command_id)? {
            return command_retry(receipt, &command_hash);
        }
        let snapshot = read_snapshot(&conn, run_id)?;
        drop(conn);
        if expected_sequence.is_some_and(|expected| expected != snapshot.sequence) {
            return Err("stale workflow sequence".into());
        }
        let revisions = fingerprint(&snapshot.plan_id, &plan_stories(&snapshot)?, true)?;
        let dispatch_target = match &command {
            RunCommand::Graph {
                transition: super::graph::GraphTransition::Start { target_id, .. },
            } if *target_id != snapshot.plan_id
                && matches!(snapshot.root_target, Some(RunTarget::Plan(_))) =>
            {
                Some(target_id.clone())
            }
            _ => None,
        };
        let cancelled = matches!(&command, RunCommand::Cancel);
        // All filesystem/Git probes finish before acquiring the writer lock.
        let kind = match choose_event(&snapshot, command, at_ms) {
            Ok(kind) => kind,
            Err(error) => {
                let conn = self.connect()?;
                return match read_command_receipt(&conn, run_id, command_id)? {
                    Some(receipt) => command_retry(receipt, &command_hash),
                    None => Err(error),
                };
            }
        };
        precommit();
        let mut conn = self.connect()?;
        let tx = conn
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .map_err(|e| format!("begin workflow command: {e}"))?;
        if let Some(receipt) = read_command_receipt(&tx, run_id, command_id)? {
            return command_retry(receipt, &command_hash);
        }
        validate_preflight(&tx, &snapshot, &revisions)?;
        if let Some(target) = dispatch_target {
            let story = StoryStore::open()?.get_story(&target)?;
            require_dispatch_slot(&tx, &snapshot, &story)?;
        }
        let receipt = persist_event(&tx, snapshot, command_id, Some(command_hash), at_ms, kind)?;
        tx.commit()
            .map_err(|e| format!("commit workflow command: {e}"))?;
        if cancelled {
            cancel_checks(&self.db_path, run_id);
        }
        Ok(receipt)
    }

    /// Refresh projections at runtime without treating live work as crash debris.
    pub fn reconcile(&self, run_id: &str) -> Result<RunSnapshot, String> {
        let snapshot = self.snapshot(run_id)?;
        StoryStore::open()?.reconcile_integrated_dependencies(&snapshot.plan_id)?;
        self.snapshot(run_id)
    }

    /// Recovery only: old effects are uncertain and old attempts interrupted.
    pub(super) fn reconcile_after_restart(&self, run_id: &str) -> Result<RunSnapshot, String> {
        let snapshot = self.snapshot(run_id)?;
        if matches!(snapshot.status, RunStatus::Completed | RunStatus::Cancelled) {
            return Ok(snapshot);
        }
        for effect in snapshot
            .effects
            .iter()
            .filter(|effect| effect.state == EffectState::Intended)
        {
            self.append_reconcile_event(
                run_id,
                &format!("reconcile-effect:{}", effect.id),
                RunEventKind::EffectChanged {
                    effect_id: effect.id.clone(),
                    state: EffectState::Uncertain,
                },
            )?;
        }
        for attempt in snapshot
            .attempts
            .iter()
            .filter(|attempt| attempt.state == AttemptState::Running)
        {
            self.append_reconcile_event(
                run_id,
                &format!("reconcile-attempt:{}", attempt.id),
                RunEventKind::AttemptInterrupted {
                    attempt_id: attempt.id.clone(),
                },
            )?;
        }
        if self.snapshot(run_id)?.status == RunStatus::Running {
            self.append_reconcile_event(
                run_id,
                &format!("reconcile-run:{run_id}"),
                RunEventKind::Paused,
            )?;
        }
        self.snapshot(run_id)
    }

    pub(super) fn active_run_ids(&self) -> Result<Vec<String>, String> {
        let conn = self.connect()?;
        let mut stmt = conn
            .prepare("SELECT id FROM workflow_runs WHERE status IN ('running','paused')")
            .map_err(|e| format!("prepare active workflow runs: {e}"))?;
        let run_ids: Vec<String> = stmt
            .query_map([], |row| row.get(0))
            .map_err(|e| format!("read active workflow runs: {e}"))?
            .map(|row| row.map_err(|e| format!("read active workflow run: {e}")))
            .collect::<Result<_, _>>()?;
        drop(stmt);
        drop(conn);
        Ok(run_ids)
    }

    #[cfg(test)]
    pub(super) fn reconcile_active_after_restart(&self) -> Result<usize, String> {
        Ok(self.reconcile_runs_after_restart(&self.active_run_ids()?))
    }

    fn reconcile_run_after_restart(&self, run_id: &str) -> Result<(), String> {
        let snapshot = self.reconcile_after_restart(run_id)?;
        StoryStore::open()?.reconcile_integrated_dependencies(&snapshot.plan_id)
    }

    pub(super) fn reconcile_runs_after_restart(&self, run_ids: &[String]) -> usize {
        let mut recovered = 0;
        for run_id in run_ids {
            match self.reconcile_run_after_restart(run_id) {
                Ok(()) => recovered += 1,
                Err(error) => {
                    tracing::warn!(
                        source = "workflows", run_id = %run_id, %error,
                        "Workflow restart recovery failed for this run; continuing other runs"
                    );
                }
            }
        }
        recovered
    }

    fn append_reconcile_event(
        &self,
        run_id: &str,
        command_id: &str,
        kind: RunEventKind,
    ) -> Result<(), String> {
        let mut conn = self.connect()?;
        let tx = conn
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .map_err(|e| format!("begin workflow reconcile: {e}"))?;
        if read_command_receipt(&tx, run_id, command_id)?.is_none() {
            let snapshot = read_snapshot(&tx, run_id)?;
            persist_event(&tx, snapshot, command_id, None, now_ms(), kind)?;
        }
        tx.commit()
            .map_err(|e| format!("commit workflow reconcile: {e}"))?;
        Ok(())
    }
}

/// One authority for dependency release, dispatch, and final verification.
/// A prior story receipt remains usable after a later *recorded and checked*
/// integration on the same ref; an unrecorded ref or tree movement fails closed.
#[cfg(test)]
pub fn story_integrated_at_revision(story_id: &str, revision: i64) -> Result<bool, String> {
    story_integrated_at_revision_in(
        &crate::config::config_dir().join("workflow_runs.sqlite3"),
        story_id,
        revision,
    )
}

/// Persisted cursors used to reject a receipt preflight invalidated by a run write.
pub(crate) fn plan_run_sequences_in(
    db_path: &Path,
    plan_id: &str,
) -> Result<Vec<(String, i64)>, String> {
    if !db_path.exists() {
        return Ok(Vec::new());
    }
    let conn = Connection::open_with_flags(db_path, rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY)
        .map_err(|error| format!("open workflow run cursors: {error}"))?;
    let has_table: bool = conn.query_row(
        "SELECT EXISTS(SELECT 1 FROM sqlite_master WHERE type='table' AND name='workflow_runs')",
        [], |row| row.get(0),
    ).map_err(|error| format!("read workflow run schema: {error}"))?;
    if !has_table {
        return Ok(Vec::new());
    }
    let mut stmt = conn
        .prepare("SELECT id,snapshot_json FROM workflow_runs WHERE plan_id=?1 ORDER BY id")
        .map_err(|error| format!("prepare workflow run cursors: {error}"))?;
    let rows = stmt
        .query_map([plan_id], |row| {
            Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
        })
        .map_err(|error| format!("read workflow run cursors: {error}"))?;
    rows.map(|row| {
        let (id, json) = row.map_err(|error| format!("read workflow run cursor: {error}"))?;
        let snapshot: RunSnapshot = decode(&json)?;
        Ok((id, snapshot.sequence))
    })
    .collect()
}

pub(crate) fn plan_has_workflow_run_in(db_path: &Path, plan_id: &str) -> Result<bool, String> {
    if !db_path.exists() {
        return Ok(false);
    }
    let conn = Connection::open_with_flags(db_path, rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY)
        .map_err(|error| format!("open workflow run store: {error}"))?;
    // A store file without the schema holds no run; only `RunStore::open_at` creates it.
    let has_table: bool = conn
        .query_row(
            "SELECT EXISTS(SELECT 1 FROM sqlite_master WHERE type='table' AND name='workflow_runs')",
            [],
            |row| row.get(0),
        )
        .map_err(|error| format!("read workflow run schema: {error}"))?;
    if !has_table {
        return Ok(false);
    }
    conn.query_row(
        "SELECT EXISTS(SELECT 1 FROM workflow_runs WHERE plan_id=?1)",
        [plan_id],
        |row| row.get(0),
    )
    .map_err(|error| format!("read workflow run ownership: {error}"))
}

pub(crate) fn story_integrated_at_revision_in(
    db_path: &Path,
    story_id: &str,
    revision: i64,
) -> Result<bool, String> {
    let store = RunStore::open_at(db_path)?;
    let conn = store.connect()?;
    let mut stmt = conn.prepare(
        "SELECT r.snapshot_json FROM workflow_runs r JOIN workflow_story_executions s ON s.run_id=r.id WHERE s.story_id=?1 ORDER BY r.rowid DESC"
    ).map_err(|error| format!("prepare integration receipts: {error}"))?;
    let rows = stmt
        .query_map([story_id], |row| row.get::<_, String>(0))
        .map_err(|error| format!("read integration receipts: {error}"))?;
    for row in rows {
        let snapshot: RunSnapshot =
            decode(&row.map_err(|error| format!("read integration receipt: {error}"))?)?;
        if receipt_current(&snapshot, story_id, revision)? {
            return Ok(true);
        }
    }
    Ok(false)
}

/// Archive history is the only revision change that does not change accepted work.
fn story_work_revision_current(
    story_id: &str,
    accepted: i64,
    current: i64,
) -> Result<bool, String> {
    if accepted == current {
        return Ok(true);
    }
    StoryStore::open()?.archive_only_since(story_id, accepted, current)
}

pub(super) fn receipt_current(
    snapshot: &RunSnapshot,
    story_id: &str,
    revision: i64,
) -> Result<bool, String> {
    let Some(execution) = snapshot
        .stories
        .iter()
        .find(|item| item.story_id == story_id)
    else {
        return Ok(false);
    };
    let Some(receipt) = &execution.integration_receipt else {
        return Ok(false);
    };
    if !execution.accepted
        || execution.accepted_revision != Some(receipt.story_revision)
        || !story_work_revision_current(story_id, receipt.story_revision, revision)?
    {
        return Ok(false);
    }
    let canonical = Path::new(&snapshot.project);
    let Ok(current_ref) = git_output(canonical, &["symbolic-ref", "HEAD"]) else {
        return Ok(false);
    };
    let Ok((head, tree)) = clean_artifact(canonical) else {
        return Ok(false);
    };
    let current_integration = snapshot
        .stories
        .iter()
        .filter_map(|item| item.integration_receipt.as_ref())
        .find(|item| {
            item.canonical_ref == current_ref
                && item.merge_commit == head
                && item.merge_tree == tree
                && !item.post_checks.is_empty()
                && item.post_checks.iter().all(|check| {
                    check.exit_code == 0
                        && check.ref_name == current_ref
                        && check.commit == head
                        && check.tree == tree
                })
        });
    let current_recertification = snapshot
        .canonical_recertification
        .as_ref()
        .is_some_and(|item| {
            item.canonical_ref == current_ref
                && item.commit == head
                && item.tree == tree
                && !item.post_checks.is_empty()
                && item.post_checks.iter().all(|check| {
                    check.exit_code == 0
                        && check.ref_name == current_ref
                        && check.commit == head
                        && check.tree == tree
                })
        });
    if (current_integration.is_none() && !current_recertification)
        || receipt.canonical_ref != current_ref
        || snapshot.canonical_ref.as_deref() != Some(current_ref.as_str())
    {
        return Ok(false);
    }
    source_is_ancestor(canonical, &receipt.source_commit, &head)
}

fn source_is_ancestor(canonical: &Path, source: &str, head: &str) -> Result<bool, String> {
    let ancestor = Command::new("git")
        .args(["merge-base", "--is-ancestor", source, head])
        .current_dir(canonical)
        .status()
        .map_err(|error| format!("verify integrated source ancestry: {error}"))?;
    Ok(ancestor.success())
}

/// An integration with no pinned checks would record an empty `post_checks`
/// list and release dependents without validating anything.
pub(super) fn require_nonempty_policy(definition: &PublishedWorkflow) -> Result<(), String> {
    if definition.required_checks.is_empty() {
        return Err(format!(
            "story workflow {} revision {} pins no required checks; publish a revision with at least one check",
            definition.name, definition.revision
        ));
    }
    Ok(())
}

pub(super) fn require_current_checks(
    required: &[CheckDefinition],
    receipts: &[CheckReceipt],
    ref_name: &str,
    commit: &str,
    tree: &str,
) -> Result<(), String> {
    for check in required {
        if !receipts.iter().any(|receipt| {
            receipt.check_id == check.id
                && receipt.argv == check.argv
                && receipt.exit_code == 0
                && receipt.ref_name == ref_name
                && receipt.commit == commit
                && receipt.tree == tree
        }) {
            return Err(format!(
                "required check {} is missing, failed, or stale",
                check.id
            ));
        }
    }
    Ok(())
}

pub(super) fn now_ms() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis()
        .min(i64::MAX as u128) as i64
}

fn validate_key(label: &str, value: &str) -> Result<(), String> {
    if value.trim().is_empty() || value.len() > 128 || value.contains('\0') {
        return Err(format!("invalid {label}"));
    }
    Ok(())
}

fn encode<T: Serialize>(value: &T) -> Result<String, String> {
    serde_json::to_string(value).map_err(|e| format!("encode workflow run: {e}"))
}
fn decode<T: DeserializeOwned>(raw: &str) -> Result<T, String> {
    serde_json::from_str(raw).map_err(|e| format!("decode workflow run: {e}"))
}

fn read_snapshot(conn: &Connection, run_id: &str) -> Result<RunSnapshot, String> {
    let raw: String = conn
        .query_row(
            "SELECT snapshot_json FROM workflow_runs WHERE id=?1",
            [run_id],
            |row| row.get(0),
        )
        .optional()
        .map_err(|e| format!("read workflow run: {e}"))?
        .ok_or("workflow run not found")?;
    let snapshot: RunSnapshot = decode(&raw)?;
    // Older snapshots lack timing fields, but their existing ledger owns pause times.
    if !raw.contains("\"pausedDurationMs\"") {
        return replay_events(conn, run_id);
    }
    Ok(snapshot)
}

fn read_command_receipt(
    conn: &Connection,
    run_id: &str,
    command_id: &str,
) -> Result<Option<RunReceipt>, String> {
    let raw: Option<String> = conn
        .query_row(
            "SELECT receipt_json FROM workflow_events WHERE run_id=?1 AND command_id=?2",
            params![run_id, command_id],
            |row| row.get(0),
        )
        .optional()
        .map_err(|e| format!("read workflow command receipt: {e}"))?;
    raw.map(|raw| decode(&raw)).transpose()
}

/// The unique index admits the run; translate only its refusal, while the
/// immediate transaction still holds the writer lock and the active id is stable.
fn insert_started_run(
    conn: &Connection,
    snapshot: &RunSnapshot,
    context: &str,
) -> Result<(), RunStartError> {
    let result = conn.execute(
        "INSERT INTO workflow_runs(id,project,plan_id,status,snapshot_json) VALUES (?1,?2,?3,?4,?5)",
        params![snapshot.id, snapshot.project, snapshot.plan_id, snapshot.status.as_str(), encode(snapshot)?],
    );
    if let Err(error) = result {
        if matches!(&error, rusqlite::Error::SqliteFailure(code, _)
            if code.extended_code == rusqlite::ffi::SQLITE_CONSTRAINT_UNIQUE)
        {
            let active_run_id: Option<String> = conn.query_row(
                "SELECT id FROM workflow_runs WHERE project=?1 AND plan_id=?2 AND status IN ('running','paused')",
                params![snapshot.project, snapshot.plan_id], |row| row.get(0),
            ).optional().map_err(|e| format!("read active workflow run: {e}"))?;
            if let Some(active_run_id) = active_run_id {
                return Err(RunStartError::AlreadyActive {
                    plan_id: snapshot.plan_id.clone(),
                    active_run_id,
                });
            }
        }
        return Err(RunStartError::Other(format!("{context}: {error}")));
    }
    Ok(())
}

fn insert_event(conn: &Connection, run_id: &str, receipt: &RunReceipt) -> Result<(), String> {
    conn.execute("INSERT INTO workflow_events(run_id,sequence,command_id,event_json,receipt_json) VALUES (?1,?2,?3,?4,?5)",
        params![run_id, receipt.sequence, receipt.event.command_id, encode(&receipt.event)?, encode(receipt)?])
        .map_err(|e| format!("append workflow event: {e}"))?;
    Ok(())
}

fn persist_event(
    conn: &Connection,
    previous: RunSnapshot,
    command_id: &str,
    command_hash: Option<String>,
    at_ms: i64,
    kind: RunEventKind,
) -> Result<RunReceipt, String> {
    let event = RunEvent {
        sequence: previous.sequence + 1,
        command_id: command_id.into(),
        command_hash,
        at_ms,
        kind,
    };
    let snapshot = apply_event(Some(previous.clone()), &event)?;
    let receipt = RunReceipt {
        sequence: event.sequence,
        event,
        snapshot: snapshot.clone(),
    };
    // Compare the actual stored encoding: legacy snapshots omit additive fields
    // and cannot be compared to a freshly serialized projection byte-for-byte.
    let stored_json: String = conn
        .query_row(
            "SELECT snapshot_json FROM workflow_runs WHERE id=?1",
            [&snapshot.id],
            |row| row.get(0),
        )
        .map_err(|error| format!("read workflow projection encoding: {error}"))?;
    if read_snapshot(conn, &snapshot.id)? != previous {
        return Err("workflow projection changed concurrently".into());
    }
    insert_event(conn, &snapshot.id, &receipt)?;
    let changed = conn
        .execute(
            "UPDATE workflow_runs SET status=?1,snapshot_json=?2 WHERE id=?3 AND snapshot_json=?4",
            params![
                snapshot.status.as_str(),
                encode(&snapshot)?,
                snapshot.id,
                stored_json
            ],
        )
        .map_err(|e| format!("update workflow projection: {e}"))?;
    if changed != 1 {
        return Err("workflow projection changed concurrently".into());
    }
    replace_projections(conn, &snapshot)?;
    Ok(receipt)
}

fn replace_projections(conn: &Connection, snapshot: &RunSnapshot) -> Result<(), String> {
    conn.execute(
        "DELETE FROM workflow_story_executions WHERE run_id=?1",
        [&snapshot.id],
    )
    .map_err(|e| format!("refresh story executions: {e}"))?;
    conn.execute(
        "DELETE FROM workflow_node_attempts WHERE run_id=?1",
        [&snapshot.id],
    )
    .map_err(|e| format!("refresh node attempts: {e}"))?;
    conn.execute(
        "DELETE FROM workflow_effects WHERE run_id=?1",
        [&snapshot.id],
    )
    .map_err(|e| format!("refresh effect intents: {e}"))?;
    for story in &snapshot.stories {
        conn.execute("INSERT INTO workflow_story_executions(run_id,story_id,document_json) VALUES (?1,?2,?3)",
            params![snapshot.id, story.story_id, encode(story)?]).map_err(|e| format!("project story execution: {e}"))?;
    }
    for attempt in &snapshot.attempts {
        conn.execute(
            "INSERT INTO workflow_node_attempts(run_id,attempt_id,document_json) VALUES (?1,?2,?3)",
            params![snapshot.id, attempt.id, encode(attempt)?],
        )
        .map_err(|e| format!("project node attempt: {e}"))?;
    }
    for effect in &snapshot.effects {
        conn.execute("INSERT INTO workflow_effects(run_id,effect_id,effect_key,document_json) VALUES (?1,?2,?3,?4)",
            params![snapshot.id, effect.id, effect.key, encode(effect)?]).map_err(|e| format!("project effect intent: {e}"))?;
    }
    Ok(())
}

fn command_retry(receipt: RunReceipt, hash: &str) -> Result<RunReceipt, String> {
    if receipt.event.command_hash.as_deref() != Some(hash) {
        return Err("workflow command id was reused with a different payload".into());
    }
    Ok(receipt)
}

fn validate_preflight(
    conn: &Connection,
    snapshot: &RunSnapshot,
    revisions: &str,
) -> Result<(), String> {
    if read_snapshot(conn, &snapshot.id)?.sequence != snapshot.sequence
        || fingerprint(&snapshot.plan_id, &plan_stories(snapshot)?, true)? != revisions
    {
        return Err("workflow or story revisions changed during command preflight; retry".into());
    }
    Ok(())
}

pub(super) fn fingerprint(
    plan_id: &str,
    stories: &[Story],
    include_execution: bool,
) -> Result<String, String> {
    let mut sorted = stories.to_vec();
    sorted.sort_by(|a, b| a.id.cmp(&b.id));
    let shape: Vec<_> = sorted
        .iter()
        .map(|story| {
            if include_execution {
                serde_json::json!([story.id, story.revision, story.status, story.checked])
            } else {
                serde_json::json!([
                    story.id,
                    story.title,
                    story.criteria,
                    story.dependencies,
                    story.priority,
                    story.origin,
                    story.file_scope,
                    story.problem,
                    story.risk,
                    story.story_type
                ])
            }
        })
        .collect();
    // Plan metadata is a prompt and verification subject even when the plan has no stories.
    let plan = StoryStore::open()?.get_plan(plan_id)?;
    let subject = serde_json::json!([plan.title, plan.source, shape]);
    let bytes =
        serde_json::to_vec(&subject).map_err(|e| format!("encode plan fingerprint: {e}"))?;
    Ok(hex::encode(Sha256::digest(bytes)))
}

pub(in crate::workflows::run) fn plan_stories(
    snapshot: &RunSnapshot,
) -> Result<Vec<Story>, String> {
    Ok(StoryStore::open()?
        .list_stories(&snapshot.plan_id)?
        .into_iter()
        .filter(|story| !story.archived)
        .collect())
}

/// Unknown, globbed or overlapping scopes cannot safely share a work wave.
fn scopes_may_overlap(left: &[String], right: &[String]) -> bool {
    fn normalized(path: &str) -> Option<String> {
        let path = path.replace('\\', "/").to_ascii_lowercase();
        if path.is_empty()
            || path.starts_with('/')
            || path.starts_with('~')
            || path.contains(':')
            || path
                .chars()
                .any(|ch| matches!(ch, '*' | '?' | '[' | ']' | '{' | '}'))
            || path
                .split('/')
                .any(|part| part.is_empty() || matches!(part, "." | ".."))
        {
            return None;
        }
        Some(path)
    }
    if left.is_empty() || right.is_empty() {
        return true;
    }
    left.iter().any(|item| {
        right.iter().any(|other| {
            let (Some(a), Some(b)) = (normalized(item), normalized(other)) else {
                return true;
            };
            a == b || a.starts_with(&format!("{b}/")) || b.starts_with(&format!("{a}/"))
        })
    })
}

pub(super) fn ready_to_verify(snapshot: &RunSnapshot, stories: &[Story]) -> Result<(), String> {
    if snapshot.planning_fingerprint.is_none() {
        return Err("planning is still open".into());
    }
    if snapshot.attempts.iter().any(|attempt| {
        attempt.state == AttemptState::Running
            || (attempt.outcome == Some(AttemptOutcome::NeedsInput)
                && attempt.input_answer.is_none())
    }) || snapshot.effects.iter().any(|effect| {
        effect.state == EffectState::Intended || effect.state == EffectState::Uncertain
    }) {
        return Err("workflow has active or uncertain work".into());
    }
    for story in stories {
        if story.status != StoryStatus::Done
            || !receipt_current(snapshot, &story.id, story.revision)?
        {
            return Err("not all plan stories have current integration receipts at their accepted revisions".into());
        }
    }
    Ok(())
}

fn check_receipt_retry(
    prior: RunReceipt,
    story_id: &str,
    check_id: &str,
    expected_sequence: i64,
) -> Result<RunReceipt, String> {
    let RunEventKind::CheckRecorded {
        story_id: prior_story,
        receipt,
    } = &prior.event.kind
    else {
        return Err("workflow command id was reused with a different payload".into());
    };
    let command = RunCommand::RecordCheck {
        story_id: story_id.into(),
        receipt: receipt.clone(),
    };
    if prior_story != story_id
        || receipt.check_id != check_id
        || prior.event.command_hash.as_deref()
            != Some(command_hash(Some(expected_sequence), &command)?.as_str())
    {
        return Err("workflow command id was reused with a different payload".into());
    }
    Ok(prior)
}

fn command_hash(expected_sequence: Option<i64>, command: &RunCommand) -> Result<String, String> {
    Ok(hex::encode(Sha256::digest(
        encode(&(expected_sequence, command))?.as_bytes(),
    )))
}

fn choose_event(
    snapshot: &RunSnapshot,
    command: RunCommand,
    at_ms: i64,
) -> Result<RunEventKind, String> {
    if matches!(snapshot.status, RunStatus::Completed | RunStatus::Cancelled)
        && !matches!(command, RunCommand::RecordRecertification { .. })
    {
        return Err("terminal workflow cannot advance".into());
    }
    if snapshot.event_contract_version == 0 && !matches!(command, RunCommand::Cancel) {
        return Err("legacy workflow runs support inspect and cancel only; start a new run".into());
    }
    if snapshot.event_contract_version > super::graph::RUN_EVENT_CONTRACT_VERSION {
        return Err("unsupported workflow event contract".into());
    }
    if !snapshot.graph_executions.is_empty()
        && matches!(
            command,
            RunCommand::StartAttempt { .. }
                | RunCommand::StartPlanAgent { .. }
                | RunCommand::AdvanceLoop
        )
    {
        return Err("graph runs require an eligible typed activation transition".into());
    }
    let expired = at_ms >= super::runtime::deadline_at(snapshot, at_ms);
    if matches!(command, RunCommand::ExpireDeadline) {
        if !expired || snapshot.status != RunStatus::Running {
            return Err("workflow deadline is not due".into());
        }
        return Ok(RunEventKind::DeadlineExpired {
            deadline_ms: super::runtime::deadline_at(snapshot, at_ms),
        });
    }
    if expired
        && matches!(
            command,
            RunCommand::Resume
                | RunCommand::ResumeGraph { .. }
                | RunCommand::Graph {
                    transition: super::graph::GraphTransition::ResolvePause { .. }
                }
        )
    {
        return Err("workflow deadline expired; cancel and start a new run".into());
    }
    if expired
        && !matches!(
            command,
            RunCommand::Cancel
                | RunCommand::Pause
                | RunCommand::PauseFor { .. }
                | RunCommand::BindAgent { .. }
                | RunCommand::MarkEffect { .. }
                | RunCommand::ResolveUncertainEffect { .. }
                | RunCommand::ReportAttempt { .. }
                | RunCommand::ReportBoundAttempt { .. }
                | RunCommand::AnswerInput { .. }
                | RunCommand::RecordRecertification { .. }
        )
    {
        return Ok(RunEventKind::Paused);
    }
    let stories = plan_stories(snapshot)?;
    let current_plan = fingerprint(&snapshot.plan_id, &stories, false)?;
    if snapshot
        .planning_fingerprint
        .as_deref()
        .is_some_and(|closed| closed != current_plan)
        && !matches!(
            command,
            RunCommand::Cancel
                | RunCommand::Pause
                | RunCommand::PauseFor { .. }
                | RunCommand::BindAgent { .. }
                | RunCommand::MarkEffect { .. }
                | RunCommand::ResolveUncertainEffect { .. }
                | RunCommand::ReportAttempt { .. }
                | RunCommand::ReportBoundAttempt { .. }
                | RunCommand::AnswerInput { .. }
                | RunCommand::RecordRecertification { .. }
        )
    {
        return Ok(RunEventKind::PlanningReopened);
    }
    if snapshot.status == RunStatus::Paused
        && !matches!(
            command,
            RunCommand::Resume
                | RunCommand::ResumeGraph { .. }
                | RunCommand::Graph {
                    transition: super::graph::GraphTransition::ResolvePause { .. }
                }
                | RunCommand::Cancel
                | RunCommand::ResolveUncertainEffect { .. }
                | RunCommand::BindAgent { .. }
                | RunCommand::MarkEffect { .. }
                | RunCommand::ReportAttempt { .. }
                | RunCommand::ReportBoundAttempt { .. }
                | RunCommand::AnswerInput { .. }
                | RunCommand::RecordRecertification { .. }
        )
    {
        return Err("workflow is paused".into());
    }
    match command {
        RunCommand::Graph { transition } => {
            if matches!(
                transition,
                super::graph::GraphTransition::ResolvePause { .. }
            ) {
                super::reducer::validate_resume(snapshot)?;
            }
            let event = if let super::graph::GraphTransition::Start {
                execution_id,
                target_id,
            } = &transition
            {
                let (id, revision) = if *target_id == snapshot.plan_id {
                    (&snapshot.definition_id, snapshot.definition_revision)
                } else {
                    let story = stories
                        .iter()
                        .find(|story| story.id == *target_id)
                        .ok_or("graph target does not belong to run plan")?;
                    if matches!(snapshot.root_target, Some(RunTarget::Plan(_)))
                        && story.status != StoryStatus::Ready
                    {
                        return Err("only a ready story can dispatch".into());
                    }
                    for dependency in &story.dependencies {
                        let dependency = stories
                            .iter()
                            .find(|s| s.id == *dependency)
                            .ok_or("dependency is not in plan")?;
                        if dependency.status != StoryStatus::Done
                            || !receipt_current(snapshot, &dependency.id, dependency.revision)?
                        {
                            return Err(
                                "dependent story dispatch requires a current integration receipt"
                                    .into(),
                            );
                        }
                    }
                    if matches!(snapshot.root_target, Some(RunTarget::Plan(_))) {
                        require_dispatch_slot(&RunStore::open()?.connect()?, snapshot, story)?;
                    }
                    (
                        &snapshot.story_definition_id,
                        snapshot.story_definition_revision,
                    )
                };
                let definition = WorkflowStore::open()?.get_published(id, revision)?;
                if definition.project != snapshot.project {
                    return Err("graph definition does not belong to run project".into());
                }
                super::graph::GraphEvent::Started {
                    execution: Box::new(super::graph::GraphExecution::start(
                        execution_id.clone(),
                        target_id.clone(),
                        definition,
                    )?),
                }
            } else {
                super::graph::GraphEvent::Transition { transition }
            };
            Ok(RunEventKind::Graph { event })
        }
        RunCommand::ClosePlanning => Ok(RunEventKind::PlanningClosed {
            fingerprint: current_plan,
        }),
        RunCommand::AssignWorktree { story_id, path } => {
            let story = snapshot
                .stories
                .iter()
                .find(|story| story.story_id == story_id)
                .ok_or("story execution not found")?;
            if story.worktree_path.is_some() {
                return Err("story worktree already assigned".into());
            }
            if !snapshot.attempts.iter().any(|attempt| {
                attempt.story_id == story_id && attempt.state == AttemptState::Running
            }) {
                return Err("story has no running attempt".into());
            }
            if snapshot
                .stories
                .iter()
                .any(|story| story.worktree_path.as_deref() == Some(&path))
            {
                return Err("worktree is assigned to another story".into());
            }
            let canonical = Path::new(&path)
                .canonicalize()
                .map_err(|error| format!("resolve story worktree: {error}"))?;
            if canonical.to_string_lossy() != path || path == snapshot.project {
                return Err("story worktree must be a canonical isolated path".into());
            }
            let registered = crate::worktree::get_worktree_paths_raw(&snapshot.project)?;
            if !registered.values().any(|worktree| {
                tuic_core::path_spelling::portable_spelling(&worktree.path)
                    == tuic_core::path_spelling::portable_spelling(&path)
            }) {
                return Err("story worktree is not a registered checkout".into());
            }
            Ok(RunEventKind::WorktreeAssigned { story_id, path })
        }
        RunCommand::StartPlanAgent { node_id } => choose_plan_attempt(snapshot, node_id),
        RunCommand::StartAttempt { story_id, node_id } => {
            choose_story_attempt(snapshot, &stories, story_id, node_id)
        }
        RunCommand::StartGraphAgent {
            execution_id,
            activation_id,
        } => {
            let graph = snapshot
                .graph_executions
                .iter()
                .find(|g| g.id == execution_id)
                .ok_or("graph execution not found")?;
            let activation = graph
                .activations
                .iter()
                .find(|a| {
                    a.id == activation_id && a.state == super::graph::ActivationState::Running
                })
                .ok_or("Agent activation is not running")?;
            if graph.definition.kind == WorkflowKind::Plan {
                let node_id = super::runtime::effects::agent_node_id(graph, activation)?;
                if super::runtime::effects::activation_attempt(snapshot, graph, activation)
                    .is_some()
                {
                    return Err("plan activation already has an attempt".into());
                }
                return choose_plan_attempt(snapshot, node_id.to_owned());
            }
            let reached = graph
                .activations
                .iter()
                .take_while(|a| a.id != activation.id)
                .filter(|a| a.node_id == activation.node_id)
                .count();
            let attempts = snapshot
                .attempts
                .iter()
                .filter(|a| a.story_id == graph.target_id && a.node_id == activation.node_id)
                .count();
            if attempts != reached {
                return Err("Agent activation already has an attempt".into());
            }
            choose_story_attempt(
                snapshot,
                &stories,
                graph.target_id.clone(),
                activation.node_id.clone(),
            )
        }
        RunCommand::ReportAttempt {
            attempt_id,
            generation,
            outcome,
        } => {
            let attempt = snapshot
                .attempts
                .iter()
                .find(|attempt| attempt.id == attempt_id)
                .ok_or("node attempt not found")?;
            if attempt.agent.is_some() {
                return Err("bound agent must use a typed attempt report".into());
            }
            if expired || attempt.generation != generation || attempt.state != AttemptState::Running
            {
                Ok(RunEventKind::LateReportIgnored {
                    attempt_id,
                    generation,
                })
            } else {
                Ok(RunEventKind::AttemptReported {
                    attempt_id,
                    generation,
                    outcome,
                    report: None,
                })
            }
        }
        RunCommand::BindAgent {
            attempt_id,
            binding,
        } => {
            validate_key("agent session", &binding.session_id)?;
            validate_key("spawn effect", &binding.effect_id)?;
            if binding.prompt_contract_version != crate::workflows::PROMPT_CONTRACT_VERSION
                || binding.prompt_sha256.len() != 64
                || !binding
                    .prompt_sha256
                    .bytes()
                    .all(|byte| byte.is_ascii_hexdigit())
                || binding.audit_preview.chars().count() > 2_000
                || binding
                    .task_id
                    .as_ref()
                    .is_some_and(|id| validate_key("task id", id).is_err())
            {
                return Err("invalid agent binding".into());
            }
            let attempt = snapshot
                .attempts
                .iter()
                .find(|attempt| attempt.id == attempt_id)
                .ok_or("node attempt not found")?;
            if attempt.state != AttemptState::Running || attempt.agent.is_some() {
                return Err("node attempt cannot bind an agent".into());
            }
            if snapshot.attempts.iter().any(|existing| {
                existing
                    .agent
                    .as_ref()
                    .is_some_and(|agent| agent.session_id == binding.session_id)
            }) {
                return Err("agent session is already bound to a workflow attempt".into());
            }
            let effect = snapshot
                .effects
                .iter()
                .find(|effect| effect.id == binding.effect_id)
                .ok_or("spawn effect not found")?;
            if effect.kind != EffectKind::SpawnAgent
                || effect.state != EffectState::Intended
                || effect.key != format!("spawn:{attempt_id}")
            {
                return Err("spawn effect is not intended".into());
            }
            Ok(RunEventKind::AgentBound {
                attempt_id,
                binding,
            })
        }
        RunCommand::ReportBoundAttempt {
            caller_session,
            report,
        } => {
            validate_key("agent session", &caller_session)?;
            validate_key("run id", &report.run_id)?;
            validate_key("story id", &report.story_id)?;
            validate_key("attempt id", &report.attempt_id)?;
            if report.contract_version != crate::workflows::PROMPT_CONTRACT_VERSION
                || report.run_id != snapshot.id
                || report.generation == 0
                || report.story_revision < 0
                || report.summary.trim().is_empty()
                || report.summary.len() > 4_000
                || report.evidence.len() > 32
                || report
                    .evidence
                    .iter()
                    .any(|item| item.trim().is_empty() || item.len() > 2_000)
                || report.criterion_results.len() > 100
                || report
                    .criterion_results
                    .iter()
                    .any(|item| item.evidence.trim().is_empty() || item.evidence.len() > 2_000)
                || (report.outcome == AttemptOutcome::NeedsInput) != report.input_request.is_some()
                || report.input_request.as_ref().is_some_and(|request| {
                    request.question.trim().is_empty()
                        || request.question.len() > 2_000
                        || request.options.len() > 16
                        || request
                            .options
                            .iter()
                            .any(|option| option.trim().is_empty() || option.len() > 256)
                })
            {
                return Err("invalid typed attempt report".into());
            }
            let attempt = snapshot
                .attempts
                .iter()
                .find(|attempt| attempt.id == report.attempt_id)
                .ok_or("node attempt not found")?;
            if attempt.story_id != report.story_id
                || attempt
                    .agent
                    .as_ref()
                    .map(|agent| agent.session_id.as_str())
                    != Some(caller_session.as_str())
            {
                return Err("agent does not own the attempt".into());
            }
            let definition = if report.story_id == snapshot.plan_id {
                WorkflowStore::open()?
                    .get_published(&snapshot.definition_id, snapshot.definition_revision)?
            } else {
                WorkflowStore::open()?.get_published(
                    &snapshot.story_definition_id,
                    snapshot.story_definition_revision,
                )?
            };
            let reviewer = definition.graph.nodes.iter().any(|node| {
                node.id == attempt.node_id
                    && matches!(
                        node.kind,
                        NodeKind::Agent {
                            role: crate::workflows::AgentRole::Reviewer,
                            ..
                        }
                    )
            });
            if (reviewer && report.outcome == AttemptOutcome::Completed) != report.review.is_some()
            {
                return Err("completed reviewer reports require review evidence".into());
            }
            let mut seen = std::collections::HashSet::new();
            if report.story_id == snapshot.plan_id {
                if report.story_revision != 0 || !report.criterion_results.is_empty() {
                    return Err("plan agent report cannot contain story criteria".into());
                }
            } else {
                let story = stories
                    .iter()
                    .find(|story| story.id == report.story_id)
                    .ok_or("story is not in plan")?;
                if story.revision != report.story_revision {
                    return Err("attempt report targets a stale story revision".into());
                }
                if report
                    .criterion_results
                    .iter()
                    .any(|item| item.index >= story.criteria.len() || !seen.insert(item.index))
                {
                    return Err("invalid criterion result index".into());
                }
                if let Some(review) = &report.review
                    && (review.artifact_digest.len() != 64
                        || !review
                            .artifact_digest
                            .bytes()
                            .all(|byte| byte.is_ascii_hexdigit())
                        || review.findings.len() > 100
                        || (review.decision == ReviewDecision::Approved
                            && !review.findings.is_empty())
                        || (review.decision == ReviewDecision::ChangesRequested
                            && review.findings.is_empty())
                        || review.findings.iter().any(|finding| {
                            finding.criterion_index >= story.criteria.len()
                                || finding.summary.trim().is_empty()
                                || finding.summary.len() > 1_000
                                || finding.evidence.trim().is_empty()
                                || finding.evidence.len() > 2_000
                        }))
                {
                    return Err("invalid review evidence".into());
                }
            }
            if expired
                || attempt.generation != report.generation
                || attempt.state != AttemptState::Running
            {
                return Ok(RunEventKind::LateReportIgnored {
                    attempt_id: report.attempt_id,
                    generation: report.generation,
                });
            }
            Ok(RunEventKind::AttemptReported {
                attempt_id: report.attempt_id.clone(),
                generation: report.generation,
                outcome: report.outcome,
                report: Some(report),
            })
        }
        RunCommand::ReserveEffect { key, kind } => {
            validate_key("effect key", &key)?;
            if snapshot.effects.iter().any(|effect| effect.key == key) {
                return Err("effect key already reserved".into());
            }
            if kind == EffectKind::SpawnAgent && snapshot.spawns >= snapshot.limits.max_spawns
                || kind == EffectKind::CreateStory
                    && snapshot.story_creations >= snapshot.limits.max_story_creations
            {
                return Err("workflow effect budget exhausted".into());
            }
            Ok(RunEventKind::EffectReserved {
                effect: EffectIntent {
                    id: Uuid::now_v7().to_string(),
                    key,
                    kind,
                    state: EffectState::Intended,
                },
            })
        }
        RunCommand::AnswerInput { attempt_id, answer } => {
            validate_key("attempt id", &attempt_id)?;
            if snapshot.status != RunStatus::Paused {
                return Err("workflow is not paused for input".into());
            }
            if answer.trim().is_empty() || answer.len() > 4_000 {
                return Err("invalid input answer".into());
            }
            let attempt = snapshot
                .attempts
                .iter()
                .find(|attempt| attempt.id == attempt_id)
                .ok_or("node attempt not found")?;
            if attempt.outcome != Some(AttemptOutcome::NeedsInput)
                || attempt.input_answer.is_some()
                || attempt
                    .report
                    .as_ref()
                    .and_then(|report| report.input_request.as_ref())
                    .is_none()
            {
                return Err("attempt has no pending input request".into());
            }
            Ok(RunEventKind::InputAnswered { attempt_id, answer })
        }
        RunCommand::MarkEffect {
            effect_id,
            succeeded,
        } => {
            let effect = snapshot
                .effects
                .iter()
                .find(|effect| effect.id == effect_id)
                .ok_or("effect intent not found")?;
            if effect.state != EffectState::Intended {
                return Err("effect is no longer intended".into());
            }
            Ok(RunEventKind::EffectChanged {
                effect_id,
                state: if succeeded {
                    EffectState::Succeeded
                } else {
                    EffectState::Failed
                },
            })
        }
        RunCommand::ResolveUncertainEffect {
            effect_id,
            succeeded,
        } => {
            let effect = snapshot
                .effects
                .iter()
                .find(|effect| effect.id == effect_id)
                .ok_or("effect intent not found")?;
            if effect.state != EffectState::Uncertain {
                return Err("effect is not uncertain".into());
            }
            Ok(RunEventKind::EffectChanged {
                effect_id,
                state: if succeeded {
                    EffectState::Succeeded
                } else {
                    EffectState::Failed
                },
            })
        }
        RunCommand::AdvanceLoop => {
            if snapshot.loops >= snapshot.limits.max_loops {
                return Err("workflow loop budget exhausted".into());
            }
            Ok(RunEventKind::LoopAdvanced)
        }
        RunCommand::AcceptStory { story_id } => {
            let story = stories
                .iter()
                .find(|story| story.id == story_id)
                .ok_or("story is not in plan")?;
            if story.status != StoryStatus::Done {
                return Err("story has not been approved".into());
            }
            if snapshot.attempts.iter().any(|attempt| {
                attempt.story_id == story_id && attempt.state == AttemptState::Running
            }) {
                return Err("story still has a running attempt".into());
            }
            Ok(RunEventKind::StoryAccepted {
                story_id,
                revision: story.revision,
            })
        }
        RunCommand::RecordCheck { story_id, receipt } => {
            let story = stories
                .iter()
                .find(|story| story.id == story_id)
                .ok_or("story is not in plan")?;
            let execution = snapshot
                .stories
                .iter()
                .find(|item| item.story_id == story_id)
                .ok_or("story execution not found")?;
            if (!execution.accepted || execution.accepted_revision != Some(story.revision))
                && !snapshot
                    .graph_executions
                    .iter()
                    .any(|g| g.target_id == story_id && !g.completed)
            {
                return Err("check receipt has a stale story revision".into());
            }
            if receipt.commit.is_empty() || receipt.tree.is_empty() {
                return Err("check receipt has no artifact digest".into());
            }
            Ok(RunEventKind::CheckRecorded { story_id, receipt })
        }
        RunCommand::RecordIntegration { story_id, receipt } => {
            let story = stories
                .iter()
                .find(|story| story.id == story_id)
                .ok_or("story is not in plan")?;
            let execution = snapshot
                .stories
                .iter()
                .find(|item| item.story_id == story_id)
                .ok_or("story execution not found")?;
            if story.status != StoryStatus::Done
                || !execution.accepted
                || execution.accepted_revision != Some(receipt.story_revision)
                || !story_work_revision_current(&story.id, receipt.story_revision, story.revision)?
            {
                return Err("integration receipt has a stale story revision".into());
            }
            if execution.integration_receipt.is_some() {
                return Err("story already has an integration receipt".into());
            }
            Ok(RunEventKind::StoryIntegrated { story_id, receipt })
        }
        RunCommand::RecordRecertification { receipt } => {
            if snapshot.canonical_ref.as_deref() != Some(receipt.canonical_ref.as_str())
                || receipt.commit.is_empty()
                || receipt.tree.is_empty()
                || !snapshot
                    .stories
                    .iter()
                    .any(|story| story.integration_receipt.is_some())
                    && !matches!(snapshot.root_target, Some(RunTarget::Plan(_)))
            {
                return Err("canonical recertification has no current integration".into());
            }
            Ok(RunEventKind::CanonicalRecertified { receipt })
        }
        RunCommand::FinalVerificationPassed => {
            ready_to_verify(snapshot, &stories)?;
            if matches!(snapshot.root_target, Some(RunTarget::Plan(_))) {
                let definition = WorkflowStore::open()?
                    .get_published(&snapshot.definition_id, snapshot.definition_revision)?;
                require_nonempty_policy(&definition)?;
                let (commit, tree) = clean_artifact(Path::new(&snapshot.project))?;
                let receipt = snapshot
                    .canonical_recertification
                    .as_ref()
                    .ok_or("deterministic final plan receipt missing")?;
                require_current_checks(
                    &definition.required_checks,
                    &receipt.post_checks,
                    snapshot
                        .canonical_ref
                        .as_deref()
                        .ok_or("canonical ref missing")?,
                    &commit,
                    &tree,
                )?;
            }
            Ok(RunEventKind::VerificationPassed {
                fingerprint: fingerprint(&snapshot.plan_id, &stories, true)?,
            })
        }
        RunCommand::Complete => {
            if !snapshot.graph_executions.is_empty()
                && (!matches!(snapshot.root_target, Some(RunTarget::Plan(_)))
                    || !snapshot.graph_executions.iter().all(|g| g.completed))
            {
                return Err(
                    "graph plan completion requires all reached executions complete".into(),
                );
            }
            ready_to_verify(snapshot, &stories)?;
            if snapshot.verification_fingerprint.as_deref()
                != Some(fingerprint(&snapshot.plan_id, &stories, true)?.as_str())
            {
                return Err("final verification is missing or stale".into());
            }
            Ok(RunEventKind::Completed)
        }
        RunCommand::Pause => Ok(RunEventKind::Paused),
        RunCommand::PauseFor { cause } => Ok(RunEventKind::PausedFor { cause }),
        RunCommand::ExpireDeadline => unreachable!("deadline handled before ordinary commands"),
        RunCommand::ResumeGraph {
            execution_id,
            activation_id,
            resolution,
        } => {
            if resolution.trim().is_empty() || resolution.len() > 4096 {
                return Err("graph resume needs a bounded explicit resolution".into());
            }
            super::reducer::validate_resume(snapshot)?;
            let graph = snapshot
                .graph_executions
                .iter()
                .find(|graph| graph.id == execution_id)
                .ok_or("graph execution not found")?;
            let activation = graph
                .activations
                .iter()
                .find(|activation| activation.id == activation_id)
                .ok_or("graph activation not reached")?;
            if activation.state == super::graph::ActivationState::Paused {
                Ok(RunEventKind::Graph {
                    event: super::graph::GraphEvent::Transition {
                        transition: super::graph::GraphTransition::ResolvePause {
                            execution_id,
                            activation_id,
                            resolution,
                        },
                    },
                })
            } else {
                validate_graph_recovery(snapshot, &execution_id, &activation_id)?;
                Ok(RunEventKind::GraphResumed {
                    execution_id,
                    activation_id,
                    resolution,
                })
            }
        }
        RunCommand::Resume => {
            if snapshot
                .graph_executions
                .iter()
                .any(|graph| graph.pauses.iter().any(|pause| pause.resolution.is_none()))
            {
                return Err("graph pause needs ResolvePause".into());
            }
            if !snapshot.graph_executions.is_empty() {
                return Err("graph resume needs an explicit activation and resolution".into());
            }
            super::reducer::validate_resume(snapshot)?;
            Ok(RunEventKind::Resumed)
        }
        RunCommand::Cancel => Ok(RunEventKind::Cancelled),
    }
}

#[cfg(test)]
mod independent_check_tests {
    use super::*;

    #[test]
    fn independent_check_does_not_wait_for_another_receipt_service() {
        // catches: the global integration/recertification mutex serializing an
        // unrelated check for the full duration of another service's subprocess.
        let (flow, _config_guard) = super::super::critic_tests::accepted_flow(false);
        let held = SERVICE_RECEIPT_LOCK.lock().unwrap();
        std::thread::scope(|scope| {
            let (sender, receiver) = std::sync::mpsc::channel();
            let flow = &flow;
            let worker = scope.spawn(move || {
                sender
                    .send(flow.store.execute_check(
                        &flow.run_id,
                        &flow.story_id,
                        "repository-integrity",
                        "check",
                        flow.sequence,
                    ))
                    .unwrap();
            });
            // This bound checks lock independence, not subprocess performance.
            // Release before asserting so a regressed worker can finish and join.
            let result = receiver.recv_timeout(Duration::from_secs(120));
            drop(held);
            worker.join().unwrap();
            let receipt = result
                .expect("independent check waited for the held receipt lock")
                .expect("published check passes");
            assert!(matches!(
                receipt.event.kind,
                RunEventKind::CheckRecorded { .. }
            ));
        });
    }
}

#[cfg(test)]
mod preflight_tests {
    use super::*;

    #[test]
    fn concurrent_story_revision_rejects_a_prepared_run_event() {
        let (config, project, plan, story, definition, _guard) = super::super::tests::fixture();
        let store = RunStore::open_at(&config.path().join("workflow_runs.sqlite3")).unwrap();
        let run = store
            .start_plan(
                &project.path().canonicalize().unwrap().to_string_lossy(),
                &plan,
                &definition,
                1,
                RunLimits::default(),
            )
            .unwrap();
        let error = store
            .command_at_checked_with_precommit(
                &run.id,
                "pause",
                Some(run.sequence),
                RunCommand::Pause,
                now_ms(),
                || {
                    StoryStore::open()
                        .unwrap()
                        .transition(&story, 1, crate::stories::StoryCommand::StartManual)
                        .unwrap();
                },
            )
            .unwrap_err();
        assert!(error.contains("revisions changed"), "{error}");
        assert_eq!(store.snapshot(&run.id).unwrap().sequence, run.sequence);
        assert_eq!(
            store.replay(&run.id).unwrap(),
            store.snapshot(&run.id).unwrap()
        );
    }

    #[test]
    fn concurrent_run_revision_rejects_a_prepared_run_event() {
        let (config, project, plan, _story, definition, _guard) = super::super::tests::fixture();
        let store = RunStore::open_at(&config.path().join("workflow_runs.sqlite3")).unwrap();
        let run = store
            .start_plan(
                &project.path().canonicalize().unwrap().to_string_lossy(),
                &plan,
                &definition,
                1,
                RunLimits::default(),
            )
            .unwrap();
        let error = store
            .command_at_checked_with_precommit(
                &run.id,
                "pause",
                Some(run.sequence),
                RunCommand::Pause,
                now_ms(),
                || {
                    store
                        .command(&run.id, "other-writer", RunCommand::Pause)
                        .unwrap();
                },
            )
            .unwrap_err();
        assert!(error.contains("revisions changed"), "{error}");
        assert_eq!(store.snapshot(&run.id).unwrap().sequence, run.sequence + 1);
        assert_eq!(
            store.replay(&run.id).unwrap(),
            store.snapshot(&run.id).unwrap()
        );
    }
}

/// A recovery resolution names the actual pending position, not an arbitrary node.
pub(super) fn validate_graph_recovery(
    snapshot: &RunSnapshot,
    execution_id: &str,
    activation_id: &str,
) -> Result<(), String> {
    super::reducer::validate_resume(snapshot)?;
    if snapshot
        .graph_executions
        .iter()
        .any(|graph| graph.pauses.iter().any(|pause| pause.resolution.is_none()))
    {
        return Err("graph pause needs ResolvePause".into());
    }
    let graph = snapshot
        .graph_executions
        .iter()
        .find(|graph| graph.id == execution_id)
        .ok_or("graph execution not found")?;
    if !graph.activations.iter().any(|activation| {
        activation.id == activation_id
            && matches!(
                activation.state,
                super::graph::ActivationState::Ready | super::graph::ActivationState::Running
            )
    }) {
        return Err("graph activation is not pending recovery".into());
    }
    Ok(())
}

fn choose_story_attempt(
    snapshot: &RunSnapshot,
    stories: &[Story],
    story_id: String,
    node_id: String,
) -> Result<RunEventKind, String> {
    let story = stories
        .iter()
        .find(|story| story.id == story_id)
        .ok_or("story is not in plan")?;
    for dependency_id in &story.dependencies {
        let dependency = stories
            .iter()
            .find(|item| item.id == *dependency_id)
            .ok_or("dependency is not in plan")?;
        if !receipt_current(snapshot, &dependency.id, dependency.revision)? {
            return Err("dependent story dispatch requires an integration receipt".into());
        }
    }
    if !matches!(
        story.status,
        StoryStatus::Ready | StoryStatus::InProgress | StoryStatus::Review
    ) {
        return Err("story is not ready for a workflow attempt".into());
    }
    let active: Vec<_> = snapshot
        .attempts
        .iter()
        .filter(|attempt| {
            attempt.story_id != snapshot.plan_id && attempt.state == AttemptState::Running
        })
        .collect();
    if active.iter().any(|attempt| attempt.story_id == story_id) {
        return Err("story already has a running attempt".into());
    }
    if active.len() >= usize::from(snapshot.limits.max_parallel_stories) {
        return Err("parallel story limit reached".into());
    }
    if active.iter().any(|attempt| {
        stories
            .iter()
            .find(|other| other.id == attempt.story_id)
            .is_none_or(|other| scopes_may_overlap(&story.file_scope, &other.file_scope))
    }) {
        return Err("story file scope overlaps active or unknown work".into());
    }
    if snapshot.attempts.len() >= 512 {
        return Err("workflow attempt budget exhausted".into());
    }
    let definition = WorkflowStore::open()?.get_published(
        &snapshot.story_definition_id,
        snapshot.story_definition_revision,
    )?;
    if !definition
        .graph
        .nodes
        .iter()
        .any(|node| node.id == node_id && matches!(node.kind, NodeKind::Agent { .. }))
    {
        return Err("node is not an agent in the pinned story template".into());
    }
    if snapshot.attempts.iter().any(|attempt| {
        attempt.story_id == story_id
            && attempt.node_id == node_id
            && attempt.state == AttemptState::Running
    }) {
        return Err("story node already has a running attempt".into());
    }
    let generation = snapshot
        .attempts
        .iter()
        .filter(|attempt| attempt.story_id == story_id && attempt.node_id == node_id)
        .map(|attempt| attempt.generation)
        .max()
        .unwrap_or(0)
        + 1;
    Ok(RunEventKind::AttemptStarted {
        attempt: Box::new(NodeAttempt {
            id: Uuid::now_v7().to_string(),
            story_id,
            node_id,
            generation,
            state: AttemptState::Running,
            outcome: None,
            agent: None,
            report: None,
            input_answer: None,
        }),
    })
}

fn replay_events(conn: &Connection, run_id: &str) -> Result<RunSnapshot, String> {
    let mut stmt = conn
        .prepare("SELECT event_json FROM workflow_events WHERE run_id=?1 ORDER BY sequence")
        .map_err(|e| format!("prepare workflow replay: {e}"))?;
    let rows = stmt
        .query_map([run_id], |row| row.get::<_, String>(0))
        .map_err(|e| format!("read workflow replay: {e}"))?;
    let mut snapshot = None;
    for row in rows {
        let event: RunEvent = decode(&row.map_err(|e| format!("read replay event: {e}"))?)?;
        snapshot = Some(apply_event(snapshot, &event)?);
    }
    snapshot.ok_or("workflow run has no events".into())
}

fn choose_plan_attempt(snapshot: &RunSnapshot, node_id: String) -> Result<RunEventKind, String> {
    if snapshot.attempts.len() >= 512 {
        return Err("workflow attempt budget exhausted".into());
    }
    let definition = WorkflowStore::open()?
        .get_published(&snapshot.definition_id, snapshot.definition_revision)?;
    if !definition.graph.nodes.iter().any(|node| {
        node.id == node_id
            && matches!(
                node.kind,
                NodeKind::Agent {
                    role: crate::workflows::AgentRole::Coordinator
                        | crate::workflows::AgentRole::Planner,
                    ..
                }
            )
    }) {
        return Err("node is not a coordinator or planner in the pinned plan template".into());
    }
    if snapshot.attempts.iter().any(|attempt| {
        attempt.story_id == snapshot.plan_id
            && attempt.node_id == node_id
            && attempt.state == AttemptState::Running
    }) {
        return Err("plan node already has a running attempt".into());
    }
    let generation = snapshot
        .attempts
        .iter()
        .filter(|attempt| attempt.story_id == snapshot.plan_id && attempt.node_id == node_id)
        .map(|attempt| attempt.generation)
        .max()
        .unwrap_or(0)
        + 1;
    Ok(RunEventKind::AttemptStarted {
        attempt: Box::new(NodeAttempt {
            id: Uuid::now_v7().to_string(),
            story_id: snapshot.plan_id.clone(),
            node_id,
            generation,
            state: AttemptState::Running,
            outcome: None,
            agent: None,
            report: None,
            input_answer: None,
        }),
    })
}

/// Project-wide wave admission uses the existing graph reservations, not another scheduler.
fn require_dispatch_slot(
    conn: &Connection,
    snapshot: &RunSnapshot,
    story: &Story,
) -> Result<(), String> {
    runtime_start::require_available_story(conn, &snapshot.project, &story.id)?;
    let mut stmt = conn.prepare("SELECT snapshot_json FROM workflow_runs WHERE project=?1 AND status IN ('running','paused')")
        .map_err(|e| format!("read dispatch wave: {e}"))?;
    let mut active = 0;
    for row in stmt
        .query_map([&snapshot.project], |r| r.get::<_, String>(0))
        .map_err(|e| format!("read dispatch wave: {e}"))?
    {
        let run: RunSnapshot = decode(&row.map_err(|e| format!("read dispatch wave: {e}"))?)?;
        for graph in run
            .graph_executions
            .iter()
            .filter(|g| g.target_id != run.plan_id && !g.completed)
        {
            active += 1;
            let other = StoryStore::open()?.get_story(&graph.target_id)?;
            if scopes_may_overlap(&story.file_scope, &other.file_scope) {
                return Err("story file scope overlaps project dispatch wave".into());
            }
        }
    }
    if active >= usize::from(snapshot.limits.max_parallel_stories) {
        return Err("project parallel story limit reached".into());
    }
    Ok(())
}
