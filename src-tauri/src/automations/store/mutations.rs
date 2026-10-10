use super::*;

impl RunStore {
    /// Only a fresh reservation may perform effects. Commit the claim before
    /// resolving a workspace, so concurrent dispatch calls cannot spawn twice.
    pub(in crate::automations) fn claim_dispatch(
        &self,
        id: &str,
        now_ms: i64,
    ) -> Result<Option<AutomationRun>, String> {
        self.require_owner()?;
        let mut conn = self.connect()?;
        let tx = conn
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .map_err(error)?;
        let mut run = read(&tx, id)?;
        if run.status != RunStatus::Reserved {
            return Ok(None);
        }
        run.status = RunStatus::Prechecking;
        run.updated_ms = now_ms.max(run.updated_ms);
        write(&tx, &run)?;
        tx.commit().map_err(error)?;
        Ok(Some(run))
    }

    /// Apply observed execution state; the first terminal result is immutable.
    pub fn transition(
        &self,
        id: &str,
        next: RunStatus,
        details: RunDetails,
        now_ms: i64,
    ) -> Result<AutomationRun, String> {
        self.require_owner()?;
        let mut conn = self.connect()?;
        let tx = conn
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .map_err(error)?;
        let mut run = read(&tx, id)?;
        if !run.status.is_open() {
            return Ok(run);
        }
        if now_ms < run.updated_ms {
            return Err("run transition timestamp precedes its latest state".into());
        }
        // Active state cannot move backwards to reservation or precheck after launch.
        if (next == RunStatus::Reserved && run.status != next)
            || (next == RunStatus::Prechecking
                && matches!(run.status, RunStatus::Running | RunStatus::NeedsYou))
        {
            return Err("invalid backwards automation transition".into());
        }
        run.status = next;
        run.updated_ms = now_ms;
        if !next.is_open() {
            run.finished_ms = Some(now_ms);
        }
        if let Some(value) = details.task_id {
            run.task_id = Some(value);
        }
        if let Some(value) = details.session_id {
            run.session_id = Some(value);
        }
        if let Some(value) = details.workspace {
            run.workspace = Some(value);
        }
        if let Some(value) = details.workspace_id {
            run.workspace_id = Some(value);
        }
        if let Some(value) = details.stdout {
            run.stdout = SavedOutput::bounded(&value);
        }
        if let Some(value) = details.stderr {
            run.stderr = SavedOutput::bounded(&value);
        }
        if let Some(mut value) = details.precheck {
            let stdout = SavedOutput::bounded(&value.stdout.text);
            let stderr = SavedOutput::bounded(&value.stderr.text);
            value.stdout = SavedOutput {
                truncated: value.stdout.truncated || stdout.truncated,
                ..stdout
            };
            value.stderr = SavedOutput {
                truncated: value.stderr.truncated || stderr.truncated,
                ..stderr
            };
            run.precheck = Some(value);
        }
        if let Some(value) = details.reason {
            run.reason = Some(value);
        }
        if let Some(mut value) = details.precheck_outcome {
            if let super::super::precheck::PrecheckOutcome::Executed(result) = &mut value {
                let stdout = SavedOutput::bounded(&result.stdout);
                let stderr = SavedOutput::bounded(&result.stderr);
                result.stdout = stdout.text;
                result.stderr = stderr.text;
                result.stdout_truncated |= stdout.truncated;
                result.stderr_truncated |= stderr.truncated;
            }
            run.precheck_outcome = Some(value);
        }
        write(&tx, &run)?;
        tx.commit().map_err(error)?;
        Ok(run)
    }

    /// Interrupt all open runs in one commit. Recovery never returns work to dispatch.
    pub(super) fn interrupt_open(&self, now_ms: i64) -> Result<(), String> {
        self.require_owner()?;
        let mut conn = self.connect()?;
        let tx = conn
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .map_err(error)?;
        let runs = {
            let mut stmt = tx
                .prepare(&format!(
                    "SELECT snapshot_json FROM automation_runs WHERE status IN {OPEN}"
                ))
                .map_err(error)?;
            stmt.query_map([], |r| r.get::<_, String>(0))
                .map_err(error)?
                .map(|row| decode::<AutomationRun>(row.map_err(error)?))
                .collect::<Result<Vec<_>, String>>()?
        };
        for mut run in runs {
            run.status = RunStatus::Interrupted;
            run.updated_ms = now_ms.max(run.updated_ms);
            run.finished_ms = Some(run.updated_ms);
            run.reason =
                Some("Application restarted before execution finished; not retried".into());
            write(&tx, &run)?;
        }
        // An attempted send may already have arrived. Mark it unknown, never claim again.
        let deliveries = {
            let mut stmt = tx
                .prepare("SELECT document_json FROM automation_deliveries")
                .map_err(error)?;
            stmt.query_map([], |r| r.get::<_, String>(0))
                .map_err(error)?
                .map(|row| decode::<NotificationDelivery>(row.map_err(error)?))
                .collect::<Result<Vec<_>, String>>()?
        };
        for mut delivery in deliveries {
            if delivery.status == DeliveryStatus::Attempted {
                delivery.status = DeliveryStatus::Unknown;
                delivery.settled_ms = Some(now_ms.max(delivery.attempted_ms));
                save_delivery(&tx, &delivery)?;
            }
        }
        tx.commit().map_err(error)
    }

    /// Claim before sending; ambiguous or historical sends can never be replayed.
    pub fn claim_notification(
        &self,
        id: &str,
        transition: &str,
        channel: &str,
        now_ms: i64,
    ) -> Result<bool, String> {
        self.require_owner()?;
        if transition.trim().is_empty() || channel.trim().is_empty() {
            return Err("notification transition and channel required".into());
        }
        let delivery = NotificationDelivery {
            run_id: id.into(),
            transition: transition.into(),
            channel: channel.into(),
            status: DeliveryStatus::Attempted,
            attempted_ms: now_ms,
            settled_ms: None,
        };
        let conn = self.connect()?;
        let inserted = conn.execute("INSERT INTO automation_deliveries(run_id,transition,channel,document_json) VALUES(?1,?2,?3,?4) ON CONFLICT DO NOTHING", params![id,transition,channel,encode(&delivery)?]).map_err(error)?;
        Ok(inserted == 1)
    }

    /// Settle delivery separately; execution state and canonical output remain unchanged.
    pub fn settle_notification(
        &self,
        id: &str,
        transition: &str,
        channel: &str,
        result: DeliveryStatus,
        now_ms: i64,
    ) -> Result<NotificationDelivery, String> {
        self.require_owner()?;
        if result == DeliveryStatus::Attempted {
            return Err("notification settlement must be confirmed or unknown".into());
        }
        let mut conn = self.connect()?;
        let tx = conn
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .map_err(error)?;
        let mut delivery = self.read_delivery(&tx, id, transition, channel)?;
        if delivery.status == DeliveryStatus::Attempted {
            if now_ms < delivery.attempted_ms {
                return Err("notification settlement precedes attempt".into());
            }
            delivery.status = result;
            delivery.settled_ms = Some(now_ms);
            save_delivery(&tx, &delivery)?;
        }
        tx.commit().map_err(error)?;
        Ok(delivery)
    }

    pub fn notification(
        &self,
        id: &str,
        transition: &str,
        channel: &str,
    ) -> Result<NotificationDelivery, String> {
        self.read_delivery(&self.connect()?, id, transition, channel)
    }

    fn read_delivery(
        &self,
        conn: &Connection,
        id: &str,
        transition: &str,
        channel: &str,
    ) -> Result<NotificationDelivery, String> {
        let json = conn.query_row("SELECT document_json FROM automation_deliveries WHERE run_id=?1 AND transition=?2 AND channel=?3", params![id,transition,channel], |r| r.get(0)).map_err(error)?;
        decode(json)
    }
}

fn save_delivery(conn: &Connection, delivery: &NotificationDelivery) -> Result<(), String> {
    conn.execute("UPDATE automation_deliveries SET document_json=?4 WHERE run_id=?1 AND transition=?2 AND channel=?3", params![delivery.run_id,delivery.transition,delivery.channel,encode(delivery)?]).map_err(error)?;
    Ok(())
}
