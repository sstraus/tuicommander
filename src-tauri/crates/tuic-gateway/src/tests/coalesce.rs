use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::Duration;

use tokio::sync::Notify;

use crate::coalesce::{Coalescer, FlightLost};

type Reply = Result<Arc<String>, String>;
type Flight = Pin<Box<dyn Future<Output = Reply> + Send>>;

fn counting_work(sends: &Arc<AtomicUsize>, gate: &Arc<Notify>) -> impl FnOnce() -> Flight + use<> {
    let (sends, gate) = (sends.clone(), gate.clone());
    move || {
        Box::pin(async move {
            let n = sends.fetch_add(1, Ordering::SeqCst) + 1;
            gate.notified().await;
            Ok(Arc::new(format!("body-{n}")))
        })
    }
}

async fn wait_for(sends: &AtomicUsize, want: usize) {
    for _ in 0..200 {
        if sends.load(Ordering::SeqCst) >= want {
            return;
        }
        tokio::time::sleep(Duration::from_millis(5)).await;
    }
    panic!("sends never reached {want}");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn ten_concurrent_identical_reads_send_once() {
    let c = Arc::new(Coalescer::<&'static str, Reply>::new());
    let (sends, gate) = (Arc::new(AtomicUsize::new(0)), Arc::new(Notify::new()));
    let handles: Vec<_> = (0..10)
        .map(|_| {
            let (c, w) = (c.clone(), counting_work(&sends, &gate));
            tokio::spawn(async move { c.run("k", w).await })
        })
        .collect();
    wait_for(&sends, 1).await;
    tokio::time::sleep(Duration::from_millis(50)).await; // let the other nine join
    gate.notify_waiters();
    for h in handles {
        assert_eq!(h.await.unwrap().unwrap().unwrap().as_str(), "body-1");
    }
    assert_eq!(sends.load(Ordering::SeqCst), 1);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn cancelled_first_caller_does_not_cancel_the_others() {
    let c = Arc::new(Coalescer::<&'static str, Reply>::new());
    let (sends, gate) = (Arc::new(AtomicUsize::new(0)), Arc::new(Notify::new()));
    let first = {
        let (c, w) = (c.clone(), counting_work(&sends, &gate));
        tokio::spawn(async move { c.run("k", w).await })
    };
    wait_for(&sends, 1).await;
    let second = {
        let (c, w) = (c.clone(), counting_work(&sends, &gate));
        tokio::spawn(async move { c.run("k", w).await })
    };
    tokio::time::sleep(Duration::from_millis(50)).await;
    first.abort();
    let _ = first.await;
    gate.notify_waiters();
    assert_eq!(second.await.unwrap().unwrap().unwrap().as_str(), "body-1");
    assert_eq!(sends.load(Ordering::SeqCst), 1);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn writes_are_never_coalesced() {
    // Writes bypass the coalescer: the caller decides via Effect and never calls
    // `run`. What the coalescer owes them: a landed flight frees its key, so a
    // later identical call sends again instead of replaying a stale result.
    let c = Coalescer::<&'static str, Reply>::new();
    let (sends, gate) = (Arc::new(AtomicUsize::new(0)), Arc::new(Notify::new()));
    for want in 1..=2 {
        let w = counting_work(&sends, &gate);
        let g = gate.clone();
        let release = tokio::spawn(async move {
            tokio::time::sleep(Duration::from_millis(30)).await;
            g.notify_waiters();
        });
        let r = c.run("k", w).await.unwrap().unwrap();
        release.await.unwrap();
        assert_eq!(r.as_str(), format!("body-{want}"));
    }
    assert_eq!(sends.load(Ordering::SeqCst), 2);
}

#[tokio::test]
async fn panicking_flight_reports_lost_and_frees_the_key() {
    let c = Coalescer::<&'static str, u32>::new();
    let r = c.run("k", || async { panic!("boom") }).await;
    assert_eq!(r, Err(FlightLost));
    assert_eq!(c.run("k", || async { 7 }).await, Ok(7));
}
