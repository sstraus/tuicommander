//! Conformance of `ExternalBackend` against real adapter processes: the
//! reference adapter in `scripts/story-adapter-reference/` and small Node
//! scripts that crash or hang.

use super::*;
use std::time::Instant;
use tempfile::TempDir;

fn scratch() -> TempDir {
    TempDir::new_in(crate::test_support::test_temp_root()).unwrap()
}

fn reference_backend(root: &Path) -> ExternalBackend {
    let script = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../scripts/story-adapter-reference/adapter.mjs");
    let command = AdapterCommand {
        program: "node".into(),
        args: vec![script.to_string_lossy().into_owned()],
    };
    ExternalBackend::new(command, root)
}

fn node_script(source: &str, args: &[&str]) -> AdapterCommand {
    let mut all = vec!["-e".to_string(), source.to_string()];
    all.extend(args.iter().map(|arg| arg.to_string()));
    AdapterCommand {
        program: "node".into(),
        args: all,
    }
}

#[test]
fn reference_adapter_round_trip_conforms_to_protocol() {
    // Catches: a stale expected_version write accepted instead of Conflict,
    // a backend rule answer not mapped to Refused, an unknown id not mapped to
    // NotFound, a watch path that is not the file the adapter writes, or
    // `changes` missing a write made after the cursor.
    let dir = scratch();
    let backend = reference_backend(dir.path());

    let info = backend.info().unwrap();
    assert_eq!(info.protocol, PROTOCOL_VERSION);
    assert_eq!(info.adapter.id, "tuic-reference");
    assert!(info.supports("apply") && info.supports("set_status"));
    assert!(!info.supports("delete_story"));
    let store = dir.path().join(".tuic-reference-stories.json");
    assert_eq!(info.watch_paths.len(), 1);

    let Changes::RescanRequired { cursor: baseline } = backend.changes(None).unwrap() else {
        panic!("a missing cursor must ask for a rescan");
    };

    let created = backend
        .create_story(&NewExternalStory {
            title: "Fix login".into(),
            criteria: vec!["first".into(), "second".into()],
            ..Default::default()
        })
        .unwrap();
    assert_eq!(created.status, "pending");
    assert_eq!(backend.get_story(&created.id).unwrap(), created);
    assert_eq!(
        info.watch_paths[0].canonicalize().unwrap(),
        store.canonicalize().unwrap()
    );

    let checked = backend
        .apply(
            &created.id,
            &created.version,
            &ApplyOp::CheckCriterion { index: 0 },
        )
        .unwrap();
    assert!(checked.criteria[0].checked);
    assert_ne!(checked.version, created.version);

    let stale = backend.apply(
        &created.id,
        &created.version,
        &ApplyOp::SetFields {
            title: Some("overwrite".into()),
            problem: None,
            priority: None,
        },
    );
    assert_eq!(stale, Err(AdapterError::Conflict));
    assert_eq!(backend.get_story(&created.id).unwrap().title, "Fix login");

    let refused = backend.apply(
        &created.id,
        &checked.version,
        &ApplyOp::SetStatus {
            status: "complete".into(),
        },
    );
    assert!(
        matches!(refused, Err(AdapterError::Refused(_))),
        "{refused:?}"
    );

    let noted = backend.append_note(&created.id, "started").unwrap();
    assert_eq!(noted.notes.last().unwrap().text, "started");

    let Changes::Changed { ids, cursor } = backend.changes(Some(&baseline)).unwrap() else {
        panic!("a current cursor must return changes");
    };
    assert_eq!(ids, vec![created.id.clone()]);
    assert_eq!(
        backend.changes(Some(&cursor)).unwrap(),
        Changes::Changed {
            ids: vec![],
            cursor: cursor.clone()
        }
    );

    assert!(matches!(
        backend.get_story("missing"),
        Err(AdapterError::NotFound(_))
    ));
    backend.shutdown();
}

#[test]
fn restarted_adapter_turns_an_old_cursor_into_a_rescan() {
    // Catches: after a crash and restart TUIC passes its old cursor to the new
    // process and trusts the delta, missing changes made while it was down.
    let dir = scratch();
    let backend = reference_backend(dir.path());
    let Changes::RescanRequired { cursor } = backend.changes(None).unwrap() else {
        panic!("baseline must ask for a rescan");
    };
    assert!(matches!(
        backend.changes(Some(&cursor)).unwrap(),
        Changes::Changed { .. }
    ));

    backend.kill_adapter_for_test();
    let after_crash = backend.changes(Some(&cursor));
    assert!(
        matches!(after_crash, Err(AdapterError::Unavailable(_))),
        "{after_crash:?}"
    );
    assert!(
        matches!(
            backend.changes(Some(&cursor)).unwrap(),
            Changes::RescanRequired { .. }
        ),
        "a cursor from before the restart must not yield a delta"
    );
    backend.shutdown();
}

#[test]
fn crashing_adapter_is_unavailable_and_restarts_with_backoff() {
    // Catches: a crash loop that starts a new process on every call, or a
    // failure reported as anything but Unavailable.
    let dir = scratch();
    let spawns = dir.path().join("spawns");
    let backend = ExternalBackend::new(
        node_script(
            "require('fs').appendFileSync(process.argv[1], 'x'); process.exit(3)",
            &[spawns.to_str().unwrap()],
        ),
        dir.path(),
    );
    for _ in 0..4 {
        let result = backend.list_plans();
        assert!(
            matches!(result, Err(AdapterError::Unavailable(_))),
            "{result:?}"
        );
    }
    // First failure restarts at once; the second waits 1 s, so calls 3 and 4 do not spawn.
    assert_eq!(std::fs::read_to_string(&spawns).unwrap(), "xx");
}

#[test]
fn silent_adapter_times_out_as_unavailable() {
    // Catches: an adapter that never answers blocks the caller forever.
    let dir = scratch();
    let backend = ExternalBackend::new(node_script("setInterval(() => {}, 1000)", &[]), dir.path())
        .with_timeout(Duration::from_millis(300));
    let started = Instant::now();
    let result = backend.info();
    assert!(
        matches!(result, Err(AdapterError::Unavailable(_))),
        "{result:?}"
    );
    assert!(
        started.elapsed() < Duration::from_secs(20),
        "harness bound: the 300 ms request timeout did not fire"
    );
}

#[test]
fn adapter_with_another_protocol_version_is_refused() {
    // Catches: TUIC accepting an adapter that speaks a different protocol and
    // misreading its answers.
    let dir = scratch();
    let source = r#"
require('readline').createInterface({ input: process.stdin }).on('line', (line) => {
  const { id } = JSON.parse(line);
  process.stdout.write(JSON.stringify({ jsonrpc: '2.0', id, result: {
    protocol: 2, adapter: { id: 'future', version: '9' }, capabilities: {} } }) + '\n');
});"#;
    let backend = ExternalBackend::new(node_script(source, &[]), dir.path());
    let result = backend.info();
    assert!(
        matches!(&result, Err(AdapterError::Unavailable(reason)) if reason.contains("protocol 2")),
        "{result:?}"
    );
}

#[test]
fn backoff_doubles_from_one_second_to_sixty() {
    // Catches: an off-by-one that delays the first restart, or an uncapped
    // shift that waits for hours.
    let waits: Vec<u64> = (1..=10).map(|n| backoff(n).as_secs()).collect();
    assert_eq!(waits, vec![0, 1, 2, 4, 8, 16, 32, 60, 60, 60]);
}
