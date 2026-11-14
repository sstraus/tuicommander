//! Durable GitHub source generations; polls and verified deliveries share this.
use super::{RunStore, runtime::WorkflowRuntime};
use crate::workflows::{
    github_trigger::{GithubResource, GithubSnapshot, snapshot_schema},
    trigger::TriggerEvent,
};
use rusqlite::{OptionalExtension, TransactionBehavior, params};
use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize)]
struct SourceState {
    snapshot: GithubSnapshot,
    epoch: u64,
    events: Vec<TriggerEvent>,
    #[serde(default)]
    unavailable: bool,
}
impl RunStore {
    pub(super) fn known_github_numbers(
        &self,
        repository: &str,
        kind: GithubResource,
    ) -> Result<Vec<i64>, String> {
        let conn = self.trigger_connection()?;
        conn.execute_batch("CREATE TABLE IF NOT EXISTS neutral_github_sources(resource TEXT PRIMARY KEY,document TEXT NOT NULL);").map_err(|e|e.to_string())?;
        let mut query = conn
            .prepare("SELECT document FROM neutral_github_sources")
            .map_err(|e| e.to_string())?;
        query
            .query_map([], |r| r.get::<_, String>(0))
            .map_err(|e| e.to_string())?
            .map(|r| {
                serde_json::from_str::<SourceState>(&r.map_err(|e| e.to_string())?)
                    .map_err(|e| e.to_string())
            })
            .filter_map(|r| match r {
                Ok(s)
                    if s.snapshot.open
                        && !s.unavailable
                        && s.snapshot.repository == repository
                        && s.snapshot.kind == kind =>
                {
                    Some(Ok(s.snapshot.number))
                }
                Ok(_) => None,
                Err(e) => Some(Err(e)),
            })
            .collect()
    }

    pub(super) fn retire_github_source(
        &self,
        repository: &str,
        kind: GithubResource,
        number: i64,
    ) -> Result<(), String> {
        let resource = format!(
            "github:{}/{}/{number}",
            crate::workflows::github_trigger::repository(repository)?,
            match kind {
                GithubResource::Issue => "issue",
                GithubResource::PullRequest => "pull_request",
            }
        );
        let conn = self.trigger_connection()?;
        let document: Option<String> = conn
            .query_row(
                "SELECT document FROM neutral_github_sources WHERE resource=?1",
                [&resource],
                |r| r.get(0),
            )
            .optional()
            .map_err(|e| e.to_string())?;
        if let Some(document) = document {
            let mut state: SourceState =
                serde_json::from_str(&document).map_err(|e| e.to_string())?;
            state.unavailable = true;
            conn.execute(
                "UPDATE neutral_github_sources SET document=?2 WHERE resource=?1",
                params![
                    resource,
                    serde_json::to_string(&state).map_err(|e| e.to_string())?
                ],
            )
            .map_err(|e| e.to_string())?;
        }
        Ok(())
    }

    pub(super) fn retained_github_events(
        &self,
        repository: &str,
        kind: GithubResource,
        include_unavailable: bool,
    ) -> Result<Vec<TriggerEvent>, String> {
        let conn = self.trigger_connection()?;
        conn.execute_batch("CREATE TABLE IF NOT EXISTS neutral_github_sources(resource TEXT PRIMARY KEY,document TEXT NOT NULL);").map_err(|e|e.to_string())?;
        let mut query = conn
            .prepare("SELECT document FROM neutral_github_sources ORDER BY resource")
            .map_err(|e| e.to_string())?;
        let documents = query
            .query_map([], |r| r.get::<_, String>(0))
            .map_err(|e| e.to_string())?;
        let mut events = Vec::new();
        for document in documents {
            let state: SourceState = serde_json::from_str(&document.map_err(|e| e.to_string())?)
                .map_err(|e| e.to_string())?;
            if state.snapshot.repository == repository
                && state.snapshot.kind == kind
                && (include_unavailable || !state.unavailable)
            {
                events.extend(state.events);
            }
        }
        Ok(events)
    }

    pub(super) fn normalize_github(
        &self,
        mut snapshot: GithubSnapshot,
        delivery_id: Option<String>,
    ) -> Result<Vec<TriggerEvent>, String> {
        snapshot.validate()?;
        snapshot.repository = snapshot.repository.to_ascii_lowercase();
        snapshot.labels = snapshot
            .labels
            .iter()
            .map(|s| s.to_ascii_lowercase())
            .collect();
        let resource = snapshot.resource()?;
        let mut conn = self.trigger_connection()?;
        conn.execute_batch("CREATE TABLE IF NOT EXISTS neutral_github_sources(resource TEXT PRIMARY KEY,document TEXT NOT NULL);").map_err(|e|e.to_string())?;
        let tx = conn
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .map_err(|e| e.to_string())?;
        let old: Option<String> = tx
            .query_row(
                "SELECT document FROM neutral_github_sources WHERE resource=?1",
                [&resource],
                |r| r.get(0),
            )
            .optional()
            .map_err(|e| e.to_string())?;
        let old: Option<SourceState> = old
            .map(|s| serde_json::from_str(&s).map_err(|e| e.to_string()))
            .transpose()?;
        let clock = |s: &GithubSnapshot| {
            chrono::DateTime::parse_from_rfc3339(&s.updated_at).map_err(|e| e.to_string())
        };
        if let Some(old) = &old
            && clock(&snapshot)? < clock(&old.snapshot)?
        {
            return Ok(Vec::new());
        }
        let changed = old
            .as_ref()
            .is_none_or(|old| relevant(&old.snapshot) != relevant(&snapshot));
        // A run's own label write may be the only change: self events.
        let self_write = old
            .as_ref()
            .map(|old| (old.epoch, labels_only_added(&old.snapshot, &snapshot)));
        let state = if changed {
            let epoch = old.as_ref().map_or(Ok(1), |s| {
                s.epoch.checked_add(1).ok_or("GitHub generation exhausted")
            })?;
            let mut actions = Vec::new();
            if snapshot.open {
                if old.as_ref().is_none_or(|s| !s.snapshot.open) {
                    actions.push((if old.is_none() { "opened" } else { "reopened" }, None));
                } else if old.as_ref().is_some_and(|s| match snapshot.kind {
                    GithubResource::Issue => s.snapshot.material_digest != snapshot.material_digest,
                    GithubResource::PullRequest => {
                        s.snapshot.head != snapshot.head || s.snapshot.base != snapshot.base
                    }
                }) {
                    actions.push(("updated", None));
                }
                if snapshot.kind == GithubResource::Issue {
                    for label in &snapshot.labels {
                        if old
                            .as_ref()
                            .is_none_or(|s| !s.snapshot.labels.contains(label))
                        {
                            actions.push(("labeled", Some(label.clone())));
                        }
                    }
                }
            }
            let events = actions
                .into_iter()
                .map(|(action, label)| {
                    let action_kind = match action {
                        "opened" => super::super::github_trigger::GithubAction::Opened,
                        "reopened" => super::super::github_trigger::GithubAction::Reopened,
                        "labeled" => super::super::github_trigger::GithubAction::Labeled,
                        _ => super::super::github_trigger::GithubAction::Updated,
                    };
                    let mut values = snapshot.values(action_kind);
                    if let Some(label) = label {
                        values.insert("label".into(), serde_json::json!(label));
                    }
                    TriggerEvent {
                        source: crate::workflows::trigger::TriggerSource::Github,
                        kind: match snapshot.kind {
                            GithubResource::Issue => "issue",
                            GithubResource::PullRequest => "pull_request",
                        }
                        .into(),
                        resource: resource.clone(),
                        generation: epoch.to_string(),
                        event: action.into(),
                        delivery_id: delivery_id.clone(),
                        schema: snapshot_schema(snapshot.kind),
                        values,
                        actor: snapshot.actor.clone(),
                    }
                })
                .collect();
            SourceState {
                snapshot,
                epoch,
                events,
                unavailable: false,
            }
        } else {
            let mut old = old.ok_or("GitHub source state missing")?;
            old.snapshot.updated_at = snapshot.updated_at;
            old.unavailable = false;
            old
        };
        if changed && let Some((base, added)) = &self_write {
            super::neutral_trigger_store::record_self_events(
                &tx,
                &resource,
                &base.to_string(),
                &state.epoch.to_string(),
                added.as_ref(),
            )?;
        }
        tx.execute("INSERT INTO neutral_github_sources VALUES(?1,?2) ON CONFLICT(resource) DO UPDATE SET document=excluded.document",
            params![resource,serde_json::to_string(&state).map_err(|e|e.to_string())?]).map_err(|e|e.to_string())?;
        tx.commit().map_err(|e| e.to_string())?;
        // The source outbox retains the current events for crash-safe admission;
        // an unchanged observation itself emits no new source event.
        Ok(if changed { state.events } else { Vec::new() })
    }
}
/// The labels a new issue generation adds when nothing else relevant changed:
/// same lifecycle, same material and no label removed. Otherwise none.
fn labels_only_added(
    old: &GithubSnapshot,
    new: &GithubSnapshot,
) -> Option<std::collections::BTreeSet<String>> {
    let old_labels: std::collections::BTreeSet<_> = old.labels.iter().cloned().collect();
    let new_labels: std::collections::BTreeSet<_> = new.labels.iter().cloned().collect();
    (new.kind == GithubResource::Issue
        && old.open == new.open
        && old.material_digest == new.material_digest
        && old_labels.is_subset(&new_labels))
    .then(|| new_labels.difference(&old_labels).cloned().collect())
}
fn relevant(s: &GithubSnapshot) -> serde_json::Value {
    match s.kind {
        GithubResource::Issue => serde_json::json!([s.open, s.material_digest, s.labels]),
        GithubResource::PullRequest => serde_json::json!([s.open, s.head, s.base]),
    }
}
impl WorkflowRuntime {
    pub(crate) fn known_github_numbers(
        &self,
        repository: &str,
        kind: GithubResource,
    ) -> Result<Vec<i64>, String> {
        self.neutral_owner()?
            .store
            .known_github_numbers(repository, kind)
    }

    pub(crate) fn retire_github_source(
        &self,
        repository: &str,
        kind: GithubResource,
        number: i64,
    ) -> Result<(), String> {
        self.neutral_owner()?
            .store
            .retire_github_source(repository, kind, number)
    }
}
