//! Source-scan guard: nothing reaches a GitHub host, or the CI and release
//! endpoints that share its budget story, except through the gateway.

use std::path::{Path, PathBuf};

/// Files allowed to `.send()` a request that mentions GitHub, each with its
/// reason. Anything else goes through `Gateway::execute` / `execute_stream`.
const ALLOWED: [(&str, &str); 4] = [
    (
        "provider_gateway/transport.rs",
        "the gateway's own wire adapter",
    ),
    (
        "github_live_measurement_1995.rs",
        "test-only receiver, compiled under #[cfg(test)] through #[path]",
    ),
    (
        "github_volume_bench/etag_probe.rs",
        "opt-in live probe of the benchmark, compiled under #[cfg(test)] through #[path]",
    ),
    (
        "workflows/run/native_github_git.rs",
        "git over HTTPS is git transport, not the REST API: the git client owns that \
         connection, and it has no `.send()` to route. Out of scope for the gateway",
    ),
];

/// Files that send without naming a GitHub host in the file (a CI provider, a
/// release base URL passed in by the caller). The scan checks them by path.
const GATEWAY_ONLY: [&str; 6] = [
    "circleci.rs",
    "updater.rs",
    "release_install.rs",
    "remote_deploy/assets.rs",
    "accounts/verify.rs",
    "github_account.rs",
];

fn rust_sources(dir: &Path, out: &mut Vec<PathBuf>) {
    for entry in std::fs::read_dir(dir).unwrap() {
        let path = entry.unwrap().path();
        if path.is_dir() {
            rust_sources(&path, out);
        } else if path.extension().is_some_and(|e| e == "rs") {
            out.push(path);
        }
    }
}

/// The text before the first `#[cfg(test)]` at the start of a line.
fn production(text: &str) -> &str {
    let mut offset = 0;
    for line in text.split_inclusive('\n') {
        if line.starts_with("#[cfg(test)]") {
            return &text[..offset];
        }
        offset += line.len();
    }
    text
}

fn is_test_file(rel: &str) -> bool {
    rel.ends_with("_tests.rs") || rel.contains("/tests/") || rel.ends_with("/tests.rs")
}

// Catches: a new `.send()` to a GitHub host (or to the CI and release endpoints)
// that skips the budget, the failure memory and the per-caller metrics (R1).
#[test]
fn no_production_reqwest_send_to_github_hosts_outside_the_gateway() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
    let mut files = Vec::new();
    rust_sources(&root, &mut files);
    let mut violations = Vec::new();
    let mut scanned = 0;
    for file in files {
        let rel = file
            .strip_prefix(&root)
            .unwrap()
            .to_string_lossy()
            .replace('\\', "/");
        if is_test_file(&rel) || ALLOWED.iter().any(|(path, _)| *path == rel) {
            continue;
        }
        let text = std::fs::read_to_string(&file).unwrap();
        let prod = production(&text);
        scanned += 1;
        if !prod.contains(".send()") {
            continue;
        }
        let names_github = prod.to_ascii_lowercase().contains("github");
        if names_github || GATEWAY_ONLY.contains(&rel.as_str()) {
            violations.push(rel);
        }
    }
    assert!(scanned > 200, "the scan covered only {scanned} files");
    assert!(
        violations.is_empty(),
        "reqwest .send() outside the gateway: {violations:#?}"
    );
    for (path, reason) in ALLOWED {
        assert!(root.join(path).exists(), "allow-list entry {path} is gone");
        assert!(!reason.is_empty());
    }
}
