//! Secret provider port (#2914-b125 mock). A provider turns a reference into a
//! value on demand. Errors are typed and there is never a fallback to a plain
//! value: a failed resolve stores nothing.
use super::*;

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum SecretRef {
    /// An `op://vault/item/field` reference and an optional account shorthand.
    Op {
        reference: String,
        account: Option<String>,
    },
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum ProviderError {
    OpMissing,
    NotSignedIn,
    /// Locked vault, or the biometric/authorization prompt was dismissed or unanswered.
    Locked,
    UnknownReference,
    Timeout,
    /// Reference or resolved value breaks the secret schema.
    Invalid,
    /// The provider failed in a way none of the other variants names.
    Failed,
}

impl std::fmt::Display for ProviderError {
    // Never carries provider stderr: it could echo a reference or a value.
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            Self::OpMissing => "1Password CLI (op) is not installed",
            Self::NotSignedIn => "1Password CLI is not signed in",
            Self::Locked => "1Password is locked or authorization was not granted",
            Self::UnknownReference => "Secret reference was not found",
            Self::Timeout => "Secret provider timed out",
            Self::Invalid => "Secret reference or value is invalid",
            Self::Failed => "Secret provider failed",
        })
    }
}
impl std::error::Error for ProviderError {}

pub(crate) trait SecretProvider {
    fn resolve(&self, reference: &SecretRef) -> Result<Zeroizing<String>, ProviderError>;
}

/// One entry of `secret put references`: an `op://` reference and an optional account.
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct ReferencePut {
    pub references: Vec<ReferenceEntry>,
    pub scope: String,
    pub ttl: u64,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct ReferenceEntry {
    pub name: String,
    pub op: String,
    pub account: Option<String>,
}

impl ReferenceEntry {
    pub(crate) fn into_ref(self) -> (String, SecretRef) {
        (
            self.name,
            SecretRef::Op {
                reference: self.op,
                account: self.account,
            },
        )
    }
}

impl SecretStore {
    /// Resolve every reference now and store the values as a task scope, so
    /// `agent spawn scope=...` injects them into the child env only. All or
    /// nothing: the first failure returns its error and stores no scope.
    pub(crate) fn put_references(
        &self,
        provider: &dyn SecretProvider,
        scope: &str,
        entries: &[(String, SecretRef)],
        ttl: u64,
    ) -> Result<(Vec<String>, task::PutOutcome), ProviderError> {
        let mut names = Vec::new();
        let mut values = Vec::new();
        for (name, reference) in entries {
            let value = provider.resolve(reference)?;
            names.push(name.clone());
            values.push(value.to_string());
        }
        tracing::info!(
            source = "secrets",
            ?names,
            scope,
            "Resolved secret references"
        );
        let outcome = self
            .put(task::Put {
                names: names.clone(),
                values,
                scope: scope.to_owned(),
                ttl,
            })
            .map_err(|_| ProviderError::Invalid)?;
        Ok((names, outcome))
    }
}
