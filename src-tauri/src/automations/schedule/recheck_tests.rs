use super::Schedule;

#[test]
fn missing_cron_list_entries_are_not_silently_saved() {
    // A missing entry in an edited custom schedule must be visible, not saved
    // as a different cadence that silently omits the intended run.
    for expression in ["0,,30 9 * * 1-5", ",15 9 * * 1-5", "15, 9 * * 1-5"] {
        assert!(
            Schedule::parse(expression, "UTC").is_err(),
            "malformed schedule was accepted: {expression}"
        );
    }
}
