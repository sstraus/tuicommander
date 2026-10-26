use super::*;
use crate::chat_view::{Agent, Bound, MAX_LOG_BYTES, MAX_LOG_ENTRIES, View};

const RECORDED: &str = include_str!("../../fixtures/chat_view/ego/transcript-v1.jsonl");

fn sessions() -> (Vec<Value>, Vec<Value>) {
    let rows: Vec<Value> = RECORDED
        .lines()
        .map(|line| serde_json::from_str(line).unwrap())
        .collect();
    let split = rows
        .iter()
        .enumerate()
        .skip(1)
        .find(|(_, row)| row["type"] == "transcript.header")
        .unwrap()
        .0;
    (rows[..split].to_vec(), rows[split..].to_vec())
}

// Catches: duplicated full answers after deltas, lost tool arguments/results,
// and goal continuations incorrectly presented as prompts typed by the human.
#[test]
fn ego_recorded_v1_preserves_chat_tools_and_system_continuations_once() {
    let (root, _) = sessions();
    let mut adapter = EgoAdapter::default();
    let updates: Vec<Value> = root
        .iter()
        .flat_map(|row| adapter.absorb(&row.to_string()))
        .collect();
    if std::env::var_os("TUIC_EGO_CHAT_CAPTURE").is_some() {
        println!(
            "EGO_CHAT_UPDATES={}",
            serde_json::to_string(&updates).unwrap()
        );
    }
    let messages: Vec<_> = updates
        .iter()
        .filter(|u| u["_meta"].is_null())
        .filter_map(|u| u["content"]["text"].as_str())
        .collect();
    assert_eq!(
        messages,
        ["Produce a local transcript", "First answer", "Final answer"]
    );
    let start = updates
        .iter()
        .find(|u| u["sessionUpdate"] == "tool_call")
        .unwrap();
    assert_eq!(start["title"], "exec");
    assert_eq!(
        start["rawInput"]["argv"][2],
        "printf transcript-tool-result"
    );
    let end = updates
        .iter()
        .find(|u| u["sessionUpdate"] == "tool_call_update")
        .unwrap();
    assert_eq!(end["toolCallId"], start["toolCallId"]);
    assert_eq!(end["status"], "completed");
    assert!(
        end["content"][0]["content"]["text"]
            .as_str()
            .unwrap()
            .contains("transcript-tool-result")
    );
    let continuation = updates
        .iter()
        .find(|u| u["_meta"]["ego"]["salience"] == "card")
        .unwrap();
    assert!(
        continuation["content"]["text"]
            .as_str()
            .unwrap()
            .starts_with("Goal continuation\n")
    );
    assert!(
        root.iter()
            .all(|row| adapter.absorb(&row.to_string()).is_empty()),
        "reconnect must not duplicate retained records"
    );
}

// Catches: a successor's checkpoint being mixed into ancestor chat or treated
// as a human prompt when the terminal acquires a new lease after compaction.
#[test]
fn ego_compaction_rebind_resets_shared_view_and_keeps_checkpoint_a_notice() {
    let (root, successor) = sessions();
    assert_eq!(successor[0]["compaction_predecessor"], root[0]["session"]);
    let mut view = View::for_agent(
        Agent::Ego,
        PathBuf::from("root"),
        MAX_LOG_ENTRIES,
        MAX_LOG_BYTES,
    );
    for row in &root {
        for update in view.adapter.absorb(&row.to_string()) {
            view.log.push(update);
        }
    }
    let epoch = view.log.epoch;
    let next = view.log.next_seq;
    view.rebind_to(Bound {
        path: PathBuf::from("successor"),
        agent: Agent::Ego,
        export: None,
    });
    for row in &successor {
        for update in view.adapter.absorb(&row.to_string()) {
            view.log.push(update);
        }
    }
    let (reset, updates) = view.log.since(Some(epoch), next);
    assert!(reset);
    assert_eq!(updates.len(), 1);
    assert_eq!(updates[0]["_meta"]["ego"]["salience"], "card");
    assert!(
        updates[0]["content"]["text"]
            .as_str()
            .unwrap()
            .starts_with("Conversation compacted\n")
    );
    assert!(
        !updates
            .iter()
            .any(|u| u["sessionUpdate"] == "user_message_chunk")
    );
}

// Catches: future schema versions, cross-session events and repeated sequences
// silently being interpreted as conversation; malformed rows must not poison later records.
#[test]
fn ego_rejects_unversioned_cross_session_rows_and_tolerates_additive_kinds() {
    let (root, _) = sessions();
    let prompt = root
        .iter()
        .find(|r| r["event"]["kind"] == "user_message")
        .unwrap();
    let mut adapter = EgoAdapter::default();
    assert_eq!(
        adapter.absorb(&prompt.to_string()),
        [] as [serde_json::Value; 0]
    );
    let mut header = root[0].clone();
    header["v"] = 2.into();
    adapter.absorb(&header.to_string());
    assert_eq!(
        adapter.absorb(&prompt.to_string()),
        [] as [serde_json::Value; 0]
    );
    adapter.absorb(&root[0].to_string());
    let mut other = prompt.clone();
    other["session"] = "other".into();
    assert_eq!(
        adapter.absorb(&other.to_string()),
        [] as [serde_json::Value; 0]
    );
    assert_eq!(adapter.absorb("{broken"), [] as [serde_json::Value; 0]);
    let mut future = prompt.clone();
    future["seq"] = 1.into();
    future["event"]["type"] = "future_kind".into();
    assert_eq!(
        adapter.absorb(&future.to_string()),
        [] as [serde_json::Value; 0]
    );
    assert_eq!(adapter.absorb(&prompt.to_string()).len(), 1);
    assert_eq!(
        adapter.absorb(&prompt.to_string()),
        [] as [serde_json::Value; 0]
    );
    assert_eq!(adapter.stats.unknown_rows, 1);
}

// Catches: failed tools looking successful, or ego bypassing shared redaction
// and field bounds. Input is a mutation of the real binary's recorded event.
#[test]
fn ego_failed_tools_and_text_share_existing_redaction_and_bounds() {
    let (root, _) = sessions();
    let mut adapter = EgoAdapter::default();
    adapter.absorb(&root[0].to_string());
    let mut tool = root
        .iter()
        .find(|r| r["event"]["kind"] == "tool" && r["event"]["type"] == "item_completed")
        .unwrap()
        .clone();
    tool["event"]["ok"] = false.into();
    tool["event"]["text"] = format!("AKIAIOSFODNN7EXAMPLE {}", "é".repeat(5000)).into();
    let update = adapter.absorb(&tool.to_string());
    assert_eq!(update[0]["status"], "failed");
    assert!(!update[0].to_string().contains("AKIAIOSFODNN7EXAMPLE"));
    assert_eq!(
        update[0]["content"][0]["content"]["text"]
            .as_str()
            .unwrap()
            .chars()
            .count(),
        4000
    );
}

// Catches: export launch/refusal errors returning a successful empty transcript.
#[cfg(unix)]
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn ego_export_failure_is_an_unreadable_reason_not_an_empty_snapshot() {
    let source = ExportSource {
        program: "/usr/bin/false".into(),
        home: crate::test_support::test_temp_root(),
        session: "0198c7f4-9c1f-7a2b-8c3d-4e5f60718293".into(),
    };
    let error = tokio::task::spawn_blocking(move || source.read(0, |_| Ok(())))
        .await
        .unwrap()
        .unwrap_err();
    assert!(
        error.to_string().contains("configured executable"),
        "{error}"
    );
}

// Catches: confusing a successful empty export with an executable refusal.
#[cfg(unix)]
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn ego_successful_empty_export_still_reports_missing_header() {
    let source = ExportSource {
        program: "/usr/bin/true".into(),
        home: crate::test_support::test_temp_root(),
        session: "01a152f2-3bd3-7175-90dd-13580205b9c9".into(),
    };
    let error = tokio::task::spawn_blocking(move || source.read(0, |_| Ok(())))
        .await
        .unwrap()
        .unwrap_err();
    assert!(error.to_string().contains("header missing"), "{error}");
}

// Catches: rejecting the installed public-event-v15 export or dropping its
// completed answer while reading the subprocess stream.
#[cfg(unix)]
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn ego_installed_recorded_export_reads_completed_answer() {
    let fixture = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("src/fixtures/chat_view/ego/installed-2026-10-19.jsonl");
    let source = ExportSource {
        program: crate::test_support::fake_ssh_script(
            "ego-installed-transcript-2032",
            &tuic_test_support::print_file_script(fixture.to_str().unwrap()),
            "",
        ),
        home: crate::test_support::test_temp_root(),
        session: "01a152f2-3bd3-7175-90dd-13580205b9c9".into(),
    };
    let updates = tokio::task::spawn_blocking(move || {
        let mut adapter = EgoAdapter::default();
        let mut updates = Vec::new();
        source
            .read(0, |line| {
                updates.extend(adapter.absorb(line));
                Ok(())
            })
            .unwrap();
        updates
    })
    .await
    .unwrap();
    let answers: Vec<_> = updates
        .iter()
        .filter(|row| row["sessionUpdate"] == "agent_message_chunk")
        .filter_map(|row| row["content"]["text"].as_str())
        .collect();
    assert_eq!(
        answers,
        [
            "TUICommander v2.0.0 is connected.\nintent: Returning the smoke test reply (Chat probe)\n",
            "💬 LIVE_A_CHAT_OK"
        ]
    );
}
