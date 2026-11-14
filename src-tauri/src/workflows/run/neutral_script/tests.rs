use super::super::neutral_effects::EffectCommand;
use super::super::neutral_execution::{NeutralResult, NeutralStatus};
use super::fixtures::*;
use super::*;
use serde_json::json;
use std::collections::BTreeMap;
use std::sync::Arc;
use tokio::sync::Notify;

// Catches: flattening structured values, dropping artifact references, accepting
// stderr as data, or returning another activation's result.
#[test]
fn structured_process_preserves_json_artifacts_identity_and_separate_stderr() {
    let (dir, _owner, effect, adapter) = prepared(ECHO, &policy(false));
    let observation = adapter.invoke(&effect, dir.path());
    let result = observation.result.unwrap();
    assert_eq!(result.version, 1);
    assert_eq!(result.identity, effect.key);
    assert_eq!(
        result.data["payload"],
        json!({"empty":[],"nothing":null,"text":"7"})
    );
    assert_eq!(
        result.artifacts["report"],
        json!({"reference":"artifact","id":"report","revision":"r1"})
    );
    assert_eq!(observation.stderr, "diagnostic only\n");
    match observation.effect.outcome {
        EffectOutcome::Completed {
            result: NeutralResult::Completed { outcome, outputs },
        } => {
            assert_eq!(outcome, "ok");
            assert_eq!(outputs["next"], json!(7));
            assert_eq!(
                outputs["report"],
                json!({"reference":"artifact","id":"report","revision":"r1"})
            );
        }
        other => panic!("{other:?}"),
    }
    assert!(observation.effect.stopped);
}

// Catches: exit zero replacing required protocol validation; foreign attempt or
// undeclared outcome advancing the workflow, or diagnostics becoming output.
#[test]
fn exit_zero_invalid_missing_foreign_or_wrong_shape_json_is_protocol_failure() {
    let base = "import json,sys; q=json.load(sys.stdin); r=dict(version=1,identity=q['identity'],outcome='ok',data=dict(next=7),artifacts={}); ";
    for suffix in [
        "print('')",
        "print('{}')",
        "print(json.dumps(r),file=sys.stderr); print('invalid')",
        "r['identity']['fence']['attempt']+=1; print(json.dumps(r))",
        "r['version']=2; print(json.dumps(r))",
        "r['outcome']='invented'; print(json.dumps(r))",
        "r['data']['next']='7'; print(json.dumps(r))",
        "r['artifacts']['next']=dict(reference='artifact',id='x',revision='r'); print(json.dumps(r))",
    ] {
        let (dir, _owner, effect, adapter) = prepared(&format!("{base}{suffix}"), &policy(false));
        let observation = adapter.invoke(&effect, dir.path());
        assert!(observation.result.is_none(), "{suffix}");
        match observation.effect.outcome {
            EffectOutcome::Completed {
                result: NeutralResult::Failed { diagnostic },
            } => {
                assert!(
                    diagnostic.starts_with("script protocol:")
                        || diagnostic.starts_with("script outputs."),
                    "{suffix}: {diagnostic}"
                );
            }
            other => panic!("{suffix}: {other:?}"),
        }
        assert!(observation.effect.stopped);
    }
}

// Catches: admitting unbounded limits, literal secret values or mutation without
// declared ownership/reconciliation through the generic configuration schema.
#[test]
fn script_policy_checks_limits_references_and_mutation_requirements() {
    let p = policy(false);
    for (field, value) in [
        ("version", json!(2)),
        ("timeout_secs", json!(0)),
        ("stdout_bytes", json!(1_048_576)),
        ("stderr_bytes", json!(0)),
        ("input_bytes", json!(0)),
        ("artifact_limit", json!(65)),
        ("secret_refs", json!({"KEY":"literal token"})),
        ("retry", json!("idempotent")),
        ("mutation", json!(true)),
    ] {
        let mut d = descriptor(ECHO, &p);
        let mut malformed = json!(p);
        malformed[field] = value;
        d.config.fields.get_mut("script").unwrap().default = Some(malformed);
        let instance = crate::workflows::catalog::ActivityInstance {
            descriptor: d.id.clone(),
            config: BTreeMap::new(),
            execution_policy: "local".into(),
        };
        assert!(ScriptPolicy::resolve(&d, &instance).is_err(), "{field}");
    }
}

// Catches: rejecting explicit simple-command output or silently accepting
// structured mode's missing JSON on the same command.
#[test]
fn explicit_exit_status_accepts_plain_stdout_and_no_required_data() {
    let mut p = policy(false);
    p.output = OutputMode::ExitStatus {
        outcome: "ok".into(),
    };
    let (dir, _owner, effect, adapter) = prepared("print('plain command')", &p);
    let observation = adapter.invoke(&effect, dir.path());
    assert!(matches!(
        observation.effect.outcome,
        EffectOutcome::Completed {
            result: NeutralResult::Completed { .. }
        }
    ));
    assert_eq!(observation.result.unwrap().data, BTreeMap::new());
}

// Catches: custom operations requiring a kernel case, losing descriptor schemas
// on publication/replay, or bypassing the existing neutral owner to execute.
#[tokio::test]
async fn custom_descriptor_publishes_renderer_data_and_executes_through_neutral_actor() {
    let (dir, owner, run, _) = fixture(ECHO, &policy(false));
    let published = serde_json::to_value(&run.run.closure.descriptors[0]).unwrap();
    assert_eq!(published["id"]["namespace"], "custom");
    assert_eq!(
        published["handler"],
        json!({"name":"host.script","version":1})
    );
    assert_eq!(
        published["config"]["fields"]["script"]["value_type"],
        "object"
    );
    assert_eq!(
        published["inputs"]["fields"]["value"]["value_type"],
        "integer"
    );
    assert_eq!(
        published["outputs"]["fields"]["report"]["value_type"],
        "artifact_reference"
    );
    assert_eq!(published["outcomes"], json!(["ok"]));
    owner.neutral_host(Arc::new(Host {
        adapter: ScriptAdapter {
            runs_db: dir.path().join("runs.sqlite3"),
            input_directory: dir.path().join("inputs"),
        },
        workspace: dir.path().into(),
    }));
    super::super::runtime::run_actor(
        Arc::downgrade(&owner),
        run.run.id.clone(),
        Arc::new(Notify::new()),
    )
    .await;
    let finished = owner.store.neutral_execution(&run.run.id).unwrap();
    assert_eq!(
        finished.status,
        NeutralStatus::Completed {
            outcome: "done".into()
        }
    );
    assert_eq!(finished.outputs.unwrap().values["result"], json!(7));
    assert_eq!(finished.effects.len(), 1);
}

// Catches: interpreting missing post-write output as a safe failure, releasing its
// ownership on process stop, or reconciliation executing the mutating script again.
#[test]
fn mutation_with_lost_output_remains_uncertain_after_stop_replay_and_reconciliation() {
    let script = "from pathlib import Path; p=Path('writes'); p.write_text(p.read_text()+'x' if p.exists() else 'x')";
    let (dir, owner, effect, adapter) = prepared(script, &policy(true));
    let observation = adapter.invoke(&effect, dir.path());
    assert_eq!(
        std::fs::read_to_string(dir.path().join("writes")).unwrap(),
        "x"
    );
    assert!(matches!(
        observation.effect.outcome,
        EffectOutcome::Uncertain { .. }
    ));
    assert!(observation.effect.stopped);
    let run = owner
        .neutral_effect_command(
            &effect.key.fence.run,
            EffectCommand::Observe {
                key: effect.key.clone(),
                observation: observation.effect,
            },
        )
        .unwrap();
    assert!(!run.effect(&effect.key).unwrap().settled());
    drop(owner);
    let owner =
        super::super::runtime::RuntimeOwner::acquire(&dir.path().join("runs.sqlite3")).unwrap();
    let replayed = owner
        .store
        .neutral_execution(&effect.key.fence.run)
        .unwrap();
    let observation = adapter.reconcile(replayed.effect(&effect.key).unwrap());
    assert!(observation.stopped);
    match observation.outcome {
        EffectOutcome::Uncertain { diagnostic } => {
            assert!(diagnostic.contains("Inspect the retained effect"))
        }
        other => panic!("{other:?}"),
    }
    assert_eq!(
        std::fs::read_to_string(dir.path().join("writes")).unwrap(),
        "x"
    );
    assert!(replayed.attempt.unwrap().result.is_none());
}

// Catches: parsing a valid truncated prefix as success or allowing unbounded
// stderr; input limits must refuse before the child can write.
#[test]
fn stream_and_input_limits_refuse_without_fabricating_success() {
    for (field, script) in [
        ("stdout", "print('x'*100)"),
        ("stderr", "import sys; print('x'*100,file=sys.stderr)"),
        (
            "input",
            "from pathlib import Path; Path('entered').write_text('yes')",
        ),
    ] {
        let mut p = policy(false);
        match field {
            "stdout" => p.stdout_bytes = 10,
            "stderr" => p.stderr_bytes = 10,
            _ => p.input_bytes = 1,
        }
        let (dir, _owner, effect, adapter) = prepared(script, &p);
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
}

// Catches: process cancellation being reported as rollback certainty, or a
// cancelled owned worker continuing to write after stop was reported.
#[test]
fn cancellation_stops_owned_process_and_keeps_mutation_certainty_separate() {
    let script = "from pathlib import Path; import time; Path('entered').write_text('x'); time.sleep(120); Path('late').write_text('bad')";
    let (dir, _owner, effect, adapter) = prepared(script, &policy(true));
    let worker_adapter = adapter.clone();
    let worker_effect = effect.clone();
    let workspace = dir.path().to_path_buf();
    let worker = std::thread::spawn(move || worker_adapter.invoke(&worker_effect, &workspace));
    let setup_deadline = std::time::Instant::now() + std::time::Duration::from_secs(90);
    while !dir.path().join("entered").exists() {
        assert!(
            !worker.is_finished(),
            "owned process ended before readiness"
        );
        assert!(
            std::time::Instant::now() < setup_deadline,
            "owned script readiness unavailable"
        );
        std::thread::sleep(std::time::Duration::from_millis(10));
    }
    let request = adapter.cancel(&effect);
    assert!(matches!(request.outcome, EffectOutcome::Uncertain { .. }));
    assert!(!request.stopped);
    let observation = worker.join().unwrap();
    assert!(observation.effect.stopped);
    assert!(matches!(
        observation.effect.outcome,
        EffectOutcome::Uncertain { .. }
    ));
    assert_eq!(
        std::fs::read_to_string(dir.path().join("entered")).unwrap(),
        "x"
    );
    assert!(!dir.path().join("late").exists());
}
