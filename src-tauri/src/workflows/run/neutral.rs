//! Neutral run persistence in the existing owner's ledger. Creation grants no rights.

use super::runtime::RuntimeOwner;
use super::store::{RunStore, now_ms};
use crate::workflows::WorkflowStore;
use crate::workflows::catalog::{ActivityDescriptor, HandlerRef, WorkflowCatalog, content_digest};
use crate::workflows::neutral_publication::{NeutralPublication, PublicationRef};
use rusqlite::{Connection, OptionalExtension, TransactionBehavior, params};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::{BTreeMap, BTreeSet};

pub(super) const CONTRACT_VERSION: u32 = 1;

pub(super) fn prepare_db(conn: &Connection) -> Result<(), String> {
    conn.execute_batch(
        "CREATE TABLE IF NOT EXISTS neutral_workflow_runs (
            id TEXT PRIMARY KEY, project TEXT NOT NULL, request_id TEXT NOT NULL,
            request_hash TEXT NOT NULL, contract_version INTEGER NOT NULL,
            UNIQUE(project,request_id)
        );
        CREATE TABLE IF NOT EXISTS neutral_workflow_events (
            run_id TEXT NOT NULL REFERENCES neutral_workflow_runs(id),
            sequence INTEGER NOT NULL CHECK(sequence > 0), command_id TEXT NOT NULL,
            event_json TEXT NOT NULL, sha256 TEXT NOT NULL,
            PRIMARY KEY(run_id,sequence), UNIQUE(run_id,command_id)
        );
        CREATE TABLE IF NOT EXISTS neutral_effect_resources (
            resource TEXT PRIMARY KEY, run_id TEXT NOT NULL REFERENCES neutral_workflow_runs(id),
            effect_id TEXT NOT NULL
        );",
    )
    .map_err(|e| format!("prepare neutral run schema: {e}"))
}

/// Generic limits do not encode story creation, review or compiler policy.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub(crate) struct NeutralLimits {
    pub max_activations: u32,
    pub max_concurrency: u32,
    pub max_duration_ms: u64,
}

impl NeutralLimits {
    pub(crate) fn validate(&self) -> Result<(), String> {
        if self.max_activations == 0
            || self.max_concurrency == 0
            || self.max_concurrency > self.max_activations
            || self.max_duration_ms == 0
            || self.max_duration_ms > crate::workflows::duration::MAX_DURATION_MS
        {
            return Err(
                "neutral limits require positive bounded duration and concurrency <= activations"
                    .into(),
            );
        }
        Ok(())
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub(crate) struct NeutralStartRequest {
    pub publication: PublicationRef,
    pub request_id: String,
    pub inputs: BTreeMap<String, Value>,
    pub subject: Option<BTreeMap<String, Value>>,
    pub limits: NeutralLimits,
}

/// Exact retained bodies, not current-catalog lookups or an executable permit.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct RetainedClosure {
    pub root: NeutralPublication,
    pub children: Vec<NeutralPublication>,
    pub descriptors: Vec<ActivityDescriptor>,
}

impl RetainedClosure {
    pub(super) fn resolve(
        workflows: &WorkflowStore,
        reference: &PublicationRef,
        catalog: &WorkflowCatalog,
        handlers: &BTreeSet<HandlerRef>,
    ) -> Result<Self, String> {
        let (root, children) = workflows.resolve_neutral_publication(
            &reference.project,
            &reference.id,
            reference.revision,
            catalog,
            handlers,
        )?;
        let mut descriptors = BTreeMap::new();
        for publication in std::iter::once(&root).chain(&children) {
            for pin in publication.pins.pins() {
                descriptors.insert(
                    pin.id.clone(),
                    catalog.resolve_pinned(pin, handlers)?.clone(),
                );
            }
        }
        Ok(Self {
            root,
            children,
            descriptors: descriptors.into_values().collect(),
        })
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct NeutralRun {
    pub contract_version: u32,
    pub id: String,
    pub request: NeutralStartRequest,
    pub closure: RetainedClosure,
    pub inputs: BTreeMap<String, Value>,
    pub subject: Option<BTreeMap<String, Value>>,
}

/// Keep the existing ledger envelope semantics without its business event enum.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct NeutralEvent {
    pub contract_version: u32,
    pub sequence: i64,
    pub command_id: String,
    pub command_hash: String,
    pub at_ms: i64,
    pub kind: NeutralEventKind,
}

#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(tag = "type", rename_all = "snake_case", deny_unknown_fields)]
pub(crate) enum NeutralEventKind {
    Started {
        run: Box<NeutralRun>,
    },
    Execution {
        command: Box<super::neutral_execution::NeutralExecutionCommand>,
        decision: Box<super::neutral_execution::NeutralDecision>,
    },
}

// An internally tagged derive buffers nested maps through serde Content, which
// cannot decode JSON object keys into the definition's usize activity indices.
// Decode each JSON payload directly instead; the stored event shape is unchanged.
impl<'de> Deserialize<'de> for NeutralEventKind {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let value = Value::deserialize(deserializer)?;
        match value.get("type").and_then(Value::as_str) {
            Some("started") => {
                #[derive(Deserialize)]
                #[serde(deny_unknown_fields)]
                struct Started {
                    #[serde(rename = "type")]
                    _kind: String,
                    run: Box<NeutralRun>,
                }
                let body: Started =
                    serde_json::from_value(value).map_err(serde::de::Error::custom)?;
                Ok(Self::Started { run: body.run })
            }
            Some("execution") => {
                #[derive(Deserialize)]
                #[serde(deny_unknown_fields)]
                struct Execution {
                    #[serde(rename = "type")]
                    _kind: String,
                    command: Box<super::neutral_execution::NeutralExecutionCommand>,
                    decision: Box<super::neutral_execution::NeutralDecision>,
                }
                let body: Execution =
                    serde_json::from_value(value).map_err(serde::de::Error::custom)?;
                Ok(Self::Execution {
                    command: body.command,
                    decision: body.decision,
                })
            }
            _ => Err(serde::de::Error::custom("unsupported neutral event kind")),
        }
    }
}

impl RuntimeOwner {
    /// Requires the same process-lifetime lock as existing runs. No actor is
    /// woken until the neutral scheduling consumer is installed in this story.
    pub(super) fn start_neutral(
        &self,
        workflows: &WorkflowStore,
        request: &NeutralStartRequest,
        catalog: &WorkflowCatalog,
        handlers: &BTreeSet<HandlerRef>,
    ) -> Result<NeutralRun, String> {
        if request.request_id.trim().is_empty() {
            return Err("neutral start requires a request identity".into());
        }
        request.limits.validate()?;
        let request_hash = content_digest(request)?;
        // An exact retry must not repin or depend on today's catalog/handlers.
        if let Some(run) = self.store.neutral_start_receipt(request, &request_hash)? {
            return Ok(run);
        }
        let closure = RetainedClosure::resolve(workflows, &request.publication, catalog, handlers)?;
        let (inputs, subject) = closure
            .root
            .definition
            .validate_values(&request.inputs, request.subject.as_ref())?;
        let run = NeutralRun {
            contract_version: CONTRACT_VERSION,
            id: uuid::Uuid::new_v4().to_string(),
            request: request.clone(),
            closure,
            inputs,
            subject,
        };
        let event = NeutralEvent {
            contract_version: CONTRACT_VERSION,
            sequence: 1,
            command_id: request.request_id.clone(),
            command_hash: request_hash.clone(),
            at_ms: now_ms(),
            kind: NeutralEventKind::Started {
                run: Box::new(run.clone()),
            },
        };
        let mut conn = self.store.connect()?;
        let tx = conn
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .map_err(|e| format!("begin neutral start: {e}"))?;
        // Concurrent same-owner callers may have resolved the same request.
        if let Some(existing) = start_receipt(&tx, request, &request_hash)? {
            tx.commit()
                .map_err(|e| format!("finish neutral retry: {e}"))?;
            return Ok(existing);
        }
        tx.execute(
            "INSERT INTO neutral_workflow_runs(id,project,request_id,request_hash,contract_version) VALUES (?1,?2,?3,?4,?5)",
            params![run.id, request.publication.project, request.request_id, request_hash, CONTRACT_VERSION],
        ).map_err(|e| format!("save neutral run: {e}"))?;
        let document =
            serde_json::to_string(&event).map_err(|e| format!("encode neutral event: {e}"))?;
        tx.execute(
            "INSERT INTO neutral_workflow_events(run_id,sequence,command_id,event_json,sha256) VALUES (?1,1,?2,?3,?4)",
            params![run.id, event.command_id, document, content_digest(&event)?],
        ).map_err(|e| format!("save neutral start event: {e}"))?;
        tx.commit()
            .map_err(|e| format!("commit neutral start: {e}"))?;
        self.publish_neutral_cursor(&run.request.publication.project, &run.id, 1);
        Ok(run)
    }
}

impl RunStore {
    pub(super) fn neutral_start_receipt(
        &self,
        request: &NeutralStartRequest,
        hash: &str,
    ) -> Result<Option<NeutralRun>, String> {
        start_receipt(&self.connect()?, request, hash)
    }

    /// Historical inspection requires neither an owner nor current handlers.
    pub(crate) fn neutral_run(&self, id: &str) -> Result<NeutralRun, String> {
        read_run(&self.connect()?, id)
    }

    /// `(run id, request id)` of the project's runs whose request id starts
    /// with `prefix`, newest first.
    pub(super) fn neutral_runs_with_request_prefix(
        &self,
        project: &str,
        prefix: &str,
    ) -> Result<Vec<(String, String)>, String> {
        let conn = self.connect()?;
        let mut query = conn
            .prepare(
                "SELECT id,request_id FROM neutral_workflow_runs
                 WHERE project=?1 AND substr(request_id,1,length(?2))=?2 ORDER BY rowid DESC",
            )
            .map_err(|e| format!("prepare neutral run list: {e}"))?;
        query
            .query_map(params![project, prefix], |row| {
                Ok((row.get(0)?, row.get(1)?))
            })
            .map_err(|e| format!("read neutral run list: {e}"))?
            .collect::<Result<Vec<_>, _>>()
            .map_err(|e| format!("read neutral run row: {e}"))
    }
}

fn start_receipt(
    conn: &Connection,
    request: &NeutralStartRequest,
    hash: &str,
) -> Result<Option<NeutralRun>, String> {
    let row: Option<(String, String)> = conn
        .query_row(
            "SELECT id,request_hash FROM neutral_workflow_runs WHERE project=?1 AND request_id=?2",
            params![request.publication.project, request.request_id],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .optional()
        .map_err(|e| format!("read neutral start receipt: {e}"))?;
    match row {
        None => Ok(None),
        Some((_, old_hash)) if old_hash != hash => {
            Err("neutral request identity was reused with different arguments".into())
        }
        Some((id, _)) => read_run(conn, &id).map(Some),
    }
}

pub(super) fn read_run(conn: &Connection, id: &str) -> Result<NeutralRun, String> {
    let (project, request_id, request_hash, version): (String, String, String, u32) = conn.query_row(
        "SELECT project,request_id,request_hash,contract_version FROM neutral_workflow_runs WHERE id=?1",
        [id], |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?)),
    ).map_err(|e| format!("read neutral run identity: {e}"))?;
    if version != CONTRACT_VERSION {
        return Err("unsupported neutral run contract".into());
    }
    let (document, digest): (String, String) = conn
        .query_row(
            "SELECT event_json,sha256 FROM neutral_workflow_events WHERE run_id=?1 AND sequence=1",
            [id],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .map_err(|e| format!("read neutral start event: {e}"))?;
    let event: NeutralEvent =
        serde_json::from_str(&document).map_err(|e| format!("decode neutral start event: {e}"))?;
    if event.contract_version != CONTRACT_VERSION
        || event.sequence != 1
        || event.command_id != request_id
        || event.command_hash != request_hash
        || content_digest(&event)? != digest
    {
        return Err("neutral start event identity or content mismatch".into());
    }
    let NeutralEventKind::Started { run } = event.kind else {
        return Err("neutral first event is not a start".into());
    };
    if run.contract_version != CONTRACT_VERSION
        || run.id != id
        || run.request.publication.project != project
        || run.request.request_id != request_id
        || content_digest(&run.request)? != request_hash
    {
        return Err("neutral run identity or request mismatch".into());
    }
    Ok(*run)
}
