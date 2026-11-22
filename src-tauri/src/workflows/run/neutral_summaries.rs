//! Disposable presentation cache, updated with the ledger transaction. Never authorizes a command.
use super::{
    neutral::NeutralRun,
    neutral_execution::{NeutralExecution, NeutralStatus},
    store::RunStore,
};
use rusqlite::{Connection, params};
use serde_json::{Value, json};

fn save(
    conn: &Connection,
    id: &str,
    project: &str,
    active: bool,
    sequence: i64,
    value: Value,
) -> Result<(), String> {
    conn.execute("INSERT INTO neutral_run_summaries(run_id,project,active,sequence,value_json) VALUES (?1,?2,?3,?4,json_set(?5,'$.started_ms',COALESCE((SELECT json_extract(event_json,'$.at_ms') FROM neutral_workflow_events WHERE run_id=?1 AND sequence=1),json_extract(?5,'$.started_ms'))))
        ON CONFLICT(run_id) DO UPDATE SET active=excluded.active,sequence=excluded.sequence,value_json=json_set(excluded.value_json,
        '$.started_ms',json_extract(neutral_run_summaries.value_json,'$.started_ms')) WHERE excluded.sequence > neutral_run_summaries.sequence",
        params![id,project,active,sequence,value.to_string()]).map(|_| ()).map_err(|e| e.to_string())
}
pub(super) fn record_start(conn: &Connection, run: &NeutralRun, at: i64) -> Result<(), String> {
    save(
        conn,
        &run.id,
        &run.request.publication.project,
        true,
        1,
        json!({"id":run.id,
        "publication":run.request.publication,"name":run.request.publication.id,
        "status":{"type":"running"},"current_node":run.closure.root.definition.control.entry()?,
        "waiting_on_approval":false,"approval_deadlines":[],"resource_waits":[],"started_ms":at,"updated_ms":at}),
    )
}
pub(super) fn record(
    conn: &Connection,
    snapshot: &NeutralExecution,
    at: i64,
) -> Result<(), String> {
    let now = super::store::now_ms();
    let deadlines: Vec<_> = snapshot
        .effects
        .values()
        .filter(|e| super::neutral_views::pending(e, snapshot, now))
        .filter_map(|e| {
            e.question
                .as_ref()
                .map(|q| q.deadline_ms.min(snapshot.deadline_ms))
        })
        .collect();
    let active = snapshot.status == NeutralStatus::Running;
    let resources: Vec<_> = if active {
        snapshot
            .effects
            .values()
            .filter(|e| !e.cancel_started && e.observation.is_none())
            .filter_map(|e| e.resource_wait.as_ref())
            .collect()
    } else {
        Vec::new()
    };
    save(
        conn,
        &snapshot.run.id,
        &snapshot.run.request.publication.project,
        active,
        snapshot.sequence,
        json!({
        "id":snapshot.run.id,"publication":snapshot.run.request.publication,"name":snapshot.run.request.publication.id,
        "status":snapshot.status,"current_node":snapshot.node,"waiting_on_approval":!deadlines.is_empty(),
        "approval_deadlines":deadlines,"resource_waits":resources,"started_ms":at,"updated_ms":at}),
    )
}
impl RunStore {
    pub(crate) fn neutral_run_views(&self, project: &str) -> Result<Vec<Value>, String> {
        let conn = self.connect()?;
        // Lazy upgrade is bounded; startup recovery also projects every valid old run.
        let mut missing = conn.prepare("SELECT r.id FROM neutral_workflow_runs r LEFT JOIN neutral_run_summaries s ON s.run_id=r.id
            WHERE r.project=?1 AND s.run_id IS NULL ORDER BY r.rowid DESC LIMIT 50").map_err(|e| e.to_string())?;
        let ids = missing
            .query_map([project], |r| r.get::<_, String>(0))
            .map_err(|e| e.to_string())?
            .collect::<Result<Vec<_>, _>>()
            .map_err(|e| e.to_string())?;
        drop(missing);
        for id in ids {
            match self.neutral_inspection(&id, 0, 1) {
                Ok((snapshot, events)) => {
                    record_start(&conn, &snapshot.run, events.first().map_or(0, |e| e.at_ms))?;
                    // Read the last event timestamp without replaying a second time.
                    let updated: i64 = conn.query_row("SELECT json_extract(event_json,'$.at_ms') FROM neutral_workflow_events WHERE run_id=?1 ORDER BY sequence DESC LIMIT 1",[&id],|r|r.get(0)).map_err(|e|e.to_string())?;
                    record(&conn, &snapshot, updated)?;
                }
                Err(error) if error.contains("locked") || error.contains("busy") => {
                    return Err(error);
                }
                Err(error) => save(
                    &conn,
                    &id,
                    project,
                    false,
                    0,
                    json!({"id":id,"publication":{"project":project,"id":"unavailable","revision":0},"name":"Unreadable run",
                    "status":{"type":"unavailable"},"current_node":"","waiting_on_approval":false,"started_ms":0,"updated_ms":0,"error":error}),
                )?,
            }
        }
        let mut query = conn.prepare("SELECT s.value_json FROM neutral_run_summaries s JOIN neutral_workflow_runs r ON r.id=s.run_id
            WHERE s.project=?1 AND (s.active=1 OR s.run_id IN (SELECT s2.run_id FROM neutral_run_summaries s2 JOIN neutral_workflow_runs r2 ON r2.id=s2.run_id WHERE s2.project=?1 AND s2.active=0 ORDER BY r2.rowid DESC LIMIT 50))
            ORDER BY r.rowid DESC").map_err(|e|e.to_string())?;
        let rows = query
            .query_map([project], |r| r.get::<_, String>(0))
            .map_err(|e| e.to_string())?;
        let workflows = crate::workflows::WorkflowStore::open()?.connect()?;
        let now = super::store::now_ms();
        rows.map(|row| {
            let mut value: Value = serde_json::from_str(&row.map_err(|e| e.to_string())?)
                .map_err(|e| e.to_string())?;
            value["waiting_on_approval"] = json!(
                value["approval_deadlines"]
                    .as_array()
                    .is_some_and(|ds| ds.iter().any(|d| d.as_i64().is_some_and(|d| d > now)))
            );
            let id = value["publication"]["id"].as_str().unwrap_or_default();
            use rusqlite::OptionalExtension;
            if let Some(name) = workflows
                .query_row(
                    "SELECT name FROM neutral_workflow_drafts WHERE project=?1 AND id=?2",
                    params![project, id],
                    |r| r.get::<_, String>(0),
                )
                .optional()
                .map_err(|e| e.to_string())?
            {
                value["name"] = json!(name);
            }
            Ok(value)
        })
        .collect()
    }
}
