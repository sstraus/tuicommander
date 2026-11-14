use super::correction_fixture::fixture;
use crate::workflows::catalog::WorkflowCatalog;
use crate::workflows::correction_region::CorrectionRegion;
use crate::workflows::neutral_api::model::NeutralAction;
use serde_json::json;

// Catches: a wire action hiding the selected scope, losing its correction limit,
// or offering success on exhaustion when a client saves the returned definition.
#[tokio::test]
async fn correction_action_roundtrips_inspectable_pins_budget_and_operator_exit() {
    let (_dir, _owner, run, _) = fixture(2);
    let selected = run
        .run
        .closure
        .descriptors
        .iter()
        .find(|d| d.id.name == "selected-region")
        .unwrap()
        .clone();
    let mut catalog = WorkflowCatalog::new();
    catalog.register(selected.clone()).unwrap();
    let instance = run
        .run
        .closure
        .children
        .iter()
        .find(|p| p.id == "bounded-correction")
        .unwrap()
        .definition
        .instances[0]
        .clone();
    let wire = json!({"action":"correction_region", "region":{"max_corrections":2, "criteria":crate::workflows::correction_tests::policy(), "selected":instance}});
    let action: NeutralAction = serde_json::from_value(wire.clone()).unwrap();
    let NeutralAction::CorrectionRegion { region } = action else {
        panic!("wrong action")
    };
    assert_eq!(
        serde_json::to_value(NeutralAction::CorrectionRegion {
            region: region.clone()
        })
        .unwrap(),
        wire
    );
    let config = tempfile::tempdir_in(crate::test_support::test_temp_root()).unwrap();
    let _guard = crate::config::set_config_dir_override(config.path().to_path_buf());
    let state = std::sync::Arc::new(crate::state::tests_support::make_test_app_state());
    state
        .workflow_runtime
        .install_neutral_catalog_host(std::sync::Arc::new(InspectHost(catalog)));
    let project = config.path().canonicalize().unwrap();
    let reply = serde_json::to_value(
        crate::workflows::neutral_api::neutral_action(
            &state,
            project.to_str().unwrap(),
            NeutralAction::CorrectionRegion { region },
            None,
        )
        .unwrap(),
    )
    .unwrap();
    assert_eq!(reply["type"], "correction_region");
    let graph = &reply["value"]["definition"];
    assert_eq!(graph["loops"]["correction"]["max_iterations"], 3);
    assert_eq!(graph["instances"].as_array().unwrap().len(), 1);
    assert_eq!(
        graph["control"]["edges"]["correction"]["exhausted"],
        "operator"
    );
    assert_eq!(graph["subflows"].as_array().unwrap().len(), 1);
    let schema = serde_json::to_value(schemars::schema_for!(NeutralAction)).unwrap();
    assert!(schema.to_string().contains("correction_region"));
    let mut invalid = wire;
    invalid["region"]["passed"] = json!(true);
    assert!(serde_json::from_value::<NeutralAction>(invalid).is_err());
    let _typed: CorrectionRegion =
        serde_json::from_value(reply["value"]["region"].clone()).unwrap();
}

// Authoring-only catalog, with no executable adapter or fabricated service result.
struct InspectHost(WorkflowCatalog);
impl super::neutral_runtime::NeutralHost for InspectHost {
    fn context(
        &self,
        _: &super::neutral_runtime::NeutralInvocation,
    ) -> Result<Box<dyn crate::workflows::neutral_admission::HostContext>, String> {
        Err("inspection only".into())
    }
    fn registry(&self) -> crate::workflows::neutral_admission::HostRegistry<'_> {
        Default::default()
    }
    fn prepare(
        &self,
        _: &super::neutral_runtime::NeutralInvocation,
    ) -> Result<super::neutral_effects::EffectPreparation, String> {
        Err("inspection only".into())
    }
    fn invoke_effect(
        &self,
        _: super::neutral_effects::NeutralEffect,
    ) -> super::neutral_runtime::ObservationFuture {
        Box::pin(async { Err("inspection only".into()) })
    }
}
impl crate::workflows::neutral_host::NeutralCatalogHost for InspectHost {
    fn catalog(&self, _: &str) -> Result<WorkflowCatalog, String> {
        Ok(self.0.clone())
    }
    fn publication_context(
        &self,
        _: &str,
    ) -> Result<Box<dyn crate::workflows::neutral_admission::HostContext>, String> {
        Err("inspection only".into())
    }
}
