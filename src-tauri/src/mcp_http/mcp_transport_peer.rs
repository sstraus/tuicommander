use crate::AppState;
use axum::http::HeaderMap;
use parking_lot::Mutex;
use std::collections::HashMap;
use std::sync::{Arc, LazyLock};
use uuid::Uuid;

use super::mcp_transport::MCP_SESSION_HEADER;
use super::mcp_transport::SSE_INLINE_MESSAGE_MAX_BYTES;
use super::mcp_transport::SSE_POINTER_SUBJECT_MAX_BYTES;
use super::mcp_transport::external_recipient_supports_claude_channel;
use super::mcp_transport::insert_optional_value;
use super::mcp_transport::run_blocking_handler;
use super::mcp_transport_ancillary::journal_hand_off;
use super::mcp_transport_catalogue::AGENT_ACTIONS;
use super::mcp_transport_session_agent::clamp_wait_timeout;
use super::mcp_transport_session_agent::require_action;

/// Validate that a string is a well-formed UUID in canonical 8-4-4-4-12 form.
/// Used to reject non-UUID `tuic_session` values at register time to prevent
/// prompt-injection via preamble string interpolation (SEC-1).
///
/// Length check rejects the `uuid` crate's accepted simple/urn/braced forms —
/// `$TUIC_SESSION` is always written canonical, and narrowing the accepted
/// surface keeps the injection guard tight.
pub(super) fn is_valid_uuid(s: &str) -> bool {
    crate::acp::valid_peer_id(s)
}

/// Current unix time in milliseconds. Centralizes the `SystemTime` boilerplate
/// duplicated across the messaging/spawn paths.
pub(super) fn now_unix_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64
}

/// Peer ownership spans several DashMaps, so registration/reconnect takeover
/// must update them as one critical section rather than racing map-by-map.
pub(super) static PEER_IDENTITY_BIND_LOCK: LazyLock<Mutex<()>> = LazyLock::new(|| Mutex::new(()));

/// A bridge checks liveness every three seconds. A session is an active owner
/// while it has a real SSE subscriber, or while requests arrived recently enough
/// to cover one missed health tick. Mere presence in `mcp_sessions` is not
/// liveness: entries intentionally remain for up to one hour.
pub(super) const MCP_OWNER_ACTIVITY_GRACE: std::time::Duration = std::time::Duration::from_secs(6);

/// Takeover rejection is a *permanent* condition while the incumbent lives, and
/// the loser retries every three seconds forever — one duplicated MCP
/// registration produced 5004 identical WARN lines in a single day, burying
/// every other log. Report the first rejection per claimant pair in full, then
/// one periodic summary carrying the suppressed count.
pub(super) const TAKEOVER_REJECT_SUMMARY_INTERVAL: std::time::Duration =
    std::time::Duration::from_secs(300);

/// Drop claimants idle for this long, so a long-lived app does not accumulate
/// one entry per short-lived MCP session.
pub(super) const TAKEOVER_REJECT_ENTRY_TTL: std::time::Duration =
    std::time::Duration::from_secs(3600);

pub(super) struct TakeoverRejectLog {
    pub(super) suppressed: u64,
    pub(super) last_reported: std::time::Instant,
}

pub(super) static TAKEOVER_REJECT_LOG: LazyLock<
    Mutex<HashMap<(String, String), TakeoverRejectLog>>,
> = LazyLock::new(|| Mutex::new(HashMap::new()));

/// Decide how to report one takeover rejection.
///
/// Returns `Some(suppressed_since_last_report)` when this occurrence must be
/// logged at WARN — `Some(0)` is the first sighting of the pair — and `None`
/// when it is a repeat that the caller should log at debug instead.
pub(super) fn takeover_rejection_report(tuic_session: &str, mcp_sid: &str) -> Option<u64> {
    let now = std::time::Instant::now();
    let mut log = TAKEOVER_REJECT_LOG.lock();

    log.retain(|_, entry| now.duration_since(entry.last_reported) < TAKEOVER_REJECT_ENTRY_TTL);

    match log.get_mut(&(tuic_session.to_string(), mcp_sid.to_string())) {
        None => {
            log.insert(
                (tuic_session.to_string(), mcp_sid.to_string()),
                TakeoverRejectLog {
                    suppressed: 0,
                    last_reported: now,
                },
            );
            Some(0)
        }
        Some(entry) => {
            if now.duration_since(entry.last_reported) >= TAKEOVER_REJECT_SUMMARY_INTERVAL {
                let suppressed = entry.suppressed;
                entry.suppressed = 0;
                entry.last_reported = now;
                Some(suppressed)
            } else {
                entry.suppressed += 1;
                None
            }
        }
    }
}

/// How long a deferred initial prompt may sit undelivered before the parent is
/// told. The prompt is not dropped at this point — it stays queued for the
/// child's next ready window — so this is a "not yet" warning, not a deadline.
pub(super) const INITIAL_PROMPT_DELIVERY_TIMEOUT: std::time::Duration =
    std::time::Duration::from_secs(30);

/// Placeholder stored in `session_parent` when a caller spawns before binding
/// its MCP connection to a TUIC peer identity. The later `register` call swaps
/// this for the stable TUIC session UUID and migrates any early lifecycle mail.
const PENDING_PARENT_PREFIX: &str = "pending-mcp:";

pub(super) fn pending_parent_id(mcp_session_id: &str) -> String {
    format!("{PENDING_PARENT_PREFIX}{mcp_session_id}")
}

/// A placeholder is a routing key, not a session: never publish it as a parent.
pub(crate) fn is_pending_parent(parent: &str) -> bool {
    parent.starts_with(PENDING_PARENT_PREFIX)
}

/// Interval a client should poll a task handle at. Floored well above zero so a
/// stuck orchestrator cannot hot-loop the server.
pub(super) const TASK_POLL_INTERVAL_MS: u64 = 1000;

/// Owner recorded for a caller with no identity of any kind. `agent spawn` is
/// loopback-only and per AGENTS.md the OS user is the auth boundary, so anonymous
/// local callers share one bucket rather than being locked out of their own tasks.
const ANONYMOUS_TASK_OWNER: &str = "loopback";

/// Every identity the caller may legitimately claim, most specific first.
///
/// The first element stamps a new task; the whole set is what an ownership check
/// accepts. Both matter: a caller that spawns before registering is stamped with
/// its pending id, and once it auto-binds its specific identity changes — it must
/// not lose the handle it was already given. `pending_parent_id` is the same alias
/// `link_pending_children_to_parent` reconciles for child sessions.
pub(super) fn caller_task_identities(
    caller_tuic: Option<&str>,
    mcp_session_id: Option<&str>,
) -> Vec<String> {
    let mut identities: Vec<String> = caller_tuic
        .map(str::to_string)
        .into_iter()
        .chain(mcp_session_id.map(pending_parent_id))
        .collect();
    if identities.is_empty() {
        identities.push(ANONYMOUS_TASK_OWNER.to_string());
    }
    identities
}

/// The identity a new task is stamped with.
pub(super) fn task_owner_identity(
    caller_tuic: Option<&str>,
    mcp_session_id: Option<&str>,
) -> String {
    caller_task_identities(caller_tuic, mcp_session_id).swap_remove(0)
}

pub(super) fn link_pending_children_to_parent(
    state: &AppState,
    mcp_session_id: &str,
    parent_tuic_session: &str,
) -> usize {
    let pending_parent = pending_parent_id(mcp_session_id);
    let children: Vec<String> = state
        .session_maps
        .session_parent
        .iter()
        .filter(|entry| entry.value() == &pending_parent)
        .map(|entry| entry.key().clone())
        .collect();
    for child in &children {
        state
            .session_maps
            .session_parent
            .insert(child.clone(), parent_tuic_session.to_string());
    }
    if let Some((_, messages)) = state.agent_inbox.remove(&pending_parent) {
        for message in messages {
            let message_id = message.id.clone();
            let message_timestamp = state.push_agent_inbox(parent_tuic_session, message);
            crate::pty::route_registered_orchestrator_mail(
                state,
                parent_tuic_session,
                &message_id,
                message_timestamp,
            );
        }
    }
    if let Some((_, missed)) = state.agent_inbox_evictions.remove(&pending_parent) {
        *state
            .agent_inbox_evictions
            .entry(parent_tuic_session.to_string())
            .or_default() += missed;
    }

    children.len()
}

/// HTTP header the bridge asserts to declare its TUIC peer identity. A PTY
/// agent inherits it from its tab; ACP-hosted ego receives a host-issued UUID.
pub(super) const TUIC_SESSION_HEADER: &str = "x-tuic-session";
/// Pid of the bridge or CLI process that sent the request, logged on initialize.
pub(super) const CLIENT_PID_HEADER: &str = "x-tuic-client-pid";

/// The pid a client reported, or `""`. Digits only: the value reaches the log
/// verbatim, so anything else is dropped rather than written.
pub(super) fn client_pid_header(headers: &HeaderMap) -> &str {
    headers
        .get(CLIENT_PID_HEADER)
        .and_then(|v| v.to_str().ok())
        .filter(|v| !v.is_empty() && v.len() <= 10 && v.bytes().all(|b| b.is_ascii_digit()))
        .unwrap_or("")
}

/// Bind an MCP session to a TUIC peer identity: upsert `peer_agents`
/// and the `mcp_to_session` / `session_to_mcp` reverse indices. Callers hold
/// `PEER_IDENTITY_BIND_LOCK` after applying the shared live-owner policy below.
fn bind_peer_identity_locked(
    state: &AppState,
    mcp_sid: &str,
    tuic_session: &str,
    name: String,
    project: Option<String>,
    registered_at: u64,
) {
    let prior_mcp = state
        .peer_agents
        .get(tuic_session)
        .map(|peer| peer.mcp_session_id.clone())
        .filter(|prior| !prior.is_empty() && prior != mcp_sid);

    // A reconnect changes the protocol-session id. Retire the old routing
    // entries before publishing the replacement so inbox ownership cannot be
    // split between the stale and current bridge.
    if let Some(prior_mcp) = prior_mcp {
        state
            .mcp
            .to_session
            .remove_if(&prior_mcp, |_, mapped| mapped == tuic_session);
        let remove_reverse =
            if let Some(mut reverse) = state.mcp.session_to_mcp.get_mut(tuic_session) {
                reverse.retain(|sid| sid != &prior_mcp);
                reverse.is_empty()
            } else {
                false
            };
        if remove_reverse {
            state.mcp.session_to_mcp.remove(tuic_session);
        }
        // The retired bridge may still hold `agent wait` leases. They outlive its
        // routing entries and keep beating terminal delivery, so the reconnected
        // peer would go unwoken until they time out.
        state.revoke_waiters_for_reconnect(tuic_session);
    }

    state.peer_agents.insert(
        tuic_session.to_string(),
        crate::state::PeerAgent {
            tuic_session: tuic_session.to_string(),
            mcp_session_id: mcp_sid.to_string(),
            name,
            project,
            registered_at,
        },
    );
    state
        .mcp
        .to_session
        .insert(mcp_sid.to_string(), tuic_session.to_string());
    let mut reverse = state
        .mcp
        .session_to_mcp
        .entry(tuic_session.to_string())
        .or_default();
    if !reverse.iter().any(|s| s == mcp_sid) {
        reverse.push(mcp_sid.to_string());
    }
}

/// Who holds an identity when another protocol session asks for it.
enum PeerIdentityOwnership {
    /// Nothing live stands in the way. The payload is the stale prior owner,
    /// whose routing entries and wait leases must be retired on the way in.
    Vacant(Option<String>),
    /// Another protocol session owns it and is still live. Whether that means
    /// "share" or "refuse" is the caller's call, not this predicate's.
    LiveOwner,
}

fn mcp_session_has_live_owner(state: &AppState, mcp_sid: &str) -> bool {
    let has_sse_subscriber = state
        .session_maps
        .messaging_channels
        .get(mcp_sid)
        .is_some_and(|sender| sender.receiver_count() > 0);
    if has_sse_subscriber {
        return true;
    }
    state
        .mcp
        .sessions
        .get(mcp_sid)
        .is_some_and(|meta| meta.last_activity.elapsed() <= MCP_OWNER_ACTIVITY_GRACE)
}

/// Report who holds the identity. The caller must hold `PEER_IDENTITY_BIND_LOCK`
/// so the liveness answer and the routing-map change it drives form one critical
/// section.
fn peer_identity_ownership_locked(
    state: &AppState,
    mcp_sid: &str,
    tuic_session: &str,
) -> PeerIdentityOwnership {
    let prior_mcp = state
        .peer_agents
        .get(tuic_session)
        .map(|peer| peer.mcp_session_id.clone())
        .filter(|prior| !prior.is_empty() && prior != mcp_sid);

    if prior_mcp
        .as_deref()
        .is_some_and(|prior| mcp_session_has_live_owner(state, prior))
    {
        return PeerIdentityOwnership::LiveOwner;
    }

    PeerIdentityOwnership::Vacant(prior_mcp)
}

/// True when this protocol session already routes to the identity, which only
/// happens after an earlier header assertion or registration bound the two.
fn mcp_session_routes_to(state: &AppState, mcp_sid: &str, tuic_session: &str) -> bool {
    state
        .mcp
        .to_session
        .get(mcp_sid)
        .is_some_and(|bound| bound.value() == tuic_session)
}

/// A short-lived bridge can exit without DELETE /mcp. Retire its protocol
/// metadata and routes when another bridge for the same identity arrives,
/// while preserving subscribed and recently active sibling bridges.
/// Callers hold PEER_IDENTITY_BIND_LOCK.
fn retire_stale_identity_sessions_locked(state: &AppState, tuic_session: &str, incoming: &str) {
    let stale: std::collections::HashSet<String> = state
        .mcp
        .session_to_mcp
        .get(tuic_session)
        .map(|reverse| {
            reverse
                .iter()
                .filter(|sid| sid.as_str() != incoming && !mcp_session_has_live_owner(state, sid))
                .cloned()
                .collect()
        })
        .unwrap_or_default();
    if stale.is_empty() {
        return;
    }
    for sid in &stale {
        state.mcp.sessions.remove(sid);
        state.mcp.to_session.remove(sid);
        state.session_maps.messaging_channels.remove(sid);
    }
    if let Some(mut reverse) = state.mcp.session_to_mcp.get_mut(tuic_session) {
        reverse.retain(|sid| !stale.contains(sid));
    }
}

/// Add a co-owner to an identity a live sibling already owns: routing entries
/// only, so the sibling keeps delivery ownership. Two live bridges that traded
/// ownership on every request would flip the delivery channel back and forth;
/// the inbox is keyed by the PTY identity, so sharing the routes is enough for
/// both to read the same mail.
fn join_peer_identity_locked(state: &AppState, mcp_sid: &str, tuic_session: &str) {
    state
        .mcp
        .to_session
        .insert(mcp_sid.to_string(), tuic_session.to_string());
    let mut reverse = state
        .mcp
        .session_to_mcp
        .entry(tuic_session.to_string())
        .or_default();
    if !reverse.iter().any(|s| s == mcp_sid) {
        reverse.push(mcp_sid.to_string());
    }
}

/// Retire the identity a caller just abandoned by registering a different one.
///
/// Only a terminal-less identity is retired: it is a mailbox nobody can reach any
/// more once its single owner has moved on, and leaving it behind is how
/// `list_peers` accumulates addresses that silently swallow every message sent to
/// them. Mail already buffered under it is carried over rather than dropped —
/// those are the replies the caller went looking for in the first place. An
/// abandoned identity that still owns a PTY is left alone; its terminal, not this
/// registration, decides its lifetime.
/// What a retire attempt actually did. The skip case used to be a silent early
/// return, which is how mail addressed to a superseded identity sat unread with
/// nobody told: `register` now reports it back to the caller.
#[derive(Debug, PartialEq, Eq)]
pub(crate) enum IdentityHandoff {
    /// The prior identity was abandoned; its mail moved and was re-routed.
    Migrated { messages: usize },
    /// The prior identity still owns a live PTY, so it is a reachable peer rather
    /// than an abandoned one. Migrating would take mail from a working agent.
    SkippedLivePty { pending: usize },
}

pub(super) fn retire_repaired_phantom_identity(
    state: &AppState,
    phantom: &str,
    repaired: &str,
) -> IdentityHandoff {
    // The retire spans several maps, and `send` reads one of them (`peer_agents`)
    // to decide whether a recipient exists before buffering. Both halves take
    // `PEER_IDENTITY_BIND_LOCK` so a send can only land entirely before the
    // retire — and be carried over with the rest of the inbox — or entirely
    // after it, where the missing peer makes it an explicit refusal. Routing the
    // carried mail happens after the guard drops: it wakes PTYs and must not
    // hold an identity lock across that I/O.
    let carried: Vec<(String, u64)> = {
        let _bind_guard = PEER_IDENTITY_BIND_LOCK.lock();
        if state.live_pty_for_peer(phantom).is_some() {
            let pending = state
                .agent_inbox
                .get(phantom)
                .map(|inbox| inbox.len())
                .unwrap_or(0);
            return IdentityHandoff::SkippedLivePty { pending };
        }
        let was_orchestrator = state.orchestrator_peers.remove(phantom).is_some();
        if was_orchestrator {
            let children: Vec<String> = state
                .session_maps
                .session_parent
                .iter()
                .filter(|entry| entry.value() == phantom)
                .map(|entry| entry.key().clone())
                .collect();
            for child in children {
                state
                    .session_maps
                    .session_parent
                    .insert(child, repaired.to_string());
            }
            state.orchestrator_peers.insert(repaired.to_string());
        }
        // Drop the addressable identity before draining: a send blocked on the
        // guard then finds no recipient instead of refilling the inbox we just
        // emptied.
        crate::mcp_http::remote_peer::unregister_peer(state, phantom);
        let carried = match state.agent_inbox.remove(phantom) {
            Some((_, pending)) => pending
                .into_iter()
                .map(|message| {
                    let message_id = message.id.clone();
                    let message_timestamp = state.store_agent_inbox(repaired, message);
                    (message_id, message_timestamp)
                })
                .collect(),
            None => Vec::new(),
        };
        // The cursor indexes the mail we just moved, so it moves with it. `max`
        // because the replacing identity may have read mail of its own already;
        // a migrated message whose logical timestamp had to be bumped past the
        // cursor is re-delivered once, which is the harmless direction to err in.
        let phantom_cursor = state
            .agent_read_cursor
            .get(phantom)
            .map(|cursor| *cursor.value())
            .unwrap_or(0);
        advance_agent_cursor(state, repaired, phantom_cursor);
        drop_identity_buffers(state, phantom);
        carried
    };
    let migrated = carried.len();
    for (message_id, message_timestamp) in carried {
        crate::pty::route_registered_orchestrator_mail(
            state,
            repaired,
            &message_id,
            message_timestamp,
        );
    }
    tracing::info!(
        source = "agent_msg",
        event = "phantom_identity_retired",
        phantom = %phantom,
        repaired = %repaired,
        "Retired a terminal-less identity its owner abandoned"
    );
    let _ = state
        .event_bus
        .send(crate::state::AppEvent::PeerUnregistered {
            tuic_session: phantom.to_string(),
        });
    IdentityHandoff::Migrated { messages: migrated }
}

fn register_peer_identity(
    state: &AppState,
    mcp_sid: &str,
    tuic_session: &str,
    name: String,
    project: Option<String>,
    registered_at: u64,
) -> Result<Option<String>, String> {
    let _bind_guard = PEER_IDENTITY_BIND_LOCK.lock();
    match peer_identity_ownership_locked(state, mcp_sid, tuic_session) {
        PeerIdentityOwnership::Vacant(prior_mcp) => {
            bind_peer_identity_locked(state, mcp_sid, tuic_session, name, project, registered_at);
            Ok(prior_mcp)
        }
        // A session that already routes to the identity was bound to it by an
        // earlier header assertion, so it is a sibling bridge inside the same
        // PTY rather than a claimant — register is its rename, not a takeover.
        PeerIdentityOwnership::LiveOwner if mcp_session_routes_to(state, mcp_sid, tuic_session) => {
            join_peer_identity_locked(state, mcp_sid, tuic_session);
            if let Some(mut peer) = state.peer_agents.get_mut(tuic_session) {
                peer.name = name;
                if project.is_some() {
                    peer.project = project;
                }
            }
            Ok(None)
        }
        PeerIdentityOwnership::LiveOwner => {
            Err("tuic_session is already registered to another active MCP session".to_string())
        }
    }
}

/// Auto-bind an MCP session to the `x-tuic-session` identity asserted by the
/// bridge. PTY agents inherit it from the terminal; ACP ego receives it from
/// the host's durable conversation binding.
/// Makes managed-peer identity automatic — no explicit `agent register` needed, which
/// matters for clients that never surface initialize `instructions` (e.g. Codex).
/// Ignored unless the header is a well-formed UUID. Preserves an existing peer's
/// display name/project across a bridge reconnect (only `register` renames).
/// A fresh MCP session may reclaim a stale owner, but cannot replace another
/// subscribed or recently active bridge. Returns whether a bind happened.
pub(super) fn apply_initialize_identity(
    state: &AppState,
    mcp_sid: &str,
    header: Option<&str>,
) -> bool {
    let Some(asserted) = header.filter(|s| !s.is_empty()) else {
        return false;
    };
    if !is_valid_uuid(asserted) {
        return false;
    }
    // Steady state for a connected bridge: it already routes to the identity and
    // already owns it, so both branches below would rewrite the values that are
    // there. Every bridge asserts this header on a `ping` every 3s, so without
    // this the liveness poll serialises N terminals on a process-global mutex to
    // do nothing. Any disagreement between the maps still takes the lock.
    if mcp_session_routes_to(state, mcp_sid, asserted)
        && state
            .peer_agents
            .get(asserted)
            .is_some_and(|peer| peer.mcp_session_id == mcp_sid)
    {
        return true;
    }
    // An aliased sibling pings with its asserted UUID every three seconds.
    // Keep the steady state lock-free after its first canonical bind.
    if let Some(bound) = state.mcp.to_session.get(mcp_sid)
        && state.peer_agents.contains_key(bound.value())
        && state.live_pty_for_peer(bound.value()).is_some()
        && state.live_pty_for_peer(bound.value()) == state.live_pty_for_peer(asserted)
    {
        return true;
    }
    let _bind_guard = PEER_IDENTITY_BIND_LOCK.lock();
    let canonical = state.peer_identity_for_live_pty(asserted);
    let tuic = canonical.as_deref().unwrap_or(asserted);
    retire_stale_identity_sessions_locked(state, tuic, mcp_sid);
    // Only a process that inherited this PTY's `$TUIC_SESSION` can assert the
    // header, so a second asserting bridge is a sibling inside that PTY, not a
    // claimant from outside it — Codex opens two. It joins the identity's routing
    // instead of taking it over; ownership stays with the bridge that has it, so
    // two live siblings cannot trade the delivery channel on every request.
    let prior_mcp = match peer_identity_ownership_locked(state, mcp_sid, tuic) {
        PeerIdentityOwnership::Vacant(prior_mcp) => prior_mcp,
        PeerIdentityOwnership::LiveOwner => {
            join_peer_identity_locked(state, mcp_sid, tuic);
            return true;
        }
    };
    let (name, project, registered_at) = match state.peer_agents.get(tuic) {
        Some(existing) => (
            existing.name.clone(),
            existing.project.clone(),
            existing.registered_at,
        ),
        // PTY peers take a terminal alias; the ACP peer is named ego and has
        // a repository root but no terminal. Other external peers stay agent.
        None => (
            state
                .acp
                .peer_root(tuic)
                .map(|_| "ego".to_string())
                .or_else(|| {
                    state.live_pty_for_peer(tuic).and_then(|session_id| {
                        state
                            .session_maps
                            .term_aliases
                            .get(&session_id)
                            .map(|alias| alias.value().clone())
                    })
                })
                .unwrap_or_else(|| "agent".to_string()),
            state
                .acp
                .peer_root(tuic)
                .map(|root| root.to_string_lossy().into_owned()),
            now_unix_ms(),
        ),
    };
    bind_peer_identity_locked(state, mcp_sid, tuic, name, project, registered_at);
    if let Some(prior_mcp) = prior_mcp {
        tracing::warn!(
            source = "mcp_initialize",
            event = "stale_binding_takeover",
            tuic_session = %tuic,
            prior_mcp_session = %prior_mcp,
            mcp_session = %mcp_sid,
            "Reclaimed stale MCP peer binding during initialize"
        );
    }
    true
}

/// Resolve the cwd of the managed PTY asserted by the bridge header without
/// changing peer ownership or routing. Spawn uses this only as a cwd hint when
/// identity binding is legitimately unavailable (for example, a live-owner
/// conflict); messaging continues to rely exclusively on the binding maps.
pub(super) fn managed_parent_cwd_from_header(
    state: &AppState,
    mcp_session_id: Option<&str>,
    header: Option<&str>,
) -> Option<String> {
    let tuic_session = header.filter(|value| is_valid_uuid(value))?;
    if let Some(bound_tuic) = mcp_session_id.and_then(|sid| state.mcp.to_session.get(sid))
        && bound_tuic.value() != tuic_session
    {
        return None;
    }
    state
        .session_maps
        .sessions
        .get(tuic_session)
        .and_then(|session| session.lock().cwd.clone())
}

/// Refresh a protocol session and re-assert its PTY identity on every request.
/// Both maps are in-memory and disappear on a TUIC restart; a long-lived bridge
/// may keep its old MCP session id, so merely recreating `mcp_sessions` is not
/// enough to keep `agent send` registered.
pub(super) fn refresh_mcp_session(
    state: &AppState,
    mcp_sid: &str,
    is_claude_code: bool,
    tuic_session_header: Option<&str>,
) {
    if let Some(mut meta) = state.mcp.sessions.get_mut(mcp_sid) {
        meta.last_activity = std::time::Instant::now();
    } else {
        // A reaper that removed metadata still owns the identity lock until
        // it has retired routes and channels. Wait for that cleanup before
        // recreating the protocol session, then re-assert identity below.
        // Existing sessions take the lock-free path above on every 3s ping.
        let _bind_guard = PEER_IDENTITY_BIND_LOCK.lock();
        if let Some(mut meta) = state.mcp.sessions.get_mut(mcp_sid) {
            meta.last_activity = std::time::Instant::now();
        } else {
            state.mcp.sessions.insert(
                mcp_sid.to_string(),
                crate::state::McpSessionMeta {
                    prompt_instructions: None,
                    last_activity: std::time::Instant::now(),
                    is_claude_code,
                    requires_meta_tools: false,
                    has_sse_stream: false,
                    sse_generation: 0,
                    repo_path: None,
                },
            );
        }
    }
    apply_initialize_identity(state, mcp_sid, tuic_session_header);
}

/// Reuse the bridge's live protocol session when it proxies the downstream
/// client's initialize request. The bridge eagerly initializes once so it can
/// expose tools while the client is starting, then forwards the client's own
/// initialize with that session ID. Minting a second ID here would make the
/// live-owner guard correctly reject the same bridge as an identity takeover.
pub(super) fn initialize_session_id(
    state: &AppState,
    headers: &HeaderMap,
) -> (String, InitializeKind) {
    let presented = headers
        .get(MCP_SESSION_HEADER)
        .and_then(|value| value.to_str().ok())
        .filter(|session_id| is_valid_uuid(session_id));
    match presented {
        Some(sid) if state.mcp.sessions.contains_key(sid) => {
            (sid.to_string(), InitializeKind::Resumed)
        }
        // The client came back holding a session id we no longer have — reaped by
        // the idle sweep, or lost with a restart. It gets a new one and never
        // learns why. This is the moment an agent reports "TUICommander is back",
        // so it is the one case that must not be silent.
        Some(sid) => (
            Uuid::new_v4().to_string(),
            InitializeKind::Reconnected {
                presented: sid.to_string(),
            },
        ),
        None => (Uuid::new_v4().to_string(), InitializeKind::Fresh),
    }
}

/// How a client arrived at `initialize`. Logged so a claim about the MCP
/// connection dropping is checkable against the record instead of taken on trust:
/// previously only the peer-binding takeover was logged, and only when a prior
/// binding happened to exist, so an ordinary reconnect left no trace at all.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) enum InitializeKind {
    /// First contact: no session id presented.
    Fresh,
    /// Presented a session id we still hold — the same connection continuing.
    Resumed,
    /// Presented a session id we no longer hold. A new one was minted.
    Reconnected { presented: String },
}

impl InitializeKind {
    pub(super) fn as_str(&self) -> &'static str {
        match self {
            InitializeKind::Fresh => "fresh",
            InitializeKind::Resumed => "resumed",
            InitializeKind::Reconnected { .. } => "reconnected",
        }
    }
}

/// `agent action=wait` — block until the caller's inbox has a message newer than
/// `since` (unix ms), or the timeout elapses. The caller must be registered
/// (identity auto-binds at initialize, so this is normally already true).
struct ActiveAgentWaitGuard {
    pub(super) state: Arc<AppState>,
    pub(super) tuic_session: String,
    pub(super) lease: u64,
    pub(super) since: u64,
    pub(super) finished: bool,
}

impl ActiveAgentWaitGuard {
    fn new(
        state: &Arc<AppState>,
        tuic_session: &str,
        since: u64,
    ) -> (Self, tokio::sync::watch::Receiver<u64>) {
        let (lease, events) = state.begin_agent_wait_with_events(tuic_session);
        (
            Self {
                state: Arc::clone(state),
                tuic_session: tuic_session.to_string(),
                lease,
                since,
                finished: false,
            },
            events,
        )
    }

    fn finish(&mut self, observe_fresh: bool) -> crate::state::AgentWaitFinish {
        if self.finished {
            return crate::state::AgentWaitFinish::default();
        }
        let finish =
            self.state
                .finish_agent_wait(&self.tuic_session, self.lease, self.since, observe_fresh);
        dispatch_waiter_handoff(&self.state, &self.tuic_session, &finish.terminal_handoff);
        self.finished = true;
        finish
    }
}

impl Drop for ActiveAgentWaitGuard {
    fn drop(&mut self) {
        self.finish(false);
    }
}

/// Hand messages a finishing `agent wait` did not carry back to the recipient's
/// terminal.
///
/// Deferred to the injection worker: `finish` is reached from
/// `ActiveAgentWaitGuard::drop`, so the caller is whatever tokio worker is
/// dropping the wait future, and each message here sleeps `INJECT_ENTER_GAP`
/// under the recipient's writer mutex.
fn dispatch_waiter_handoff(state: &Arc<AppState>, recipient: &str, message_ids: &[String]) {
    if message_ids.is_empty() {
        return;
    }
    let state = Arc::clone(state);
    let recipient = recipient.to_string();
    let message_ids = message_ids.to_vec();
    crate::pty::spawn_injection_job(move || {
        dispatch_waiter_handoff_blocking(&state, &recipient, &message_ids)
    });
}

fn dispatch_waiter_handoff_blocking(state: &AppState, recipient: &str, message_ids: &[String]) {
    let wanted: std::collections::HashSet<&str> = message_ids.iter().map(String::as_str).collect();
    let messages: Vec<crate::state::AgentMessage> = state
        .agent_inbox
        .get(recipient)
        .map(|inbox| {
            inbox
                .iter()
                .filter(|message| wanted.contains(message.id.as_str()))
                .cloned()
                .collect()
        })
        .unwrap_or_default();
    for message in messages {
        if crate::pty::route_registered_orchestrator_mail(
            state,
            recipient,
            &message.id,
            message.timestamp,
        )
        .is_some()
        {
            continue;
        }
        // The payload stays in the inbox; the terminal only gets the pointer.
        let outcome =
            crate::pty::deliver_notice_to_managed_pty(state, recipient, crate::pty::PEER_MAIL_WAKE);
        crate::pty::settle_terminal_delivery(state, recipient, &message.id, outcome);
    }
}

const AGENT_WAIT_INLINE_LIMIT: usize = crate::state::AGENT_INBOX_CAPACITY;

fn bounded_agent_messages<'a>(
    messages: impl DoubleEndedIterator<Item = &'a crate::state::AgentMessage>,
    limit: usize,
) -> Vec<crate::state::AgentMessage> {
    let mut page: Vec<_> = messages.rev().take(limit).cloned().collect();
    page.reverse();
    page
}

/// Resolve the read position for a wait/inbox call.
///
/// An explicit `since` always wins — `since=0` is the deliberate "replay
/// everything" escape hatch and must stay honoured. Omitting it resumes from the
/// server-side cursor, so a caller that cannot thread the value (or that lost it
/// to a timeout) no longer falls back to replaying the whole inbox.
fn resolve_agent_since(state: &AppState, tuic_session: &str, args: &serde_json::Value) -> u64 {
    match args["since"].as_u64() {
        Some(explicit) => explicit,
        None => state
            .agent_read_cursor
            .get(tuic_session)
            .map(|entry| *entry.value())
            .unwrap_or(0),
    }
}

/// Drop every per-identity buffer that must not outlive a retired or torn-down
/// peer. `agent_inbox` stays out on purpose: retire drains it into the replacing
/// identity while teardown deletes it, so each caller owns that decision.
///
/// One function so the two teardown paths cannot disagree about the set — the
/// read cursor was added to `AppState` and wired into neither, which left it
/// growing without bound and let a replacing identity resume from zero.
pub(super) fn drop_identity_buffers(state: &AppState, tuic_session: &str) {
    state.agent_inbox_evictions.remove(tuic_session);
    state.active_agent_waiters.remove(tuic_session);
    state.pending_injections.remove(tuic_session);
    state.mcp.session_to_mcp.remove(tuic_session);
    state.agent_read_cursor.remove(tuic_session);
}

/// Advance the stored read position. Never moves backwards: a deliberate replay
/// (`since=0`) must not rewind the cursor for the next omitted-`since` call.
fn advance_agent_cursor(state: &AppState, tuic_session: &str, cursor: u64) {
    let mut entry = state
        .agent_read_cursor
        .entry(tuic_session.to_string())
        .or_insert(0);
    if cursor > *entry {
        *entry = cursor;
    }
}

fn agent_wait_success_response(
    state: &AppState,
    tuic_session: &str,
    since: u64,
    finish: crate::state::AgentWaitFinish,
) -> serde_json::Value {
    let messages = bounded_agent_messages(finish.messages.iter(), AGENT_WAIT_INLINE_LIMIT);
    // Fall back to the position we read from, so `next_since` is present even when
    // the batch is empty — losing the cursor is what drove callers back to since=0.
    let next_since = messages
        .iter()
        .map(|message| message.timestamp)
        .max()
        .unwrap_or(since);
    advance_agent_cursor(state, tuic_session, next_since);
    serde_json::json!({
        "met": true,
        "timed_out": false,
        "new_messages": finish.fresh_count,
        "messages": messages,
        "next_since": next_since,
    })
}

pub(super) async fn handle_agent_wait(
    state: &Arc<AppState>,
    args: &serde_json::Value,
    mcp_session_id: Option<&str>,
) -> serde_json::Value {
    let caller_tuic = match mcp_session_id
        .and_then(|sid| state.mcp.to_session.get(sid).map(|e| e.value().clone()))
    {
        Some(t) => t,
        None => {
            return serde_json::json!({"error": "You are not registered. Identity normally auto-binds at initialize; ensure $TUIC_SESSION is set or call agent action=register."});
        }
    };
    let since = resolve_agent_since(state, &caller_tuic, args);
    let (mut active_wait, mut inbox_events) = ActiveAgentWaitGuard::new(state, &caller_tuic, since);
    let timeout_ms = clamp_wait_timeout(args["timeout_ms"].as_u64());
    if state.waiter_fresh_message_count(&caller_tuic, since) > 0 {
        return agent_wait_success_response(state, &caller_tuic, since, active_wait.finish(true));
    }
    let woke = tokio::time::timeout(std::time::Duration::from_millis(timeout_ms), async {
        loop {
            if inbox_events.changed().await.is_err() {
                return false;
            }
            if state.waiter_fresh_message_count(&caller_tuic, since) > 0 {
                return true;
            }
        }
    })
    .await
    .unwrap_or(false);
    let finish = active_wait.finish(true);
    if woke || finish.fresh_count > 0 {
        return agent_wait_success_response(state, &caller_tuic, since, finish);
    }
    // A timed-out wait must still hand back a usable cursor, or the caller has
    // nothing to pass but `since=0`. Prefer the stored position over the requested
    // one: a deliberate `since=0` replay that finds nothing new should resume from
    // where reading actually got to, not send the caller back to the start.
    let resume = state
        .agent_read_cursor
        .get(&caller_tuic)
        .map(|entry| *entry.value())
        .unwrap_or(0)
        .max(since);
    serde_json::json!({"met": false, "timed_out": true, "new_messages": 0, "next_since": resume})
}

fn resolve_registration_identity(
    state: &AppState,
    args: &serde_json::Value,
    mcp_sid: &str,
) -> Result<(String, bool), serde_json::Value> {
    let current = state
        .mcp
        .to_session
        .get(mcp_sid)
        .map(|entry| entry.value().clone());
    if let Some(explicit) = args["tuic_session"]
        .as_str()
        .filter(|value| !value.is_empty())
    {
        if !is_valid_uuid(explicit) {
            return Err(serde_json::json!({
                "error": "tuic_session must be a UUID (xxxxxxxx-xxxx-xxxx-xxxx-xxxxxxxxxxxx)"
            }));
        }
        // Authority, not mere difference. An identity that resolves to a live PTY
        // outranks one that does not: the caller announcing its real `$TUIC_SESSION`
        // after having registered an invented UUID is repairing itself, and refusing
        // that leaves an orchestrator permanently unreachable through its terminal.
        if let Some(bound) = current.as_deref().filter(|bound| *bound != explicit) {
            if let Some(bound_pty) = state.live_pty_for_peer(bound) {
                if state.live_pty_for_peer(explicit).as_deref() == Some(bound_pty.as_str()) {
                    return Ok((bound.to_string(), false));
                }
                return Err(serde_json::json!({
                    "error": format!(
                        "This MCP session is bound to '{bound}', which owns a live terminal. \
                         That is your $TUIC_SESSION — register it instead of '{explicit}', \
                         or omit tuic_session entirely. An identity with no PTY behind it \
                         can never be typed into or woken."
                    )
                }));
            }
            if state.live_pty_for_peer(explicit).is_none() {
                return Err(serde_json::json!({
                    "error": "This MCP session is already bound to a different peer identity"
                }));
            }
        }
        return Ok((
            state
                .peer_identity_for_live_pty(explicit)
                .unwrap_or_else(|| explicit.to_string()),
            false,
        ));
    }
    if let Some(current) = current {
        return Ok((current, false));
    }
    Ok((Uuid::new_v4().to_string(), true))
}

fn managed_recipient_state(state: &AppState, tuic_session: &str) -> Option<serde_json::Value> {
    let _session = state.session_maps.sessions.get(tuic_session)?;
    let snapshot = state.session_state_with_shell(tuic_session);
    let mut summary = serde_json::Map::new();
    if let Some(snapshot) = snapshot {
        insert_optional_value(
            &mut summary,
            "shell_state",
            snapshot.shell_state.map(serde_json::Value::String),
        );
        insert_optional_value(
            &mut summary,
            "agent_state",
            snapshot.agent_state.map(serde_json::Value::String),
        );
    }
    Some(serde_json::Value::Object(summary))
}

/// Preserve a higher-priority delivery route; otherwise report the subscribed
/// ACP inbox notification in the same way for urgent, orchestrator, and normal mail.
fn apply_acp_inbox_receipt(response: &mut serde_json::Value, subscribed: bool) {
    if !subscribed || response["delivery_path"] != "inbox_only" {
        return;
    }
    let object = response
        .as_object_mut()
        .expect("send response is an object");
    object.insert("delivered".to_owned(), serde_json::json!(true));
    object.insert(
        "delivery_path".to_owned(),
        serde_json::json!("acp_inbox_resource"),
    );
    if object.contains_key("urgent_delivered") {
        object.insert("urgent_delivered".to_owned(), serde_json::json!(true));
        object.remove("urgent_fallback_reason");
    }
    object.remove("warning");
}

/// Native mail-only calls shared with the authenticated federation endpoint.
pub(super) async fn local_peer_call(
    state: &Arc<AppState>,
    args: &serde_json::Value,
    sid: Option<&str>,
) -> serde_json::Value {
    local_peer_call_with_message_id(state, args, sid, None).await
}

pub(crate) async fn local_peer_call_with_message_id(
    state: &Arc<AppState>,
    args: &serde_json::Value,
    sid: Option<&str>,
    message_id: Option<String>,
) -> serde_json::Value {
    if state
        .config
        .read()
        .disabled_native_tools
        .iter()
        .any(|name| name == "agent")
    {
        return serde_json::json!({"error":"Tool 'agent' is disabled by configuration"});
    }
    match args["action"].as_str() {
        Some("wait") => handle_agent_wait(state, args, sid).await,
        Some("send") => {
            let state = state.clone();
            let args = args.clone();
            let sid = sid.map(str::to_owned);
            run_blocking_handler(move || {
                handle_messaging_with_message_id(
                    &state,
                    &args,
                    sid.as_deref(),
                    message_id.as_deref(),
                )
            })
            .await
        }
        Some("register" | "list_peers" | "inbox") => handle_messaging(state, args, sid),
        _ => {
            serde_json::json!({"error":"Peer mail permits register/list_peers/send/inbox/wait only"})
        }
    }
}

pub(super) fn handle_messaging(
    state: &Arc<AppState>,
    args: &serde_json::Value,
    mcp_session_id: Option<&str>,
) -> serde_json::Value {
    handle_messaging_with_message_id(state, args, mcp_session_id, None)
}

fn handle_messaging_with_message_id(
    state: &Arc<AppState>,
    args: &serde_json::Value,
    mcp_session_id: Option<&str>,
    forwarded_message_id: Option<&str>,
) -> serde_json::Value {
    let action = match require_action(args, "agent", AGENT_ACTIONS) {
        Ok(a) => a,
        Err(e) => return e,
    };
    if matches!(action, "register" | "list_peers") && args.get("project").is_some() {
        return serde_json::json!({"error": "agent parameter project was renamed to path"});
    }
    match action {
        "register" => {
            let mcp_sid = match mcp_session_id {
                Some(sid) => sid.to_string(),
                None => {
                    // Reached by a stateless caller now that tools/call no longer
                    // gates on the header (#0f44). Registration keys the identity on
                    // the protocol session, so it cannot mint one from nothing —
                    // say what to do instead of just stating the problem. Making
                    // identity itself stateless is Phase E, not this step.
                    return serde_json::json!({"error": "Registration needs an MCP protocol session, and this request carried no `mcp-session-id`. Run `initialize` first — managed PTYs auto-bind from $TUIC_SESSION. Tools that need no caller identity work without it."});
                }
            };
            let previously_bound = state
                .mcp
                .to_session
                .get(&mcp_sid)
                .map(|entry| entry.value().clone());
            let (tuic_session, generated_identity) =
                match resolve_registration_identity(state, args, &mcp_sid) {
                    Ok(identity) => identity,
                    Err(error) => return error,
                };
            let existing = state
                .peer_agents
                .get(&tuic_session)
                .map(|peer| (peer.name.clone(), peer.project.clone()));
            let orchestrator = args["orchestrator"].as_bool().unwrap_or_else(|| {
                state.orchestrator_peers.contains(&tuic_session)
                    || previously_bound
                        .as_ref()
                        .is_some_and(|prior| state.orchestrator_peers.contains(prior))
            });
            // An empty name is unset: it would otherwise be an address nothing can use.
            let name = args["name"]
                .as_str()
                .filter(|name| !name.is_empty())
                .map(str::to_string)
                .or_else(|| existing.as_ref().map(|(name, _)| name.clone()))
                .unwrap_or_else(|| "agent".to_string());
            let project = args["path"]
                .as_str()
                .map(str::to_string)
                .or_else(|| existing.and_then(|(_, project)| project));
            let now_ms = now_unix_ms();

            // Presence in mcp_sessions is not proof of liveness: protocol sessions
            // have a one-hour TTL. Reconnect may reclaim a stale owner, but not an
            // actually subscribed/recently active one. The check and routing-map
            // replacement share one lock to prevent concurrent takeovers.
            let prior_mcp = match register_peer_identity(
                state,
                &mcp_sid,
                &tuic_session,
                name.clone(),
                project,
                now_ms,
            ) {
                Ok(prior) => prior,
                Err(error) => {
                    // A refused claimant retries; report the first sighting in
                    // full and keep the repeats out of the log.
                    match takeover_rejection_report(&tuic_session, &mcp_sid) {
                        Some(suppressed) => tracing::warn!(
                            source = "agent_msg",
                            event = "live_binding_takeover_rejected",
                            tuic_session = %tuic_session,
                            mcp_session = %mcp_sid,
                            suppressed_since_last_report = suppressed,
                            error = %error,
                            "Refused a register takeover of a live peer identity"
                        ),
                        None => tracing::debug!(
                            source = "agent_msg",
                            event = "live_binding_takeover_rejected",
                            tuic_session = %tuic_session,
                            mcp_session = %mcp_sid,
                            "Refused a register takeover (repeat)"
                        ),
                    }
                    return serde_json::json!({"error": error});
                }
            };
            if let Some(prior_mcp) = prior_mcp {
                tracing::warn!(
                    source = "agent_msg",
                    event = "stale_binding_takeover",
                    tuic_session = %tuic_session,
                    prior_mcp_session = %prior_mcp,
                    mcp_session = %mcp_sid,
                    "Reclaimed stale MCP peer binding after reconnect"
                );
            }
            // Which prior identity is this registration superseding? The implicit
            // answer only exists when the SAME protocol session rebinds. A caller
            // that reconnects and registers a new UUID arrives with no link at all,
            // so its old inbox used to be stranded with nobody told — it must say
            // which identity it replaces. Guessing (by name, by project) is not an
            // option: peer identity decides who may read whose mail.
            let superseded = previously_bound
                .filter(|prior| prior != &tuic_session)
                .or_else(|| {
                    args["replaces"]
                        .as_str()
                        .map(str::to_string)
                        .filter(|prior| prior != &tuic_session)
                        .filter(|prior| state.peer_agents.contains_key(prior))
                });
            let handoff = superseded
                .as_ref()
                .map(|phantom| retire_repaired_phantom_identity(state, phantom, &tuic_session));
            if orchestrator {
                state.orchestrator_peers.insert(tuic_session.clone());
            } else {
                state.orchestrator_peers.remove(&tuic_session);
                state.clear_orchestrator_delivery(&tuic_session);
            }
            let linked_children = link_pending_children_to_parent(state, &mcp_sid, &tuic_session);
            // Identity bindings are security-relevant; record them (no message content).
            tracing::info!(
                source = "agent_msg",
                event = "register",
                tuic_session = %tuic_session,
                mcp_session = %mcp_sid,
                name = %name,
                "Peer registered"
            );
            let _ = state
                .event_bus
                .send(crate::state::AppEvent::PeerRegistered {
                    tuic_session: tuic_session.to_string(),
                    name: name.clone(),
                });
            // Teach the full multi-agent workflow in the register response so the
            // static instructions can stay compact (AC1 token budget). Any agent
            // that registers immediately receives the operational details it needs
            // for spawn/monitor/cleanup.
            // Whether this identity has a terminal behind it. An identity that
            // resolves to no live PTY is a mailbox and nothing more: no `send` can be
            // typed into it and no wake can reach it, so it must say so instead of
            // implying the peer is addressable in the usual sense. That silence is
            // how a self-registered identity with no PTY (headerless bridge, agent
            // launched outside TUIC, invented UUID) ended up losing every reply.
            let has_terminal = state.live_pty_for_peer(&tuic_session).is_some();
            let has_managed_lifecycle = state
                .live_pty_for_peer(&tuic_session)
                .and_then(|session_id| {
                    state
                        .session_maps
                        .session_states
                        .get(&session_id)
                        .map(|session| session.agent_type.is_some())
                })
                .unwrap_or(false);
            let wake_capability = if orchestrator && has_managed_lifecycle {
                "managed_pty_lifecycle"
            } else {
                "none"
            };
            let mut response = serde_json::json!({
                "ok": true,
                "tuic_session": tuic_session,
                "name": name,
                "linked_children": linked_children,
                "identity_generated": generated_identity,
                "terminal": has_terminal,
                "orchestrator": orchestrator,
                "mail_wake": wake_capability,
                "identity": if !has_terminal {
                    "This identity has NO terminal behind it: nothing can be typed into it and no message can wake it. Incoming mail only lands in your inbox, so you MUST consume it yourself — `agent action=wait` (blocking) or `agent action=inbox`. To be reachable through a terminal, run inside a TUIC-managed PTY so the bridge asserts its $TUIC_SESSION, or let TUIC spawn you with agent action=spawn."
                } else if generated_identity {
                    "This headerless caller now has an MCP-scoped UUID. It is stable for this MCP connection. Supply an explicit UUID on a future connection when cross-reconnect identity stability is required."
                } else {
                    "This MCP session is bound to its managed or explicitly supplied stable UUID."
                },
                "workflow": {
                    "spawn_same_repo": "agent action=spawn prompt=<task> cwd=<repo_path> — returns {session_id, name, task_id}. As orchestrator, prefer wait/inbox over raw session output to avoid token burn.",
                    "spawn_isolated": "repo action=worktree_create path=<repo> branch=<name> spawn_session=true — worktree + PTY in one call.",
                    "monitor": "Use blocking waits instead of polling: agent action=wait (wakes on new mail; the cursor is kept server-side) or session action=wait session_id=<id> until=idle|exited. Task results arrive through agent send/inbox. Use session output only as an anomaly fallback when a child failed to send.",
                    "auto_state_change": "Spawned peers auto-post state only: {type:state_change, state:idle|completed|exited|awaiting_input, session_id, exit_code?, prompt?}. This is not task output. awaiting_input means the child hit an interactive prompt and is parked with nobody at its keyboard — it will NOT progress until you answer it with session action=input (the `prompt` field carries the question). Every child must report its result or blocker with agent action=send; use session output only when a child anomalously failed to send.",
                    "send": "agent action=send to=<peer tuic_session | its registered name | its PTY id | that terminal's alias, e.g. tu-1> message=<text, max 64KB> [urgency=normal|urgent]. Normal is the default; urgent writes only a payload-free inbox notice to a safe busy Claude/Codex composer for its next tool boundary and returns urgent_delivered or urgent_fallback_reason. The message is always buffered in the inbox. A peer explicitly registered with orchestrator=true keeps payloads out of its active turn and composer; managed idle/completed lifecycle, or a confirmed-ready empty composer held working only by background work, may submit one coalesced, payload-free wake instructing `agent action=inbox`. Busy, questioning, partially typed, external without a subscribed ACP inbox, or unknown state stays inbox-only. An active agent wait owns delivery and suppresses that wake. Check `delivered` and `delivery_path` (the only route field); a message reaching the inbox is not delivery.",
                    "list_peers": "agent action=list_peers path=<optional filter> — see who else is connected.",
                    "conflict_control": "Use send/inbox to serialize shared-file edits: child sends 'claim <path>', orchestrator replies 'ack'/'deny'; child sends 'release <path>' on commit. Orchestrator is the arbiter — children never ack each other directly.",
                    "cleanup": "On MCP session close, peer routes and inbox are drained. Managed PTY lifecycle remains separate; an MCP-scoped external identity has no PTY to reap."
                }
            });
            // Say what happened to the superseded identity's mail. Both outcomes were
            // silent before: a migration looked like nothing happened, and a skip left
            // messages addressed to the old UUID unread with no wake and no notice.
            if let (Some(prior), Some(handoff)) = (superseded.as_deref(), handoff) {
                let object = response
                    .as_object_mut()
                    .expect("register response is an object");
                object.insert("superseded_identity".to_string(), serde_json::json!(prior));
                match handoff {
                    IdentityHandoff::Migrated { messages } => {
                        object.insert("mail_migrated".to_string(), serde_json::json!(messages));
                    }
                    IdentityHandoff::SkippedLivePty { pending } => {
                        object.insert("mail_migrated".to_string(), serde_json::json!(0));
                        object.insert("mail_stranded".to_string(), serde_json::json!(pending));
                        object.insert("identity_warning".to_string(), serde_json::json!(format!(
                            "'{prior}' still owns a live PTY, so it is a reachable peer and its mail was NOT moved: {pending} message(s) remain addressed to it. Taking them would strand a working agent. Read them as that identity, or have it hand over by registering from its own session."
                        )));
                    }
                }
            }
            response
        }
        "list_peers" => {
            let project_filter = args["path"].as_str();
            let peers: Vec<serde_json::Value> = state
                .peer_agents
                .iter()
                .filter(|entry| {
                    if let Some(filter) = project_filter {
                        entry.value().project.as_deref() == Some(filter)
                    } else {
                        true
                    }
                })
                .map(|entry| {
                    let p = entry.value();
                    // The terminal behind the peer, when it has one. Without it
                    // `list_peers` answered with names nothing could be typed into,
                    // so finding "the tab working on X" still cost a session list
                    // and a manual join.
                    let live_pty = state.live_pty_for_peer(&p.tuic_session);
                    let mut peer = serde_json::json!({
                        "tuic_session": p.tuic_session,
                        "name": p.name,
                        "orchestrator": state.orchestrator_peers.contains(&p.tuic_session),
                    });
                    let object = peer.as_object_mut().expect("peer entry is an object");
                    insert_optional_value(
                        object,
                        "alias",
                        live_pty
                            .as_deref()
                            .and_then(|session_id| {
                                state
                                    .session_maps
                                    .term_aliases
                                    .get(session_id)
                                    .map(|alias| alias.value().clone())
                            })
                            .map(serde_json::Value::String),
                    );
                    insert_optional_value(
                        object,
                        "session_id",
                        live_pty.map(serde_json::Value::String),
                    );
                    insert_optional_value(
                        object,
                        "path",
                        p.project.clone().map(serde_json::Value::String),
                    );
                    peer
                })
                .collect();
            serde_json::json!({"peers": peers})
        }
        "send" => {
            let requested_to = match args["to"].as_str() {
                Some(s) if !s.is_empty() => s,
                _ => {
                    return serde_json::json!({"error": "Action 'send' requires 'to' — the recipient's tuic_session, registered name, the id of the PTY it runs in, or that terminal's alias (e.g. 'tu-1')"});
                }
            };
            // Mail is filed under the peer key, so an address that names the
            // terminal has to be walked back to the peer that owns it — otherwise
            // "notify tu-1" is a dead letter with a valid-looking address.
            let resolved_to = match state.resolve_peer_ref_checked(requested_to) {
                Ok(resolved) => resolved,
                Err(error) => return serde_json::json!({"error": error}),
            };
            let to = resolved_to.as_deref().unwrap_or(requested_to);
            let keep_open = match args.get("keep_open") {
                None => None,
                Some(value) => match value.as_bool() {
                    Some(value) => Some(value),
                    None => return serde_json::json!({"error": "keep_open must be a boolean"}),
                },
            };
            if keep_open.is_some() && !state.session_maps.session_parent.contains_key(to) {
                return serde_json::json!({"error": "keep_open applies only to managed child sessions"});
            }
            let message = match args["message"].as_str() {
                Some(s) if !s.is_empty() => s,
                _ => return serde_json::json!({"error": "Action 'send' requires 'message'"}),
            };
            if message.len() > crate::state::AGENT_MESSAGE_MAX_BYTES {
                return serde_json::json!({"error": format!(
                    "Message exceeds 64 KB limit ({} bytes)", message.len()
                )});
            }
            let urgent = match args.get("urgency") {
                None => false,
                Some(serde_json::Value::String(value)) if value == "normal" => false,
                Some(serde_json::Value::String(value)) if value == "urgent" => true,
                _ => {
                    return serde_json::json!({"error": "urgency must be 'normal' or 'urgent'"});
                }
            };
            // Resolve sender via O(1) mcp_to_session reverse map (RUST-3/PERF-2).
            let sender = match mcp_session_id
                .and_then(|sid| state.mcp.to_session.get(sid).map(|e| e.value().clone()))
                .and_then(|tuic| {
                    state
                        .peer_agents
                        .get(&tuic)
                        .map(|p| (p.tuic_session.clone(), p.name.clone()))
                }) {
                Some(s) => s,
                None => {
                    return serde_json::json!({"error": "You are not registered. Register first with agent action=register"});
                }
            };
            let now_ms = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_millis() as u64;
            let (sender_tuic, sender_name) = sender;
            let msg = crate::state::AgentMessage {
                id: forwarded_message_id
                    .map(str::to_owned)
                    .unwrap_or_else(|| uuid::Uuid::new_v4().to_string()),
                from_tuic_session: sender_tuic.clone(),
                from_name: sender_name.clone(),
                content: message.to_string(),
                timestamp: now_ms,
                delivered_via_channel: false,
            };
            let msg_id = msg.id.clone();

            // Buffer first, then assign exactly one wake-up owner. Assignment preserves
            // a claim made by a concurrent waiter between these two operations.
            // External peers without a live SSE subscriber remain inbox-only.
            //
            // The existence check and the buffering are one critical section under
            // the identity lock: `retire_repaired_phantom_identity` deletes the
            // recipient and drains its inbox under the same guard, so a message can
            // never be filed under an identity that is removed a moment later.
            let message_timestamp = {
                let _bind_guard = PEER_IDENTITY_BIND_LOCK.lock();
                if !state.peer_agents.contains_key(to) {
                    return serde_json::json!({"error": format!(
                        "Recipient '{requested_to}' is not registered — it matched no tuic_session, registered name, PTY id or terminal alias. Use list_peers to find valid targets."
                    )});
                }
                if let Some(id) = forwarded_message_id {
                    match super::remote_peer::enqueue_forwarded(state, to, msg) {
                        Ok(Some(timestamp)) => timestamp,
                        Ok(None) => {
                            return serde_json::json!({"message_id":id,"delivered":false,"delivery_path":"inbox_duplicate"});
                        }
                        Err(detail) => return serde_json::json!({"error":detail}),
                    }
                } else {
                    state.push_agent_inbox(to, msg)
                }
            };
            if let Some(enabled) = keep_open {
                if enabled {
                    state.keep_open_sessions.insert(to.to_string());
                } else {
                    state.keep_open_sessions.remove(to);
                }
            }
            // DEFERRED (2026-09-23) — a recipient with no terminal (an external
            // MCP client) has no Progress Flow column, so its mail is not
            // journaled. Showing it needs a non-terminal participant kind.
            if let Some(to_pty) = state.live_pty_for_peer(to) {
                let to_name = state.peer_agents.get(to).map(|peer| peer.name.clone());
                journal_hand_off(
                    state,
                    crate::progress::ProgressKind::Message,
                    &sender_tuic,
                    &to_pty,
                    to_name.as_deref(),
                    message,
                );
            }
            // Resolve, do not compare. A self-registering agent announces its
            // `$TUIC_SESSION`, which is not the key its PTY was filed under, so the
            // old `sessions.contains_key(to)` answered "no terminal" for every peer
            // that had not been spawned by the server itself.
            let live_pty = state.live_pty_for_peer(to);
            let managed_recipient = live_pty.is_some();
            let acp_inbox_subscriber = !managed_recipient && state.acp.has_acp_inbox_subscriber(to);
            if urgent {
                use crate::state::{AgentDeliveryAssignment, UrgentNoticeReservation};

                let mut delivered = false;
                let mut urgent_delivered = false;
                let mut delivery_path = "inbox_only";
                let mut fallback_reason = None;
                // A blocking wait already owns the newly buffered message and
                // returns it inside the recipient's current turn.
                if state.assign_agent_delivery(to, &msg_id, false)
                    == AgentDeliveryAssignment::Waiter
                {
                    delivered = true;
                    urgent_delivered = true;
                    delivery_path = "waiter_and_inbox";
                } else if let Some(pty_session) = live_pty.as_deref() {
                    match state.reserve_urgent_notice(to, &sender_tuic, message_timestamp) {
                        UrgentNoticeReservation::AlreadyRead => {
                            delivered = true;
                            urgent_delivered = true;
                            delivery_path = "inbox_read";
                        }
                        UrgentNoticeReservation::Written => {
                            delivered = true;
                            urgent_delivered = true;
                            delivery_path = "coalesced_urgent_notice_and_inbox";
                        }
                        UrgentNoticeReservation::InFlight => {
                            fallback_reason = Some("urgent_notice_in_flight");
                        }
                        UrgentNoticeReservation::Reserved => {
                            let outcome = crate::pty::deliver_urgent_mail_notice(
                                state,
                                pty_session,
                                &sender_tuic,
                            );
                            state.finish_urgent_notice(
                                to,
                                &sender_tuic,
                                message_timestamp,
                                outcome.is_ok(),
                            );
                            match outcome {
                                Ok(()) => {
                                    delivered = true;
                                    urgent_delivered = true;
                                    delivery_path = "urgent_notice_and_inbox";
                                }
                                Err(reason) => fallback_reason = Some(reason),
                            }
                        }
                    }
                } else {
                    fallback_reason = Some("recipient_exited");
                }

                // A failed urgent write still has a durable inbox copy. Use
                // the existing safe idle wake/queue path, never the ordinary
                // Claude channel because it carries the peer's payload.
                if let Some(reason) = fallback_reason
                    && reason != "urgent_notice_in_flight"
                    && reason != "write_uncertain"
                {
                    if let Some(assignment) = crate::pty::route_registered_orchestrator_mail(
                        state,
                        to,
                        &msg_id,
                        message_timestamp,
                    ) {
                        use crate::state::OrchestratorDeliveryAssignment;
                        match assignment {
                            OrchestratorDeliveryAssignment::Waiter => {
                                delivered = true;
                                urgent_delivered = true;
                                delivery_path = "waiter_and_inbox";
                                fallback_reason = None;
                            }
                            OrchestratorDeliveryAssignment::WakeSubmitted => {
                                delivered = true;
                                delivery_path = "wake_notification_and_inbox";
                            }
                            OrchestratorDeliveryAssignment::WakeCoalesced => {
                                delivered = true;
                                delivery_path = "coalesced_wake_and_inbox";
                            }
                            OrchestratorDeliveryAssignment::WakeSummarySubmitted => {
                                delivered = true;
                                delivery_path = "lifecycle_summary_and_inbox";
                            }
                            OrchestratorDeliveryAssignment::InboxOnly => {}
                        }
                    } else {
                        match state.assign_agent_delivery(to, &msg_id, managed_recipient) {
                            AgentDeliveryAssignment::Waiter => {
                                delivered = true;
                                urgent_delivered = true;
                                delivery_path = "waiter_and_inbox";
                                fallback_reason = None;
                            }
                            AgentDeliveryAssignment::Terminal => {
                                if let Some(pty_session) = live_pty.as_deref() {
                                    let outcome = crate::pty::deliver_notice_to_managed_pty(
                                        state,
                                        pty_session,
                                        crate::pty::PEER_MAIL_WAKE,
                                    );
                                    crate::pty::settle_terminal_delivery(
                                        state, to, &msg_id, outcome,
                                    );
                                    if outcome != crate::pty::PtyDelivery::Unavailable {
                                        delivered = true;
                                        delivery_path = "wake_notification_and_inbox";
                                    }
                                }
                            }
                            AgentDeliveryAssignment::InboxOnly => {}
                        }
                    }
                }
                let mut response = serde_json::json!({
                    "message_id": msg_id,
                    "delivered": delivered,
                    "delivery_path": delivery_path,
                    "urgent_delivered": urgent_delivered,
                });
                if let Some(reason) = fallback_reason {
                    response["urgent_fallback_reason"] = serde_json::json!(reason);
                }
                apply_acp_inbox_receipt(&mut response, acp_inbox_subscriber);
                tracing::info!(
                    source = "agent_msg",
                    event = "urgent_send",
                    from = %sender_tuic,
                    to = %to,
                    message_id = %msg_id,
                    urgent_delivered = %response["urgent_delivered"],
                    delivery_path = %response["delivery_path"],
                    "Urgent peer mail routed"
                );
                insert_optional_value(
                    response
                        .as_object_mut()
                        .expect("send response is an object"),
                    "recipient_state",
                    live_pty
                        .as_deref()
                        .and_then(|pty_session| managed_recipient_state(state, pty_session)),
                );
                return response;
            }
            if let Some(assignment) = crate::pty::route_registered_orchestrator_mail(
                state,
                to,
                &msg_id,
                message_timestamp,
            ) {
                use crate::state::OrchestratorDeliveryAssignment;

                let (delivered, delivery_path) = match assignment {
                    OrchestratorDeliveryAssignment::Waiter => (true, "waiter_and_inbox"),
                    OrchestratorDeliveryAssignment::WakeSubmitted => {
                        (true, "wake_notification_and_inbox")
                    }
                    // Unreachable for a peer `send`: its own payload is in the
                    // covered window, which disqualifies the lifecycle summary.
                    // Mapped anyway so the two never drift.
                    OrchestratorDeliveryAssignment::WakeSummarySubmitted => {
                        (true, "lifecycle_summary_and_inbox")
                    }
                    OrchestratorDeliveryAssignment::WakeCoalesced => {
                        (true, "coalesced_wake_and_inbox")
                    }
                    OrchestratorDeliveryAssignment::InboxOnly => (false, "inbox_only"),
                };
                let mut response = serde_json::json!({
                    "message_id": msg_id,
                    "delivered": delivered,
                    // See the note on the other send response: `delivery_path` is the
                    // single source of truth for the route. This branch used to
                    // hardcode `delivered_via_channel: false` next to a `delivered:
                    // true` — factually correct, and unreadable.
                    "delivery_path": delivery_path,
                });
                if !delivered {
                    let object = response
                        .as_object_mut()
                        .expect("send response is an object");
                    object.insert(
                        "warning".to_string(),
                        serde_json::json!(if managed_recipient {
                            "The orchestrator has no active wait or safely claimable composer. The message remains inbox-only until it reads the inbox."
                        } else {
                            "Recipient has NO terminal and no active wait: nothing will wake it. The message stays in its inbox until it calls agent action=wait/inbox. If you need an answer, do not block on it."
                        }),
                    );
                }
                apply_acp_inbox_receipt(&mut response, acp_inbox_subscriber);
                tracing::info!(
                    source = "agent_msg",
                    event = "send",
                    from = %sender_tuic,
                    from_name = %sender_name,
                    to = %to,
                    bytes = message.len(),
                    delivered_via_channel = false,
                    delivery_path = %response["delivery_path"],
                    message_id = %msg_id,
                    "Peer message routed to orchestrator inbox"
                );
                insert_optional_value(
                    response
                        .as_object_mut()
                        .expect("send response is an object"),
                    "recipient_state",
                    live_pty
                        .as_deref()
                        .and_then(|pty_session| managed_recipient_state(state, pty_session)),
                );
                return response;
            }
            // External Claude clients have no PTY; their SSE channel can
            // surface mail while managed peers keep a deferred terminal wake.
            let recipient_mcp_sid = state.peer_agents.get(to).map(|p| p.mcp_session_id.clone());
            let (delivery_assignment, pushed) = state.assign_agent_delivery_with_channel_attempt(
                to,
                &msg_id,
                managed_recipient,
                || {
                    // Managed mail must retain the PTY's idle wake. An SSE push
                    // during a turn cannot start a new turn after it completes.
                    if managed_recipient {
                        return false;
                    }
                    let Some(mcp_sid) = recipient_mcp_sid.as_ref() else {
                        return false;
                    };
                    if !external_recipient_supports_claude_channel(state, mcp_sid) {
                        return false;
                    }
                    let Some(channel) = state.session_maps.messaging_channels.get(mcp_sid) else {
                        return false;
                    };
                    let content = if message.len() <= SSE_INLINE_MESSAGE_MAX_BYTES {
                        format!("Message from {sender_tuic}: {message}")
                    } else {
                        let mut subject = String::new();
                        for character in message
                            .lines()
                            .next()
                            .unwrap_or_default()
                            .chars()
                            .filter(|character| !character.is_control())
                        {
                            if subject.len() + character.len_utf8() > SSE_POINTER_SUBJECT_MAX_BYTES
                            {
                                break;
                            }
                            subject.push(character);
                        }
                        format!(
                            "{}\nfrom {} id {} {} bytes: {}",
                            crate::pty::PEER_MAIL_WAKE,
                            sender_tuic,
                            msg_id,
                            message.len(),
                            subject
                        )
                    };
                    let notification = serde_json::json!({
                        "jsonrpc": "2.0",
                        "method": "notifications/claude/channel",
                        "params": {
                            "content": content,
                            "meta": {
                                "from_tuic_session": sender_tuic,
                                "message_id": msg_id,
                            }
                        }
                    });
                    let notification = serde_json::to_string(&notification).unwrap_or_default();
                    channel.send(notification).is_ok()
                },
            );
            let waiter_owned = matches!(
                delivery_assignment,
                crate::state::AgentDeliveryAssignment::Waiter
            );
            let terminal_owned = matches!(
                delivery_assignment,
                crate::state::AgentDeliveryAssignment::Terminal
            );
            let mut inbox_only = matches!(
                delivery_assignment,
                crate::state::AgentDeliveryAssignment::InboxOnly
            );
            if pushed
                && let Some(mut inbox) = state.agent_inbox.get_mut(to)
                && let Some(message) = inbox.iter_mut().find(|m| m.id == msg_id)
            {
                message.delivered_via_channel = true;
            }

            #[cfg(unix)]
            if let Some(pty_session) = live_pty.as_deref()
                && let Err(e) = crate::pty::wake_session(state, pty_session)
            {
                tracing::debug!(session = %pty_session, error = %e, "Wake on message delivery failed");
            }
            // Event-driven wake: tell a managed recipient it has mail without
            // polling. The line typed is `PEER_MAIL_WAKE` — a pointer, never
            // the payload. External SSE recipients have no PTY to wake.
            let terminal_outcome =
                live_pty
                    .as_ref()
                    .filter(|_| terminal_owned)
                    .map(|pty_session| {
                        crate::pty::deliver_notice_to_managed_pty(
                            state,
                            pty_session,
                            crate::pty::PEER_MAIL_WAKE,
                        )
                    });
            if let Some(outcome) = terminal_outcome {
                crate::pty::settle_terminal_delivery(state, to, &msg_id, outcome);
                // Only a session that cannot take the message at all falls back to
                // the inbox. `Queued` is still a terminal delivery — it types on the
                // recipient's next idle transition — so reporting it as inbox_only
                // would understate what happens.
                inbox_only = outcome == crate::pty::PtyDelivery::Unavailable;
            }
            let delivery_path = if waiter_owned {
                "waiter_and_inbox"
            } else if pushed {
                "sse_channel_and_inbox"
            } else if inbox_only {
                "inbox_only"
            } else {
                "wake_notification_and_inbox"
            };
            let mut response = serde_json::json!({
                "message_id": msg_id,
                // `delivered` answers the only question the sender actually has:
                // will anything surface this message? A waiter consumed it, the SSE
                // channel took it, or the terminal typed/queued it — those reach the
                // recipient. `inbox_only` does not: it means no waiter, no channel and
                // no live terminal, so the message sits unread until the recipient
                // polls. Reporting that as a bare `ok` is how an orchestrator's reply
                // vanished while both sides believed delivery had happened.
                "delivered": !inbox_only,
                // No `delivered_via_channel` here: it reported one sub-route (SSE)
                // but read as a delivery verdict, so `false` alongside a confirming
                // `delivery_path` was pure ambiguity. `delivery_path` names the SSE
                // case as `sse_channel_and_inbox` and subsumes it.
                "delivery_path": delivery_path,
            });
            if inbox_only {
                let object = response
                    .as_object_mut()
                    .expect("send response is an object");
                object.insert(
                    "warning".to_string(),
                    serde_json::json!(if let Some(pty_session) = live_pty.as_deref() {
                        crate::pty::agent_mail_wake_detail(state, pty_session)
                    } else {
                        "Recipient has NO terminal and no active wait: nothing will wake it. The message stays in its inbox until it calls agent action=wait/inbox. If you need an answer, do not block on it.".to_string()
                    }),
                );
            }
            insert_optional_value(
                response
                    .as_object_mut()
                    .expect("send response is an object"),
                "recipient_state",
                live_pty
                    .as_deref()
                    .and_then(|pty_session| managed_recipient_state(state, pty_session)),
            );
            apply_acp_inbox_receipt(&mut response, acp_inbox_subscriber);
            // Forensic trail omits the mail body, which may contain sensitive data.
            tracing::info!(
                source = "agent_msg",
                event = "send",
                from = %sender_tuic,
                from_name = %sender_name,
                to = %to,
                bytes = message.len(),
                delivered_via_channel = pushed,
                delivery_path = %response["delivery_path"],
                message_id = %msg_id,
                "Peer message delivered"
            );
            response
        }
        "inbox" => {
            // Resolve caller's tuic_session via O(1) mcp_to_session reverse map (RUST-3/PERF-2).
            let tuic_session = match mcp_session_id
                .and_then(|sid| state.mcp.to_session.get(sid).map(|e| e.value().clone()))
                .filter(|tuic| state.peer_agents.contains_key(tuic))
            {
                Some(ts) => ts,
                None => {
                    return serde_json::json!({"error": "You are not registered. Register first with agent action=register"});
                }
            };
            let limit = args["limit"]
                .as_u64()
                .unwrap_or(crate::state::AGENT_INBOX_CAPACITY as u64)
                .clamp(1, crate::state::AGENT_INBOX_CAPACITY as u64)
                as usize;
            let since = resolve_agent_since(state, &tuic_session, args);
            let (messages, has_more, missed_count) =
                state.observe_agent_inbox(&tuic_session, since, limit);
            // Same contract as wait: always hand back a usable cursor, falling back
            // to the position we read from when the batch is empty.
            let next_since = messages
                .iter()
                .map(|message| message.timestamp)
                .max()
                .unwrap_or(since);
            advance_agent_cursor(state, &tuic_session, next_since);
            let message_ids: Vec<&str> =
                messages.iter().map(|message| message.id.as_str()).collect();
            tracing::info!(
                source = "agent_msg",
                event = "inbox_read",
                caller_session_id = mcp_session_id.unwrap_or(""),
                caller_peer_id = %tuic_session,
                inbox_owner = %tuic_session,
                message_ids = %serde_json::json!(message_ids),
                "Peer inbox read"
            );
            let mut resp = serde_json::json!({
                "messages": messages,
                "count": messages.len(),
                "next_since": next_since,
                "has_more": has_more,
            });
            if missed_count > 0 {
                resp["missed_count"] = serde_json::json!(missed_count);
            }
            resp
        }
        other => serde_json::json!({"error": format!(
            "Unknown action '{}' for tool 'agent'. Available: {}", other, AGENT_ACTIONS
        )}),
    }
}
