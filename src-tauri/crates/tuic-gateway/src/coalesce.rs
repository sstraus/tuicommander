//! Request coalescing: concurrent callers with the same key share one flight.
//!
//! The flight runs in its own spawned task, so a caller that is dropped never
//! cancels it for the others. Only reads belong here: the caller decides via
//! `Effect` and never routes a write through `run`.

use std::collections::HashMap;
use std::future::Future;
use std::hash::Hash;
use std::sync::{Arc, Mutex, MutexGuard};

use tokio::sync::watch;

/// The in-flight work ended without a result (its task panicked or was aborted).
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
#[error("coalesced flight ended without a result")]
pub struct FlightLost;

type Flights<K, V> = Arc<Mutex<HashMap<K, watch::Receiver<Option<V>>>>>;

fn lock<K, V>(m: &Mutex<HashMap<K, V>>) -> MutexGuard<'_, HashMap<K, V>> {
    m.lock().unwrap_or_else(|e| e.into_inner())
}

/// Frees the key when the flight ends, including by panic.
struct Landing<K: Hash + Eq, V> {
    flights: Flights<K, V>,
    key: K,
}

impl<K: Hash + Eq, V> Drop for Landing<K, V> {
    fn drop(&mut self) {
        lock(&self.flights).remove(&self.key);
    }
}

pub struct Coalescer<K, V> {
    flights: Flights<K, V>,
}

impl<K: Hash + Eq + Clone + Send + 'static, V: Clone + Send + Sync + 'static> Default
    for Coalescer<K, V>
{
    fn default() -> Self {
        Self::new()
    }
}

impl<K: Hash + Eq + Clone + Send + 'static, V: Clone + Send + Sync + 'static> Coalescer<K, V> {
    pub fn new() -> Self {
        Self {
            flights: Arc::default(),
        }
    }

    /// Runs `work` unless a flight for `key` is already airborne, then returns
    /// that flight's result. Must be called inside a tokio runtime.
    pub async fn run<F, Fut>(&self, key: K, work: F) -> Result<V, FlightLost>
    where
        F: FnOnce() -> Fut,
        Fut: Future<Output = V> + Send + 'static,
    {
        let mut rx = {
            let mut flights = lock(&self.flights);
            match flights.get(&key) {
                Some(rx) => rx.clone(),
                None => {
                    let (tx, rx) = watch::channel(None);
                    flights.insert(key.clone(), rx.clone());
                    let fut = work();
                    let landing = Landing {
                        flights: self.flights.clone(),
                        key,
                    };
                    tokio::spawn(async move {
                        let value = fut.await;
                        drop(landing); // free the key before waking waiters
                        tx.send_replace(Some(value));
                    });
                    rx
                }
            }
        };
        let landed = rx.wait_for(Option::is_some).await.map_err(|_| FlightLost)?;
        landed.clone().ok_or(FlightLost)
    }
}
