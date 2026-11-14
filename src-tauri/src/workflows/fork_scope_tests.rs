use super::collection::{AggregateState, CollectionIdentity};
use super::fork_scope::*;
use std::collections::BTreeMap;

fn contract() -> ForkContract {
    ForkContract {
        branches: BTreeMap::from([
            (
                "left".into(),
                BranchContract {
                    activity: 0,
                    required: true,
                },
            ),
            (
                "right".into(),
                BranchContract {
                    activity: 1,
                    required: false,
                },
            ),
        ]),
        max_concurrency: 2,
    }
}
fn identity() -> CollectionIdentity {
    CollectionIdentity {
        run: "run".into(),
        scope: "fork".into(),
        generation: 2,
    }
}
fn expected() -> BTreeMap<String, BranchAttempt> {
    BTreeMap::from([
        (
            "left".into(),
            BranchAttempt {
                activation: "a-left".into(),
                attempt: 2,
            },
        ),
        (
            "right".into(),
            BranchAttempt {
                activation: "a-right".into(),
                attempt: 1,
            },
        ),
    ])
}
fn arrival(branch: &str, state: ArrivalState) -> BranchArrival {
    BranchArrival {
        identity: identity(),
        branch: branch.into(),
        owner: expected()[branch].clone(),
        state,
    }
}

// Catches: duplicate or out-of-order arrivals satisfying another branch, or optional failures hiding unfinished work.
#[test]
fn joins_wait_for_every_branch_and_fold_arrivals_idempotently() {
    let contract = contract();
    contract.validate(2).unwrap();
    let left = arrival("left", ArrivalState::Completed);
    assert_eq!(
        contract
            .join(&identity(), &expected(), &[left.clone(), left.clone()])
            .unwrap()
            .state,
        AggregateState::Pending
    );
    let right = arrival("right", ArrivalState::Failed);
    let forward = contract
        .join(&identity(), &expected(), &[left.clone(), right.clone()])
        .unwrap();
    assert_eq!(forward.state, AggregateState::Complete);
    assert_eq!(
        forward,
        contract
            .join(&identity(), &expected(), &[right, left])
            .unwrap()
    );
    for failed in [
        ArrivalState::Failed,
        ArrivalState::Cancelled,
        ArrivalState::Skipped,
    ] {
        assert_eq!(
            contract
                .join(
                    &identity(),
                    &expected(),
                    &[
                        arrival("left", failed),
                        arrival("right", ArrivalState::Completed)
                    ]
                )
                .unwrap()
                .state,
            AggregateState::Blocked
        );
    }
}

// Catches: a stale run/scope/generation or replaced activation/attempt satisfying the current join.
#[test]
fn stale_results_cannot_satisfy_a_later_join_snapshot() {
    let contract = contract();
    let valid = arrival("left", ArrivalState::Completed);
    for stale in [
        CollectionIdentity {
            run: "other".into(),
            ..identity()
        },
        CollectionIdentity {
            scope: "other".into(),
            ..identity()
        },
        CollectionIdentity {
            generation: 1,
            ..identity()
        },
    ] {
        let mut invalid = valid.clone();
        invalid.identity = stale;
        assert!(
            contract
                .join(&identity(), &expected(), &[invalid])
                .unwrap_err()
                .contains("generation")
        );
    }
    for owner in [
        BranchAttempt {
            activation: "old".into(),
            attempt: 2,
        },
        BranchAttempt {
            activation: "a-left".into(),
            attempt: 1,
        },
    ] {
        let mut invalid = valid.clone();
        invalid.owner = owner;
        assert!(
            contract
                .join(&identity(), &expected(), &[invalid])
                .unwrap_err()
                .contains("activation/attempt")
        );
    }
    let mut unknown = valid.clone();
    unknown.branch = "foreign".into();
    assert!(contract.join(&identity(), &expected(), &[unknown]).is_err());
    assert!(
        contract
            .join(
                &identity(),
                &expected(),
                &[valid, arrival("left", ArrivalState::Failed)]
            )
            .unwrap_err()
            .contains("conflicting")
    );
    let mut duplicate_owner = expected();
    duplicate_owner.get_mut("right").unwrap().activation = "a-left".into();
    assert!(contract.join(&identity(), &duplicate_owner, &[]).is_err());
    let mut missing = expected();
    missing.remove("right");
    assert!(contract.join(&identity(), &missing, &[]).is_err());
}

// Catches: invalid body declarations or concurrency bounds bypassing publication validation.
#[test]
fn fork_contract_requires_distinct_declared_bodies_and_bounds() {
    let valid = contract();
    for concurrency in [0, 3] {
        let mut invalid = valid.clone();
        invalid.max_concurrency = concurrency;
        assert!(invalid.validate(2).is_err());
    }
    let mut invalid = valid.clone();
    invalid.branches.get_mut("right").unwrap().activity = 0;
    assert!(invalid.validate(2).unwrap_err().contains("distinct"));
    assert!(valid.validate(1).is_err());
}
