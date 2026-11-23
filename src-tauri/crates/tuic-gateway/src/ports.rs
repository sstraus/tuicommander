//! The two doors to the outside world. Policy code takes these, never
//! `reqwest` or `Instant::now()`.

use std::time::{Instant, SystemTime};

use futures::future::{BoxFuture, FutureExt};
use futures::stream::{self, StreamExt};

use crate::invalidate::Signal;
use crate::model::{TransportError, WireRequest, WireResponse, WireStream};

pub trait Transport: Send + Sync + 'static {
    fn send(&self, req: WireRequest) -> BoxFuture<'static, Result<WireResponse, TransportError>>;

    /// Like `send`, with the body left unread. The default buffers it, so a
    /// transport without real streaming still answers.
    fn send_stream(
        &self,
        req: WireRequest,
    ) -> BoxFuture<'static, Result<WireStream, TransportError>> {
        let reply = self.send(req);
        async move {
            let r = reply.await?;
            Ok(WireStream {
                status: r.status,
                headers: r.headers,
                body: stream::once(std::future::ready(Ok(r.body))).boxed(),
            })
        }
        .boxed()
    }
}

pub trait Clock: Send + Sync + 'static {
    /// Monotonic time, for durations and windows.
    fn now(&self) -> Instant;
    /// Wall time, for provider reset timestamps.
    fn wall(&self) -> SystemTime;
}

/// Told of every successful write after the gateway cache applied it, so a
/// derived view of the provider's state (the app's parsed PR status) can drop
/// what the write made stale.
pub trait SignalSink: Send + Sync + 'static {
    fn on_signal(&self, signal: &Signal);
}
