//! The schema of `tuic.sqlite3`: every durable store, one migration list.
//!
//! Append a migration to change the layout; never edit a shipped one.

use super::Schema;

pub(crate) const TUIC_SCHEMA: Schema = Schema {
    name: "tuic",
    migrations: &[
        &[
            crate::stories::STORY_SCHEMA,
            crate::stories::EXTERNAL_LEDGER_SCHEMA,
            crate::stories::INGEST_SCHEMA,
            crate::workflows::activity_preset::SCHEMA,
            crate::workflows::reusable_operation::SCHEMA,
            crate::workflows::neutral_api::drafts::SCHEMA,
            crate::workflows::neutral_publication::SCHEMA,
            crate::workflows::trigger_bindings::SCHEMA,
            crate::mcp_http::launch_transaction::SCHEMA,
            crate::mcp_http::mcp_usage_rollup::SCHEMA,
        ],
        &[crate::work_item_visits::SCHEMA],
    ],
};
