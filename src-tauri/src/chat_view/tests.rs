use super::*;
use serde_json::json;
use std::path::Path;

fn update(n: u64) -> Value {
    json!({ "sessionUpdate": "agent_message_chunk", "n": n })
}

fn assistant_row(id: &str, text: &str) -> String {
    format!(
        r#"{{"type":"assistant","uuid":"{id}","message":{{"id":"m-{id}","content":[{{"type":"text","text":"{text}"}}]}}}}"#
    )
}

fn append(path: &Path, line: &str) {
    use std::io::Write;
    let mut f = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(path)
        .expect("open");
    writeln!(f, "{line}").expect("write");
}

fn texts(updates: &[Value]) -> Vec<String> {
    updates
        .iter()
        .map(|u| u["content"]["text"].as_str().unwrap_or_default().to_owned())
        .collect()
}

/// A client that fell behind the ring must be told to start over, not handed
/// a stream with a hole in it.
#[test]
fn view_log_behind_the_ring_returns_snapshot_not_silent_gap() {
    let mut log = ViewLog::new(3, usize::MAX);
    for n in 0..5 {
        log.push(update(n));
    }
    let (reset, updates) = log.since(Some(log.epoch), 1);
    assert!(reset, "seq 1 was evicted");
    assert_eq!(updates.len(), 3, "the whole retained log");
    let (reset, updates) = log.since(Some(log.epoch), 3);
    assert!(!reset);
    assert_eq!(updates.len(), 2, "only what the client has not seen");
    let (reset, updates) = log.since(Some(log.epoch), log.next_seq);
    assert!(!reset);
    assert_eq!(updates, [] as [serde_json::Value; 0]);
}

#[test]
fn view_log_first_read_is_a_reset_with_everything() {
    let mut log = ViewLog::new(10, usize::MAX);
    log.push(update(0));
    let (reset, updates) = log.since(None, 0);
    assert!(reset, "a client with no epoch starts from scratch");
    assert_eq!(updates.len(), 1);
}

/// The byte bound holds even when the entry bound does not bite.
#[test]
fn view_log_is_bounded_by_bytes_too() {
    let mut log = ViewLog::new(1_000, 200);
    for n in 0..50 {
        log.push(update(n));
    }
    assert!(log.bytes <= 200 + 64, "kept {} bytes", log.bytes);
    assert!(log.entries.len() < 50);
}

/// `/clear` and `/resume` start a new session file. Two conversations in one
/// log would show the new one under the old one.
#[test]
fn view_log_new_session_id_bumps_epoch_and_clears() {
    let tmp = tempfile::tempdir().expect("tempdir");
    let a = tmp.path().join("a.jsonl");
    let b = tmp.path().join("b.jsonl");
    append(&a, &assistant_row("1", "from a"));
    append(&b, &assistant_row("2", "from b"));

    let mut view = View::for_agent(Agent::Claude, a, 100, usize::MAX);
    view.advance(1 << 20).expect("advance");
    let old_epoch = view.log.epoch;
    let seen = view.log.next_seq;

    view.rebind(b);
    view.advance(1 << 20).expect("advance");
    assert_ne!(view.log.epoch, old_epoch);
    let (reset, updates) = view.log.since(Some(old_epoch), seen);
    assert!(reset, "a client on the old epoch must start over");
    assert_eq!(texts(&updates), ["from b"], "nothing of file a remains");
}

#[test]
fn view_rebind_to_the_same_file_keeps_the_log() {
    let tmp = tempfile::tempdir().expect("tempdir");
    let a = tmp.path().join("a.jsonl");
    append(&a, &assistant_row("1", "from a"));
    let mut view = View::for_agent(Agent::Claude, a.clone(), 100, usize::MAX);
    view.advance(1 << 20).expect("advance");
    let epoch = view.log.epoch;
    view.rebind(a);
    assert_eq!(view.log.epoch, epoch);
    assert!(
        !view.advance(1 << 20).expect("advance"),
        "nothing re-read after a same-file rebind"
    );
}

/// The tail must read only what was appended, and must not re-emit rows.
#[test]
fn view_advance_emits_only_appended_rows() {
    let tmp = tempfile::tempdir().expect("tempdir");
    let a = tmp.path().join("a.jsonl");
    append(&a, &assistant_row("1", "one"));
    let mut view = View::for_agent(Agent::Claude, a.clone(), 100, usize::MAX);
    assert!(view.advance(1 << 20).expect("advance"));
    assert!(!view.advance(1 << 20).expect("advance"), "nothing new");
    append(&a, &assistant_row("2", "two"));
    assert!(view.advance(1 << 20).expect("advance"));
    let (_, updates) = view.log.since(Some(view.log.epoch), 0);
    assert_eq!(texts(&updates), ["one", "two"]);
}

/// A first attach on a long transcript reads the end, not byte 0.
#[test]
fn view_first_attach_reads_only_the_last_window() {
    let tmp = tempfile::tempdir().expect("tempdir");
    let a = tmp.path().join("a.jsonl");
    for n in 0..200 {
        append(&a, &assistant_row(&n.to_string(), &format!("row {n}")));
    }
    let mut view = View::for_agent(Agent::Claude, a, 1_000, usize::MAX);
    view.advance(2_000).expect("advance");
    let (_, updates) = view.log.since(None, 0);
    let texts = texts(&updates);
    assert!(texts.len() < 200, "the head of the file was not read");
    assert_eq!(texts.last().map(String::as_str), Some("row 199"));
    // Every kept row is whole: a cut line would have failed to parse.
    assert_eq!(view.adapter.stats().malformed_rows, 0);
}

/// A truncated transcript must not leave the old conversation on screen.
#[test]
fn view_restarts_when_the_file_shrinks() {
    let tmp = tempfile::tempdir().expect("tempdir");
    let a = tmp.path().join("a.jsonl");
    append(&a, &assistant_row("1", "old one"));
    append(&a, &assistant_row("2", "old two"));
    let mut view = View::for_agent(Agent::Claude, a.clone(), 100, usize::MAX);
    view.advance(1 << 20).expect("advance");
    let epoch = view.log.epoch;

    std::fs::write(&a, format!("{}\n", assistant_row("3", "new"))).expect("truncate");
    assert!(view.advance(1 << 20).expect("advance"));
    assert_ne!(view.log.epoch, epoch);
    let (_, updates) = view.log.since(Some(view.log.epoch), 0);
    assert_eq!(texts(&updates), ["new"]);
}

// Catches: mid-turn human attachments disappear on attach or live tail, or queue metadata duplicates them.
#[test]
fn queued_human_prompts_survive_full_load_and_incremental_tail() {
    let fixture = include_str!("../fixtures/chat_view/recorded/queued-human.jsonl");
    let rows: Vec<Value> = fixture
        .lines()
        .map(|line| serde_json::from_str(line).unwrap())
        .collect();
    let expected: Vec<Value> = rows.iter().filter_map(|row| {
        if row["type"] == "assistant" {
            let text = row["message"]["content"].as_array()?.iter().find(|b| b["type"] == "text")?["text"].as_str()?;
            Some(json!({"sessionUpdate":"agent_message_chunk", "messageId":row["message"]["id"], "content":{"type":"text","text":text}}))
        } else if row.pointer("/attachment/origin/kind") == Some(&json!("human")) {
            Some(json!({"sessionUpdate":"user_message_chunk", "messageId":row["uuid"], "content":{"type":"text","text":row["attachment"]["prompt"]}}))
        } else { None }
    }).collect();
    assert_eq!(
        expected.len(),
        3,
        "two real assistant blocks around the queued human prompt"
    );
    let tmp = tempfile::tempdir_in(crate::test_support::test_temp_root()).unwrap();
    let path = tmp.path().join("queued.jsonl");
    std::fs::write(&path, fixture).unwrap();
    let mut full = View::for_agent(Agent::Claude, path.clone(), 100, usize::MAX);
    full.advance(TAIL_WINDOW_BYTES).unwrap();
    assert_eq!(
        full.log.since(None, 0).1,
        expected,
        "full load lost queued human prompt"
    );
    std::fs::write(&path, "").unwrap();
    let mut tail = View::for_agent(Agent::Claude, path.clone(), 100, usize::MAX);
    tail.advance(TAIL_WINDOW_BYTES).unwrap();
    for line in fixture.lines() {
        append(&path, line);
        tail.advance(TAIL_WINDOW_BYTES).unwrap();
    }
    assert_eq!(
        tail.log.since(None, 0).1,
        expected,
        "live tail lost or duplicated queued human prompt"
    );
    assert!(
        !tail.advance(TAIL_WINDOW_BYTES).unwrap(),
        "unchanged file replays no prompts"
    );
    assert_eq!(tail.adapter.stats().unknown_rows, 0);
}

// Catches: restarting a Codex transcript silently switching back to Claude parsing.
#[test]
fn codex_view_file_reset_preserves_rollout_projection() {
    let tmp = tempfile::tempdir_in(crate::test_support::test_temp_root()).expect("tempdir");
    let path = tmp.path().join("rollout.jsonl");
    let recorded = include_str!("../fixtures/chat_view/codex/completed.jsonl");
    std::fs::write(&path, recorded).expect("write recording");
    let mut view = View::for_agent(Agent::Codex, path, 100, usize::MAX);
    view.advance(1 << 20).expect("read rollout");
    let before = view.log.epoch;
    view.restart();
    view.advance(1 << 20).expect("read reset rollout");
    assert_ne!(view.log.epoch, before);
    let (_, updates) = view.log.since(None, 0);
    let replies: Vec<_> = updates
        .iter()
        .filter(|update| update["sessionUpdate"] == "agent_message_chunk")
        .map(|update| update["content"]["text"].as_str().unwrap())
        .collect();
    assert_eq!(replies, ["Recorded reply"]);
}

// Catches: a Grok terminal tailed with the Claude adapter (every row unknown, empty
// chat), or the old agent's conversation kept when the tab's agent changes.
#[test]
fn view_reads_a_grok_transcript_and_resets_when_the_agent_changes() {
    let tmp = tempfile::tempdir_in(crate::test_support::test_temp_root()).unwrap();
    let grok = tmp.path().join("chat_history.jsonl");
    std::fs::write(
        &grok,
        include_str!("../fixtures/chat_view/grok_chat_history.jsonl"),
    )
    .unwrap();
    let claude = tmp.path().join("a.jsonl");
    append(&claude, &assistant_row("1", "from claude"));

    let mut view = View::for_agent(Agent::Grok, grok.clone(), 100, usize::MAX);
    assert!(view.advance(TAIL_WINDOW_BYTES).unwrap());
    let (_, updates) = view.log.since(None, 0);
    assert_eq!(
        updates
            .iter()
            .filter(|u| u["sessionUpdate"] == "tool_call")
            .count(),
        7
    );
    assert_eq!(view.adapter.stats().unknown_rows, 0);

    let epoch = view.log.epoch;
    view.rebind_to(Bound {
        export: None,
        path: claude,
        agent: Agent::Claude,
    });
    view.advance(TAIL_WINDOW_BYTES).unwrap();
    assert_ne!(view.log.epoch, epoch);
    let (_, updates) = view.log.since(None, 0);
    assert_eq!(
        texts(&updates),
        ["from claude"],
        "nothing of the Grok tab remains"
    );
}

// Catches: an invalidated Codex ticker removing or advancing the replacement tab view.
#[test]
fn replaced_binding_stops_detached_ticker_and_preserves_current_view() {
    let state = crate::state::tests_support::make_test_app_state();
    let old = Arc::new(Mutex::new(View::for_agent(
        Agent::Codex,
        PathBuf::from("old.jsonl"),
        10,
        1024,
    )));
    let current = Arc::new(Mutex::new(View::for_agent(
        Agent::Codex,
        PathBuf::from("new.jsonl"),
        10,
        1024,
    )));
    state
        .chat_views
        .views
        .lock()
        .insert("tab".into(), current.clone());
    assert!(matches!(tick(&state, "tab", &old), Tick::Stop(_)));
    assert!(Arc::ptr_eq(
        state.chat_views.views.lock().get("tab").unwrap(),
        &current
    ));
}
