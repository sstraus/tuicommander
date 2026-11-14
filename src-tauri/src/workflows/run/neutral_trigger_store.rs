//! Durable trigger intents live beside neutral runs, not in another executor.
use super::RunStore;
use super::neutral::NeutralStartRequest;
use crate::workflows::catalog::content_digest;
use crate::workflows::trigger::{TriggerAdmission, TriggerEvent};
use rusqlite::{Connection, OptionalExtension, TransactionBehavior, params};
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Serialize, Deserialize)]
pub(super) struct TriggerIntent {
    pub project: String,
    pub receipt: String,
    pub event: TriggerEvent,
    pub request: NeutralStartRequest,
    pub admission: TriggerAdmission,
}
const SCHEMA: &str = "CREATE TABLE IF NOT EXISTS neutral_trigger_intents (
 receipt TEXT PRIMARY KEY, project TEXT NOT NULL, binding_id TEXT NOT NULL,
 resource TEXT NOT NULL, generation TEXT NOT NULL, replay_id TEXT NOT NULL,
 document TEXT NOT NULL, run_id TEXT,
 UNIQUE(project,binding_id,resource,generation,replay_id));
 CREATE TABLE IF NOT EXISTS neutral_trigger_resources (
 project TEXT NOT NULL,binding_id TEXT NOT NULL,resource TEXT NOT NULL,
 active_run TEXT,pending_receipt TEXT,
 PRIMARY KEY(project,binding_id,resource));
 CREATE TABLE IF NOT EXISTS neutral_trigger_self_events (
 project TEXT NOT NULL,binding_id TEXT NOT NULL,resource TEXT NOT NULL,generation TEXT NOT NULL,
 effect_digest TEXT NOT NULL, PRIMARY KEY(project,binding_id,resource,generation));
 CREATE TABLE IF NOT EXISTS neutral_trigger_self_writes (
 project TEXT NOT NULL,binding_id TEXT NOT NULL,resource TEXT NOT NULL,base_generation TEXT NOT NULL,
 label TEXT NOT NULL,effect_digest TEXT NOT NULL,
 PRIMARY KEY(project,binding_id,resource,base_generation,label));
 CREATE TABLE IF NOT EXISTS neutral_trigger_baselines (
 project TEXT NOT NULL,binding_id TEXT NOT NULL,revision INTEGER NOT NULL,
 PRIMARY KEY(project,binding_id));";
impl RunStore {
    pub(super) fn trigger_connection(&self) -> Result<Connection, String> {
        let conn = self.connect()?;
        conn.execute_batch(SCHEMA).map_err(|e| e.to_string())?;
        Ok(conn)
    }
    pub(super) fn reserve_trigger(
        &self,
        project: &str,
        binding: &str,
        event: TriggerEvent,
        request: NeutralStartRequest,
        replay_id: &str,
    ) -> Result<TriggerIntent, String> {
        self.retain_trigger(project, binding, event, request, replay_id, true)
    }
    pub(super) fn baseline_trigger(
        &self,
        project: &str,
        binding: &str,
        event: TriggerEvent,
        request: NeutralStartRequest,
    ) -> Result<TriggerIntent, String> {
        self.retain_trigger(project, binding, event, request, "", false)
    }
    fn retain_trigger(
        &self,
        project: &str,
        binding: &str,
        event: TriggerEvent,
        mut request: NeutralStartRequest,
        replay_id: &str,
        enqueue: bool,
    ) -> Result<TriggerIntent, String> {
        let receipt = content_digest(&(
            project,
            binding,
            &event.resource,
            &event.generation,
            replay_id,
        ))?;
        let mut conn = self.trigger_connection()?;
        let tx = conn
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .map_err(|e| e.to_string())?;
        if let Some(intent) = read_intent(&tx, &receipt)? {
            return Ok(intent);
        }
        request.request_id = format!("trigger:{receipt}");
        let intent = TriggerIntent {
            project: project.into(),
            receipt: receipt.clone(),
            admission: TriggerAdmission {
                binding_id: binding.into(),
                resource: event.resource.clone(),
                generation: event.generation.clone(),
                run_id: None,
                disposition: if enqueue { "pending" } else { "baseline" }.into(),
            },
            event,
            request,
        };
        tx.execute("INSERT INTO neutral_trigger_intents(receipt,project,binding_id,resource,generation,replay_id,document)
            VALUES(?1,?2,?3,?4,?5,?6,?7)",params![receipt,project,binding,intent.event.resource,intent.event.generation,replay_id,
            serde_json::to_string(&intent).map_err(|e|e.to_string())?]).map_err(|e|e.to_string())?;
        if enqueue {
            tx.execute("INSERT INTO neutral_trigger_resources(project,binding_id,resource,pending_receipt) VALUES(?1,?2,?3,?4)
            ON CONFLICT(project,binding_id,resource) DO UPDATE SET pending_receipt=excluded.pending_receipt",
            params![project,binding,intent.event.resource,receipt]).map_err(|e|e.to_string())?;
        }
        tx.commit().map_err(|e| e.to_string())?;
        Ok(intent)
    }
    pub(super) fn trigger_baseline_ready(
        &self,
        project: &str,
        binding: &str,
        revision: i64,
    ) -> Result<bool, String> {
        self.trigger_connection()?.query_row("SELECT EXISTS(SELECT 1 FROM neutral_trigger_baselines WHERE project=?1 AND binding_id=?2 AND revision=?3)",params![project,binding,revision],|r|r.get(0)).map_err(|e|e.to_string())
    }
    pub(super) fn complete_trigger_baseline(
        &self,
        project: &str,
        binding: &str,
        revision: i64,
    ) -> Result<(), String> {
        self.trigger_connection()?.execute("INSERT INTO neutral_trigger_baselines VALUES(?1,?2,?3) ON CONFLICT(project,binding_id) DO UPDATE SET revision=excluded.revision",params![project,binding,revision]).map_err(|e|e.to_string())?;
        Ok(())
    }
    pub(super) fn trigger_intent(&self, receipt: &str) -> Result<Option<TriggerIntent>, String> {
        read_intent(&self.trigger_connection()?, receipt)
    }
    pub(super) fn trigger_pending(
        &self,
        project: &str,
    ) -> Result<Vec<(Option<String>, TriggerIntent)>, String> {
        let conn = self.trigger_connection()?;
        let mut query = conn
            .prepare(
                "SELECT active_run,pending_receipt FROM neutral_trigger_resources
            WHERE project=?1 AND pending_receipt IS NOT NULL ORDER BY binding_id,resource",
            )
            .map_err(|e| e.to_string())?;
        let rows = query
            .query_map([project], |r| {
                Ok((r.get::<_, Option<String>>(0)?, r.get::<_, String>(1)?))
            })
            .map_err(|e| e.to_string())?;
        rows.map(|r| {
            let (active, receipt) = r.map_err(|e| e.to_string())?;
            Ok((
                active,
                read_intent(&conn, &receipt)?.ok_or("trigger intent missing")?,
            ))
        })
        .collect()
    }
    pub(super) fn trigger_started(&self, intent: &TriggerIntent, run: &str) -> Result<(), String> {
        let mut updated = intent.clone();
        updated.admission.run_id = Some(run.into());
        updated.admission.disposition = "started".into();
        let mut conn = self.trigger_connection()?;
        let tx = conn
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .map_err(|e| e.to_string())?;
        tx.execute(
            "UPDATE neutral_trigger_intents SET run_id=?2,document=?3 WHERE receipt=?1",
            params![
                intent.receipt,
                run,
                serde_json::to_string(&updated).map_err(|e| e.to_string())?
            ],
        )
        .map_err(|e| e.to_string())?;
        tx.execute(
            "UPDATE neutral_trigger_resources SET active_run=?4,pending_receipt=NULL
            WHERE project=?1 AND binding_id=?2 AND resource=?3 AND pending_receipt=?5",
            params![
                intent.project,
                intent.admission.binding_id,
                intent.event.resource,
                run,
                intent.receipt
            ],
        )
        .map_err(|e| e.to_string())?;
        tx.commit().map_err(|e| e.to_string())?;
        Ok(())
    }
    pub(super) fn trigger_original(
        &self,
        project: &str,
        binding: &str,
        resource: &str,
        generation: &str,
    ) -> Result<TriggerIntent, String> {
        let conn = self.trigger_connection()?;
        let receipt: Option<String> = conn
            .query_row(
                "SELECT receipt FROM neutral_trigger_intents WHERE
            project=?1 AND binding_id=?2 AND resource=?3 AND generation=?4 AND replay_id=''",
                params![project, binding, resource, generation],
                |r| r.get(0),
            )
            .optional()
            .map_err(|e| e.to_string())?;
        read_intent(
            &conn,
            &receipt.ok_or("retained trigger snapshot not found")?,
        )?
        .ok_or("trigger intent missing".into())
    }
    /// A run is about to add `label` to the issue it was admitted for. The claim names the run's binding and its admitted generation; the
    /// poll that observes exactly this label on the next generation records the
    /// self event (`normalize_github`). Called before the provider write, so a
    /// poll that races the reply still finds the claim.
    pub(super) fn record_self_label(
        &self,
        receipt: &str,
        label: &str,
        effect_digest: &str,
    ) -> Result<(), String> {
        let conn = self.trigger_connection()?;
        let intent = read_intent(&conn, receipt)?.ok_or("trigger intent missing")?;
        conn.execute(
            "INSERT INTO neutral_trigger_self_writes(project,binding_id,resource,base_generation,label,effect_digest)
            VALUES(?1,?2,?3,?4,?5,?6) ON CONFLICT DO NOTHING",
            params![
                intent.project,
                intent.admission.binding_id,
                intent.event.resource,
                intent.event.generation,
                label.to_ascii_lowercase(),
                effect_digest
            ],
        )
        .map_err(|e| e.to_string())?;
        Ok(())
    }
    /// Give back the claim of a label write that did not reach the issue, so a
    /// later identical label from someone else still admits a run.
    pub(super) fn release_self_label(&self, receipt: &str, label: &str) -> Result<(), String> {
        let conn = self.trigger_connection()?;
        let intent = read_intent(&conn, receipt)?.ok_or("trigger intent missing")?;
        conn.execute(
            "DELETE FROM neutral_trigger_self_writes WHERE project=?1 AND binding_id=?2
            AND resource=?3 AND base_generation=?4 AND label=?5",
            params![
                intent.project,
                intent.admission.binding_id,
                intent.event.resource,
                intent.event.generation,
                label.to_ascii_lowercase()
            ],
        )
        .map_err(|e| e.to_string())?;
        Ok(())
    }
    pub(super) fn trigger_suppressed(
        &self,
        project: &str,
        binding: &str,
        event: &TriggerEvent,
    ) -> Result<bool, String> {
        self.trigger_connection()?
            .query_row(
                "SELECT EXISTS(SELECT 1 FROM neutral_trigger_self_events
            WHERE project=?1 AND binding_id=?2 AND resource=?3 AND generation=?4)",
                params![project, binding, event.resource, event.generation],
                |r| r.get(0),
            )
            .map_err(|e| e.to_string())
    }
}
/// The new generation of `resource` differs from `base` only by
/// `added` labels. Every binding whose run claimed all of them records a self
/// event for `generation`; its own write never admits it again. Claims on
/// `base` are consumed either way: a later generation is never a self event.
pub(super) fn record_self_events(
    conn: &Connection,
    resource: &str,
    base: &str,
    generation: &str,
    added: Option<&std::collections::BTreeSet<String>>,
) -> Result<(), String> {
    let mut claims: std::collections::BTreeMap<
        (String, String),
        (std::collections::BTreeSet<String>, String),
    > = Default::default();
    {
        let mut query = conn
            .prepare(
                "SELECT project,binding_id,label,effect_digest FROM neutral_trigger_self_writes
            WHERE resource=?1 AND base_generation=?2",
            )
            .map_err(|e| e.to_string())?;
        let rows = query
            .query_map(params![resource, base], |r| {
                Ok((
                    r.get::<_, String>(0)?,
                    r.get::<_, String>(1)?,
                    r.get::<_, String>(2)?,
                    r.get::<_, String>(3)?,
                ))
            })
            .map_err(|e| e.to_string())?;
        for row in rows {
            let (project, binding, label, digest) = row.map_err(|e| e.to_string())?;
            let entry = claims.entry((project, binding)).or_default();
            entry.0.insert(label);
            if entry.1.is_empty() {
                entry.1 = digest;
            }
        }
    }
    if let Some(added) = added.filter(|added| !added.is_empty()) {
        for ((project, binding), (labels, digest)) in &claims {
            if added.is_subset(labels) {
                conn.execute(
                    "INSERT INTO neutral_trigger_self_events VALUES(?1,?2,?3,?4,?5) ON CONFLICT DO NOTHING",
                    params![project, binding, resource, generation, digest],
                )
                .map_err(|e| e.to_string())?;
            }
        }
    }
    conn.execute(
        "DELETE FROM neutral_trigger_self_writes WHERE resource=?1 AND base_generation=?2",
        params![resource, base],
    )
    .map_err(|e| e.to_string())?;
    Ok(())
}
fn read_intent(conn: &Connection, receipt: &str) -> Result<Option<TriggerIntent>, String> {
    let document: Option<String> = conn
        .query_row(
            "SELECT document FROM neutral_trigger_intents WHERE receipt=?1",
            [receipt],
            |r| r.get(0),
        )
        .optional()
        .map_err(|e| e.to_string())?;
    document
        .map(|s| serde_json::from_str(&s).map_err(|e| e.to_string()))
        .transpose()
}
