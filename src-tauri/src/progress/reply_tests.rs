use super::*;
use std::sync::{
    Arc,
    atomic::{AtomicUsize, Ordering},
};

fn setup() -> (impl Drop, ProgressStore, tempfile::TempDir) {
    let dir = tempfile::tempdir().unwrap();
    let guard = crate::config::set_config_dir_override(dir.path().to_path_buf());
    (guard, ProgressStore::open().unwrap(), dir)
}
fn report(store: &ProgressStore, binary: bool) -> ProgressEntry {
    store
        .record_report(
            "/original",
            &NewProgressEntry {
                kind: ProgressKind::Blocked,
                text: "Ship this version?".into(),
                step: None,
                agent_name: Some("author".into()),
            },
            None,
            Some("original-author"),
            binary.then_some(ProgressQuestion::YesNo),
        )
        .unwrap()
}
fn reply(id: i64, request: &str, answer: bool) -> ProgressReplyInput {
    ProgressReplyInput {
        entry_id: id,
        request_id: request.into(),
        text: None,
        answer: Some(answer),
    }
}

// Catches routing by focused project, loss of ACP origin, and bare unquoted Yes payloads.
#[test]
fn contextual_reply_preserves_author_question_and_project() {
    let (_guard, store, _dir) = setup();
    let entry = report(&store, true);
    let reopened = ProgressStore::open().unwrap();
    let listed = reopened
        .list("/original", &ProgressListInput::default())
        .unwrap();
    assert_eq!(listed.entries[0].question, Some(ProgressQuestion::YesNo));
    assert_eq!(listed.entries[0].answered, Some(false));
    assert!(
        reopened
            .reply(
                "/other",
                &reply(entry.id, "wrong-project", true),
                |_, _, _, _| panic!("must not deliver")
            )
            .is_err()
    );
    let receipt = reopened
        .reply(
            "/original",
            &reply(entry.id, "request-1", true),
            |author, pty, message, _| {
                assert_eq!(author, Some("original-author"));
                assert_eq!(pty, None);
                assert!(message.contains("Ship this version?"));
                assert!(message.contains(&format!("#{}", entry.id)));
                assert!(message.contains("Yes"));
                Ok(ProgressReplyDelivery::Queued)
            },
        )
        .unwrap();
    assert_eq!(receipt.delivery, ProgressReplyDelivery::Queued);
    assert_eq!(
        reopened
            .list("/original", &ProgressListInput::default())
            .unwrap()
            .entries[0]
            .answered,
        Some(true)
    );
}

// Catches duplicate enqueue on retries and different concurrent binary answers.
#[test]
fn contextual_reply_deduplicates_requests_and_serializes_binary_races() {
    let (_guard, store, _dir) = setup();
    let entry = report(&store, true);
    let calls = Arc::new(AtomicUsize::new(0));
    let handles: Vec<_> = (0..2)
        .map(|i| {
            let store = store.clone();
            let calls = calls.clone();
            std::thread::spawn(move || {
                store.reply(
                    "/original",
                    &reply(entry.id, &format!("race-{i}"), i == 0),
                    |_, _, _, _| {
                        calls.fetch_add(1, Ordering::SeqCst);
                        Ok(ProgressReplyDelivery::Delivered)
                    },
                )
            })
        })
        .collect();
    let results: Vec<_> = handles.into_iter().map(|h| h.join().unwrap()).collect();
    assert_eq!(results.iter().filter(|r| r.is_ok()).count(), 1);
    assert_eq!(calls.load(Ordering::SeqCst), 1);
    let receipt = results.into_iter().find_map(Result::ok).unwrap();
    let answer = receipt.request_id == "race-0";
    let duplicate = store
        .reply(
            "/original",
            &reply(entry.id, &receipt.request_id, answer),
            |_, _, _, _| panic!("duplicate delivery"),
        )
        .unwrap();
    assert_eq!(duplicate, receipt);
    assert!(
        store
            .reply(
                "/original",
                &reply(entry.id, &receipt.request_id, !answer),
                |_, _, _, _| panic!("changed request")
            )
            .is_err()
    );
}

// Catches failed deliveries consuming an answer and allowing bools on prose-only reports.
#[test]
fn contextual_reply_failure_is_retryable_and_shapes_are_explicit() {
    let (_guard, store, _dir) = setup();
    let entry = report(&store, true);
    let input = reply(entry.id, "retry", false);
    assert!(
        store
            .reply("/original", &input, |_, _, _, _| Err(
                "author unavailable".into()
            ))
            .is_err()
    );
    assert_eq!(
        store
            .list("/original", &ProgressListInput::default())
            .unwrap()
            .entries[0]
            .answered,
        Some(false)
    );
    store
        .reply("/original", &input, |_, _, _, _| {
            Ok(ProgressReplyDelivery::Queued)
        })
        .unwrap();
    let prose = report(&store, false);
    assert!(
        store
            .reply(
                "/original",
                &reply(prose.id, "not-binary", true),
                |_, _, _, _| panic!("not binary")
            )
            .is_err()
    );
    let mut text = ProgressReplyInput {
        entry_id: prose.id,
        request_id: "comment".into(),
        text: Some("Please explain".into()),
        answer: None,
    };
    store
        .reply("/original", &text, |_, _, _, _| {
            Ok(ProgressReplyDelivery::Delivered)
        })
        .unwrap();
    text.request_id = "comment-2".into();
    store
        .reply("/original", &text, |_, _, _, _| {
            Ok(ProgressReplyDelivery::Queued)
        })
        .unwrap();
    text.answer = Some(true);
    assert!(
        store
            .reply("/original", &text, |_, _, _, _| panic!("invalid shape"))
            .is_err()
    );
}

// Catches: a legacy report being delivered to a replacement agent in the same PTY.
#[test]
fn contextual_reply_refuses_unproven_legacy_author() {
    let (_guard, store, _dir) = setup();
    let entry = store
        .record_for_pty(
            "/original",
            &NewProgressEntry {
                kind: ProgressKind::Done,
                text: "Old report".into(),
                step: None,
                agent_name: None,
            },
            Some("reused-terminal"),
        )
        .unwrap();
    assert_eq!(
        store
            .list("/original", &ProgressListInput::default())
            .unwrap()
            .entries[0]
            .reply_target_available,
        Some(false)
    );
    let input = ProgressReplyInput {
        entry_id: entry.id,
        request_id: "legacy-reply".into(),
        text: Some("Continue".into()),
        answer: None,
    };
    let result = store.reply("/original", &input, |_, _, _, _| {
        panic!("unknown authors must not deliver")
    });
    assert!(result.unwrap_err().contains("author is unavailable"));
}
