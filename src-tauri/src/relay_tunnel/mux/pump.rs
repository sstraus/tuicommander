//! The two tasks behind a mux: a writer that seals and sends frames, and a
//! reader that opens records and dispatches frames. They are separate so a
//! full link can never stop us reading, which would deadlock two peers that
//! are both writing.

use tokio::time::Instant;

use super::*;

type SharedChannel = Arc<Mutex<SecureChannel>>;

pub(super) fn spawn(
    core: Arc<Core>,
    channel: SecureChannel,
    link_tx: mpsc::Sender<Vec<u8>>,
    link_rx: mpsc::Receiver<Vec<u8>>,
    data_rx: mpsc::Receiver<Frame>,
    ctl_rx: mpsc::UnboundedReceiver<Frame>,
    inbound_tx: mpsc::Sender<Inbound>,
) {
    let channel: SharedChannel = Arc::new(Mutex::new(channel));
    tokio::spawn(write_task(
        core.clone(),
        channel.clone(),
        link_tx,
        data_rx,
        ctl_rx,
    ));
    tokio::spawn(async move {
        let reason = read_loop(&core, &channel, link_rx, &inbound_tx).await;
        close(&core, reason);
    });
}

/// End the session once: wake every blocked reader, writer and opener.
fn close(core: &Core, reason: CloseReason) {
    let mut shared = core.lock();
    if shared.closed.is_some() {
        return;
    }
    shared.closed = Some(reason.clone());
    for (_, entry) in shared.streams.drain() {
        entry.credit.close();
    }
    drop(shared);
    core.connection_credit.close();
    let _ = core.closed_tx.send(Some(reason));
}

/// Resolves once the session has been closed (or can never close).
async fn wait_closed(rx: &mut watch::Receiver<Option<CloseReason>>) {
    loop {
        if rx.borrow().is_some() {
            return;
        }
        if rx.changed().await.is_err() {
            return;
        }
    }
}

async fn write_task(
    core: Arc<Core>,
    channel: SharedChannel,
    link_tx: mpsc::Sender<Vec<u8>>,
    mut data_rx: mpsc::Receiver<Frame>,
    mut ctl_rx: mpsc::UnboundedReceiver<Frame>,
) {
    let mut closed = core.closed_tx.subscribe();
    loop {
        let frame = tokio::select! {
            biased;
            _ = wait_closed(&mut closed) => return,
            Some(frame) = ctl_rx.recv() => frame,
            Some(frame) = data_rx.recv() => frame,
            else => return,
        };
        // Seal and send in one task, in order: the nonce sequence is the wire order.
        let sealed = frame
            .encode()
            .map_err(CloseReason::Frame)
            .and_then(|plain| {
                channel
                    .lock()
                    .expect("channel poisoned")
                    .seal(&plain)
                    .map_err(CloseReason::Crypto)
            });
        match sealed {
            Ok(record) => {
                if link_tx.send(record).await.is_err() {
                    return close(&core, CloseReason::LinkClosed);
                }
            }
            Err(reason) => return close(&core, reason),
        }
    }
}

async fn read_loop(
    core: &Arc<Core>,
    channel: &SharedChannel,
    mut link_rx: mpsc::Receiver<Vec<u8>>,
    inbound_tx: &mpsc::Sender<Inbound>,
) -> CloseReason {
    let config = core.config;
    let mut ping =
        tokio::time::interval_at(Instant::now() + config.ping_interval, config.ping_interval);
    ping.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
    let mut last_rx = Instant::now();
    let mut nonce = 0u64;
    let mut closed = core.closed_tx.subscribe();
    loop {
        tokio::select! {
            _ = wait_closed(&mut closed) => {
                return core.lock().closed.clone().unwrap_or(CloseReason::LinkClosed);
            }
            _ = core.shutdown.notified() => return CloseReason::LocalShutdown,
            record = link_rx.recv() => {
                let Some(record) = record else { return CloseReason::LinkClosed };
                let opened = channel.lock().expect("channel poisoned").open(&record);
                let frame = match opened {
                    Ok(plain) => match Frame::decode(&plain) {
                        Ok(frame) => frame,
                        Err(e) => return CloseReason::Frame(e),
                    },
                    Err(e) => return CloseReason::Crypto(e),
                };
                // Only an authenticated frame proves the peer is alive.
                last_rx = Instant::now();
                if let Err(reason) = dispatch(core, frame, inbound_tx) {
                    return reason;
                }
            }
            _ = ping.tick() => {
                if last_rx.elapsed() > config.dead_after {
                    return CloseReason::HeartbeatTimeout;
                }
                nonce += 1;
                let _ = core.ctl_tx.send(Frame::Ping { nonce });
            }
        }
    }
}

fn dispatch(
    core: &Arc<Core>,
    frame: Frame,
    inbound_tx: &mpsc::Sender<Inbound>,
) -> Result<(), CloseReason> {
    let ctl = |f: Frame| {
        let _ = core.ctl_tx.send(f);
    };
    match frame {
        Frame::Ping { nonce } => ctl(Frame::Pong { nonce }),
        Frame::Pong { .. } => {}
        Frame::Credential { bytes } => inbound_tx
            .try_send(Inbound::Credential(bytes))
            .map_err(|_| CloseReason::Protocol("credential flood"))?,
        Frame::Open { stream } => open_requested(core, stream, inbound_tx)?,
        Frame::Opened { stream } => {
            let mut shared = core.lock();
            let opened = shared
                .streams
                .get_mut(&stream)
                .and_then(|e| e.opened.take());
            match opened {
                Some(tx) => {
                    let _ = tx.send(Ok(()));
                }
                None => return Err(CloseReason::Protocol("unexpected Opened")),
            }
        }
        Frame::OpenFailed { stream, reason } => {
            let opened = core
                .lock()
                .streams
                .get_mut(&stream)
                .and_then(|e| e.opened.take());
            match opened {
                Some(tx) => {
                    core.remove(stream);
                    let _ = tx.send(Err(reason));
                }
                None => return Err(CloseReason::Protocol("unexpected OpenFailed")),
            }
        }
        Frame::Data { stream, bytes } => {
            let mut shared = core.lock();
            // Unknown stream: we reset it while the peer's frame was in flight.
            if let Some(entry) = shared.streams.get_mut(&stream) {
                if entry.unread + bytes.len() > STREAM_CREDIT {
                    return Err(CloseReason::Protocol("stream credit overrun"));
                }
                entry.unread += bytes.len();
                let _ = entry.to_app.send(Event::Data(bytes));
            }
        }
        Frame::WindowUpdate { stream, credit } => {
            let credit = credit as usize;
            let mut shared = core.lock();
            if let Some(entry) = shared.streams.get_mut(&stream) {
                if credit > entry.outstanding {
                    return Err(CloseReason::Protocol("credit beyond what was sent"));
                }
                entry.outstanding -= credit;
                entry.credit.add_permits(credit);
                core.connection_credit.add_permits(credit);
            }
        }
        Frame::Fin { stream } => {
            let mut shared = core.lock();
            let both = shared.streams.get_mut(&stream).map(|entry| {
                entry.remote_fin = true;
                let _ = entry.to_app.send(Event::Fin);
                entry.local_fin
            });
            if both == Some(true) {
                drop(shared);
                core.remove(stream);
            }
        }
        Frame::Reset { stream } => {
            if let Some(entry) = core.remove(stream) {
                let _ = entry.to_app.send(Event::Reset);
            }
        }
    }
    Ok(())
}

fn open_requested(
    core: &Arc<Core>,
    stream: u32,
    inbound_tx: &mpsc::Sender<Inbound>,
) -> Result<(), CloseReason> {
    if core.role != Role::Responder {
        return Err(CloseReason::Protocol("Open sent to the initiator"));
    }
    let refuse = |reason| {
        let _ = core.ctl_tx.send(Frame::OpenFailed { stream, reason });
    };
    let events = {
        let mut shared = core.lock();
        if stream <= shared.last_remote_id {
            return Err(CloseReason::Protocol("stream id reused"));
        }
        shared.last_remote_id = stream;
        if shared.streams.len() >= core.config.max_streams {
            drop(shared);
            refuse(OpenFailure::TooManyStreams);
            return Ok(());
        }
        let (to_app, events) = mpsc::unbounded_channel();
        shared.streams.insert(
            stream,
            Entry {
                to_app,
                credit: Arc::new(Semaphore::new(STREAM_CREDIT)),
                outstanding: 0,
                unread: 0,
                local_fin: false,
                remote_fin: false,
                opened: None,
            },
        );
        events
    };
    let incoming = IncomingStream::new(MuxStream::new(core.clone(), stream, events));
    // Nobody is accepting (or the backlog is full): answer, don't hang.
    if let Err(
        mpsc::error::TrySendError::Full(Inbound::Stream(s))
        | mpsc::error::TrySendError::Closed(Inbound::Stream(s)),
    ) = inbound_tx.try_send(Inbound::Stream(incoming))
    {
        s.reject(OpenFailure::TooManyStreams);
    }
    Ok(())
}
