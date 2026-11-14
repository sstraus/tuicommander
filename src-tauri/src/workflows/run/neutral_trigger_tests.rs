use super::neutral_trigger_fixture::*;
use crate::workflows::trigger::*;
use serde_json::json;
use std::collections::BTreeMap;

// Catches: UNIQUE(source,repository), invalid mapping enablement, missing overlap
// acknowledgement, cross-channel duplicates and policy edits replaying events.
#[test]
fn independent_bindings_validate_overlap_and_keep_source_receipts_across_edits() {
    let f = fixture();
    let mut bad = f.draft.clone();
    bad.mapping.inputs.insert("initial".into(), "title".into());
    assert!(
        f.workflows
            .save_neutral_trigger_binding("/project", 0, bad, &f.catalog, &f.handlers)
            .unwrap_err()
            .contains("type")
    );
    let first = f.enable(f.draft.clone(), 0);
    let admitted = f.admit(event());
    let original = admitted[0].run_id.clone().unwrap();
    assert_eq!(
        f.owner.store.neutral_run(&original).unwrap().inputs["initial"],
        json!(1347)
    );
    let mut again = event();
    again.delivery_id = Some("manual-or-webhook-retry".into());
    assert_eq!(f.admit(again)[0].run_id.as_ref(), Some(&original));
    let old = f.owner.store.neutral_run(&original).unwrap();
    let publication = f
        .workflows
        .save_neutral_publication(
            "/project",
            &first.draft.publication.id,
            1,
            old.closure.root.definition.clone(),
            &f.catalog,
            &f.handlers,
        )
        .unwrap();
    let mut changed = first.draft.clone();
    changed.publication.revision = publication.revision;
    let changed = f.enable(changed, first.revision);
    assert_eq!(f.admit(event())[0].run_id.as_ref(), Some(&original));
    assert_eq!(
        f.owner
            .store
            .neutral_run(&original)
            .unwrap()
            .request
            .publication
            .revision,
        1
    );
    let mut second = f.draft.clone();
    second.id = "binding-b".into();
    let second = f
        .workflows
        .save_neutral_trigger_binding("/project", 0, second, &f.catalog, &f.handlers)
        .unwrap();
    assert!(
        f.workflows
            .enable_neutral_trigger_binding(
                "/project",
                &second.draft.id,
                second.revision,
                true,
                &BTreeMap::new(),
                &f.catalog,
                &f.handlers
            )
            .unwrap_err()
            .contains("overlapping")
    );
    let preview = f
        .workflows
        .preview_neutral_trigger_mapping(
            "/project",
            &second.draft,
            &event(),
            &f.catalog,
            &f.handlers,
        )
        .unwrap();
    assert_eq!(
        preview.overlaps,
        BTreeMap::from([("binding-a".into(), changed.revision)])
    );
    f.workflows
        .enable_neutral_trigger_binding(
            "/project",
            &second.draft.id,
            second.revision,
            true,
            &preview.overlaps,
            &f.catalog,
            &f.handlers,
        )
        .unwrap();
    f.complete_empty_poll(
        &f.workflows
            .neutral_trigger_bindings("/project")
            .unwrap()
            .into_iter()
            .find(|b| b.draft.id == second.draft.id)
            .unwrap(),
    );
    let both = f.admit(event());
    assert_eq!(both.len(), 2);
    assert_ne!(both[0].run_id, both[1].run_id);
}

// Catches: graph-supplied replay grants, replay repinning to a policy edit, and
// duplicated replay requests starting another execution after restart.
#[test]
fn replay_requires_project_authority_and_retains_original_start_request() {
    let f = fixture();
    f.enable(f.draft.clone(), 0);
    let original = f.admit(event())[0].run_id.clone().unwrap();
    let source = event();
    assert!(
        f.owner
            .replay_trigger(
                &f.workflows,
                &f.catalog,
                &f.handlers,
                "/project",
                "binding-a",
                &source.resource,
                &source.generation,
                "operator-replay",
                &ReplayAuthority::authenticated_owner("/other")
            )
            .is_err()
    );
    let replay = || {
        f.owner
            .replay_trigger(
                &f.workflows,
                &f.catalog,
                &f.handlers,
                "/project",
                "binding-a",
                &source.resource,
                &source.generation,
                "operator-replay",
                &ReplayAuthority::authenticated_owner("/project"),
            )
            .unwrap()
    };
    let first = replay()[0].run_id.clone().unwrap();
    assert_ne!(first, original);
    assert_eq!(replay()[0].run_id.as_ref(), Some(&first));
    let reopened = super::RunStore::open_at(&f._dir.path().join("runs.sqlite3")).unwrap();
    assert_eq!(
        reopened.neutral_run(&first).unwrap().inputs["initial"],
        json!(1347)
    );
}

// Catches F3: mixed-case repository selectors silently missing events and a
// missing slash admitting sibling repositories such as example-docs.
#[test]
fn resource_prefix_selects_exactly_its_repository_in_any_case() {
    for prefix in ["github:Octo-Org/Example/", "github:octo-org/example"] {
        let f = fixture();
        let mut draft = f.draft.clone();
        draft.resource_prefix = prefix.into();
        let binding = f.enable(draft, 0);
        assert_eq!(binding.draft.resource_prefix, "github:octo-org/example/");
        let mut sibling = event();
        sibling.resource = "github:octo-org/example-docs/issue/1347".into();
        sibling
            .values
            .insert("repository".into(), json!("octo-org/example-docs"));
        assert_eq!(
            f.admit(sibling),
            [] as [crate::workflows::trigger::TriggerAdmission; 0]
        );
        assert!(f.admit(event())[0].run_id.is_some());
        let mut invalid = f.draft.clone();
        invalid.id = "invalid-prefix".into();
        invalid.resource_prefix = "github:octo-org/example/issue/".into();
        assert!(
            f.workflows
                .save_neutral_trigger_binding("/project", 0, invalid, &f.catalog, &f.handlers)
                .is_err()
        );
    }
}
