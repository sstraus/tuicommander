//! Task deadlines revoke future injection, not the masks of children already started.
use super::*;
use std::sync::Arc;
use std::time::Duration;
use tokio::time::Instant;

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct Put {
    pub names: Vec<String>,
    pub values: Vec<String>,
    pub scope: String,
    pub ttl: u64,
}

impl Drop for Put {
    fn drop(&mut self) {
        self.values.zeroize();
    }
}

/// Stored status, scope generation, and the instant new injection stops.
pub(super) type PutOutcome = (Status, uuid::Uuid, Instant);

pub(super) struct ScopedValues {
    values: BTreeMap<String, Zeroizing<String>>,
    deadline: Instant,
    generation: uuid::Uuid,
}

/// A spawn reservation contains no argv or PTY input. Drop wipes its env copy.
pub(crate) struct ScopedEnvironment {
    pub values: BTreeMap<String, Zeroizing<String>>,
    deadline: Instant,
    needles: Arc<Vec<Zeroizing<String>>>,
}

impl ScopedEnvironment {
    /// Recheck at each spawn attempt, including a delayed blocking-pool retry.
    pub(crate) fn apply_to(&self, command: &mut portable_pty::CommandBuilder) {
        let live = Instant::now() < self.deadline;
        for (name, value) in &self.values {
            if live {
                command.env(name, value.as_str());
            } else {
                command.env_remove(name);
            }
        }
    }
}

fn needles<'a>(values: impl IntoIterator<Item = &'a str>) -> Arc<Vec<Zeroizing<String>>> {
    Arc::new(values.into_iter().flat_map(mask::representations).collect())
}

impl SecretStore {
    pub(super) fn put(&self, mut put: Put) -> Result<PutOutcome, String> {
        if put.scope.is_empty()
            || put.scope.len() > 256
            || put.scope.contains('\0')
            || put.ttl == 0
            || put.names.is_empty()
            || put.names.len() > 16
            || put.names.len() != put.values.len()
            || put.names.iter().any(|name| {
                !valid_name(name) || matches!(name.as_str(), "TUIC_SESSION" | "TUIC_PARENT")
            })
            || put.values.iter().any(|value| !valid_secret_value(value))
            || put
                .names
                .iter()
                .collect::<std::collections::BTreeSet<_>>()
                .len()
                != put.names.len()
        {
            return Err("Invalid task secret schema".into());
        }
        let generation = uuid::Uuid::new_v4();
        let values = put
            .names
            .iter()
            .cloned()
            .zip(
                std::mem::take(&mut put.values)
                    .into_iter()
                    .map(Zeroizing::new),
            )
            .collect();
        let scoped = ScopedValues {
            values,
            deadline: Instant::now()
                .checked_add(Duration::from_secs(put.ttl))
                .ok_or("TTL is out of range")?,
            generation,
        };
        let deadline = scoped.deadline;
        self.inner.lock().scopes.insert(put.scope.clone(), scoped);
        Ok((
            Status {
                names: put.names.clone(),
                status: "stored".into(),
            },
            generation,
            deadline,
        ))
    }

    pub(super) fn expire_scope(&self, scope: &str, generation: uuid::Uuid) {
        let mut inner = self.inner.lock();
        if inner
            .scopes
            .get(scope)
            .is_some_and(|entry| entry.generation == generation)
        {
            inner.scopes.remove(scope);
        }
    }

    pub(crate) fn scope_environment(&self, scope: &str) -> Result<ScopedEnvironment, String> {
        let mut inner = self.inner.lock();
        if inner
            .scopes
            .get(scope)
            .is_some_and(|entry| entry.deadline <= Instant::now())
        {
            inner.scopes.remove(scope);
        }
        let entry = inner
            .scopes
            .get(scope)
            .ok_or("Task secret scope is missing or expired")?;
        let values = entry
            .values
            .iter()
            .map(|(name, value)| (name.clone(), Zeroizing::new(value.to_string())))
            .collect();
        let needles = needles(entry.values.values().map(|value| value.as_str()));
        Ok(ScopedEnvironment {
            values,
            needles,
            deadline: entry.deadline,
        })
    }

    pub(crate) fn bind_scope(&self, session: &str, environment: &ScopedEnvironment) {
        self.session_masks
            .insert(session.to_owned(), environment.needles.clone());
    }

    pub(crate) fn release_session(&self, session: &str) {
        self.session_masks.remove(session);
        self.literal_reads.remove(session);
    }

    /// Owner grant for literal terminal reads of one PTY session (#2643-6b38).
    /// Pattern redaction is skipped for that session; registered and scoped
    /// values stay masked. Only the desktop owner control calls the setter.
    pub(crate) fn set_literal_reads(&self, session: &str, enabled: bool) {
        if enabled {
            self.literal_reads.insert(session.to_owned());
        } else {
            self.literal_reads.remove(session);
        }
    }

    pub(crate) fn literal_reads_granted(&self, session: &str) -> bool {
        self.literal_reads.contains(session)
    }

    /// Mask the registered and session-scoped values without moving byte offsets.
    pub(crate) fn mask_session_preserving_offsets(
        &self,
        session: &str,
        bytes: &[u8],
    ) -> Zeroizing<Vec<u8>> {
        mask::mask_preserving_offsets(bytes, &self.pty_needles(session))
    }

    pub(crate) fn has_pty_secrets(&self, session: &str) -> bool {
        self.inner
            .lock()
            .values
            .values()
            .any(|value| value.kind != FieldKind::Username)
            || self.session_masks.contains_key(session)
    }

    /// Legacy form values also protect every PTY; scoped values protect bound PTYs.
    pub(crate) fn pty_needles(&self, session: &str) -> Vec<Zeroizing<String>> {
        let inner = self.inner.lock();
        let mut needles: Vec<_> = inner
            .values
            .values()
            .flat_map(FormValue::mask_representations)
            .collect();
        if let Some(scoped) = self.session_masks.get(session) {
            needles.extend(scoped.iter().map(|value| Zeroizing::new(value.to_string())));
        }
        needles
    }
}
