//! Copy bytes between a TCP socket and a multiplexed stream, both directions,
//! with half-close. Used by the daemon (stream to the loopback listener) and by
//! the desktop (loopback accept to a new stream).

use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpStream;

use super::mux::MuxStream;

pub(crate) async fn bridge(stream: MuxStream, tcp: TcpStream) {
    let (mut tcp_read, mut tcp_write) = tcp.into_split();
    // `Err` means the stream was reset or the session ended: nothing more can
    // flow, so the whole bridge stops instead of waiting for the socket.
    let from_stream = async {
        loop {
            match stream.read().await {
                Ok(Some(bytes)) => {
                    if tcp_write.write_all(&bytes).await.is_err() {
                        return Err(());
                    }
                }
                Ok(None) => {
                    let _ = tcp_write.shutdown().await;
                    return Ok(());
                }
                Err(_) => return Err(()),
            }
        }
    };
    let from_socket = async {
        let mut buf = vec![0u8; 16 * 1024];
        loop {
            match tcp_read.read(&mut buf).await {
                Ok(0) => {
                    stream.finish().await;
                    return Ok(());
                }
                Ok(n) => {
                    if stream.write_all(&buf[..n]).await.is_err() {
                        return Err(());
                    }
                }
                Err(_) => return Err(()),
            }
        }
    };
    tokio::pin!(from_stream, from_socket);
    let failed = tokio::select! {
        res = &mut from_stream => match res {
            Ok(()) => from_socket.await.is_err(),
            Err(()) => true,
        },
        res = &mut from_socket => match res {
            Ok(()) => from_stream.await.is_err(),
            Err(()) => true,
        },
    };
    if failed {
        stream.reset();
    }
}
