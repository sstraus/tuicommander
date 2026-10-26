//! Extension liveness for armed Design Mode. The browser extension beats every
//! few seconds; a mode whose browser went away is stopped, and each reply
//! carries the reload sequence an agent bumps to refresh the reviewed page.
use std::collections::HashMap;
use std::time::{Duration, Instant};

/// No beat for this long after the first one: the browser is gone.
pub(crate) const BEAT_TIMEOUT: Duration = Duration::from_secs(60);
/// How often the manager looks for modes whose browser went quiet.
pub(crate) const SWEEP_INTERVAL: Duration = Duration::from_secs(5);
/// First run asks the user to install the unpacked extension by hand.
pub(crate) const SETUP_TIMEOUT: Duration = Duration::from_secs(600);

struct Entry {
    session_id: String,
    armed_at: Instant,
    last_beat: Option<Instant>,
}

#[derive(Default)]
pub(crate) struct Liveness {
    entries: HashMap<String, Entry>,
    /// Kept across re-arms, so a new Start never looks like a reload request.
    reload_seq: HashMap<String, u64>,
    /// Sequence kept like `reload_seq`; the URL is dropped when the mode is armed or stopped,
    /// so a new Start never replays an old navigation.
    navigate: HashMap<String, (u64, Option<String>)>,
}

impl Liveness {
    pub(crate) fn armed(&mut self, repo: &str, session_id: &str, now: Instant) {
        self.drop_navigation(repo);
        self.entries.insert(
            repo.to_owned(),
            Entry {
                session_id: session_id.to_owned(),
                armed_at: now,
                last_beat: None,
            },
        );
    }

    pub(crate) fn stopped(&mut self, repo: &str) {
        self.entries.remove(repo);
        self.drop_navigation(repo);
    }

    fn drop_navigation(&mut self, repo: &str) {
        if let Some((_, url)) = self.navigate.get_mut(repo) {
            *url = None;
        }
    }

    /// Records a beat for an armed repository and returns its reload sequence.
    pub(crate) fn beat(&mut self, repo: &str, now: Instant) -> Option<u64> {
        let entry = self.entries.get_mut(repo)?;
        entry.last_beat = Some(now);
        Some(self.reload_seq.get(repo).copied().unwrap_or(0))
    }

    pub(crate) fn request_reload(&mut self, repo: &str) -> Option<u64> {
        if !self.entries.contains_key(repo) {
            return None;
        }
        let seq = self.reload_seq.entry(repo.to_owned()).or_default();
        *seq += 1;
        Some(*seq)
    }

    pub(crate) fn request_navigate(&mut self, repo: &str, url: String) -> Option<u64> {
        if !self.entries.contains_key(repo) {
            return None;
        }
        let entry = self.navigate.entry(repo.to_owned()).or_default();
        entry.0 += 1;
        entry.1 = Some(url);
        Some(entry.0)
    }

    /// The navigation the extension has not necessarily applied yet, for an armed repository.
    pub(crate) fn pending_navigate(&self, repo: &str) -> Option<(u64, Option<String>)> {
        self.entries.get(repo)?;
        Some(self.navigate.get(repo).cloned().unwrap_or_default())
    }

    /// Removes and returns `(repo, session)` for every mode whose browser went quiet.
    pub(crate) fn expire(&mut self, now: Instant) -> Vec<(String, String)> {
        let stale: Vec<String> = self
            .entries
            .iter()
            .filter(|(_, entry)| match entry.last_beat {
                Some(beat) => now.duration_since(beat) >= BEAT_TIMEOUT,
                None => now.duration_since(entry.armed_at) >= SETUP_TIMEOUT,
            })
            .map(|(repo, _)| repo.clone())
            .collect();
        stale
            .into_iter()
            .filter_map(|repo| {
                self.entries
                    .remove(&repo)
                    .map(|entry| (repo, entry.session_id))
            })
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // Catches: Design Mode staying armed after the browser is gone, or a slow
    // first-run install being cut off before the extension ever connected.
    #[test]
    fn armed_mode_without_heartbeat_expires() {
        let start = Instant::now();
        let mut live = Liveness::default();
        live.armed("/repo", "agent", start);
        assert_eq!(
            live.expire(start + SETUP_TIMEOUT - Duration::from_secs(1)),
            [] as [(std::string::String, std::string::String); 0]
        );
        assert_eq!(
            live.beat("/repo", start + Duration::from_secs(300)),
            Some(0)
        );
        let beat = start + Duration::from_secs(300);
        assert_eq!(
            live.expire(beat + BEAT_TIMEOUT - Duration::from_secs(1)),
            [] as [(std::string::String, std::string::String); 0]
        );
        assert_eq!(
            live.expire(beat + BEAT_TIMEOUT),
            vec![("/repo".to_owned(), "agent".to_owned())]
        );
        assert_eq!(
            live.beat("/repo", beat + BEAT_TIMEOUT),
            None,
            "expired mode is gone"
        );

        live.armed("/never", "agent", start);
        assert_eq!(live.expire(start + SETUP_TIMEOUT).len(), 1);
    }

    // Catches: a reload lost, repeated on every beat, or fired by a fresh Start.
    #[test]
    fn reload_request_reaches_the_extension_once() {
        let now = Instant::now();
        let mut live = Liveness::default();
        assert_eq!(
            live.request_reload("/repo"),
            None,
            "nothing armed to reload"
        );
        live.armed("/repo", "agent", now);
        assert_eq!(live.beat("/repo", now), Some(0));
        assert_eq!(live.request_reload("/repo"), Some(1));
        assert_eq!(live.beat("/repo", now), Some(1));
        assert_eq!(
            live.beat("/repo", now),
            Some(1),
            "the sequence moves only on request"
        );
        live.armed("/repo", "other-agent", now);
        assert_eq!(
            live.beat("/repo", now),
            Some(1),
            "re-arming is not a reload"
        );
        live.stopped("/repo");
        assert_eq!(live.beat("/repo", now), None);
    }

    // Catches: a navigation lost, replayed after a re-arm or Stop, or accepted with nothing armed.
    #[test]
    fn navigation_is_pending_only_while_armed_and_never_replayed() {
        let now = Instant::now();
        let mut live = Liveness::default();
        assert_eq!(
            live.request_navigate("/repo", "http://localhost:1/".into()),
            None
        );
        live.armed("/repo", "agent", now);
        assert_eq!(live.pending_navigate("/repo"), Some((0, None)));
        assert_eq!(
            live.request_navigate("/repo", "http://localhost:1/a".into()),
            Some(1)
        );
        assert_eq!(
            live.request_navigate("/repo", "http://localhost:1/b".into()),
            Some(2)
        );
        assert_eq!(
            live.pending_navigate("/repo"),
            Some((2, Some("http://localhost:1/b".into())))
        );
        live.armed("/repo", "other-agent", now);
        assert_eq!(
            live.pending_navigate("/repo"),
            Some((2, None)),
            "re-arming is not a navigation"
        );
        assert_eq!(
            live.request_navigate("/repo", "http://localhost:1/c".into()),
            Some(3)
        );
        live.stopped("/repo");
        assert_eq!(live.pending_navigate("/repo"), None);
    }
}
