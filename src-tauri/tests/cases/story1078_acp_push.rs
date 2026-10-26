//! The ACP wake signal that the mobile push consumer reads must still name a
//! real, answerable interaction when it reaches the host.

use agent_client_protocol::schema::v1;
use tuicommander_lib::acp::{
    AcpClientEvent, AcpConnectionId, AcpEventEnvelope, AcpNotice, AcpNoticeKind,
    AcpPendingInteraction,
};

use crate::acp_support;
use acp_support::{Fixture, authority, text};

#[tokio::test]
async fn a_permission_notice_names_a_live_seat_and_the_desktop_answer_removes_it() {
    let fixture = Fixture::with("permission-turn");
    let mut notices = fixture.manager.notices();
    let connection = fixture.connect().await;
    let session = fixture
        .manager
        .new_session(connection.connection_id, authority(fixture.root()))
        .await
        .unwrap();
    fixture
        .manager
        .prompt(
            connection.connection_id,
            session.session_id.clone(),
            vec![text("hello")],
        )
        .await
        .unwrap();

    let pending_notice = tokio::time::timeout(std::time::Duration::from_secs(30), async {
        loop {
            let notice = notices.recv().await.unwrap();
            if notice.kind == AcpNoticeKind::InteractionPending {
                break notice;
            }
        }
    })
    .await
    .expect("ACP permission notice");
    assert_eq!(
        pending_notice.session_id.as_ref(),
        Some(&session.session_id)
    );
    let pending = fixture
        .manager
        .pending_interactions(connection.connection_id)
        .await
        .unwrap();
    let [AcpPendingInteraction::Permission { request_id, .. }] = pending.as_slice() else {
        panic!("permission must be answerable from the phone");
    };
    assert_eq!(pending_notice.request_id, Some(*request_id));

    fixture
        .manager
        .respond_permission(
            connection.connection_id,
            *request_id,
            v1::RequestPermissionOutcome::Selected(v1::SelectedPermissionOutcome::new(
                v1::PermissionOptionId::new("allow"),
            )),
        )
        .await
        .unwrap();
    assert_eq!(
        fixture
            .manager
            .pending_interactions(connection.connection_id)
            .await
            .unwrap(),
        [] as [tuicommander_lib::acp::AcpPendingInteraction; 0]
    );
    fixture
        .manager
        .disconnect(connection.connection_id)
        .await
        .unwrap();
}

#[tokio::test]
async fn a_form_elicitation_notice_names_the_seat_the_phone_can_answer() {
    let fixture = Fixture::with("elicitation-turn");
    let mut notices = fixture.manager.notices();
    let connection = fixture.connect().await;
    let session = fixture
        .manager
        .new_session(connection.connection_id, authority(fixture.root()))
        .await
        .unwrap();
    fixture
        .manager
        .prompt(
            connection.connection_id,
            session.session_id.clone(),
            vec![text("branch")],
        )
        .await
        .unwrap();

    let notice = tokio::time::timeout(std::time::Duration::from_secs(30), async {
        loop {
            let notice = notices.recv().await.unwrap();
            if notice.kind == AcpNoticeKind::InteractionPending {
                break notice;
            }
        }
    })
    .await
    .expect("ACP form notice");
    let pending = fixture
        .manager
        .pending_interactions(connection.connection_id)
        .await
        .unwrap();
    let [
        AcpPendingInteraction::Elicitation {
            request_id,
            session_id,
            ..
        },
    ] = pending.as_slice()
    else {
        panic!("form must be answerable from the phone");
    };
    assert_eq!(notice.session_id.as_ref(), Some(session_id));
    assert_eq!(notice.request_id, Some(*request_id));

    let mut accepted = v1::ElicitationAcceptAction::new();
    accepted.content = Some([("branch".to_owned(), "main".into())].into_iter().collect());
    fixture
        .manager
        .respond_elicitation(
            connection.connection_id,
            *request_id,
            v1::ElicitationAction::Accept(accepted),
        )
        .await
        .unwrap();
    fixture
        .manager
        .disconnect(connection.connection_id)
        .await
        .unwrap();
}

#[test]
fn ordinary_acp_activity_does_not_generate_a_mobile_wake_notice() {
    let mut call = v1::ToolCall::new(v1::ToolCallId::new("call-1"), "Read a file");
    call.kind = v1::ToolKind::Read;
    let envelope = AcpEventEnvelope {
        connection_id: AcpConnectionId::new(),
        generation: 1,
        sequence: 4,
        session_id: Some(v1::SessionId::new("conversation-1")),
        turn_id: None,
        event: AcpClientEvent::SessionUpdate {
            update: Box::new(v1::SessionUpdate::ToolCall(call)),
        },
    };
    assert_eq!(AcpNotice::from_envelope(&envelope), None);
}

// Catches: card SessionUpdates silently discarded before the push pump, or
// ordinary/activity messages promoted to phone alerts.
#[test]
fn ego_cards_wake_the_phone_but_activity_and_plain_text_do_not() {
    for (salience, expected) in [
        (Some("card"), true),
        (Some("activity"), false),
        (None, false),
    ] {
        let mut chunk = v1::ContentChunk::new(text("Worker finished: RESULT"));
        chunk.meta = salience.map(|salience| {
            serde_json::Map::from_iter([(
                "ego".to_string(),
                serde_json::json!({ "salience": salience }),
            )])
        });
        let envelope = AcpEventEnvelope {
            connection_id: AcpConnectionId::new(),
            generation: 1,
            sequence: 9,
            session_id: Some(v1::SessionId::new("conversation-1")),
            turn_id: None,
            event: AcpClientEvent::SessionUpdate {
                update: Box::new(v1::SessionUpdate::AgentMessageChunk(chunk)),
            },
        };
        let notice = AcpNotice::from_envelope(&envelope);
        assert_eq!(notice.is_some(), expected, "{salience:?}");
        if let Some(notice) = notice {
            assert_eq!(notice.kind, AcpNoticeKind::Card);
            assert_eq!(notice.session_id, envelope.session_id);
            assert_eq!(notice.sequence, envelope.sequence);
            assert_eq!(notice.request_id, None);
        }
    }
}
