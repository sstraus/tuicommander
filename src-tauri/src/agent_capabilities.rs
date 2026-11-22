//! Per-agent capabilities, fixed when TUIC spawns a managed agent (story 2713).
//!
//! The set is written once, by the spawn that creates the session, and removed
//! with its PTY. Nothing that carries agent text — mail, prompts, later tool
//! arguments — writes it, so text sent to an agent cannot widen what it may do.
//!
//! Only `operator` remains: it gates automation cutover, rollback and the
//! answers to native workflow approvals. The per-session `land`, `push`, `tag`,
//! `children` and `progress` capabilities and their git shim were removed
//! (Boss, 2026-11-20): models handed those switches capped every child and broke
//! delegation, while native workflow publication is already gated by its
//! approval question. The map that holds the sets also binds a session to native
//! workflow calls, so a capped child keeps an (empty) entry.

use std::collections::BTreeSet;

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub(crate) enum Capability {
    Operator,
}

impl Capability {
    pub(crate) const NAMES: [&'static str; 1] = ["operator"];

    pub(crate) fn as_str(self) -> &'static str {
        match self {
            Self::Operator => "operator",
        }
    }

    const ALL: [Self; 1] = [Self::Operator];

    fn parse(name: &str) -> Option<Self> {
        Self::ALL
            .into_iter()
            .find(|capability| capability.as_str() == name)
    }
}

/// Capability names removed on 2026-11-20, accepted and ignored.
const RETIRED: [&str; 5] = ["land", "push", "tag", "children", "progress"];

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct CapabilitySet(BTreeSet<Capability>);

impl CapabilitySet {
    /// Workflow and automation agents: never `operator`.
    pub(crate) fn automation() -> Self {
        Self::default()
    }

    /// Parse the `capabilities` spawn argument: an array of capability names.
    pub(crate) fn from_spawn_arg(value: &serde_json::Value) -> Result<Self, String> {
        let names = value
            .as_array()
            .ok_or("capabilities must be an array of capability names")?;
        names
            .iter()
            // Briefs written before the removal still name these; they grant nothing.
            .filter(|name| !name.as_str().is_some_and(|name| RETIRED.contains(&name)))
            .map(|name| {
                name.as_str().and_then(Capability::parse).ok_or_else(|| {
                    format!(
                        "Unknown capability {name}; expected one of: {}",
                        Capability::NAMES.join(", ")
                    )
                })
            })
            .collect::<Result<BTreeSet<_>, _>>()
            .map(Self)
    }

    /// A persisted record: known names only, so a legacy record keeps `operator`.
    fn from_record(value: &serde_json::Value) -> Self {
        Self(
            value
                .as_array()
                .into_iter()
                .flatten()
                .filter_map(|name| name.as_str().and_then(Capability::parse))
                .collect(),
        )
    }

    pub(crate) fn contains(&self, capability: Capability) -> bool {
        self.0.contains(&capability)
    }

    /// The first capability of `self` that `holder` lacks, if any.
    pub(crate) fn first_outside(&self, holder: &Self) -> Option<Capability> {
        self.0.difference(&holder.0).next().copied()
    }
}

/// The file name, without `.json`, that holds `identity`'s set.
pub(crate) fn file_stem(identity: &str) -> String {
    use sha2::{Digest, Sha256};
    if uuid::Uuid::parse_str(identity).is_ok() {
        identity.to_owned()
    } else {
        format!(
            "nonuuid-{}",
            hex::encode(Sha256::digest(identity.as_bytes()))
        )
    }
}

fn capability_path(data_dir: &std::path::Path, identity: &str) -> std::path::PathBuf {
    data_dir
        .join("agent-capabilities")
        .join(format!("{}.json", file_stem(identity)))
}

/// Persist before launch so a restored identity cannot regain capabilities.
pub(crate) fn save(
    data_dir: &std::path::Path,
    identity: &str,
    set: &CapabilitySet,
) -> Result<(), String> {
    let dir = data_dir.join("agent-capabilities");
    std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    let path = capability_path(data_dir, identity);
    let names: Vec<_> = set.0.iter().map(|cap| cap.as_str()).collect();
    let bytes = serde_json::to_vec(&names).map_err(|e| e.to_string())?;
    save_bytes(data_dir, identity, &path, &bytes)
}

pub(crate) fn save_root(data_dir: &std::path::Path, identity: &str) -> Result<(), String> {
    let dir = data_dir.join("agent-capabilities");
    std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    let path = capability_path(data_dir, identity);
    save_bytes(data_dir, identity, &path, b"null")
}

fn save_bytes(
    _data_dir: &std::path::Path,
    _identity: &str,
    path: &std::path::Path,
    bytes: &[u8],
) -> Result<(), String> {
    use std::io::Write;
    match std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(path)
    {
        Ok(mut file) => {
            file.write_all(bytes)
                .and_then(|_| file.sync_all())
                .map_err(|e| e.to_string())?;
        }
        Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {
            if std::fs::read(path).map_err(|e| e.to_string())? != bytes {
                return Err("capability identity already has a different set".into());
            }
        }
        Err(error) => return Err(error.to_string()),
    }
    Ok(())
}

/// A missing or damaged legacy record never grants the new operator capability.
pub(crate) fn load(data_dir: &std::path::Path, identity: &str) -> Option<CapabilitySet> {
    let path = capability_path(data_dir, identity);
    // A restored identity is in use: keep its record clear of the cleanup's
    // age cutoff (`db::retention`).
    let _ = std::fs::File::options()
        .write(true)
        .open(&path)
        .and_then(|file| file.set_modified(std::time::SystemTime::now()));
    match std::fs::read(path) {
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            Some(CapabilitySet::automation())
        }
        Err(_) => Some(CapabilitySet::default()),
        Ok(bytes) => match serde_json::from_slice::<serde_json::Value>(&bytes) {
            Ok(serde_json::Value::Null) => None,
            // Records written before 2026-11-20 may name removed capabilities.
            Ok(value) => Some(CapabilitySet::from_record(&value)),
            Err(_) => Some(CapabilitySet::default()),
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // Catches: a client-provided non-UUID session cannot be restored as an uncapped root.
    #[test]
    fn non_uuid_root_identity_survives_restore() {
        let root = tempfile::tempdir_in(tuic_test_support::test_temp_root()).unwrap();
        save_root(root.path(), "client/id").unwrap();
        assert!(load(root.path(), "client/id").is_none());
        assert!(
            !root
                .path()
                .join("agent-capabilities/client/id.json")
                .exists()
        );
    }

    // Catches: a pre-removal record or brief naming retired capabilities drops
    // `operator` or refuses the spawn.
    #[test]
    fn retired_capability_names_are_ignored_and_keep_operator() {
        let root = tempfile::tempdir_in(tuic_test_support::test_temp_root()).unwrap();
        let names = serde_json::json!(["push", "operator", "children"]);
        let operator = CapabilitySet::from_spawn_arg(&names).unwrap();
        assert!(operator.contains(Capability::Operator));
        let path = capability_path(root.path(), "legacy");
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(&path, names.to_string()).unwrap();
        assert_eq!(load(root.path(), "legacy"), Some(operator));
        assert!(CapabilitySet::from_spawn_arg(&serde_json::json!(["bogus"])).is_err());
    }

    // Catches: the schema enum (NAMES) drifts from the names spawn accepts.
    #[test]
    fn schema_names_are_exactly_the_parsed_capabilities() {
        let parsed: Vec<&str> = Capability::NAMES
            .iter()
            .map(|name| Capability::parse(name).map_or("<unparsed>", Capability::as_str))
            .collect();
        assert_eq!(parsed, Capability::NAMES);
        assert_eq!(Capability::ALL.len(), Capability::NAMES.len());
    }
}
