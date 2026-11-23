use crate::github::BranchPrStatus;
use serde::Serialize;

// Transition detection
// ---------------------------------------------------------------------------

#[derive(Clone, Debug, Serialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum PrTransition {
    Merged {
        repo_path: String,
        branch: String,
        pr_number: i32,
        title: String,
    },
    Closed {
        repo_path: String,
        branch: String,
        pr_number: i32,
        title: String,
    },
    Blocked {
        repo_path: String,
        branch: String,
        pr_number: i32,
        title: String,
    },
    CiFailed {
        repo_path: String,
        branch: String,
        pr_number: i32,
        title: String,
    },
    CiRecovered {
        repo_path: String,
        branch: String,
        pr_number: i32,
        title: String,
    },
    ChangesRequested {
        repo_path: String,
        branch: String,
        pr_number: i32,
        title: String,
    },
    Ready {
        repo_path: String,
        branch: String,
        pr_number: i32,
        title: String,
    },
    Pushed {
        repo_path: String,
        branch: String,
        pr_number: i32,
        title: String,
        head_ref_oid: String,
        /// PR author login — used by the watcher's authored_by_others filter.
        author: String,
    },
    /// A brand-new PR appeared on an open branch. Detected in `process_repo_update`
    /// (no prior state to diff), not in `detect_transitions`. Carries `author` for
    /// the watcher's authored_by_others filter and `head_ref_oid` for worktree review.
    Opened {
        repo_path: String,
        branch: String,
        pr_number: i32,
        title: String,
        head_ref_oid: String,
        author: String,
    },
}

impl PrTransition {
    /// The repository the transition was detected in.
    #[must_use]
    pub fn repo_path(&self) -> &str {
        match self {
            Self::Merged { repo_path, .. }
            | Self::Closed { repo_path, .. }
            | Self::Blocked { repo_path, .. }
            | Self::CiFailed { repo_path, .. }
            | Self::CiRecovered { repo_path, .. }
            | Self::ChangesRequested { repo_path, .. }
            | Self::Ready { repo_path, .. }
            | Self::Pushed { repo_path, .. }
            | Self::Opened { repo_path, .. } => repo_path,
        }
    }
}

/// The "ready" verdict of the shared readiness function (`src/utils/prReadiness.ts`): the OS
/// notice must never say "ready to merge" while the badge says Comments or CI Running. Keep the
/// two in step.
fn is_ready(pr: &BranchPrStatus) -> bool {
    !pr.is_draft
        && pr.conflict_state == crate::github::ConflictState::Clear
        && pr.mergeable == "MERGEABLE"
        && pr.review_decision == "APPROVED"
        && pr.checks.failed == 0
        && pr.checks.pending == 0
        && pr.unresolved_threads == 0
        && !pr.unresolved_threads_truncated
}

pub fn detect_transitions(
    repo_path: &str,
    old: &BranchPrStatus,
    new: &BranchPrStatus,
) -> Vec<PrTransition> {
    let mut out = Vec::new();
    let old_state = old.state.to_uppercase();
    let new_state = new.state.to_uppercase();

    let rp = repo_path.to_string();
    let branch = new.branch.clone();
    let pr_number = new.number;
    let title = new.title.clone();

    let mut primary_type: Option<&str> = None;

    // A push replaces the head SHA. Its failing checks are a *fresh* failure even
    // when the previous head was also failing — so `ci_failed` must re-fire on a
    // new commit, not only on the failed==0 → failed>0 edge. Otherwise an agent's
    // fix-push whose CI fails again before the poller observes an intermediate
    // all-pending (failed==0) poll never re-triggers auto-heal, stalling the loop.
    let new_commit = old.head_ref_oid != new.head_ref_oid && !new.head_ref_oid.is_empty();

    // Terminal transitions
    if old_state != "MERGED" && new_state == "MERGED" {
        primary_type = Some("merged");
        out.push(PrTransition::Merged {
            repo_path: rp.clone(),
            branch: branch.clone(),
            pr_number,
            title: title.clone(),
        });
    } else if old_state != "CLOSED" && new_state == "CLOSED" {
        primary_type = Some("closed");
        out.push(PrTransition::Closed {
            repo_path: rp.clone(),
            branch: branch.clone(),
            pr_number,
            title: title.clone(),
        });
    }
    // Actionable transitions (only for OPEN PRs)
    else if new_state == "OPEN" {
        // Same rule as the badge and the popover (#8537): a notification is an
        // accusation too, and GitHub's last known `mergeable` is not evidence
        // while it recomputes.
        use crate::github::ConflictState;
        if old.conflict_state != ConflictState::Conflicting
            && new.conflict_state == ConflictState::Conflicting
        {
            primary_type = Some("blocked");
            out.push(PrTransition::Blocked {
                repo_path: rp.clone(),
                branch: branch.clone(),
                pr_number,
                title: title.clone(),
            });
        } else if new.checks.failed > 0 && (old.checks.failed == 0 || new_commit) {
            primary_type = Some("ci_failed");
            out.push(PrTransition::CiFailed {
                repo_path: rp.clone(),
                branch: branch.clone(),
                pr_number,
                title: title.clone(),
            });
        } else if old.review_decision != "CHANGES_REQUESTED"
            && new.review_decision == "CHANGES_REQUESTED"
        {
            primary_type = Some("changes_requested");
            out.push(PrTransition::ChangesRequested {
                repo_path: rp.clone(),
                branch: branch.clone(),
                pr_number,
                title: title.clone(),
            });
        } else if !is_ready(old) && is_ready(new) {
            primary_type = Some("ready");
            out.push(PrTransition::Ready {
                repo_path: rp.clone(),
                branch: branch.clone(),
                pr_number,
                title: title.clone(),
            });
        }
    }

    // New commit pushed to an open PR: head_ref_oid changed. Independent signal
    // (a push can coincide with ci_failed etc.), carries the new oid for dedup.
    if new_state == "OPEN" && new_commit {
        out.push(PrTransition::Pushed {
            repo_path: rp.clone(),
            branch: branch.clone(),
            pr_number,
            title: title.clone(),
            head_ref_oid: new.head_ref_oid.clone(),
            author: new.author.clone(),
        });
    }

    // CI recovery: failed → all passing, suppressed when "ready" already fired
    if primary_type != Some("ready") && new_state == "OPEN" {
        let old_failed = old.checks.failed;
        let new_failed = new.checks.failed;
        let new_pending = new.checks.pending;
        if old_failed > 0 && new_failed == 0 && new_pending == 0 {
            out.push(PrTransition::CiRecovered {
                repo_path: rp,
                branch,
                pr_number,
                title,
            });
        }
    }

    out
}

// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::github::CheckSummary;

    fn make_pr(
        state: &str,
        mergeable: &str,
        review: &str,
        failed: u32,
        pending: u32,
    ) -> BranchPrStatus {
        let merge_state_status = if mergeable == "CONFLICTING" {
            "DIRTY"
        } else {
            "CLEAN"
        };
        BranchPrStatus {
            branch: "feat/test".to_string(),
            number: 42,
            title: "Test PR".to_string(),
            state: state.to_string(),
            url: String::new(),
            additions: 0,
            deletions: 0,
            checks: CheckSummary {
                passed: 0,
                failed,
                pending,
                total: failed + pending,
            },
            author: String::new(),
            commits: 1,
            mergeable: mergeable.to_string(),
            // Realistic pairing: GitHub reports DIRTY alongside a computed
            // CONFLICTING. An empty status would mean "still recomputing", which
            // is a different case entirely.
            merge_state_status: merge_state_status.to_string(),
            conflict_state: crate::github::classify_conflict_state(
                Some(mergeable),
                Some(merge_state_status),
            ),
            review_decision: review.to_string(),
            viewer_did_approve: false,
            labels: vec![],
            is_draft: false,
            base_ref_name: "main".to_string(),
            head_ref_oid: String::new(),
            created_at: String::new(),
            updated_at: String::new(),
            merge_state_label: None,
            review_state_label: None,
            merge_commit_allowed: true,
            squash_merge_allowed: true,
            rebase_merge_allowed: true,
            unresolved_threads: 0,
            unresolved_threads_truncated: false,
        }
    }

    #[test]
    fn transition_merged() {
        let old = make_pr("OPEN", "MERGEABLE", "APPROVED", 0, 0);
        let new = make_pr("MERGED", "MERGEABLE", "APPROVED", 0, 0);
        let t = detect_transitions("/repo", &old, &new);
        assert_eq!(t.len(), 1);
        assert!(matches!(&t[0], PrTransition::Merged { .. }));
    }

    #[test]
    fn transition_closed() {
        let old = make_pr("OPEN", "UNKNOWN", "", 0, 0);
        let new = make_pr("CLOSED", "UNKNOWN", "", 0, 0);
        let t = detect_transitions("/repo", &old, &new);
        assert_eq!(t.len(), 1);
        assert!(matches!(&t[0], PrTransition::Closed { .. }));
    }

    #[test]
    fn transition_blocked() {
        let old = make_pr("OPEN", "MERGEABLE", "", 0, 0);
        let new = make_pr("OPEN", "CONFLICTING", "", 0, 0);
        let t = detect_transitions("/repo", &old, &new);
        assert_eq!(t.len(), 1);
        assert!(matches!(&t[0], PrTransition::Blocked { .. }));
    }

    #[test]
    fn transition_ci_failed() {
        let old = make_pr("OPEN", "MERGEABLE", "", 0, 0);
        let new = make_pr("OPEN", "MERGEABLE", "", 2, 0);
        let t = detect_transitions("/repo", &old, &new);
        assert_eq!(t.len(), 1);
        assert!(matches!(&t[0], PrTransition::CiFailed { .. }));
    }

    #[test]
    fn transition_ci_failed_on_new_commit_while_still_failing() {
        // Regression: an agent's fix-push whose CI fails again before the poller
        // sees an intermediate all-pending (failed==0) poll. old and new are both
        // failing, but the head SHA changed — ci_failed must re-fire so auto-heal
        // continues past the first attempt instead of stalling.
        let mut old = make_pr("OPEN", "MERGEABLE", "", 2, 0);
        old.head_ref_oid = "aaaa".to_string();
        let mut new = make_pr("OPEN", "MERGEABLE", "", 2, 0);
        new.head_ref_oid = "bbbb".to_string();
        let t = detect_transitions("/repo", &old, &new);
        // Both ci_failed (fresh failure on the new head) and pushed (oid changed).
        assert!(t.iter().any(|x| matches!(x, PrTransition::CiFailed { .. })));
        assert!(t.iter().any(|x| matches!(x, PrTransition::Pushed { .. })));
    }

    #[test]
    fn no_ci_failed_when_still_failing_same_commit() {
        // Same head SHA, still failing: no new ci_failed (would re-heal endlessly
        // without a real change to react to).
        let mut old = make_pr("OPEN", "MERGEABLE", "", 2, 0);
        old.head_ref_oid = "aaaa".to_string();
        let mut new = make_pr("OPEN", "MERGEABLE", "", 2, 0);
        new.head_ref_oid = "aaaa".to_string();
        let t = detect_transitions("/repo", &old, &new);
        assert!(!t.iter().any(|x| matches!(x, PrTransition::CiFailed { .. })));
    }

    #[test]
    fn transition_changes_requested() {
        let old = make_pr("OPEN", "MERGEABLE", "", 0, 0);
        let new = make_pr("OPEN", "MERGEABLE", "CHANGES_REQUESTED", 0, 0);
        let t = detect_transitions("/repo", &old, &new);
        assert_eq!(t.len(), 1);
        assert!(matches!(&t[0], PrTransition::ChangesRequested { .. }));
    }

    #[test]
    fn transition_ready() {
        let old = make_pr("OPEN", "UNKNOWN", "", 1, 0);
        let new = make_pr("OPEN", "MERGEABLE", "APPROVED", 0, 0);
        let t = detect_transitions("/repo", &old, &new);
        assert_eq!(t.len(), 1);
        assert!(matches!(&t[0], PrTransition::Ready { .. }));
    }

    #[test]
    fn transition_ci_recovered() {
        let old = make_pr("OPEN", "UNKNOWN", "", 3, 0);
        let new = make_pr("OPEN", "UNKNOWN", "", 0, 0);
        let t = detect_transitions("/repo", &old, &new);
        assert_eq!(t.len(), 1);
        assert!(matches!(&t[0], PrTransition::CiRecovered { .. }));
    }

    #[test]
    fn ci_recovered_suppressed_when_ready() {
        // Old: failing + not ready. New: ready (all green). Only "ready" fires, not ci_recovered.
        let old = make_pr("OPEN", "UNKNOWN", "", 2, 0);
        let new = make_pr("OPEN", "MERGEABLE", "APPROVED", 0, 0);
        let t = detect_transitions("/repo", &old, &new);
        assert_eq!(t.len(), 1);
        assert!(matches!(&t[0], PrTransition::Ready { .. }));
    }

    #[test]
    fn transition_pushed() {
        // New commit on an open PR: head_ref_oid changed → exactly one Pushed.
        let mut old = make_pr("OPEN", "MERGEABLE", "", 0, 0);
        old.head_ref_oid = "aaa111".to_string();
        let mut new = make_pr("OPEN", "MERGEABLE", "", 0, 0);
        new.head_ref_oid = "bbb222".to_string();
        new.author = "octocat".to_string();
        let t = detect_transitions("/repo", &old, &new);
        assert_eq!(t.len(), 1);
        assert!(matches!(
            &t[0],
            PrTransition::Pushed { head_ref_oid, author, .. }
                if head_ref_oid == "bbb222" && author == "octocat"
        ));
    }

    #[test]
    fn pushed_same_oid_none() {
        // Unchanged head_ref_oid → no Pushed.
        let mut old = make_pr("OPEN", "MERGEABLE", "", 0, 0);
        old.head_ref_oid = "aaa111".to_string();
        let mut new = make_pr("OPEN", "MERGEABLE", "", 0, 0);
        new.head_ref_oid = "aaa111".to_string();
        let t = detect_transitions("/repo", &old, &new);
        assert!(!t.iter().any(|x| matches!(x, PrTransition::Pushed { .. })));
    }

    #[test]
    fn pushed_non_open_none() {
        // oid changed but PR is no longer OPEN → no Pushed (Closed fires instead).
        let mut old = make_pr("OPEN", "MERGEABLE", "", 0, 0);
        old.head_ref_oid = "aaa111".to_string();
        let mut new = make_pr("CLOSED", "MERGEABLE", "", 0, 0);
        new.head_ref_oid = "bbb222".to_string();
        let t = detect_transitions("/repo", &old, &new);
        assert!(!t.iter().any(|x| matches!(x, PrTransition::Pushed { .. })));
    }

    #[test]
    fn no_transition_on_unchanged() {
        let pr = make_pr("OPEN", "MERGEABLE", "APPROVED", 0, 0);
        let t = detect_transitions("/repo", &pr, &pr);
        assert!(t.is_empty());
    }

    #[test]
    fn no_transition_pending_ci() {
        // CI recovered from failed, but still has pending — no ci_recovered yet
        let old = make_pr("OPEN", "UNKNOWN", "", 2, 0);
        let new = make_pr("OPEN", "UNKNOWN", "", 0, 3);
        let t = detect_transitions("/repo", &old, &new);
        assert!(t.is_empty());
    }

    /// Catches: the OS notification "PR ready to merge" firing for a PR the badge and
    /// panel show as "Comments" (unresolved review threads) or "CI Running" — two
    /// readiness verdicts for the same PR (story 1347: one shared verdict).
    #[test]
    fn ready_transition_waits_for_unresolved_threads_and_pending_checks() {
        let old = make_pr("OPEN", "UNKNOWN", "", 1, 0);

        let mut open_threads = make_pr("OPEN", "MERGEABLE", "APPROVED", 0, 0);
        open_threads.unresolved_threads = 2;
        let t = detect_transitions("/repo", &old, &open_threads);
        assert!(
            !t.iter().any(|x| matches!(x, PrTransition::Ready { .. })),
            "unresolved threads must not notify ready: {t:?}"
        );

        let mut truncated = make_pr("OPEN", "MERGEABLE", "APPROVED", 0, 0);
        truncated.unresolved_threads_truncated = true;
        let t = detect_transitions("/repo", &old, &truncated);
        assert!(
            !t.iter().any(|x| matches!(x, PrTransition::Ready { .. })),
            "a truncated thread page must not notify ready: {t:?}"
        );

        let pending = make_pr("OPEN", "MERGEABLE", "APPROVED", 0, 3);
        let t = detect_transitions("/repo", &old, &pending);
        assert!(
            !t.iter().any(|x| matches!(x, PrTransition::Ready { .. })),
            "pending checks must not notify ready: {t:?}"
        );
    }
}

#[cfg(test)]
mod critic3_tests {
    use super::*;
    use crate::github::ConflictState;

    fn base(unresolved: u32) -> BranchPrStatus {
        // MERGEABLE + CLEAN + APPROVED, no checks: ready unless a blocker is set below.
        let open: Vec<serde_json::Value> = (0..unresolved)
            .map(|_| serde_json::json!({"isResolved": false}))
            .collect();
        crate::github::parse_pr_node(&serde_json::json!({
            "number": 42, "headRefName": "feat/test", "state": "OPEN",
            "mergeable": "MERGEABLE", "mergeStateStatus": "CLEAN",
            "reviewDecision": "APPROVED",
            "reviewThreads": {"nodes": open}
        }))
        .expect("PR node parses")
    }

    fn fires_ready(old: &BranchPrStatus, new: &BranchPrStatus) -> bool {
        detect_transitions("/repo", old, new)
            .iter()
            .any(|x| matches!(x, PrTransition::Ready { .. }))
    }

    /// Catches: the last blocker (open threads) being resolved while everything else is
    /// green never notifying, because ready is only evaluated on review/check edges.
    #[test]
    fn resolving_the_last_thread_fires_ready() {
        assert!(fires_ready(&base(2), &base(0)));
    }

    /// Catches: is_ready ignoring the draft flag or a recomputing conflict state, so a draft
    /// or a "checking" PR is announced as ready to merge while the badge says Draft/Checking.
    #[test]
    fn draft_or_checking_pr_never_fires_ready() {
        let old = base(1);
        let mut draft = base(0);
        draft.is_draft = true;
        assert!(!fires_ready(&old, &draft), "draft");
        let mut checking = base(0);
        checking.conflict_state = ConflictState::Checking;
        assert!(!fires_ready(&old, &checking), "checking");
    }
}
