//! Workflow calls share the existing worker, rate limiter, BotApi and credentials.
use super::{
    Error,
    runtime::{Command, HANDLE, Runtime},
};
use crate::workflows::{TelegramOperation, TelegramRequest};
use serde_json::Value;
use tokio::sync::oneshot;

pub(crate) async fn execute(
    bot: String,
    operation: TelegramOperation,
    request: TelegramRequest,
    notification_run: String,
) -> Result<Value, Error> {
    let (reply, receive) = oneshot::channel();
    HANDLE
        .get()
        .ok_or(Error::State)?
        .send(Command::Workflow {
            bot,
            operation,
            request,
            notification_run,
            reply,
        })
        .await
        .map_err(|_| Error::State)?;
    receive.await.map_err(|_| Error::State)?
}

impl Runtime {
    pub(super) async fn workflow(
        &mut self,
        bot: &str,
        operation: TelegramOperation,
        request: TelegramRequest,
        notification_run: &str,
    ) -> Result<Value, Error> {
        if !self.enabled() || bot != self.config.bot_alias {
            return Err(Error::Config);
        }
        let (method, body) = request
            .wire(bot, operation, notification_run)
            .map_err(|_| Error::Config)?;
        self.outbound.call(request.chat, method, body).await
    }

    pub(super) async fn workflow_callback(&mut self, value: &Value) -> Option<bool> {
        let query = &value["callback_query"];
        let data = query["data"]
            .as_str()
            .filter(|data| data.starts_with("w:"))?;
        if !self.enabled() {
            return Some(false);
        }
        let message = &query["message"];
        let Some(chat) = message["chat"]["id"].as_i64() else {
            return Some(false);
        };
        let Some(id) = query["id"].as_str().filter(|id| !id.is_empty()) else {
            return Some(false);
        };
        // An undecidable tap is rejected and consumed. Propagating a ledger or
        // configuration error here would prevent Inbound from saving its offset.
        let accepted = message["chat"]["type"] == "private"
            && message["date"].as_i64().is_some_and(|date| date > 0)
            && query["from"]["id"].as_i64() == Some(chat)
            && self
                .outbound
                .paths
                .allowlist()
                .is_ok_and(|ids| ids.contains(&chat))
            && message["message_id"].as_i64().is_some_and(|message_id| {
                self.state
                    .workflow_runtime
                    .telegram_answer(&self.config.bot_alias, chat, message_id, data)
                    .unwrap_or(false)
            });
        // The durable decision precedes ack. Ack failure must not replay consent.
        let _ = self
            .outbound
            .call(
                chat,
                "answerCallbackQuery",
                serde_json::json!({"callback_query_id": id}),
            )
            .await;
        Some(accepted)
    }
}

#[cfg(test)]
mod tests {
    use super::super::{
        BotApi, Config,
        outbound::Outbound,
        tests::{FakeServer, setup},
    };
    use super::*;
    use axum::http::StatusCode;
    use serde_json::json;
    use std::sync::Arc;

    // Catches: bypassing the existing serialized worker/authorization path or
    // projecting send/edit/reply to the wrong Bot API method or message.
    #[tokio::test]
    async fn workflow_operations_use_existing_worker_without_registered_peer() {
        let (_dir, paths) = setup();
        let fixtures: Value =
            serde_json::from_str(include_str!("fixtures/workflow-contract.json")).unwrap();
        let server = FakeServer::start(
            ["send", "update", "reply"]
                .map(|name| (StatusCode::OK, fixtures[name]["response"].clone()))
                .into(),
        )
        .await;
        let state = Arc::new(crate::state::tests_support::make_test_app_state());
        let mut runtime = Runtime::new(
            state,
            Config::load(&paths).unwrap().unwrap(),
            paths.clone(),
            "no-peer".into(),
        )
        .unwrap();
        runtime.outbound = Outbound::new(
            paths.clone(),
            BotApi::loopback(paths.clone(), server.address),
        );
        let (send, mut receive) = tokio::sync::mpsc::channel(4);
        let worker = tokio::spawn(async move { runtime.run_ref(&mut receive).await });
        let run = uuid::Uuid::nil().to_string();
        for (op, message) in [
            (TelegramOperation::Send, None),
            (TelegramOperation::Update, Some(17)),
            (TelegramOperation::Reply, Some(17)),
        ] {
            let request: TelegramRequest = serde_json::from_value(json!({"chat":1111111,"text":"exact text","message":message.map(|message| json!({"bot":"test-bot","chat":1111111,"message":message}))})).unwrap();
            let (reply, result) = oneshot::channel();
            send.send(Command::Workflow {
                bot: "test-bot".into(),
                operation: op,
                request: request.clone(),
                notification_run: run.clone(),
                reply,
            })
            .await
            .unwrap();
            let response = result.await.unwrap().unwrap();
            request.receipt("test-bot", op, &response).unwrap();
        }
        let requests = server.requests();
        assert_eq!(requests.len(), 3);
        for (request, name) in requests.iter().zip(["send", "update", "reply"]) {
            assert!(
                request
                    .0
                    .ends_with(fixtures[name]["method"].as_str().unwrap())
            );
            assert_eq!(request.1, fixtures[name]["request"]);
        }
        drop(send);
        worker.await.unwrap();
    }
    // Catches: workflow callback ingress silently falling through to volatile
    // peer routing, or accepting a callback from an unauthorized sender/chat.
    #[tokio::test]
    async fn workflow_callback_reaches_owner_without_peer_and_rejects_wrong_sender() {
        let (_dir, paths) = setup();
        let state = Arc::new(crate::state::tests_support::make_test_app_state());
        let mut runtime = Runtime::new(
            state,
            Config::load(&paths).unwrap().unwrap(),
            paths.clone(),
            "no-peer".into(),
        )
        .unwrap();
        let server = FakeServer::start(vec![]).await;
        runtime.outbound = Outbound::new(paths.clone(), BotApi::loopback(paths, server.address));
        let mut update = json!({"callback_query": {"id": "query", "data": "w:00000000000000000000000000000000:0000000000000000000000000000", "from": {"id": 1111111}, "message": {"date": 1, "message_id": 17, "chat": {"id": 1111111, "type": "private"}}}});
        assert_eq!(
            runtime.update(update.clone()).await.unwrap(),
            json!({"accepted":true})
        );
        update["callback_query"]["from"]["id"] = json!(2222222);
        assert_eq!(runtime.workflow_callback(&update).await, Some(false));
        update["callback_query"]["from"]["id"] = json!(1111111);
        update["callback_query"]["message"]["chat"]["type"] = json!("group");
        assert_eq!(runtime.workflow_callback(&update).await, Some(false));
        update["callback_query"]["data"] = json!("tc1:ordinary-choice");
        assert_eq!(runtime.workflow_callback(&update).await, None);
    }

    // Catches: an undecidable workflow button (no executor, deleted run, receipt
    // never retained) failing Runtime::update. Inbound::poll propagates that error
    // before it writes the offset, so the poller refetches the same callback every
    // second forever and no later Telegram message is ever processed.
    #[tokio::test]
    async fn undecidable_workflow_callback_is_consumed_so_polling_advances() {
        let (_dir, paths) = setup();
        let server = FakeServer::start(vec![
            (StatusCode::OK, json!({"ok": true, "result": true})),
            (StatusCode::INTERNAL_SERVER_ERROR, json!({"ok":false})),
        ])
        .await;
        let state = Arc::new(crate::state::tests_support::make_test_app_state());
        let mut runtime = Runtime::new(
            state,
            Config::load(&paths).unwrap().unwrap(),
            paths.clone(),
            "no-peer".into(),
        )
        .unwrap();
        runtime.outbound = Outbound::new(
            paths.clone(),
            BotApi::loopback(paths.clone(), server.address),
        );
        let update = json!({"callback_query": {"id": "query", "data": "w:00000000000000000000000000000000:0000000000000000000000000000", "from": {"id": 1111111}, "message": {"date": 1, "message_id": 17, "chat": {"id": 1111111, "type": "private"}}}});
        assert!(
            runtime.update(update.clone()).await.is_ok(),
            "an undecidable workflow callback must be consumed, not replayed by the poller"
        );
        assert_eq!(server.requests()[0].1, json!({"callback_query_id":"query"}));
        assert!(server.requests()[0].0.ends_with("/answerCallbackQuery"));
        let recent = serde_json::to_value(runtime.state.telegram_recent.lock().snapshot()).unwrap();
        assert!(
            recent
                .as_array()
                .unwrap()
                .iter()
                .any(|event| event["direction"] == "in"
                    && event["outcome"] == "rejected: not authorized")
        );
        assert!(
            runtime.update(update.clone()).await.is_ok(),
            "ack failure must not replay consent"
        );
        std::fs::write(paths.file("allowed_chat_ids"), "invalid\n").unwrap();
        assert!(
            runtime.update(update.clone()).await.is_ok(),
            "unreadable chat policy must reject and consume the callback"
        );
        assert_eq!(
            server.requests().len(),
            2,
            "invalid policy must not send an ack"
        );
        std::fs::write(
            paths.file("config.json"),
            r#"{"enabled":false,"bot_alias":"test-bot"}"#,
        )
        .unwrap();
        assert!(
            runtime.update(update).await.is_ok(),
            "disabled callback must not poison the cursor"
        );
        assert_eq!(
            server.requests().len(),
            2,
            "disabled config must not send an ack"
        );
    }
}
