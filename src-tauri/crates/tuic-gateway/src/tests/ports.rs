use std::time::Duration;

use bytes::Bytes;
use futures::executor::block_on;
use http::{HeaderMap, Method};
use url::Url;

use crate::model::{TransportError, WireRequest, WireResponse};
use crate::ports::{Clock, Transport};
use crate::testing::{FakeClock, FakeTransport};

fn wire(path: &str) -> WireRequest {
    WireRequest {
        method: Method::GET,
        url: Url::parse(&format!("https://api.github.com{path}")).unwrap(),
        headers: HeaderMap::new(),
        body: None,
    }
}

fn ok(status: u16) -> WireResponse {
    WireResponse {
        status,
        headers: HeaderMap::new(),
        body: Bytes::new(),
    }
}

#[test]
fn fake_transport_records_requests_and_fake_clock_drives_time() {
    let transport = FakeTransport::new();
    transport.push_reply(Ok(ok(200)));
    transport.push_reply(Err(TransportError::Timeout));

    assert_eq!(block_on(transport.send(wire("/a"))).unwrap().status, 200);
    assert_eq!(
        block_on(transport.send(wire("/b"))),
        Err(TransportError::Timeout)
    );
    // Queue drained: still an error, never a panic.
    assert!(block_on(transport.send(wire("/c"))).is_err());
    let paths: Vec<_> = transport
        .requests()
        .iter()
        .map(|r| r.url.path().to_owned())
        .collect();
    assert_eq!(paths, ["/a", "/b", "/c"]);

    let clock = FakeClock::new();
    let (t0, w0) = (clock.now(), clock.wall());
    assert_eq!(clock.now(), t0, "time must not move by itself");
    clock.clone().advance(Duration::from_secs(90));
    assert_eq!(clock.now() - t0, Duration::from_secs(90));
    assert_eq!(
        clock.wall().duration_since(w0).unwrap(),
        Duration::from_secs(90)
    );
}
