//! Binding metadata shares WorkflowStore, separate from legacy kind bindings.
use super::WorkflowStore;
use super::catalog::{HandlerRef, ObjectSchema, WorkflowCatalog};
use super::trigger::*;
use rusqlite::{Connection, TransactionBehavior, params};
use std::collections::{BTreeMap, BTreeSet};

const SCHEMA: &str = "CREATE TABLE IF NOT EXISTS neutral_trigger_bindings (
 project TEXT NOT NULL, id TEXT NOT NULL, revision INTEGER NOT NULL,
 document TEXT NOT NULL, PRIMARY KEY(project,id));";

impl WorkflowStore {
    fn trigger_connection(&self) -> Result<Connection, String> {
        let conn = self.connect()?;
        conn.execute_batch(SCHEMA).map_err(|e| e.to_string())?;
        Ok(conn)
    }
    pub(crate) fn all_neutral_trigger_bindings(
        &self,
    ) -> Result<Vec<NeutralTriggerBinding>, String> {
        let conn = self.trigger_connection()?;
        let mut query = conn
            .prepare("SELECT document FROM neutral_trigger_bindings ORDER BY project,id")
            .map_err(|e| e.to_string())?;
        query
            .query_map([], |r| r.get::<_, String>(0))
            .map_err(|e| e.to_string())?
            .map(|r| {
                serde_json::from_str(&r.map_err(|e| e.to_string())?).map_err(|e| e.to_string())
            })
            .collect()
    }
    pub(crate) fn neutral_trigger_bindings(
        &self,
        project: &str,
    ) -> Result<Vec<NeutralTriggerBinding>, String> {
        Ok(self
            .stored_neutral_trigger_bindings(project)?
            .into_iter()
            .map(super::trigger_state::binding_state)
            .collect())
    }
    pub(crate) fn stored_neutral_trigger_bindings(
        &self,
        project: &str,
    ) -> Result<Vec<NeutralTriggerBinding>, String> {
        read_bindings(&self.trigger_connection()?, project)
    }
    pub(crate) fn save_neutral_trigger_binding(
        &self,
        project: &str,
        expected_revision: i64,
        mut draft: TriggerBindingDraft,
        catalog: &WorkflowCatalog,
        handlers: &BTreeSet<HandlerRef>,
    ) -> Result<NeutralTriggerBinding, String> {
        match draft.source {
            TriggerSource::Github => {
                draft.resource_prefix =
                    super::github_trigger::resource_prefix(&draft.resource_prefix)?
            }
            // DEFERRED (2026-11-11) — GitLab resource-prefix normalization is not
            // defined yet; GitLab drafts keep their pre-010 save behavior.
            TriggerSource::Gitlab => {}
            TriggerSource::Scheduled => {}
        }
        self.validate_trigger_mapping(project, &draft, catalog, handlers)?;
        let mut conn = self.trigger_connection()?;
        let tx = conn
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .map_err(|e| e.to_string())?;
        let old = read_bindings(&tx, project)?
            .into_iter()
            .find(|b| b.draft.id == draft.id);
        if old.as_ref().map_or(0, |b| b.revision) != expected_revision {
            return Err("trigger binding revision changed".into());
        }
        let binding = NeutralTriggerBinding {
            draft,
            revision: expected_revision + 1,
            enabled: false,
            refusal_reason: None,
        };
        write_binding(&tx, project, &binding)?;
        tx.commit().map_err(|e| e.to_string())?;
        Ok(binding)
    }
    #[allow(clippy::too_many_arguments)] // revision fence, overlap acknowledgement and catalog stay explicit.
    pub(crate) fn enable_neutral_trigger_binding(
        &self,
        project: &str,
        id: &str,
        expected_revision: i64,
        enabled: bool,
        acknowledged_overlaps: &BTreeMap<String, i64>,
        catalog: &WorkflowCatalog,
        handlers: &BTreeSet<HandlerRef>,
    ) -> Result<NeutralTriggerBinding, String> {
        let mut conn = self.trigger_connection()?;
        let tx = conn
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .map_err(|e| e.to_string())?;
        let bindings = read_bindings(&tx, project)?;
        let mut binding = bindings
            .iter()
            .find(|b| b.draft.id == id)
            .cloned()
            .ok_or("trigger binding not found")?;
        if binding.revision != expected_revision {
            return Err("trigger binding revision changed".into());
        }
        if enabled {
            self.validate_trigger_mapping(project, &binding.draft, catalog, handlers)?;
            if overlaps(&binding.draft, &bindings) != *acknowledged_overlaps {
                return Err("preview and acknowledge current overlapping trigger bindings".into());
            }
        }
        binding.enabled = enabled;
        binding.revision += 1;
        write_binding(&tx, project, &binding)?;
        tx.commit().map_err(|e| e.to_string())?;
        Ok(super::trigger_state::binding_state(binding))
    }
    pub(crate) fn preview_neutral_trigger_mapping(
        &self,
        project: &str,
        draft: &TriggerBindingDraft,
        snapshot: &TriggerEvent,
        catalog: &WorkflowCatalog,
        handlers: &BTreeSet<HandlerRef>,
    ) -> Result<TriggerPreview, String> {
        self.validate_trigger_mapping(project, draft, catalog, handlers)?;
        snapshot.validate()?;
        if draft.source != snapshot.source
            || trigger_schema(draft.source, &draft.kind)? != snapshot.schema
        {
            return Err("snapshot does not match binding repository/resource".into());
        }
        let publication = self.get_neutral_publication(
            project,
            &draft.publication.id,
            draft.publication.revision,
            catalog,
            handlers,
        )?;
        let values = &snapshot.values;
        let inputs = map_values(&draft.mapping.inputs, values);
        let workspaces = project_workspaces(&draft.mapping.subject, &publication.definition);
        let subject = (!draft.mapping.subject.is_empty() || !workspaces.is_empty()).then(|| {
            let mut subject = map_values(&draft.mapping.subject, values);
            for name in workspaces {
                subject.insert(
                    name,
                    serde_json::json!({"reference": "workspace", "id": project}),
                );
            }
            subject
        });
        let (inputs, subject) = publication
            .definition
            .validate_values(&inputs, subject.as_ref())?;
        Ok(TriggerPreview {
            inputs,
            subject,
            overlaps: overlaps(draft, &self.stored_neutral_trigger_bindings(project)?),
        })
    }
    pub(crate) fn validate_trigger_mapping(
        &self,
        project: &str,
        draft: &TriggerBindingDraft,
        catalog: &WorkflowCatalog,
        handlers: &BTreeSet<HandlerRef>,
    ) -> Result<(), String> {
        if draft.publication.project != project
            || draft.id.trim().is_empty()
            || draft.id.len() > 128
            || draft.kind.is_empty()
            || draft.kind.len() > 128
            || draft.resource_prefix.is_empty()
            || draft.resource_prefix.len() > 1024
            || draft.events.is_empty()
            || draft.events.len() > 32
            || draft.events.iter().any(|s| s.is_empty() || s.len() > 128)
            || draft.ignored_actors.len() > 100
        {
            return Err("invalid trigger binding identity/filter".into());
        }
        super::scheduled_trigger::validate(draft)?;
        let source_schema = trigger_schema(draft.source, &draft.kind)?;
        for (field, value) in &draft.filters {
            let schema = source_schema
                .fields
                .get(field)
                .ok_or("unknown trigger filter field")?;
            if schema.value_type != super::catalog::ValueType::Array
                && !schema.value_type.matches(value)
            {
                return Err("trigger filter type mismatch".into());
            }
        }
        draft.limits.validate()?;
        let publication = self.get_neutral_publication(
            project,
            &draft.publication.id,
            draft.publication.revision,
            catalog,
            handlers,
        )?;
        let source = &source_schema;
        check_mapping(
            &draft.mapping.inputs,
            source,
            &publication.definition.inputs,
        )?;
        match &publication.definition.subject {
            Some(subject) if subject.required || !draft.mapping.subject.is_empty() => {
                // The bound project supplies every unmapped workspace field.
                let mut fields = subject.fields.clone();
                for name in project_workspaces(&draft.mapping.subject, &publication.definition) {
                    fields.fields.remove(&name);
                }
                check_mapping(&draft.mapping.subject, source, &fields)?
            }
            None if !draft.mapping.subject.is_empty() => {
                return Err("workflow declares no subject".into());
            }
            _ => {}
        }
        Ok(())
    }
}
/// Subject fields the binding's own project fills: a source snapshot carries no
/// workspace reference, and a native operation runs only in its bound project.
fn project_workspaces(
    mapping: &BTreeMap<String, String>,
    definition: &super::neutral_publication::NeutralDefinition,
) -> Vec<String> {
    definition
        .subject
        .iter()
        .flat_map(|subject| &subject.fields.fields)
        .filter(|(name, field)| {
            field.value_type == super::catalog::ValueType::WorkspaceReference
                && !mapping.contains_key(*name)
        })
        .map(|(name, _)| name.clone())
        .collect()
}
fn check_mapping(
    mapping: &BTreeMap<String, String>,
    source: &ObjectSchema,
    target: &ObjectSchema,
) -> Result<(), String> {
    for (name, from) in mapping {
        let source = source
            .fields
            .get(from)
            .ok_or("unknown trigger snapshot field")?;
        let target = target
            .fields
            .get(name)
            .ok_or("unknown workflow mapping target")?;
        if source.value_type != target.value_type
            || (!source.required && target.required && target.default.is_none())
        {
            return Err("trigger mapping type or required-field mismatch".into());
        }
    }
    if target
        .fields
        .iter()
        .any(|(name, f)| f.required && f.default.is_none() && !mapping.contains_key(name))
    {
        return Err("required workflow field has no trigger mapping".into());
    }
    Ok(())
}
pub(crate) fn overlaps(
    draft: &TriggerBindingDraft,
    bindings: &[NeutralTriggerBinding],
) -> BTreeMap<String, i64> {
    // Conservative overlap: different filters can still match the same resource.
    bindings
        .iter()
        .filter(|b| {
            b.enabled
                && b.draft.id != draft.id
                && b.draft.source == draft.source
                && b.draft.kind == draft.kind
                && !b.draft.events.is_disjoint(&draft.events)
                && (b.draft.resource_prefix.starts_with(&draft.resource_prefix)
                    || draft.resource_prefix.starts_with(&b.draft.resource_prefix))
        })
        .map(|b| (b.draft.id.clone(), b.revision))
        .collect()
}
fn read_bindings(conn: &Connection, project: &str) -> Result<Vec<NeutralTriggerBinding>, String> {
    let mut query = conn
        .prepare("SELECT document FROM neutral_trigger_bindings WHERE project=?1 ORDER BY id")
        .map_err(|e| e.to_string())?;
    query
        .query_map([project], |r| r.get::<_, String>(0))
        .map_err(|e| e.to_string())?
        .map(|r| serde_json::from_str(&r.map_err(|e| e.to_string())?).map_err(|e| e.to_string()))
        .collect()
}
fn write_binding(
    conn: &Connection,
    project: &str,
    b: &NeutralTriggerBinding,
) -> Result<(), String> {
    conn.execute("INSERT INTO neutral_trigger_bindings(project,id,revision,document) VALUES(?1,?2,?3,?4)
        ON CONFLICT(project,id) DO UPDATE SET revision=excluded.revision,document=excluded.document",
        params![project,b.draft.id,b.revision,serde_json::to_string(b).map_err(|e|e.to_string())?]).map_err(|e|e.to_string())?;
    Ok(())
}
