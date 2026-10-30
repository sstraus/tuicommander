//! Durable command intents and observations. Unknown effects retain named resources.
use super::*;
use crate::workflows::run::check::{clean_artifact, execute_process};
use rusqlite::{Connection, OptionalExtension, TransactionBehavior, params};
use sha2::{Digest, Sha256};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::Duration;

fn digest(bytes: &[u8]) -> String {
    hex::encode(Sha256::digest(bytes))
}
fn encode(value: &impl Serialize) -> Result<String, String> {
    serde_json::to_string(value).map_err(|_| "encode command contract".into())
}
fn sql(error: rusqlite::Error) -> String {
    format!("command journal: {error}")
}

/// These are installed policy adapters, not workflow-supplied executables.
#[derive(Clone)]
pub(crate) struct CommandAdapters {
    pub build_slot: PathBuf,
    pub rb: PathBuf,
    pub freeze: PathBuf,
    pub agent_name: String,
}

impl CommandAdapters {
    pub fn installed() -> Self {
        let root = std::env::var_os("TUIC_WORKFLOW_ADAPTER_ROOT")
            .map(PathBuf::from)
            .unwrap_or_else(|| {
                dirs::home_dir()
                    .unwrap_or_default()
                    .join("Gits/personal/orchestrator/tools")
            });
        Self {
            build_slot: root.join("build/build-slot.sh"),
            rb: root.join("remote/rb"),
            freeze: std::env::var_os("RB_BUILD_FREEZE")
                .map(PathBuf::from)
                .unwrap_or_else(|| {
                    dirs::home_dir()
                        .unwrap_or_default()
                        .join("Gits/.tmp/BUILD_FREEZE")
                }),
            agent_name: "tuic-workflow".into(),
        }
    }
    fn frozen(&self) -> Result<bool, String> {
        match std::fs::read_to_string(&self.freeze) {
            Ok(text) => Ok(self.agent_name.is_empty()
                || !text.lines().any(|line| line.contains(&self.agent_name))),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(false),
            Err(_) => Err("cannot inspect build freeze".into()),
        }
    }
}

#[derive(Clone)]
pub(crate) struct CommandJournal {
    path: PathBuf,
}

struct ExecutionContext<'a> {
    definition: &'a CommandDefinition,
    identity: &'a CommandIdentity,
    workspace: &'a Path,
    runs_db: &'a Path,
    adapters: &'a CommandAdapters,
    stdin: Option<&'a [u8]>,
}

impl CommandJournal {
    pub fn for_runs(path: &Path) -> Self {
        Self {
            path: path.with_extension("commands.sqlite3"),
        }
    }
    fn connect(&self) -> Result<Connection, String> {
        let conn = Connection::open(&self.path).map_err(sql)?;
        conn.busy_timeout(Duration::from_secs(10)).map_err(sql)?;
        conn.execute_batch(
            "PRAGMA journal_mode=WAL;
            CREATE TABLE IF NOT EXISTS commands (
                effect TEXT PRIMARY KEY, run TEXT NOT NULL, payload TEXT NOT NULL,
                identity TEXT NOT NULL, state TEXT NOT NULL, reason TEXT NOT NULL,
                receipt TEXT, handle_path TEXT NOT NULL);
            CREATE TABLE IF NOT EXISTS command_observations (
                sequence INTEGER PRIMARY KEY AUTOINCREMENT, effect TEXT NOT NULL,
                run TEXT NOT NULL, identity TEXT NOT NULL, state TEXT NOT NULL,
                reason TEXT NOT NULL, receipt TEXT);
            CREATE TABLE IF NOT EXISTS command_resources (
                resource TEXT PRIMARY KEY, effect TEXT NOT NULL);",
        )
        .map_err(sql)?;
        Ok(conn)
    }
    pub fn observations(&self, run: &str) -> Result<Vec<CommandObservation>, String> {
        let conn = self.connect()?;
        let mut statement = conn.prepare("SELECT sequence,identity,state,reason,receipt FROM command_observations WHERE run=?1 ORDER BY sequence").map_err(sql)?;
        let rows = statement
            .query_map([run], |row| {
                Ok((
                    row.get::<_, i64>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, String>(2)?,
                    row.get::<_, String>(3)?,
                    row.get::<_, Option<String>>(4)?,
                ))
            })
            .map_err(sql)?;
        rows.map(|row| {
            let (sequence, identity, state, reason, receipt) = row.map_err(sql)?;
            Ok(CommandObservation {
                sequence,
                identity: serde_json::from_str(&identity).map_err(|_| "decode command identity")?,
                state,
                reason,
                receipt: receipt
                    .map(|v| serde_json::from_str(&v))
                    .transpose()
                    .map_err(|_| "decode command receipt")?,
            })
        })
        .collect()
    }
    pub fn receipt(&self, effect: &str) -> Result<Option<CommandReceipt>, String> {
        let text: Option<String> = self
            .connect()?
            .query_row(
                "SELECT receipt FROM commands WHERE effect=?1",
                [effect],
                |row| row.get(0),
            )
            .optional()
            .map_err(sql)?
            .flatten();
        text.map(|text| serde_json::from_str(&text).map_err(|_| "decode command receipt".into()))
            .transpose()
    }
    fn observe(
        conn: &Connection,
        identity: &CommandIdentity,
        state: &str,
        reason: &str,
        receipt: Option<&CommandReceipt>,
    ) -> Result<(), String> {
        let receipt = receipt.map(encode).transpose()?;
        conn.execute(
            "UPDATE commands SET state=?2,reason=?3,receipt=?4 WHERE effect=?1",
            params![identity.effect, state, reason, receipt],
        )
        .map_err(sql)?;
        conn.execute("INSERT INTO command_observations(effect,run,identity,state,reason,receipt) VALUES (?1,?2,?3,?4,?5,?6)", params![identity.effect, identity.run, encode(identity)?, state, reason, receipt]).map_err(sql)?;
        Ok(())
    }
    fn admit(
        &self,
        identity: &CommandIdentity,
        payload: &str,
        resources: &[String],
        handle: &Path,
        adapters: &CommandAdapters,
    ) -> Result<bool, String> {
        let mut conn = self.connect()?;
        let tx = conn
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .map_err(sql)?;
        let existing: Option<(String, String)> = tx
            .query_row(
                "SELECT payload,state FROM commands WHERE effect=?1",
                [&identity.effect],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .optional()
            .map_err(sql)?;
        if let Some((old, state)) = existing {
            if old != payload {
                return Err("command effect key conflicts with pinned payload".into());
            }
            if !matches!(state.as_str(), "ready" | "queued") {
                return Ok(false);
            }
        } else {
            tx.execute(
                "INSERT INTO commands VALUES (?1,?2,?3,?4,'ready','graph position ready',NULL,?5)",
                params![
                    identity.effect,
                    identity.run,
                    payload,
                    encode(identity)?,
                    handle.to_string_lossy()
                ],
            )
            .map_err(sql)?;
            Self::observe(&tx, identity, "ready", "graph position ready", None)?;
        }
        let freeze = adapters.frozen()?;
        let mut blocked = None;
        for resource in resources {
            let owner: Option<String> = tx
                .query_row(
                    "SELECT effect FROM command_resources WHERE resource=?1",
                    [resource],
                    |row| row.get(0),
                )
                .optional()
                .map_err(sql)?;
            if owner.is_some_and(|owner| owner != identity.effect) {
                blocked = Some(format!("named resource busy: {resource}"));
                break;
            }
        }
        if freeze || blocked.is_some() {
            Self::observe(
                &tx,
                identity,
                "queued",
                if freeze {
                    "BUILD_FREEZE blocks new admission"
                } else {
                    blocked.as_deref().unwrap_or_default()
                },
                None,
            )?;
            tx.commit().map_err(sql)?;
            return Ok(false);
        }
        for resource in resources {
            tx.execute(
                "INSERT OR IGNORE INTO command_resources VALUES (?1,?2)",
                params![resource, identity.effect],
            )
            .map_err(sql)?;
        }
        Self::observe(
            &tx,
            identity,
            "admitted",
            "intent committed; build capacity/fairness delegated to build-slot/rb",
            None,
        )?;
        tx.commit().map_err(sql)?;
        Ok(true)
    }
    fn finish(&self, receipt: &CommandReceipt) -> Result<(), String> {
        let mut conn = self.connect()?;
        let tx = conn
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .map_err(sql)?;
        let existing: Option<String> = tx
            .query_row(
                "SELECT receipt FROM commands WHERE effect=?1",
                [&receipt.identity.effect],
                |row| row.get(0),
            )
            .map_err(sql)?;
        if let Some(existing) = existing {
            if existing != encode(receipt)? {
                return Err("command receipt is immutable".into());
            }
            return Ok(());
        }
        if !receipt.stopped || !receipt.released {
            return Err("stop/release proof required before final receipt".into());
        }
        Self::observe(
            &tx,
            &receipt.identity,
            "completed",
            &receipt.reason,
            Some(receipt),
        )?;
        tx.execute(
            "DELETE FROM command_resources WHERE effect=?1",
            [&receipt.identity.effect],
        )
        .map_err(sql)?;
        tx.commit().map_err(sql)
    }
    fn uncertain(&self, identity: &CommandIdentity, reason: &str) -> Result<(), String> {
        let mut conn = self.connect()?;
        let tx = conn
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .map_err(sql)?;
        Self::observe(&tx, identity, "uncertain", reason, None)?;
        tx.commit().map_err(sql)
    }

    pub fn execute(
        &self,
        definition: &CommandDefinition,
        identity: &CommandIdentity,
        workspace: &Path,
        runs_db: &Path,
        adapters: &CommandAdapters,
    ) -> Result<CommandReceipt, String> {
        definition.validate()?;
        let definition_sha256 = digest(encode(definition)?.as_bytes());
        if let Some(receipt) = self.receipt(&identity.effect)? {
            if receipt.identity != *identity || receipt.definition_sha256 != definition_sha256 {
                return Err("command effect key conflicts with pinned payload".into());
            }
            return Ok(receipt);
        }
        let (commit, tree) = clean_artifact(workspace)?;
        let stdin = definition
            .stdin_artifact
            .as_ref()
            .map(|name| {
                use std::io::Read;
                let mut file =
                    std::fs::File::open(workspace.join(name)).map_err(|_| "open stdin artifact")?;
                let mut bytes = Vec::new();
                file.by_ref()
                    .take(64 * 1024 * 1024 + 1)
                    .read_to_end(&mut bytes)
                    .map_err(|_| "snapshot stdin artifact")?;
                if bytes.len() > 64 * 1024 * 1024 {
                    return Err("stdin artifact exceeds 64 MiB");
                }
                Ok(bytes)
            })
            .transpose()?;
        let subject = CommandSubject {
            stdin_sha256: stdin.as_ref().map(|bytes| digest(bytes)),
            workspace: workspace.to_string_lossy().into_owned(),
            commit,
            tree,
        };
        let payload = digest(encode(&(identity, definition, &subject))?.as_bytes());
        let private = self.path.with_extension("command-inputs");
        std::fs::create_dir_all(&private).map_err(|_| "create command input directory")?;
        let handle_path = private.join(format!(
            "{}.handle.json",
            digest(identity.effect.as_bytes())
        ));
        let resources = resource_keys(definition, workspace)?;
        if !self.admit(identity, &payload, &resources, &handle_path, adapters)? {
            if handle_path.exists()
                && matches!(
                    definition.target,
                    CommandTarget::Rb | CommandTarget::RbAfterNative { .. }
                )
            {
                return self.reconcile(
                    &ExecutionContext {
                        definition,
                        identity,
                        workspace,
                        runs_db,
                        adapters,
                        stdin: stdin.as_deref(),
                    },
                    &subject,
                    &definition_sha256,
                    &handle_path,
                );
            }
            return Err(
                "command queued or uncertain; inspect command_executions before retry or cleanup"
                    .into(),
            );
        }
        let result = self.run_admitted(
            &ExecutionContext {
                definition,
                identity,
                workspace,
                runs_db,
                adapters,
                stdin: stdin.as_deref(),
            },
            subject,
            definition_sha256,
            &handle_path,
            &private,
        );
        match result {
            Ok(receipt) => {
                self.finish(&receipt)?;
                Ok(receipt)
            }
            Err(error) => {
                self.uncertain(identity, &error)?;
                Err(error)
            }
        }
    }

    fn run_admitted(
        &self,
        context: &ExecutionContext<'_>,
        subject: CommandSubject,
        definition_sha256: String,
        handle_path: &Path,
        private: &Path,
    ) -> Result<CommandReceipt, String> {
        let ExecutionContext {
            definition,
            identity,
            workspace,
            runs_db,
            adapters,
            stdin,
        } = *context;
        let mut values = BTreeMap::new();
        for (name, reference) in &definition.secret_refs {
            let value = std::env::var(reference).map_err(|_| "secret reference unavailable")?;
            if value.is_empty() || value.len() > 16_384 {
                return Err("invalid secret reference value".into());
            }
            values.insert(name.clone(), value);
        }
        let argv = definition.invocation.argv();
        if let CommandTarget::RbAfterNative {
            native_effect,
            os,
            arch,
        } = &definition.target
        {
            let native = self
                .receipt(native_effect)?
                .ok_or("forced rb rerun requires native proof")?;
            if !native.proves_target(&CommandTarget::Local {
                os: os.clone(),
                arch: arch.clone(),
            }) || native.subject.commit != subject.commit
                || native.subject.tree != subject.tree
                || native.subject.stdin_sha256 != subject.stdin_sha256
                || native.command_sha256 != command_digest(definition)?
            {
                return Err("forced rb rerun lacks matching native artifact/command proof".into());
            }
        }
        let remote = matches!(
            definition.target,
            CommandTarget::Rb | CommandTarget::RbAfterNative { .. }
        );
        if let CommandTarget::Local { os, arch } = &definition.target
            && (os != std::env::consts::OS || arch != std::env::consts::ARCH)
        {
            return Err("local target does not match required platform".into());
        }
        let mut private_env = None;
        let mut command =
            if remote {
                let mut command = Command::new(&adapters.build_slot);
                command.arg("--remote");
                if definition.resources.contains(&CommandResource::Mutation) {
                    command.arg("--gate");
                }
                if matches!(definition.target, CommandTarget::RbAfterNative { .. }) {
                    command.env("RB_FORCE", "1");
                }
                // Output is redacted on the executor before rb's durable tee sees it.
                command.args(["python3", "-c", REMOTE_COMMAND]);
                command.arg(encode(&RemoteRequest {
                    names: values.keys().collect(),
                    timeout: definition.timeout_secs,
                })?);
                command.arg("--").args(&argv);
                let file = tempfile::NamedTempFile::new_in(private)
                    .map_err(|_| "create private environment input")?;
                serde_json::to_writer(file.as_file(), &values)
                    .map_err(|_| "write private environment input")?;
                command
                    .env("RB_SECRET_ENV_FILE", file.path())
                    .env("RB_HANDLE_FILE", handle_path);
                private_env = Some(file);
                command
            } else if definition.resources.iter().any(|resource| {
                matches!(resource, CommandResource::Build | CommandResource::Mutation)
            }) {
                let mut command = Command::new(&adapters.build_slot);
                if definition.resources.contains(&CommandResource::Mutation) {
                    command.arg("--gate");
                }
                command.args(&argv);
                command.envs(&values);
                command
            } else {
                let mut command = Command::new(&argv[0]);
                command.args(&argv[1..]).envs(&values);
                command
            };
        command
            .env_remove("RB_REF")
            .env_remove("RB_SECRET_ENV_FILE")
            .env_remove("RB_STDIN_FILE");
        if !matches!(definition.target, CommandTarget::RbAfterNative { .. }) {
            command.env_remove("RB_FORCE");
        }
        command.env("RB_BIN", &adapters.rb);
        // Reapply our transport reference after removing inherited wrapper controls.
        if let Some(file) = &private_env {
            command.env("RB_SECRET_ENV_FILE", file.path());
        }
        command
            .current_dir(workspace)
            .env("PATH", execution_path())
            .env("RB_AGENT_NAME", &adapters.agent_name)
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());
        let private_stdin = if let Some(bytes) = stdin {
            use std::io::Write;
            let mut file = tempfile::NamedTempFile::new_in(private)
                .map_err(|_| "create private stdin transport")?;
            file.write_all(bytes)
                .map_err(|_| "write private stdin transport")?;
            if remote {
                command.env("RB_STDIN_FILE", file.path());
            } else {
                command.stdin(file.reopen().map_err(|_| "open private stdin snapshot")?);
            }
            Some(file)
        } else {
            None
        };
        let output = execute_process(
            command,
            Duration::from_secs(u64::from(definition.timeout_secs)),
            Some((runs_db.to_owned(), identity.run.clone())),
            false,
        )?;
        drop(private_env);
        drop(private_stdin);
        let stdout = redact_captured(&output.stdout, &values);
        let stderr = redact_captured(&output.stderr, &values);
        let mut receipt = CommandReceipt {
            identity: identity.clone(),
            definition: definition.clone(),
            duration_ms: output.duration_ms,
            definition_sha256,
            command_sha256: command_digest(definition)?,
            subject,
            workspace_unchanged: true,
            result_artifact: None,
            os: std::env::consts::OS.into(),
            arch: std::env::consts::ARCH.into(),
            executor: "local".into(),
            toolchain: executable_stamp(&argv[0], workspace),
            exit_code: output.exit_code,
            succeeded: output.exit_code == 0 && !output.cancelled,
            reason: if output.cancelled {
                "owned process tree cancelled".into()
            } else {
                "owned process tree stopped".into()
            },
            output_sha256: digest(encode(&(&stdout, &stderr))?.as_bytes()),
            stdout,
            stderr,
            stopped: true,
            released: true,
            remote_handle: None,
            source_manifest_sha256: None,
            source_fingerprint: None,
        };
        if remote {
            let lookup = lookup_remote(adapters, handle_path)?;
            apply_remote_proof(&mut receipt, &lookup)?;
            let result = receipt
                .stdout
                .lines()
                .find_map(|line| line.strip_prefix("TUIC_COMMAND_RESULT="))
                .ok_or("remote command result missing; retain effect and resource")?;
            let value: RemoteOutput =
                serde_json::from_str(result).map_err(|_| "invalid remote command result")?;
            receipt.os = value.os;
            receipt.arch = value.arch;
            receipt.toolchain = value.toolchain;
            receipt.exit_code = value.exit_code;
            receipt.stdout = value.stdout;
            receipt.stderr = value.stderr;
            receipt.succeeded &= value.exit_code == 0;
            receipt.output_sha256 = digest(encode(&(&receipt.stdout, &receipt.stderr))?.as_bytes());
        }
        if let Err(reason) = validate_output(&definition.output, &receipt.stdout) {
            receipt.succeeded = false;
            receipt.reason = reason;
        }
        let after = clean_artifact(workspace).ok();
        receipt.workspace_unchanged =
            after.as_ref() == Some(&(receipt.subject.commit.clone(), receipt.subject.tree.clone()));
        receipt.result_artifact = after.map(|(commit, tree)| CommandSubject {
            workspace: receipt.subject.workspace.clone(),
            commit,
            tree,
            stdin_sha256: receipt.subject.stdin_sha256.clone(),
        });
        if !receipt.workspace_unchanged {
            if identity.tool.is_none() {
                receipt.succeeded = false;
                receipt.reason = "workspace source changed during execution".into();
            } else {
                receipt.reason = "typed Tool effect completed; changed workspace is not unchanged-artifact proof".into();
            }
        }
        if !receipt.succeeded && receipt.reason == "owned process tree stopped" {
            receipt.reason = "command failed or timed out".into();
        }
        Ok(receipt)
    }

    fn reconcile(
        &self,
        context: &ExecutionContext<'_>,
        subject: &CommandSubject,
        definition_sha256: &str,
        handle: &Path,
    ) -> Result<CommandReceipt, String> {
        let ExecutionContext {
            definition,
            identity,
            adapters,
            ..
        } = *context;
        let lookup = lookup_remote(adapters, handle)?;
        let mut receipt = CommandReceipt {
            identity: identity.clone(),
            definition: definition.clone(),
            duration_ms: 0,
            definition_sha256: definition_sha256.into(),
            command_sha256: command_digest(definition)?,
            subject: subject.clone(),
            workspace_unchanged: false,
            result_artifact: None,
            os: "linux".into(),
            arch: String::new(),
            executor: "rb".into(),
            toolchain: "unknown after client loss".into(),
            exit_code: -1,
            succeeded: false,
            reason: "remote stop reconciled; lost command output cannot satisfy proof".into(),
            stdout: String::new(),
            stderr: String::new(),
            output_sha256: digest(b""),
            stopped: false,
            released: false,
            remote_handle: None,
            source_manifest_sha256: None,
            source_fingerprint: None,
        };
        apply_remote_proof(&mut receipt, &lookup)?;
        // A recovered stop permits cleanup, not a fabricated successful proof or replay.
        self.finish(&receipt)?;
        Ok(receipt)
    }
}

fn execution_path() -> std::ffi::OsString {
    let path = std::env::var_os("PATH").unwrap_or_default();
    if cfg!(target_os = "macos") {
        let paths =
            std::iter::once(PathBuf::from("/opt/homebrew/bin")).chain(std::env::split_paths(&path));
        std::env::join_paths(paths).unwrap_or(path)
    } else {
        path
    }
}

fn executable_stamp(program: &str, workspace: &Path) -> String {
    use std::io::Read;
    let program_path = Path::new(program);
    let candidates = if program_path.components().count() > 1 || program_path.is_absolute() {
        vec![workspace.join(program_path)]
    } else {
        let path = execution_path();
        std::env::split_paths(&path)
            .flat_map(|directory| {
                let base = directory.join(program);
                let mut paths = vec![base.clone()];
                if cfg!(windows) && base.extension().is_none() {
                    for extension in std::env::var("PATHEXT")
                        .unwrap_or_else(|_| ".EXE;.CMD;.BAT;.COM".into())
                        .split(';')
                    {
                        paths.push(directory.join(format!("{program}{extension}")));
                    }
                }
                paths
            })
            .collect()
    };
    for path in candidates {
        let Ok(mut file) = std::fs::File::open(&path) else {
            continue;
        };
        let mut hash = Sha256::new();
        let mut bytes = [0; 65_536];
        loop {
            match file.read(&mut bytes) {
                Ok(0) => {
                    return format!(
                        "{}; sha256={}",
                        path.display(),
                        hex::encode(hash.finalize())
                    );
                }
                Ok(count) => hash.update(&bytes[..count]),
                Err(_) => return "executable fingerprint unavailable".into(),
            }
        }
    }
    "executable fingerprint unavailable".into()
}

fn command_digest(definition: &CommandDefinition) -> Result<String, String> {
    Ok(digest(
        encode(&(
            definition.version,
            &definition.invocation,
            &definition.secret_refs,
            &definition.stdin_artifact,
            definition.timeout_secs,
            &definition.output,
        ))?
        .as_bytes(),
    ))
}

fn resource_keys(definition: &CommandDefinition, workspace: &Path) -> Result<Vec<String>, String> {
    definition
        .resources
        .iter()
        .filter(|resource| !matches!(resource, CommandResource::Build))
        .map(|resource| match resource {
            CommandResource::CanonicalGit(_) => {
                let output = Command::new("git")
                    .args(["rev-parse", "--path-format=absolute", "--git-common-dir"])
                    .current_dir(workspace)
                    .output()
                    .map_err(|_| "resolve canonical repository resource")?;
                if !output.status.success() {
                    return Err("resolve canonical repository resource".into());
                }
                Ok(format!(
                    "canonical_git:{}",
                    std::fs::canonicalize(String::from_utf8_lossy(&output.stdout).trim())
                        .map_err(|_| "canonicalize repository resource")?
                        .to_string_lossy()
                ))
            }
            _ => encode(resource),
        })
        .collect()
}

fn redact_captured(bytes: &[u8], values: &BTreeMap<String, String>) -> String {
    let mut text = String::from_utf8_lossy(bytes).into_owned();
    // Capture truncation must not retain a partial secret at the final boundary.
    if bytes.len() == 1024 * 1024 {
        let prefix = values
            .values()
            .flat_map(|value| {
                value
                    .char_indices()
                    .skip(1)
                    .map(move |(end, _)| &value[..end])
            })
            .filter(|prefix| text.ends_with(prefix))
            .max_by_key(|prefix| prefix.len());
        if let Some(prefix) = prefix {
            text.truncate(text.len() - prefix.len());
            text.push_str("[REDACTED]");
        }
    }
    redact(&text, values)
}

fn redact(text: &str, values: &BTreeMap<String, String>) -> String {
    let mut result = text.to_owned();
    let mut values: Vec<_> = values.values().collect();
    values.sort_by_key(|value| std::cmp::Reverse(value.len()));
    for value in values {
        result = result.replace(value, "[REDACTED]");
    }
    result
}

fn lookup_remote(adapters: &CommandAdapters, handle: &Path) -> Result<serde_json::Value, String> {
    let output = Command::new(&adapters.rb)
        .arg("lookup")
        .arg(handle)
        .stdin(Stdio::null())
        .output()
        .map_err(|_| "rb lookup unavailable; effect remains uncertain")?;
    if !output.status.success() {
        return Err("rb lookup failed; effect remains uncertain".into());
    }
    serde_json::from_slice(&output.stdout).map_err(|_| "invalid rb lookup response".into())
}

fn validate_remote_stop(lookup: &serde_json::Value) -> Result<(), String> {
    let observation = &lookup["observation"];
    for key in ["task_id", "generation", "fence"] {
        if lookup[key].as_str().is_none() || observation[key] != lookup[key] {
            return Err("remote handle fence mismatch".into());
        }
        if !lookup["final_receipt"].is_null() && lookup["final_receipt"][key] != lookup[key] {
            return Err("remote final receipt fence mismatch".into());
        }
    }
    if !matches!(
        observation["state"].as_str(),
        Some("stopped_released" | "superseded")
    ) {
        return Err("remote effect/slot uncertain; no stop-release proof".into());
    }
    Ok(())
}

fn apply_remote_proof(
    receipt: &mut CommandReceipt,
    lookup: &serde_json::Value,
) -> Result<(), String> {
    validate_remote_stop(lookup)?;
    receipt.executor = "rb".into();
    receipt.remote_handle = Some(lookup.clone());
    receipt.stopped = true;
    receipt.released = true;
    let run_dir = lookup["run_dir"]
        .as_str()
        .ok_or("remote receipt missing artifact directory")?;
    let manifest = std::fs::read(Path::new(run_dir).join("manifest.z"))
        .map_err(|_| "remote source manifest missing")?;
    receipt.source_manifest_sha256 = Some(digest(&manifest));
    let result = std::fs::read_to_string(Path::new(run_dir).join("result.txt")).unwrap_or_default();
    receipt.source_fingerprint = result
        .lines()
        .find_map(|line| line.strip_prefix("tree_sha256="))
        .map(str::to_owned);
    let expected_head = format!("head={}", receipt.subject.commit);
    receipt.succeeded &= lookup["final_receipt"]["stopped"] == true
        && lookup["final_receipt"]["released"] == true
        && lookup["final_receipt"]["artifacts"] == "collected"
        && lookup["final_receipt"]["exit"] == 0
        && result.lines().any(|line| line == expected_head)
        && result.lines().any(|line| line == "stale=no")
        && result.lines().any(|line| line == "box_dirty=no");
    Ok(())
}

#[derive(Serialize)]
struct RemoteRequest<'a> {
    names: Vec<&'a String>,
    timeout: u32,
}
#[derive(Deserialize)]
struct RemoteOutput {
    os: String,
    arch: String,
    toolchain: String,
    exit_code: i32,
    stdout: String,
    stderr: String,
}

// Runs inside rb's admitted/fenced task. No successor or retry policy lives here.
const REMOTE_COMMAND: &str = r#"
import hashlib, json, os, platform, shutil, signal, subprocess, sys, threading
request=json.loads(sys.argv[1])
argv=sys.argv[3:]
values=sorted([os.environ[name] for name in request['names']],key=len,reverse=True)
def capture(pipe, target):
    while True:
        data=pipe.read(8192)
        if not data: break
        target.extend(data[:max(0,1048576-len(target))])
def redact(data):
    text=data.decode('utf-8',errors='replace')
    if len(data)==1048576:
        prefixes=[value[:end] for value in values for end in range(1,len(value)) if text.endswith(value[:end])]
        if prefixes: text=text[:-len(max(prefixes,key=len))]+'[REDACTED]'
    for value in values: text=text.replace(value,'[REDACTED]')
    return text
# A child group remains in rb's owned session, so rb retains its stop authority.
process=subprocess.Popen(argv,stdin=sys.stdin.buffer,stdout=subprocess.PIPE,stderr=subprocess.PIPE,preexec_fn=os.setpgrp)
out=bytearray(); err=bytearray()
readers=[threading.Thread(target=capture,args=(process.stdout,out)),threading.Thread(target=capture,args=(process.stderr,err))]
for reader in readers: reader.start()
try: code=process.wait(timeout=request['timeout'])
except subprocess.TimeoutExpired: code=-1
finally:
    try: os.killpg(process.pid,signal.SIGKILL)
    except ProcessLookupError: pass
    process.wait()
for reader in readers: reader.join()
executable=shutil.which(argv[0])
toolchain=platform.platform()+'; executable='+str(executable)
if executable:
    hasher=hashlib.sha256()
    with open(executable,'rb') as source:
        for data in iter(lambda: source.read(65536),b''): hasher.update(data)
    toolchain+='; sha256='+hasher.hexdigest()
print('TUIC_COMMAND_RESULT='+json.dumps(dict(os=platform.system().lower(),arch=platform.machine(),toolchain=toolchain,exit_code=code,stdout=redact(out),stderr=redact(err))))
"#;

#[cfg(test)]
mod tests;
