//! Sequential batches are retained ledger data and joins over owned children, not a
//! scheduler. Evidence (push and critic report) is verified in `native_batch_evidence`.
use super::RunStore;
use super::native_cohort::Cohort;
use super::neutral_effects::EffectKey;
use rusqlite::{Connection, OptionalExtension, Transaction, TransactionBehavior, params};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::path::Path;

pub(super) const PENDING: &str = "native.story.batch.pending";
pub(super) const ADMIT: &str = "native.story.batch.admit";
pub(super) const LAND: &str = "native.story.batch.land";
pub(super) const PIN: &str = "native.story.batch.pin";
pub(super) const VERDICT: &str = "native.story.batch.verdict";
pub(super) const HANDLERS: [&str; 5] = [PENDING, ADMIT, LAND, PIN, VERDICT];
pub(super) const ONE_SLOT: &str = "sequential batch admits one story at a time";

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct PendingRequest {
    pub cohort: Cohort,
    pub limit: u32,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct AdmitRequest {
    pub cohort: Cohort,
    pub story: String,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct LandRequest {
    pub plan: String,
    pub story: String,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct PinRequest {
    pub plan: String,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct VerdictRequest {
    pub plan: String,
    pub generation: u32,
    pub report: EffectKey,
    pub source: super::native_agent::Source,
}

/// A verified critic verdict over one exact artifact. `fix` names the FIX owner.
pub(super) struct Critic {
    pub report: String,
    pub commit: String,
    pub tree: String,
    pub fix: Option<String>,
}

fn connect(store: &RunStore) -> Result<Connection, String> {
    let conn = Connection::open(store.database_path()).map_err(|e| e.to_string())?;
    conn.busy_timeout(std::time::Duration::from_secs(30))
        .map_err(|e| e.to_string())?;
    conn.execute_batch(
        "CREATE TABLE IF NOT EXISTS native_batch_receipts (
         run TEXT NOT NULL, effect TEXT NOT NULL, receipt TEXT NOT NULL, PRIMARY KEY(run,effect));
         CREATE TABLE IF NOT EXISTS native_batch_cohorts (
         run TEXT NOT NULL, plan TEXT NOT NULL, cohort TEXT NOT NULL, PRIMARY KEY(run,plan));
         CREATE TABLE IF NOT EXISTS native_batch_members (
         run TEXT NOT NULL, plan TEXT NOT NULL, story TEXT NOT NULL, generation INTEGER NOT NULL,
         admit TEXT, known TEXT, landing TEXT, landed_commit TEXT, batch INTEGER,
         PRIMARY KEY(run,plan,story,generation), UNIQUE(run,landing));
         CREATE TABLE IF NOT EXISTS native_batch_pins (
         run TEXT NOT NULL, plan TEXT NOT NULL, generation INTEGER NOT NULL, members TEXT NOT NULL,
         commit_id TEXT NOT NULL, tree TEXT NOT NULL, PRIMARY KEY(run,plan,generation));
         CREATE TABLE IF NOT EXISTS native_batch_verdicts (
         run TEXT NOT NULL, plan TEXT NOT NULL, generation INTEGER NOT NULL, fix_story TEXT,
         report TEXT NOT NULL, PRIMARY KEY(run,plan,generation));",
    )
    .map_err(|e| e.to_string())?;
    Ok(conn)
}

fn sql<T>(r: rusqlite::Result<T>) -> Result<T, String> {
    r.map_err(|e| e.to_string())
}

/// The receipt this effect already recorded; a repeat delivery never writes again.
pub(super) fn retained(store: &RunStore, run: &str, effect: &str) -> Result<Option<Value>, String> {
    let conn = connect(store)?;
    receipt_of(&conn, run, effect)
}

fn receipt_of(conn: &Connection, run: &str, effect: &str) -> Result<Option<Value>, String> {
    let text: Option<String> = sql(conn
        .query_row(
            "SELECT receipt FROM native_batch_receipts WHERE run=?1 AND effect=?2",
            params![run, effect],
            |r| r.get(0),
        )
        .optional())?;
    text.map(|t| serde_json::from_str(&t).map_err(|e| e.to_string()))
        .transpose()
}

fn record(
    store: &RunStore,
    run: &str,
    effect: &str,
    op: impl FnOnce(&Transaction) -> Result<Value, String>,
) -> Result<Value, String> {
    let mut conn = connect(store)?;
    let tx = sql(conn.transaction_with_behavior(TransactionBehavior::Immediate))?;
    if let Some(receipt) = receipt_of(&tx, run, effect)? {
        return Ok(receipt);
    }
    let receipt = op(&tx)?;
    sql(tx.execute(
        "INSERT INTO native_batch_receipts VALUES (?1,?2,?3)",
        params![run, effect, receipt.to_string()],
    ))?;
    sql(tx.commit())?;
    Ok(receipt)
}

fn count(tx: &Transaction, run: &str, plan: &str, filter: &str) -> Result<i64, String> {
    sql(tx.query_row(
        &format!("SELECT count(*) FROM native_batch_members WHERE run=?1 AND plan=?2 AND {filter}"),
        params![run, plan],
        |r| r.get(0),
    ))
}

fn started(tx: &Transaction, run: &str, plan: &str, story: &str) -> Result<bool, String> {
    sql(tx.query_row(
        "SELECT count(*) FROM native_batch_members WHERE run=?1 AND plan=?2 AND story=?3",
        params![run, plan, story],
        |r| r.get::<_, i64>(0),
    ))
    .map(|n| n > 0)
}

/// True while this run's ledger holds an armed or running FIX re-entry (generation
/// 2 or later, not landed) of `story`.
pub(super) fn reentry_open(store: &RunStore, run: &str, story: &str) -> bool {
    connect(store)
        .and_then(|conn| {
            sql(conn.query_row(
                "SELECT count(*) FROM native_batch_members WHERE run=?1 AND story=?2
                 AND generation > 1 AND landing IS NULL",
                params![run, story],
                |r| r.get::<_, i64>(0),
            ))
        })
        .is_ok_and(|n| n > 0)
}

/// True while `story` holds this run's batch slot (admitted, not yet landed).
pub(super) fn admitted(store: &RunStore, run: &str, story: &str) -> bool {
    connect(store)
        .and_then(|conn| {
            sql(conn.query_row(
                "SELECT count(*) FROM native_batch_members WHERE run=?1 AND story=?2
                 AND admit IS NOT NULL AND landing IS NULL",
                params![run, story],
                |r| r.get::<_, i64>(0),
            ))
        })
        .is_ok_and(|n| n > 0)
}

/// Retain a read snapshot under the original effect as well: recovery observes
/// the same pending selection even after later rounds change the ledger.
pub(super) fn retain_pending(
    store: &RunStore,
    run: &str,
    effect: &str,
    receipt: Value,
) -> Result<Value, String> {
    record(store, run, effect, |_| Ok(receipt))
}

/// Select one round without changing the retained admission cohort. An armed FIX
/// is its own round; otherwise take unstarted members in original cohort order.
pub(super) fn pending(
    store: &RunStore,
    run: &str,
    cohort: &Cohort,
    limit: u32,
) -> Result<Vec<super::native_cohort::StoryPin>, String> {
    cohort.validate()?;
    if !(1..=128).contains(&limit) {
        return Err("batch pending limit must be between 1 and 128".into());
    }
    let mut conn = connect(store)?;
    let tx = sql(conn.transaction())?;
    let retained: Option<String> = sql(tx
        .query_row(
            "SELECT cohort FROM native_batch_cohorts WHERE run=?1 AND plan=?2",
            params![run, cohort.plan],
            |r| r.get(0),
        )
        .optional())?;
    if let Some(retained) = retained {
        let retained: Cohort = serde_json::from_str(&retained).map_err(|e| e.to_string())?;
        if retained != *cohort {
            return Err("batch cohort cannot change within run/plan".into());
        }
    }
    if count(
        &tx,
        run,
        &cohort.plan,
        "admit IS NOT NULL AND landing IS NULL",
    )? > 0
    {
        return Err(ONE_SLOT.into());
    }
    let armed: Option<String> = sql(tx
        .query_row(
            "SELECT story FROM native_batch_members WHERE run=?1 AND plan=?2 AND admit IS NULL",
            params![run, cohort.plan],
            |r| r.get(0),
        )
        .optional())?;
    if let Some(armed) = armed {
        return cohort
            .stories
            .iter()
            .find(|s| s.id == armed)
            .cloned()
            .map(|s| vec![s])
            .ok_or_else(|| "armed FIX owner is outside the cohort".into());
    }
    let mut pending = Vec::new();
    for story in &cohort.stories {
        if !started(&tx, run, &cohort.plan, &story.id)? {
            pending.push(story.clone());
            if pending.len() == limit as usize {
                break;
            }
        }
    }
    Ok(pending)
}

/// The barrier: cohort order, one admitted-but-unlanded member, no duplicate start.
pub(super) fn admit(
    store: &RunStore,
    run: &str,
    effect: &str,
    cohort: &Cohort,
    story: &str,
    known_pushes: &[String],
) -> Result<Value, String> {
    record(store, run, effect, |tx| {
        cohort.validate()?;
        let plan = cohort.plan.as_str();
        let value = serde_json::to_string(cohort).map_err(|e| e.to_string())?;
        let old: Option<String> = sql(tx
            .query_row(
                "SELECT cohort FROM native_batch_cohorts WHERE run=?1 AND plan=?2",
                params![run, plan],
                |r| r.get(0),
            )
            .optional())?;
        match old {
            Some(old) if old != value => {
                return Err("batch cohort cannot change within run/plan".into());
            }
            Some(_) => {}
            None => {
                sql(tx.execute(
                    "INSERT INTO native_batch_cohorts VALUES (?1,?2,?3)",
                    params![run, plan, value],
                ))?;
            }
        }
        if !cohort.stories.iter().any(|s| s.id == story) {
            return Err("story is not a member of the batch cohort".into());
        }
        if count(tx, run, plan, "admit IS NOT NULL AND landing IS NULL")? > 0 {
            return Err(ONE_SLOT.into());
        }
        let known = serde_json::to_string(known_pushes).map_err(|e| e.to_string())?;
        let armed: Option<(String, u32)> = sql(tx
            .query_row(
                "SELECT story, generation FROM native_batch_members
                 WHERE run=?1 AND plan=?2 AND admit IS NULL",
                params![run, plan],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )
            .optional())?;
        let generation = match armed {
            Some((owner, generation)) if owner == story => {
                sql(tx.execute(
                    "UPDATE native_batch_members SET admit=?5, known=?6
                     WHERE run=?1 AND plan=?2 AND story=?3 AND generation=?4",
                    params![run, plan, story, generation, effect, known],
                ))?;
                generation
            }
            Some(_) => return Err("an armed FIX re-entry owns the next batch slot".into()),
            None => {
                let mut next = None;
                for pin in &cohort.stories {
                    if !started(tx, run, plan, &pin.id)? {
                        next = Some(pin.id.as_str());
                        break;
                    }
                }
                if next != Some(story) {
                    return Err(if started(tx, run, plan, story)? {
                        "story already started in this batch".into()
                    } else {
                        "batch admits cohort members in order".into()
                    });
                }
                sql(tx.execute(
                    "INSERT INTO native_batch_members(run,plan,story,generation,admit,known)
                     VALUES (?1,?2,?3,1,?4,?5)",
                    params![run, plan, story, effect, known],
                ))?;
                1
            }
        };
        Ok(json!({"plan": plan, "story": story, "generation": generation}))
    })
}

/// The landings this run held when `story` took its current slot.
pub(super) fn known_at_admission(
    store: &RunStore,
    run: &str,
    plan: &str,
    story: &str,
) -> Result<Vec<String>, String> {
    let conn = connect(store)?;
    let known: String = sql(conn
        .query_row(
            "SELECT known FROM native_batch_members WHERE run=?1 AND plan=?2 AND story=?3
             AND admit IS NOT NULL AND landing IS NULL",
            params![run, plan, story],
            |r| r.get(0),
        )
        .optional())?
    .ok_or("story holds no admitted batch slot")?;
    serde_json::from_str(&known).map_err(|e| e.to_string())
}

/// Credits the slot holder with one owned push that appeared after its admission.
pub(super) fn land(
    store: &RunStore,
    run: &str,
    effect: &str,
    plan: &str,
    story: &str,
    push: &str,
    commit: &str,
) -> Result<Value, String> {
    record(store, run, effect, |tx| {
        let (generation, known): (u32, String) = sql(tx
            .query_row(
                "SELECT generation, known FROM native_batch_members WHERE run=?1 AND plan=?2
                 AND story=?3 AND admit IS NOT NULL AND landing IS NULL",
                params![run, plan, story],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )
            .optional())?
        .ok_or("story holds no admitted batch slot")?;
        let known: Vec<String> = serde_json::from_str(&known).map_err(|e| e.to_string())?;
        if known.iter().any(|k| k == push) {
            return Err("push predates this member's admission; it cannot release the join".into());
        }
        tx.execute(
            "UPDATE native_batch_members SET landing=?5, landed_commit=?6
             WHERE run=?1 AND plan=?2 AND story=?3 AND generation=?4",
            params![run, plan, story, generation, push, commit],
        )
        .map_err(|_| "push is already credited to another batch member".to_owned())?;
        Ok(
            json!({"plan": plan, "story": story, "generation": generation,
            "commit": commit, "push": push}),
        )
    })
}

/// Pins exactly the landed, unbatched members at the clean containing artifact.
pub(super) fn pin(
    store: &RunStore,
    run: &str,
    effect: &str,
    plan: &str,
    workspace: &Path,
) -> Result<Value, String> {
    record(store, run, effect, |tx| {
        if count(tx, run, plan, "landing IS NULL")? > 0 {
            return Err("a started or armed batch member has not landed".into());
        }
        let mut query = sql(tx.prepare(
            "SELECT story, generation, landed_commit FROM native_batch_members
             WHERE run=?1 AND plan=?2 AND landing IS NOT NULL AND batch IS NULL ORDER BY rowid",
        ))?;
        let members = sql(sql(query.query_map(params![run, plan], |r| {
            Ok((
                r.get::<_, String>(0)?,
                r.get::<_, u32>(1)?,
                r.get::<_, String>(2)?,
            ))
        }))?
        .collect::<rusqlite::Result<Vec<_>>>())?;
        if members.is_empty() {
            return Err("no landed member waits for a batch".into());
        }
        let (commit, tree) = super::check::clean_artifact(workspace)?;
        for (_, _, landed) in &members {
            super::check::git_output(workspace, &["merge-base", "--is-ancestor", landed, &commit])
                .map_err(|_| "pinned artifact does not contain every member landing")?;
        }
        let generation: u32 = sql(tx.query_row(
            "SELECT COALESCE(MAX(generation),0)+1 FROM native_batch_pins WHERE run=?1 AND plan=?2",
            params![run, plan],
            |r| r.get(0),
        ))?;
        let members: Vec<Value> = members
            .into_iter()
            .map(|(story, g, c)| json!({"story": story, "generation": g, "commit": c}))
            .collect();
        sql(tx.execute(
            "INSERT INTO native_batch_pins VALUES (?1,?2,?3,?4,?5,?6)",
            params![
                run,
                plan,
                generation,
                json!(members).to_string(),
                commit,
                tree
            ],
        ))?;
        sql(tx.execute(
            "UPDATE native_batch_members SET batch=?3
             WHERE run=?1 AND plan=?2 AND landing IS NOT NULL AND batch IS NULL",
            params![run, plan, generation],
        ))?;
        Ok(
            json!({"plan": plan, "generation": generation, "members": members,
            "commit": commit, "tree": tree,
            "artifact_digest": review_artifact_digest(&commit, &tree)}),
        )
    })
}

/// The single verdict of the latest pin. A FIX arms its owner's next generation.
pub(super) fn verdict(
    store: &RunStore,
    run: &str,
    effect: &str,
    plan: &str,
    generation: u32,
    critic: &Critic,
) -> Result<Value, String> {
    record(store, run, effect, |tx| {
        let (latest, members, commit, tree): (u32, String, String, String) = sql(tx
            .query_row(
                "SELECT generation, members, commit_id, tree FROM native_batch_pins
                 WHERE run=?1 AND plan=?2 ORDER BY generation DESC LIMIT 1",
                params![run, plan],
                |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?)),
            )
            .optional())?
        .ok_or("batch has no pinned generation")?;
        if latest != generation {
            return Err("a batch verdict must review the latest pinned batch".into());
        }
        if sql(tx.query_row(
            "SELECT count(*) FROM native_batch_verdicts WHERE run=?1 AND plan=?2 AND generation=?3",
            params![run, plan, generation],
            |r| r.get::<_, i64>(0),
        ))? > 0
        {
            return Err("this batch already has its single review".into());
        }
        if critic.commit != commit || critic.tree != tree {
            return Err("the batch critic reviewed another artifact".into());
        }
        let members: Vec<Value> = serde_json::from_str(&members).map_err(|e| e.to_string())?;
        if let Some(owner) = &critic.fix
            && !members.iter().any(|m| m["story"] == owner.as_str())
        {
            return Err("a batch FIX must name a member of the reviewed batch".into());
        }
        sql(tx.execute(
            "INSERT INTO native_batch_verdicts VALUES (?1,?2,?3,?4,?5)",
            params![run, plan, generation, critic.fix, critic.report],
        ))?;
        let mut remaining = Vec::new();
        let outcome = if let Some(owner) = &critic.fix {
            sql(tx.execute(
                "INSERT INTO native_batch_members(run,plan,story,generation)
                 SELECT ?1,?2,?3,MAX(generation)+1 FROM native_batch_members
                 WHERE run=?1 AND plan=?2 AND story=?3",
                params![run, plan, owner],
            ))?;
            "fix"
        } else {
            let cohort: String = sql(tx.query_row(
                "SELECT cohort FROM native_batch_cohorts WHERE run=?1 AND plan=?2",
                params![run, plan],
                |r| r.get(0),
            ))?;
            let cohort: Cohort = serde_json::from_str(&cohort).map_err(|e| e.to_string())?;
            for story in cohort.stories {
                let batched: i64 = sql(tx.query_row(
                    "SELECT count(*) FROM native_batch_members WHERE run=?1 AND plan=?2
                     AND story=?3 AND batch IS NOT NULL",
                    params![run, plan, story.id],
                    |r| r.get(0),
                ))?;
                if batched == 0 {
                    remaining.push(story.id);
                }
            }
            if remaining.is_empty() {
                "completed"
            } else {
                "continue"
            }
        };
        Ok(
            json!({"plan": plan, "generation": generation, "outcome": outcome,
            "approved": critic.fix.is_none(), "fix_story": critic.fix, "repeat": outcome != "completed",
            "report": critic.report, "remaining": remaining}),
        )
    })
}

fn review_artifact_digest(commit: &str, tree: &str) -> String {
    use sha2::Digest;
    hex::encode(sha2::Sha256::digest(format!("{commit}:{tree}").as_bytes()))
}
