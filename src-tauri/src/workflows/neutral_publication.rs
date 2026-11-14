//! Internal neutral publication storage. This is not admission to execute.
//! Exact same-project subflow publications are pinned transitively. External
//! prompt assets, execution, and authority remain outside this contract.

// DEFERRED (2026-11-10) — contract entry points await the 004-2809 runtime consumer.
#![cfg_attr(
    not(test),
    expect(dead_code, reason = "002-eeb6 contract; first consumer 004-2809")
)]

use super::WorkflowStore;
use super::catalog::{
    ActivityDescriptor, ActivityInstance, CatalogClosure, HandlerRef, ObjectSchema, ReusableTarget,
    SubjectSchema, WorkflowCatalog, content_digest,
};
use rusqlite::{Connection, OptionalExtension, TransactionBehavior, params};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::{BTreeMap, BTreeSet};

type ResolvedValues = (BTreeMap<String, Value>, Option<BTreeMap<String, Value>>);

pub(super) const SCHEMA: &str = "CREATE TABLE IF NOT EXISTS neutral_workflow_publications (
    project TEXT NOT NULL,
    id TEXT NOT NULL,
    revision INTEGER NOT NULL CHECK(revision > 0),
    document TEXT NOT NULL,
    sha256 TEXT NOT NULL,
    PRIMARY KEY(project,id,revision)
);";

// Prior internal formats are refused rather than reinterpreted.
const FORMAT_VERSION: u32 = 11;
// Bound nested neutral subflows independently of reusable Call publication.
const MAX_SUBFLOW_DEPTH: usize = 8;
const MAX_SUBFLOW_EDGES: usize = 32;

#[derive(
    Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize, schemars::JsonSchema,
)]
#[serde(deny_unknown_fields)]
pub struct PublicationRef {
    pub project: String,
    pub id: String,
    pub revision: i64,
}

#[derive(
    Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize, schemars::JsonSchema,
)]
#[serde(deny_unknown_fields)]
pub struct PublicationPin {
    pub reference: PublicationRef,
    pub sha256: String,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub(crate) struct NeutralDefinition {
    pub control: super::neutral_control::ControlGraph,
    pub outputs: ObjectSchema,
    pub outcomes: BTreeSet<String>,
    pub inputs: ObjectSchema,
    pub subject: Option<SubjectSchema>,
    pub instances: Vec<ActivityInstance>,
    pub input_bindings: BTreeMap<usize, super::data_binding::InputBindings>,
    pub conditions: BTreeMap<String, super::condition::Condition>,
    pub collections: BTreeMap<String, super::collection::ForEach>,
    pub loops: BTreeMap<String, super::loop_data::LoopContract>,
    pub forks: BTreeMap<String, super::fork_scope::ForkContract>,
    pub output_bindings: super::data_binding::InputBindings,
    pub subflows: Vec<PublicationRef>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub(crate) struct NeutralPublication {
    pub format_version: u32,
    pub id: String,
    pub project: String,
    pub revision: i64,
    pub definition: NeutralDefinition,
    pub pins: CatalogClosure,
    pub subflow_pins: BTreeSet<PublicationPin>,
}

impl NeutralPublication {
    /// The root identity plus the stored, verified descendant identities.
    /// A content hash is not a signature or dispatch authority.
    pub(crate) fn closure_pins(&self) -> Result<BTreeSet<PublicationPin>, String> {
        let mut pins = self.subflow_pins.clone();
        pins.insert(PublicationPin {
            reference: PublicationRef {
                project: self.project.clone(),
                id: self.id.clone(),
                revision: self.revision,
            },
            sha256: content_digest(self)?,
        });
        Ok(pins)
    }
}

impl NeutralDefinition {
    pub(super) fn validate_schema(&self) -> Result<(), String> {
        self.inputs.check_schema("workflow inputs")?;
        self.outputs.check_schema("workflow outputs")?;
        if self.outcomes.is_empty() || self.outcomes.iter().any(|s| s.trim().is_empty()) {
            return Err("workflow requires named outcomes".into());
        }
        if let Some(subject) = &self.subject {
            subject.fields.check_schema("workflow subject")?;
        }
        Ok(())
    }

    pub(super) fn validate_subject_bindings(
        &self,
        catalog: &WorkflowCatalog,
    ) -> Result<(), String> {
        for instance in &self.instances {
            let descriptor = catalog
                .get(&instance.descriptor)
                .ok_or("missing descriptor for subject binding")?;
            if let Some(subject) = &descriptor.subject {
                subject.check_binding(self.subject.as_ref())?;
            }
        }
        Ok(())
    }

    pub(super) fn validate_input_bindings(&self, catalog: &WorkflowCatalog) -> Result<(), String> {
        let descriptors = self
            .instances
            .iter()
            .map(|instance| {
                catalog
                    .get(&instance.descriptor)
                    .ok_or("missing descriptor for input binding")
            })
            .collect::<Result<Vec<_>, _>>()?;
        let outputs: Vec<_> = descriptors
            .iter()
            .map(|descriptor| &descriptor.outputs)
            .collect();
        let scopes = super::scope_data::schemas(self)?;
        for (name, condition) in &self.conditions {
            if name.trim().is_empty() {
                return Err("condition requires a name".into());
            }
            condition
                .validate(&self.inputs, &outputs, &scopes)
                .map_err(|e| format!("condition.{name}: {e}"))?;
        }
        let mut scope_names = BTreeSet::new();
        for name in self
            .collections
            .keys()
            .chain(self.loops.keys())
            .chain(self.forks.keys())
        {
            if name.trim().is_empty() || !scope_names.insert(name) {
                return Err("iteration/fork scopes require unique nonempty names".into());
            }
        }
        let mut scoped_inputs = BTreeMap::new();
        for (name, collection) in &self.collections {
            if name.trim().is_empty() {
                return Err("collection requires a name".into());
            }
            let body = descriptors
                .get(collection.body)
                .ok_or("collection body not found")?;
            collection.validate(&self.inputs, &outputs, &body.inputs, &scopes)?;
            let fields = BTreeSet::from([
                collection.item_input.clone(),
                collection.index_input.clone(),
            ]);
            if scoped_inputs.insert(collection.body, fields).is_some() {
                return Err("activity belongs to more than one collection scope".into());
            }
        }
        for (name, loop_contract) in &self.loops {
            if name.trim().is_empty() {
                return Err("loop requires a name".into());
            }
            let body = descriptors
                .get(loop_contract.body)
                .ok_or("loop body not found")?;
            loop_contract.validate(&self.inputs, &outputs, &body.inputs, &scopes)?;
            let fields = loop_contract.carried.fields.keys().cloned().collect();
            if scoped_inputs.insert(loop_contract.body, fields).is_some() {
                return Err("activity belongs to more than one iteration scope".into());
            }
        }
        for fork in self.forks.values() {
            fork.validate(self.instances.len())?;
            for branch in fork.branches.values() {
                if scoped_inputs
                    .insert(branch.activity, BTreeSet::new())
                    .is_some()
                {
                    return Err("activity belongs to more than one iteration/fork scope".into());
                }
            }
        }
        super::data_binding::validate_bindings(
            &self.output_bindings,
            &self.inputs,
            &outputs,
            &self.outputs,
            usize::MAX,
            &scopes,
        )?;
        // Unbound, unscoped activities retain the inherited root-field-name
        // identity-input compatibility. Only declared bindings prove explicit mapping.
        let targets: BTreeSet<_> = self
            .input_bindings
            .keys()
            .chain(scoped_inputs.keys())
            .copied()
            .collect();
        for activity in targets {
            let target = descriptors
                .get(activity)
                .ok_or("binding target activity not found")?;
            let empty = BTreeMap::new();
            let bindings = self.input_bindings.get(&activity).unwrap_or(&empty);
            let mut inputs = target.inputs.clone();
            if let Some(fields) = scoped_inputs.get(&activity) {
                for field in fields {
                    if bindings.contains_key(field) {
                        return Err("ordinary binding overwrites iteration scope".into());
                    }
                    inputs.fields.remove(field);
                }
            }
            super::data_binding::validate_bindings(
                bindings,
                &self.inputs,
                &outputs,
                &inputs,
                activity,
                &scopes,
            )?;
        }
        // Ordinary output bindings form a DAG. Loop-carried results have an
        // explicit iteration boundary; they are not back edges in this graph.
        // Use an iterative topology check so graph depth cannot exhaust the stack.
        let mut dependencies = vec![0usize; self.instances.len()];
        let mut consumers = vec![Vec::new(); self.instances.len()];
        let mut add =
            |target: usize, source: &super::data_binding::DataSource| -> Result<(), String> {
                let mut producers = Vec::new();
                for leaf in source.leaves() {
                    match leaf {
                        super::data_binding::DataSource::ActivityOutput { activity, .. } => {
                            producers.push(*activity)
                        }
                        super::data_binding::DataSource::ScopeOutput { scope, .. } => {
                            producers.extend(super::scope_data::producers(self, scope)?)
                        }
                        _ => {}
                    }
                }
                for producer in producers {
                    dependencies[target] += 1;
                    consumers[producer].push(target);
                }
                Ok(())
            };
        for (target, bindings) in &self.input_bindings {
            for source in bindings.values() {
                add(*target, source)?;
            }
        }
        for collection in self.collections.values() {
            add(collection.body, &collection.collection)?;
        }
        for loop_contract in self.loops.values() {
            for source in loop_contract.initial.values() {
                add(loop_contract.body, source)?;
            }
        }
        let mut ready: Vec<_> = dependencies
            .iter()
            .enumerate()
            .filter_map(|(i, count)| (*count == 0).then_some(i))
            .collect();
        let mut visited = 0;
        while let Some(producer) = ready.pop() {
            visited += 1;
            for target in &consumers[producer] {
                dependencies[*target] -= 1;
                if dependencies[*target] == 0 {
                    ready.push(*target);
                }
            }
        }
        if visited != self.instances.len() {
            return Err("ordinary input binding dependency cycle".into());
        }
        self.control.validate(self, catalog)
    }

    /// Structured return projection, shared by root and reusable subflows.
    /// Provenance remains data; it is not inherited dispatch authority.
    pub(crate) fn resolve_outputs(
        &self,
        workflow: &BTreeMap<String, super::data_binding::Datum>,
        outputs: &BTreeMap<usize, BTreeMap<String, super::data_binding::Datum>>,
        scopes: &super::scope_data::ScopeValues,
    ) -> Result<super::data_binding::ResolvedInputs, String> {
        super::data_binding::resolve_bindings(
            &self.output_bindings,
            workflow,
            outputs,
            &self.outputs,
            scopes,
        )
    }

    /// Validate values without asserting subject freshness or host authority.
    pub(crate) fn validate_values(
        &self,
        inputs: &BTreeMap<String, Value>,
        subject: Option<&BTreeMap<String, Value>>,
    ) -> Result<ResolvedValues, String> {
        self.validate_schema()?;
        let inputs = self.inputs.resolve(inputs, "workflow inputs")?;
        let subject = match (&self.subject, subject) {
            (None, None) => None,
            (None, Some(_)) => return Err("workflow does not declare a subject".into()),
            (Some(schema), None) if schema.required => {
                return Err("workflow subject is required".into());
            }
            (Some(_), None) => None,
            (Some(schema), Some(values)) => {
                Some(schema.fields.resolve(values, "workflow subject")?)
            }
        };
        Ok((inputs, subject))
    }
}

impl WorkflowStore {
    /// Create with expected revision 0; update with the current latest revision.
    /// Each successful call inserts an immutable new revision. The catalog and
    /// handler inventory only establish pin consistency, not permission.
    pub(crate) fn save_neutral_publication(
        &self,
        project: &str,
        id: &str,
        expected_revision: i64,
        definition: NeutralDefinition,
        catalog: &WorkflowCatalog,
        handlers: &BTreeSet<HandlerRef>,
    ) -> Result<NeutralPublication, String> {
        self.save_neutral_publication_checked(
            project,
            id,
            expected_revision,
            definition,
            catalog,
            handlers,
            |_, _| Ok(()),
        )
    }

    /// Runs the host service check on the same verified snapshot before insertion.
    #[allow(clippy::too_many_arguments)] // Publication identity, content and validation share one transaction.
    pub(super) fn save_neutral_publication_checked(
        &self,
        project: &str,
        id: &str,
        expected_revision: i64,
        definition: NeutralDefinition,
        catalog: &WorkflowCatalog,
        handlers: &BTreeSet<HandlerRef>,
        check: impl FnOnce(&NeutralPublication, &[NeutralPublication]) -> Result<(), String>,
    ) -> Result<NeutralPublication, String> {
        self.save_neutral_publication_checked_from_draft(
            project,
            id,
            expected_revision,
            definition,
            catalog,
            handlers,
            check,
            None,
        )
    }

    #[allow(clippy::too_many_arguments)] // The optional draft fence joins the publication transaction.
    pub(super) fn save_neutral_publication_checked_from_draft(
        &self,
        project: &str,
        id: &str,
        expected_revision: i64,
        definition: NeutralDefinition,
        catalog: &WorkflowCatalog,
        handlers: &BTreeSet<HandlerRef>,
        check: impl FnOnce(&NeutralPublication, &[NeutralPublication]) -> Result<(), String>,
        draft: Option<&super::neutral_api::model::NeutralDraft>,
    ) -> Result<NeutralPublication, String> {
        if !crate::fs::is_absolute_on_any_platform(project)
            || project.len() > 4096
            || id.is_empty()
            || id.len() > 200
            || !id
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'.' | b'_' | b'-'))
        {
            return Err("invalid neutral workflow project or id".into());
        }
        let revision = expected_revision
            .checked_add(1)
            .filter(|r| *r > 0)
            .ok_or("invalid neutral workflow revision")?;
        definition.validate_schema()?;
        let pins = CatalogClosure::from_instances(catalog, &definition.instances, handlers)?;
        definition.validate_subject_bindings(catalog)?;
        definition.validate_input_bindings(catalog)?;
        let mut publication = NeutralPublication {
            format_version: FORMAT_VERSION,
            id: id.into(),
            project: project.into(),
            revision,
            definition,
            pins,
            subflow_pins: BTreeSet::new(),
        };
        let mut conn = self.connect()?;
        let tx = conn
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .map_err(|e| format!("begin neutral publication: {e}"))?;
        if let Some(draft) = draft {
            let current: i64 = tx
                .query_row(
                    "SELECT revision FROM neutral_workflow_drafts WHERE project=?1 AND id=?2",
                    params![project, id],
                    |r| r.get(0),
                )
                .map_err(|e| format!("read publication draft fence: {e}"))?;
            if current != draft.revision {
                return Err("neutral draft revision changed".into());
            }
            let already_published:bool=tx.query_row("SELECT EXISTS(SELECT 1 FROM neutral_workflow_presentation WHERE project=?1 AND id=?2 AND draft_revision=?3)",params![project,id,draft.revision],|r|r.get(0)).map_err(|e|e.to_string())?;
            if already_published {
                return Err("neutral draft is unchanged since publication".into());
            }
        }
        let current: i64 = tx.query_row("SELECT COALESCE(MAX(revision),0) FROM neutral_workflow_publications WHERE project=?1 AND id=?2", params![project, id], |row| row.get(0)).map_err(|e| format!("read neutral revision: {e}"))?;
        if current != expected_revision {
            return Err("stale neutral workflow revision".into());
        }
        publication.subflow_pins = derive_subflow_pins(&tx, &publication, catalog, handlers)?;
        let children = read_closure_members(&tx, &publication, catalog, handlers)?;
        check(&publication, &children)?;
        let digest = content_digest(&publication)?;
        let document = serde_json::to_string(&publication)
            .map_err(|e| format!("encode neutral publication: {e}"))?;
        tx.execute("INSERT INTO neutral_workflow_publications(project,id,revision,document,sha256) VALUES (?1,?2,?3,?4,?5)", params![project, id, revision, document, digest]).map_err(|e| format!("save neutral publication: {e}"))?;
        if let Some(draft) = draft {
            let editor = serde_json::to_string(&draft.editor).map_err(|e| e.to_string())?;
            tx.execute("INSERT INTO neutral_workflow_presentation(project,id,revision,draft_revision,editor_json) VALUES (?1,?2,?3,?4,?5)",params![project,id,revision,draft.revision,editor]).map_err(|e|format!("save publication presentation: {e}"))?;
        }
        tx.commit()
            .map_err(|e| format!("commit neutral publication: {e}"))?;
        Ok(publication)
    }

    /// Read and verify content, instance pin coverage and current handler availability.
    /// A matching hash detects changes; it is not a signature or authorization.
    pub(crate) fn get_neutral_publication(
        &self,
        project: &str,
        id: &str,
        revision: i64,
        catalog: &WorkflowCatalog,
        handlers: &BTreeSet<HandlerRef>,
    ) -> Result<NeutralPublication, String> {
        let (publication, _) =
            self.resolve_neutral_publication(project, id, revision, catalog, handlers)?;
        Ok(publication)
    }

    pub(super) fn resolve_neutral_publication(
        &self,
        project: &str,
        id: &str,
        revision: i64,
        catalog: &WorkflowCatalog,
        handlers: &BTreeSet<HandlerRef>,
    ) -> Result<(NeutralPublication, Vec<NeutralPublication>), String> {
        let mut conn = self.connect()?;
        let tx = conn
            .transaction()
            .map_err(|e| format!("begin neutral publication read: {e}"))?;
        let reference = PublicationRef {
            project: project.into(),
            id: id.into(),
            revision,
        };
        let publication = read_verified(&tx, &reference, catalog, handlers)?;
        let pins = derive_subflow_pins(&tx, &publication, catalog, handlers)?;
        if pins != publication.subflow_pins {
            return Err("neutral publication subflow closure mismatch".into());
        }
        let children = read_closure_members(&tx, &publication, catalog, handlers)?;
        tx.commit()
            .map_err(|e| format!("finish neutral publication read: {e}"))?;
        Ok((publication, children))
    }
}

fn read_closure_members(
    conn: &Connection,
    root: &NeutralPublication,
    catalog: &WorkflowCatalog,
    handlers: &BTreeSet<HandlerRef>,
) -> Result<Vec<NeutralPublication>, String> {
    root.subflow_pins
        .iter()
        .map(|pin| {
            let child = read_verified(conn, &pin.reference, catalog, handlers)?;
            if content_digest(&child)? != pin.sha256 {
                return Err(format!(
                    "neutral subflow content mismatch: {}@{}",
                    child.id, child.revision
                ));
            }
            Ok(child)
        })
        .collect()
}

fn read_verified(
    conn: &Connection,
    reference: &PublicationRef,
    catalog: &WorkflowCatalog,
    handlers: &BTreeSet<HandlerRef>,
) -> Result<NeutralPublication, String> {
    let row: Option<(String, String)> = conn.query_row("SELECT document,sha256 FROM neutral_workflow_publications WHERE project=?1 AND id=?2 AND revision=?3", params![reference.project, reference.id, reference.revision], |row| Ok((row.get(0)?, row.get(1)?))).optional().map_err(|e| format!("read neutral publication: {e}"))?;
    let (document, digest) = row.ok_or_else(|| {
        format!(
            "neutral publication not found: {}:{}@{}",
            reference.project, reference.id, reference.revision
        )
    })?;
    let value: Value = serde_json::from_str(&document).map_err(|e| {
        format!(
            "decode neutral publication {}@{}: {e}",
            reference.id, reference.revision
        )
    })?;
    if value.get("format_version").and_then(Value::as_u64) != Some(u64::from(FORMAT_VERSION)) {
        return Err(format!(
            "unsupported neutral publication format: {}@{}",
            reference.id, reference.revision
        ));
    }
    let publication: NeutralPublication = serde_json::from_value(value).map_err(|e| {
        format!(
            "decode neutral publication {}@{}: {e}",
            reference.id, reference.revision
        )
    })?;
    if publication.format_version != FORMAT_VERSION
        || publication.project != reference.project
        || publication.id != reference.id
        || publication.revision != reference.revision
        || content_digest(&publication)? != digest
    {
        return Err(format!(
            "neutral publication identity or content mismatch: {}@{}",
            reference.id, reference.revision
        ));
    }
    publication.definition.validate_schema()?;
    publication
        .pins
        .verify_instances(catalog, &publication.definition.instances, handlers)
        .map_err(|e| {
            format!(
                "neutral publication {}@{}: {e}",
                reference.id, reference.revision
            )
        })?;
    publication.definition.validate_subject_bindings(catalog)?;
    publication.definition.validate_input_bindings(catalog)?;
    Ok(publication)
}

/// Resolve actual records from one SQLite snapshot. Count expanded edges rather
/// than unique children so shared subgraphs cannot bypass the resource bound.
fn derive_subflow_pins(
    conn: &Connection,
    root: &NeutralPublication,
    catalog: &WorkflowCatalog,
    handlers: &BTreeSet<HandlerRef>,
) -> Result<BTreeSet<PublicationPin>, String> {
    fn walk(
        conn: &Connection,
        current: &NeutralPublication,
        catalog: &WorkflowCatalog,
        handlers: &BTreeSet<HandlerRef>,
        path: &mut Vec<PublicationRef>,
        depth: usize,
        edges: &mut usize,
    ) -> Result<BTreeSet<PublicationPin>, String> {
        if depth > MAX_SUBFLOW_DEPTH {
            return Err(format!("neutral subflow depth exceeds {MAX_SUBFLOW_DEPTH}"));
        }
        let here = PublicationRef {
            project: current.project.clone(),
            id: current.id.clone(),
            revision: current.revision,
        };
        if path.contains(&here) {
            return Err(format!(
                "neutral subflow cycle: {}@{}",
                here.id, here.revision
            ));
        }
        path.push(here);
        let mut pins = BTreeSet::new();
        let mut references: Vec<_> = current
            .definition
            .subflows
            .iter()
            .map(|r| (r, None))
            .collect();
        for instance in &current.definition.instances {
            let descriptor = catalog
                .get(&instance.descriptor)
                .ok_or("missing reusable descriptor")?;
            if let Some(ReusableTarget::Subflow { publication }) = &descriptor.target {
                references.push((&publication.reference, Some((descriptor, publication))));
            }
        }
        for (child_ref, target) in references {
            if child_ref.project != current.project {
                return Err("cross-project neutral subflow is not supported".into());
            }
            *edges += 1;
            if *edges > MAX_SUBFLOW_EDGES {
                return Err("neutral subflow expanded edges exceed 32".into());
            }
            if path.contains(child_ref) {
                return Err(format!(
                    "neutral subflow cycle: {}@{}",
                    child_ref.id, child_ref.revision
                ));
            }
            let child = read_verified(conn, child_ref, catalog, handlers)?;
            if let Some((descriptor, pin)) = target {
                if content_digest(&child)? != pin.sha256 {
                    return Err("reusable subflow content mismatch".into());
                }
                check_subflow_interface(descriptor, &child)?;
            }
            let descendants = walk(conn, &child, catalog, handlers, path, depth + 1, edges)?;
            if child.subflow_pins != descendants {
                return Err(format!(
                    "neutral subflow closure mismatch: {}@{}",
                    child.id, child.revision
                ));
            }
            pins.extend(descendants);
            pins.insert(PublicationPin {
                reference: child_ref.clone(),
                sha256: content_digest(&child)?,
            });
        }
        path.pop();
        Ok(pins)
    }
    walk(conn, root, catalog, handlers, &mut Vec::new(), 0, &mut 0)
}

/// Identity interfaces are exact here. Explicit projections/bindings belong to
/// the data contract; config is not an alternate schema language.
pub(super) fn check_subflow_interface(
    descriptor: &ActivityDescriptor,
    child: &NeutralPublication,
) -> Result<(), String> {
    if descriptor.inputs != child.definition.inputs
        || descriptor.outputs != child.definition.outputs
        || descriptor.outcomes != child.definition.outcomes
        || descriptor.subject != child.definition.subject
    {
        return Err("reusable subflow interface mismatch".into());
    }
    Ok(())
}

/// An ephemeral shape for pure sample preparation, never a stored publication or permit.
pub(crate) fn preview_publication(
    project: &str,
    definition: NeutralDefinition,
    catalog: &WorkflowCatalog,
    handlers: &BTreeSet<HandlerRef>,
) -> Result<NeutralPublication, String> {
    let pins = CatalogClosure::from_instances(catalog, &definition.instances, handlers)?;
    Ok(NeutralPublication {
        format_version: FORMAT_VERSION,
        id: "preview".into(),
        project: project.into(),
        revision: 0,
        definition,
        pins,
        subflow_pins: BTreeSet::new(),
    })
}
