use super::tests::{FakeServer, PEER, setup};
use super::{mail::PendingMail, tool::Input};
use axum::http::StatusCode;
use serde_json::json;

// Catches: a routine retirement makes an authenticated final answer disappear.
#[tokio::test]
async fn finish_after_draft_retired_still_delivers_reply() {
    let (_dir, paths) = setup();
    let server = FakeServer::start(vec![(
        StatusCode::OK,
        json!({"ok":true,"result":{"message_id":7}}),
    )])
    .await;
    let mut runtime = super::tests::outbound_tests::runtime(paths, server.address).await;
    runtime
        .track(
            PendingMail {
                id: "request".into(),
                recipient: PEER.into(),
                content: json!({"chat_id":"1111111"}).to_string(),
            },
            super::streaming::Recipient::capture(&runtime.state, PEER),
        )
        .unwrap();
    // No draft remains (the same state after an epoch change or a lag).
    assert!(runtime.outbound.active.is_none());
    let result = runtime
        .tool(
            PEER,
            "target-mcp",
            Input::Finish {
                request_id: "request".into(),
                text: " exact final\n".into(),
            },
        )
        .await
        .unwrap();
    assert_eq!(result["message_ids"], json!([7]));
    assert_eq!(server.requests()[0].1["text"], " exact final\n");
}

use super::runtime::Runtime;
use crate::acp::{AcpClientEvent as Event, AcpEventJournal, AcpTurnId};
use agent_client_protocol::schema::v1;
use serde_json::Value;
#[cfg(unix)]
use std::sync::Arc;
use std::time::Duration;

fn mail(id: i64) -> PendingMail {
    // Recorded Telegram public example, with the documented modern Chat.type
    // adaptation and test-specific text/id, as in the existing inbound suite.
    let mut update: Value =
        serde_json::from_str(include_str!("fixtures/public-text-update.json")).unwrap();
    update["update_id"] = json!(id);
    update["message"]["chat"]["type"] = json!("private");
    update["message"]["text"] = json!("Please reply");
    super::mail::Update::parse(&update, &[1111111].into(), "test-bot", PEER)
        .unwrap()
        .mail
        .unwrap()
}
fn reactions(server: &FakeServer) -> Vec<String> {
    server
        .requests()
        .iter()
        .filter(|(method, _)| method.ends_with("setMessageReaction"))
        .map(|(_, body)| body["reaction"][0]["emoji"].as_str().unwrap().to_owned())
        .collect()
}
fn finals(server: &FakeServer) -> Vec<String> {
    server
        .requests()
        .iter()
        .filter(|(method, _)| method.ends_with("sendMessage"))
        .map(|(_, body)| body["text"].as_str().unwrap().to_owned())
        .collect()
}
async fn read(runtime: &Runtime) {
    let inbox = crate::mcp_http::mcp_transport::local_peer_call_with_message_id(
        &runtime.state,
        &json!({"action":"inbox","since":0}),
        Some("target-mcp"),
        None,
    )
    .await;
    assert!(inbox["count"].as_u64().unwrap() > 0, "{inbox}");
}
async fn finish(runtime: &mut Runtime, id: &str, text: &str) -> Result<Value, super::Error> {
    runtime
        .tool(
            PEER,
            "target-mcp",
            Input::Finish {
                request_id: id.into(),
                text: text.into(),
            },
        )
        .await
}
async fn drain(runtime: &Runtime, sequence: u64) {
    tokio::time::timeout(Duration::from_secs(30), async {
        while runtime.requests[0].recipient.observed_sequence() < sequence {
            tokio::task::yield_now().await;
        }
    })
    .await
    .expect("ACP reader did not reach the journal barrier");
}
fn append(journal: &AcpEventJournal, turn: AcpTurnId, event: Event) -> u64 {
    journal
        .append(Some(v1::SessionId::from("chat")), Some(turn), event)
        .sequence
}
fn start(journal: &AcpEventJournal, mail_wake: bool) -> (AcpTurnId, u64) {
    let turn = AcpTurnId::new();
    append(
        journal,
        turn,
        Event::PromptSent {
            text: if mail_wake {
                crate::pty::PEER_MAIL_WAKE
            } else {
                "Unrelated user turn"
            }
            .into(),
        },
    );
    (turn, append(journal, turn, Event::TurnStarted))
}
fn chunk(text: &str) -> Event {
    Event::SessionUpdate {
        update: Box::new(v1::SessionUpdate::AgentMessageChunk(v1::ContentChunk::new(
            v1::ContentBlock::Text(v1::TextContent::new(text)),
        ))),
    }
}

// Catches: receipt requires an agent tool call, or system status is labelled as an unapproved answer.
#[tokio::test]
async fn accepted_message_gets_received_reaction_and_draft_without_agent_action() {
    let (_dir, paths) = setup();
    let server = FakeServer::start(vec![]).await;
    let mut runtime = super::tests::outbound_tests::runtime(paths, server.address).await;
    runtime.deliver(mail(7)).await.unwrap();
    assert_eq!(reactions(&server), ["👀"]);
    let requests = server.requests();
    assert_eq!(requests.len(), 2);
    assert!(requests[1].0.ends_with("sendMessageDraft"));
    assert_eq!(
        requests[1].1["text"],
        "TUICommander ha ricevuto il messaggio"
    );
    assert_eq!(requests[1].1["can_stop"], true);
    assert!(!requests[1].1["text"].as_str().unwrap().contains("Preview"));
    let events = serde_json::to_value(runtime.state.telegram_recent.lock().snapshot()).unwrap();
    for event in events
        .as_array()
        .unwrap()
        .iter()
        .filter(|event| event["kind"] == "status")
    {
        assert_eq!(event["preview"], "[redacted]");
    }
}

// Catches: no-agent acceptance leaves an orphan status instead of the pairing branch's persistent explanation.
#[tokio::test]
async fn no_agent_message_gets_reaction_and_1955_reply_but_no_draft() {
    let (_dir, paths) = setup();
    let server = FakeServer::start(vec![]).await;
    let mut runtime = super::tests::outbound_tests::runtime(paths, server.address).await;
    runtime.clear_registration();
    assert_eq!(runtime.deliver(mail(7)).await.unwrap()["accepted"], false);
    assert_eq!(reactions(&server), ["👀"]);
    assert_eq!(
        finals(&server),
        ["No agent is listening. Ask an agent in TUICommander to register for Telegram."]
    );
    assert!(
        server
            .requests()
            .iter()
            .all(|(method, _)| !method.ends_with("sendMessageDraft"))
    );
}

// Catches: inbox-only mail falsely reports working, or the queue silently disappears before ten minutes.
#[tokio::test]
async fn inbox_only_delivery_shows_queued() {
    let (_dir, paths) = setup();
    let server = FakeServer::start(vec![]).await;
    let mut runtime = super::tests::outbound_tests::runtime(paths, server.address).await;
    runtime.deliver(mail(7)).await.unwrap();
    tokio::time::sleep(Duration::from_secs(2)).await;
    runtime.tick().await;
    assert!(
        server.requests().last().unwrap().1["text"]
            .as_str()
            .unwrap()
            .contains("In coda")
    );
    runtime.requests[0].received -= Duration::from_secs(600);
    runtime.tick().await;
    runtime.tick().await;
    assert_eq!(
        finals(&server),
        ["Ancora in coda: l’agente non ha iniziato"]
    );
    assert_eq!(reactions(&server), ["👀"]);
}

// Catches: event-bus lag retires a request that can still finish.
#[tokio::test]
async fn lagged_events_keep_the_open_request() {
    let (_dir, paths) = setup();
    let server = FakeServer::start(vec![]).await;
    let mut runtime = super::tests::outbound_tests::runtime(paths, server.address).await;
    runtime.deliver(mail(7)).await.unwrap();
    let draft = runtime.outbound.active.as_ref().unwrap().draft;
    runtime.events_lagged();
    assert_eq!(runtime.outbound.active.as_ref().unwrap().draft, draft);
    finish(&mut runtime, "tg:test-bot:7", "Answer")
        .await
        .unwrap();
    assert_eq!(finals(&server), ["Answer"]);
}

// Catches: the shared serialization funnel sends bursts faster than Telegram permits.
#[tokio::test]
async fn status_calls_stay_at_or_under_one_per_second() {
    let (_dir, paths) = setup();
    let server = FakeServer::start(vec![]).await;
    let mut runtime = super::tests::outbound_tests::runtime(paths, server.address).await;
    let start = tokio::time::Instant::now();
    runtime.deliver(mail(7)).await.unwrap();
    finish(&mut runtime, "tg:test-bot:7", "Answer")
        .await
        .unwrap();
    assert_eq!(server.requests().len(), 4);
    assert!(start.elapsed() >= Duration::from_secs(3));
}

// Catches: a terminal requirement rejects an authenticated ACP begin/finish.
#[tokio::test]
async fn acp_registered_peer_can_begin_and_finish_without_a_pty() {
    let (_dir, paths) = setup();
    let server = FakeServer::start(vec![]).await;
    let mut runtime = super::tests::outbound_tests::runtime(paths, server.address).await;
    let _journal = runtime.state.acp.test_peer_journal(PEER);
    runtime.deliver(mail(7)).await.unwrap();
    let begun = runtime
        .tool(
            PEER,
            "target-mcp",
            Input::Begin {
                request_id: "tg:test-bot:7".into(),
            },
        )
        .await
        .unwrap();
    assert!(begun["draft_id"].as_i64().unwrap() > 0);
    finish(&mut runtime, "tg:test-bot:7", "ACP answer")
        .await
        .unwrap();
    assert_eq!(finals(&server), ["ACP answer"]);
}

// Catches: a pre-existing ACP turn or an unread inbox produces a false second acknowledgement.
#[tokio::test]
async fn acp_turn_started_after_delivery_sets_working_reaction() {
    let (_dir, paths) = setup();
    let server = FakeServer::start(vec![]).await;
    let mut runtime = super::tests::outbound_tests::runtime(paths, server.address).await;
    let journal = runtime.state.acp.test_peer_journal(PEER);
    start(&journal, false);
    runtime.deliver(mail(7)).await.unwrap();
    read(&runtime).await;
    runtime.tick().await;
    assert_eq!(reactions(&server), ["👀"]);
    let (_, seq) = start(&journal, true);
    drain(&runtime, seq).await;
    runtime.tick().await;
    assert_eq!(reactions(&server), ["👀", "👨‍💻"]);
}

// Catches: ACP chunks cause per-token API calls, leak thought chunks, or never reach the preview.
#[tokio::test]
async fn acp_message_chunks_stream_into_the_draft_coalesced() {
    let (_dir, paths) = setup();
    let server = FakeServer::start(vec![]).await;
    let mut runtime = super::tests::outbound_tests::runtime(paths, server.address).await;
    let journal = runtime.state.acp.test_peer_journal(PEER);
    runtime.deliver(mail(7)).await.unwrap();
    read(&runtime).await;
    let (turn, _) = start(&journal, true);
    append(&journal, turn, chunk("Hello "));
    append(
        &journal,
        turn,
        Event::SessionUpdate {
            update: Box::new(v1::SessionUpdate::AgentThoughtChunk(v1::ContentChunk::new(
                v1::ContentBlock::Text(v1::TextContent::new("private reasoning")),
            ))),
        },
    );
    let seq = append(&journal, turn, chunk("world"));
    drain(&runtime, seq).await;
    assert_eq!(server.requests().len(), 2);
    tokio::time::sleep(Duration::from_secs(2)).await;
    runtime.tick().await;
    let drafts: Vec<_> = server
        .requests()
        .into_iter()
        .filter(|(method, _)| method.ends_with("sendMessageDraft"))
        .collect();
    assert_eq!(drafts.len(), 2);
    assert_eq!(drafts[1].1["text"], "Preview — not approved.\nHello world");
    assert_eq!(drafts[0].1["draft_id"], drafts[1].1["draft_id"]);
}

// Catches: AI Chat's Telegram-woken answer is lost, duplicated, or replaced with unrelated-turn text.
#[tokio::test]
async fn acp_mail_woken_turn_settling_without_finish_sends_its_text_once() {
    for mail_wake in [false, true] {
        let (_dir, paths) = setup();
        let server = FakeServer::start(vec![]).await;
        let mut runtime = super::tests::outbound_tests::runtime(paths, server.address).await;
        let journal = runtime.state.acp.test_peer_journal(PEER);
        runtime.deliver(mail(7)).await.unwrap();
        read(&runtime).await;
        let (turn, _) = start(&journal, mail_wake);
        append(&journal, turn, chunk(" exact answer\n"));
        let seq = append(
            &journal,
            turn,
            Event::TurnSettled {
                stop_reason: v1::StopReason::EndTurn,
                usage: None,
            },
        );
        drain(&runtime, seq).await;
        runtime.tick().await;
        runtime.tick().await;
        if mail_wake {
            finish(&mut runtime, "tg:test-bot:7", " exact answer\n")
                .await
                .unwrap();
            assert_eq!(finals(&server), [" exact answer\n"]);
            assert_eq!(reactions(&server).last().unwrap(), "👍");
        } else {
            assert_eq!(finals(&server), [] as [std::string::String; 0]);
        }
    }
}

// Catches: an ACP permission prompt remains shown as working instead of waiting.
#[tokio::test]
async fn acp_permission_request_sets_waiting_status() {
    let (_dir, paths) = setup();
    let server = FakeServer::start(vec![]).await;
    let mut runtime = super::tests::outbound_tests::runtime(paths, server.address).await;
    let journal = runtime.state.acp.test_peer_journal(PEER);
    runtime.deliver(mail(7)).await.unwrap();
    read(&runtime).await;
    let (turn, _) = start(&journal, true);
    let seq = append(
        &journal,
        turn,
        Event::PermissionRequested {
            request_id: crate::acp::AcpHostRequestId::new(),
            request: Box::new(v1::RequestPermissionRequest::new(
                v1::SessionId::from("chat"),
                v1::ToolCallUpdate::new("t1", v1::ToolCallUpdateFields::new()),
                vec![],
            )),
        },
    );
    drain(&runtime, seq).await;
    tokio::time::sleep(Duration::from_secs(2)).await;
    runtime.tick().await;
    assert_eq!(reactions(&server).last().unwrap(), "🤔");
    assert_eq!(
        server.requests().last().unwrap().1["text"],
        "L’agente ti aspetta in TUICommander"
    );
}

// Catches: an ACP failure exposes provider diagnostics or leaves a working bubble forever.
#[tokio::test]
async fn acp_turn_failed_sends_one_failure_notice() {
    let (_dir, paths) = setup();
    let server = FakeServer::start(vec![]).await;
    let mut runtime = super::tests::outbound_tests::runtime(paths, server.address).await;
    let journal = runtime.state.acp.test_peer_journal(PEER);
    runtime.deliver(mail(7)).await.unwrap();
    read(&runtime).await;
    let (turn, _) = start(&journal, true);
    let seq = append(
        &journal,
        turn,
        Event::TurnFailed {
            message: "secret provider detail".into(),
            state: crate::acp::AcpAttachmentState::Idle,
        },
    );
    drain(&runtime, seq).await;
    runtime.tick().await;
    runtime.tick().await;
    assert_eq!(
        finals(&server),
        ["L’agente si è fermato prima di rispondere"]
    );
    assert_eq!(reactions(&server).last().unwrap(), "👎");
}

#[cfg(unix)]
struct PtyFixture(Arc<crate::state::AppState>);
#[cfg(unix)]
impl Drop for PtyFixture {
    fn drop(&mut self) {
        if let Some((_, session)) = self.0.session_maps.sessions.remove(PEER) {
            let mut session = session.lock();
            let _ = session._child.kill();
            let _ = session._child.wait();
        }
    }
}
#[cfg(unix)]
async fn pty(runtime: &mut Runtime) -> PtyFixture {
    crate::test_support::insert_recording_session(&runtime.state, PEER);
    crate::test_support::agent_session(&runtime.state, PEER, crate::pty::SHELL_BUSY);
    runtime
        .tool(PEER, "target-mcp", Input::Register {})
        .await
        .unwrap();
    PtyFixture(runtime.state.clone())
}

// Catches: the PTY's pre-delivery turn (or a new but unread turn) falsely acknowledges the phone mail.
#[cfg(unix)]
#[tokio::test]
async fn pty_turn_start_after_read_sets_working_reaction() {
    let (_dir, paths) = setup();
    let server = FakeServer::start(vec![]).await;
    let mut runtime = super::tests::outbound_tests::runtime(paths, server.address).await;
    let _pty = pty(&mut runtime).await;
    runtime.deliver(mail(7)).await.unwrap();
    runtime.tick().await;
    assert_eq!(reactions(&server), ["👀"]);
    read(&runtime).await;
    runtime.tick().await;
    assert_eq!(reactions(&server), ["👀"]);
    crate::pty::note_submitted_input(&runtime.state, PEER);
    runtime.tick().await;
    assert_eq!(reactions(&server), ["👀", "👨‍💻"]);
}

// Catches: a legitimate replacement epoch retires the open reply.
#[cfg(unix)]
#[tokio::test]
async fn epoch_change_does_not_retire_request() {
    let (_dir, paths) = setup();
    let server = FakeServer::start(vec![]).await;
    let mut runtime = super::tests::outbound_tests::runtime(paths, server.address).await;
    let _pty = pty(&mut runtime).await;
    runtime.deliver(mail(7)).await.unwrap();
    crate::pty::note_submitted_input(&runtime.state, PEER);
    runtime.tick().await;
    finish(&mut runtime, "tg:test-bot:7", "Late reply")
        .await
        .unwrap();
    assert_eq!(finals(&server), ["Late reply"]);
}

// Catches: an exited recipient silently expires, or repeats its stopped notice every tick.
#[cfg(unix)]
#[tokio::test]
async fn pty_exit_sends_one_stopped_notice() {
    let (_dir, paths) = setup();
    let server = FakeServer::start(vec![]).await;
    let mut runtime = super::tests::outbound_tests::runtime(paths, server.address).await;
    let _pty = pty(&mut runtime).await;
    runtime.deliver(mail(7)).await.unwrap();
    runtime.state.session_maps.exit_codes.insert(PEER.into(), 1);
    runtime.tick().await;
    runtime.tick().await;
    assert_eq!(
        finals(&server),
        ["L’agente si è fermato prima di rispondere"]
    );
    assert_eq!(reactions(&server).last().unwrap(), "👎");
}

// Catches: a read-but-unanswered turn never notifies, or a later authoritative finish is rejected.
#[cfg(unix)]
#[tokio::test]
async fn idle_without_reply_notifies_once_and_late_finish_still_delivers() {
    let (_dir, paths) = setup();
    let server = FakeServer::start(vec![]).await;
    let mut runtime = super::tests::outbound_tests::runtime(paths, server.address).await;
    let _pty = pty(&mut runtime).await;
    runtime.deliver(mail(7)).await.unwrap();
    read(&runtime).await;
    crate::pty::note_submitted_input(&runtime.state, PEER);
    runtime.tick().await;
    runtime
        .state
        .session_maps
        .session_states
        .get_mut(PEER)
        .unwrap()
        .suggested_actions = Some(vec![]);
    runtime.requests[0].read = Some(tokio::time::Instant::now() - Duration::from_secs(60));
    runtime.tick().await;
    runtime.tick().await;
    assert_eq!(finals(&server), ["L’agente ha terminato senza rispondere"]);
    finish(&mut runtime, "tg:test-bot:7", "Late reply")
        .await
        .unwrap();
    assert_eq!(finals(&server).len(), 2);
    assert_eq!(reactions(&server).last().unwrap(), "👍");
}

// Catches: progress done and explicit finish both persist the same reply.
#[cfg(unix)]
#[tokio::test]
async fn progress_done_and_finish_produce_one_final() {
    for progress_first in [false, true] {
        let (_dir, paths) = setup();
        let server = FakeServer::start(vec![]).await;
        let mut runtime = super::tests::outbound_tests::runtime(paths, server.address).await;
        let _pty = pty(&mut runtime).await;
        runtime.deliver(mail(7)).await.unwrap();
        read(&runtime).await;
        crate::pty::note_submitted_input(&runtime.state, PEER);
        runtime.tick().await;
        let event = || crate::state::AppEvent::ProgressRecorded {
            repo_path: "repo".into(),
            payload: json!({"entry":{"ptyId":PEER,"type":"done","text":"Answer"}}),
        };
        if progress_first {
            runtime.event(event()).await;
        }
        finish(&mut runtime, "tg:test-bot:7", "Answer")
            .await
            .unwrap();
        runtime.event(event()).await;
        assert_eq!(finals(&server), ["Answer"]);
    }
}

// Catches: a PTY waiting for a person looks like ordinary work.
#[cfg(unix)]
#[tokio::test]
async fn awaiting_input_sets_waiting_status() {
    let (_dir, paths) = setup();
    let server = FakeServer::start(vec![]).await;
    let mut runtime = super::tests::outbound_tests::runtime(paths, server.address).await;
    let _pty = pty(&mut runtime).await;
    runtime.deliver(mail(7)).await.unwrap();
    read(&runtime).await;
    crate::pty::note_submitted_input(&runtime.state, PEER);
    runtime
        .state
        .session_maps
        .session_states
        .get_mut(PEER)
        .unwrap()
        .awaiting_input = true;
    tokio::time::sleep(Duration::from_secs(2)).await;
    runtime.tick().await;
    assert_eq!(reactions(&server).last().unwrap(), "🤔");
    assert_eq!(
        server.requests().last().unwrap().1["text"],
        "L’agente ti aspetta in TUICommander"
    );
}

// Catches: explicit finish before a tick lets a second request publish the first turn's answer.
#[tokio::test]
async fn explicit_finish_before_tick_reserves_acp_turn_for_first_request() {
    let (_dir, paths) = setup();
    let server = FakeServer::start(vec![]).await;
    let mut runtime = super::tests::outbound_tests::runtime(paths, server.address).await;
    let journal = runtime.state.acp.test_peer_journal(PEER);
    runtime.deliver(mail(7)).await.unwrap();
    runtime.deliver(mail(8)).await.unwrap();
    read(&runtime).await;
    let (turn, _) = start(&journal, true);
    let seq = append(&journal, turn, chunk("First turn text"));
    tokio::time::timeout(Duration::from_secs(30), async {
        while runtime
            .requests
            .iter()
            .any(|request| request.recipient.observed_sequence() < seq)
        {
            tokio::task::yield_now().await;
        }
    })
    .await
    .expect("Both ACP readers must observe the answer before explicit finish");
    finish(&mut runtime, "tg:test-bot:7", "Explicit first final")
        .await
        .unwrap();
    let seq = append(
        &journal,
        turn,
        Event::TurnSettled {
            stop_reason: v1::StopReason::EndTurn,
            usage: None,
        },
    );
    tokio::time::timeout(Duration::from_secs(30), async {
        while runtime.requests[1].recipient.observed_sequence() < seq {
            tokio::task::yield_now().await;
        }
    })
    .await
    .expect("Second ACP reader must observe settlement");
    runtime.tick().await;
    assert_eq!(finals(&server), ["Explicit first final"]);
    assert!(
        runtime.requests[1].completed.is_none(),
        "Second request must remain unanswered"
    );
}

// Catches: successive Telegram requests publish the same ACP turn twice.
#[tokio::test]
async fn several_messages_claim_distinct_acp_turns_and_keep_one_draft() {
    let (_dir, paths) = setup();
    let server = FakeServer::start(vec![]).await;
    let mut runtime = super::tests::outbound_tests::runtime(paths, server.address).await;
    let journal = runtime.state.acp.test_peer_journal(PEER);
    runtime.deliver(mail(7)).await.unwrap();
    runtime.deliver(mail(8)).await.unwrap();
    read(&runtime).await;
    let (turn, _) = start(&journal, true);
    append(&journal, turn, chunk("First answer"));
    let seq = append(
        &journal,
        turn,
        Event::TurnSettled {
            stop_reason: v1::StopReason::EndTurn,
            usage: None,
        },
    );
    drain(&runtime, seq).await;
    tokio::time::timeout(Duration::from_secs(30), async {
        while runtime.requests[1].recipient.observed_sequence() < seq {
            tokio::task::yield_now().await;
        }
    })
    .await
    .unwrap();
    runtime.tick().await;
    assert_eq!(finals(&server), ["First answer"]);
    assert_eq!(
        runtime.outbound.active.as_ref().unwrap().request,
        "tg:test-bot:8"
    );
    assert_ne!(runtime.requests[0].turn, runtime.requests[1].turn);
    let (second, _) = start(&journal, true);
    append(&journal, second, chunk("Second answer"));
    let seq = append(
        &journal,
        second,
        Event::TurnSettled {
            stop_reason: v1::StopReason::EndTurn,
            usage: None,
        },
    );
    tokio::time::timeout(Duration::from_secs(30), async {
        while runtime.requests[1].recipient.observed_sequence() < seq {
            tokio::task::yield_now().await;
        }
    })
    .await
    .unwrap();
    runtime.tick().await;
    assert_eq!(finals(&server), ["First answer", "Second answer"]);
    assert!(runtime.outbound.active.is_none());
}

// Catches: a retained ACP journal stays silent on process exit and leaves a working draft forever.
#[tokio::test]
async fn acp_connection_exit_sends_one_stopped_notice() {
    let (_dir, paths) = setup();
    let server = FakeServer::start(vec![]).await;
    let mut runtime = super::tests::outbound_tests::runtime(paths, server.address).await;
    let journal = runtime.state.acp.test_peer_journal(PEER);
    runtime.deliver(mail(7)).await.unwrap();
    let seq = journal
        .append(
            None,
            None,
            Event::ConnectionState {
                state: crate::acp::AcpConnectionState::Failed,
            },
        )
        .sequence;
    drain(&runtime, seq).await;
    runtime.tick().await;
    runtime.tick().await;
    assert_eq!(
        finals(&server),
        ["L’agente si è fermato prima di rispondere"]
    );
}

// Catches: truncating a long streamed preview loses its approval label or splits an astral character.
#[tokio::test]
async fn long_agent_preview_preserves_label_and_utf16_budget() {
    let (_dir, paths) = setup();
    let server = FakeServer::start(vec![]).await;
    let mut runtime = super::tests::outbound_tests::runtime(paths, server.address).await;
    runtime.deliver(mail(7)).await.unwrap();
    let text = format!("{}\n{}", super::outbound::PREVIEW_LABEL, "😀".repeat(4000));
    runtime.outbound.status("tg:test-bot:7", &text, true);
    tokio::time::sleep(Duration::from_secs(2)).await;
    runtime.outbound.refresh().await.unwrap();
    let request = server.requests().last().unwrap().1.clone();
    let text = request["text"].as_str().unwrap();
    assert!(text.starts_with(super::outbound::PREVIEW_LABEL));
    assert!(text.encode_utf16().count() <= 4096);
    assert!(text.ends_with('😀'));
}

// Catches: a Stop on queued mail interrupts the PTY's unrelated pre-delivery turn.
#[cfg(unix)]
#[tokio::test]
async fn queued_stop_withdraws_without_interrupting_current_turn() {
    let (_dir, paths) = setup();
    let server = FakeServer::start(vec![]).await;
    let mut runtime = super::tests::outbound_tests::runtime(paths, server.address).await;
    let bytes = crate::test_support::insert_recording_session(&runtime.state, PEER);
    crate::test_support::agent_session(&runtime.state, PEER, crate::pty::SHELL_BUSY);
    let _pty = PtyFixture(runtime.state.clone());
    runtime
        .tool(PEER, "target-mcp", Input::Register {})
        .await
        .unwrap();
    runtime.deliver(mail(7)).await.unwrap();
    let draft = runtime.outbound.active.as_ref().unwrap().draft;
    let stopped = json!({"stopped_message_generation":{"chat":{"id":1111111,"type":"private"},"draft_id":draft}});
    runtime.update(stopped.clone()).await.unwrap();
    runtime.update(stopped).await.unwrap();
    runtime.tick().await;
    assert!(bytes.lock().unwrap().is_empty());
    assert!(runtime.outbound.active.is_none());
    assert_eq!(finals(&server), [] as [std::string::String; 0]);
    finish(&mut runtime, "tg:test-bot:7", "Late answer")
        .await
        .unwrap();
    assert_eq!(finals(&server), ["Late answer"]);
}

// Catches: mail read in a pre-existing ACP turn never reaches the no-reply deadline after that turn ends.
#[tokio::test]
async fn read_in_preexisting_acp_turn_still_gets_no_reply_notice() {
    let (_dir, paths) = setup();
    let server = FakeServer::start(vec![]).await;
    let mut runtime = super::tests::outbound_tests::runtime(paths, server.address).await;
    let journal = runtime.state.acp.test_peer_journal(PEER);
    let (turn, _) = start(&journal, false);
    runtime.deliver(mail(7)).await.unwrap();
    read(&runtime).await;
    let seq = append(
        &journal,
        turn,
        Event::TurnSettled {
            stop_reason: v1::StopReason::EndTurn,
            usage: None,
        },
    );
    drain(&runtime, seq).await;
    runtime.requests[0].read = Some(tokio::time::Instant::now() - Duration::from_secs(60));
    runtime.tick().await;
    runtime.tick().await;
    assert_eq!(finals(&server), ["L’agente ha terminato senza rispondere"]);
    assert!(!reactions(&server).iter().any(|reaction| reaction == "👨‍💻"));
}

// Catches: a live AI Chat peer is ignored unless it explicitly registers for Telegram.
#[tokio::test]
async fn telegram_routes_to_the_active_ai_chat_agent() {
    let (_dir, paths) = setup();
    let server = FakeServer::start(vec![]).await;
    let mut runtime = super::tests::outbound_tests::runtime(paths, server.address).await;
    runtime.clear_registration();
    runtime
        .state
        .config
        .write()
        .ai_chat_peer_ids
        .insert("/chat".into(), PEER.into());
    let journal = runtime.state.acp.test_peer_journal(PEER);
    assert!(runtime.registration.is_none());
    let delivered = runtime.deliver(mail(7)).await.unwrap();
    assert_eq!(
        delivered["accepted"], true,
        "connected AI Chat must receive phone mail"
    );
    read(&runtime).await;
    let (turn, _) = start(&journal, true);
    append(&journal, turn, chunk("AI Chat reply"));
    let seq = append(
        &journal,
        turn,
        Event::TurnSettled {
            stop_reason: v1::StopReason::EndTurn,
            usage: None,
        },
    );
    drain(&runtime, seq).await;
    runtime.tick().await;
    assert_eq!(finals(&server), ["AI Chat reply"]);
    assert!(
        runtime.registration.is_none(),
        "fallback must not take over registration"
    );
}

// Catches: an AI Chat fallback steals an explicitly registered agent's channel.
#[tokio::test]
async fn telegram_registered_owner_wins_over_ai_chat() {
    let (_dir, paths) = setup();
    let server = FakeServer::start(vec![]).await;
    let mut runtime = super::tests::outbound_tests::runtime(paths, server.address).await;
    let other = "22222222-2222-4222-8222-222222222222";
    runtime
        .state
        .config
        .write()
        .ai_chat_peer_ids
        .insert("/other".into(), other.into());
    let _journal = runtime.state.acp.test_peer_journal(other);
    assert_eq!(runtime.deliver(mail(7)).await.unwrap()["accepted"], true);
    assert_eq!(runtime.requests[0].peer, PEER);
    assert_eq!(runtime.registered_peer(), Some(PEER));
    assert!(
        runtime
            .state
            .agent_inbox
            .get(other)
            .is_none_or(|inbox| inbox.is_empty())
    );
    assert_eq!(finals(&server), [] as [std::string::String; 0]);
}

// Catches: hash-map iteration chooses an arbitrary AI Chat instead of the latest connection.
#[tokio::test]
async fn telegram_latest_connected_ai_chat_wins_without_registration() {
    let (_dir, paths) = setup();
    let server = FakeServer::start(vec![]).await;
    let mut runtime = super::tests::outbound_tests::runtime(paths, server.address).await;
    runtime.clear_registration();
    let _first = runtime.state.acp.test_peer_journal(PEER);
    let other = "22222222-2222-4222-8222-222222222222";
    let _latest = runtime.state.acp.test_peer_journal(other);
    runtime
        .state
        .config
        .write()
        .ai_chat_peer_ids
        .insert("/first".into(), PEER.into());
    runtime.state.config.write().ai_chat_launches.insert(
        "latest-chat".into(),
        crate::acp_chat::ChatLaunch {
            executable: "ego".into(),
            profile: String::new(),
            workspace: crate::test_support::test_temp_root(),
            peer_id: other.into(),
        },
    );
    assert_eq!(runtime.deliver(mail(7)).await.unwrap()["accepted"], true);
    assert_eq!(runtime.requests[0].peer, other);
    assert!(runtime.registration.is_none());
    assert_eq!(finals(&server), [] as [std::string::String; 0]);
}

// Catches: connecting a second AI Chat discards the first chat's in-flight Telegram reply.
#[tokio::test]
async fn connecting_another_ai_chat_keeps_the_inflight_telegram_reply() {
    let (_dir, paths) = setup();
    let server = FakeServer::start(vec![]).await;
    let mut runtime = super::tests::outbound_tests::runtime(paths, server.address).await;
    runtime.clear_registration();
    runtime
        .state
        .config
        .write()
        .ai_chat_peer_ids
        .insert("/first".into(), PEER.into());
    let journal = runtime.state.acp.test_peer_journal(PEER);
    assert_eq!(runtime.deliver(mail(7)).await.unwrap()["accepted"], true);
    read(&runtime).await;
    let (turn, sequence) = start(&journal, true);
    drain(&runtime, sequence).await;
    runtime.tick().await;

    // The next inbound message should choose this chat, but the first reply
    // still belongs to the connected conversation that received its request.
    let other = "22222222-2222-4222-8222-222222222222";
    runtime
        .state
        .config
        .write()
        .ai_chat_peer_ids
        .insert("/second".into(), other.into());
    let _second = runtime.state.acp.test_peer_journal(other);
    append(&journal, turn, chunk("First chat answer"));
    let sequence = append(
        &journal,
        turn,
        Event::TurnSettled {
            stop_reason: v1::StopReason::EndTurn,
            usage: None,
        },
    );
    drain(&runtime, sequence).await;
    runtime.tick().await;
    assert_eq!(
        finals(&server),
        ["First chat answer"],
        "opening another chat must not replace a correlated Telegram answer with a stopped notice"
    );
}

// Catches: fallback selection rejects an explicit correlated reply or lets the new chat steal it.
#[tokio::test]
async fn correlated_reply_tools_keep_the_original_chat_until_explicit_takeover() {
    let (_dir, paths) = setup();
    let server = FakeServer::start(vec![]).await;
    let mut runtime = super::tests::outbound_tests::runtime(paths, server.address).await;
    runtime.clear_registration();
    runtime
        .state
        .config
        .write()
        .ai_chat_peer_ids
        .insert("/first".into(), PEER.into());
    let _first = runtime.state.acp.test_peer_journal(PEER);
    runtime.deliver(mail(7)).await.unwrap();

    let other = "33333333-3333-4333-8333-333333333333";
    let registered = crate::mcp_http::mcp_transport::local_peer_call_with_message_id(
        &runtime.state,
        &json!({"action":"register","tuic_session":other,"name":"second"}),
        Some("second-mcp"),
        None,
    )
    .await;
    assert!(registered.get("error").is_none(), "{registered}");
    runtime
        .state
        .config
        .write()
        .ai_chat_peer_ids
        .insert("/second".into(), other.into());
    let _second = runtime.state.acp.test_peer_journal(other);
    assert_eq!(runtime.recipient_peer().as_deref(), Some(other));
    assert_eq!(
        runtime
            .tool(
                other,
                "second-mcp",
                Input::Finish {
                    request_id: "tg:test-bot:7".into(),
                    text: "Stolen answer".into(),
                }
            )
            .await
            .unwrap_err(),
        super::Error::NotRegistered
    );
    assert_eq!(
        runtime
            .tool(
                PEER,
                "second-mcp",
                Input::Finish {
                    request_id: "tg:test-bot:7".into(),
                    text: "Forged binding".into(),
                }
            )
            .await
            .unwrap_err(),
        super::Error::NotRegistered
    );
    // Non-owner notifications no longer require inbound ownership (2119), but
    // must not transfer this pending request or its reply authority.
    runtime
        .tool(
            PEER,
            "target-mcp",
            Input::Send {
                text: "Unsolicited".into(),
                buttons: vec![],
            },
        )
        .await
        .unwrap();
    assert_eq!(runtime.requests[0].peer, PEER);
    runtime
        .tool(
            PEER,
            "target-mcp",
            Input::Begin {
                request_id: "tg:test-bot:7".into(),
            },
        )
        .await
        .unwrap();
    runtime
        .tool(
            PEER,
            "target-mcp",
            Input::Activity {
                request_id: "tg:test-bot:7".into(),
                text: "Working".into(),
            },
        )
        .await
        .unwrap();
    finish(&mut runtime, "tg:test-bot:7", "Original answer")
        .await
        .unwrap();
    finish(&mut runtime, "tg:test-bot:7", "Duplicate")
        .await
        .unwrap();
    assert_eq!(
        finals(&server),
        ["[target-mcp] Unsolicited", "Original answer"]
    );
    runtime.deliver(mail(8)).await.unwrap();
    assert_eq!(runtime.requests[1].peer, other);

    runtime
        .tool(PEER, "target-mcp", Input::Register {})
        .await
        .unwrap();
    runtime.deliver(mail(9)).await.unwrap();
    runtime
        .tool(other, "second-mcp", Input::Register {})
        .await
        .unwrap();
    assert_eq!(
        finish(&mut runtime, "tg:test-bot:9", "Revoked answer")
            .await
            .unwrap_err(),
        super::Error::NotRegistered
    );
    assert_eq!(
        finals(&server),
        ["[target-mcp] Unsolicited", "Original answer"]
    );
}

// Catches: saved chat identities without a live connection suppress the no-agent response.
#[tokio::test]
async fn telegram_no_agent_reply_requires_no_live_owner_or_chat() {
    let (_dir, paths) = setup();
    let server = FakeServer::start(vec![]).await;
    let mut runtime = super::tests::outbound_tests::runtime(paths, server.address).await;
    runtime.clear_registration();
    runtime
        .state
        .config
        .write()
        .ai_chat_peer_ids
        .insert("/offline".into(), PEER.into());
    assert_eq!(runtime.deliver(mail(7)).await.unwrap()["accepted"], false);
    assert_eq!(
        finals(&server),
        ["No agent is listening. Ask an agent in TUICommander to register for Telegram."]
    );
    assert!(runtime.requests.is_empty());
}
