//! Execution state of stories whose content an external backend owns.
//!
//! The adapter holds title, criteria, checks, status and dependencies. TUIC keeps
//! what the protocol has no field for, keyed by adapter id + story id: the claim,
//! the review/QA sub-state over the adapter's `in_progress`, the transition
//! history, and the integration receipt that a TUIC approval writes. The rows
//! never reference the native `stories` table.

use super::*;

pub(super) const SCHEMA: &str = "
    CREATE TABLE IF NOT EXISTS external_story_execution (
        adapter_id TEXT NOT NULL,
        story_id TEXT NOT NULL,
        sub_state TEXT CHECK(sub_state IN ('review','qa')),
        claim_session TEXT,
        receipt_version TEXT,
        PRIMARY KEY(adapter_id, story_id)
    );
    CREATE UNIQUE INDEX IF NOT EXISTS one_external_story_per_session
      ON external_story_execution(claim_session) WHERE claim_session IS NOT NULL;
    CREATE TABLE IF NOT EXISTS external_story_transitions (
        adapter_id TEXT NOT NULL,
        story_id TEXT NOT NULL,
        seq INTEGER NOT NULL,
        version TEXT NOT NULL,
        command_json TEXT NOT NULL,
        actor_json TEXT NOT NULL,
        PRIMARY KEY(adapter_id, story_id, seq)
    );
    CREATE TABLE IF NOT EXISTS external_story_proposals (
        adapter_id TEXT NOT NULL,
        run_id TEXT NOT NULL,
        proposal_key TEXT NOT NULL,
        story_id TEXT NOT NULL,
        input_sha256 TEXT NOT NULL,
        origin_json TEXT NOT NULL,
        file_scope_json TEXT NOT NULL,
        PRIMARY KEY(adapter_id, run_id, proposal_key)
    );
";

/// TUIC-owned state of one external story. The default is "nothing recorded".
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ExternalExecution {
    /// `Review` or `Qa` while the adapter reports `in_progress`.
    pub sub_state: Option<StoryStatus>,
    pub claim_session: Option<String>,
    /// Adapter version of the story when TUIC approved it: the integration receipt.
    pub receipt_version: Option<String>,
}

/// One committed TUIC transition of an external story.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ExternalTransition {
    pub version: String,
    pub command: StoryCommand,
    pub actor: StoryTransitionActor,
}

fn sub_state_text(status: Option<StoryStatus>) -> Result<Option<&'static str>, String> {
    match status {
        None => Ok(None),
        Some(StoryStatus::Review) => Ok(Some("review")),
        Some(StoryStatus::Qa) => Ok(Some("qa")),
        Some(other) => Err(format!(
            "external sub-state must be review or qa, not {}",
            other.as_str()
        )),
    }
}

impl StoryStore {
    /// The adapter story that proposal (`run_id`, `proposal_key`) created, if any.
    /// The same key with another payload is an error, as in the native store.
    pub fn external_proposal(
        &self,
        adapter_id: &str,
        run_id: &str,
        proposal_key: &str,
        input: &NewStory,
    ) -> Result<Option<String>, String> {
        validate_text("workflow run id", run_id, 128)?;
        validate_text("story proposal key", proposal_key, 128)?;
        let hash = proposal_hash(input)?;
        let conn = self.connect()?;
        let prior: Option<(String, String)> = conn
            .query_row(
                "SELECT story_id,input_sha256 FROM external_story_proposals
                 WHERE adapter_id=?1 AND run_id=?2 AND proposal_key=?3",
                params![adapter_id, run_id, proposal_key],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .optional()
            .map_err(|e| format!("read external story proposal: {e}"))?;
        match prior {
            Some((story_id, prior_hash)) if prior_hash == hash => Ok(Some(story_id)),
            Some(_) => Err("story proposal key was reused with a different payload".into()),
            None => Ok(None),
        }
    }

    /// Records that the adapter created `story_id` for the proposal. The origin and
    /// file scope are TUIC-only: the protocol has no field for them.
    pub fn record_external_proposal(
        &self,
        adapter_id: &str,
        run_id: &str,
        proposal_key: &str,
        story_id: &str,
        input: &NewStory,
    ) -> Result<(), String> {
        let origin = serde_json::to_string(&input.origin)
            .map_err(|e| format!("encode proposal origin: {e}"))?;
        let scope = serde_json::to_string(&input.file_scope)
            .map_err(|e| format!("encode proposal file scope: {e}"))?;
        self.connect()?
            .execute(
                "INSERT INTO external_story_proposals
                 (adapter_id,run_id,proposal_key,story_id,input_sha256,origin_json,file_scope_json)
                 VALUES (?1,?2,?3,?4,?5,?6,?7)",
                params![
                    adapter_id,
                    run_id,
                    proposal_key,
                    story_id,
                    proposal_hash(input)?,
                    origin,
                    scope
                ],
            )
            .map_err(|e| format!("record external story proposal: {e}"))?;
        Ok(())
    }

    /// Origin and file scope of every proposal story of `adapter_id`, by story id.
    pub fn external_proposal_shapes(
        &self,
        adapter_id: &str,
    ) -> Result<HashMap<String, (StoryOrigin, Vec<String>)>, String> {
        let conn = self.connect()?;
        let mut stmt = conn
            .prepare(
                "SELECT story_id,origin_json,file_scope_json
                 FROM external_story_proposals WHERE adapter_id=?1",
            )
            .map_err(|e| format!("prepare external proposals: {e}"))?;
        let rows = stmt
            .query_map([adapter_id], |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, String>(2)?,
                ))
            })
            .map_err(|e| format!("read external proposals: {e}"))?;
        rows.map(|row| {
            let (story_id, origin, scope) =
                row.map_err(|e| format!("read external proposal: {e}"))?;
            Ok((
                story_id,
                (
                    serde_json::from_str(&origin)
                        .map_err(|e| format!("decode proposal origin: {e}"))?,
                    serde_json::from_str(&scope)
                        .map_err(|e| format!("decode proposal file scope: {e}"))?,
                ),
            ))
        })
        .collect()
    }

    /// Every recorded execution of `adapter_id`, by story id.
    pub fn external_executions(
        &self,
        adapter_id: &str,
    ) -> Result<HashMap<String, ExternalExecution>, String> {
        let conn = self.connect()?;
        let mut stmt = conn
            .prepare(
                "SELECT story_id,sub_state,claim_session,receipt_version
                 FROM external_story_execution WHERE adapter_id=?1",
            )
            .map_err(|e| format!("prepare external executions: {e}"))?;
        let rows = stmt
            .query_map([adapter_id], |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, Option<String>>(1)?,
                    row.get::<_, Option<String>>(2)?,
                    row.get::<_, Option<String>>(3)?,
                ))
            })
            .map_err(|e| format!("read external executions: {e}"))?;
        rows.map(|row| {
            let (story_id, sub_state, claim_session, receipt_version) =
                row.map_err(|e| format!("read external execution: {e}"))?;
            let sub_state = match sub_state.as_deref() {
                None => None,
                Some("review") => Some(StoryStatus::Review),
                Some("qa") => Some(StoryStatus::Qa),
                Some(other) => return Err(format!("unknown external sub-state {other}")),
            };
            Ok((
                story_id,
                ExternalExecution {
                    sub_state,
                    claim_session,
                    receipt_version,
                },
            ))
        })
        .collect()
    }

    /// Replaces `expected` with `execution` and appends the transition to the
    /// history, in one transaction; the claims of `release_claims` (stories the
    /// adapter no longer reports in progress) are cleared in the same transaction.
    /// A row that no longer equals `expected` is a
    /// concurrent TUIC transition: the call fails as a stale revision.
    pub fn record_external_transition(
        &self,
        adapter_id: &str,
        story_id: &str,
        expected: &ExternalExecution,
        execution: &ExternalExecution,
        transition: &ExternalTransition,
        release_claims: &[String],
    ) -> Result<(), String> {
        let sub_state = sub_state_text(execution.sub_state)?;
        let mut conn = self.connect()?;
        let tx = immediate(&mut conn)?;
        let current = tx
            .query_row(
                "SELECT sub_state,claim_session,receipt_version FROM external_story_execution
                 WHERE adapter_id=?1 AND story_id=?2",
                params![adapter_id, story_id],
                |row| {
                    Ok((
                        row.get::<_, Option<String>>(0)?,
                        row.get::<_, Option<String>>(1)?,
                        row.get::<_, Option<String>>(2)?,
                    ))
                },
            )
            .optional()
            .map_err(|e| format!("read external execution: {e}"))?
            .unwrap_or_default();
        let expected_row = (
            sub_state_text(expected.sub_state)?.map(String::from),
            expected.claim_session.clone(),
            expected.receipt_version.clone(),
        );
        if current != expected_row {
            return Err(
                "stale story revision: its TUIC execution state changed; reload and retry".into(),
            );
        }
        for released in release_claims {
            tx.execute(
                "UPDATE external_story_execution SET claim_session=NULL
                 WHERE adapter_id=?1 AND story_id=?2",
                params![adapter_id, released],
            )
            .map_err(|e| format!("release external claim: {e}"))?;
        }
        tx.execute(
            "INSERT INTO external_story_execution
               (adapter_id,story_id,sub_state,claim_session,receipt_version)
             VALUES (?1,?2,?3,?4,?5)
             ON CONFLICT(adapter_id,story_id) DO UPDATE SET
               sub_state=excluded.sub_state, claim_session=excluded.claim_session,
               receipt_version=excluded.receipt_version",
            params![
                adapter_id,
                story_id,
                sub_state,
                execution.claim_session,
                execution.receipt_version
            ],
        )
        .map_err(|e| match e.sqlite_error_code() {
            Some(rusqlite::ErrorCode::ConstraintViolation) => {
                "session already claims another story".to_string()
            }
            _ => format!("record external execution: {e}"),
        })?;
        tx.execute(
            "INSERT INTO external_story_transitions
               (adapter_id,story_id,seq,version,command_json,actor_json)
             VALUES (?1,?2,
               (SELECT COALESCE(MAX(seq),0)+1 FROM external_story_transitions
                WHERE adapter_id=?1 AND story_id=?2),
               ?3,?4,?5)",
            params![
                adapter_id,
                story_id,
                transition.version,
                serde_json::to_string(&transition.command)
                    .map_err(|e| format!("encode story command: {e}"))?,
                serde_json::to_string(&transition.actor)
                    .map_err(|e| format!("encode story actor: {e}"))?,
            ],
        )
        .map_err(|e| format!("record external transition: {e}"))?;
        tx.commit()
            .map_err(|e| format!("commit external transition: {e}"))
    }

    pub fn external_transition_history(
        &self,
        adapter_id: &str,
        story_id: &str,
    ) -> Result<Vec<ExternalTransition>, String> {
        let conn = self.connect()?;
        let mut stmt = conn
            .prepare(
                "SELECT version,command_json,actor_json FROM external_story_transitions
                 WHERE adapter_id=?1 AND story_id=?2 ORDER BY seq",
            )
            .map_err(|e| format!("prepare external transition history: {e}"))?;
        let rows = stmt
            .query_map(params![adapter_id, story_id], |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, String>(2)?,
                ))
            })
            .map_err(|e| format!("read external transition history: {e}"))?;
        rows.map(|row| {
            let (version, command, actor) =
                row.map_err(|e| format!("read external transition: {e}"))?;
            Ok(ExternalTransition {
                version,
                command: serde_json::from_str(&command)
                    .map_err(|e| format!("decode story command: {e}"))?,
                actor: serde_json::from_str(&actor)
                    .map_err(|e| format!("decode story actor: {e}"))?,
            })
        })
        .collect()
    }
}
