use super::*;

// Catches: operator evidence releasing a live run, an unconfirmed stop, or an
// empty reconciliation, and a late observation undoing an explicit disposition.
#[tokio::test]
async fn mutation_resolution_requires_ended_run_stop_and_evidence_and_survives_replay() {
    for expired in [false, true] {
        let (dir, owner, run, at) = fixture(30, false);
        let run = prepared(&owner, &run, at);
        owner.neutral_host(FileHost::new(
            &owner,
            dir.path().join("write"),
            Reply::Lost,
            false,
        ));
        let uncertain = dispatch(&owner, &run.run.id).await;
        let key = only_key(&uncertain);
        assert!(
            owner
                .neutral_command(
                    &run.run.id,
                    "live",
                    uncertain.sequence,
                    resolution(&key, "reconciled")
                )
                .is_err()
        );
        let cancelled = if expired {
            owner
                .neutral_command_at(
                    &run.run.id,
                    "expire",
                    uncertain.sequence,
                    Command::Step,
                    uncertain.deadline_ms,
                )
                .unwrap()
        } else {
            owner
                .neutral_command(&run.run.id, "cancel", uncertain.sequence, Command::Cancel)
                .unwrap()
        };
        assert!(
            owner
                .neutral_command(
                    &run.run.id,
                    "unproved-stop",
                    cancelled.sequence,
                    resolution(&key, "operator says stopped, but backend has no proof")
                )
                .is_err()
        );
        assert_eq!(
            claims(&owner),
            1,
            "operator confirmation cannot replace backend stop proof"
        );
        let state = Arc::new(crate::state::tests_support::make_test_app_state());
        state.workflow_runtime.attach_owner(owner.clone());
        state
            .workflow_runtime
            .install_neutral_catalog_host(FileHost::new(
                &owner,
                dir.path().join("unused"),
                Reply::Lost,
                false,
            ));
        let action =
            serde_json::from_value(json!({"action":"resolve_mutation", "run_id":run.run.id,
            "command_id":"api-unproved-stop", "expected_sequence":cancelled.sequence,
            "key":key, "evidence":"operator assertion is not backend proof"}))
            .unwrap();
        let refusal = crate::workflows::run::neutral_api::run_action(
            &state,
            &run.run.request.publication.project,
            action,
            Some(crate::workflows::NativeCaller::Owner),
        )
        .unwrap_err();
        assert!(
            refusal.contains("no original-process stop proof"),
            "{refusal}"
        );
        assert_eq!(claims(&owner), 1);
        let cancelled = owner
            .neutral_effect_command(
                &run.run.id,
                EffectCommand::Observe {
                    key: key.clone(),
                    observation: EffectObservation {
                        stopped: true,
                        outcome: EffectOutcome::Uncertain {
                            diagnostic: "original write requires reconciliation; stop now proven"
                                .into(),
                        },
                    },
                },
            )
            .unwrap();
        {
            let (name, evidence) = ("empty", " ");
            assert!(
                owner
                    .neutral_command(
                        &run.run.id,
                        name,
                        cancelled.sequence,
                        resolution(&key, evidence)
                    )
                    .is_err()
            );
            assert_eq!(claims(&owner), 1);
        }
        let resolved = owner
            .neutral_command(
                &run.run.id,
                "resolve",
                cancelled.sequence,
                resolution(
                    &key,
                    "Original process stopped; verified and reconciled workspace",
                ),
            )
            .unwrap();
        assert_eq!(claims(&owner), 0);
        assert_eq!(
            owner
                .neutral_command(
                    &run.run.id,
                    "resolve",
                    cancelled.sequence,
                    resolution(
                        &key,
                        "Original process stopped; verified and reconciled workspace",
                    )
                )
                .unwrap(),
            resolved
        );
        let late = owner
            .neutral_effect_command(
                &run.run.id,
                EffectCommand::Observe {
                    key,
                    observation: EffectObservation {
                        stopped: false,
                        outcome: EffectOutcome::Uncertain {
                            diagnostic: "late lost reply".into(),
                        },
                    },
                },
            )
            .unwrap();
        assert_eq!(claims(&owner), 0);
        assert_eq!(late.status, resolved.status);
        drop(state);
        drop(owner);
        let owner = RuntimeOwner::acquire(&dir.path().join("runs.sqlite3")).unwrap();
        assert_eq!(
            owner.store.neutral_execution(&run.run.id).unwrap(),
            resolved
        );
    }
}

// Catches: allowing only Cancelled while leaving expired/failed mutation owners
// permanently locked. These are the reducer's actual terminal decisions, not
// hand-set status flags; the full actor/SQLite release path is tested above.
#[tokio::test]
async fn mutation_resolution_accepts_expired_and_failed_reducer_states_without_reopening() {
    for end in [
        crate::workflows::run::neutral_execution::NeutralDecision::Expire,
        crate::workflows::run::neutral_execution::NeutralDecision::Failure {
            diagnostic: "sibling failed".into(),
        },
    ] {
        let (dir, owner, run, at) = fixture(30, false);
        let run = prepared(&owner, &run, at);
        owner.neutral_host(FileHost::new(
            &owner,
            dir.path().join("write"),
            Reply::Lost,
            false,
        ));
        let mut uncertain = dispatch(&owner, &run.run.id).await;
        let key = only_key(&uncertain);
        uncertain = owner
            .neutral_effect_command(
                &run.run.id,
                EffectCommand::Observe {
                    key: key.clone(),
                    observation: EffectObservation {
                        stopped: true,
                        outcome: EffectOutcome::Uncertain {
                            diagnostic: "original stopped write still requires reconciliation"
                                .into(),
                        },
                    },
                },
            )
            .unwrap();
        uncertain.apply(&end).unwrap();
        let status = uncertain.status.clone();
        let Command::Effect { command } = resolution(
            &key,
            "Stopped original operation and completed reconciliation",
        ) else {
            unreachable!()
        };
        let decision = uncertain.decide_effect(&command, at).unwrap().unwrap();
        uncertain.apply_effect(&decision).unwrap();
        crate::workflows::run::neutral_effects::project_resources(
            &owner.store.connect().unwrap(),
            &uncertain,
            &decision,
        )
        .unwrap();
        assert_eq!(claims(&owner), 0);
        assert_eq!(uncertain.status, status);
        assert!(matches!(
            uncertain
                .effect(&key)
                .unwrap()
                .observation
                .as_ref()
                .unwrap()
                .outcome,
            EffectOutcome::Uncertain { .. }
        ));
    }
}
