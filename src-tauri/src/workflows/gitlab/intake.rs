//! GitLab normalization feeds the shared source-neutral binding owner.
use super::event::GitLabEvent;
use crate::workflows::trigger::{TriggerEvent, TriggerSource};
use serde_json::json;
use std::collections::BTreeMap;
impl GitLabEvent {
    pub(crate) fn trigger(&self) -> Result<TriggerEvent, String> {
        let event = self
            .action
            .clone()
            .filter(|s| !s.is_empty() && s.len() <= 128)
            .ok_or("GitLab event action/status missing")?;
        let mut values = BTreeMap::from([
            ("instance".into(), json!(self.instance)),
            ("project".into(), json!(self.project)),
            ("generation".into(), json!(self.generation)),
            ("action".into(), json!(event)),
        ]);
        for (name, value) in [
            ("issue", self.issue),
            ("mr", self.mr),
            ("pipeline", self.pipeline),
        ] {
            if let Some(value) = value {
                values.insert(name.into(), json!(value));
            }
        }
        for (name, value) in [
            ("head", &self.head),
            ("base", &self.base),
            ("actor", &self.actor),
        ] {
            if let Some(value) = value {
                values.insert(name.into(), json!(value));
            }
        }
        let trigger = TriggerEvent {
            source: TriggerSource::Gitlab,
            kind: self.kind.clone(),
            resource: self.resource.clone(),
            generation: self.generation.clone(),
            event,
            delivery_id: Some(self.delivery.clone()),
            schema: super::event::snapshot_schema(&self.kind)?,
            values,
            actor: self.actor.clone(),
        };
        trigger.validate()?;
        Ok(trigger)
    }
}
