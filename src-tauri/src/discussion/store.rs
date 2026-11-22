//! Discussion in the Progress journal (plan `discussion.md`).
//!
//! Every question and answer is a journal entry (`question`, `answer`), so the
//! journal's list, unread state, toasts, delete and retention apply to them.
//! `discussion_turns` beside the entries keeps what the 500-char entry cannot:
//! the full texts, the fork session, usage and errors. A thread is the turns
//! sharing the first question's entry id.

use rusqlite::{OptionalExtension, Transaction, params};
use serde::Serialize;

use super::{Answer, Category, Classified};
use crate::progress::{
    MAX_TEXT_CHARS, NewProgressEntry, ProgressEntry, ProgressKind, ProgressStore,
};

/// Where a question came from.
pub(crate) const SOURCE_SIDE: &str = "side";
pub(crate) const SOURCE_DIRECT: &str = "direct";

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct TurnRecord {
    pub question_entry_id: i64,
    pub answer_entry_id: Option<i64>,
    pub question: String,
    pub answer: Option<String>,
    pub category: Option<Category>,
    pub title: Option<String>,
    pub error: Option<String>,
    pub cache_read_tokens: Option<i64>,
    pub cache_creation_tokens: Option<i64>,
    pub cost_usd: Option<f64>,
    pub finished_ms: Option<i64>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct ThreadDetail {
    pub thread_id: i64,
    pub project: String,
    pub pty_id: Option<String>,
    pub source: String,
    pub fork_session_id: Option<String>,
    /// True while the newest turn has no answer.
    pub running: bool,
    pub turns: Vec<TurnRecord>,
}

pub(crate) struct NewQuestion<'a> {
    pub project: &'a str,
    pub pty_id: &'a str,
    pub agent_name: Option<&'a str>,
    pub question: &'a str,
}

fn sql(e: rusqlite::Error) -> String {
    e.to_string()
}

/// The journal line for a long text: redaction happens in the store, the cap here.
fn entry_text(text: &str) -> String {
    let text = text.trim();
    if text.chars().count() <= MAX_TEXT_CHARS {
        return text.to_owned();
    }
    text.chars().take(MAX_TEXT_CHARS - 1).chain(['…']).collect()
}

fn question_entry(q: &NewQuestion, source: &str) -> NewProgressEntry {
    NewProgressEntry {
        kind: ProgressKind::Question,
        text: entry_text(q.question),
        step: Some(source.to_owned()),
        agent_name: q.agent_name.map(str::to_owned),
    }
}

fn insert_turn(
    tx: &Transaction<'_>,
    question_entry_id: i64,
    thread_id: Option<i64>,
    source: &str,
    question: &str,
    fork_session_id: Option<&str>,
) -> Result<i64, String> {
    tx.execute(
        "INSERT INTO discussion_turns (question_entry_id, thread_id, source, question, fork_session_id)
         VALUES (?1, ?2, ?3, ?4, ?5)",
        params![question_entry_id, thread_id.unwrap_or(question_entry_id), source, question.trim(), fork_session_id],
    )
    .map_err(sql)?;
    Ok(tx.last_insert_rowid())
}

/// A new side question: its entry starts the thread. Returns `(entry, turn_id)`.
pub(crate) fn ask(store: &ProgressStore, q: &NewQuestion) -> Result<(ProgressEntry, i64), String> {
    store.record_exchange(
        q.project,
        &question_entry(q, SOURCE_SIDE),
        Some(q.pty_id),
        |tx, entry| insert_turn(tx, entry.id, None, SOURCE_SIDE, q.question, None),
    )
}

/// A follow-up in a thread. Refused while the thread answers, or before its
/// fork exists. Returns `(entry, turn_id, fork_session_id)`.
pub(crate) fn follow_up(
    store: &ProgressStore,
    thread_id: i64,
    question: &str,
) -> Result<(ProgressEntry, i64, String), String> {
    let thread = get(store, thread_id)?;
    if thread.running {
        return Err("this thread is still answering".to_owned());
    }
    let fork = thread
        .fork_session_id
        .clone()
        .ok_or("this thread has no fork to continue; ask a new question")?;
    let pty_id = thread.pty_id.clone().ok_or("this thread has no terminal")?;
    let q = NewQuestion {
        project: &thread.project,
        pty_id: &pty_id,
        agent_name: None,
        question,
    };
    let (entry, turn) = store.record_exchange(
        &thread.project,
        &question_entry(&q, SOURCE_SIDE),
        Some(&pty_id),
        |tx, entry| {
            insert_turn(
                tx,
                entry.id,
                Some(thread.thread_id),
                SOURCE_SIDE,
                question,
                Some(&fork),
            )
        },
    )?;
    Ok((entry, turn, fork))
}

/// Record a finished turn as an `answer` entry. A failed or cancelled run
/// answers too, so the thread never stays open.
pub(crate) fn finish(
    store: &ProgressStore,
    turn_id: i64,
    result: &Result<Answer, String>,
) -> Result<ProgressEntry, String> {
    let (project, pty_id): (String, Option<String>) = store
        .connection()?
        .query_row(
            "SELECT e.project, e.pty_id FROM discussion_turns t JOIN entries e ON e.id = t.question_entry_id
              WHERE t.id = ?1",
            [turn_id],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )
        .map_err(|e| format!("no such discussion turn: {e}"))?;
    let (text, step) = match result {
        Ok(answer) => {
            let c = &answer.classified;
            let line = if c.body.is_empty() {
                c.title.clone()
            } else {
                format!("{}\n{}", c.title, c.body)
            };
            (entry_text(&line), c.category.as_str().to_owned())
        }
        Err(error) if error == "cancelled" => ("Cancelled".to_owned(), "cancelled".to_owned()),
        Err(error) => (entry_text(error), "failed".to_owned()),
    };
    let entry = NewProgressEntry {
        kind: ProgressKind::Answer,
        text,
        step: Some(step),
        agent_name: None,
    };
    let (entry, ()) = store.record_exchange(&project, &entry, pty_id.as_deref(), |tx, entry| {
        let now = entry.created_at_ms as i64;
        match result {
            Ok(answer) => {
                let (u, c) = (&answer.outcome.usage, &answer.classified);
                tx.execute(
                    "UPDATE discussion_turns SET answer_entry_id = ?2, answer = ?3, category = ?4, title = ?5,
                       fork_session_id = COALESCE(fork_session_id, ?6), input_tokens = ?7, output_tokens = ?8,
                       cache_read_tokens = ?9, cache_creation_tokens = ?10, cost_usd = ?11, finished_ms = ?12
                     WHERE id = ?1",
                    params![
                        turn_id,
                        entry.id,
                        c.body,
                        c.category.as_str(),
                        c.title,
                        answer.outcome.fork_session_id,
                        u.input_tokens as i64,
                        u.output_tokens as i64,
                        u.cache_read_tokens as i64,
                        u.cache_creation_tokens as i64,
                        u.cost_usd,
                        now
                    ],
                )
                .map_err(sql)?;
            }
            Err(error) => {
                tx.execute(
                    "UPDATE discussion_turns SET answer_entry_id = ?2, error = ?3, finished_ms = ?4 WHERE id = ?1",
                    params![turn_id, entry.id, error, now],
                )
                .map_err(sql)?;
            }
        }
        Ok(())
    })?;
    Ok(entry)
}

/// A direct question and its answer, read from a transcript after the turn.
/// `None` when that transcript message was already filed.
pub(crate) fn record_direct(
    store: &ProgressStore,
    q: &NewQuestion,
    answer: &str,
    source_message_id: &str,
) -> Result<Option<(ProgressEntry, ProgressEntry)>, String> {
    let filed: Option<i64> = store
        .connection()?
        .query_row(
            "SELECT id FROM discussion_turns WHERE source_message_id = ?1",
            [source_message_id],
            |r| r.get(0),
        )
        .optional()
        .map_err(sql)?;
    if filed.is_some() {
        return Ok(None);
    }
    let (question, turn) = store.record_exchange(
        q.project,
        &question_entry(q, SOURCE_DIRECT),
        Some(q.pty_id),
        |tx, entry| {
            let turn = insert_turn(tx, entry.id, None, SOURCE_DIRECT, q.question, None)?;
            tx.execute(
                "UPDATE discussion_turns SET source_message_id = ?2 WHERE id = ?1",
                params![turn, source_message_id],
            )
            .map_err(sql)?;
            Ok(turn)
        },
    )?;
    let body = answer.trim().to_owned();
    let classified = Classified {
        category: Category::Answer,
        title: body
            .lines()
            .next()
            .unwrap_or_default()
            .chars()
            .take(80)
            .collect(),
        body,
        header_found: false,
    };
    let entry = NewProgressEntry {
        kind: ProgressKind::Answer,
        text: entry_text(&classified.body),
        step: Some(Category::Answer.as_str().to_owned()),
        agent_name: q.agent_name.map(str::to_owned),
    };
    let (answer_entry, ()) = store.record_exchange(q.project, &entry, Some(q.pty_id), |tx, entry| {
        tx.execute(
            "UPDATE discussion_turns SET answer_entry_id = ?2, answer = ?3, category = 'answer', title = ?4,
               finished_ms = ?5 WHERE id = ?1",
            params![turn, entry.id, classified.body, classified.title, entry.created_at_ms as i64],
        )
        .map_err(sql)?;
        Ok(())
    })?;
    Ok(Some((question, answer_entry)))
}

/// The thread an entry belongs to; `entry_id` may be any question or answer of it.
pub(crate) fn get(store: &ProgressStore, entry_id: i64) -> Result<ThreadDetail, String> {
    let conn = store.connection()?;
    let thread_id: i64 = conn
        .query_row(
            "SELECT thread_id FROM discussion_turns WHERE question_entry_id = ?1 OR answer_entry_id = ?1",
            [entry_id],
            |r| r.get(0),
        )
        .optional()
        .map_err(sql)?
        .ok_or("no Discussion thread for this entry")?;
    let (project, pty_id): (String, Option<String>) = conn
        .query_row(
            "SELECT project, pty_id FROM entries WHERE id = ?1",
            [thread_id],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )
        .map_err(|_| "the thread's first question was deleted".to_owned())?;
    let mut stmt = conn
        .prepare(
            "SELECT question_entry_id, answer_entry_id, question, answer, category, title, error,
                    cache_read_tokens, cache_creation_tokens, cost_usd, finished_ms, source, fork_session_id
               FROM discussion_turns WHERE thread_id = ?1 ORDER BY id",
        )
        .map_err(sql)?;
    let rows = stmt
        .query_map([thread_id], |r| {
            let category: Option<String> = r.get(4)?;
            Ok((
                TurnRecord {
                    question_entry_id: r.get(0)?,
                    answer_entry_id: r.get(1)?,
                    question: r.get(2)?,
                    answer: r.get(3)?,
                    category: category
                        .and_then(|c| serde_json::from_value(serde_json::Value::String(c)).ok()),
                    title: r.get(5)?,
                    error: r.get(6)?,
                    cache_read_tokens: r.get(7)?,
                    cache_creation_tokens: r.get(8)?,
                    cost_usd: r.get(9)?,
                    finished_ms: r.get(10)?,
                },
                r.get::<_, String>(11)?,
                r.get::<_, Option<String>>(12)?,
            ))
        })
        .map_err(sql)?
        .collect::<Result<Vec<_>, _>>()
        .map_err(sql)?;
    let source = rows
        .first()
        .map(|(_, s, _)| s.clone())
        .unwrap_or_else(|| SOURCE_SIDE.to_owned());
    let fork_session_id = rows.iter().find_map(|(_, _, fork)| fork.clone());
    let turns: Vec<TurnRecord> = rows.into_iter().map(|(turn, _, _)| turn).collect();
    let running = turns.last().is_some_and(|t| t.finished_ms.is_none());
    Ok(ThreadDetail {
        thread_id,
        project,
        pty_id,
        source,
        fork_session_id,
        running,
        turns,
    })
}

/// Turns still answering in one terminal.
pub(crate) fn running_count(store: &ProgressStore, pty_id: &str) -> Result<usize, String> {
    let n: i64 = store
        .connection()?
        .query_row(
            "SELECT COUNT(*) FROM discussion_turns t JOIN entries e ON e.id = t.question_entry_id
              WHERE e.pty_id = ?1 AND t.finished_ms IS NULL",
            [pty_id],
            |r| r.get(0),
        )
        .map_err(sql)?;
    Ok(n as usize)
}

/// After a restart no fork survives: answer every open turn with a failure.
pub(crate) fn fail_interrupted(store: &ProgressStore) -> Result<usize, String> {
    let open: Vec<i64> = {
        let conn = store.connection()?;
        let mut stmt = conn
            .prepare("SELECT id FROM discussion_turns WHERE finished_ms IS NULL")
            .map_err(sql)?;
        stmt.query_map([], |r| r.get(0))
            .map_err(sql)?
            .collect::<Result<_, _>>()
            .map_err(sql)?
    };
    for turn in &open {
        finish(store, *turn, &Err("interrupted by a restart".to_owned()))?;
    }
    Ok(open.len())
}

#[cfg(test)]
#[path = "store_tests.rs"]
mod tests;
