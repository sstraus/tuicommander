use super::*;
use crate::workflows::catalog::content_digest;
use crate::workflows::command::executor::{execution_path, local_environment, redact_captured};
use std::collections::BTreeMap;
use std::io::Write;
use std::path::Path;
use std::process::{Command, Stdio};
use std::time::Duration;

impl ScriptAdapter {
    /// Called once by the neutral owner after durable dispatch. This synchronous
    /// entry point belongs on the host's blocking worker, not a Tokio core thread.
    pub fn invoke(&self, effect: &NeutralEffect, workspace: &Path) -> ScriptObservation {
        match self.execute(effect, workspace) {
            Ok(observation) => observation,
            Err(reason) => ScriptObservation {
                effect: EffectObservation {
                    outcome: EffectOutcome::Completed {
                        result: NeutralResult::Failed { diagnostic: reason },
                    },
                    stopped: true,
                },
                result: None,
                stderr: String::new(),
            },
        }
    }

    fn execute(
        &self,
        effect: &NeutralEffect,
        workspace: &Path,
    ) -> Result<ScriptObservation, String> {
        let prepared = Self::prepare(&effect.invocation)?;
        if !effect.dispatched
            || !effect.approved()
            || effect.cancel_started
            || effect.preparation != prepared
            || effect.tool.is_some()
            || effect.key.digest != content_digest(&(&effect.invocation, &prepared))?
            || effect.key.fence != effect.invocation.fence
        {
            return Err("script invocation does not match the dispatched effect".into());
        }
        let (policy, invocation) =
            ScriptPolicy::resolve(&effect.invocation.descriptor, &effect.invocation.instance)?;
        let request = ScriptRequest {
            version: 1,
            identity: effect.key.clone(),
            config: effect
                .invocation
                .descriptor
                .config
                .resolve(&effect.invocation.instance.config, "script config")?,
            inputs: effect
                .invocation
                .descriptor
                .inputs
                .resolve(&effect.invocation.inputs, "script inputs")?,
            subject: effect
                .invocation
                .descriptor
                .subject
                .as_ref()
                .map(|schema| schema.resolve(effect.invocation.subject.as_ref()))
                .transpose()?
                .flatten(),
        };
        let input =
            serde_json::to_vec(&request).map_err(|e| format!("encode script request: {e}"))?;
        if input.len() > policy.input_bytes {
            return Err("script request exceeds input limit".into());
        }
        let mut values = BTreeMap::new();
        let mut secrets = BTreeMap::new();
        for (name, reference) in policy.environment_refs.iter().chain(&policy.secret_refs) {
            let value =
                std::env::var(reference).map_err(|_| "script environment reference unavailable")?;
            if value.is_empty() || value.len() > 16_384 {
                return Err("invalid script environment reference value".into());
            }
            if policy.secret_refs.contains_key(name) {
                secrets.insert(name.clone(), value.clone());
            }
            values.insert(name.clone(), value);
        }
        let argv = invocation.argv();
        let mut command = Command::new(&argv[0]);
        local_environment(&mut command, false);
        command
            .args(&argv[1..])
            .current_dir(workspace)
            .env("PATH", execution_path())
            .envs(&values)
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());
        std::fs::create_dir_all(&self.input_directory)
            .map_err(|_| "create script input directory")?;
        let mut input_file = tempfile::NamedTempFile::new_in(&self.input_directory)
            .map_err(|_| "create script input transport")?;
        input_file
            .write_all(&input)
            .map_err(|_| "write script input transport")?;
        command.stdin(
            input_file
                .reopen()
                .map_err(|_| "open script input transport")?,
        );
        let output = match super::super::check::execute_process(
            command,
            Duration::from_secs(u64::from(policy.timeout_secs)),
            Some((self.runs_db.clone(), owner(effect))),
            false,
        ) {
            Ok(output) => output,
            Err(super::super::check::ProcessError::NotStarted(reason)) => {
                // This error proves that no process entered, even for mutations.
                return Ok(ScriptObservation {
                    effect: EffectObservation {
                        outcome: EffectOutcome::Completed {
                            result: NeutralResult::Failed { diagnostic: reason },
                        },
                        stopped: true,
                    },
                    result: None,
                    stderr: String::new(),
                });
            }
            Err(super::super::check::ProcessError::Uncertain(diagnostic)) => {
                return Ok(ScriptObservation {
                    effect: EffectObservation {
                        outcome: EffectOutcome::Uncertain { diagnostic },
                        stopped: false,
                    },
                    result: None,
                    stderr: String::new(),
                });
            }
        };
        // Keep the private stdin file alive through process-tree stop and reap.
        drop(input_file);
        let stderr = redact_captured(&output.stderr, &secrets);
        let result = (|| {
            if output.stdout.len() > policy.stdout_bytes
                || output.stderr.len() > policy.stderr_bytes
            {
                return Err("script output limit exceeded".to_owned());
            }
            if output.cancelled || output.exit_code != 0 {
                return Err(format!(
                    "script stopped without successful result (exit {})",
                    output.exit_code
                ));
            }
            let result = match &policy.output {
                OutputMode::Structured => {
                    std::str::from_utf8(&output.stdout)
                        .map_err(|_| "script protocol: stdout is not UTF-8")?;
                    let stdout = redact_captured(&output.stdout, &secrets);
                    serde_json::from_str::<ScriptResult>(&stdout).map_err(|e| {
                        format!("script protocol: required JSON result missing or invalid: {e}")
                    })?
                }
                OutputMode::ExitStatus { outcome } => ScriptResult {
                    version: 1,
                    identity: effect.key.clone(),
                    outcome: outcome.clone(),
                    data: BTreeMap::new(),
                    artifacts: BTreeMap::new(),
                },
            };
            let completed = validate_result(effect, &policy, &result)?;
            Ok((result, completed))
        })();
        match result {
            Ok((result, completed)) => Ok(ScriptObservation {
                effect: EffectObservation {
                    outcome: EffectOutcome::Completed { result: completed },
                    stopped: true,
                },
                result: Some(result),
                stderr,
            }),
            Err(reason) => {
                let mut observation = failed(effect, reason, true);
                observation.stderr = stderr;
                Ok(observation)
            }
        }
    }
}
