use super::neutral_effects::{EffectCommand, EffectOutcome, EffectQuestion};
use super::neutral_runtime::NeutralHost;
use super::neutral_telegram_contract::{MessageReference, TelegramOperation, TelegramRequest};
use super::neutral_telegram_test_fixture::*;
use super::runtime::RuntimeOwner;
use crate::workflows::neutral_admission::HostAdmission;
use serde_json::json;
use std::path::Path;

fn configure(root: &Path) {
    let paths = root.join("telegram");
    std::fs::create_dir_all(&paths).unwrap();
    for (name, value) in [
        ("config.json", r#"{"enabled":true,"bot_alias":"test-bot"}"#),
        ("allowed_chat_ids", "1111111\n"),
    ] {
        let path = paths.join(name);
        std::fs::write(&path, value).unwrap();
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o600)).unwrap();
        }
    }
}

// Catches: a schema-valid activity replacing the installed bot/operation,
// or a prepared send retaining authorization after its chat is revoked.
#[test]
fn installed_telegram_metadata_is_pinned_and_current_chat_is_checked() {
    let config = tempfile::tempdir_in(crate::test_support::test_temp_root()).unwrap();
    let _guard = crate::config::set_config_dir_override(config.path().into());
    configure(config.path());
    let f = fixture();
    let invocation = &f.notification.invocation;
    let context = f.host.context(invocation).unwrap();
    f.host
        .publication(
            context.as_ref(),
            &invocation.publication,
            &invocation.descriptor,
            &invocation.instance,
        )
        .unwrap();
    let mut changed = invocation.instance.clone();
    changed.config.get_mut("connector").unwrap()["bot"] = json!("other");
    assert!(
        f.host
            .publication(
                context.as_ref(),
                &invocation.publication,
                &invocation.descriptor,
                &changed
            )
            .unwrap_err()
            .contains("replace installed")
    );
    let mut changed = invocation.descriptor.clone();
    changed.purpose = "substituted".into();
    assert!(
        f.host
            .publication(
                context.as_ref(),
                &invocation.publication,
                &changed,
                &invocation.instance
            )
            .unwrap_err()
            .contains("differs")
    );
    std::fs::write(config.path().join("telegram/allowed_chat_ids"), "2222222\n").unwrap();
    assert!(
        f.host
            .prepare(invocation)
            .unwrap_err()
            .contains("chat is unavailable")
    );
}

// Catches: callbacks depending on volatile peer registration, selecting another
// chat/message, or committing a duplicate/contrary answer after daemon restart.
#[test]
fn retained_telegram_receipt_resolves_exact_wait_after_restart_once() {
    let config = tempfile::tempdir_in(crate::test_support::test_temp_root()).unwrap();
    let _guard = crate::config::set_config_dir_override(config.path().into());
    configure(config.path());
    let f = fixture();
    receipt(&f);
    let token = f.action.token(&f.notification.key.fence.run).unwrap();
    assert_eq!(token.len(), 63);
    assert!(
        !f.owner
            .telegram_answer("test-bot", 1111111, 18, &token)
            .unwrap()
    );
    assert!(
        !f.owner
            .telegram_answer("other", 1111111, 17, &token)
            .unwrap()
    );
    assert!(
        !f.owner
            .telegram_answer("test-bot", 2222222, 17, &token)
            .unwrap()
    );
    let path = f.dir.path().join("runs.sqlite3");
    let target = f.target.key.clone();
    drop(f.owner);
    let owner = RuntimeOwner::acquire(&path).unwrap();
    assert!(
        owner
            .telegram_answer("test-bot", 1111111, 17, &token)
            .unwrap()
    );
    let snapshot = owner.store.neutral_execution(&target.fence.run).unwrap();
    assert_eq!(snapshot.effect(&target).unwrap().answer, Some(true));
    let sequence = snapshot.sequence;
    assert!(
        !owner
            .telegram_answer("test-bot", 1111111, 17, &token)
            .unwrap()
    );
    assert_eq!(
        owner
            .store
            .neutral_execution(&target.fence.run)
            .unwrap()
            .sequence,
        sequence
    );
}

// Catches: a revision-1 button authorizing revision 2, or the same button
// digest remaining valid after target arguments, subject or decision changes.
#[test]
fn telegram_old_question_and_changed_target_cannot_select_new_wait() {
    let config = tempfile::tempdir_in(crate::test_support::test_temp_root()).unwrap();
    let _guard = crate::config::set_config_dir_override(config.path().into());
    configure(config.path());
    let f = fixture();
    receipt(&f);
    let run = &f.notification.key.fence.run;
    let token = f.action.token(run).unwrap();
    f.owner
        .neutral_effect_command(
            &f.target.key.fence.run,
            EffectCommand::Question {
                key: f.target.key.clone(),
                question: EffectQuestion {
                    revision: 2,
                    title: "Revised exact question".into(),
                    deadline_ms: f.target.question.as_ref().unwrap().deadline_ms,
                },
            },
        )
        .unwrap();
    assert!(
        !f.owner
            .telegram_answer("test-bot", 1111111, 17, &token)
            .unwrap()
    );
    let current = f
        .owner
        .store
        .neutral_execution(&f.target.key.fence.run)
        .unwrap();
    assert_eq!(current.effect(&f.target.key).unwrap().answer, None);
    for kind in ["arguments", "subject", "answer", "revision"] {
        let mut changed = f.action.clone();
        if kind == "arguments" || kind == "subject" {
            let mut invocation = f.target.invocation.clone();
            if kind == "arguments" {
                invocation.inputs.insert("value".into(), json!(42));
            } else {
                invocation.subject = Some(std::collections::BTreeMap::from([(
                    "id".into(),
                    json!("other"),
                )]));
            }
            changed.effect = super::neutral_effects::NeutralEffect::prepare(
                invocation,
                f.target.preparation.clone(),
            )
            .unwrap()
            .key;
        } else if kind == "answer" {
            changed.approved = false;
        } else {
            changed.revision = 2;
        }
        assert_ne!(changed.token(run).unwrap(), token, "{kind}");
        assert!(
            !f.owner
                .telegram_answer("test-bot", 1111111, 17, &changed.token(run).unwrap())
                .unwrap()
        );
    }
}

// Catches: lost-send recovery invoking Telegram again, or accepting an action
// when no service message receipt has reached the durable effect ledger.
#[tokio::test]
async fn telegram_missing_send_receipt_blocks_callback_and_reconcile_never_resends() {
    let config = tempfile::tempdir_in(crate::test_support::test_temp_root()).unwrap();
    let _guard = crate::config::set_config_dir_override(config.path().into());
    configure(config.path());
    let f = fixture();
    let token = f.action.token(&f.notification.key.fence.run).unwrap();
    assert!(
        f.owner
            .telegram_answer("test-bot", 1111111, 17, &token)
            .unwrap_err()
            .contains("receipt is not retained")
    );
    let observation = f.host.reconcile(f.notification.clone()).await.unwrap();
    assert!(!observation.stopped);
    assert!(matches!(
        observation.outcome,
        EffectOutcome::Uncertain { .. }
    ));
    assert_eq!(
        f.owner
            .store
            .neutral_execution(&f.target.key.fence.run)
            .unwrap()
            .effect(&f.target.key)
            .unwrap()
            .answer,
        None
    );
}

// Catches: reply/edit accepting a foreign bot/chat or a non-message response,
// splitting a mutation into partial sends, or exceeding Telegram's text budget.
#[test]
fn telegram_request_rejects_wrong_message_and_invalid_service_receipt() {
    let run = uuid::Uuid::nil().to_string();
    let mut request = TelegramRequest {
        chat: 11,
        text: "text".into(),
        message: Some(MessageReference {
            bot: "b".into(),
            chat: 11,
            message: 17,
        }),
        actions: vec![],
    };
    for op in [TelegramOperation::Reply, TelegramOperation::Update] {
        request.wire("b", op, &run).unwrap();
        assert!(request.wire("other", op, &run).is_err());
        assert!(request.receipt("b", op, &json!(true)).is_err());
        assert!(
            request
                .receipt("b", op, &json!({"message_id":17,"chat":{"id":12}}))
                .is_err()
        );
    }
    assert!(request.wire("b", TelegramOperation::Send, &run).is_err());
    request.message = None;
    request.text = "😀".repeat(2049);
    assert!(request.wire("b", TelegramOperation::Send, &run).is_err());
    request.text.clear();
    assert!(request.wire("b", TelegramOperation::Send, &run).is_err());
}
