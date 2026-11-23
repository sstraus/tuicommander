//! Raises gateway invalidation signals from what TUIC already learns: poller
//! updates, repo watcher head changes and account removal or logout
//! (design "Invalidation (R2)").

use std::collections::HashMap;
use std::sync::Arc;

use tokio::sync::broadcast::{Receiver, error::RecvError};
use tuic_gateway::gateway::Gateway;
use tuic_gateway::invalidate::Signal;
use tuic_gateway::model::AccountKey;
use tuic_gateway::ports::Transport;
use tuic_gateway::routes::Tag;

use crate::accounts::ClearedAccount;
use crate::state::{AppEvent, AppState};

/// The repository a local path maps to on a provider account.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct RepoRef {
    pub(crate) account: AccountKey,
    pub(crate) owner: String,
    pub(crate) repo: String,
}

type Resolver = Arc<dyn Fn(&str) -> Option<RepoRef> + Send + Sync>;

/// Turns events into signals. Remembers the PR number of each branch from the
/// last poll, since `HeadChanged` carries only the branch name.
pub(crate) struct SignalAdapter {
    resolve: Resolver,
    branch_prs: HashMap<String, HashMap<String, u64>>,
}

impl SignalAdapter {
    pub(crate) fn new(resolve: Resolver) -> Self {
        Self {
            resolve,
            branch_prs: HashMap::new(),
        }
    }

    fn repo_lists(&self, repo_path: &str) -> Option<Signal> {
        let r = (self.resolve)(repo_path)?;
        let tags = vec![Tag::repo(&r.account.account_id, &r.owner, &r.repo)];
        Some(Signal::RepoChanged {
            account: r.account,
            tags,
        })
    }

    pub(crate) fn on_event(&mut self, event: &AppEvent) -> Option<Signal> {
        match event {
            AppEvent::GitHubPrUpdate {
                repo_path,
                statuses,
            } => {
                let prs = statuses
                    .iter()
                    .filter_map(|s| Some((s.branch.clone(), u64::try_from(s.number).ok()?)))
                    .collect();
                self.branch_prs.insert(repo_path.clone(), prs);
                self.repo_lists(repo_path)
            }
            AppEvent::GitHubIssuesUpdate { repo_path, .. }
            | AppEvent::GitHubIntakeIssuesUpdate { repo_path, .. } => self.repo_lists(repo_path),
            AppEvent::GitHubTransition { transition } => self.repo_lists(transition.repo_path()),
            AppEvent::HeadChanged { repo_path, branch } => {
                let number = *self.branch_prs.get(repo_path)?.get(branch)?;
                let r = (self.resolve)(repo_path)?;
                let tags = vec![Tag::pr(&r.account.account_id, &r.owner, &r.repo, number)];
                Some(Signal::RepoChanged {
                    account: r.account,
                    tags,
                })
            }
            _ => None,
        }
    }
}

/// A logout drops every entry of the account, in L1 and L2.
pub(crate) fn on_account_cleared(cleared: &ClearedAccount) -> Option<Signal> {
    let api_base = cleared.github_api_base.as_deref()?.parse().ok()?;
    Some(Signal::Logout {
        account: AccountKey {
            account_id: cleared.id.clone(),
            api_base,
        },
    })
}

pub(crate) async fn run<T: Transport>(
    gateway: Arc<Gateway<T>>,
    mut adapter: SignalAdapter,
    mut events: Receiver<AppEvent>,
    mut cleared: Receiver<ClearedAccount>,
) {
    loop {
        let signal = tokio::select! {
            event = events.recv() => match event {
                Ok(event) => adapter.on_event(&event),
                // A lagged receiver missed events whose repos are unknown now.
                Err(RecvError::Lagged(_)) => continue,
                Err(RecvError::Closed) => return,
            },
            account = cleared.recv() => match account {
                Ok(account) => on_account_cleared(&account),
                Err(RecvError::Lagged(_)) => continue,
                Err(RecvError::Closed) => return,
            },
        };
        if let Some(signal) = signal {
            gateway.invalidate(&signal).await;
        }
    }
}

/// Maps a repo path to its account and `owner/repo` the way the poller does.
fn resolve_repo(state: &AppState, repo_path: &str) -> Option<RepoRef> {
    use crate::github_account::{
        GitHubAccountRegistry, RepoBindingStore, RepoResolution, resolve_repo_account,
    };
    let default = crate::github::github_com_account(state);
    let resolution = resolve_repo_account(
        std::path::Path::new(repo_path),
        &GitHubAccountRegistry::load(),
        &RepoBindingStore::load(),
        Some(&default),
    );
    let RepoResolution::Bound {
        account,
        owner,
        repo,
    } = resolution
    else {
        return None;
    };
    Some(RepoRef {
        account: AccountKey {
            account_id: account.id,
            api_base: account.host.rest_base().parse().ok()?,
        },
        owner,
        repo,
    })
}

pub(crate) fn spawn(state: Arc<AppState>) {
    let gateway = state.gateway.gateway.clone();
    let events = state.event_bus.subscribe();
    let cleared = crate::accounts::subscribe_cleared();
    let resolver_state = state;
    let resolve: Resolver = Arc::new(move |path| resolve_repo(&resolver_state, path));
    tokio::spawn(run(gateway, SignalAdapter::new(resolve), events, cleared));
}
