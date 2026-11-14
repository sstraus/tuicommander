//! Compose policy with an inspected publication, retaining the ordinary loop kernel.
use super::{
    WorkflowStore, catalog::*, correction_region::CorrectionRegion, data_binding::DataSource,
    neutral_publication::*, review_policy::ReviewAdmissionPolicy,
};
use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;

#[derive(Clone, Debug, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub(crate) struct CorrectionPolicyRegion {
    pub region: CorrectionRegion,
    pub policy: ReviewAdmissionPolicy,
}
/// A staged review that only gates its round's commit (Boss Q7=A, 2026-11-12) is not
/// a verdict: its output may feed nothing but a native.commit.create request, so it
/// never reaches the reviewer, the assessment, a condition or the body outputs.
pub(crate) fn commit_gate(
    body: &NeutralDefinition,
    review: usize,
    catalog: &WorkflowCatalog,
) -> Result<bool, String> {
    let reads = |source: &DataSource| source.reads_activity(review);
    if body.output_bindings.values().any(reads) {
        return Ok(false);
    }
    let marker = serde_json::json!({"source": "activity_output", "activity": review});
    let conditions = serde_json::to_value(&body.conditions).map_err(|e| e.to_string())?;
    if contains(&conditions, &marker) {
        return Ok(false);
    }
    let mut gated = false;
    for (activity, fields) in &body.input_bindings {
        for (name, source) in fields {
            if !reads(source) {
                continue;
            }
            let handler = &catalog
                .get(
                    &body
                        .instances
                        .get(*activity)
                        .ok_or("binding activity absent")?
                        .descriptor,
                )
                .ok_or("policy body descriptor absent")?
                .handler
                .name;
            if handler != "native.commit.create" || name != "request" {
                return Ok(false);
            }
            gated = true;
        }
    }
    Ok(gated)
}

fn contains(value: &serde_json::Value, marker: &serde_json::Value) -> bool {
    match value {
        serde_json::Value::Object(map) => {
            marker
                .as_object()
                .is_some_and(|m| m.iter().all(|(k, v)| map.get(k) == Some(v)))
                || map.values().any(|v| contains(v, marker))
        }
        serde_json::Value::Array(items) => items.iter().any(|v| contains(v, marker)),
        _ => false,
    }
}

/// Each policy round admits two reviews: the reviewer role and the native bounded
/// review (Boss Q8=A, 2026-11-12). A budget below 2 * (maxRepairs + 1) would starve
/// a round the region still schedules.
pub(crate) fn review_budget(
    policy: &ReviewAdmissionPolicy,
    max_corrections: u32,
) -> Result<(), String> {
    policy.validate()?;
    let evidence = &policy.evidence;
    if max_corrections != u32::from(evidence.max_repairs) {
        return Err("correction loop must count maxRepairs after the initial attempt".into());
    }
    if u32::from(evidence.max_reviews) < 2 * (max_corrections + 1) {
        return Err(
            "correction policy needs at least 2 review admissions per round: maxReviews >= 2 * (maxRepairs + 1)".into(),
        );
    }
    Ok(())
}

impl CorrectionPolicyRegion {
    pub(crate) fn definition(
        &self,
        store: &WorkflowStore,
        project: &str,
        catalog: &WorkflowCatalog,
        handlers: &BTreeSet<HandlerRef>,
    ) -> Result<NeutralDefinition, String> {
        review_budget(&self.policy, self.region.max_corrections)?;
        let definition = self.region.definition(catalog)?;
        let descriptor = catalog
            .get(&self.region.selected.descriptor)
            .ok_or("selected descriptor absent")?;
        let Some(ReusableTarget::Subflow { publication: pin }) = &descriptor.target else {
            return Err("policy requires a pinned selected body".into());
        };
        if pin.reference.project != project {
            return Err("selected policy body belongs to another project".into());
        }
        let (root, children) = store.resolve_neutral_publication(
            project,
            &pin.reference.id,
            pin.reference.revision,
            catalog,
            handlers,
        )?;
        if content_digest(&root)? != pin.sha256 {
            return Err("selected policy body pin differs".into());
        }
        self.inspect(&root, catalog)?;
        for child in children {
            self.inspect(&child, catalog)?;
        }
        Ok(definition)
    }
    fn inspect(
        &self,
        publication: &NeutralPublication,
        catalog: &WorkflowCatalog,
    ) -> Result<(), String> {
        let body = &publication.definition;
        let mut bounded = false;
        let mut assessment = None;
        for (i, instance) in body.instances.iter().enumerate() {
            let descriptor = catalog
                .get(&instance.descriptor)
                .ok_or("policy body descriptor absent")?;
            match descriptor.handler.name.as_str() {
                "host.agent.role"
                    if matches!(
                        instance
                            .config
                            .get("role")
                            .and_then(serde_json::Value::as_str),
                        Some("reviewer" | "validator")
                    ) =>
                {
                    let role_policy: ReviewAdmissionPolicy = serde_json::from_value(
                        instance
                            .config
                            .get("policy")
                            .cloned()
                            .ok_or("selected reviewer role is uncounted")?,
                    )
                    .map_err(|e| e.to_string())?;
                    if role_policy != self.policy {
                        return Err("selected reviewer role policy differs".into());
                    }
                }
                "native.review.change" if commit_gate(body, i, catalog)? => {}
                "native.review.change" | "native.correction.assess" => {
                    return Err("policy body contains an uncounted review/assessment".into());
                }
                "native.review.bounded" | "native.correction.policy.assess" => {
                    let policy: ReviewAdmissionPolicy = serde_json::from_value(
                        instance
                            .config
                            .get("policy")
                            .cloned()
                            .ok_or("selected body policy missing")?,
                    )
                    .map_err(|e| e.to_string())?;
                    if policy != self.policy {
                        return Err(
                            "selected body policy differs from immutable region policy".into()
                        );
                    }
                    if descriptor.handler.name == "native.review.bounded" {
                        bounded = true;
                    } else {
                        if assessment.replace(i).is_some() {
                            return Err("selected body has ambiguous assessments".into());
                        }
                        let Some(DataSource::ActivityOutput { activity, field }) =
                            body.input_bindings.get(&i).and_then(|v| v.get("report"))
                        else {
                            return Err(
                                "assessment must consume an owned role report output".into()
                            );
                        };
                        let producer = body
                            .instances
                            .get(*activity)
                            .ok_or("report producer absent")?;
                        let role_policy: ReviewAdmissionPolicy =
                            serde_json::from_value(
                                producer.config.get("policy").cloned().ok_or(
                                    "selected role review must share the admission policy",
                                )?,
                            )
                            .map_err(|e| e.to_string())?;
                        if role_policy != self.policy {
                            return Err("role review admission policy differs".into());
                        }
                        let role = catalog
                            .get(&producer.descriptor)
                            .ok_or("report descriptor absent")?;
                        if field != "report"
                            || role.handler.name != "host.agent.role"
                            || !matches!(
                                producer
                                    .config
                                    .get("role")
                                    .and_then(serde_json::Value::as_str),
                                Some("reviewer" | "validator")
                            )
                        {
                            return Err(
                                "assessment requires an independent source role report".into()
                            );
                        }
                    }
                }
                _ => {}
            }
        }
        // Each selected body is explicit; helper subflows without assessment are
        // allowed, but cannot expose the selected body's assessed state contract.
        if body.outputs == super::correction_region::correction_state_schema() {
            let i = assessment.ok_or("selected body lacks policy assessment")?;
            if !bounded {
                return Err("selected body lacks bounded native review".into());
            }
            for field in ["state", "repeat"] {
                if body.output_bindings.get(field)
                    != Some(&DataSource::ActivityOutput {
                        activity: i,
                        field: field.into(),
                    })
                {
                    return Err("selected body output bypasses native policy assessment".into());
                }
            }
        }
        Ok(())
    }
}

#[cfg(test)]
#[path = "correction_policy_region_tests.rs"]
mod tests;
