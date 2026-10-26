use super::inbound::{text_update, updates};
use super::regression::elapse_backoff;
use super::*;
use std::sync::atomic::{AtomicBool, Ordering};

fn offset(paths: &Paths) -> i64 {
    std::fs::read_to_string(paths.file("next_offset"))
        .unwrap()
        .trim()
        .parse()
        .unwrap()
}

struct Probe {
    paths: Paths,
    delivered: Arc<Mutex<Vec<(i64, String)>>>,
    full: Arc<AtomicBool>,
    fail_id: Option<String>,
}
impl MailPort for Probe {
    fn offer(&mut self, mail: &PendingMail) -> impl Future<Output = Result<bool, Error>> {
        if self.full.load(Ordering::Relaxed)
            && self.fail_id.as_ref().is_none_or(|id| id == &mail.id)
        {
            return std::future::ready(Err(Error::Capacity));
        }
        self.delivered
            .lock()
            .unwrap()
            .push((offset(&self.paths), mail.id.clone()));
        std::future::ready(Ok(true))
    }
}

// Catches: offset advances before the real mail-port handoff, or a failed
// second handoff advances a partial batch or causes an immediate network loop.
#[tokio::test]
async fn capacity_at_first_or_second_handoff_preserves_cursor_and_schedules_retry() {
    for fail_id in [
        None,
        Some("tg:test-bot:8".to_string()),
        Some("tg:test-bot:7".to_string()),
    ] {
        let (_dir, paths) = setup();
        let batch = vec![
            text_update(7, 1111111, "first"),
            text_update(8, 1111111, "second"),
        ];
        let server = FakeServer::start(vec![
            updates(vec![]),
            updates(batch.clone()),
            updates(batch),
        ])
        .await;
        let delivered = Arc::new(Mutex::new(vec![]));
        let full = Arc::new(AtomicBool::new(true));
        let probe = Probe {
            paths: paths.clone(),
            delivered: delivered.clone(),
            full: full.clone(),
            fail_id: fail_id.clone(),
        };
        let mut adapter =
            super::super::inbound::Inbound::with_loopback(paths.clone(), server.address, probe)
                .unwrap();
        adapter.poll().await.unwrap();
        assert_eq!(adapter.poll().await.err(), Some(Error::Capacity));
        assert_eq!(offset(&paths), 0);
        let partial = fail_id.as_deref() == Some("tg:test-bot:8");
        if partial {
            assert_eq!(
                *delivered.lock().unwrap(),
                vec![(0, "tg:test-bot:7".into())]
            );
        } else {
            assert!(delivered.lock().unwrap().is_empty());
        }
        assert!(matches!(adapter.poll().await.unwrap(), Poll::Backoff(_)));
        assert_eq!(server.requests().len(), 2);
        full.store(false, Ordering::Relaxed);
        elapse_backoff().await;
        assert!(matches!(adapter.poll().await.unwrap(), Poll::Accepted(_)));
        assert_eq!(server.requests()[2].1["offset"], 0);
        assert_eq!(offset(&paths), 9);
        let mut expected = vec![];
        if partial {
            // A failed batch retries already offered mail with its original ID.
            expected.push((0, "tg:test-bot:7".into()));
        }
        expected.extend([(0, "tg:test-bot:7".into()), (0, "tg:test-bot:8".into())]);
        assert_eq!(*delivered.lock().unwrap(), expected);
        assert_eq!(
            std::fs::read_to_string(paths.file("next_offset")).unwrap(),
            "9\n"
        );
        let mut files: Vec<_> = std::fs::read_dir(&paths.directory)
            .unwrap()
            .map(|entry| entry.unwrap().file_name().into_string().unwrap())
            .collect();
        files.sort();
        assert_eq!(
            files,
            [
                "allowed_chat_ids",
                "bot.token",
                "config.json",
                "next_offset",
                "owner.lock"
            ]
        );
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            assert_eq!(
                std::fs::metadata(paths.file("next_offset"))
                    .unwrap()
                    .permissions()
                    .mode()
                    & 0o777,
                0o600
            );
        }
    }
}

// Catches: a corrupt/missing prior cursor silently becomes offset zero,
// reads old backlog as new mail, or resets on every later restart.
#[tokio::test]
async fn missing_or_corrupt_cursor_skips_backlog_then_persists_the_new_tail() {
    for corrupt in [
        None,
        Some(""),
        Some("not-an-offset"),
        Some("-1"),
        Some("9223372036854775808"),
    ] {
        let (_dir, paths) = setup();
        write_private(&paths.file("next_offset"), "10\n");
        if let Some(text) = corrupt {
            write_private(&paths.file("next_offset"), text);
        } else {
            std::fs::remove_file(paths.file("next_offset")).unwrap();
        }
        let server = FakeServer::start(vec![
            updates(vec![text_update(100, 1111111, "old backlog")]),
            updates(vec![text_update(101, 1111111, "new phone message")]),
        ])
        .await;
        let mut adapter = Inbound::loopback(paths.clone(), server.address).unwrap();
        assert!(matches!(adapter.poll().await.unwrap(), Poll::Accepted(0)));
        assert_eq!(server.requests()[0].1["offset"], -1);
        assert_eq!(server.requests()[0].1["limit"], 1);
        assert_eq!(offset(&paths), 101);
        assert_eq!(
            adapter.pending().unwrap(),
            [] as [crate::telegram::mail::PendingMail; 0]
        );
        drop(adapter);
        let mut adapter = Inbound::loopback(paths.clone(), server.address).unwrap();
        assert!(matches!(adapter.poll().await.unwrap(), Poll::Accepted(1)));
        assert_eq!(server.requests()[1].1["offset"], 101);
        assert_eq!(offset(&paths), 102);
    }
}

// Catches: a first-start network fault still enters the discarded
// BootstrapUncertain state instead of using bounded retry and the new contract.
#[tokio::test]
async fn first_start_network_fault_uses_backoff_and_retries_backlog_skip() {
    for status in [
        StatusCode::BAD_GATEWAY,
        StatusCode::TOO_MANY_REQUESTS,
        StatusCode::BAD_REQUEST,
    ] {
        let (_dir, paths) = setup();
        let server = FakeServer::start(vec![
            (
                status,
                json!({"ok":false,"error_code":status.as_u16(),"parameters":{"retry_after":1}}),
            ),
            updates(vec![]),
        ])
        .await;
        let mut adapter = Inbound::loopback(paths.clone(), server.address).unwrap();
        let _ = adapter.poll().await;
        assert!(!paths.file("next_offset").exists());
        assert!(matches!(adapter.poll().await.unwrap(), Poll::Backoff(_)));
        assert_eq!(server.requests().len(), 1);
        elapse_backoff().await;
        assert!(matches!(adapter.poll().await.unwrap(), Poll::Accepted(0)));
        assert_eq!(server.requests()[1].1["offset"], -1);
        assert_eq!(offset(&paths), 0);
    }
}

// Catches: the fixed request limit drifts, or ignored strangers are excluded
// from the once-per-batch cursor and get replayed forever.
#[tokio::test]
async fn fixed_ten_update_batches_commit_the_tail_including_ignored_chats() {
    for chats in [[2222222, 2222222], [1111111, 2222222], [2222222, 1111111]] {
        let (_dir, paths) = setup();
        let server = FakeServer::start(vec![
            updates(vec![]),
            updates(vec![
                text_update(7, chats[0], "first"),
                text_update(8, chats[1], "tail"),
            ]),
            updates(vec![]),
        ])
        .await;
        let mut adapter = Inbound::loopback(paths.clone(), server.address).unwrap();
        adapter.poll().await.unwrap();
        let expected_mail = if chats == [2222222, 2222222] { 0 } else { 1 };
        assert!(
            matches!(adapter.poll().await.unwrap(), Poll::Accepted(count) if count == expected_mail)
        );
        assert_eq!(adapter.pending().unwrap().len(), expected_mail);
        assert_eq!(offset(&paths), 9);
        adapter.poll().await.unwrap();
        assert_eq!(server.requests()[1].1["limit"], 10);
        assert_eq!(server.requests()[2].1["limit"], 10);
        assert_eq!(server.requests()[2].1["offset"], 9);
    }
}
