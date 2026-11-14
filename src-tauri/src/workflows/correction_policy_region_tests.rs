use super::*;
use serde_json::json;
use std::collections::BTreeMap;

// Catches accepting an uncounted body, a caller report, changed policy or
// state outputs bypassing the selected native assessment.
#[test]
fn selected_policy_body_requires_owned_report_same_policy_and_assessment_outputs() {
    let policy = ReviewAdmissionPolicy {
        scope: "review".into(),
        evidence: Default::default(),
    };
    let mut catalog = super::super::native_catalog::native_operation_catalog().unwrap();
    let review = catalog
        .descriptors()
        .find(|d| d.handler.name == "native.review.bounded")
        .unwrap()
        .clone();
    let mut role = review.clone();
    role.id.name = "source-role".into();
    role.handler.name = "host.agent.role".into();
    role.config.fields.insert(
        "role".into(),
        FieldSchema {
            value_type: ValueType::String,
            required: true,
            default: None,
        },
    );
    let mut assess = review.clone();
    assess.id.name = "policy-assessment".into();
    assess.handler.name = "native.correction.policy.assess".into();
    catalog.register(role.clone()).unwrap();
    catalog.register(assess.clone()).unwrap();
    let instance = |d: &ActivityDescriptor| ActivityInstance {
        descriptor: d.id.clone(),
        execution_policy: "native".into(),
        config: BTreeMap::from([("policy".into(), json!(policy))]),
    };
    let mut role_instance = instance(&role);
    role_instance
        .config
        .insert("role".into(), json!("reviewer"));
    let instances = vec![instance(&review), role_instance, instance(&assess)];
    let handlers = catalog.descriptors().map(|d| d.handler.clone()).collect();
    let mut body:NeutralDefinition=serde_json::from_value(json!({
        "control":{"nodes":{},"edges":{}},"outputs":super::super::correction_region::correction_state_schema(),
        "outcomes":["assessed"],"inputs":{"fields":{}},"subject":null,"instances":[],"input_bindings":{},
        "conditions":{},"loops":{},"collections":{},"forks":{},"output_bindings":{},"subflows":[]
    })).unwrap();
    body.instances = instances;
    body.input_bindings.insert(
        2,
        BTreeMap::from([(
            "report".into(),
            DataSource::ActivityOutput {
                activity: 1,
                field: "report".into(),
            },
        )]),
    );
    body.output_bindings = ["state", "repeat"]
        .into_iter()
        .map(|f| {
            (
                f.into(),
                DataSource::ActivityOutput {
                    activity: 2,
                    field: f.into(),
                },
            )
        })
        .collect();
    let region = CorrectionPolicyRegion {
        region: CorrectionRegion {
            max_corrections: 1,
            criteria: super::super::correction::CorrectionPolicy {
                criteria: BTreeSet::from(["independent_review".into()]),
            },
            selected: body.instances[0].clone(),
        },
        policy,
    };
    let publication = NeutralPublication {
        format_version: 1,
        id: "selected".into(),
        project: "/project".into(),
        revision: 1,
        pins: CatalogClosure::from_instances(&catalog, &body.instances, &handlers).unwrap(),
        definition: body,
        subflow_pins: BTreeSet::new(),
    };
    region.inspect(&publication, &catalog).unwrap();
    let mut changed = publication.clone();
    changed.definition.instances[0]
        .config
        .get_mut("policy")
        .unwrap()["evidence"]["maxReviews"] = json!(3);
    assert!(
        region
            .inspect(&changed, &catalog)
            .unwrap_err()
            .contains("policy differs")
    );
    let mut changed = publication.clone();
    changed
        .definition
        .input_bindings
        .get_mut(&2)
        .unwrap()
        .insert(
            "report".into(),
            DataSource::Constant {
                value: json!({"pass":true}),
            },
        );
    assert!(
        region
            .inspect(&changed, &catalog)
            .unwrap_err()
            .contains("owned role report")
    );
    let mut changed = publication.clone();
    changed.definition.output_bindings.insert(
        "state".into(),
        DataSource::WorkflowInput {
            field: "state".into(),
        },
    );
    assert!(
        region
            .inspect(&changed, &catalog)
            .unwrap_err()
            .contains("bypasses")
    );
    let mut changed = publication;
    changed.definition.instances[0].descriptor = catalog
        .descriptors()
        .find(|d| d.handler.name == "native.review.change")
        .unwrap()
        .id
        .clone();
    assert!(
        region
            .inspect(&changed, &catalog)
            .unwrap_err()
            .contains("uncounted")
    );
}

// Catches a policy region whose review budget starves a scheduled round (each round
// spends a reviewer-role and a native review admission) and a stale 16 cap that
// refuses the plan workflow's 8 rounds (Boss Q8=A).
#[test]
fn policy_region_budget_needs_two_admissions_per_round_up_to_32() {
    let policy = |max_reviews: u16, max_repairs: u16| ReviewAdmissionPolicy {
        scope: "delivery".into(),
        evidence: crate::workflows::run::evidence::EvidencePolicy {
            max_reviews,
            max_repairs,
            ..Default::default()
        },
    };
    let budget = super::review_budget;
    budget(&policy(4, 1), 1).unwrap();
    budget(&policy(8, 3), 3).unwrap();
    budget(&policy(18, 8), 8).unwrap();
    budget(&policy(32, 15), 15).unwrap();
    assert!(
        budget(&policy(3, 1), 1)
            .unwrap_err()
            .contains("2 review admissions per round")
    );
    assert!(
        budget(&policy(17, 8), 8)
            .unwrap_err()
            .contains("2 review admissions per round")
    );
    assert!(
        budget(&policy(33, 15), 15)
            .unwrap_err()
            .contains("evidence policy bounds")
    );
    assert!(budget(&policy(8, 3), 2).unwrap_err().contains("maxRepairs"));
}
