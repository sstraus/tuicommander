//! Bounded native-port command delivery. Heartbeats are not command acknowledgements.
use serde_json::{Value, json};
use std::{
    collections::HashMap,
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, Ordering},
    },
    time::Duration,
};
use tokio::sync::{mpsc, oneshot};

pub(crate) const COMMAND_TIMEOUT: Duration = Duration::from_secs(10);
const LIMIT: usize = 16;

pub(crate) struct CommandChannel {
    pub(crate) session_id: String,
    tx: mpsc::Sender<Value>,
    rx: tokio::sync::Mutex<mpsc::Receiver<Value>>,
    pending: Mutex<HashMap<String, oneshot::Sender<Value>>>,
    closed: AtomicBool,
    disconnected: tokio::sync::Notify,
}
impl Default for CommandChannel {
    fn default() -> Self {
        let (tx, rx) = mpsc::channel(LIMIT);
        Self {
            session_id: String::new(),
            tx,
            rx: tokio::sync::Mutex::new(rx),
            pending: Mutex::default(),
            closed: AtomicBool::new(false),
            disconnected: tokio::sync::Notify::new(),
        }
    }
}
struct Pending<'a>(&'a CommandChannel, String);
impl Drop for Pending<'_> {
    fn drop(&mut self) {
        self.0.pending.lock().unwrap().remove(&self.1);
    }
}
impl CommandChannel {
    pub(crate) fn new(session_id: String) -> Self {
        Self {
            session_id,
            ..Default::default()
        }
    }
    pub(crate) async fn request(
        &self,
        action: &str,
        args: Value,
        session: &str,
    ) -> Result<Value, String> {
        let id = uuid::Uuid::new_v4().to_string();
        let (tx, rx) = oneshot::channel();
        {
            let mut pending = self.pending.lock().unwrap();
            if self.closed.load(Ordering::Acquire) {
                return Err(
                    "browser_disconnected: Restart Design Mode to reconnect the browser".into(),
                );
            }
            if pending.len() >= LIMIT {
                return Err("command_queue_full: Too many browser commands".into());
            }
            pending.insert(id.clone(), tx);
        }
        let _pending = Pending(self, id.clone());
        self.tx.try_send(json!({"kind":"browser_command", "requestId":id, "action":action, "args":args, "sessionId":session}))
            .map_err(|_| "command_queue_full: Browser command queue is full".to_owned())?;
        tokio::time::timeout(COMMAND_TIMEOUT, rx)
            .await
            .map_err(|_| "command_timeout: Browser did not answer within 10 seconds".to_owned())?
            .map_err(|_| {
                "browser_disconnected: Design Mode stopped or browser disconnected".to_owned()
            })
    }
    pub(crate) async fn next(&self) -> Option<Value> {
        let mut rx = self.rx.lock().await;
        loop {
            if self.closed.load(Ordering::Acquire) {
                return None;
            }
            let command = tokio::select! {
                _ = self.disconnected.notified() => return None,
                command = rx.recv() => command?,
            };
            if self
                .pending
                .lock()
                .unwrap()
                .contains_key(command["requestId"].as_str()?)
            {
                return Some(command);
            }
        }
    }
    pub(crate) fn reply(&self, id: &str, reply: Value) -> bool {
        self.pending
            .lock()
            .unwrap()
            .remove(id)
            .is_some_and(|tx| tx.send(reply).is_ok())
    }
    pub(crate) fn cancel(&self) {
        {
            let mut pending = self.pending.lock().unwrap();
            self.closed.store(true, Ordering::Release);
            pending.clear();
        }
        self.disconnected.notify_one();
    }
}
pub(crate) type Channels = Arc<Mutex<HashMap<String, Arc<CommandChannel>>>>;

#[cfg(test)]
mod tests {
    use super::*;
    // Catches: crossed replies and Stop leaving an agent waiting until the timeout.
    #[tokio::test]
    async fn command_replies_are_correlated_and_stop_cancels_waiters() {
        let channel = Arc::new(CommandChannel::default());
        let c = channel.clone();
        let task =
            tokio::spawn(async move { c.request("eval", json!({"script":"1+1"}), "agent").await });
        let command = channel.next().await.unwrap();
        assert_eq!(command["action"], "eval");
        assert!(!channel.reply("unrelated", json!(0)));
        assert!(channel.reply(
            command["requestId"].as_str().unwrap(),
            json!({"ok":true,"result":2})
        ));
        assert_eq!(task.await.unwrap().unwrap()["result"], 2);
        let c = channel.clone();
        let task = tokio::spawn(async move { c.request("reload", json!({}), "agent").await });
        channel.next().await.unwrap();
        channel.cancel();
        assert!(
            task.await
                .unwrap()
                .unwrap_err()
                .starts_with("browser_disconnected:")
        );
    }
    // Catches: native disconnect leaving a live queue that accepts commands nobody can answer.
    #[tokio::test(start_paused = true)]
    async fn disconnected_channel_rejects_new_commands_and_releases_receiver() {
        let channel = CommandChannel::default();
        channel.cancel();
        let result = channel.request("screenshot", json!({}), "agent").await;
        assert!(
            result.unwrap_err().starts_with("browser_disconnected:"),
            "a disconnected extension must not leave the agent waiting for command_timeout"
        );
        assert!(channel.next().await.is_none());
    }

    // Catches: cancellation releasing agent requests but leaving the extension poll stuck.
    #[tokio::test(start_paused = true)]
    async fn disconnect_releases_an_already_waiting_receiver() {
        let channel = Arc::new(CommandChannel::default());
        let receiver = channel.clone();
        let poll = tokio::spawn(async move { receiver.next().await });
        tokio::task::yield_now().await;
        channel.cancel();
        assert!(
            tokio::time::timeout(Duration::from_secs(1), poll)
                .await
                .unwrap()
                .unwrap()
                .is_none()
        );
    }

    // Catches: a silent extension hanging a tool call forever or executing an expired command later.
    #[tokio::test(start_paused = true)]
    async fn mute_browser_times_out_and_drops_pending_command() {
        let channel = Arc::new(CommandChannel::default());
        let c = channel.clone();
        let task = tokio::spawn(async move { c.request("reload", json!({}), "agent").await });
        tokio::task::yield_now().await;
        tokio::time::advance(COMMAND_TIMEOUT).await;
        assert!(
            task.await
                .unwrap()
                .unwrap_err()
                .starts_with("command_timeout:")
        );
        assert!(channel.pending.lock().unwrap().is_empty());
        assert!(
            tokio::time::timeout(Duration::from_secs(1), channel.next())
                .await
                .is_err()
        );
    }
}
