use super::*;
use rusqlite::{Transaction, TransactionBehavior};

pub(super) fn immediate(conn: &mut Connection) -> Result<Transaction<'_>, String> {
    conn.transaction_with_behavior(TransactionBehavior::Immediate)
        .map_err(|e| format!("begin story transaction: {e}"))
}

pub(super) fn validate_text(label: &str, value: &str, max: usize) -> Result<(), String> {
    if value.trim().is_empty() {
        return Err(format!("{label} must not be empty"));
    }
    if value.chars().count() > max {
        return Err(format!("{label} exceeds {max} characters"));
    }
    Ok(())
}

pub(super) fn validate_criteria(criteria: &[String]) -> Result<(), String> {
    if criteria.is_empty() || criteria.len() > 100 {
        return Err("story must have between 1 and 100 criteria".into());
    }
    for criterion in criteria {
        validate_text("criterion", criterion, 2000)?;
    }
    Ok(())
}

pub(super) fn validate_scope_path(path: &str) -> Result<(), String> {
    validate_text("file scope path", path, 4096)?;
    if crate::fs::is_absolute_on_any_platform(path)
        || path.contains(':')
        || path.split(['/', '\\']).any(|part| part == "..")
    {
        return Err("file scope must remain relative to the project".into());
    }
    Ok(())
}

pub(super) fn encode(story: &Story) -> Result<String, String> {
    serde_json::to_string(story).map_err(|e| format!("encode story: {e}"))
}

pub(super) fn read_story(conn: &Connection, id: &str) -> Result<Story, String> {
    let document: String = conn
        .query_row("SELECT document FROM stories WHERE id=?1", [id], |row| {
            row.get(0)
        })
        .optional()
        .map_err(|e| format!("read story: {e}"))?
        .ok_or_else(|| format!("story not found: {id}"))?;
    serde_json::from_str(&document).map_err(|e| format!("decode story: {e}"))
}

pub(super) fn read_plan_stories(conn: &Connection, plan_id: &str) -> Result<Vec<Story>, String> {
    let mut stmt = conn
        .prepare("SELECT document FROM stories WHERE plan_id=?1 ORDER BY id")
        .map_err(|e| format!("list stories: {e}"))?;
    let documents = stmt
        .query_map([plan_id], |row| row.get::<_, String>(0))
        .map_err(|e| format!("list stories: {e}"))?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|e| format!("read stories: {e}"))?;
    documents
        .iter()
        .map(|doc| serde_json::from_str(doc).map_err(|e| format!("decode story: {e}")))
        .collect()
}

pub(super) fn check_revision(story: &Story, expected: i64) -> Result<(), String> {
    if story.revision != expected {
        return Err(format!(
            "stale story revision: expected {expected}, current {}",
            story.revision
        ));
    }
    Ok(())
}

pub(super) fn save_story(
    tx: &Transaction<'_>,
    story: &mut Story,
    expected: i64,
) -> Result<(), String> {
    story.revision = expected.checked_add(1).ok_or("story revision exhausted")?;
    let updated = tx.execute(
        "UPDATE stories SET document=?1,status=?2,revision=?3,claim_session=?4 WHERE id=?5 AND revision=?6",
        params![encode(story)?, story.status.as_str(), story.revision, story.claim_session, story.id, expected],
    ).map_err(|e| format!("save story: {e}"))?;
    if updated != 1 {
        return Err("stale story revision".into());
    }
    Ok(())
}

/// Receipt probes and the persisted inputs they observed. Build this before
/// acquiring either the store mutex or an SQLite write transaction.
pub(super) struct DependencyPreflight {
    plan_id: String,
    revisions: Vec<(String, i64)>,
    run_db: PathBuf,
    run_sequences: Vec<(String, i64)>,
    integrated: HashSet<(String, i64)>,
}

impl DependencyPreflight {
    pub(super) fn prepare(store: &StoryStore, plan_id: &str) -> Result<Self, String> {
        let stories = store.list_stories(plan_id)?;
        let run_db = store
            .db_path
            .parent()
            .ok_or("story store has no parent directory")?
            .join("workflow_runs.sqlite3");
        let run_sequences = crate::workflows::plan_run_sequences_in(&run_db, plan_id)?;
        let mut integrated = HashSet::new();
        if !run_sequences.is_empty() {
            for story in &stories {
                if story.status == StoryStatus::Done
                    && crate::workflows::story_integrated_at_revision_in(
                        &run_db,
                        &story.id,
                        story.revision,
                    )?
                {
                    integrated.insert((story.id.clone(), story.revision));
                }
            }
        }
        Ok(Self {
            plan_id: plan_id.into(),
            revisions: story_revisions(&stories),
            run_db,
            run_sequences,
            integrated,
        })
    }

    pub(super) fn validate(&self, conn: &Connection) -> Result<(), String> {
        if story_revisions(&read_plan_stories(conn, &self.plan_id)?) != self.revisions
            || crate::workflows::plan_run_sequences_in(&self.run_db, &self.plan_id)?
                != self.run_sequences
        {
            return Err(
                "story or workflow revisions changed during dependency preflight; retry".into(),
            );
        }
        Ok(())
    }

    fn receipt_current(&self, story: &Story) -> bool {
        self.run_sequences.is_empty()
            || self
                .integrated
                .contains(&(story.id.clone(), story.revision))
    }
}

pub(super) fn story_revisions(stories: &[Story]) -> Vec<(String, i64)> {
    let mut revisions: Vec<_> = stories
        .iter()
        .map(|story| (story.id.clone(), story.revision))
        .collect();
    revisions.sort();
    revisions
}

/// Dependencies that block a story, using already validated preflight receipts.
pub(super) fn unmet_dependencies(
    conn: &Connection,
    story: &Story,
    preflight: &DependencyPreflight,
) -> Result<Vec<String>, String> {
    if story.dependencies.is_empty() {
        return Ok(Vec::new());
    }
    let mut unmet = Vec::new();
    for id in &story.dependencies {
        let dependency = read_story(conn, id)?;
        if dependency.status != StoryStatus::Done || !preflight.receipt_current(&dependency) {
            unmet.push(id.clone());
        }
    }
    Ok(unmet)
}

pub(super) fn dependencies_integrated(
    conn: &Connection,
    story: &Story,
    preflight: &DependencyPreflight,
) -> Result<bool, String> {
    Ok(unmet_dependencies(conn, story, preflight)?.is_empty())
}

pub(super) fn reconcile_ready(
    tx: &Transaction<'_>,
    plan_id: &str,
    preflight: &DependencyPreflight,
) -> Result<(), String> {
    for mut candidate in read_plan_stories(tx, plan_id)? {
        if !matches!(candidate.status, StoryStatus::Backlog | StoryStatus::Ready) {
            continue;
        }
        let desired = if dependencies_integrated(tx, &candidate, preflight)? {
            StoryStatus::Ready
        } else {
            StoryStatus::Backlog
        };
        if candidate.status != desired {
            let revision = candidate.revision;
            candidate.status = desired;
            save_story(tx, &mut candidate, revision)?;
        }
    }
    Ok(())
}

/// Check within the caller's SQLite transaction, not an earlier presentation read.
pub(super) fn require_active_plan(conn: &Connection, id: &str) -> Result<(), String> {
    let archived: bool = conn
        .query_row("SELECT archived FROM plans WHERE id=?1", [id], |row| {
            row.get(0)
        })
        .map_err(|e| format!("read plan archive state: {e}"))?;
    if archived {
        return Err("plan is archived; restore it first".into());
    }
    Ok(())
}

pub(super) fn require_active_story(conn: &Connection, story: &Story) -> Result<(), String> {
    if story.archived {
        return Err("story is archived; restore it first".into());
    }
    require_active_plan(conn, &story.plan_id)
}
