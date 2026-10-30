//! A report's acceptance is evidence, not approval or permission to close its worker.
//! Accepted decisions share the event transaction; refused payloads retain their
//! own digest so a conflicting retry cannot overwrite the original decision.
use super::*;

pub(super) fn persist(
    conn: &Connection,
    report: &AttemptReport,
    caller: &str,
    caller_acp: Option<&AcpAgentBinding>,
    outcome: Result<i64, &str>,
    at_ms: i64,
) -> Result<(), String> {
    let bytes = serde_json::to_vec(&(caller, caller_acp, report)).map_err(|e| e.to_string())?;
    let digest = hex::encode(Sha256::digest(bytes));
    let (decision, sequence, reason) = match outcome {
        Ok(sequence) => (ReportDecision::Accepted, Some(sequence), None),
        Err(reason) => (ReportDecision::Refused, None, Some(reason.to_owned())),
    };
    // A refused payload can become valid after an explicit state correction.
    // Keep both decisions, while identical retries retain one decision row.
    let decision_key = hex::encode(Sha256::digest(
        serde_json::to_vec(&(&digest, &decision, sequence, &reason)).map_err(|e| e.to_string())?,
    ));
    let disposition = ReportDisposition {
        submission_sha256: digest.clone(),
        run_id: report.run_id.clone(),
        attempt_id: report.attempt_id.clone(),
        generation: report.generation,
        caller_session: caller.into(),
        caller_acp: caller_acp.cloned(),
        decision,
        sequence,
        reason,
        received_at_ms: at_ms,
    };
    conn.execute(
        "INSERT OR IGNORE INTO workflow_report_dispositions(run_id,decision_key,document_json) VALUES (?1,?2,?3)",
        params![report.run_id, decision_key, serde_json::to_string(&disposition).map_err(|e| e.to_string())?],
    ).map_err(|e| format!("persist workflow report disposition: {e}"))?;
    Ok(())
}

#[cfg(test)]
#[path = "report_disposition_tests.rs"]
mod tests;

impl RunStore {
    pub fn report_dispositions(&self, run_id: &str) -> Result<Vec<ReportDisposition>, String> {
        self.snapshot(run_id)?;
        let conn = self.connect()?;
        let mut statement = conn.prepare(
            "SELECT document_json FROM workflow_report_dispositions WHERE run_id=?1 ORDER BY rowid",
        ).map_err(|e| format!("read workflow report dispositions: {e}"))?;
        statement
            .query_map([run_id], |row| row.get::<_, String>(0))
            .map_err(|e| format!("query workflow report dispositions: {e}"))?
            .map(|row| decode(&row.map_err(|e| format!("read workflow report disposition: {e}"))?))
            .collect()
    }
}
