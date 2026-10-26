use agent_client_protocol::schema::v1;
use tuicommander_lib::acp::{
    AcpAttachmentState, AcpClientEvent, AcpHoldState, AcpTurnState, EgoHoldRequest,
};

use crate::acp_support;
use acp_support::{Fixture, authority, text, until_settled};

#[tokio::test]
async fn phone_prompt_waits_for_desktop_turn_to_end_before_reaching_ego() {
    let fixture = Fixture::with("prompt-queued-after-turn");
    let connection = fixture.connect().await;
    let session = fixture
        .manager
        .new_session(connection.connection_id, authority(fixture.root()))
        .await
        .expect("session/new");
    let mut desktop = fixture
        .manager
        .subscribe(connection.connection_id, 0)
        .unwrap();
    let mut phone = fixture
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
        .expect("desktop prompt");
    let second = fixture
        .manager
        .prompt(
            connection.connection_id,
            session.session_id.clone(),
            vec![text("phone")],
        )
        .await
        .expect("phone prompt accepted while desktop runs");
    assert_ne!(first, second);

    let snapshot = fixture.manager.snapshot(connection.connection_id).unwrap();
    assert_eq!(
        snapshot.attachments[0].active_turn.as_ref().unwrap().state,
        AcpTurnState::Running
    );
    assert_eq!(snapshot.attachments[0].queued_prompts.len(), 1);
    assert_eq!(snapshot.attachments[0].queued_prompts[0].summary, "phone");
    for stream in [&mut desktop, &mut phone] {
        let seen = acp_support::until(stream, |event| {
            matches!(event, AcpClientEvent::PromptQueueChanged { .. })
        })
        .await;
        assert_eq!(seen.last().unwrap().turn_id, Some(second));
    }

    fixture
        .manager
        .list_sessions(connection.connection_id, Default::default())
        .await
        .expect("fixture barrier before end_turn");
    let first_end = until_settled(&mut desktop).await;
    assert_eq!(first_end.last().unwrap().turn_id, Some(first));
    let second_end = until_settled(&mut desktop).await;
    assert_eq!(second_end.last().unwrap().turn_id, Some(second));
    fixture
        .manager
        .disconnect(connection.connection_id)
        .await
        .unwrap();
}

#[tokio::test]
async fn either_view_can_cancel_a_queued_prompt_before_ego_receives_it() {
    let fixture = Fixture::with("prompt-queued-cancelled");
    let connection = fixture.connect().await;
    let session = fixture
        .manager
        .new_session(connection.connection_id, authority(fixture.root()))
        .await
        .unwrap();
    let mut desktop = fixture
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
            vec![text("phone")],
        )
        .await
        .unwrap();

    fixture
        .manager
        .cancel_queued(connection.connection_id, session.session_id.clone(), queued)
        .await
        .expect("desktop cancelled phone's queued prompt");
    assert_eq!(
        fixture
            .manager
            .snapshot(connection.connection_id)
            .unwrap()
            .attachments[0]
            .queued_prompts,
        [] as [tuicommander_lib::acp::AcpQueuedPrompt; 0]
    );
    let seen = acp_support::until(&mut desktop, |event| matches!(event, AcpClientEvent::PromptQueueChanged { queued_prompts } if queued_prompts.is_empty())).await;
    assert!(matches!(
        seen.last().unwrap().event,
        AcpClientEvent::PromptQueueChanged { .. }
    ));

    fixture
        .manager
        .cancel(connection.connection_id, session.session_id.clone())
        .await
        .unwrap();
    until_settled(&mut desktop).await;
    fixture
        .manager
        .disconnect(connection.connection_id)
        .await
        .unwrap();
}

#[tokio::test]
async fn cancelling_the_running_turn_is_visible_to_both_views() {
    let fixture = Fixture::with("prompt-queued-cancelled");
    let connection = fixture.connect().await;
    let session = fixture
        .manager
        .new_session(connection.connection_id, authority(fixture.root()))
        .await
        .unwrap();
    let mut desktop = fixture
        .manager
        .subscribe(connection.connection_id, 0)
        .unwrap();
    let mut phone = fixture
        .manager
        .subscribe(connection.connection_id, 0)
        .unwrap();
    let turn = fixture
        .manager
        .prompt(
            connection.connection_id,
            session.session_id.clone(),
            vec![text("desktop")],
        )
        .await
        .unwrap();

    fixture
        .manager
        .cancel(connection.connection_id, session.session_id.clone())
        .await
        .expect("phone cancels the shared running turn");
    for stream in [&mut desktop, &mut phone] {
        let cancelling = acp_support::until(stream, |event| {
            matches!(
                event,
                AcpClientEvent::AttachmentState {
                    state: tuicommander_lib::acp::AcpAttachmentState::Cancelling
                }
            )
        })
        .await;
        assert_eq!(cancelling.last().unwrap().turn_id, Some(turn));
        let settled = until_settled(stream).await;
        assert_eq!(settled.last().unwrap().turn_id, Some(turn));
    }
    fixture
        .manager
        .disconnect(connection.connection_id)
        .await
        .unwrap();
}

#[tokio::test]
async fn paused_session_holds_queued_prompts_until_resume_and_both_views_can_cancel() {
    let fixture = Fixture::with("prompt-queued-through-pause");
    let connection = fixture.connect().await;
    let session = fixture
        .manager
        .new_session(connection.connection_id, authority(fixture.root()))
        .await
        .unwrap();
    let mut desktop = fixture
        .manager
        .subscribe(connection.connection_id, 0)
        .unwrap();
    let mut phone = fixture
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
    let kept = fixture
        .manager
        .prompt(
            connection.connection_id,
            session.session_id.clone(),
            vec![text("kept")],
        )
        .await
        .unwrap();
    let cancelled = fixture
        .manager
        .prompt(
            connection.connection_id,
            session.session_id.clone(),
            vec![text("cancelled")],
        )
        .await
        .unwrap();

    let request_id = "01932d5e-0000-7000-8000-0000000000f1".parse().unwrap();
    let hold = EgoHoldRequest {
        session_id: session.session_id.clone(),
        request_id,
    };
    let paused = fixture
        .manager
        .pause_turn(connection.connection_id, hold.clone())
        .await
        .unwrap();
    assert_eq!(paused.state, AcpHoldState::Pending);
    for stream in [&mut desktop, &mut phone] {
        acp_support::until(stream, |event| {
            matches!(
                event,
                AcpClientEvent::TurnFailed {
                    state: AcpAttachmentState::Paused,
                    ..
                }
            )
        })
        .await;
    }
    let attachment = &fixture
        .manager
        .snapshot(connection.connection_id)
        .unwrap()
        .attachments[0];
    assert_eq!(attachment.state, AcpAttachmentState::Paused);
    assert_eq!(attachment.queued_prompts.len(), 2);

    fixture
        .manager
        .cancel_queued(
            connection.connection_id,
            session.session_id.clone(),
            cancelled,
        )
        .await
        .expect("desktop cancels the phone's queued prompt while paused");
    let added = fixture
        .manager
        .prompt(
            connection.connection_id,
            session.session_id.clone(),
            vec![text("added while paused")],
        )
        .await
        .expect("phone can queue while paused");
    let queued = &fixture
        .manager
        .snapshot(connection.connection_id)
        .unwrap()
        .attachments[0]
        .queued_prompts;
    assert_eq!(
        queued.iter().map(|item| item.turn_id).collect::<Vec<_>>(),
        [kept, added]
    );

    let resumed = fixture
        .manager
        .resume_turn(connection.connection_id, hold)
        .await
        .unwrap();
    assert_eq!(resumed.state, AcpHoldState::Running);
    for turn in [kept, added] {
        let settled = until_settled(&mut phone).await;
        assert_eq!(settled.last().unwrap().turn_id, Some(turn));
    }
    fixture
        .manager
        .disconnect(connection.connection_id)
        .await
        .unwrap();
}

#[tokio::test]
async fn queued_image_bytes_stay_out_of_snapshots_and_shared_events() {
    let fixture = Fixture::with("prompt-queued-cancelled");
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
    let secret = "aGVsbG8tZnJvbS10aGUtcGhvbmU=";
    let queued = fixture
        .manager
        .prompt(
            connection.connection_id,
            session.session_id.clone(),
            vec![v1::ContentBlock::Image(v1::ImageContent::new(
                secret,
                "image/png",
            ))],
        )
        .await
        .unwrap();

    let snapshot = fixture.manager.snapshot(connection.connection_id).unwrap();
    let wire = serde_json::to_string(&snapshot).unwrap();
    assert!(
        wire.contains("Image"),
        "a second view can identify the queued image"
    );
    assert!(
        !wire.contains(secret),
        "image bytes were copied into the shared snapshot"
    );
    let seen = acp_support::until(&mut stream, |event| {
        matches!(event, AcpClientEvent::PromptQueueChanged { .. })
    })
    .await;
    let queued_event = serde_json::to_string(seen.last().unwrap()).unwrap();
    assert!(
        !queued_event.contains(secret),
        "image bytes were retained by the journal"
    );

    fixture
        .manager
        .cancel_queued(connection.connection_id, session.session_id.clone(), queued)
        .await
        .unwrap();
    fixture
        .manager
        .cancel(connection.connection_id, session.session_id.clone())
        .await
        .unwrap();
    until_settled(&mut stream).await;
    fixture
        .manager
        .disconnect(connection.connection_id)
        .await
        .unwrap();
}
