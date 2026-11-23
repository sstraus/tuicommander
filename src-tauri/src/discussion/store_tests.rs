use super::*;
use crate::discussion::{ForkOutcome, Usage};
use crate::progress::{ProgressListInput, ProgressStore};

struct Journal {
    _guard: Box<dyn std::any::Any>,
    dir: tempfile::TempDir,
    store: ProgressStore,
}

fn journal() -> Journal {
    let dir = tempfile::tempdir_in(tuic_test_support::test_temp_root()).unwrap();
    let guard = crate::config::set_config_dir_override(dir.path().to_path_buf());
    let store = ProgressStore::open().unwrap();
    Journal {
        _guard: Box::new(guard),
        dir,
        store,
    }
}

fn q<'a>(pty: &'a str, text: &'a str) -> NewQuestion<'a> {
    NewQuestion {
        project: "/p",
        pty_id: pty,
        agent_name: Some("Claude"),
        question: text,
    }
}

fn answer(fork: &str, category: Category, title: &str) -> Result<Answer, String> {
    Ok(Answer {
        outcome: ForkOutcome {
            fork_session_id: fork.to_owned(),
            text: String::new(),
            usage: Usage {
                cache_read_tokens: 46060,
                cost_usd: Some(0.007),
                ..Usage::default()
            },
        },
        classified: Classified {
            category,
            title: title.to_owned(),
            body: "the body".to_owned(),
            header_found: true,
        },
    })
}

fn kinds(store: &ProgressStore, include_activity: Option<bool>) -> Vec<(String, Option<String>)> {
    let input = ProgressListInput {
        include_activity,
        ..Default::default()
    };
    store
        .list("/p", &input)
        .unwrap()
        .entries
        .into_iter()
        .rev()
        .map(|e| (e.kind.as_str().to_owned(), e.step))
        .collect()
}

// Catches: a question or answer missing from the journal, or an answer entry
// without its category, which the list shows as the badge.
#[test]
fn question_and_answer_are_journal_entries() {
    let j = journal();
    let (question, turn) = ask(&j.store, &q("t1", "Why does it wait?")).unwrap();
    assert_eq!(question.kind, ProgressKind::Question);
    let answered = finish(
        &j.store,
        turn,
        &answer("fork-1", Category::Action, "Revert it"),
    )
    .unwrap();
    assert_eq!(answered.text, "Revert it\nthe body");
    assert_eq!(
        kinds(&j.store, None),
        vec![
            ("question".into(), Some("side".into())),
            ("answer".into(), Some("action".into()))
        ]
    );
    let thread = get(&j.store, answered.id).unwrap();
    assert_eq!(thread.thread_id, question.id);
    assert_eq!(thread.fork_session_id.as_deref(), Some("fork-1"));
    assert!(!thread.running);
    assert_eq!(thread.turns[0].answer.as_deref(), Some("the body"));
    assert_eq!(thread.turns[0].cache_read_tokens, Some(46060));
}

// Catches: the default list still showing the host trace, or losing an
// exchange when the trace is hidden.
#[test]
fn default_list_hides_the_trace_but_keeps_exchanges() {
    let j = journal();
    let intent = crate::progress::NewProgressEntry {
        kind: ProgressKind::Intent,
        text: "Starting".into(),
        step: None,
        agent_name: None,
    };
    j.store
        .record_hand_off("/p", &intent, Some("t1"), None, None)
        .unwrap();
    ask(&j.store, &q("t1", "What now?")).unwrap();
    assert_eq!(kinds(&j.store, None).len(), 2, "absent keeps every kind");
    assert_eq!(
        kinds(&j.store, Some(false)),
        vec![("question".into(), Some("side".into()))]
    );
}

// Catches: a follow-up on a thread still answering, or before a fork exists.
#[test]
fn follow_up_needs_a_finished_thread_with_a_fork() {
    let j = journal();
    let (first, turn) = ask(&j.store, &q("t1", "Why?")).unwrap();
    assert!(
        follow_up(&j.store, first.id, "And?")
            .unwrap_err()
            .contains("still answering")
    );
    finish(&j.store, turn, &Err("boom".into())).unwrap();
    assert!(
        follow_up(&j.store, first.id, "And?")
            .unwrap_err()
            .contains("no fork")
    );

    let (second, turn) = ask(&j.store, &q("t1", "Why?")).unwrap();
    let answered = finish(
        &j.store,
        turn,
        &answer("fork-2", Category::Answer, "Because"),
    )
    .unwrap();
    let (entry, _, fork) = follow_up(&j.store, answered.id, "And?").unwrap();
    assert_eq!(fork, "fork-2");
    let thread = get(&j.store, entry.id).unwrap();
    assert_eq!(thread.thread_id, second.id);
    assert_eq!(thread.turns.len(), 2);
    assert!(thread.running);
}

#[test]
fn cancel_and_failure_answer_with_their_own_step() {
    let j = journal();
    let (_, a) = ask(&j.store, &q("t1", "a")).unwrap();
    let (_, b) = ask(&j.store, &q("t1", "b")).unwrap();
    assert_eq!(
        finish(&j.store, a, &Err("cancelled".into()))
            .unwrap()
            .step
            .as_deref(),
        Some("cancelled")
    );
    let failed = finish(&j.store, b, &Err("the fork did not answer".into())).unwrap();
    assert_eq!(failed.step.as_deref(), Some("failed"));
    assert_eq!(
        get(&j.store, failed.id).unwrap().turns[0].error.as_deref(),
        Some("the fork did not answer")
    );
}

// Catches: the limit counting finished turns or other terminals.
#[test]
fn running_count_is_per_terminal_and_open_turns_only() {
    let j = journal();
    let (_, t1) = ask(&j.store, &q("t1", "one")).unwrap();
    ask(&j.store, &q("t1", "two")).unwrap();
    ask(&j.store, &q("t2", "three")).unwrap();
    finish(&j.store, t1, &answer("f", Category::Answer, "x")).unwrap();
    assert_eq!(running_count(&j.store, "t1").unwrap(), 1);
    assert_eq!(running_count(&j.store, "t2").unwrap(), 1);
}

// Catches: foreign keys off, leaving orphan turns behind a deleted question.
#[test]
fn deleting_the_question_entry_deletes_its_turns() {
    let j = journal();
    let (question, _) = ask(&j.store, &q("t1", "q")).unwrap();
    j.store.delete("/p", &[question.id]).unwrap();
    let turns: i64 = j
        .store
        .connection()
        .unwrap()
        .query_row("SELECT COUNT(*) FROM discussion_turns", [], |r| r.get(0))
        .unwrap();
    assert_eq!(turns, 0);
}

// Catches: threads stuck answering forever after a restart killed the fork.
#[test]
fn restart_answers_open_turns_as_failed() {
    let j = journal();
    let (question, _) = ask(&j.store, &q("t1", "q")).unwrap();
    assert_eq!(fail_interrupted(&j.store).unwrap(), 1);
    let thread = get(&j.store, question.id).unwrap();
    assert!(!thread.running);
    assert!(
        thread.turns[0]
            .error
            .as_deref()
            .unwrap()
            .contains("restart")
    );
}

// Catches: the same transcript message filed again on every turn end.
#[test]
fn a_direct_question_is_filed_once() {
    let j = journal();
    let first = record_direct(&j.store, &q("t1", "Is it done?"), "Yes, merged.", "msg-1").unwrap();
    let (question, answer) = first.expect("first capture files");
    assert_eq!(question.step.as_deref(), Some("direct"));
    assert_eq!(answer.text, "Yes, merged.");
    assert!(
        record_direct(&j.store, &q("t1", "Is it done?"), "Yes, merged.", "msg-1")
            .unwrap()
            .is_none()
    );
    assert_eq!(kinds(&j.store, Some(false)).len(), 2);
}

// Catches: the retention job deleting a question still waiting for its answer,
// or keeping the turns of a deleted entry.
#[test]
fn retention_keeps_a_pending_question_and_drops_answered_ones() {
    let j = journal();
    let (old_done, t) = ask(&j.store, &q("t1", "old done")).unwrap();
    finish(&j.store, t, &answer("f", Category::Answer, "x")).unwrap();
    let (pending, _) = ask(&j.store, &q("t2", "pending")).unwrap();
    j.store
        .connection()
        .unwrap()
        .execute("UPDATE entries SET created_at_ms = 1", [])
        .unwrap();
    let config = crate::config::RetentionConfig {
        progress_days: 1,
        ..Default::default()
    };
    crate::db::retention::run(j.dir.path(), &config, 10 * 86_400_000, &Default::default());
    assert!(get(&j.store, pending.id).is_ok(), "pending question kept");
    assert!(
        get(&j.store, old_done.id).is_err(),
        "answered thread removed"
    );
    let turns: i64 = j
        .store
        .connection()
        .unwrap()
        .query_row("SELECT COUNT(*) FROM discussion_turns", [], |r| r.get(0))
        .unwrap();
    assert_eq!(turns, 1);
}

// Catches: the kind rebuild losing rows, or resetting the id counter so a
// dialog still holding a deleted newest id would delete a new entry.
#[test]
fn migrating_an_old_journal_keeps_rows_and_the_id_counter() {
    let dir = tempfile::tempdir_in(tuic_test_support::test_temp_root()).unwrap();
    let path = dir.path().join(crate::db::PROGRESS_DB);
    {
        let conn = rusqlite::Connection::open(&path).unwrap();
        conn.execute_batch(crate::progress::JOURNAL_SCHEMA.migrations[0][0])
            .unwrap();
        conn.execute_batch(
            "INSERT INTO entries (project, created_at_ms, kind, text) VALUES ('/p', 1, 'done', 'kept');
             INSERT INTO entries (project, created_at_ms, kind, text) VALUES ('/p', 2, 'done', 'deleted newest');
             DELETE FROM entries WHERE id = 2;
             PRAGMA user_version = 1;",
        )
        .unwrap();
    }
    let _guard = crate::config::set_config_dir_override(dir.path().to_path_buf());
    let store = ProgressStore::open().unwrap();
    let (question, _) = ask(&store, &q("t1", "after the upgrade")).unwrap();
    assert_eq!(question.id, 3, "id 2 is never reused");
    let texts: Vec<String> = store
        .list("/p", &ProgressListInput::default())
        .unwrap()
        .entries
        .into_iter()
        .map(|e| e.text)
        .collect();
    assert_eq!(
        texts,
        vec!["after the upgrade".to_owned(), "kept".to_owned()]
    );
}

#[test]
fn agents_cannot_report_questions_or_answers() {
    for kind in ["question", "answer"] {
        let error = ProgressKind::parse_reportable(kind).unwrap_err();
        assert!(error.contains("recorded by TUIC"), "{error}");
    }
}

// Catches: the cross-project list leaking the host trace, mixing projects'
// view marks, or marking a seen entry unread.
#[test]
fn recent_spans_projects_with_unread_per_project() {
    let j = journal();
    let intent = crate::progress::NewProgressEntry {
        kind: ProgressKind::Intent,
        text: "Starting".into(),
        step: None,
        agent_name: None,
    };
    j.store
        .record_hand_off("/a", &intent, Some("t1"), None, None)
        .unwrap();
    let (seen, _) = ask(
        &j.store,
        &NewQuestion {
            project: "/a",
            pty_id: "t1",
            agent_name: None,
            question: "seen?",
        },
    )
    .unwrap();
    j.store.mark_viewed_for_pty("/a", None).unwrap();
    std::thread::sleep(std::time::Duration::from_millis(5));
    let (fresh, _) = ask(
        &j.store,
        &NewQuestion {
            project: "/b",
            pty_id: "t2",
            agent_name: None,
            question: "new?",
        },
    )
    .unwrap();

    let all = j.store.recent(&Default::default()).unwrap();
    let ids: Vec<(i64, bool)> = all.iter().map(|r| (r.entry.id, r.unread)).collect();
    assert_eq!(
        ids,
        vec![(fresh.id, true), (seen.id, false)],
        "newest first, no intent"
    );
    let unread = j
        .store
        .recent(&crate::progress::ProgressRecentInput {
            unread_only: Some(true),
            ..Default::default()
        })
        .unwrap();
    assert_eq!(unread.len(), 1);
    let intents = j
        .store
        .recent(&crate::progress::ProgressRecentInput {
            kind: Some(ProgressKind::Intent),
            ..Default::default()
        })
        .unwrap();
    assert_eq!(intents.len(), 1, "an explicit kind still selects the trace");
}

// Catches: an empty fork answer building an empty entry text that the journal
// rejects, leaving the turn open (thread running) for ever.
#[test]
fn an_empty_answer_closes_the_turn_as_a_failure() {
    let j = journal();
    let (question, turn) = ask(&j.store, &q("t1", "q")).unwrap();
    let mut empty = answer("f", Category::Answer, "");
    if let Ok(a) = empty.as_mut() {
        a.classified.body.clear();
    }
    let entry = finish(&j.store, turn, &empty).unwrap();
    assert_eq!(entry.step.as_deref(), Some("failed"));
    let thread = get(&j.store, question.id).unwrap();
    assert!(!thread.running);
    assert_eq!(
        thread.turns[0].error.as_deref(),
        Some("The fork returned no answer")
    );
}

// Catches: a second finish of a closed turn adding a duplicate answer entry
// and overwriting the first outcome.
#[test]
fn finishing_a_closed_turn_changes_nothing() {
    let j = journal();
    let (question, turn) = ask(&j.store, &q("t1", "q")).unwrap();
    finish(&j.store, turn, &answer("f", Category::Answer, "first")).unwrap();
    let before = kinds(&j.store, Some(false)).len();
    assert!(finish(&j.store, turn, &Err("late failure".to_owned())).is_err());
    assert_eq!(kinds(&j.store, Some(false)).len(), before);
    let thread = get(&j.store, question.id).unwrap();
    assert_eq!(thread.turns[0].error, None);
}
