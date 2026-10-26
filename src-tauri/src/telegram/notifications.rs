use crate::state::AppEvent;

/// Only the bound terminal contributes authored progress, never another peer.
pub(super) fn notice(event: &AppEvent, pty: &str) -> Option<String> {
    let AppEvent::ProgressRecorded { payload, .. } = event else {
        return None;
    };
    let entry = &payload["entry"];
    if entry["ptyId"].as_str()? != pty {
        return None;
    }
    if !matches!(entry["type"].as_str()?, "done" | "blocked") {
        return None;
    }
    let text = entry["text"].as_str()?;
    (!text.is_empty()).then(|| text.to_owned())
}

#[cfg(all(test, unix))]
mod tests {
    use super::*;
    use serde_json::json;

    // Catches: another peer's outcome leaks to the phone, intent becomes a
    // completed reply, or authored language/whitespace is rewritten.
    #[tokio::test]
    async fn notices_keep_bound_authored_language_and_ignore_other_progress() {
        use crate::telegram::tests::{FakeServer, PEER, setup};
        use axum::http::StatusCode;
        let (_dir, paths) = setup();
        let server = FakeServer::start(vec![
            (StatusCode::OK, json!({"ok":true,"result":{"message_id":7}})),
            (StatusCode::OK, json!({"ok":true,"result":{"message_id":8}})),
        ])
        .await;
        let runtime = crate::telegram::tests::outbound_tests::runtime(paths, server.address).await;
        crate::state::tests_support::insert_dummy_session(&runtime.state, PEER);
        crate::test_support::agent_session(&runtime.state, PEER, crate::pty::SHELL_IDLE);
        let state = runtime.state.clone();
        let (commands, receive) = tokio::sync::mpsc::channel(1);
        let worker = tokio::spawn(runtime.run(receive));
        // Let the worker subscribe; a native command reply is the readiness barrier.
        let (reply, done) = tokio::sync::oneshot::channel();
        commands
            .send(crate::telegram::runtime::Command::Update {
                value: json!({}),
                reply,
            })
            .await
            .unwrap();
        done.await.unwrap().unwrap();
        for (pty, kind) in [
            ("other", "done"),
            (PEER, "intent"),
            (PEER, "done"),
            (PEER, "blocked"),
        ] {
            state
                .event_bus
                .send(AppEvent::ProgressRecorded {
                    repo_path: "repo".into(),
                    payload: json!({"entry":{"id":7,"ptyId":pty,"type":kind,"text":" Finito.\n"}}),
                })
                .unwrap();
        }
        tokio::time::timeout(std::time::Duration::from_secs(30), async {
            while server.requests().len() < 2 {
                tokio::time::sleep(std::time::Duration::from_millis(10)).await;
            }
        })
        .await
        .expect("runtime did not deliver bound progress");
        drop(commands);
        worker.await.unwrap();
        let (_, session) = state.session_maps.sessions.remove(PEER).unwrap();
        let mut session = session.lock();
        session._child.kill().unwrap();
        session._child.wait().unwrap();
        let requests = server.requests();
        assert_eq!(requests.len(), 2);
        for (method, body) in requests {
            assert!(method.ends_with("sendMessage"));
            assert_eq!(body["chat_id"], 1111111);
            assert_eq!(body["text"], " Finito.\n");
        }
    }
}

/// Use the existing registration and allowlisted destination; never register on behalf of a run.
pub(crate) fn automation_notice(
    store: crate::automations::store::RunStore,
    run: crate::automations::run::AutomationRun,
    text: String,
) {
    if let Some(sender) = super::runtime::HANDLE.get() {
        let _ = sender.try_send(super::runtime::Command::Automation {
            store,
            run: Box::new(run),
            text,
        });
    }
}
impl super::runtime::Runtime {
    pub(super) async fn automation_notice(
        &mut self,
        store: &crate::automations::store::RunStore,
        run: &crate::automations::run::AutomationRun,
        text: &str,
    ) {
        self.retire_registration();
        if !self.enabled() {
            return;
        }
        let Some(peer) = self.registered_peer() else {
            return;
        };
        if self.state.live_pty_for_peer(peer).as_deref() != run.session_id.as_deref()
            || run.session_id.is_none()
        {
            return;
        }
        let Ok(chat) = self.destination() else { return };
        if !crate::automations::notifications::claim(store, run, "telegram").unwrap_or(false) {
            return;
        }
        let result = self.outbound.send(chat, text, None).await;
        crate::automations::notifications::settle(store, run, "telegram", result.is_ok());
    }
}

#[cfg(all(test, unix))]
mod automation_tests {
    use crate::automations::{
        model::AutomationDefinition,
        run::{RunDetails, RunStatus, RunTrigger},
        store::RunOwner,
    };
    use crate::telegram::tests::{FakeServer, setup};
    use axum::http::StatusCode;
    use serde_json::json;
    // Catches scheduler notices bypassing the existing registered-agent opt-in.
    #[tokio::test]
    async fn automation_telegram_respects_registration_ownership_and_durable_dedup() {
        let (dir, paths) = setup();
        let server = FakeServer::start(vec![(
            StatusCode::OK,
            json!({"ok":true,"result":{"message_id":1}}),
        )])
        .await;
        let mut runtime =
            crate::telegram::tests::outbound_tests::runtime(paths, server.address).await;
        runtime.clear_registration();
        let owner = RunOwner::acquire_at(&dir.path().join("runs.sqlite3"), 0).unwrap();
        let definition: AutomationDefinition = serde_json::from_value(json!({
            "id":"notice", "name":"Review", "prompt":"private", "run_config":"codex", "repository":"/repo", "workspace":{"mode":"existing"}, "cron":"0 9 * * *", "timezone":"UTC", "enabled":true, "grace_secs":43200, "overlap":"skip", "max_duration_secs":3600,"precheck":null
        })).unwrap();
        let run = owner
            .store()
            .reserve(&definition, RunTrigger::Manual, 1)
            .unwrap()
            .unwrap();
        let run = owner
            .store()
            .transition(
                &run.id,
                RunStatus::NeedsYou,
                RunDetails {
                    session_id: Some("owned".into()),
                    ..Default::default()
                },
                2,
            )
            .unwrap();
        runtime
            .automation_notice(owner.store(), &run, "notice")
            .await;
        assert_eq!(
            server.requests(),
            [] as [(std::string::String, serde_json::Value); 0]
        );
        // Re-register the owning live PTY, then another run must remain quiet.
        let peer = crate::telegram::tests::PEER;
        crate::state::tests_support::insert_dummy_session(&runtime.state, peer);
        crate::test_support::agent_session(&runtime.state, peer, crate::pty::SHELL_IDLE);
        runtime
            .tool(
                peer,
                "target-mcp",
                crate::telegram::tool::Input::Register {},
            )
            .await
            .unwrap();
        runtime
            .automation_notice(owner.store(), &run, "foreign")
            .await;
        assert_eq!(
            server.requests(),
            [] as [(std::string::String, serde_json::Value); 0]
        );
        let mut owned = run.clone();
        owned.session_id = Some(peer.into());
        runtime
            .automation_notice(owner.store(), &owned, "owned notice")
            .await;
        runtime
            .automation_notice(owner.store(), &owned, "duplicate")
            .await;
        assert_eq!(server.requests().len(), 1);
        assert_eq!(server.requests()[0].1["text"], "owned notice");
        assert_eq!(
            owner
                .store()
                .notification(&run.id, "needs_you", "telegram")
                .unwrap()
                .status,
            crate::automations::run::DeliveryStatus::Confirmed
        );
        let (_, session) = runtime.state.session_maps.sessions.remove(peer).unwrap();
        let mut session = session.lock();
        session._child.kill().unwrap();
        session._child.wait().unwrap();
    }
}
