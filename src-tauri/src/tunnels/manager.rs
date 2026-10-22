use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};

use chrono::{DateTime, Utc};
use dashmap::DashMap;
use parking_lot::Mutex;

use super::audit::{AuditLog, EventKind};
use super::profile::TunnelProfile;
use super::supervisor::{GRACEFUL_SHUTDOWN_TIMEOUT, TunnelStatus, TunnelSupervisor};

pub struct TunnelHandle {
    pub profile: TunnelProfile,
    pub supervisor: TunnelSupervisor,
    pub started_at: DateTime<Utc>,
}

enum TunnelSlot {
    Starting(u64),
    Running(Arc<Mutex<TunnelHandle>>),
}

struct StartReservation<'a> {
    manager: &'a TunnelManager,
    id: String,
    token: u64,
    /// The slot is ours (the claim succeeded); only then may Drop remove it.
    claimed: bool,
    published: bool,
}

impl StartReservation<'_> {
    fn publish(&mut self, handle: Arc<Mutex<TunnelHandle>>) -> bool {
        let Some(mut slot) = self.manager.tunnels.get_mut(&self.id) else {
            return false;
        };
        if !matches!(*slot, TunnelSlot::Starting(token) if token == self.token) {
            return false;
        }
        *slot = TunnelSlot::Running(handle);
        self.published = true;
        true
    }
}

impl Drop for StartReservation<'_> {
    fn drop(&mut self) {
        // Runs when `start_with` returns, i.e. AFTER its lost-publication
        // cleanup has been awaited, so a zero in-flight count means every start
        // has finished (or discarded) whatever it spawned.
        let _release = InFlightRelease(&self.manager.starting_in_flight);
        if self.published || !self.claimed {
            return;
        }
        if let dashmap::mapref::entry::Entry::Occupied(entry) =
            self.manager.tunnels.entry(self.id.clone())
            && matches!(entry.get(), TunnelSlot::Starting(token) if *token == self.token)
        {
            entry.remove();
        }
    }
}

/// Decrements the in-flight start counter on drop.
struct InFlightRelease<'a>(&'a tokio::sync::watch::Sender<usize>);

impl Drop for InFlightRelease<'_> {
    fn drop(&mut self) {
        self.0.send_modify(|count| *count -= 1);
    }
}

pub struct TunnelManager {
    tunnels: DashMap<String, TunnelSlot>,
    /// Number of `start_with` calls between claiming a slot and returning.
    /// `shutdown_all_and_wait` waits for it to reach zero: removing a
    /// `Starting` reservation alone says nothing about the supervisor/ssh that
    /// start is about to create (and then has to clean up itself).
    starting_in_flight: tokio::sync::watch::Sender<usize>,
    /// Set (and never cleared) by `shutdown_all_and_wait` BEFORE it snapshots the
    /// map. A start that claims its id after that snapshot would otherwise
    /// publish a Running tunnel nothing will ever stop; see `start_with`.
    shutting_down: AtomicBool,
    /// Cleanup of a start that lost its publication race (its `stop_and_take_task`
    /// wait). Owned HERE, on a spawned task, rather than by the `start_with`
    /// future: a caller can abort that future (e.g. the remote-runtime
    /// supervisor's `dispose`), which would drop the wait and let the in-flight
    /// counter reach zero while the supervisor's ssh is still being killed.
    /// `shutdown_all_and_wait` joins these.
    cleanups: Mutex<Vec<tokio::task::JoinHandle<()>>>,
    next_reservation: AtomicU64,
    /// Wrapped in Mutex so Arc<Mutex<AuditLog>> is Send+Sync and can be
    /// captured in the `Send + 'static` status callback required by TunnelSupervisor.
    audit: Arc<Mutex<AuditLog>>,
}

impl TunnelManager {
    pub fn new(audit: Arc<Mutex<AuditLog>>) -> Self {
        Self {
            tunnels: DashMap::new(),
            starting_in_flight: tokio::sync::watch::channel(0).0,
            shutting_down: AtomicBool::new(false),
            cleanups: Mutex::new(Vec::new()),
            next_reservation: AtomicU64::new(1),
            audit,
        }
    }

    /// Start a tunnel for `profile`, store its handle, and log the Started event.
    /// Returns the profile id on success.
    pub async fn start(&self, profile: TunnelProfile) -> Result<String, String> {
        self.start_with(profile, |profile, callback| async move {
            TunnelSupervisor::start(profile, callback).await
        })
        .await
    }

    async fn start_with<F, Fut>(&self, profile: TunnelProfile, spawn: F) -> Result<String, String>
    where
        F: FnOnce(TunnelProfile, Box<dyn Fn(TunnelStatus) + Send + 'static>) -> Fut + Send,
        Fut: std::future::Future<Output = TunnelSupervisor> + Send,
    {
        let id = profile.id.clone();
        let token = self.next_reservation.fetch_add(1, Ordering::Relaxed);

        // Count this start BEFORE the slot becomes visible: `shutdown_all_and_wait`
        // removes a `Starting` slot and then reads the counter, so a slot that is
        // visible while the count is still zero would be waited for by nobody.
        // The reservation's Drop releases the count on every exit path, and only
        // removes a slot carrying its own token, so the early returns below are
        // safe.
        self.starting_in_flight.send_modify(|count| *count += 1);
        let mut reservation = StartReservation {
            manager: self,
            id: id.clone(),
            token,
            claimed: false,
            published: false,
        };

        // Claim the id ATOMICALLY, before spawning anything. A `contains_key`
        // check followed by an `.await` and a later `insert` is a TOCTOU: two
        // concurrent starts both saw the id free, both spawned a supervisor, and
        // the second insert overwrote the first — orphaning its supervisor loop
        // and ssh child forever (TunnelHandle has no Drop). The DashMap entry API
        // holds the shard lock across check-and-insert, so exactly one caller can
        // win, and it wins before any await point.
        match self.tunnels.entry(id.clone()) {
            dashmap::mapref::entry::Entry::Occupied(_) => {
                return Err(format!("tunnel '{id}' already running"));
            }
            dashmap::mapref::entry::Entry::Vacant(slot) => {
                slot.insert(TunnelSlot::Starting(token));
                reservation.claimed = true;
            }
        }
        // Checked AFTER the claim (SeqCst, like the store in
        // `shutdown_all_and_wait`, which sets the flag BEFORE its snapshot):
        // either the snapshot saw this slot and removed it (so `publish` fails
        // and this start discards its own supervisor), or the claim came after
        // the snapshot, in which case the flag is already visible here.
        if self.shutting_down.load(Ordering::SeqCst) {
            return Err(format!("tunnel '{id}' not started: app is shutting down"));
        }

        let audit = Arc::clone(&self.audit);
        let audit_cb = Arc::clone(&self.audit);
        let cb_id = id.clone();

        let status_callback = move |status: TunnelStatus| {
            let kind = match &status {
                TunnelStatus::Connected => EventKind::Connected,
                TunnelStatus::Reconnecting { .. } => EventKind::Retry,
                TunnelStatus::Stopped { .. } => EventKind::Stopped,
                TunnelStatus::Error { .. } => EventKind::Error,
                TunnelStatus::Starting => return, // Starting is logged via Started below
            };
            let detail = match &status {
                TunnelStatus::Error { message } => serde_json::json!({ "message": message }),
                TunnelStatus::Stopped { reason } => serde_json::json!({ "reason": reason }),
                TunnelStatus::Reconnecting { attempt, reason } => {
                    serde_json::json!({ "attempt": attempt, "reason": reason })
                }
                _ => serde_json::json!({}),
            };
            let _ = audit_cb.lock().insert(&cb_id, kind, detail);
        };

        let supervisor = spawn(profile.clone(), Box::new(status_callback)).await;
        if let TunnelStatus::Error { message } = supervisor.status() {
            return Err(format!("Failed to start tunnel '{id}': {message}"));
        }

        let handle = Arc::new(Mutex::new(TunnelHandle {
            profile,
            supervisor,
            started_at: Utc::now(),
        }));

        // Publish into OUR reservation. If it is gone, a stop() landed while we
        // were spawning: honour it rather than resurrecting a tunnel the user
        // asked to stop, and shut down the supervisor we just created so it does
        // not outlive this call. Waited (bounded), not fire-and-forget — this
        // supervisor and its ssh child are about to become fully unreachable
        // (nothing else holds `handle`), so if we didn't wait here, nothing
        // would ever confirm they actually stopped.
        if !reservation.publish(Arc::clone(&handle)) {
            // Extracted to its own statement, not `if let Some(task) =
            // handle.lock()...` directly: a temporary created in an `if
            // let` scrutinee lives until the end of that statement, which
            // would hold this (non-`Send`) `parking_lot::MutexGuard` across
            // the `.await` below.
            let task = handle.lock().supervisor.stop_and_take_task();
            if let Some(task) = task {
                // Handed to the manager, not awaited by this future: aborting
                // THIS future must not abandon the wait (see `cleanups`). No
                // await between `stop_and_take_task` and the registration, so
                // an abort cannot land in the gap.
                let (done_tx, done_rx) = tokio::sync::oneshot::channel::<()>();
                let cleanup = tokio::spawn(async move {
                    let _ = tokio::time::timeout(GRACEFUL_SHUTDOWN_TIMEOUT, task).await;
                    let _ = done_tx.send(());
                });
                {
                    let mut cleanups = self.cleanups.lock();
                    cleanups.retain(|c| !c.is_finished());
                    cleanups.push(cleanup);
                }
                let _ = done_rx.await;
            }
            return Err(format!("tunnel '{id}' was stopped while starting"));
        }
        let _ = audit
            .lock()
            .insert(&id, EventKind::Started, serde_json::json!({}));
        Ok(id)
    }

    /// Start a tunnel that runs `ssh_binary` instead of the real `ssh`.
    ///
    /// For tests that need a tunnel to EXIST — so they can assert who stops it —
    /// rather than to work. `start_with_binary` returns as soon as the
    /// supervision loop is spawned, so a binary that cannot be executed puts a
    /// real entry in the map without waiting on anything.
    #[cfg(test)]
    pub(crate) async fn start_with_binary_for_test(
        &self,
        profile: TunnelProfile,
        ssh_binary: std::path::PathBuf,
    ) -> Result<String, String> {
        self.start_with(profile, move |profile, callback| async move {
            TunnelSupervisor::start_with_binary(profile, ssh_binary, callback).await
        })
        .await
    }

    /// Stop the tunnel with `id`, remove it from the map, and log a Stopped event.
    pub fn stop(&self, id: &str) -> Result<(), String> {
        let handle = self
            .tunnels
            .remove(id)
            .map(|(_, v)| v)
            .ok_or_else(|| format!("tunnel '{id}' not found"))?;

        // `None` = a reservation whose start() is still spawning. Removing it is
        // the whole stop: that start() will find its slot gone and shut its own
        // supervisor down.
        if let TunnelSlot::Running(handle) = handle {
            handle.lock().supervisor.stop();
        }
        let _ = self.audit.lock().insert(
            id,
            EventKind::Stopped,
            serde_json::json!({"reason": "stop requested"}),
        );
        Ok(())
    }

    /// Stop the tunnel if it exists, ignoring "not found".
    pub fn stop_if_running(&self, id: &str) {
        if let Some((_, handle)) = self.tunnels.remove(id) {
            if let TunnelSlot::Running(handle) = handle {
                handle.lock().supervisor.stop();
            }
            let _ = self.audit.lock().insert(
                id,
                EventKind::Stopped,
                serde_json::json!({"reason": "stop requested"}),
            );
        }
    }

    /// Return all tunnel ids with their current status.
    pub fn list(&self) -> Vec<(String, TunnelStatus)> {
        self.tunnels
            .iter()
            .map(|entry| {
                let id = entry.key().clone();
                // A reservation has no supervisor to ask — it IS starting.
                let status = match entry.value() {
                    TunnelSlot::Starting(_) => TunnelStatus::Starting,
                    TunnelSlot::Running(handle) => handle.lock().supervisor.status(),
                };
                (id, status)
            })
            .collect()
    }

    /// Return the current status of a single tunnel, or `None` if not found.
    pub fn get_status(&self, id: &str) -> Option<TunnelStatus> {
        self.tunnels.get(id).map(|entry| match entry.value() {
            TunnelSlot::Starting(_) => TunnelStatus::Starting,
            TunnelSlot::Running(handle) => handle.lock().supervisor.status(),
        })
    }

    /// Stop all running tunnels and clear the map. Used on app exit.
    ///
    /// Fire-and-forget: returns as soon as the shutdown signal is sent to
    /// every supervisor, without waiting for their SSH processes to actually
    /// exit. Fine for a live, long-running app process — the async shutdown
    /// (SIGTERM, up to 5s grace, SIGKILL) keeps running on the same runtime
    /// and completes within a few seconds regardless of whether anything is
    /// still around to observe it. **Not fine for real app exit**, where the
    /// process itself may terminate before that completes — use
    /// `shutdown_all_and_wait` there instead.
    pub fn shutdown_all(&self) {
        // Collect keys, then remove one at a time — NOT iterate-then-clear().
        // A separate `clear()` after a snapshot iteration is a real TOCTOU: a
        // concurrent `start()` that publishes into the map in the gap between
        // the snapshot and the clear is wiped out by `clear()` without ever
        // being asked to stop (found by code review, 2026-09-23). Removing
        // each key individually means a handle published after our snapshot
        // was taken is simply left alone (picked up by a later shutdown call)
        // instead of being silently destroyed.
        let ids: Vec<String> = self
            .tunnels
            .iter()
            .map(|entry| entry.key().clone())
            .collect();
        for id in ids {
            if let Some((_, TunnelSlot::Running(handle))) = self.tunnels.remove(&id) {
                handle.lock().supervisor.stop();
            }
        }
    }

    /// Like `stop`, but waits — bounded by `GRACEFUL_SHUTDOWN_TIMEOUT` — for
    /// confirmation that the tunnel's underlying SSH process actually exited
    /// before returning, instead of firing the shutdown signal and returning
    /// immediately. Use this wherever "asked it to stop" isn't good enough —
    /// test cleanup, primarily, so a `#[tokio::test]`'s per-test runtime
    /// isn't torn down (aborting the supervision task before it finishes
    /// killing its child) before the process is actually gone.
    ///
    /// Known, accepted gap: if `id` is still a `TunnelSlot::Starting`
    /// reservation (its `start_with` hasn't published a real supervisor yet),
    /// this only removes the reservation and returns immediately — it does
    /// NOT wait for that in-flight `start_with` call's own eventual cleanup
    /// (see its lost-publish-race branch), because nothing here has a handle
    /// to that unrelated task to wait on. No caller in this codebase hits
    /// this today (every caller awaits its own `start()`/`start_with()` to
    /// completion before ever calling `stop_and_wait`), so the "confirmed
    /// stopped" contract is honored in practice; a future caller that calls
    /// this on a tunnel it knows is still starting would need a real fix
    /// (threading a completion signal through `TunnelSlot::Starting`), not
    /// just this doc comment.
    pub async fn stop_and_wait(&self, id: &str) -> Result<(), String> {
        let handle = self
            .tunnels
            .remove(id)
            .map(|(_, v)| v)
            .ok_or_else(|| format!("tunnel '{id}' not found"))?;

        if let TunnelSlot::Running(handle) = handle {
            let task = handle.lock().supervisor.stop_and_take_task();
            if let Some(task) = task {
                let _ = tokio::time::timeout(GRACEFUL_SHUTDOWN_TIMEOUT, task).await;
            }
        }
        let _ = self.audit.lock().insert(
            id,
            EventKind::Stopped,
            serde_json::json!({"reason": "stop requested"}),
        );
        Ok(())
    }

    /// Like `shutdown_all`, but waits for confirmation that every tunnel's
    /// SSH process actually exited before returning — used on real app exit,
    /// where the process is about to disappear and a fire-and-forget signal
    /// has no guarantee of being delivered (let alone acted on) before that
    /// happens. All tunnels are asked to stop concurrently, not one after
    /// another, and the WHOLE batch (including any start still in flight, see
    /// `starting_in_flight`) shares a single `GRACEFUL_SHUTDOWN_TIMEOUT`
    /// bound — this does not scale with tunnel count, so app exit can never
    /// hang indefinitely no matter how many tunnels are running.
    pub async fn shutdown_all_and_wait(&self) {
        // Collect keys, then remove one at a time — NOT iterate-then-clear().
        // Same real TOCTOU as `shutdown_all` (see its comment): a separate
        // `clear()` after the snapshot can silently drop a tunnel a
        // concurrent `start()` publishes in the gap, with no stop signal and
        // no wait — directly undermining the "no orphaned SSH process on
        // exit" guarantee this method exists to provide.
        //
        // The flag goes up BEFORE the snapshot so a start that claims an id
        // after it cannot publish a tunnel this call will never stop (see
        // `start_with`).
        self.shutting_down.store(true, Ordering::SeqCst);
        let ids: Vec<String> = self
            .tunnels
            .iter()
            .map(|entry| entry.key().clone())
            .collect();
        let tasks: Vec<tokio::task::JoinHandle<()>> = ids
            .into_iter()
            .filter_map(|id| match self.tunnels.remove(&id) {
                Some((_, TunnelSlot::Running(handle))) => {
                    handle.lock().supervisor.stop_and_take_task()
                }
                _ => None,
            })
            .collect();

        // Also wait for every in-flight `start_with`: the reservation removed
        // above makes it discard what it was spawning, but only once its
        // `spawn` future returns, which may be after this batch otherwise
        // finishes (and the process exits, orphaning that ssh).
        let mut in_flight = self.starting_in_flight.subscribe();
        let joined = async {
            for task in tasks {
                let _ = task.await;
            }
            let _ = in_flight.wait_for(|count| *count == 0).await;
            // After the counter: every start that returned or was aborted has
            // registered its cleanup (synchronously) before it could be dropped.
            let cleanups = std::mem::take(&mut *self.cleanups.lock());
            for cleanup in cleanups {
                let _ = cleanup.await;
            }
        };
        let _ = tokio::time::timeout(GRACEFUL_SHUTDOWN_TIMEOUT, joined).await;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    use std::path::PathBuf;
    use std::sync::Arc;

    /// This mints a fresh executable inode under `$TMPDIR` on every call, which
    /// is the shape that made the worktree post-checkout hook test cost 27s:
    /// macOS scans a never-before-seen executable on its first exec, measured
    /// 2026-09-06 at 25-192s under `$TMPDIR` against ~0.3s elsewhere.
    ///
    /// It is acceptable *here* only because no test in this module ever waits on
    /// that exec — `start_with` records `Started` once the spawn returns, so the
    /// scan is paid by the orphaned child and never enters a timing window. All
    /// 12 tests run in ~0.07s total, repeatedly.
    ///
    /// That is incidental, not designed. The moment a test awaits process
    /// readiness, output, or exit status, this helper must switch to the stable
    /// pre-warmed script under `target/` that `supervisor.rs` and
    /// `worktree::tests::shared_post_checkout_hook` already use.
    fn fake_ssh_script(behavior: &str) -> tempfile::NamedTempFile {
        let mut f = tempfile::NamedTempFile::new().unwrap();
        writeln!(f, "#!/bin/sh\n{behavior}").unwrap();
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(f.path(), std::fs::Permissions::from_mode(0o755)).unwrap();
        }
        f
    }

    fn test_profile(name: &str) -> TunnelProfile {
        TunnelProfile {
            id: uuid::Uuid::new_v4().to_string(),
            name: name.to_string(),
            host: "example.com".to_string(),
            port: 22,
            user: "alice".to_string(),
            identity_file: None,
            forwards: Vec::new(),
            options: super::super::profile::ProfileOptions::default(),
            auto_connect: false,
        }
    }

    fn temp_audit() -> (Arc<Mutex<AuditLog>>, tempfile::TempDir) {
        let dir = tempfile::TempDir::new().unwrap();
        let audit = Arc::new(Mutex::new(
            AuditLog::open(&dir.path().join("audit.db")).unwrap(),
        ));
        (audit, dir)
    }

    /// Start a tunnel using the fake ssh binary via `start_with_binary`.
    async fn start_with_fake_ssh(
        manager: &TunnelManager,
        profile: TunnelProfile,
        ssh_path: PathBuf,
    ) -> Result<String, String> {
        manager.start_with_binary_for_test(profile, ssh_path).await
    }

    #[tokio::test]
    async fn create_and_list() {
        let (audit, _dir) = temp_audit();
        let manager = TunnelManager::new(audit);
        // Sleep-forever script so the tunnel stays alive for the assertion.
        let script = fake_ssh_script("exec sleep 3600");

        let id = start_with_fake_ssh(&manager, test_profile("t1"), script.path().to_path_buf())
            .await
            .unwrap();

        let list = manager.list();
        assert_eq!(list.len(), 1);
        assert_eq!(list[0].0, id);
        let events = manager.audit.lock().query_by_tunnel(&id, 20).unwrap();
        assert!(events.iter().any(|event| event.kind == EventKind::Started));

        // Waited, not fire-and-forget: confirms the fake-ssh child is
        // actually gone before the test ends, rather than leaking a
        // `sleep 3600` process (see supervisor.rs's GRACEFUL_SHUTDOWN_TIMEOUT
        // doc comment for why the fire-and-forget `stop()` can't guarantee
        // that within a `#[tokio::test]`'s short-lived runtime).
        manager.stop_and_wait(&id).await.unwrap();
    }

    #[tokio::test]
    async fn stop_removes_from_map() {
        let (audit, _dir) = temp_audit();
        let manager = TunnelManager::new(audit);
        let script = fake_ssh_script("exec sleep 3600");

        let id = start_with_fake_ssh(&manager, test_profile("t2"), script.path().to_path_buf())
            .await
            .unwrap();

        manager.stop_and_wait(&id).await.unwrap();

        assert!(manager.list().is_empty(), "map should be empty after stop");
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn stop_does_not_leave_fake_ssh_sleep_running() {
        use crate::test_support::{assert_fake_ssh_stopped, fake_ssh_processes, test_temp_root};

        let (audit, _dir) = temp_audit();
        let manager = TunnelManager::new(audit);
        let marker = test_temp_root().join("manager_stop_ssh.pid");
        let _ = std::fs::remove_file(&marker);
        let script = crate::test_support::fake_ssh_script(
            "manager_stop_sleep",
            &format!("echo $$ > '{}'; exec sleep 3600", marker.display()),
            "exit /b 0",
        );

        let id = start_with_fake_ssh(&manager, test_profile("stop-child"), script)
            .await
            .unwrap();
        tokio::time::timeout(std::time::Duration::from_secs(30), async {
            // `echo $$ > marker` truncates the file before writing the PID, so
            // polling on existence alone can read the file mid-truncate.
            while std::fs::read_to_string(&marker)
                .unwrap_or_default()
                .is_empty()
            {
                tokio::time::sleep(std::time::Duration::from_millis(20)).await;
            }
        })
        .await
        .expect("fake SSH started");
        let pids = fake_ssh_processes(&marker);

        manager.stop(&id).unwrap();
        let _ = tokio::time::timeout(std::time::Duration::from_secs(6), async {
            while pids.iter().any(|pid| unsafe { libc::kill(*pid, 0) == 0 }) {
                tokio::time::sleep(std::time::Duration::from_millis(50)).await;
            }
        })
        .await;
        assert_fake_ssh_stopped(pids);
    }

    #[tokio::test]
    async fn get_status_returns_correct_status() {
        let (audit, _dir) = temp_audit();
        let manager = TunnelManager::new(audit);
        let script = fake_ssh_script("exec sleep 3600");

        let id = start_with_fake_ssh(&manager, test_profile("t3"), script.path().to_path_buf())
            .await
            .unwrap();

        let status = manager.get_status(&id);
        assert!(
            status.is_some(),
            "status should be Some for a running tunnel"
        );
        match status.unwrap() {
            TunnelStatus::Starting | TunnelStatus::Connected => {} // either is valid right after start
            other => panic!("unexpected status: {other:?}"),
        }

        manager.stop_and_wait(&id).await.unwrap();
    }

    #[tokio::test]
    async fn shutdown_all_clears_everything() {
        let (audit, _dir) = temp_audit();
        let manager = TunnelManager::new(audit);
        let script = fake_ssh_script("exec sleep 3600");

        for i in 0..3 {
            start_with_fake_ssh(
                &manager,
                test_profile(&format!("t{i}")),
                script.path().to_path_buf(),
            )
            .await
            .unwrap();
        }

        assert_eq!(manager.list().len(), 3);

        manager.shutdown_all_and_wait().await;

        assert!(
            manager.list().is_empty(),
            "all tunnels should be removed after shutdown_all"
        );
    }

    #[tokio::test]
    async fn start_rejects_duplicate_id_without_orphaning() {
        let (audit, _dir) = temp_audit();
        let manager = TunnelManager::new(audit);
        let script = fake_ssh_script("exec sleep 3600");

        // Seed a live handle for id `dup` via the fake-ssh helper.
        let profile = test_profile("dup");
        let id = profile.id.clone();
        start_with_fake_ssh(&manager, profile.clone(), script.path().to_path_buf())
            .await
            .unwrap();

        // Identity of the original handle — must survive a duplicate start.
        let original_ptr = {
            let entry = manager.tunnels.get(&id).unwrap();
            match entry.value() {
                TunnelSlot::Running(handle) => Arc::as_ptr(handle),
                TunnelSlot::Starting(_) => panic!("started tunnel has no handle"),
            }
        };

        // Second start with the same id must be rejected by the collision guard
        // BEFORE it spawns a supervisor/ssh child, so nothing is orphaned and the
        // original handle stays in place (no DashMap::insert overwrite).
        let err = manager.start(profile).await.unwrap_err();
        assert!(err.contains("already running"), "unexpected error: {err}");

        assert_eq!(
            manager.list().len(),
            1,
            "duplicate start must not add a second entry"
        );

        let still_ptr = {
            let entry = manager.tunnels.get(&id).unwrap();
            match entry.value() {
                TunnelSlot::Running(handle) => Arc::as_ptr(handle),
                TunnelSlot::Starting(_) => panic!("started tunnel has no handle"),
            }
        };
        assert_eq!(
            original_ptr, still_ptr,
            "original handle must be untouched (not overwritten/orphaned)"
        );

        manager.stop_and_wait(&id).await.unwrap();
    }

    /// The TOCTOU: `start` used to check `contains_key`, then `.await` the
    /// supervisor spawn, then `insert`. During that await the map held NOTHING
    /// for the id, so a second concurrent start saw it free, spawned its own
    /// supervisor, and its insert overwrote the first — orphaning a supervisor
    /// loop and an ssh child with no Drop to clean them up.
    ///
    /// The id is now claimed before the first await. A reservation is exactly
    /// what the map looks like during that window, so a second start hitting it
    /// must be rejected — and it is rejected before spawning anything, which is
    /// why this test can run without a real ssh binary.
    #[tokio::test]
    async fn a_start_is_rejected_while_another_start_holds_the_id() {
        let (audit, _dir) = temp_audit();
        let manager = TunnelManager::new(audit);
        let profile = test_profile("in-flight");
        let id = profile.id.clone();

        // Exactly the state an in-flight start() leaves behind at its await point.
        manager.tunnels.insert(id.clone(), TunnelSlot::Starting(1));

        // Rejected BEFORE the supervisor spawn, which is what lets this test run
        // without a real ssh binary — and is the point: nothing gets orphaned.
        let err = manager.start(profile).await.unwrap_err();
        assert!(err.contains("already running"), "unexpected error: {err}");
        assert_eq!(manager.tunnels.len(), 1, "no second entry may be created");
        assert!(
            matches!(
                manager.tunnels.get(&id).as_deref(),
                Some(TunnelSlot::Starting(1))
            ),
            "the in-flight reservation must be left alone"
        );
    }

    #[tokio::test]
    async fn cancelling_start_removes_only_its_reservation() {
        let (audit, _dir) = temp_audit();
        let manager = Arc::new(TunnelManager::new(audit));
        let profile = test_profile("cancelled");
        let id = profile.id.clone();
        let task_manager = Arc::clone(&manager);
        let task = tokio::spawn(async move {
            task_manager
                .start_with(profile, |_, _| async {
                    std::future::pending::<TunnelSupervisor>().await
                })
                .await
        });
        while !matches!(manager.get_status(&id), Some(TunnelStatus::Starting)) {
            tokio::task::yield_now().await;
        }

        manager.stop(&id).unwrap();
        manager
            .tunnels
            .insert(id.clone(), TunnelSlot::Starting(999));
        task.abort();
        let _ = task.await;
        assert!(matches!(
            manager.tunnels.get(&id).as_deref(),
            Some(TunnelSlot::Starting(999))
        ));
    }

    #[tokio::test]
    async fn stop_during_spawn_prevents_publication_and_started_audit() {
        let (audit, _dir) = temp_audit();
        let manager = Arc::new(TunnelManager::new(audit));
        let script = fake_ssh_script("exec sleep 3600");
        let ssh_path = script.path().to_path_buf();
        let profile = test_profile("stopped-during-spawn");
        let id = profile.id.clone();
        let (release_tx, release_rx) = tokio::sync::oneshot::channel();
        let task_manager = Arc::clone(&manager);
        let task = tokio::spawn(async move {
            task_manager
                .start_with(profile, move |profile, callback| async move {
                    let _ = release_rx.await;
                    TunnelSupervisor::start_with_binary(profile, ssh_path, callback).await
                })
                .await
        });
        while !matches!(manager.get_status(&id), Some(TunnelStatus::Starting)) {
            tokio::task::yield_now().await;
        }

        manager.stop(&id).unwrap();
        let _ = release_tx.send(());
        let error = task
            .await
            .unwrap()
            .expect_err("stopped start must not publish");
        assert!(error.contains("stopped while starting"), "{error}");
        assert!(manager.get_status(&id).is_none());
        let events = manager.audit.lock().query_by_tunnel(&id, 20).unwrap();
        assert!(!events.iter().any(|event| event.kind == EventKind::Started));
    }

    /// Catches: `shutdown_all_and_wait` returning while a start is still
    /// spawning. The reservation is removed, the batch finds no task to await,
    /// and the app could exit before the start discards the ssh it spawns.
    #[cfg(unix)]
    #[tokio::test]
    async fn shutdown_waits_for_a_start_still_spawning_and_its_ssh_is_gone() {
        let (audit, _dir) = temp_audit();
        let manager = Arc::new(TunnelManager::new(audit));
        let marker = crate::test_support::test_temp_root().join("shutdown_in_flight_ssh.pid");
        let _ = std::fs::remove_file(&marker);
        let script = fake_ssh_script(&format!(
            "echo $$ > '{}'\nexec sleep 3600",
            marker.display()
        ));
        let ssh_path = script.path().to_path_buf();
        let profile = test_profile("shutdown-while-starting");
        let id = profile.id.clone();
        let (release_tx, release_rx) = tokio::sync::oneshot::channel();
        let task_manager = Arc::clone(&manager);
        let start = tokio::spawn(async move {
            task_manager
                .start_with(profile, move |profile, callback| async move {
                    let _ = release_rx.await;
                    TunnelSupervisor::start_with_binary(profile, ssh_path, callback).await
                })
                .await
        });
        while !matches!(manager.get_status(&id), Some(TunnelStatus::Starting)) {
            tokio::task::yield_now().await;
        }

        let shutdown_manager = Arc::clone(&manager);
        let shutdown = tokio::spawn(async move { shutdown_manager.shutdown_all_and_wait().await });
        // The reservation is gone, but the batch must still be waiting.
        while manager.get_status(&id).is_some() {
            tokio::task::yield_now().await;
        }
        tokio::time::sleep(std::time::Duration::from_millis(200)).await;
        assert!(
            !shutdown.is_finished(),
            "shutdown returned while a start was still spawning"
        );

        let _ = release_tx.send(());
        shutdown.await.unwrap();
        // Confirmed, not eventual: the start has already returned its error.
        assert!(
            start.is_finished(),
            "start must have finished before shutdown returned"
        );
        let _ = start
            .await
            .unwrap()
            .expect_err("stopped start must not publish");
        let ssh_pid: i32 = std::fs::read_to_string(&marker)
            .map(|text| text.trim().parse().unwrap())
            .unwrap_or(0);
        if ssh_pid > 0 {
            let alive = unsafe { libc::kill(ssh_pid, 0) } == 0;
            if alive {
                unsafe { libc::kill(ssh_pid, libc::SIGKILL) };
            }
            assert!(!alive, "ssh {ssh_pid} survived shutdown");
        }
    }

    /// Catches: a start that claims its id AFTER `shutdown_all_and_wait` took its
    /// snapshot. Nothing removes it and the counter is already zero, so the
    /// tunnel would publish as Running and outlive the app exit.
    #[tokio::test]
    async fn a_start_after_shutdown_is_refused_before_spawning_anything() {
        let (audit, _dir) = temp_audit();
        let manager = TunnelManager::new(audit);
        manager.shutdown_all_and_wait().await;

        let spawned = Arc::new(std::sync::atomic::AtomicBool::new(false));
        let spawned_in_closure = Arc::clone(&spawned);
        let profile = test_profile("after-shutdown");
        let id = profile.id.clone();
        let error = manager
            .start_with(profile, move |_, _| async move {
                spawned_in_closure.store(true, Ordering::SeqCst);
                std::future::pending::<TunnelSupervisor>().await
            })
            .await
            .expect_err("start after shutdown must be refused");
        assert!(error.contains("shutting down"), "{error}");
        assert!(manager.get_status(&id).is_none(), "no slot may be left");
        assert!(!spawned.load(Ordering::SeqCst), "nothing may be spawned");
    }

    /// Catches: the lost-publication cleanup being owned by the (abortable)
    /// `start_with` future. Aborting the caller — the remote-runtime supervisor's
    /// `dispose` does — dropped the wait, the in-flight counter hit zero, and
    /// `shutdown_all_and_wait` returned while ssh was still being killed. ssh
    /// ignores SIGTERM here, so it only dies after the 5s SIGKILL escalation.
    #[cfg(unix)]
    #[tokio::test]
    async fn aborting_a_start_mid_cleanup_does_not_let_shutdown_return_early() {
        let (audit, _dir) = temp_audit();
        let manager = Arc::new(TunnelManager::new(audit));
        let marker = crate::test_support::test_temp_root().join("abort_mid_cleanup_ssh.pid");
        let _ = std::fs::remove_file(&marker);
        let script = fake_ssh_script(&format!(
            "echo $$ > '{}'\ntrap '' TERM\nexec sleep 3600",
            marker.display()
        ));
        let ssh_path = script.path().to_path_buf();
        let marker_in_spawn = marker.clone();
        let profile = test_profile("abort-mid-cleanup");
        let id = profile.id.clone();
        let (release_tx, release_rx) = tokio::sync::oneshot::channel();
        let task_manager = Arc::clone(&manager);
        let start = tokio::spawn(async move {
            task_manager
                .start_with(profile, move |profile, callback| async move {
                    let _ = release_rx.await;
                    let supervisor =
                        TunnelSupervisor::start_with_binary(profile, ssh_path, callback).await;
                    // Hold the supervisor back until ssh is really up: stopping
                    // a supervisor before its ssh spawned would skip the slow
                    // kill this test needs the cleanup to sit in.
                    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
                    while !marker_in_spawn.exists() && std::time::Instant::now() < deadline {
                        tokio::time::sleep(std::time::Duration::from_millis(20)).await;
                    }
                    supervisor
                })
                .await
        });
        while !matches!(manager.get_status(&id), Some(TunnelStatus::Starting)) {
            tokio::task::yield_now().await;
        }
        let shutdown_manager = Arc::clone(&manager);
        let shutdown = tokio::spawn(async move { shutdown_manager.shutdown_all_and_wait().await });
        while manager.get_status(&id).is_some() {
            tokio::task::yield_now().await;
        }

        // The start spawns ssh, loses its publication race, and is parked in
        // cleanup waiting for ssh to die (it ignores SIGTERM).
        let _ = release_tx.send(());
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
        while !marker.exists() {
            assert!(std::time::Instant::now() < deadline, "ssh never started");
            tokio::time::sleep(std::time::Duration::from_millis(20)).await;
        }
        tokio::time::sleep(std::time::Duration::from_millis(300)).await;
        start.abort();
        let _ = start.await;

        tokio::time::sleep(std::time::Duration::from_millis(500)).await;
        assert!(
            !shutdown.is_finished(),
            "shutdown returned while ssh was still being cleaned up"
        );
        shutdown.await.unwrap();
        let ssh_pid: i32 = std::fs::read_to_string(&marker)
            .map(|text| text.trim().parse().unwrap())
            .unwrap_or(0);
        if ssh_pid > 0 {
            let alive = unsafe { libc::kill(ssh_pid, 0) } == 0;
            if alive {
                unsafe { libc::kill(ssh_pid, libc::SIGKILL) };
            }
            assert!(!alive, "ssh {ssh_pid} survived shutdown");
        }
    }

    #[tokio::test]
    async fn failed_spawn_releases_reservation_without_started_audit() {
        let (audit, _dir) = temp_audit();
        let manager = TunnelManager::new(audit);
        let mut profile = test_profile("invalid");
        profile.host = " ".to_string();
        let id = profile.id.clone();

        let error = manager.start(profile).await.expect_err("invalid profile");
        assert!(error.contains("host must not be empty"), "{error}");
        assert!(manager.get_status(&id).is_none());
        let events = manager.audit.lock().query_by_tunnel(&id, 20).unwrap();
        assert!(!events.iter().any(|event| event.kind == EventKind::Started));
    }

    /// A reserved slot has no supervisor to ask, and reporting it as absent would
    /// make a starting tunnel invisible in the UI.
    #[test]
    fn a_reserved_tunnel_reports_starting() {
        let (audit, _dir) = temp_audit();
        let manager = TunnelManager::new(audit);
        manager
            .tunnels
            .insert("in-flight".to_string(), TunnelSlot::Starting(1));

        assert!(matches!(
            manager.get_status("in-flight"),
            Some(TunnelStatus::Starting)
        ));
        let list = manager.list();
        assert_eq!(list.len(), 1);
        assert!(matches!(list[0].1, TunnelStatus::Starting));
    }

    /// Stopping a tunnel that is still starting must not panic on the missing
    /// supervisor, and must free the id so the user can start it again.
    #[test]
    fn stopping_a_reserved_tunnel_frees_the_id() {
        let (audit, _dir) = temp_audit();
        let manager = TunnelManager::new(audit);
        manager
            .tunnels
            .insert("in-flight".to_string(), TunnelSlot::Starting(1));

        manager.stop("in-flight").expect("stop a reserved tunnel");
        assert!(manager.tunnels.is_empty());

        manager
            .tunnels
            .insert("in-flight".to_string(), TunnelSlot::Starting(2));
        manager.stop_if_running("in-flight");
        assert!(manager.tunnels.is_empty());
    }

    #[tokio::test]
    async fn concurrent_starts_no_panic() {
        let (audit, _dir) = temp_audit();
        let manager = Arc::new(TunnelManager::new(audit));
        let script = fake_ssh_script("exec sleep 3600");
        let ssh_path = script.path().to_path_buf();

        // Spawn 10 tasks concurrently via tokio::spawn. Arc<Mutex<AuditLog>> and
        // Arc<TunnelManager> are Send+Sync so this is safe.
        let tasks: Vec<_> = (0..10_usize)
            .map(|i| {
                let manager = Arc::clone(&manager);
                let path = ssh_path.clone();
                tokio::spawn(async move {
                    start_with_fake_ssh(&manager, test_profile(&format!("concurrent-{i}")), path)
                        .await
                        .unwrap()
                })
            })
            .collect();

        let mut ids: Vec<String> = Vec::new();
        for t in tasks {
            ids.push(t.await.unwrap());
        }

        let list = manager.list();
        assert_eq!(list.len(), 10, "all 10 tunnels should be present");

        for id in &ids {
            assert!(
                list.iter().any(|(k, _)| k == id),
                "tunnel {id} missing from list"
            );
        }

        manager.shutdown_all_and_wait().await;
    }
}
