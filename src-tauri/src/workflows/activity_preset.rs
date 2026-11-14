//! Durable activity configurations, not publication or execution authority.

// DEFERRED (2026-11-10) — contract entry points await the 004-2809 runtime consumer.
#![cfg_attr(
    not(test),
    expect(dead_code, reason = "002-eeb6 contract; first consumer 004-2809")
)]

use super::WorkflowStore;
use super::catalog::{
    ActivityInstance, CatalogClosure, DescriptorPin, HandlerRef, WorkflowCatalog, content_digest,
};
use rusqlite::{OptionalExtension, TransactionBehavior, params};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::BTreeSet;

pub(super) const SCHEMA: &str = "CREATE TABLE IF NOT EXISTS activity_presets (
    project TEXT NOT NULL,
    id TEXT NOT NULL,
    revision INTEGER NOT NULL CHECK(revision > 0),
    document TEXT NOT NULL,
    sha256 TEXT NOT NULL,
    PRIMARY KEY(project,id,revision)
);";
const FORMAT_VERSION: u32 = 1;

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct ActivityPreset {
    format_version: u32,
    project: String,
    id: String,
    revision: i64,
    instance: ActivityInstance,
    descriptor_pin: DescriptorPin,
}

impl WorkflowStore {
    pub(crate) fn save_activity_preset(
        &self,
        project: &str,
        id: &str,
        expected_revision: i64,
        instance: ActivityInstance,
        catalog: &WorkflowCatalog,
        handlers: &BTreeSet<HandlerRef>,
    ) -> Result<ActivityPreset, String> {
        if !crate::fs::is_absolute_on_any_platform(project)
            || project.len() > 4096
            || id.is_empty()
            || id.len() > 200
            || !id
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'.' | b'_' | b'-'))
        {
            return Err("invalid activity preset project or id".into());
        }
        let revision = expected_revision
            .checked_add(1)
            .filter(|r| *r > 0)
            .ok_or("invalid activity preset revision")?;
        let closure =
            CatalogClosure::from_instances(catalog, std::slice::from_ref(&instance), handlers)?;
        let descriptor_pin = closure
            .pins()
            .iter()
            .next()
            .ok_or("missing activity descriptor pin")?
            .clone();
        let preset = ActivityPreset {
            format_version: FORMAT_VERSION,
            project: project.into(),
            id: id.into(),
            revision,
            instance,
            descriptor_pin,
        };
        let mut conn = self.connect()?;
        let tx = conn
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .map_err(|e| format!("begin activity preset: {e}"))?;
        let current: i64 = tx
            .query_row(
                "SELECT COALESCE(MAX(revision),0) FROM activity_presets WHERE project=?1 AND id=?2",
                params![project, id],
                |row| row.get(0),
            )
            .map_err(|e| format!("read activity preset revision: {e}"))?;
        if current != expected_revision {
            return Err("stale activity preset revision".into());
        }
        let digest = content_digest(&preset)?;
        let document =
            serde_json::to_string(&preset).map_err(|e| format!("encode activity preset: {e}"))?;
        tx.execute("INSERT INTO activity_presets(project,id,revision,document,sha256) VALUES (?1,?2,?3,?4,?5)", params![project, id, revision, document, digest]).map_err(|e| format!("save activity preset: {e}"))?;
        tx.commit()
            .map_err(|e| format!("commit activity preset: {e}"))?;
        Ok(preset)
    }

    pub(crate) fn get_activity_preset(
        &self,
        project: &str,
        id: &str,
        revision: i64,
        catalog: &WorkflowCatalog,
        handlers: &BTreeSet<HandlerRef>,
    ) -> Result<ActivityPreset, String> {
        let row: Option<(String, String)> = self.connect()?.query_row("SELECT document,sha256 FROM activity_presets WHERE project=?1 AND id=?2 AND revision=?3", params![project, id, revision], |row| Ok((row.get(0)?, row.get(1)?))).optional().map_err(|e| format!("read activity preset: {e}"))?;
        let (document, digest) = row.ok_or("activity preset not found")?;
        let value: Value =
            serde_json::from_str(&document).map_err(|e| format!("decode activity preset: {e}"))?;
        if value.get("format_version").and_then(Value::as_u64) != Some(u64::from(FORMAT_VERSION)) {
            return Err("unsupported activity preset format".into());
        }
        let preset: ActivityPreset =
            serde_json::from_value(value).map_err(|e| format!("decode activity preset: {e}"))?;
        if preset.project != project
            || preset.id != id
            || preset.revision != revision
            || content_digest(&preset)? != digest
        {
            return Err("activity preset identity or content mismatch".into());
        }
        catalog.resolve_pinned(&preset.descriptor_pin, handlers)?;
        let closure = CatalogClosure::from_instances(
            catalog,
            std::slice::from_ref(&preset.instance),
            handlers,
        )?;
        if !closure.pins().contains(&preset.descriptor_pin) {
            return Err("activity preset descriptor pin mismatch".into());
        }
        Ok(preset)
    }

    pub(crate) fn apply_activity_preset(
        &self,
        project: &str,
        id: &str,
        revision: i64,
        catalog: &WorkflowCatalog,
        handlers: &BTreeSet<HandlerRef>,
    ) -> Result<ActivityInstance, String> {
        Ok(self
            .get_activity_preset(project, id, revision, catalog, handlers)?
            .instance)
    }
}
