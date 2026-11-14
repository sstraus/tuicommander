//! Typed execution port. TUIC owns graph successors; build-slot/rb own build admission.
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq, JsonSchema)]
#[serde(tag = "type", rename_all = "snake_case", deny_unknown_fields)]
pub enum Invocation {
    Argv {
        argv: Vec<String>,
    },
    /// Shell interpretation is an explicit, publication-pinned contract.
    Shell {
        program: String,
        script: String,
        version: u16,
    },
}

impl Invocation {
    pub(crate) fn argv(&self) -> Vec<String> {
        match self {
            Self::Argv { argv } => argv.clone(),
            Self::Shell {
                program, script, ..
            } => {
                let stem = std::path::Path::new(program)
                    .file_stem()
                    .and_then(|s| s.to_str())
                    .unwrap_or("")
                    .to_ascii_lowercase();
                let flags = match stem.as_str() {
                    "cmd" => vec!["/D", "/S", "/C"],
                    "powershell" | "pwsh" => vec!["-NoProfile", "-Command"],
                    _ => vec!["-c"],
                };
                std::iter::once(program.clone())
                    .chain(flags.into_iter().map(str::to_owned))
                    .chain(std::iter::once(script.clone()))
                    .collect()
            }
        }
    }
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq, JsonSchema)]
#[serde(tag = "type", rename_all = "snake_case", deny_unknown_fields)]
pub enum CommandTarget {
    Local {
        os: String,
        arch: String,
    },
    Rb,
    /// An explicitly authorized forced rb rerun, gated by matching native proof.
    RbAfterNative {
        native_effect: String,
        os: String,
        arch: String,
    },
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq, JsonSchema)]
#[serde(tag = "type", rename_all = "snake_case", deny_unknown_fields)]
pub enum CommandOutput {
    ExitStatus,
    /// The command emits one JSON object; missing or zero matches never pass.
    TestSummary {
        minimum_matched: u64,
    },
    JsonObject {
        required_fields: Vec<String>,
    },
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq, PartialOrd, Ord, JsonSchema)]
#[serde(tag = "type", content = "key", rename_all = "snake_case")]
pub enum CommandResource {
    Build,
    CanonicalGit(String),
    Mutation,
    Gui(String),
    Device(String),
    // No description here: it would be copied into the workflow tool schema, which has a byte
    // budget. Maintenance names the published operation an environment node is authorized for.
    Maintenance(String),
}

/// Operations a maintenance node may name. Growing this list is a reviewed contract change
/// (docs/backend/wf16d-maintenance-contract.md), never a per-definition choice.
pub(crate) const MAINTENANCE_OPERATIONS: &[&str] = &["cache_gc"];

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct CommandDefinition {
    pub version: u16,
    pub invocation: Invocation,
    /// Symbolic workspace: `assigned` resolves through the run's workspace binding.
    pub workspace: String,
    pub target: CommandTarget,
    pub resources: Vec<CommandResource>,
    /// Environment variable name -> daemon environment reference; never a value.
    pub secret_refs: BTreeMap<String, String>,
    pub stdin_artifact: Option<String>,
    pub timeout_secs: u32,
    pub output: CommandOutput,
}

impl CommandDefinition {
    pub fn validate(&self) -> Result<(), String> {
        let argv = self.invocation.argv();
        if self.version != 1
            || self.workspace != "assigned"
            || argv.is_empty()
            || argv[0].is_empty()
            || argv.len() > 128
            || argv
                .iter()
                .any(|arg| arg.len() > 32_768 || arg.contains('\0'))
            || !(1..=86_400).contains(&self.timeout_secs)
            || self.resources.len() > 16
            || self.secret_refs.len() > 32
        {
            return Err("invalid typed command contract".into());
        }
        self.validate_maintenance()?;
        match &self.invocation {
            Invocation::Shell {
                version,
                program,
                script,
            } if *version != 1 || program.is_empty() || script.is_empty() => {
                return Err("explicit shell requires version 1, program and script".into());
            }
            Invocation::Argv { argv }
                if argv.first().is_some_and(|s| {
                    matches!(
                        std::path::Path::new(s).file_stem().and_then(|s| s.to_str()),
                        Some("sh" | "bash" | "zsh" | "cmd" | "powershell" | "pwsh")
                    )
                }) =>
            {
                return Err(
                    "shell execution must use the explicit versioned shell invocation".into(),
                );
            }
            _ => {}
        }
        if self
            .secret_refs
            .iter()
            .any(|(name, reference)| !env_name(name) || !env_name(reference))
        {
            return Err("invalid secret environment reference".into());
        }
        if self.stdin_artifact.as_ref().is_some_and(|path| {
            path.is_empty()
                || crate::fs::is_absolute_on_any_platform(path)
                || std::path::Path::new(path)
                    .components()
                    .any(|c| matches!(c, std::path::Component::ParentDir))
        }) {
            return Err("stdin artifact must be workspace relative".into());
        }
        if matches!(
            self.output,
            CommandOutput::TestSummary { minimum_matched: 0 }
        ) {
            return Err("test scope must require at least one matched test".into());
        }
        if let CommandOutput::JsonObject { required_fields } = &self.output
            && (required_fields.len() > 64
                || required_fields
                    .iter()
                    .any(|s| s.is_empty() || s.len() > 128))
        {
            return Err("invalid output schema".into());
        }
        Ok(())
    }
}

impl CommandDefinition {
    /// A node that mutates the environment names one published operation, runs on this machine
    /// and carries no credentials.
    fn validate_maintenance(&self) -> Result<(), String> {
        let operations: Vec<&str> = self
            .resources
            .iter()
            .filter_map(|resource| match resource {
                CommandResource::Maintenance(name) => Some(name.as_str()),
                _ => None,
            })
            .collect();
        if operations.is_empty() {
            return Ok(());
        }
        if operations.len() != 1 || !MAINTENANCE_OPERATIONS.contains(&operations[0]) {
            return Err("maintenance node requires one named, published operation".into());
        }
        if !self.secret_refs.is_empty() {
            return Err("maintenance node never receives credentials".into());
        }
        if !matches!(self.target, CommandTarget::Local { .. }) {
            return Err("maintenance node runs on the local target only".into());
        }
        Ok(())
    }
}

fn env_name(value: &str) -> bool {
    let mut bytes = value.bytes();
    bytes
        .next()
        .is_some_and(|c| c.is_ascii_alphabetic() || c == b'_')
        && bytes.all(|c| c.is_ascii_alphanumeric() || c == b'_')
}

pub(crate) mod executor;
