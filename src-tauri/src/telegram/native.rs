use super::{
    Error,
    mail::{MailPort, PendingMail},
    runtime::{Command, HANDLE, Runtime},
};
use crate::state::AppState;
use serde_json::{Value, json};
use std::sync::Arc;
use tokio::sync::{mpsc, oneshot};

pub(super) async fn offer(
    state: &Arc<AppState>,
    sid: &str,
    mail: &PendingMail,
) -> Result<(), Error> {
    let result = crate::mcp_http::mcp_transport::local_peer_call_with_message_id(
        state,
        &json!({"action":"send","to":mail.recipient,"message":mail.content,"urgency":"normal"}),
        Some(sid),
        Some(mail.id.clone()),
    )
    .await;
    if result.get("error").is_some() {
        return Err(Error::State);
    }
    // Inbox-only is a valid handoff; readiness/wake arbitration belongs to TUIC.
    Ok(())
}
pub(super) struct NativeMail {
    commands: mpsc::Sender<Command>,
}
impl NativeMail {
    pub(super) fn new(commands: mpsc::Sender<Command>) -> Self {
        Self { commands }
    }
}
impl MailPort for NativeMail {
    async fn offer(&mut self, mail: &PendingMail) -> Result<bool, Error> {
        let (reply, receive) = oneshot::channel();
        self.commands
            .send(Command::Deliver {
                mail: mail.clone(),
                reply,
            })
            .await
            .map_err(|_| Error::State)?;
        let value = receive.await.map_err(|_| Error::State)??;
        value["accepted"].as_bool().ok_or(Error::Protocol)
    }
    async fn update(&mut self, value: &Value) -> Result<(), Error> {
        if value.get("callback_query").is_none()
            && value.get("stopped_message_generation").is_none()
        {
            return Ok(());
        }
        let (reply, receive) = oneshot::channel();
        self.commands
            .send(Command::Update {
                value: value.clone(),
                reply,
            })
            .await
            .map_err(|_| Error::State)?;
        receive.await.map_err(|_| Error::State)??;
        Ok(())
    }
}

/// Desktop and daemon share one supervisor; Inbound holds the sole polling lock.
pub(crate) fn start(state: &Arc<AppState>) {
    let Ok(paths) = super::settings::paths() else {
        return;
    };
    let (commands, mut receive) = mpsc::channel(100);
    if HANDLE.set(commands.clone()).is_err() {
        super::runtime::alert(state, Error::AlreadyOwned);
        return;
    }
    let state = state.clone();
    tokio::spawn(async move {
        let sid = format!("telegram:{}", uuid::Uuid::new_v4());
        loop {
            let revision = super::settings::revision(&paths);
            let config = match super::Config::load(&paths) {
                Ok(Some(config)) => config,
                Ok(None) => {
                    wait(&paths, &revision, &mut receive, Error::Config).await;
                    continue;
                }
                Err(error) => {
                    super::runtime::alert(&state, error);
                    wait(&paths, &revision, &mut receive, error).await;
                    continue;
                }
            };
            let port = NativeMail::new(commands.clone());
            let mut inbound = match super::inbound::Inbound::with_port(paths.clone(), port) {
                Ok(Some(inbound)) => inbound,
                Ok(None) => continue,
                Err(error) => {
                    // A standby must not overwrite the live owner's status or registration.
                    if error == Error::AlreadyOwned {
                        tokio::select! {
                            _ = wait(&paths, &revision, &mut receive, error) => {},
                            _ = tokio::time::sleep(std::time::Duration::from_secs(1)) => {},
                        }
                    } else {
                        super::runtime::alert(&state, error);
                        wait(&paths, &revision, &mut receive, error).await;
                    }
                    continue;
                }
            };
            super::settings::status(&paths, false, None, false);
            super::settings::registration_status(&paths, None);
            let runtime = match Runtime::new(state.clone(), config, paths.clone(), sid.clone()) {
                Ok(runtime) => runtime,
                Err(error) => {
                    super::settings::status(&paths, false, Some(error), false);
                    wait(&paths, &revision, &mut receive, error).await;
                    continue;
                }
            };
            let registered = crate::mcp_http::mcp_transport::local_peer_call_with_message_id(
                &state,
                &json!({"action":"register","name":"telegram-adapter"}),
                Some(&sid),
                None,
            )
            .await;
            if registered.get("error").is_some() {
                super::settings::status(&paths, false, Some(Error::State), false);
                wait(&paths, &revision, &mut receive, Error::State).await;
                continue;
            }
            tokio::select! {
                _ = super::settings::changed(&paths, &revision) => {},
                _ = runtime.run_ref(&mut receive) => {},
                error = async {
                    loop {
                        match inbound.poll().await {
                            Ok(super::inbound::Poll::Backoff(delay)) => tokio::time::sleep(delay).await,
                            Ok(super::inbound::Poll::Accepted(count)) => super::settings::status(&paths, true, None, count > 0),
                            Err(error) => {
                                super::settings::status(&paths, false, Some(error), false);
                                super::runtime::alert(&state, error);
                                if matches!(error, Error::Unauthorized | Error::Conflict | Error::Rejected(403 | 404)) {
                                    return error;
                                }
                                tokio::time::sleep(std::time::Duration::from_secs(1)).await;
                            }
                        }
                    }
                } => {
                    // Stop polling on terminal faults, but reject tool calls until setup changes.
                    wait(&paths, &revision, &mut receive, error).await;
                },
            }
            super::settings::registration_status(&paths, None);
            super::settings::status(&paths, false, None, false);
        }
    });
}

/// Reject commands while disabled, stopped or waiting for another local owner.
async fn wait(
    paths: &super::Paths,
    revision: &[Option<std::time::SystemTime>],
    receive: &mut mpsc::Receiver<Command>,
    error: Error,
) {
    loop {
        tokio::select! {
            _ = super::settings::changed(paths, revision) => return,
            command = receive.recv() => {
                let Some(command) = command else { return; };
                let reply = match command {
                    Command::Tool { reply, .. } | Command::Deliver { reply, .. } | Command::Update { reply, .. } => reply,
                };
                let _ = reply.send(Err(error));
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // Catches: a standby queues MCP calls forever or overwrites the actual owner's shared status.
    #[tokio::test]
    async fn standby_rejects_commands_without_overwriting_owner_status() {
        for error in [Error::AlreadyOwned, Error::Config, Error::Unauthorized] {
            let (_dir, paths) = crate::telegram::tests::setup();
            let owner = super::super::Owner::acquire(&paths).unwrap();
            super::super::settings::status(&paths, true, None, false);
            let before = std::fs::read(paths.file("status.json")).unwrap();
            let revision = super::super::settings::revision(&paths);
            let (send, mut receive) = mpsc::channel(1);
            let task_paths = paths.clone();
            let task =
                tokio::spawn(
                    async move { wait(&task_paths, &revision, &mut receive, error).await },
                );
            let (reply, result) = oneshot::channel();
            send.send(Command::Tool {
                caller: "peer".into(),
                sid: "mcp".into(),
                input: super::super::tool::Input::Register {},
                reply,
            })
            .await
            .unwrap();
            assert_eq!(result.await.unwrap(), Err(error));
            assert_eq!(std::fs::read(paths.file("status.json")).unwrap(), before);
            assert!(matches!(
                super::super::Owner::acquire(&paths),
                Err(Error::AlreadyOwned)
            ));
            task.abort();
            drop(owner);
            assert!(super::super::Owner::acquire(&paths).is_ok());
        }
    }
}
