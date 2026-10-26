//! Stream multiplexer over one authenticated `SecureChannel`.
//!
//! The desktop (initiator) opens a stream per accepted loopback socket; the
//! daemon (responder) receives it as an `IncomingStream` and decides where it
//! goes. The mux knows nothing about HTTP: it moves ordered bytes with
//! per-stream and per-connection credit so a stalled reader cannot grow memory
//! or starve the other streams, and it detects a silent relay with
//! authenticated pings.
//!
//! The link is a pair of channels carrying one sealed record per message, so
//! the same code runs over a WebSocket (`ws_link`) and over an in-memory pair
//! in tests. Only decrypted frames refresh the liveness deadline; a relay
//! status message never reaches this layer.

use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use tokio::sync::{Notify, Semaphore, mpsc, oneshot, watch};

mod pump;
#[cfg(test)]
mod tests;

use super::frames::{Frame, FrameError, MAX_DATA_CHUNK, OpenFailure};
use super::secure::{SecureChannel, SecureError};

pub(crate) const MAX_STREAMS: usize = 64;
/// Bytes a sender may have in flight on one stream.
pub(crate) const STREAM_CREDIT: usize = 64 * 1024;
/// Bytes in flight across all streams: about the relay's peer queue
/// (32 records), so the relay's wait-then-close policy is rarely reached.
pub(crate) const CONNECTION_CREDIT: usize = 512 * 1024;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Role {
    /// Desktop: opens streams.
    Initiator,
    /// Daemon: receives streams.
    Responder,
}

#[derive(Clone, Copy, Debug)]
pub(crate) struct MuxConfig {
    pub max_streams: usize,
    pub ping_interval: Duration,
    /// No authenticated frame for this long ends the session.
    pub dead_after: Duration,
}

impl Default for MuxConfig {
    fn default() -> Self {
        Self {
            max_streams: MAX_STREAMS,
            ping_interval: Duration::from_secs(15),
            dead_after: Duration::from_secs(45),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum CloseReason {
    LinkClosed,
    HeartbeatTimeout,
    Crypto(SecureError),
    Frame(FrameError),
    /// The peer broke the protocol (credit overrun, bad stream id, wrong role).
    Protocol(&'static str),
    LocalShutdown,
}

#[derive(Debug, PartialEq, Eq)]
pub(crate) enum StreamError {
    /// The peer or the local side aborted this stream.
    Reset,
    /// The whole session ended.
    Closed(CloseReason),
}

#[derive(Debug, PartialEq, Eq)]
pub(crate) enum OpenError {
    Refused(OpenFailure),
    TooManyStreams,
    Closed(CloseReason),
}

/// What reaches a stream's reader.
enum Event {
    Data(Vec<u8>),
    Fin,
    Reset,
}

struct Entry {
    to_app: mpsc::UnboundedSender<Event>,
    /// Send credit toward the peer for this stream.
    credit: Arc<Semaphore>,
    /// Bytes sent and not yet credited back; returned to the connection on removal.
    outstanding: usize,
    /// Bytes received and not yet read by the app.
    unread: usize,
    local_fin: bool,
    remote_fin: bool,
    opened: Option<oneshot::Sender<Result<(), OpenFailure>>>,
}

struct Shared {
    streams: HashMap<u32, Entry>,
    next_id: u32,
    last_remote_id: u32,
    closed: Option<CloseReason>,
}

struct Core {
    shared: Mutex<Shared>,
    connection_credit: Semaphore,
    data_tx: mpsc::Sender<Frame>,
    ctl_tx: mpsc::UnboundedSender<Frame>,
    closed_tx: watch::Sender<Option<CloseReason>>,
    shutdown: Notify,
    config: MuxConfig,
    role: Role,
}

impl Core {
    fn lock(&self) -> std::sync::MutexGuard<'_, Shared> {
        self.shared.lock().expect("mux state poisoned")
    }

    /// Remove a stream and give its unacknowledged bytes back to the connection.
    fn remove(&self, stream: u32) -> Option<Entry> {
        let entry = self.lock().streams.remove(&stream)?;
        self.connection_credit.add_permits(entry.outstanding);
        Some(entry)
    }
}

/// Cloneable handle for opening streams and sending connection frames.
#[derive(Clone)]
pub(crate) struct MuxHandle {
    core: Arc<Core>,
    closed_rx: watch::Receiver<Option<CloseReason>>,
}

/// Items the responder (or the initiator, for D) receives.
pub(crate) enum Inbound {
    Stream(IncomingStream),
    Credential(Vec<u8>),
}

pub(crate) struct Mux {
    pub handle: MuxHandle,
    pub inbound: mpsc::Receiver<Inbound>,
}

/// Start the pump task. `link_tx`/`link_rx` carry one sealed record per message.
pub(crate) fn start(
    role: Role,
    channel: SecureChannel,
    link_tx: mpsc::Sender<Vec<u8>>,
    link_rx: mpsc::Receiver<Vec<u8>>,
    config: MuxConfig,
) -> Mux {
    let (data_tx, data_rx) = mpsc::channel(32);
    let (ctl_tx, ctl_rx) = mpsc::unbounded_channel();
    let (inbound_tx, inbound) = mpsc::channel(16);
    let (closed_tx, closed_rx) = watch::channel(None);
    let core = Arc::new(Core {
        shared: Mutex::new(Shared {
            streams: HashMap::new(),
            next_id: 1,
            last_remote_id: 0,
            closed: None,
        }),
        connection_credit: Semaphore::new(CONNECTION_CREDIT),
        data_tx,
        ctl_tx,
        closed_tx,
        shutdown: Notify::new(),
        config,
        role,
    });
    pump::spawn(
        core.clone(),
        channel,
        link_tx,
        link_rx,
        data_rx,
        ctl_rx,
        inbound_tx,
    );
    Mux {
        handle: MuxHandle { core, closed_rx },
        inbound,
    }
}

impl MuxHandle {
    pub(crate) fn shutdown(&self) {
        self.core.shutdown.notify_one();
    }

    /// Resolves with the reason once the session has ended.
    pub(crate) async fn closed(&mut self) -> CloseReason {
        loop {
            if let Some(reason) = self.closed_rx.borrow().clone() {
                return reason;
            }
            if self.closed_rx.changed().await.is_err() {
                return CloseReason::LinkClosed;
            }
        }
    }

    pub(crate) fn close_reason(&self) -> Option<CloseReason> {
        self.closed_rx.borrow().clone()
    }

    /// Initiator only: open a stream and wait for the peer to accept it.
    pub(crate) async fn open(&self) -> Result<MuxStream, OpenError> {
        debug_assert_eq!(self.core.role, Role::Initiator);
        let (opened_tx, opened_rx) = oneshot::channel();
        let (stream, events) = {
            let mut shared = self.core.lock();
            if let Some(reason) = &shared.closed {
                return Err(OpenError::Closed(reason.clone()));
            }
            if shared.streams.len() >= self.core.config.max_streams {
                return Err(OpenError::TooManyStreams);
            }
            let id = shared.next_id;
            shared.next_id += 1;
            let (to_app, events) = mpsc::unbounded_channel();
            shared.streams.insert(
                id,
                Entry {
                    to_app,
                    credit: Arc::new(Semaphore::new(STREAM_CREDIT)),
                    outstanding: 0,
                    unread: 0,
                    local_fin: false,
                    remote_fin: false,
                    opened: Some(opened_tx),
                },
            );
            (id, events)
        };
        let handle = MuxStream::new(self.core.clone(), stream, events);
        let _ = self.core.ctl_tx.send(Frame::Open { stream });
        match opened_rx.await {
            Ok(Ok(())) => Ok(handle),
            Ok(Err(reason)) => Err(OpenError::Refused(reason)),
            Err(_) => Err(OpenError::Closed(
                self.close_reason().unwrap_or(CloseReason::LinkClosed),
            )),
        }
    }

    /// Send D (or any connection-level credential) to the peer.
    pub(crate) fn send_credential(&self, bytes: Vec<u8>) {
        let _ = self.core.ctl_tx.send(Frame::Credential { bytes });
    }
}

/// A stream the peer opened, not yet answered.
pub(crate) struct IncomingStream {
    stream: MuxStream,
}

impl IncomingStream {
    fn new(stream: MuxStream) -> Self {
        Self { stream }
    }

    pub(crate) fn accept(self) -> MuxStream {
        let _ = self.stream.core.ctl_tx.send(Frame::Opened {
            stream: self.stream.id,
        });
        self.stream
    }

    pub(crate) fn reject(self, reason: OpenFailure) {
        // Dropping the stream afterwards finds nothing to remove, so it sends
        // no Reset on top of the OpenFailed.
        if self.stream.core.remove(self.stream.id).is_some() {
            let _ = self.stream.core.ctl_tx.send(Frame::OpenFailed {
                stream: self.stream.id,
                reason,
            });
        }
    }
}

/// One logical byte stream.
pub(crate) struct MuxStream {
    core: Arc<Core>,
    id: u32,
    /// Behind an async mutex so a task can read while another writes, through
    /// shared references (`tokio::join!` on one stream, as a TCP bridge does).
    events: tokio::sync::Mutex<mpsc::UnboundedReceiver<Event>>,
    finished_local: AtomicBool,
    done: AtomicBool,
}

impl MuxStream {
    fn new(core: Arc<Core>, id: u32, events: mpsc::UnboundedReceiver<Event>) -> Self {
        Self {
            core,
            id,
            events: tokio::sync::Mutex::new(events),
            finished_local: AtomicBool::new(false),
            done: AtomicBool::new(false),
        }
    }

    pub(crate) fn id(&self) -> u32 {
        self.id
    }

    /// Next chunk; `Ok(None)` is the peer's orderly end of stream.
    pub(crate) async fn read(&self) -> Result<Option<Vec<u8>>, StreamError> {
        match self.events.lock().await.recv().await {
            Some(Event::Data(bytes)) => {
                let credit = bytes.len();
                if let Some(entry) = self.core.lock().streams.get_mut(&self.id) {
                    entry.unread -= credit;
                }
                let _ = self.core.ctl_tx.send(Frame::WindowUpdate {
                    stream: self.id,
                    credit: credit as u32,
                });
                Ok(Some(bytes))
            }
            Some(Event::Fin) => Ok(None),
            Some(Event::Reset) => Err(StreamError::Reset),
            None => Err(self.closed_error()),
        }
    }

    fn closed_error(&self) -> StreamError {
        match self.core.lock().closed.clone() {
            Some(reason) => StreamError::Closed(reason),
            None => StreamError::Reset,
        }
    }

    /// Write all of `buf`, waiting for credit; this is where backpressure lives.
    pub(crate) async fn write_all(&self, buf: &[u8]) -> Result<(), StreamError> {
        for chunk in buf.chunks(MAX_DATA_CHUNK) {
            // Look up under the lock, report after releasing it: closed_error
            // takes the same non-reentrant lock.
            let credit = self
                .core
                .lock()
                .streams
                .get(&self.id)
                .map(|entry| entry.credit.clone());
            let Some(credit) = credit else {
                return Err(self.closed_error());
            };
            let n = chunk.len() as u32;
            // Stream credit first, then connection credit: only the peer's
            // reader frees either, so a stalled stream holds at most its own.
            let Ok(stream_permit) = credit.acquire_many(n).await else {
                return Err(self.closed_error());
            };
            let Ok(conn_permit) = self.core.connection_credit.acquire_many(n).await else {
                return Err(self.closed_error());
            };
            {
                let mut shared = self.core.lock();
                let Some(entry) = shared.streams.get_mut(&self.id) else {
                    return Err(StreamError::Reset); // permits drop back unused
                };
                entry.outstanding += chunk.len();
            }
            stream_permit.forget();
            conn_permit.forget();
            self.core
                .data_tx
                .send(Frame::Data {
                    stream: self.id,
                    bytes: chunk.to_vec(),
                })
                .await
                .map_err(|_| self.closed_error())?;
        }
        Ok(())
    }

    /// Orderly end of our direction; the peer can still write.
    pub(crate) async fn finish(&self) {
        if self.finished_local.swap(true, Ordering::SeqCst) {
            return;
        }
        {
            let mut shared = self.core.lock();
            let both = shared.streams.get_mut(&self.id).map(|entry| {
                entry.local_fin = true;
                entry.remote_fin
            });
            if both == Some(true) {
                drop(shared);
                self.core.remove(self.id);
            }
        }
        let _ = self.core.data_tx.send(Frame::Fin { stream: self.id }).await;
    }

    /// Abort both directions. A stream the pump already forgot sends nothing.
    pub(crate) fn reset(&self) {
        self.done.store(true, Ordering::SeqCst);
        if self.core.remove(self.id).is_some() {
            let _ = self.core.ctl_tx.send(Frame::Reset { stream: self.id });
        }
    }
}

impl Drop for MuxStream {
    fn drop(&mut self) {
        // A stream dropped mid-flight aborts, so the peer does not wait forever.
        // One already forgotten by the pump (both halves finished) sends nothing.
        self.reset();
    }
}
