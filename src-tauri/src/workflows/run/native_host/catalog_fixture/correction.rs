//! Capture retained loop admission, without executing native assessment.
use super::*;
use crate::workflows::correction::CorrectionPolicy;
use crate::workflows::correction_region::CorrectionRegion;
use crate::workflows::neutral_control::ControlNode;
use crate::workflows::run::neutral::{NeutralLimits, NeutralStartRequest};
use crate::workflows::run::runtime::RuntimeOwner;

pub(super) fn capture(
    root: &std::path::Path,
    store: &WorkflowStore,
    host: &NativeHost,
    catalog: &mut WorkflowCatalog,
) -> Value {
    let registry = crate::workflows::run::neutral_runtime::NeutralHost::registry(host);
    let handlers = registry.available_handlers();
    let context = host.publication_context(PROJECT).unwrap();
    let native = catalog
        .descriptors()
        .find(|d| d.handler.name == "native.correction.assess")
        .unwrap()
        .clone();
    let mut body = definition(&native);
    body.outcomes = BTreeSet::from(["assessed".into()]);
    body.control.nodes.insert(
        "end".into(),
        ControlNode::End {
            outcome: "assessed".into(),
        },
    );
    let child = NeutralAdmissionService {
        store,
        catalog,
        registry: &registry,
    }
    .publish(
        Some(context.as_ref()),
        PROJECT,
        "selected-correction",
        0,
        body,
    )
    .unwrap();
    let mut selected = native;
    selected.id = DescriptorId {
        namespace: "user".into(),
        name: "selected-correction".into(),
        version: 1,
    };
    selected.handler = HandlerRef {
        name: "host.subflow".into(),
        version: 1,
    };
    selected.purpose = "Inspect an exact selected correction publication".into();
    selected.target = Some(ReusableTarget::Subflow {
        publication: PublicationPin {
            reference: PublicationRef {
                project: PROJECT.into(),
                id: child.id.clone(),
                revision: child.revision,
            },
            sha256: content_digest(&child).unwrap(),
        },
    });
    store
        .save_reusable_operation(PROJECT, 0, selected.clone(), catalog, &registry)
        .unwrap();
    catalog.register(selected.clone()).unwrap();
    let region = CorrectionRegion {
        max_corrections: 2,
        criteria: CorrectionPolicy {
            criteria: BTreeSet::from(["independent_review".into()]),
        },
        selected: ActivityInstance {
            descriptor: selected.id.clone(),
            config: BTreeMap::new(),
            execution_policy: "native".into(),
        },
    };
    let reply = crate::workflows::neutral_api::correction_reply(region.clone(), catalog).unwrap();
    let wrapper = region.definition(catalog).unwrap();
    let publication = NeutralAdmissionService {
        store,
        catalog,
        registry: &registry,
    }
    .publish(
        Some(context.as_ref()),
        PROJECT,
        "bounded-correction",
        0,
        wrapper,
    )
    .unwrap();
    let owner = RuntimeOwner::acquire(&root.join("correction-runs.sqlite3")).unwrap();
    let request = NeutralStartRequest {
        publication: PublicationRef {
            project: PROJECT.into(),
            id: publication.id,
            revision: publication.revision,
        },
        request_id: "inspect-correction".into(),
        inputs: BTreeMap::from([("context".into(), json!({})), ("state".into(), json!({}))]),
        subject: Some(BTreeMap::from([(
            "workspace".into(),
            json!({"reference":"workspace","id":PROJECT}),
        )])),
        limits: NeutralLimits {
            max_activations: 20,
            max_concurrency: 1,
            max_duration_ms: 60_000,
        },
    };
    let started = owner
        .start_neutral(store, &request, catalog, &handlers)
        .unwrap();
    let mut run = owner.store.neutral_execution(&started.id).unwrap();
    let at = run.deadline_ms - 60_000;
    // Entry advance followed by durable scope admission, before any child call.
    for id in ["entry", "scope"] {
        run = crate::workflows::run::neutral_execution_tests::step(&owner, &run, id, at);
    }
    let (snapshot, _events) = owner.store.neutral_inspection(&started.id, 0, 500).unwrap();
    let scope = snapshot
        .active_scope
        .as_ref()
        .expect("owner retained correction scope");
    let crate::workflows::run::neutral_scopes::NeutralScopeKind::Loop { iteration, .. } =
        scope.kind
    else {
        panic!("correction scope is not a loop");
    };
    assert_eq!(iteration, 1);
    assert_eq!(scope.identity.scope, "correction");
    let selected_publication = store
        .get_neutral_publication(PROJECT, &child.id, child.revision, catalog, &handlers)
        .and_then(|p| store.neutral_publication_view(p))
        .map(NeutralReply::Publication)
        .unwrap();
    json!({"reply":reply,"selected_publication":selected_publication,
        "inspection":{"publication":snapshot.publication.id,
            "active_scope":{"identity":{"scope":scope.identity.scope,"generation":scope.identity.generation},
                "kind":{"type":"loop","iteration":iteration}}}})
}
