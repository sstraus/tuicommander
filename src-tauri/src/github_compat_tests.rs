//! Step 0 — characterization tests pinning the CURRENT github.com behavior.
//!
//! These tests form the zero-change safety net for the multi-account refactor
//! (#61 + #62). They assert the exact behavior a github.com-only user sees
//! TODAY and MUST stay green through every later step. If a refactor changes
//! one of these outputs, it has changed observable github.com behavior — which
//! the hard constraint forbids.
//!
//! As later steps re-shape signatures (e.g. Step 7 adds an account to the query
//! builder), only the *call adapters* below may change — the asserted *outputs*
//! must not.

#![cfg(test)]

use crate::github;
use crate::github_auth::{self, TokenSource};
use crate::state::tests_support::make_test_app_state;

// ---------------------------------------------------------------------------
// (a) build_unified_batch_query — query shape for 2 repos + viewer search
// ---------------------------------------------------------------------------

fn two_repos() -> Vec<(String, String, String)> {
    vec![
        ("/path/zero".to_string(), "octocat".to_string(), "hello".to_string()),
        ("/path/one".to_string(), "octocat".to_string(), "world".to_string()),
    ]
}

#[test]
fn batch_query_shape_two_repos_disabled_issues() {
    let repos = two_repos();
    let (query, aliases) =
        github::build_unified_batch_query(&repos, false, "disabled", "octocat", false);

    // Top-level operation name is stable.
    assert!(query.starts_with("query BatchPoll {"), "query was: {query}");

    // One aliased repository selection per repo, in order, with owner/name verbatim.
    assert!(query.contains("r0: repository(owner: \"octocat\", name: \"hello\")"));
    assert!(query.contains("r1: repository(owner: \"octocat\", name: \"world\")"));

    // Default (no merged, no hidden drafts) PR page size + states.
    assert!(query.contains("pullRequests(first: 20, states: [OPEN], orderBy: {field: UPDATED_AT, direction: DESC})"));

    // Merge-policy fields are always requested.
    assert!(query.contains("mergeCommitAllowed"));
    assert!(query.contains("squashMergeAllowed"));
    assert!(query.contains("rebaseMergeAllowed"));

    // Supplemental viewer-PR search across exactly the queried repos.
    assert!(query.contains(
        "viewerPrs: search(query: \"is:pr is:open author:octocat repo:octocat/hello repo:octocat/world\", type: ISSUE, first: 30)"
    ));

    // Rate-limit probe is always present.
    assert!(query.contains("rateLimit { cost remaining resetAt }"));

    // filter_mode "disabled" omits the issues section entirely.
    assert!(!query.contains("issues("), "issues section must be absent when disabled");

    // Alias → path mapping is positional.
    assert_eq!(
        aliases,
        vec![
            ("r0".to_string(), "/path/zero".to_string()),
            ("r1".to_string(), "/path/one".to_string()),
        ]
    );
}

#[test]
fn batch_query_include_merged_widens_states() {
    let repos = two_repos();
    let (query, _) =
        github::build_unified_batch_query(&repos, true, "disabled", "octocat", false);
    assert!(query.contains("states: [OPEN, MERGED]"));
}

#[test]
fn batch_query_hide_drafts_bumps_page_size_and_filters_search() {
    let repos = two_repos();
    let (query, _) =
        github::build_unified_batch_query(&repos, false, "disabled", "octocat", true);
    // Fetch more PRs so draft-filtering still leaves enough valid ones.
    assert!(query.contains("pullRequests(first: 40, states: [OPEN]"));
    // Search excludes drafts.
    assert!(query.contains("-is:draft"));
}

#[test]
fn batch_query_assigned_filter_includes_issues_section() {
    let repos = two_repos();
    let (query, _) =
        github::build_unified_batch_query(&repos, false, "assigned", "octocat", false);
    // Issues section present and scoped to the viewer as assignee.
    assert!(query.contains("issues("));
    assert!(query.contains("filterBy: { assignee: \"octocat\" }"));
}

#[test]
fn batch_query_empty_viewer_omits_search() {
    let repos = two_repos();
    let (query, _) = github::build_unified_batch_query(&repos, false, "disabled", "", false);
    // No viewer → no supplemental search clause.
    assert!(!query.contains("viewerPrs:"));
}

// ---------------------------------------------------------------------------
// (b) Token candidate fallback ORDER (pure seam)
// ---------------------------------------------------------------------------

#[test]
fn candidate_order_is_env_env_oauth_ghcli_ghcli() {
    let got = github_auth::order_token_candidates(
        Some("gh".into()),
        Some("github".into()),
        Some("oauth".into()),
        Some("ghtok".into()),
        Some("ghcli".into()),
    );
    assert_eq!(
        got,
        vec![
            ("gh".to_string(), TokenSource::Env),
            ("github".to_string(), TokenSource::Env),
            ("oauth".to_string(), TokenSource::OAuth),
            ("ghtok".to_string(), TokenSource::GhCli),
            ("ghcli".to_string(), TokenSource::GhCli),
        ]
    );
}

#[test]
fn candidate_order_dedupes_by_value_keeping_earliest() {
    // GITHUB_TOKEN duplicates GH_TOKEN → dropped; OAuth survives.
    let got = github_auth::order_token_candidates(
        Some("same".into()),
        Some("same".into()),
        Some("other".into()),
        None,
        None,
    );
    assert_eq!(
        got,
        vec![
            ("same".to_string(), TokenSource::Env),
            ("other".to_string(), TokenSource::OAuth),
        ]
    );
}

#[test]
fn candidate_order_collapses_identical_across_all_sources() {
    let got = github_auth::order_token_candidates(
        None,
        None,
        Some("x".into()),
        Some("x".into()),
        Some("x".into()),
    );
    assert_eq!(got, vec![("x".to_string(), TokenSource::OAuth)]);
}

#[test]
fn candidate_order_empty_when_no_sources() {
    let got = github_auth::order_token_candidates(None, None, None, None, None);
    assert!(got.is_empty());
}

#[test]
fn token_source_default_is_none() {
    // Disconnect resets the source to this default; logout falls back through
    // the ordered candidates above (empty → None).
    assert_eq!(TokenSource::default(), TokenSource::None);
}

// ---------------------------------------------------------------------------
// (c) github_diagnostics output
// ---------------------------------------------------------------------------

#[test]
fn diagnostics_fresh_state_reports_healthy_empty() {
    let state = make_test_app_state();
    let diag = github_auth::compute_diagnostics(&state);
    assert!(!diag.circuit_breaker_open);
    assert_eq!(diag.circuit_breaker_status, "OK");
    assert!(diag.repos_not_found.is_empty());
    assert_eq!(diag.repos_monitored, 0);
}

#[test]
fn diagnostics_reports_cooldown_and_monitored_repos() {
    let state = make_test_app_state();
    // A future cooldown → "not found"; an expired one is ignored.
    state.git_cache.github_repo_cooldown.insert(
        "octocat/hello".to_string(),
        std::time::Instant::now() + std::time::Duration::from_secs(3600),
    );
    state.git_cache.github_repo_cooldown.insert(
        "octocat/expired".to_string(),
        std::time::Instant::now() - std::time::Duration::from_secs(1),
    );
    // A cached status counts as a monitored repo.
    state
        .git_cache
        .github_status
        .insert("/path/zero".to_string(), (vec![], std::time::Instant::now()));

    let diag = github_auth::compute_diagnostics(&state);
    assert_eq!(diag.repos_not_found, vec!["octocat/hello".to_string()]);
    assert_eq!(diag.repos_monitored, 1);
}

// ---------------------------------------------------------------------------
// (d) Cooldown-cache key construction (seam Step 9 will host-scope)
// ---------------------------------------------------------------------------

#[test]
fn cooldown_key_is_owner_slash_name_today() {
    assert_eq!(github::cooldown_key("octocat", "hello"), "octocat/hello");
}

// ---------------------------------------------------------------------------
// (e) Viewer-login caching
// ---------------------------------------------------------------------------

#[tokio::test]
async fn viewer_login_returns_cached_value_without_network() {
    let state = make_test_app_state();
    *state.github_viewer_login.write() = Some("octocat".to_string());
    // Cache hit short-circuits before any GraphQL call (no token configured).
    let login = github::get_viewer_login(&state).await.unwrap();
    assert_eq!(login, "octocat");
}
