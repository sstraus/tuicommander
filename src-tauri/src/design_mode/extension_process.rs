//! Wait and stop ownership for the dedicated extension browser, independent of Tokio lifetime.
use std::process::Command;
use std::sync::{Arc, mpsc};
use std::time::Duration;
use tokio::sync::{oneshot, watch};

type Completion = Option<Result<(), String>>;
struct Control {
    stop: mpsc::Sender<()>,
    completed: watch::Receiver<Completion>,
}
impl Drop for Control {
    fn drop(&mut self) {
        let _ = self.stop.send(());
    }
}

#[derive(Clone)]
pub(crate) struct ExtensionProcess {
    control: Arc<Control>,
    pid: u32,
}
impl ExtensionProcess {
    pub(crate) async fn spawn(mut command: Command) -> Result<Self, String> {
        let (stop, receive) = mpsc::channel();
        let (completed, completion) = watch::channel(None);
        let (started, spawn_result) = oneshot::channel();
        std::thread::Builder::new()
            .name("design-browser-owner".into())
            .spawn(move || {
                let mut child = match command.spawn() {
                    Ok(child) => child,
                    Err(error) => {
                        let _ = started.send(Err(error.to_string()));
                        return;
                    }
                };
                if started.send(Ok(child.id())).is_err() {
                    let _ = child.kill();
                    let _ = child.wait();
                    return;
                }
                loop {
                    match child.try_wait() {
                        Ok(Some(_)) => {
                            let _ = completed.send(Some(Ok(())));
                            return;
                        }
                        Ok(None) => {}
                        Err(error) => {
                            tracing::warn!(%error, "Could not observe Design Mode browser exit");
                            let result = child.wait().map(|_| ()).map_err(|e| e.to_string());
                            let _ = completed.send(Some(result));
                            return;
                        }
                    }
                    match receive.recv_timeout(Duration::from_millis(100)) {
                        Ok(()) | Err(mpsc::RecvTimeoutError::Disconnected) => {
                            // Only this thread owns Child; a re-used PID can never be killed.
                            // Exit can race Stop; a successful wait still proves it was reaped.
                            let _ = child.kill();
                            let result = child.wait().map(|_| ()).map_err(|e| e.to_string());
                            let _ = completed.send(Some(result));
                            return;
                        }
                        Err(mpsc::RecvTimeoutError::Timeout) => {}
                    }
                }
            })
            .map_err(|e| e.to_string())?;
        let pid = spawn_result.await.map_err(|e| e.to_string())??;
        Ok(Self {
            control: Arc::new(Control {
                stop,
                completed: completion,
            }),
            pid,
        })
    }

    pub(crate) async fn close(&self) -> Result<(), String> {
        tracing::debug!(pid = self.pid, "Closing owned Design Mode browser");
        let mut completed = self.control.completed.clone();
        let _ = self.control.stop.send(());
        let result = completed
            .wait_for(Option::is_some)
            .await
            .map_err(|e| e.to_string())?;
        result
            .clone()
            .expect("completion predicate requires a result")
    }
}

#[cfg(all(test, unix))]
mod tests {
    use super::*;

    // Catches: Stop discarding a launch handle and leaving its dedicated browser process alive.
    // Repeated Stop must observe the same completed wait, not race a disappearing reply channel.
    #[tokio::test]
    async fn stop_reaps_the_process_started_by_design_mode() {
        let mut command = Command::new("/bin/sh");
        command.args(["-c", "kill -STOP $$"]);
        let process = ExtensionProcess::spawn(command).await.unwrap();
        let pid = process.pid as i32;
        let (first, second) = tokio::join!(process.close(), process.close());
        first.unwrap();
        second.unwrap();
        process.close().await.unwrap();
        // SAFETY: signal zero observes only the child started by this test.
        assert_eq!(unsafe { libc::kill(pid, 0) }, -1);
        assert_eq!(
            std::io::Error::last_os_error().raw_os_error(),
            Some(libc::ESRCH)
        );
    }
}
