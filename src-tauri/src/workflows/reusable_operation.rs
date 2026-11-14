//! Immutable project-owned operation descriptors. Saving does not authorize a
//! handler or execute a script/subflow. Native descriptors remain host-owned.

// DEFERRED (2026-11-10) — contract entry points await the 004-2809 runtime consumer.
#![cfg_attr(
    not(test),
    expect(dead_code, reason = "002-eeb6 contract; first consumer 004-2809")
)]

use super::WorkflowStore;
use super::catalog::{
    ActivityDescriptor, DescriptorId, ReusableTarget, WorkflowCatalog, content_digest,
};
use super::neutral_admission::HostRegistry;
use super::neutral_publication::check_subflow_interface;
use rusqlite::{OptionalExtension, TransactionBehavior, params};
use serde::{Deserialize, Serialize};

pub(super) const SCHEMA: &str = "CREATE TABLE IF NOT EXISTS reusable_operations (
    project TEXT NOT NULL,
    namespace TEXT NOT NULL,
    name TEXT NOT NULL,
    version INTEGER NOT NULL CHECK(version > 0),
    document TEXT NOT NULL,
    sha256 TEXT NOT NULL,
    PRIMARY KEY(project,namespace,name,version)
);";

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct SavedOperation {
    format_version: u32,
    project: String,
    descriptor: ActivityDescriptor,
}

impl WorkflowStore {
    fn check_reusable_operation(
        &self,
        project: &str,
        descriptor: &ActivityDescriptor,
        catalog: &WorkflowCatalog,
        registry: &HostRegistry<'_>,
    ) -> Result<(), String> {
        if !crate::fs::is_absolute_on_any_platform(project) || project.len() > 4096 {
            return Err("invalid reusable operation project".into());
        }
        registry.check_reusable_handler(descriptor)?;
        if catalog
            .get(&descriptor.id)
            .is_some_and(|existing| existing != descriptor)
        {
            return Err("reusable operation descriptor collision".into());
        }
        let handlers = registry.available_handlers();
        // Validate the complete descriptor without replacing a host entry.
        let mut isolated = WorkflowCatalog::new();
        isolated.register(descriptor.clone())?;
        isolated.resolve_pinned(&isolated.pin(&descriptor.id)?, &handlers)?;
        match descriptor
            .target
            .as_ref()
            .ok_or("native operations are host-owned")?
        {
            ReusableTarget::Script { .. } => Ok(()),
            ReusableTarget::Http {
                connector,
                plugin,
                base,
            } => super::run::HttpConnector::new(
                descriptor.clone(),
                connector.clone(),
                plugin.clone(),
                base.clone(),
            )
            .map(|_| ()),
            ReusableTarget::Subflow { publication } => {
                if publication.reference.project != project {
                    return Err("cross-project reusable subflow is not supported".into());
                }
                let child = self.get_neutral_publication(
                    project,
                    &publication.reference.id,
                    publication.reference.revision,
                    catalog,
                    &handlers,
                )?;
                if content_digest(&child)? != publication.sha256 {
                    return Err("reusable subflow content mismatch".into());
                }
                check_subflow_interface(descriptor, &child)?;
                if descriptor.handler.name == super::run::native_cohort::HANDLER {
                    super::native_child::receipt_contract(&child, catalog)?;
                    // A dynamic dispatch has no pinned snapshot; its run-time cohort is
                    // admitted against this run's creation receipts.
                    if let Some(field) = descriptor.config.fields.get("cohort") {
                        let cohort: super::run::native_cohort::Cohort = serde_json::from_value(
                            field
                                .default
                                .clone()
                                .ok_or("story dispatch cohort snapshot missing")?,
                        )
                        .map_err(|e| e.to_string())?;
                        cohort.validate()?;
                    }
                }
                Ok(())
            }
        }
    }

    /// Descriptor version is the immutable revision. Create at expected 0; each
    /// update must name the current version and insert exactly its successor.
    pub(crate) fn save_reusable_operation(
        &self,
        project: &str,
        expected_version: u32,
        descriptor: ActivityDescriptor,
        catalog: &WorkflowCatalog,
        registry: &HostRegistry<'_>,
    ) -> Result<(), String> {
        if expected_version.checked_add(1) != Some(descriptor.id.version) {
            return Err("invalid reusable operation version".into());
        }
        if catalog.get(&descriptor.id).is_some() {
            return Err("reusable operation descriptor collision".into());
        }
        self.check_reusable_operation(project, &descriptor, catalog, registry)?;
        let record = SavedOperation {
            format_version: 1,
            project: project.into(),
            descriptor,
        };
        let id = &record.descriptor.id;
        let mut conn = self.connect()?;
        let tx = conn
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .map_err(|e| format!("begin reusable operation: {e}"))?;
        let current: u32 = tx.query_row(
            "SELECT COALESCE(MAX(version),0) FROM reusable_operations WHERE project=?1 AND namespace=?2 AND name=?3",
            params![project, id.namespace, id.name], |row| row.get(0),
        ).map_err(|e| format!("read reusable operation version: {e}"))?;
        if current != expected_version {
            return Err("stale reusable operation version".into());
        }
        let document = serde_json::to_string(&record)
            .map_err(|e| format!("encode reusable operation: {e}"))?;
        tx.execute(
            "INSERT INTO reusable_operations(project,namespace,name,version,document,sha256) VALUES (?1,?2,?3,?4,?5,?6)",
            params![project, id.namespace, id.name, id.version, document, content_digest(&record)?],
        ).map_err(|e| format!("save reusable operation: {e}"))?;
        tx.commit()
            .map_err(|e| format!("commit reusable operation: {e}"))
    }

    /// Load an exact version for registration in a host-built project catalog.
    /// The caller does not get native registration or dispatch authority.
    pub(crate) fn get_reusable_operation(
        &self,
        project: &str,
        id: &DescriptorId,
        catalog: &WorkflowCatalog,
        registry: &HostRegistry<'_>,
    ) -> Result<ActivityDescriptor, String> {
        let row: Option<(String, String)> = self.connect()?.query_row(
            "SELECT document,sha256 FROM reusable_operations WHERE project=?1 AND namespace=?2 AND name=?3 AND version=?4",
            params![project, id.namespace, id.name, id.version], |row| Ok((row.get(0)?, row.get(1)?)),
        ).optional().map_err(|e| format!("read reusable operation: {e}"))?;
        let (document, digest) = row.ok_or("reusable operation not found")?;
        let descriptor = decode_operation(project, id, &document, &digest)?;
        self.check_reusable_operation(project, &descriptor, catalog, registry)?;
        Ok(descriptor)
    }

    /// Build one backend-owned project catalog. Register all verified rows before
    /// resolving subflow targets, so dependency order is not lexicographic order.
    /// Host entries are never replaced, even by byte-identical stored entries.
    pub(crate) fn build_project_catalog(
        &self,
        project: &str,
        host_catalog: &WorkflowCatalog,
        registry: &HostRegistry<'_>,
    ) -> Result<WorkflowCatalog, String> {
        if !crate::fs::is_absolute_on_any_platform(project) || project.len() > 4096 {
            return Err("invalid reusable operation project".into());
        }
        let conn = self.connect()?;
        let mut query = conn.prepare(
            "SELECT namespace,name,version,document,sha256 FROM reusable_operations WHERE project=?1 ORDER BY namespace,name,version",
        ).map_err(|e| format!("prepare project operations: {e}"))?;
        let rows = query
            .query_map([project], |row| {
                Ok((
                    DescriptorId {
                        namespace: row.get(0)?,
                        name: row.get(1)?,
                        version: row.get(2)?,
                    },
                    row.get::<_, String>(3)?,
                    row.get::<_, String>(4)?,
                ))
            })
            .map_err(|e| format!("read project operations: {e}"))?;
        let mut catalog = host_catalog.clone();
        let mut operations = Vec::new();
        for row in rows {
            let (id, document, digest) = row.map_err(|e| format!("read project operation: {e}"))?;
            let descriptor = decode_operation(project, &id, &document, &digest)?;
            registry.check_reusable_handler(&descriptor)?;
            catalog.register(descriptor.clone())?;
            operations.push(descriptor);
        }
        for descriptor in &operations {
            self.check_reusable_operation(project, descriptor, &catalog, registry)?;
        }
        Ok(catalog)
    }
}

fn decode_operation(
    project: &str,
    id: &DescriptorId,
    document: &str,
    digest: &str,
) -> Result<ActivityDescriptor, String> {
    let record: SavedOperation =
        serde_json::from_str(document).map_err(|e| format!("decode reusable operation: {e}"))?;
    if record.format_version != 1
        || record.project != project
        || &record.descriptor.id != id
        || content_digest(&record)? != digest
    {
        return Err("reusable operation identity or content mismatch".into());
    }
    Ok(record.descriptor)
}
