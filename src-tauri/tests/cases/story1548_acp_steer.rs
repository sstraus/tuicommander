//! Catches regressions in the shared desktop/mobile prompt actor.
use agent_client_protocol::schema::v1;
use std::path::Path;
use tuicommander_lib::acp::AcpClientEvent;

use crate::acp_support;
use acp_support::{Fixture, authority, text, until_settled};

fn capable(scenario: &str) -> Fixture {
    let fixture = Fixture::with(scenario);
    let source = Path::new("tests/fixtures/acp");
    for name in ["initialize", "accepted", "not-busy", "rejected"] {
        std::fs::copy(
            source.join(format!("ego-steer-{name}.json")),
            fixture.root().join(format!("ego-steer-{name}.json")),
        )
        .unwrap();
    }
    fixture
}

// Catches: capable busy text still waits in the host queue and creates a second turn/bubble.
#[tokio::test]
async fn busy_capable_text_steers_current_turn_and_only_echoes_once() {
    let fixture = capable("steer-accepted");
    let connection = fixture.connect().await;
    let session = fixture
        .manager
        .new_session(connection.connection_id, authority(fixture.root()))
        .await
        .unwrap();
    let mut stream = fixture
        .manager
        .subscribe(connection.connection_id, 0)
        .unwrap();
    let first = fixture
        .manager
        .prompt(
            connection.connection_id,
            session.session_id.clone(),
            vec![text("desktop")],
        )
        .await
        .unwrap();
    let steered = fixture
        .manager
        .prompt(
            connection.connection_id,
            session.session_id.clone(),
            vec![text("phone")],
        )
        .await
        .unwrap();
    assert_eq!(first, steered);
    assert_eq!(
        fixture
            .manager
            .snapshot(connection.connection_id)
            .unwrap()
            .attachments[0]
            .queued_prompts,
        [] as [tuicommander_lib::acp::AcpQueuedPrompt; 0]
    );
    fixture
        .manager
        .list_sessions(connection.connection_id, Default::default())
        .await
        .unwrap();
    let frames = until_settled(&mut stream).await;
    assert_eq!(
        frames
            .iter()
            .filter(|frame| matches!(frame.event, AcpClientEvent::PromptSent { .. }))
            .count(),
        1
    );
    assert_eq!(frames.iter().filter(|frame| matches!(&frame.event, AcpClientEvent::SessionUpdate { update } if matches!(update.as_ref(), v1::SessionUpdate::UserMessageChunk(chunk) if matches!(&chunk.content, v1::ContentBlock::Text(text) if text.text == "phone")))).count(), 1);
    fixture
        .manager
        .disconnect(connection.connection_id)
        .await
        .unwrap();
}

async fn fallback(scenario: &str) {
    let fixture = capable(scenario);
    let connection = fixture.connect().await;
    let session = fixture
        .manager
        .new_session(connection.connection_id, authority(fixture.root()))
        .await
        .unwrap();
    let mut stream = fixture
        .manager
        .subscribe(connection.connection_id, 0)
        .unwrap();
    let first = fixture
        .manager
        .prompt(
            connection.connection_id,
            session.session_id.clone(),
            vec![text("desktop")],
        )
        .await
        .unwrap();
    let second = fixture
        .manager
        .prompt(
            connection.connection_id,
            session.session_id.clone(),
            vec![text("phone")],
        )
        .await
        .unwrap();
    assert_ne!(first, second);
    fixture
        .manager
        .list_sessions(connection.connection_id, Default::default())
        .await
        .unwrap();
    assert_eq!(
        until_settled(&mut stream).await.last().unwrap().turn_id,
        Some(first)
    );
    let second_frames = until_settled(&mut stream).await;
    assert_eq!(second_frames.last().unwrap().turn_id, Some(second));
    assert_eq!(second_frames.iter().filter(|frame| matches!(&frame.event, AcpClientEvent::PromptSent { text } if text == "phone")).count(), 1);
    fixture
        .manager
        .disconnect(connection.connection_id)
        .await
        .unwrap();
}

// Catches: a closing turn loses or duplicates text when ego returns not-busy.
#[tokio::test]
async fn not_busy_falls_back_to_exactly_one_serialized_prompt() {
    fallback("steer-not-busy").await;
}

// Catches: rejected steering drops the message instead of preserving normal queuing.
#[tokio::test]
async fn rejected_steering_keeps_the_existing_queue() {
    fallback("steer-rejected").await;
}

// Catches: advertising text steering sends image attachments through an unsupported extension.
#[tokio::test]
async fn capable_agent_still_queues_attachments() {
    let fixture = Fixture::with("prompt-queued-cancelled");
    std::fs::copy(
        "tests/fixtures/acp/ego-steer-initialize.json",
        fixture.root().join("ego-initialize.json"),
    )
    .unwrap();
    let connection = fixture.connect().await;
    let session = fixture
        .manager
        .new_session(connection.connection_id, authority(fixture.root()))
        .await
        .unwrap();
    let mut stream = fixture
        .manager
        .subscribe(connection.connection_id, 0)
        .unwrap();
    fixture
        .manager
        .prompt(
            connection.connection_id,
            session.session_id.clone(),
            vec![text("desktop")],
        )
        .await
        .unwrap();
    let queued = fixture
        .manager
        .prompt(
            connection.connection_id,
            session.session_id.clone(),
            vec![
                text("phone"),
                v1::ContentBlock::Image(v1::ImageContent::new("aGVsbG8=", "image/png")),
            ],
        )
        .await
        .unwrap();
    assert_eq!(
        fixture
            .manager
            .snapshot(connection.connection_id)
            .unwrap()
            .attachments[0]
            .queued_prompts
            .len(),
        1
    );
    fixture
        .manager
        .cancel_queued(connection.connection_id, session.session_id.clone(), queued)
        .await
        .unwrap();
    fixture
        .manager
        .cancel(connection.connection_id, session.session_id)
        .await
        .unwrap();
    until_settled(&mut stream).await;
    fixture
        .manager
        .disconnect(connection.connection_id)
        .await
        .unwrap();
}

// Catches: incompatible capability versions or methods are treated as valid steering.
#[test]
fn mismatched_steering_capabilities_preserve_legacy_prompt_routing() {
    let wire = include_str!("../fixtures/acp/ego-steer-initialize.json");
    let recorded: serde_json::Value = serde_json::from_str(wire).unwrap();
    for advertised in [
        serde_json::json!({"version": 2, "method": "_ego/steer", "contentTypes": ["text"]}),
        serde_json::json!({"version": 1, "method": "_ego/other", "contentTypes": ["text"]}),
        serde_json::json!({"version": 1, "method": "_ego/steer", "contentTypes": ["image"]}),
        serde_json::Value::Null,
    ] {
        let mut response = recorded.clone();
        response["agentCapabilities"]["_meta"]["ego"]["steer"] = advertised;
        let response: v1::InitializeResponse = serde_json::from_value(response).unwrap();
        assert_eq!(
            tuicommander_lib::acp::capability_snapshot(&response)
                .unwrap()
                .ego_steer_version,
            None
        );
    }
}
