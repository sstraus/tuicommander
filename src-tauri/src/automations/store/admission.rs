//! The scheduled cursor and admission decision commit together before dispatch.
use super::*;

impl RunStore {
    /// Latest consumed scheduled occurrence in UTC milliseconds. Manual runs
    /// never contribute; retention cannot erase the high-water cursor.
    pub fn scheduled_cursor(&self, automation_id: &str) -> Result<Option<i64>, String> {
        scheduled_cursor_in(&self.connect()?, automation_id)
    }

    /// Reserve or durably refuse work. A scheduled occurrence is never queued.
    pub(in crate::automations) fn admit(
        &self,
        definition: &AutomationDefinition,
        trigger: RunTrigger,
        max_concurrent_runs: u32,
        now_ms: i64,
    ) -> Result<Option<AutomationRun>, String> {
        Ok(self
            .admit_batch(&[(definition, trigger)], max_concurrent_runs, now_ms)?
            .pop())
    }

    /// Commit every decision together so an error cannot strand undispatched runs.
    pub(in crate::automations) fn admit_batch(
        &self,
        candidates: &[(&AutomationDefinition, RunTrigger)],
        max_concurrent_runs: u32,
        now_ms: i64,
    ) -> Result<Vec<AutomationRun>, String> {
        self.require_owner()?;
        if max_concurrent_runs == 0 {
            return Err("Automation concurrency must be greater than zero".into());
        }
        let mut conn = self.connect()?;
        let tx = conn
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .map_err(error)?;
        let mut decisions = Vec::new();
        for (definition, trigger) in candidates {
            definition.validate()?;
            if let Some(run) = admit_in(
                &tx,
                definition,
                trigger.clone(),
                max_concurrent_runs,
                now_ms,
            )? {
                decisions.push(run);
            }
        }
        tx.commit().map_err(error)?;
        Ok(decisions)
    }
}

fn admit_in(
    tx: &Connection,
    definition: &AutomationDefinition,
    trigger: RunTrigger,
    max_concurrent_runs: u32,
    now_ms: i64,
) -> Result<Option<AutomationRun>, String> {
    let mut missed = false;
    if let RunTrigger::Scheduled { occurrence_ms } = trigger {
        if !definition.enabled {
            return Ok(None);
        }
        if occurrence_ms > now_ms {
            return Err("Automation occurrence is in the future".into());
        }
        let cursor = scheduled_cursor_in(tx, &definition.id)?;
        if cursor.is_some_and(|cursor| occurrence_ms <= cursor) {
            return Ok(None);
        }
        tx.execute("INSERT INTO automation_cursors(automation_id,occurrence_ms) VALUES(?1,?2) ON CONFLICT(automation_id) DO UPDATE SET occurrence_ms=excluded.occurrence_ms", params![definition.id,occurrence_ms]).map_err(error)?;
        missed = i128::from(now_ms) - i128::from(occurrence_ms)
            > i128::from(definition.grace_secs) * 1000;
    }
    let initial_status = if missed {
        RunStatus::SkippedMissed
    } else {
        let overlap: bool = tx.query_row(
                &format!("SELECT EXISTS(SELECT 1 FROM automation_runs WHERE automation_id=?1 AND status IN {OPEN})"),
                [&definition.id], |r| r.get(0),
            ).map_err(error)?;
        if overlap {
            RunStatus::SkippedOverlap
        } else {
            let open: i64 = tx
                .query_row(
                    &format!("SELECT COUNT(*) FROM automation_runs WHERE status IN {OPEN}"),
                    [],
                    |r| r.get(0),
                )
                .map_err(error)?;
            if open >= i64::from(max_concurrent_runs) {
                RunStatus::SkippedConcurrency
            } else {
                RunStatus::Reserved
            }
        }
    };
    reserve_in(tx, definition, trigger, initial_status, now_ms)
}

fn scheduled_cursor_in(conn: &Connection, automation_id: &str) -> Result<Option<i64>, String> {
    // Include lower-level ledger reservations; never count manual history.
    conn.query_row(
        "SELECT MAX(occurrence_ms) FROM (SELECT occurrence_ms FROM automation_cursors WHERE automation_id=?1 UNION ALL SELECT MAX(occurrence_ms) AS occurrence_ms FROM automation_runs WHERE automation_id=?1)",
        [automation_id], |r| r.get(0),
    ).map_err(error)
}
