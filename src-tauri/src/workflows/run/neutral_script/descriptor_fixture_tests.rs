use super::fixtures::{ECHO, descriptor, policy};
use crate::workflows::catalog::ReusableTarget;
use crate::workflows::command::Invocation;
use crate::workflows::run::neutral_execution_tests::fixture_with;

// Catches: the backend's published descriptor wire format drifting away from the
// fixture handed to the generic Designer, even if execution still accepts it.
#[test]
fn published_custom_script_descriptor_matches_shared_renderer_fixture() {
    let (_dir, _owner, run, _) = fixture_with(30, false, |definition, catalog, _| {
        let mut descriptor = descriptor(ECHO, &policy(false));
        // A publication pins its original interpreter; rendering on another OS
        // must not rewrite it to the current host's executable spelling.
        if let Some(ReusableTarget::Script {
            invocation: Invocation::Shell { program, .. },
        }) = &mut descriptor.target
        {
            *program = "python3".into();
        }
        definition.instances[0].descriptor = descriptor.id.clone();
        catalog.register(descriptor).unwrap();
    });
    let published = serde_json::to_value(&run.run.closure.descriptors[0]).unwrap();
    if std::env::var_os("TUIC_CAPTURE_SCRIPT_DESCRIPTOR").is_some() {
        // Explicit capture mode emits backend serialization only; this mode is
        // an artifact generator, not evidence that the shared fixture matches.
        println!(
            "TUIC_SCRIPT_DESCRIPTOR={}",
            serde_json::to_string(&published).unwrap()
        );
        return;
    }
    let bytes = std::fs::read("src/workflows/fixtures/custom_script_descriptor.json")
        .expect("shared descriptor fixture; generate with TUIC_CAPTURE_SCRIPT_DESCRIPTOR=1");
    let fixture: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
    assert_eq!(
        fixture, published,
        "update the Designer contract and captured fixture together"
    );
}
