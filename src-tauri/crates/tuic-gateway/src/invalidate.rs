//! Invalidation (design "Invalidation (R2)"): a tag index maintained on cache
//! insert, and the `Signal`s that drop entries through it.
//!
//! A read entry is indexed under the tags of its route; a `Search` entry is
//! additionally marked so any write of its account and host drops it. The
//! facade calls `Cache::apply`.

use std::collections::{HashMap, HashSet};
use std::sync::Mutex;

use crate::cache::{Cache, CacheKey};
use crate::model::{AccountKey, Effect};
use crate::routes::{Route, Tag, TtlClass};

/// Something that makes cached entries of one account stale.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Signal {
    /// A write went through the gateway: drops `tags` (its item, its
    /// collection) and every search entry of the account and host.
    Wrote { account: AccountKey, tags: Vec<Tag> },
    /// An external change (poller update, branch head change): drops the
    /// entries under `tags`; searches stay.
    RepoChanged { account: AccountKey, tags: Vec<Tag> },
    /// Login or logout: drops every entry of the account and host.
    Logout { account: AccountKey },
}

impl Signal {
    /// The signal a successful write of `route` raises.
    #[must_use]
    pub fn from_write(account: AccountKey, route: &Route) -> Self {
        debug_assert_eq!(route.effect, Effect::Write);
        Self::Wrote {
            account,
            tags: route.tags.clone(),
        }
    }
}

#[derive(Default)]
struct Inner {
    by_tag: HashMap<Tag, HashSet<CacheKey>>,
    by_key: HashMap<CacheKey, KeyMeta>,
}

struct KeyMeta {
    tags: Vec<Tag>,
    search: bool,
}

/// Tag -> cache keys. Entries are small; scans by account are linear on
/// purpose (logout and search drops are rare next to reads).
#[derive(Default)]
pub(crate) struct TagIndex(Mutex<Inner>);

impl TagIndex {
    fn lock(&self) -> std::sync::MutexGuard<'_, Inner> {
        self.0
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
    }

    pub(crate) fn add(&self, key: &CacheKey, route: &Route) {
        let mut g = self.lock();
        g.remove(key);
        for t in &route.tags {
            g.by_tag.entry(t.clone()).or_default().insert(key.clone());
        }
        g.by_key.insert(
            key.clone(),
            KeyMeta {
                tags: route.tags.clone(),
                search: route.ttl == TtlClass::Search,
            },
        );
    }

    pub(crate) fn remove(&self, key: &CacheKey) {
        self.lock().remove(key);
    }

    /// Removes and returns the keys a signal covers.
    fn take(&self, signal: &Signal) -> Vec<CacheKey> {
        let mut g = self.lock();
        let (account, tags, search) = match signal {
            Signal::Wrote { account, tags } => (account, tags.as_slice(), true),
            Signal::RepoChanged { account, tags } => (account, tags.as_slice(), false),
            Signal::Logout { account } => {
                let keys: Vec<_> = g
                    .by_key
                    .keys()
                    .filter(|k| k.account == *account)
                    .cloned()
                    .collect();
                for k in &keys {
                    g.remove(k);
                }
                return keys;
            }
        };
        let mut keys: HashSet<CacheKey> = tags
            .iter()
            .filter_map(|t| g.by_tag.get(t))
            .flatten()
            .filter(|k| k.account == *account)
            .cloned()
            .collect();
        if search {
            keys.extend(
                g.by_key
                    .iter()
                    .filter(|(k, m)| m.search && k.account == *account)
                    .map(|(k, _)| k.clone()),
            );
        }
        for k in &keys {
            g.remove(k);
        }
        keys.into_iter().collect()
    }
}

impl Inner {
    fn remove(&mut self, key: &CacheKey) {
        let Some(meta) = self.by_key.remove(key) else {
            return;
        };
        for t in meta.tags {
            if let Some(set) = self.by_tag.get_mut(&t) {
                set.remove(key);
                if set.is_empty() {
                    self.by_tag.remove(&t);
                }
            }
        }
    }
}

impl Cache {
    /// Drops every entry the signal covers. Idempotent.
    pub async fn apply(&self, signal: &Signal) {
        for key in self.index.take(signal) {
            self.drop_key(&key).await;
        }
        // L2 rows written before a restart are not in the in-memory index.
        if let (Signal::Logout { account }, Some(l2)) = (signal, &self.l2) {
            l2.purge_account(&account.account_id).await;
        }
    }
}
