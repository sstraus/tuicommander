//! Source-scan guard: GitHub traffic is never anonymous.

use std::path::Path;

fn rust_sources(dir: &Path, out: &mut Vec<std::path::PathBuf>) {
    for entry in std::fs::read_dir(dir).unwrap() {
        let path = entry.unwrap().path();
        if path.is_dir() {
            rust_sources(&path, out);
        } else if path.extension().is_some_and(|e| e == "rs") {
            out.push(path);
        }
    }
}

/// The text from `open` (an opening delimiter) to its matching close.
fn balanced(text: &str, open: usize) -> &str {
    let (o, c) = match text.as_bytes()[open] {
        b'(' => (b'(', b')'),
        _ => (b'{', b'}'),
    };
    let mut depth = 0;
    for (i, b) in text.bytes().enumerate().skip(open) {
        if b == o {
            depth += 1;
        } else if b == c {
            depth -= 1;
            if depth == 0 {
                return &text[open..=i];
            }
        }
    }
    &text[open..]
}

// Catches: a `RepoApi` built without a caller id and channel, which reaches the
// gateway as anonymous traffic and makes the per-caller benchmark lie.
#[test]
fn every_repo_api_construction_names_a_caller() {
    let mut files = Vec::new();
    rust_sources(
        &Path::new(env!("CARGO_MANIFEST_DIR")).join("src"),
        &mut files,
    );
    let mut anonymous = Vec::new();
    let mut seen = 0;
    for file in files {
        if file.ends_with("provider_gateway/caller_tests.rs") {
            continue;
        }
        let text = std::fs::read_to_string(&file).unwrap();
        for pattern in ["RepoApi::resolve(", "RepoApi {"] {
            for (at, _) in text.match_indices(pattern) {
                seen += 1;
                let open = at + pattern.len() - 1;
                // A literal `Origin::..` or a forwarded `origin` argument.
                let call = balanced(&text, open);
                if !call.contains("Origin::") && !call.contains("origin") {
                    let line = text[..at].matches('\n').count() + 1;
                    anonymous.push(format!("{}:{line}", file.display()));
                }
            }
        }
    }
    assert!(seen > 30, "the scan found only {seen} constructions");
    assert!(anonymous.is_empty(), "anonymous RepoApi: {anonymous:#?}");
}
