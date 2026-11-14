use super::super::neutral_execution::NeutralResult;
use super::fixtures::*;
use super::*;

// Catches: resolving references as literal values, leaking resolved secrets into
// diagnostics or ignoring the assigned working directory.
#[test]
fn declared_environment_references_resolve_at_entry_and_diagnostics_are_redacted() {
    let mut p = policy(true);
    p.environment_refs
        .insert("SCRIPT_HOME".into(), "HOME".into());
    p.secret_refs.insert("SCRIPT_SECRET".into(), "HOME".into());
    let script = r#"
import json,os,sys
from pathlib import Path
q=json.load(sys.stdin)
assert os.environ['SCRIPT_HOME']==os.environ['SCRIPT_SECRET']
print(os.environ['SCRIPT_SECRET'],file=sys.stderr)
Path('context').write_text('owned')
print(json.dumps(dict(version=1,identity=q['identity'],outcome='ok',data=dict(next=7),artifacts={})))
"#;
    let (dir, _owner, effect, adapter) = prepared(script, &p);
    let observation = adapter.invoke(&effect, dir.path());
    assert_eq!(observation.stderr, "[REDACTED]\n");
    assert!(observation.result.is_some());
    assert_eq!(
        std::fs::read_to_string(dir.path().join("context")).unwrap(),
        "owned"
    );
}

// Catches: a deadline proving process stop being treated as proof that an
// already-started mutation did not occur or may be safely replayed.
#[test]
fn timeout_keeps_mutation_uncertain_even_when_owned_process_stopped() {
    let mut p = policy(true);
    p.timeout_secs = 1;
    let (dir, _owner, effect, adapter) = prepared("import time; time.sleep(120)", &p);
    let observation = adapter.invoke(&effect, dir.path());
    assert!(observation.effect.stopped);
    assert!(matches!(
        observation.effect.outcome,
        EffectOutcome::Uncertain { .. }
    ));
    assert!(observation.result.is_none());
}

// Catches: cancellation before process registration still allowing a write, or
// manufacturing mutation uncertainty when no owned process could have entered.
#[test]
fn cancellation_before_entry_refuses_process_with_known_no_start() {
    let (dir, _owner, effect, adapter) = prepared(
        "from pathlib import Path; Path('entered').write_text('bad')",
        &policy(true),
    );
    let cancelled = adapter.cancel(&effect);
    assert!(!cancelled.stopped);
    let observation = adapter.invoke(&effect, dir.path());
    assert!(matches!(
        observation.effect.outcome,
        EffectOutcome::Completed {
            result: NeutralResult::Failed { .. }
        }
    ));
    assert!(observation.effect.stopped);
    assert!(!dir.path().join("entered").exists());
}

// Catches: non-secret environment references fed to the secret redactor, so a
// script that reports a path under a declared environment value returns
// `[REDACTED]/...` as workflow data — a silently wrong result.
#[test]
fn non_secret_environment_reference_is_not_redacted_from_result_data() {
    let mut p = policy(false);
    p.environment_refs
        .insert("SCRIPT_HOME".into(), "HOME".into());
    let script = r#"
import json,os,sys
q=json.load(sys.stdin)
print(json.dumps(dict(version=1,identity=q['identity'],outcome='ok',data=dict(next=7,note=os.environ['SCRIPT_HOME']+'/report.txt'),artifacts={})))
"#;
    let (dir, _owner, effect, adapter) = prepared(script, &p);
    let observation = adapter.invoke(&effect, dir.path());
    let result = observation.result.expect("structured result");
    assert_eq!(
        result.data["note"],
        serde_json::json!(format!("{}/report.txt", std::env::var("HOME").unwrap()))
    );
}
