//! Shared gate of the opt-in live suite. A live test never passes by mock: it
//! either runs against the real endpoint or reports why it could not.
//!
//! Every test ends with exactly one `WF_LIVE_RESULT {json}` record (stdout, and
//! the file named by `TUIC_LIVE_RESULTS` when set). `scripts/workflow-live.sh`
//! collects those records into the evidence file. A panic before `finish`
//! records `failed`, so a crash is never read as a skip.
use serde_json::{Value, json};
use std::io::Write;

/// Real model runs this suite may spend in total, counted across processes.
pub(super) const MODEL_BUDGET: u32 = 3;

pub(super) struct Live {
    test: &'static str,
    done: bool,
}

impl Live {
    pub(super) fn start(test: &'static str) -> Self {
        Self { test, done: false }
    }

    /// Names of the variables that are unset or empty. Values are never read
    /// into a message.
    pub(super) fn missing(vars: &[&str]) -> Vec<String> {
        vars.iter()
            .filter(|v| std::env::var(v).map_or(true, |s| s.trim().is_empty()))
            .map(|v| (*v).to_owned())
            .collect()
    }

    /// True when the test must not run; its record is already written.
    pub(super) fn gate(&mut self, vars: &[&str], result: &str) -> bool {
        let missing = Self::missing(vars);
        if missing.is_empty() {
            return false;
        }
        self.finish(
            result,
            json!({"reason": format!("{result}: missing {}", missing.join(", "))}),
        );
        true
    }

    pub(super) fn finish(&mut self, result: &str, extra: Value) {
        self.done = true;
        emit(self.test, result, extra);
    }
}

impl Drop for Live {
    fn drop(&mut self) {
        if !self.done {
            emit(
                self.test,
                "failed",
                json!({"reason": "panicked before a result"}),
            );
        }
    }
}

fn emit(test: &str, result: &str, extra: Value) {
    let mut record = json!({"test": test, "result": result});
    if let (Some(map), Some(more)) = (record.as_object_mut(), extra.as_object()) {
        map.extend(more.clone());
    }
    println!("WF_LIVE_RESULT {record}");
    if let Some(path) = std::env::var_os("TUIC_LIVE_RESULTS")
        && let Ok(mut file) = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(path)
    {
        let _ = writeln!(file, "{record}");
    }
}

/// Reserve one real model run in the persistent counter named by
/// `TUIC_LIVE_BUDGET_FILE`. `Err` once `MODEL_BUDGET` runs are spent.
pub(super) fn spend_model_run(file: &std::path::Path) -> Result<u32, String> {
    let used: u32 = std::fs::read_to_string(file)
        .ok()
        .and_then(|s| s.trim().parse().ok())
        .unwrap_or(0);
    if used >= MODEL_BUDGET {
        return Err(format!("model budget spent: {used}/{MODEL_BUDGET} runs"));
    }
    std::fs::write(file, (used + 1).to_string()).map_err(|e| e.to_string())?;
    Ok(used + 1)
}
