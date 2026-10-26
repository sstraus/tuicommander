use super::*;

// Catches: template generation persisting/enabling a schedule, losing selected
// ownership inputs, sharing ids, or inheriting an executable precheck.
#[test]
fn template_drafts_are_disabled_valid_and_only_persist_after_explicit_save() {
    let (dir, definitions, owner) = setup();
    let runs = owner.store();
    let catalogue = call(&definitions, runs, json!({"action":"templates"})).unwrap();
    assert_eq!(catalogue.as_array().unwrap().len(), 4);
    let mut ids = std::collections::HashSet::new();
    for (template, cron, first) in [
        ("weekday_audit", "0 9 * * 1-5", "2026-10-12T07:00:00Z"),
        ("release_readiness", "0 9 * * 1", "2026-10-12T07:00:00Z"),
        ("daily_review", "0 9 * * *", "2026-10-10T07:00:00Z"),
        ("hourly_maintenance", "0 * * * *", "2026-10-10T06:00:00Z"),
    ] {
        assert!(
            catalogue
                .as_array()
                .unwrap()
                .iter()
                .any(|item| item["id"] == template)
        );
        for _ in 0..2 {
            let draft = call(
                &definitions,
                runs,
                json!({"action":"template",
                "template_id":template,"repository":"/selected/repo","run_config":"selected-config",
                "timezone":"Europe/Madrid"}),
            )
            .unwrap();
            let model: AutomationDefinition = serde_json::from_value(draft.clone()).unwrap();
            model.validate().unwrap();
            assert!(ids.insert(model.id.clone()));
            assert_eq!(model.repository, "/selected/repo");
            assert_eq!(model.run_config, "selected-config");
            assert_eq!(model.timezone, "Europe/Madrid");
            assert_eq!(model.cron, cron);
            assert!(!model.enabled);
            assert_eq!(model.precheck, None);
            assert_eq!(
                model.workspace,
                crate::automations::model::Workspace::Existing
            );
            assert!(model.prompt.contains("Do not"));
            assert!(model.created_by_session.is_none());
            // Fixed local-zone examples prove schedules are interpreted by the backend.
            let at = "2026-10-10T05:30:00Z".parse().unwrap();
            assert_eq!(
                model
                    .schedule()
                    .unwrap()
                    .next_after(at)
                    .unwrap()
                    .unwrap()
                    .to_rfc3339_opts(chrono::SecondsFormat::Secs, true),
                first
            );
            assert!(!dir.path().join("automations.json").exists());
            assert_eq!(
                runs.history(None, 100, 0).unwrap(),
                [] as [crate::automations::run::AutomationRun; 0]
            );
        }
    }
    let draft = call(
        &definitions,
        runs,
        json!({"action":"template",
        "template_id":"daily_review","repository":"/selected/repo","run_config":"selected-config"}),
    )
    .unwrap();
    let zone = draft["timezone"].as_str().unwrap();
    assert_ne!(zone, "");
    assert!(zone.parse::<chrono_tz::Tz>().is_ok());
    call(
        &definitions,
        runs,
        json!({"action":"create","definition":draft}),
    )
    .unwrap();
    assert_eq!(definitions.load().unwrap().definitions.len(), 1);
    assert_eq!(
        call(&definitions, runs, json!({"action":"list"})).unwrap()[0]["next_run_ms"],
        Value::Null
    );
    assert_eq!(
        runs.history(None, 100, 0).unwrap(),
        [] as [crate::automations::run::AutomationRun; 0]
    );
}

// Catches: unknown recipes or invalid selection inputs being accepted as runnable defaults.
#[test]
fn invalid_template_requests_leave_existing_definitions_and_history_unchanged() {
    let (dir, definitions, owner) = setup();
    let runs = owner.store();
    call(
        &definitions,
        runs,
        json!({"action":"create","definition":definition("existing")}),
    )
    .unwrap();
    let before = std::fs::read(dir.path().join("automations.json")).unwrap();
    let input = json!({"action":"template","template_id":"daily_review",
        "repository":"/selected","run_config":"chosen","timezone":"UTC"});
    for (key, invalid) in [
        ("template_id", "missing"),
        ("repository", " "),
        ("run_config", ""),
        ("timezone", "invalid"),
    ] {
        let mut bad = input.clone();
        bad[key] = json!(invalid);
        assert!(call(&definitions, runs, bad).is_err());
    }
    let mut bad = input;
    bad["enabled"] = json!(true);
    assert!(call(&definitions, runs, bad).is_err());
    assert_eq!(
        std::fs::read(dir.path().join("automations.json")).unwrap(),
        before
    );
    assert_eq!(
        runs.history(None, 100, 0).unwrap(),
        [] as [crate::automations::run::AutomationRun; 0]
    );
}
