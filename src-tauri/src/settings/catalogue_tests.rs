use super::super::native::{native_catalogue, native_catalogue_for};
use super::*;
use serde_json::json;

// Catches: missing unset fields. Serialization omits unset options in several stores.
#[test]
fn unannotated_optional_field_fails() {
    let schema = schema::<crate::config::AppConfig>();
    assert!(
        schema["properties"]["shell"].get("x-setting").is_some(),
        "unset shell needs mandatory catalogue metadata, independent of serialization"
    );
    let mut schema = schema;
    schema["properties"]["shell"]
        .as_object_mut()
        .unwrap()
        .remove("x-setting");
    let mut catalogue = Catalogue::default();
    let err = catalogue
        .register_schema("app", "machine", &schema, None, Availability::Available)
        .unwrap_err();
    assert_eq!(err.path, "/shell");
    assert!(
        catalogue.fields.is_empty(),
        "failed registration must not publish a partial catalogue"
    );
}

// Catches: unsafe defaults. Missing sensitivity never falls back to public.
#[test]
fn missing_sensitivity_fails() {
    let schema = schema::<crate::config::AuthConfig>();
    assert_eq!(
        schema["properties"]["password_hash"]["x-setting"]["sensitivity"], "derived_secret",
        "password hashes must never inherit public sensitivity"
    );
    let mut schema = schema;
    schema["properties"]["password_hash"]["x-setting"]
        .as_object_mut()
        .unwrap()
        .remove("sensitivity");
    let err = Catalogue::default()
        .register_schema("auth", "machine", &schema, None, Availability::Available)
        .unwrap_err();
    assert_eq!(err.path, "/password_hash");
}

// Catches: schema descriptions accidentally exposing secret defaults/examples.
#[test]
fn secret_defaults_and_examples_never_leave_catalogue() {
    let mut schema = schema::<crate::config::AuthConfig>();
    schema["properties"]["session_token"]["default"] = json!("secret-canary");
    schema["properties"]["session_token"]["examples"] = json!(["secret-example"]);
    let mut catalogue = Catalogue::default();
    catalogue
        .register_schema(
            "auth",
            "machine",
            &schema,
            Some(&json!({"session_token":"stored-canary"})),
            Availability::Available,
        )
        .unwrap();
    let wire = catalogue.typescript().unwrap();
    for canary in ["secret-canary", "secret-example", "stored-canary"] {
        assert!(!wire.contains(canary));
    }
    assert!(catalogue.fields["auth.session_token"].default.is_none());
}

// Catches: treating an unavailable backend as an empty choices list or omitting its settings.
#[test]
fn native_inventory_preserves_optional_dynamic_and_unavailable_fields() {
    let catalogue = native_catalogue().unwrap();
    assert_eq!(
        catalogue.fields["app.ego_executable"].annotation.apply,
        ApplyMode::NextSession
    );
    assert_eq!(
        catalogue.fields["agents.agents.{key}.ego_mode"].schema["enum"],
        json!(["plan", "default", "edits", "auto", "yolo", null])
    );
    assert!(
        catalogue
            .fields
            .contains_key("upstreams.servers.{id}.auth.client_secret")
    );
    assert!(catalogue.fields.contains_key("prompts.prompts.{id}.text"));
    assert!(catalogue.fields.contains_key("voice.device"));
    assert!(
        catalogue
            .fields
            .contains_key("corrections.replacements.{from}.to")
    );
    #[cfg(not(feature = "dictation"))]
    assert!(matches!(
        catalogue.fields["corrections.replacements.{from}.to"].availability,
        Availability::Unavailable { .. }
    ));
    #[cfg(not(feature = "dictation"))]
    assert!(matches!(
        catalogue.fields["voice.device"].availability,
        Availability::Unavailable { .. }
    ));
    assert!(
        catalogue
            .exclusions
            .iter()
            .any(|e| e.path == "/ai_chat_peer_ids" && !e.reason.is_empty())
    );
}

// Catches: ownerless inventory gaps silently passing the cutover gate.
#[test]
fn inventory_gap_without_owner_fails_and_cutover_refuses_open_gaps() {
    let mut catalogue = native_catalogue().unwrap();
    catalogue.require_complete_inventory().unwrap();
    catalogue.gaps.push(InventoryGap {
        store: "fixture".into(),
        path: "/missing_owner".into(),
        owner_step: "fixture owner".into(),
        reason: "Fixture proves the cutover refuses an unresolved owner".into(),
    });
    assert!(catalogue.require_complete_inventory().is_err());
    let gap = InventoryGap {
        store: "plugin.example".into(),
        path: "/token".into(),
        owner_step: String::new(),
        reason: "Missing plugin schema".into(),
    };
    assert!(gap.validate().is_err());
    catalogue.gaps.clear();
    catalogue.require_complete_inventory().unwrap();
}

// Catches: recursively accepting untyped JSON as complete settings coverage.
#[test]
fn opaque_or_unkeyed_records_are_rejected() {
    let mut schema = schema::<crate::config::PromptLibraryConfig>();
    schema["properties"]["prompts"]
        .as_object_mut()
        .unwrap()
        .remove("x-collection-key");
    assert!(
        Catalogue::default()
            .register_schema(
                "prompts",
                "instance",
                &schema,
                None,
                Availability::Available
            )
            .is_err()
    );
    let mut schema = super::schema::<crate::config::AppConfig>();
    schema["properties"]["shell"] = json!({});
    assert!(
        Catalogue::default()
            .register_schema("app", "machine", &schema, None, Availability::Available)
            .is_err()
    );
}

// The checked-in descriptors are shared by stable, beta and nightly builds.
// Pin only the exported channel defaults; live descriptors retain the build default.
fn generated_typescript(mut catalogue: Catalogue) -> String {
    let channel = catalogue.fields.get_mut("app.update_channel").unwrap();
    channel.default = Some(json!("stable"));
    channel.schema["default"] = json!("stable");
    catalogue.typescript().unwrap()
}

// Catches: either copy of the build-dependent channel default leaking into the export.
#[test]
fn generated_typescript_is_independent_of_build_channel() {
    let expected = generated_typescript(native_catalogue_for(true).unwrap());
    for channel in ["stable", "beta", "nightly"] {
        let mut catalogue = native_catalogue_for(true).unwrap();
        let field = catalogue.fields.get_mut("app.update_channel").unwrap();
        field.default = Some(json!(channel));
        field.schema["default"] = json!(channel);
        assert_eq!(generated_typescript(catalogue), expected, "{channel}");
    }
}

// Catches: checked-in UI descriptors diverging from the Rust schema used by consumers.
#[test]
fn generated_typescript_matches_native_catalogue() {
    let expected = generated_typescript(native_catalogue_for(true).unwrap());
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../src/generated/settingsCatalogue.ts");
    if std::env::var_os("TUIC_UPDATE_SETTINGS_CATALOGUE").is_some() {
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(&path, &expected).unwrap();
    }
    assert_eq!(
        std::fs::read_to_string(path).unwrap(),
        expected,
        "Regenerate with TUIC_UPDATE_SETTINGS_CATALOGUE=1 and this test filter; package version: {}",
        env!("CARGO_PKG_VERSION")
    );
}

// Catches: tagged-union choices disappearing or variants weakening field sensitivity.
#[test]
fn tagged_union_choices_preserve_all_variants_and_metadata() {
    let catalogue = native_catalogue().unwrap();
    let field = &catalogue.fields["upstreams.servers.{id}.transport.type"];
    assert_eq!(field.schema["enum"], json!(["http", "stdio"]));
    match &field.choices {
        Some(Choices::Static { values }) => {
            assert_eq!(values, &vec![json!("http"), json!("stdio")])
        }
        other => panic!("tag choices missing: {other:?}"),
    }
    let mut schema = schema::<crate::config::TlsConfig>();
    for variant in schema["oneOf"].as_array_mut().unwrap() {
        variant["properties"]["mode"]["x-setting"] = json!({
            "sensitivity": if variant["properties"]["mode"]["const"] == "off" {"public"} else {"secret"},
            "apply":"service_restart", "effect":"Selects the TLS mode.", "operations":["get","set"], "confirmationClass":"network_exposure"
        });
    }
    assert!(
        Catalogue::default()
            .register_schema("tls", "machine", &schema, None, Availability::Available)
            .is_err()
    );
}
