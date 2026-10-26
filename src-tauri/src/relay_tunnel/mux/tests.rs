use std::time::Duration;

use super::*;
use crate::relay_tunnel::secure::{ChannelBinding, Handshake, PairingSecret, SecureChannel};

const WAIT: Duration = Duration::from_secs(5);

fn channels() -> (SecureChannel, SecureChannel) {
    let p = PairingSecret::generate().unwrap();
    let b = ChannelBinding {
        pairing_id: "p",
        room: "r",
    };
    let mut i = Handshake::initiator(&p, &b).unwrap();
    let mut r = Handshake::responder(&p, &b).unwrap();
    r.read(&i.write().unwrap()).unwrap();
    i.read(&r.write().unwrap()).unwrap();
    (i.finish().unwrap(), r.finish().unwrap())
}

fn cfg() -> MuxConfig {
    MuxConfig {
        ping_interval: Duration::from_secs(60),
        dead_after: Duration::from_secs(300),
        ..MuxConfig::default()
    }
}

/// Initiator and responder joined by a forwarding task per direction; `filter`
/// sees every record and returns whether to deliver it (the relay's powers).
fn joined(
    i_cfg: MuxConfig,
    r_cfg: MuxConfig,
    filter: impl FnMut(&[u8]) -> bool + Send + 'static,
) -> (Mux, Mux) {
    let (ci, cr) = channels();
    let (i_tx, mut i_wire) = mpsc::channel::<Vec<u8>>(64);
    let (r_tx, mut r_wire) = mpsc::channel::<Vec<u8>>(64);
    let (to_r, r_rx) = mpsc::channel::<Vec<u8>>(64);
    let (to_i, i_rx) = mpsc::channel::<Vec<u8>>(64);
    let mut filter = filter;
    tokio::spawn(async move {
        while let Some(rec) = i_wire.recv().await {
            if filter(&rec) && to_r.send(rec).await.is_err() {
                break;
            }
        }
    });
    tokio::spawn(async move {
        while let Some(rec) = r_wire.recv().await {
            if to_i.send(rec).await.is_err() {
                break;
            }
        }
    });
    (
        start(Role::Initiator, ci, i_tx, i_rx, i_cfg),
        start(Role::Responder, cr, r_tx, r_rx, r_cfg),
    )
}

fn pair() -> (Mux, Mux) {
    joined(cfg(), cfg(), |_| true)
}

async fn accept(m: &mut Mux) -> IncomingStream {
    match tokio::time::timeout(WAIT, m.inbound.recv())
        .await
        .unwrap()
        .unwrap()
    {
        Inbound::Stream(s) => s,
        Inbound::Credential(_) => panic!("expected a stream"),
    }
}

/// Echo every chunk back, then finish when the peer does.
fn spawn_echo(s: MuxStream) {
    tokio::spawn(async move {
        while let Ok(Some(chunk)) = s.read().await {
            if s.write_all(&chunk).await.is_err() {
                return;
            }
        }
        s.finish().await;
    });
}

/// Open a stream and accept it on the other side (open waits for the accept).
async fn open_accepted(i: &Mux, r: &mut Mux) -> (MuxStream, MuxStream) {
    let (ours, theirs) = tokio::join!(i.handle.open(), async { accept(r).await.accept() });
    (ours.unwrap(), theirs)
}

async fn read_exactly(s: &MuxStream, n: usize) -> Vec<u8> {
    let mut out = Vec::new();
    while out.len() < n {
        out.extend(
            tokio::time::timeout(WAIT, s.read())
                .await
                .unwrap()
                .unwrap()
                .unwrap(),
        );
    }
    out
}

#[tokio::test]
async fn bytes_cross_in_order_in_both_directions() {
    let (i, mut r) = pair();
    tokio::spawn(async move {
        spawn_echo(accept(&mut r).await.accept());
        r
    });
    let s = i.handle.open().await.unwrap();
    // 300 KiB: many chunks and several credit refills.
    let payload: Vec<u8> = (0..300 * 1024).map(|n| (n % 251) as u8).collect();
    let (written, echoed) = tokio::join!(s.write_all(&payload), read_exactly(&s, payload.len()));
    written.unwrap();
    assert_eq!(echoed, payload, "bytes were reordered, lost or duplicated");
}

// Catches responses crossing between concurrent sockets (HTTP correlation).
#[tokio::test]
async fn concurrent_streams_do_not_mix_their_bytes() {
    let (i, mut r) = pair();
    tokio::spawn(async move {
        loop {
            let inc = accept(&mut r).await;
            spawn_echo(inc.accept());
        }
    });
    let mut tasks = Vec::new();
    for n in 0u8..16 {
        let s = i.handle.open().await.unwrap();
        tasks.push(tokio::spawn(async move {
            let msg = vec![n; 40_000];
            s.write_all(&msg).await.unwrap();
            assert_eq!(read_exactly(&s, msg.len()).await, msg);
        }));
    }
    for t in tasks {
        t.await.unwrap();
    }
}

// Catches half-close being treated as full close (a request body ended, the
// response still has to flow back).
#[tokio::test]
async fn finish_ends_one_direction_only() {
    let (i, mut r) = pair();
    let (s, theirs) = open_accepted(&i, &mut r).await;
    s.write_all(b"request").await.unwrap();
    s.finish().await;
    assert_eq!(read_exactly(&theirs, 7).await, b"request");
    assert_eq!(theirs.read().await, Ok(None));
    theirs.write_all(b"response").await.unwrap();
    assert_eq!(read_exactly(&s, 8).await, b"response");
}

#[tokio::test]
async fn reset_and_drop_abort_the_peer_stream() {
    let (i, mut r) = pair();
    let (s, theirs) = open_accepted(&i, &mut r).await;
    drop(s);
    assert_eq!(
        tokio::time::timeout(WAIT, theirs.read()).await.unwrap(),
        Err(StreamError::Reset)
    );
}

#[tokio::test]
async fn a_refused_open_reports_the_reason() {
    let (i, mut r) = pair();
    tokio::spawn(async move {
        accept(&mut r)
            .await
            .reject(OpenFailure::UpstreamUnreachable);
        r
    });
    assert_eq!(
        i.handle.open().await.err(),
        Some(OpenError::Refused(OpenFailure::UpstreamUnreachable))
    );
}

// Catches an unbounded stream table on the daemon side.
#[tokio::test]
async fn the_responder_refuses_streams_beyond_its_limit() {
    let r_cfg = MuxConfig {
        max_streams: 2,
        ..cfg()
    };
    let (i, mut r) = joined(cfg(), r_cfg, |_| true);
    tokio::spawn(async move {
        let mut kept = Vec::new();
        loop {
            kept.push(accept(&mut r).await.accept());
        }
    });
    let _a = i.handle.open().await.unwrap();
    let _b = i.handle.open().await.unwrap();
    assert_eq!(
        i.handle.open().await.err(),
        Some(OpenError::Refused(OpenFailure::TooManyStreams))
    );
}

// Catches one stalled reader consuming the whole connection: its writer
// blocks at its stream credit while another stream keeps working.
#[tokio::test]
async fn a_stalled_stream_blocks_only_itself() {
    let (i, mut r) = pair();
    let (stalled, _their_stalled) = open_accepted(&i, &mut r).await; // never read
    let (busy, their_busy) = open_accepted(&i, &mut r).await;
    spawn_echo(their_busy);

    let blocked = tokio::time::timeout(
        Duration::from_millis(400),
        stalled.write_all(&vec![1u8; STREAM_CREDIT * 3]),
    )
    .await;
    assert!(blocked.is_err(), "write finished although nobody reads");
    busy.write_all(b"still alive").await.unwrap();
    assert_eq!(read_exactly(&busy, 11).await, b"still alive");
}

// Catches a reset leaking the connection credit it had in flight: nine
// resets of 64 KiB each would exhaust the 512 KiB connection window.
#[tokio::test]
async fn resetting_a_stream_returns_its_connection_credit() {
    let (i, mut r) = pair();
    tokio::spawn(async move {
        loop {
            let inc = accept(&mut r).await;
            let _ = inc.accept(); // dropped at once: nothing is read
        }
    });
    for _ in 0..10 {
        let s = tokio::time::timeout(WAIT, i.handle.open())
            .await
            .expect("open hung")
            .unwrap();
        tokio::time::timeout(WAIT, s.write_all(&vec![0u8; STREAM_CREDIT]))
            .await
            .expect("connection credit leaked")
            .ok();
        s.reset();
    }
}

/// A peer driven by hand over the raw wire, for what honest code never does.
struct RawPeer {
    channel: SecureChannel,
    tx: mpsc::Sender<Vec<u8>>,
    rx: mpsc::Receiver<Vec<u8>>,
}

impl RawPeer {
    fn against_initiator(cfg: MuxConfig) -> (Mux, RawPeer) {
        let (ci, cr) = channels();
        let (i_tx, raw_rx) = mpsc::channel(64);
        let (raw_tx, i_rx) = mpsc::channel(64);
        (
            start(Role::Initiator, ci, i_tx, i_rx, cfg),
            RawPeer {
                channel: cr,
                tx: raw_tx,
                rx: raw_rx,
            },
        )
    }
    async fn send(&mut self, f: Frame) {
        let rec = self.channel.seal(&f.encode().unwrap()).unwrap();
        self.tx.send(rec).await.unwrap();
    }
    async fn recv(&mut self) -> Frame {
        let rec = tokio::time::timeout(WAIT, self.rx.recv())
            .await
            .unwrap()
            .unwrap();
        Frame::decode(&self.channel.open(&rec).unwrap()).unwrap()
    }
}

// Catches a peer exceeding the credit it was given: memory must stay bounded.
#[tokio::test]
async fn data_beyond_the_granted_credit_ends_the_session() {
    let (i, mut raw) = RawPeer::against_initiator(cfg());
    let open = tokio::spawn({
        let h = i.handle.clone();
        async move { h.open().await }
    });
    let Frame::Open { stream } = raw.recv().await else {
        panic!()
    };
    raw.send(Frame::Opened { stream }).await;
    let _keep_unread = open.await.unwrap().unwrap();
    for _ in 0..=STREAM_CREDIT / MAX_DATA_CHUNK {
        raw.send(Frame::Data {
            stream,
            bytes: vec![0; MAX_DATA_CHUNK],
        })
        .await;
    }
    let mut h = i.handle.clone();
    assert_eq!(
        tokio::time::timeout(WAIT, h.closed()).await.unwrap(),
        CloseReason::Protocol("stream credit overrun")
    );
}

// Catches a relay that goes quiet leaving the desktop "connected" forever.
#[tokio::test]
async fn a_silent_peer_times_out() {
    let c = MuxConfig {
        ping_interval: Duration::from_millis(50),
        dead_after: Duration::from_millis(250),
        ..MuxConfig::default()
    };
    let (i, _raw) = RawPeer::against_initiator(c);
    let mut h = i.handle.clone();
    assert_eq!(
        tokio::time::timeout(WAIT, h.closed()).await.unwrap(),
        CloseReason::HeartbeatTimeout
    );
}

// Catches forged records keeping a dead peer "alive": only authenticated
// frames count, and a forgery ends the session at once.
#[tokio::test]
async fn forged_records_do_not_refresh_liveness_and_end_the_session() {
    let (i, raw) = RawPeer::against_initiator(cfg());
    raw.tx.send(vec![7u8; 48]).await.unwrap();
    let mut h = i.handle.clone();
    assert_eq!(
        tokio::time::timeout(WAIT, h.closed()).await.unwrap(),
        CloseReason::Crypto(SecureError::Record)
    );
}

// Catches a relay dropping one record while the session carries on: the
// responder sees a hole in the nonce sequence and must end the session.
#[tokio::test]
async fn a_record_dropped_by_the_relay_ends_the_session() {
    let mut count = 0;
    let (i, mut r) = joined(cfg(), cfg(), move |_| {
        count += 1;
        count != 2 // drop the 2nd record the initiator sends
    });
    let accepted = tokio::spawn(async move { accept(&mut r).await.accept() });
    let s = i.handle.open().await.unwrap(); // record 1: Open
    let theirs = accepted.await.unwrap();
    let _ = s.write_all(b"lost").await; // record 2: dropped
    let _ = s.write_all(b"after").await; // record 3: breaks the sequence
    assert_eq!(
        tokio::time::timeout(WAIT, theirs.read()).await.unwrap(),
        Err(StreamError::Closed(CloseReason::Crypto(
            SecureError::Record
        )))
    );
}

#[tokio::test]
async fn the_credential_arrives_only_through_the_channel() {
    let (mut i, r) = pair();
    r.handle.send_credential(b"daemon-token".to_vec());
    match tokio::time::timeout(WAIT, i.inbound.recv())
        .await
        .unwrap()
        .unwrap()
    {
        Inbound::Credential(c) => assert_eq!(c, b"daemon-token"),
        Inbound::Stream(_) => panic!("unexpected stream"),
    }
}

#[tokio::test]
async fn local_shutdown_ends_open_streams_with_the_reason() {
    let (i, mut r) = pair();
    let (s, _theirs) = open_accepted(&i, &mut r).await;
    i.handle.shutdown();
    assert_eq!(
        tokio::time::timeout(WAIT, s.read()).await.unwrap(),
        Err(StreamError::Closed(CloseReason::LocalShutdown))
    );
    assert!(matches!(
        i.handle.open().await.err(),
        Some(OpenError::Closed(_))
    ));
}
