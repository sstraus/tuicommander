//! App-side adapters of the provider gateway; the policy lives in
//! `tuic_gateway`.

pub(crate) mod direct;
mod l2_sqlite;
pub(crate) mod plugin_egress;
pub(crate) mod signals;
pub(crate) mod stats;
mod transport;

#[cfg(test)]
mod signal_tests;
#[cfg(test)]
mod transport_tests;

#[cfg(test)]
mod plugin_egress_gateway_tests;

#[cfg(test)]
mod bypass_tests;

#[cfg(test)]
mod direct_tests;

#[cfg(test)]
mod caller_tests;

#[cfg(test)]
mod plugin_egress_tests;

#[cfg(test)]
mod stats_tests;

use std::path::Path;
use std::sync::Arc;
use std::time::{Instant, SystemTime};

use tuic_gateway::cache::Cache;
use tuic_gateway::failure::SeededJitter;
use tuic_gateway::gateway::Gateway;
use tuic_gateway::l2::L2;
use tuic_gateway::ports::Clock;

use transport::ReqwestTransport;

/// The one gateway type the app holds.
pub(crate) type AppGateway = Gateway<ReqwestTransport>;

/// L1 body budget; the persistent tier has its own cap (`l2::CAP_BYTES`).
const L1_MAX_BYTES: u64 = 64 * 1024 * 1024;

pub(crate) struct SystemClock;

impl Clock for SystemClock {
    fn now(&self) -> Instant {
        Instant::now()
    }

    fn wall(&self) -> SystemTime {
        SystemTime::now()
    }
}

/// The gateway and its persistent tier, whose maintenance task needs a
/// runtime and so starts apart from construction.
pub(crate) struct ProviderGateway {
    pub(crate) gateway: Arc<AppGateway>,
    l2: Option<Arc<L2>>,
}

impl ProviderGateway {
    /// An unusable cache file is only a miss: the gateway then runs on L1 alone.
    pub(crate) fn new(client: reqwest::Client, cache_db: &Path) -> Self {
        let clock: Arc<dyn Clock> = Arc::new(SystemClock);
        let l2 = match l2_sqlite::SqliteColdStore::open(cache_db) {
            Ok(store) => Some(Arc::new(L2::new(Arc::new(store), clock.clone()))),
            Err(error) => {
                tracing::warn!(source = "gateway", %error, "provider cache unavailable; L1 only");
                None
            }
        };
        let built = Self::build(client, clock, l2);
        direct::register(&built.gateway);
        built
    }

    /// A test that swaps `AppState.http_client` swaps the gateway's client too.
    #[cfg(test)]
    pub(crate) fn l1_only(client: reqwest::Client) -> Self {
        Self::build(client, Arc::new(SystemClock), None)
    }

    /// A benchmark moves time itself, so a TTL or quota window needs no real wait.
    #[cfg(test)]
    pub(crate) fn l1_only_with_clock(client: reqwest::Client, clock: Arc<dyn Clock>) -> Self {
        Self::build(client, clock, None)
    }

    fn build(client: reqwest::Client, clock: Arc<dyn Clock>, l2: Option<Arc<L2>>) -> Self {
        let cache = match &l2 {
            Some(l2) => Cache::with_l2(clock.clone(), L1_MAX_BYTES, l2.clone()),
            None => Cache::new(clock.clone(), L1_MAX_BYTES),
        };
        let seed = clock
            .wall()
            .duration_since(SystemTime::UNIX_EPOCH)
            .map_or(1, |d| d.as_nanos() as u64);
        let gateway = Gateway::new(
            clock,
            Arc::new(ReqwestTransport::new(client)),
            cache,
            Arc::new(SeededJitter::new(seed)),
        );
        Self {
            gateway: Arc::new(gateway),
            l2,
        }
    }

    /// Flush and eviction loop of the persistent tier; call inside a runtime.
    pub(crate) fn spawn_maintenance(&self) {
        if let Some(l2) = &self.l2 {
            drop(l2.spawn_maintenance());
        }
    }
}
