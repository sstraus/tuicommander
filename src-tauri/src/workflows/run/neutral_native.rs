//! Public native provenance comes only from transports and retained publications.
use super::{NativeCaller, NativeContext};
use crate::state::AppState;
use crate::workflows::catalog::ActivityDescriptor;
use crate::workflows::data_binding::DataSource;
use crate::workflows::neutral_host::NeutralCatalogHost;
use crate::workflows::neutral_publication::NeutralPublication;
use std::collections::{BTreeMap, BTreeSet};
use std::sync::Arc;

pub(super) fn capture<'a>(
    state: &Arc<AppState>,
    host: &dyn NeutralCatalogHost,
    caller: Option<NativeCaller>,
    project: &str,
    publications: impl Iterator<Item = &'a NeutralPublication>,
    descriptors: &[ActivityDescriptor],
) -> Result<Option<NativeContext>, String> {
    if !host.requires_trusted_caller() {
        return Ok(None);
    }
    let caller = caller.ok_or("native start requires authenticated backend caller provenance")?;
    let descriptors: BTreeMap<_, _> = descriptors.iter().map(|d| (&d.id, d)).collect();
    let mut checks = BTreeSet::new();
    let mut require_git = false;
    for publication in publications {
        for (activity, instance) in publication.definition.instances.iter().enumerate() {
            let descriptor = descriptors
                .get(&instance.descriptor)
                .ok_or("retained native descriptor unavailable")?;
            require_git |= descriptor.handler.name.starts_with("native.");
            if descriptor.handler.name != "native.verification.run" {
                continue;
            }
            let Some(DataSource::Constant { value }) = publication
                .definition
                .input_bindings
                .get(&activity)
                .and_then(|bindings| bindings.get("request"))
            else {
                return Err(
                    "native verification requires a published constant check request".into(),
                );
            };
            let name = value
                .get("check")
                .and_then(|v| v.as_str())
                .filter(|name| !name.trim().is_empty())
                .ok_or("published native verification check name unavailable")?;
            checks.insert(name.to_owned());
        }
    }
    if !require_git {
        return NativeContext::capture_project(state, caller, std::path::Path::new(project))
            .map(Some);
    }
    NativeContext::capture(
        state,
        caller,
        std::path::Path::new(project),
        checks.into_iter().collect(),
    )
    .map(Some)
}
