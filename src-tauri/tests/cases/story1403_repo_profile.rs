//! Repository selections must carry the machine ceiling; ego owns the clamp.
use crate::acp_support;

use acp_support::{Fixture, authority};

// Catches .tuic.json replacing machine authority and losing ego's clamp warnings.
#[tokio::test]
async fn repo_yolo_profile_carries_machine_ceiling_and_recorded_warnings() {
    let fixture = Fixture::with("repo-profile-ceiling");
    std::fs::write(
        fixture.root().join(".tuic.json"),
        r#"{"ego_profile":"wide"}"#,
    )
    .unwrap();
    std::fs::copy(
        "tests/fixtures/acp/ego-profile-ceiling.json",
        fixture.root().join("ego-profile-ceiling.json"),
    )
    .unwrap();
    let connection = fixture.connect().await;
    let attached = fixture
        .manager
        .new_session_for_repo(
            connection.connection_id,
            authority(fixture.root()),
            "machine",
        )
        .await
        .unwrap();
    assert_eq!(
        attached.profile_warnings,
        [
            "mode yolo exceeds profile ceiling default; using default",
            "sandbox off exceeds profile ceiling workspace; using workspace",
        ]
    );
    let recorded: serde_json::Value =
        serde_json::from_str(include_str!("../fixtures/acp/ego-profile-ceiling.json")).unwrap();
    assert_eq!(recorded["_meta"]["ego"]["effective"]["mode"], "default");
    assert_eq!(
        recorded["_meta"]["ego"]["effective"]["sandbox"],
        "workspace"
    );
    assert_eq!(
        fixture
            .manager
            .snapshot(connection.connection_id)
            .unwrap()
            .attachments[0]
            .profile_warnings,
        attached.profile_warnings
    );
    fixture
        .manager
        .disconnect(connection.connection_id)
        .await
        .unwrap();
}

// Catches adding profile metadata to ordinary sessions, changing launch selection.
#[tokio::test]
async fn absent_repo_profile_preserves_session_new_without_metadata() {
    let fixture = Fixture::with("repo-profile-absent");
    let connection = fixture.connect().await;
    let attached = fixture
        .manager
        .new_session_for_repo(connection.connection_id, authority(fixture.root()), "")
        .await
        .unwrap();
    assert_eq!(attached.profile_warnings, [] as [std::string::String; 0]);
    fixture
        .manager
        .disconnect(connection.connection_id)
        .await
        .unwrap();
}

// Catches an empty machine setting silently omitting the non-relaxable ceiling.
#[tokio::test]
async fn repo_profile_without_explicit_machine_selection_refuses_before_creation() {
    let fixture = Fixture::with("ready");
    std::fs::write(
        fixture.root().join(".tuic.json"),
        r#"{"ego_profile":"wide"}"#,
    )
    .unwrap();
    let connection = fixture.connect().await;
    let error = fixture
        .manager
        .new_session_for_repo(connection.connection_id, authority(fixture.root()), "")
        .await
        .unwrap_err();
    assert!(error.message.contains("explicit machine ego profile"));
    assert_eq!(
        fixture
            .manager
            .snapshot(connection.connection_id)
            .unwrap()
            .attachments,
        [] as [tuicommander_lib::acp::AcpAttachmentSnapshot; 0]
    );
    fixture
        .manager
        .disconnect(connection.connection_id)
        .await
        .unwrap();
}

// Catches older ego ignoring the extension while TUIC treats the session as bounded.
#[tokio::test]
async fn ignored_repo_profile_ceiling_is_not_exposed_as_an_attached_session() {
    let fixture = Fixture::with("repo-profile-ignored");
    std::fs::write(
        fixture.root().join(".tuic.json"),
        r#"{"ego_profile":"wide"}"#,
    )
    .unwrap();
    let connection = fixture.connect().await;
    let error = fixture
        .manager
        .new_session_for_repo(
            connection.connection_id,
            authority(fixture.root()),
            "machine",
        )
        .await
        .unwrap_err();
    assert!(error.message.contains("did not acknowledge"));
    assert_eq!(
        fixture
            .manager
            .snapshot(connection.connection_id)
            .unwrap()
            .attachments,
        [] as [tuicommander_lib::acp::AcpAttachmentSnapshot; 0]
    );
    fixture
        .manager
        .disconnect(connection.connection_id)
        .await
        .unwrap();
}
