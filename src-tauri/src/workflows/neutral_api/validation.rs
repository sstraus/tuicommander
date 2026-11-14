use super::model::{Diagnostic, Validation};
use crate::workflows::{
    catalog::{CatalogClosure, HandlerRef, WorkflowCatalog},
    data_binding::{DataOrigin, DataState, Datum},
    neutral_host::NeutralCatalogHost,
    neutral_publication::NeutralDefinition,
};
use serde_json::{Value, json};
use std::collections::{BTreeMap, BTreeSet};

fn diagnostic(path: String, message: String) -> Diagnostic {
    Diagnostic {
        path,
        code: "invalid_contract".into(),
        message,
    }
}
fn pointer(value: &str) -> String {
    value.replace('~', "~0").replace('/', "~1")
}

pub(crate) fn validate(
    definition: &NeutralDefinition,
    catalog: &WorkflowCatalog,
    handlers: &BTreeSet<HandlerRef>,
) -> Validation {
    let mut diagnostics = Vec::new();
    for (path, result) in [
        ("/definition", definition.validate_schema()),
        (
            "/definition/instances",
            CatalogClosure::from_instances(catalog, &definition.instances, handlers).map(|_| ()),
        ),
        (
            "/definition/subject",
            definition.validate_subject_bindings(catalog),
        ),
        ("/definition", definition.validate_input_bindings(catalog)),
    ] {
        if let Err(error) = result {
            diagnostics.push(diagnostic(path.into(), error));
        }
    }
    // Reuse the mapping validator per field to give the editor a precise link.
    let descriptors = definition
        .instances
        .iter()
        .map(|i| catalog.get(&i.descriptor))
        .collect::<Option<Vec<_>>>();
    if let (Some(descriptors), Ok(scopes)) = (
        descriptors,
        crate::workflows::scope_data::schemas(definition),
    ) {
        let outputs = descriptors.iter().map(|d| &d.outputs).collect::<Vec<_>>();
        for (activity, bindings) in &definition.input_bindings {
            for (field, source) in bindings {
                let target = descriptors.get(*activity).map(|d| &d.inputs);
                let fields = target
                    .and_then(|schema| schema.fields.get(field))
                    .map(|schema| BTreeMap::from([(field.clone(), schema.clone())]))
                    .unwrap_or_default();
                let target = crate::workflows::catalog::ObjectSchema { fields };
                if let Err(error) = crate::workflows::data_binding::validate_bindings(
                    &BTreeMap::from([(field.clone(), source.clone())]),
                    &definition.inputs,
                    &outputs,
                    &target,
                    *activity,
                    &scopes,
                ) {
                    diagnostics.push(diagnostic(
                        format!("/definition/input_bindings/{activity}/{}", pointer(field)),
                        error,
                    ));
                }
            }
        }
    }
    for (from, edges) in &definition.control.edges {
        for (port, to) in edges {
            if !definition.control.nodes.contains_key(from)
                || !definition.control.nodes.contains_key(to)
            {
                diagnostics.push(diagnostic(
                    format!(
                        "/definition/control/edges/{}/{}",
                        pointer(from),
                        pointer(port)
                    ),
                    "control edge names an unknown node".into(),
                ));
            }
        }
    }
    Validation {
        valid: diagnostics.is_empty(),
        diagnostics,
    }
}

pub(crate) fn preview(
    host: &dyn NeutralCatalogHost,
    project: &str,
    definition: NeutralDefinition,
    inputs: BTreeMap<String, Value>,
    subject: Option<BTreeMap<String, Value>>,
    catalog: &WorkflowCatalog,
    handlers: &BTreeSet<HandlerRef>,
) -> Result<Value, String> {
    let validation = validate(&definition, catalog, handlers);
    if !validation.valid {
        return Ok(json!({"validation":validation,"activities":[],"outputs":definition.outputs}));
    }
    let (inputs, subject) = definition.validate_values(&inputs, subject.as_ref())?;
    let workflow = inputs
        .iter()
        .map(|(field, value)| {
            (
                field.clone(),
                Datum {
                    state: DataState::Present(value.clone()),
                    provenance: BTreeSet::from([DataOrigin::WorkflowInput {
                        field: field.clone(),
                    }]),
                },
            )
        })
        .collect();
    let publication = crate::workflows::neutral_publication::preview_publication(
        project,
        definition.clone(),
        catalog,
        handlers,
    )?;
    let mut activities = Vec::new();
    for (index, instance) in definition.instances.iter().enumerate() {
        let descriptor = catalog
            .get(&instance.descriptor)
            .ok_or("preview descriptor unavailable")?;
        let bindings = definition.input_bindings.get(&index);
        let mut fields = BTreeMap::new();
        for (name, schema) in &descriptor.inputs.fields {
            let scoped = definition
                .collections
                .values()
                .any(|c| c.body == index && (c.item_input == *name || c.index_input == *name))
                || definition
                    .loops
                    .values()
                    .any(|c| c.body == index && c.carried.fields.contains_key(name));
            let datum = if scoped {
                Datum {
                    state: DataState::Unknown,
                    provenance: BTreeSet::new(),
                }
            } else if let Some(source) = bindings.and_then(|b| b.get(name)) {
                source.resolve(&workflow, &BTreeMap::new(), &BTreeMap::new())
            } else if bindings.is_none() {
                inputs
                    .get(name)
                    .cloned()
                    .or(schema.default.clone())
                    .map(|v| Datum {
                        state: DataState::Present(v),
                        provenance: BTreeSet::new(),
                    })
                    .unwrap_or(Datum {
                        state: DataState::Absent,
                        provenance: BTreeSet::new(),
                    })
            } else {
                schema
                    .default
                    .clone()
                    .map(|v| Datum {
                        state: DataState::Present(v),
                        provenance: BTreeSet::from([DataOrigin::Default]),
                    })
                    .unwrap_or(Datum {
                        state: DataState::Absent,
                        provenance: BTreeSet::new(),
                    })
            };
            fields.insert(name.clone(), datum);
        }
        let resolved: BTreeMap<_, _> = fields
            .iter()
            .filter_map(|(name, datum)| match &datum.state {
                DataState::Present(v) => Some((name.clone(), v.clone())),
                _ => None,
            })
            .collect();
        let unknown = fields
            .values()
            .any(|datum| matches!(datum.state, DataState::Unknown));
        let preparation = if unknown {
            Err("upstream sample output is unknown; no effect intent prepared".into())
        } else {
            super::super::run::neutral_api::preview_effect(
                host,
                &publication,
                descriptor,
                instance,
                &resolved,
                subject.as_ref(),
            )
        };
        let (effect_intent, unavailable_reason) = match preparation {
            Ok(value) => (Some(value), None),
            Err(error) => (None, Some(error)),
        };
        activities.push(json!({"activity":index,"inputs":fields,"outputs":descriptor.outputs,"outcomes":descriptor.outcomes,"effect_intent":effect_intent,"unavailable_reason":unavailable_reason}));
    }
    Ok(json!({"validation":validation,"activities":activities,"outputs":definition.outputs}))
}
