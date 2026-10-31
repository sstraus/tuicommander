//! Opt-in integration saga. Git refs, not a lost process receipt, decide whether
//! publication happened. Proof debt and cleanup do not own the merge transition.
use super::*;
use schemars::JsonSchema;
use serde::Deserialize;
use std::fs::{File, OpenOptions};
use std::io::Write;

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct IntegrateRequest {
    pub run_id: String,
    pub story_id: String,
    pub command_id: String,
    pub expected_sequence: i64,
    pub expected_base: String,
    pub expected_source: String,
}

#[derive(Clone, Copy, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum IntegrationOutcome {
    RepairRequired,
    Integrated,
    /// The equivalent of the legacy exit 2; integration remains successful.
    IntegratedCleanupPending,
    /// The equivalent of exit 3, which takes precedence over cleanup refusal.
    IntegratedDependencyRefreshFailed,
    /// The equivalent of exit 4; the armed intent still owns the debt.
    IntegratedObligationPending,
    Prepared,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct IntegrationProgress {
    pub request: IntegrateRequest,
    pub project: String,
    pub repository: String,
    pub source_ref: String,
    pub workspace: String,
    pub candidate: String,
    pub validation_ref: String,
    pub validation_workspace: String,
    pub receipt: Option<IntegrationReceipt>,
    pub merged: bool,
    pub receipt_recorded: bool,
    pub receipt_sequence: Option<i64>,
    pub synthetic_checks: Vec<CheckReceipt>,
    pub dependencies_refreshed: bool,
    pub obligation_published: bool,
    pub cleanup_complete: bool,
    pub outcome: IntegrationOutcome,
    pub reason: Option<String>,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct IntegrationObligation {
    pub id: String,
    pub run_id: String,
    pub story_id: String,
    pub base_commit: String,
    pub source_commit: String,
    pub merge_commit: String,
    pub files: Vec<String>,
}

fn sql(error: rusqlite::Error) -> String {
    format!("integration journal: {error}")
}

fn repository(project: &Path) -> Result<PathBuf, String> {
    let common = git_output(
        project,
        &["rev-parse", "--path-format=absolute", "--git-common-dir"],
    )?;
    PathBuf::from(common)
        .canonicalize()
        .map_err(|e| format!("resolve canonical repository: {e}"))
}

/// The same lock path is used by every run, checkout and RunStore for this Git
/// common directory. OS ownership is released on process death, not a TTL.
fn lock_repository(project: &Path) -> Result<File, String> {
    let path = repository(project)?.join("tuic-workflow-integration.lock");
    let file = OpenOptions::new()
        .create(true)
        .truncate(false)
        .read(true)
        .write(true)
        .open(path)
        .map_err(|e| format!("open integration lock: {e}"))?;
    file.lock().map_err(|e| format!("lock integration: {e}"))?;
    Ok(file)
}

#[cfg(test)]
#[derive(Clone, Copy, PartialEq, Eq)]
pub(in crate::workflows::run) enum IntegrationFault {
    AfterMerge,
    Outbox,
    DependencyRefresh,
}
#[cfg(test)]
thread_local! {
    static FAULT: std::cell::Cell<Option<IntegrationFault>> = const { std::cell::Cell::new(None) };
    static BEFORE_PUBLISH: std::cell::RefCell<Option<Box<dyn FnOnce()>>> = const { std::cell::RefCell::new(None) };
}
#[cfg(test)]
pub(in crate::workflows::run) fn fault(fault: IntegrationFault) {
    FAULT.with(|slot| slot.set(Some(fault)));
}
#[cfg(test)]
pub(in crate::workflows::run) fn before_publish(hook: impl FnOnce() + 'static) {
    BEFORE_PUBLISH.with(|slot| *slot.borrow_mut() = Some(Box::new(hook)));
}
#[cfg(test)]
fn crash(point: IntegrationFault) -> Result<(), String> {
    if FAULT.with(|slot| {
        if slot.get() == Some(point) {
            slot.set(None);
            true
        } else {
            false
        }
    }) {
        Err("injected crash".into())
    } else {
        Ok(())
    }
}

impl RunStore {
    fn integration_db(&self) -> Result<Connection, String> {
        let conn = self.connect()?;
        conn.execute_batch("CREATE TABLE IF NOT EXISTS workflow_integration_policy (
            repository TEXT PRIMARY KEY, native_integration_enabled INTEGER NOT NULL DEFAULT 0, append_only_documents TEXT NOT NULL
        );
        CREATE TABLE IF NOT EXISTS workflow_integrations (
            run_id TEXT NOT NULL, story_id TEXT NOT NULL, repository TEXT NOT NULL,
            document TEXT NOT NULL, PRIMARY KEY(run_id,story_id)
        );
        CREATE TABLE IF NOT EXISTS workflow_integration_history (
            run_id TEXT NOT NULL, story_id TEXT NOT NULL, command_id TEXT NOT NULL,
            document TEXT NOT NULL, PRIMARY KEY(run_id,story_id,command_id)
        );
        CREATE TABLE IF NOT EXISTS workflow_integration_outbox (
            id TEXT PRIMARY KEY, repository TEXT NOT NULL, document TEXT NOT NULL
        );").map_err(sql)?;
        Ok(conn)
    }

    #[cfg(test)]
    pub fn set_native_integration_policy(
        &self,
        project: &Path,
        enabled: bool,
    ) -> Result<(), String> {
        self.set_integration_policy(project, enabled, &[])?;
        Ok(())
    }

    pub fn set_integration_policy(
        &self,
        project: &Path,
        enabled: bool,
        documents: &[String],
    ) -> Result<(), String> {
        if documents.len() > 32
            || documents.iter().any(|path| {
                !path.ends_with(".md")
                    || crate::fs::is_absolute_on_any_platform(path)
                    || Path::new(path)
                        .components()
                        .any(|c| !matches!(c, std::path::Component::Normal(_)))
                    || path.contains(['\n', '\r', '\0', '\\'])
            })
        {
            return Err(
                "append-only policy requires explicit repository-relative Markdown files".into(),
            );
        }
        let _lock = lock_repository(project)?;
        self.integration_db()?.execute("INSERT INTO workflow_integration_policy VALUES (?1,?2,?3)
            ON CONFLICT(repository) DO UPDATE SET native_integration_enabled=excluded.native_integration_enabled, append_only_documents=excluded.append_only_documents",
            params![repository(project)?.to_string_lossy(), enabled, serde_json::to_string(documents).map_err(|e| e.to_string())?]).map_err(sql)?;
        Ok(())
    }

    pub fn native_integration_enabled(&self, project: &Path) -> Result<bool, String> {
        Ok(self.integration_db()?.query_row("SELECT native_integration_enabled FROM workflow_integration_policy WHERE repository=?1",
            [repository(project)?.to_string_lossy()], |row| row.get(0)).optional().map_err(sql)?.unwrap_or(false))
    }

    pub fn integration_progress(
        &self,
        run: &str,
        story: &str,
    ) -> Result<Option<IntegrationProgress>, String> {
        let json: Option<String> = self
            .integration_db()?
            .query_row(
                "SELECT document FROM workflow_integrations WHERE run_id=?1 AND story_id=?2",
                params![run, story],
                |row| row.get(0),
            )
            .optional()
            .map_err(sql)?;
        json.map(|json| serde_json::from_str(&json).map_err(|e| e.to_string()))
            .transpose()
    }

    fn save_integration(&self, progress: &IntegrationProgress) -> Result<(), String> {
        self.integration_db()?
            .execute(
                "INSERT INTO workflow_integrations VALUES (?1,?2,?3,?4)
            ON CONFLICT(run_id,story_id) DO UPDATE SET document=excluded.document",
                params![
                    progress.request.run_id,
                    progress.request.story_id,
                    progress.repository,
                    serde_json::to_string(progress).map_err(|e| e.to_string())?
                ],
            )
            .map_err(sql)?;
        Ok(())
    }

    pub fn integration_obligations(
        &self,
        project: &Path,
    ) -> Result<Vec<IntegrationObligation>, String> {
        let conn = self.integration_db()?;
        let mut statement = conn
            .prepare(
                "SELECT document FROM workflow_integration_outbox WHERE repository=?1 ORDER BY id",
            )
            .map_err(sql)?;
        statement
            .query_map([repository(project)?.to_string_lossy()], |row| {
                row.get::<_, String>(0)
            })
            .map_err(sql)?
            .map(|row| serde_json::from_str(&row.map_err(sql)?).map_err(|e| e.to_string()))
            .collect()
    }

    /// Admission requires the accepted revision and its real pinned source checks.
    /// A retry must have the identical payload, including expected parent commits.
    pub fn integrate(&self, request: &IntegrateRequest) -> Result<IntegrationProgress, String> {
        let run = self.snapshot(&request.run_id)?;
        let project = Path::new(&run.project);
        let _lock = lock_repository(project)?;
        let mut progress = if let Some(prior) =
            self.integration_progress(&request.run_id, &request.story_id)?
        {
            if prior.request == *request {
                prior
            } else {
                if prior.request.command_id == request.command_id || prior.merged {
                    return Err("integration identity reused with a different payload".into());
                }
                if git_output(
                    project,
                    &["merge-base", "--is-ancestor", &prior.candidate, "HEAD"],
                )
                .is_ok()
                {
                    return Err(
                        "prior merge is published; recover its original request first".into(),
                    );
                }
                if !self.native_integration_enabled(project)? {
                    return Err("native_integration_enabled is false for this repository".into());
                }
                self.integration_db()?
                    .execute(
                        "INSERT OR IGNORE INTO workflow_integration_history VALUES (?1,?2,?3,?4)",
                        params![
                            prior.request.run_id,
                            prior.request.story_id,
                            prior.request.command_id,
                            serde_json::to_string(&prior).map_err(|e| e.to_string())?
                        ],
                    )
                    .map_err(sql)?;
                self.prepare_or_repair(&run, request)?
            }
        } else {
            if !self.native_integration_enabled(project)? {
                return Err("native_integration_enabled is false for this repository".into());
            }
            self.prepare_or_repair(&run, request)?
        };
        if progress.outcome != IntegrationOutcome::RepairRequired {
            self.resume_integration(&mut progress)?;
        }
        Ok(progress)
    }

    fn prepare_or_repair(
        &self,
        run: &RunSnapshot,
        request: &IntegrateRequest,
    ) -> Result<IntegrationProgress, String> {
        match self.prepare_integration(run, request) {
            Err(error) if error.starts_with("repair required:") => {
                let workspace = run
                    .stories
                    .iter()
                    .find(|s| s.story_id == request.story_id)
                    .and_then(|s| s.worktree_path.clone())
                    .ok_or("source workspace missing")?;
                let progress = IntegrationProgress {
                    request: request.clone(),
                    project: run.project.clone(),
                    repository: repository(Path::new(&run.project))?
                        .to_string_lossy()
                        .into_owned(),
                    source_ref: git_output(Path::new(&workspace), &["symbolic-ref", "HEAD"])?,
                    workspace,
                    candidate: String::new(),
                    validation_ref: String::new(),
                    validation_workspace: String::new(),
                    receipt: None,
                    merged: false,
                    receipt_recorded: false,
                    receipt_sequence: None,
                    synthetic_checks: vec![],
                    dependencies_refreshed: false,
                    obligation_published: false,
                    cleanup_complete: false,
                    outcome: IntegrationOutcome::RepairRequired,
                    reason: Some(error),
                };
                self.save_integration(&progress)?;
                Ok(progress)
            }
            result => result,
        }
    }

    /// Restart recovery may finish already-published phases, but never publishes
    /// an unmerged candidate. A policy rollback therefore cannot create a merge.
    pub(crate) fn recover_integration_outbox(&self) -> Result<usize, String> {
        let conn = self.integration_db()?;
        let mut statement = conn
            .prepare("SELECT document FROM workflow_integrations")
            .map_err(sql)?;
        let rows = statement
            .query_map([], |r| r.get::<_, String>(0))
            .map_err(sql)?
            .collect::<Result<Vec<_>, _>>()
            .map_err(sql)?;
        drop(statement);
        drop(conn);
        let mut count = 0;
        for row in rows {
            let mut progress: IntegrationProgress =
                serde_json::from_str(&row).map_err(|e| e.to_string())?;
            if progress.candidate.is_empty()
                || (progress.obligation_published
                    && progress.receipt_recorded
                    && progress.dependencies_refreshed)
            {
                continue;
            }
            let _lock = match lock_repository(Path::new(&progress.project)) {
                Ok(lock) => lock,
                Err(error) => {
                    tracing::warn!(run_id=%progress.request.run_id, %error, "Native integration recovery cannot open repository");
                    continue;
                }
            };
            // Refresh after the OS lock: a simultaneous retry may have completed.
            progress = self
                .integration_progress(&progress.request.run_id, &progress.request.story_id)?
                .ok_or("integration disappeared")?;
            if !progress.candidate.is_empty()
                && git_output(
                    Path::new(&progress.project),
                    &["merge-base", "--is-ancestor", &progress.candidate, "HEAD"],
                )
                .is_ok()
            {
                match self.resume_integration(&mut progress) {
                    Ok(()) => count += 1,
                    Err(error) => {
                        tracing::warn!(run_id=%progress.request.run_id, %error, "Native integration recovery remains pending")
                    }
                }
            }
        }
        Ok(count)
    }

    fn prepare_integration(
        &self,
        run: &RunSnapshot,
        request: &IntegrateRequest,
    ) -> Result<IntegrationProgress, String> {
        if request.command_id.trim().is_empty() || request.command_id.len() > 256 {
            return Err("integration command id must contain 1..256 bytes".into());
        }
        if run.sequence != request.expected_sequence {
            return Err("stale workflow sequence".into());
        }
        let repo = repository(Path::new(&run.project))?
            .to_string_lossy()
            .into_owned();
        let conn = self.integration_db()?;
        let unfinished: bool = conn.query_row("SELECT EXISTS(SELECT 1 FROM workflow_integrations WHERE repository=?1 AND json_extract(document,'$.merged')=0 AND json_extract(document,'$.candidate')<>'' AND NOT(run_id=?2 AND story_id=?3))", params![repo, request.run_id, request.story_id], |r| r.get(0)).map_err(sql)?;
        if unfinished {
            return Err(
                "canonical repository has an unfinished integration; recover it first".into(),
            );
        }
        let execution = run
            .stories
            .iter()
            .find(|s| s.story_id == request.story_id)
            .ok_or("story execution not found")?;
        let revision = execution.accepted_revision.ok_or("story is not accepted")?;
        let story = StoryStore::open()?.get_story(&request.story_id)?;
        if story.status != StoryStatus::Done
            || !story_work_revision_current(&story.id, revision, story.revision)?
        {
            return Err("integration requires the accepted story revision".into());
        }
        let workspace = execution
            .worktree_path
            .clone()
            .ok_or("story worktree is missing")?;
        let source = Path::new(&workspace);
        let project = Path::new(&run.project);
        if repository(source)?.to_string_lossy() != repo {
            return Err("source belongs to a different canonical repository".into());
        }
        let (source_commit, source_tree) = clean_artifact(source)?;
        let source_ref = git_output(source, &["symbolic-ref", "HEAD"])?;
        let canonical_ref = git_output(project, &["symbolic-ref", "HEAD"])?;
        let (base, _) = clean_artifact(project)?;
        if source_ref == canonical_ref || run.canonical_ref.as_deref() != Some(&canonical_ref) {
            return Err("canonical or source branch changed".into());
        }
        if base != request.expected_base || source_commit != request.expected_source {
            return Err("expected base or source moved; new merge checks required".into());
        }
        let definition = WorkflowStore::open()?
            .get_published(&run.story_definition_id, run.story_definition_revision)?;
        require_nonempty_policy(&definition)?;
        require_current_checks(
            &definition.required_checks,
            &execution.check_receipts,
            &source_ref,
            &source_commit,
            &source_tree,
        )?;
        require_merge_tree_support(project)?;
        let tree = self.integration_tree(project, &base, &source_commit)?;
        // A pinned two-parent commit is exactly a no-ff merge. Publish this object,
        // not a newly constructed, potentially different commit after validation.
        let candidate = git_output(
            project,
            &[
                "commit-tree",
                &tree,
                "-p",
                &base,
                "-p",
                &source_commit,
                "-m",
                &format!("Integrate {} ({})", request.story_id, request.command_id),
            ],
        )?;
        let id = Uuid::new_v4();
        let validation_ref = format!("refs/heads/tuic-integration/{id}");
        let validation_workspace = self
            .db_path
            .parent()
            .ok_or("run store parent missing")?
            .join(format!("integration-{id}"));
        let progress = IntegrationProgress {
            request: request.clone(),
            project: run.project.clone(),
            repository: repo,
            source_ref,
            workspace,
            candidate: candidate.clone(),
            validation_ref,
            validation_workspace: validation_workspace.to_string_lossy().into_owned(),
            receipt: Some(IntegrationReceipt {
                story_revision: revision,
                canonical_ref,
                base_commit: base,
                source_commit,
                source_tree,
                merge_commit: candidate,
                merge_tree: tree,
                post_checks: vec![],
            }),
            merged: false,
            receipt_recorded: false,
            receipt_sequence: None,
            synthetic_checks: vec![],
            dependencies_refreshed: false,
            obligation_published: false,
            cleanup_complete: false,
            outcome: IntegrationOutcome::Prepared,
            reason: None,
        };
        self.save_integration(&progress)?;
        Ok(progress)
    }

    fn resume_integration(&self, progress: &mut IntegrationProgress) -> Result<(), String> {
        let project = Path::new(&progress.project).to_path_buf();
        let receipt = progress
            .receipt
            .clone()
            .ok_or("integration receipt missing")?;
        // Inspect refs BEFORE checks, merge, dependency refresh or cleanup.
        let current_ref = git_output(&project, &["symbolic-ref", "HEAD"])?;
        let current = git_output(&project, &["rev-parse", "HEAD"])?;
        if current_ref != receipt.canonical_ref {
            return Err("canonical branch moved".into());
        }
        let published = git_output(
            &project,
            &["merge-base", "--is-ancestor", &progress.candidate, &current],
        )
        .is_ok();
        if progress.merged && !published {
            return Err(
                "published merge no longer belongs to canonical ref; repair required".into(),
            );
        }
        if published {
            progress.merged = true;
            self.save_integration(progress)?;
        } else {
            if current != receipt.base_commit {
                return Err("canonical base moved; new merge checks required".into());
            }
            if !self.native_integration_enabled(&project)? {
                return Err(
                    "native_integration_enabled is false; pending admission suspended".into(),
                );
            }
            let run = self.snapshot(&progress.request.run_id)?;
            let execution = run
                .stories
                .iter()
                .find(|s| s.story_id == progress.request.story_id)
                .ok_or("story execution missing")?;
            let story = StoryStore::open()?.get_story(&progress.request.story_id)?;
            if execution.accepted_revision != Some(receipt.story_revision)
                || story.status != StoryStatus::Done
                || !story_work_revision_current(&story.id, receipt.story_revision, story.revision)?
            {
                return Err("accepted story changed; new merge checks required".into());
            }
            self.validate_candidate(progress)?;
            #[cfg(test)]
            BEFORE_PUBLISH.with(|slot| {
                if let Some(hook) = slot.borrow_mut().take() {
                    hook();
                }
            });
            if clean_artifact(&project)?.0 != receipt.base_commit
                || git_output(&project, &["symbolic-ref", "HEAD"])? != receipt.canonical_ref
                || clean_artifact(Path::new(&progress.workspace))?
                    != (receipt.source_commit.clone(), receipt.source_tree.clone())
                || git_output(Path::new(&progress.workspace), &["symbolic-ref", "HEAD"])?
                    != progress.source_ref
            {
                return Err("base or source moved after synthetic merge checks".into());
            }
            // Fast-forward to the already checked no-ff merge object. Git owns
            // the index/worktree/ref transaction; no reset/force can discard dirt.
            git_output(
                &project,
                &[
                    "merge",
                    "--ff-only",
                    "--no-overwrite-ignore",
                    "--no-edit",
                    &progress.candidate,
                ],
            )?;
            #[cfg(test)]
            crash(IntegrationFault::AfterMerge)?;
            if git_output(&project, &["rev-parse", "HEAD"])? != progress.candidate {
                return Err("canonical ref moved during publication; recover refs".into());
            }
            progress.merged = true;
            self.save_integration(progress)?;
        }
        if !progress.obligation_published
            && let Err(error) = self.publish_obligation(progress)
        {
            progress.outcome = IntegrationOutcome::IntegratedObligationPending;
            progress.reason = Some(error);
            self.save_integration(progress)?;
            return Ok(());
        }
        if !progress.receipt_recorded {
            // A concurrent operator can advance the run sequence after publication.
            // Debt already exists and a replay never tries another merge.
            if progress
                .receipt
                .as_ref()
                .is_none_or(|r| r.post_checks.is_empty())
            {
                let post_checks = (|| -> Result<Vec<CheckReceipt>, String> {
                    if clean_artifact(&project)?
                        != (receipt.merge_commit.clone(), receipt.merge_tree.clone())
                    {
                        return Err("canonical ref moved before post-integration checks; explicit recertification required".into());
                    }
                    let run = self.snapshot(&progress.request.run_id)?;
                    let definition = WorkflowStore::open()?
                        .get_published(&run.story_definition_id, run.story_definition_revision)?;
                    let mut checks = Vec::new();
                    for check in &definition.required_checks {
                        let checked =
                            execute_run_check(check, &project, &self.db_path, &run.id, false)?;
                        if checked.exit_code != 0 {
                            return Err(format!("post-integration check {} failed", check.id));
                        }
                        checks.push(checked);
                    }
                    Ok(checks)
                })();
                let checks = match post_checks {
                    Ok(checks) => checks,
                    Err(error) => {
                        progress.outcome = IntegrationOutcome::IntegratedObligationPending;
                        progress.reason = Some(error);
                        self.save_integration(progress)?;
                        return Ok(());
                    }
                };
                progress
                    .receipt
                    .as_mut()
                    .ok_or("receipt missing")?
                    .post_checks = checks;
                self.save_integration(progress)?;
            }
            // This fixed internal payload is idempotent by command ID. Operator
            // events must not permanently pin recovery to a pre-crash cursor.
            let command = format!("native-integration:{}", progress.request.command_id);
            if let Err(error) = self.command(
                &progress.request.run_id,
                &command,
                RunCommand::RecordIntegration {
                    story_id: progress.request.story_id.clone(),
                    receipt: progress.receipt.clone().ok_or("receipt missing")?,
                },
            ) {
                progress.outcome = IntegrationOutcome::IntegratedObligationPending;
                progress.reason = Some(error);
                self.save_integration(progress)?;
                return Ok(());
            }
            progress.receipt_recorded = true;
            self.save_integration(progress)?;
        }
        if !progress.dependencies_refreshed {
            match self.refresh_integration_dependencies(progress) {
                Ok(()) => {
                    progress.dependencies_refreshed = true;
                    self.save_integration(progress)?;
                }
                Err(error) => {
                    progress.outcome = IntegrationOutcome::IntegratedDependencyRefreshFailed;
                    progress.reason = Some(error);
                    self.save_integration(progress)?;
                    return Ok(());
                }
            }
        }
        progress.outcome = if progress.cleanup_complete {
            IntegrationOutcome::Integrated
        } else {
            IntegrationOutcome::IntegratedCleanupPending
        };
        progress.reason = None;
        self.save_integration(progress)
    }

    fn integration_tree(&self, project: &Path, base: &str, source: &str) -> Result<String, String> {
        let output = Command::new("git")
            .args([
                "merge-tree",
                "--write-tree",
                "--name-only",
                "--no-messages",
                base,
                source,
            ])
            .current_dir(project)
            .output()
            .map_err(|e| e.to_string())?;
        let text = String::from_utf8(output.stdout).map_err(|e| e.to_string())?;
        if output.status.success() {
            return Ok(text.trim().into());
        }
        if output.status.code() != Some(1) {
            return Err(format!(
                "merge-tree failed: {}",
                String::from_utf8_lossy(&output.stderr)
            ));
        }
        let mut lines = text.lines();
        let tree = lines.next().ok_or("repair required: no conflicted tree")?;
        let conflicts: Vec<_> = lines.filter(|s| !s.is_empty()).collect();
        let json: String = self
            .integration_db()?
            .query_row(
                "SELECT append_only_documents FROM workflow_integration_policy WHERE repository=?1",
                [repository(project)?.to_string_lossy()],
                |r| r.get(0),
            )
            .map_err(sql)?;
        let declared: Vec<String> = serde_json::from_str(&json).map_err(|e| e.to_string())?;
        if conflicts.is_empty() || conflicts.iter().any(|p| !declared.iter().any(|d| d == p)) {
            return Err("repair required: code or undeclared document conflict".into());
        }
        let ancestor = git_output(project, &["merge-base", base, source])?;
        let scratch =
            tempfile::tempdir_in(self.db_path.parent().ok_or("run store parent missing")?)
                .map_err(|e| e.to_string())?;
        let index = scratch.path().join("index");
        let indexed = |args: &[&str]| -> Result<String, String> {
            let output = Command::new("git")
                .args(args)
                .env("GIT_INDEX_FILE", &index)
                .current_dir(project)
                .output()
                .map_err(|e| e.to_string())?;
            if !output.status.success() {
                return Err(format!(
                    "repair required: index operation failed: {}",
                    String::from_utf8_lossy(&output.stderr)
                ));
            }
            String::from_utf8(output.stdout)
                .map(|s| s.trim().into())
                .map_err(|e| e.to_string())
        };
        indexed(&["read-tree", tree])?;
        for path in conflicts {
            for tip in [base, source] {
                let delta =
                    git_output(project, &["diff", "--numstat", &ancestor, tip, "--", path])?;
                if delta
                    .lines()
                    .any(|line| line.split('\t').nth(1) != Some("0"))
                {
                    return Err("repair required: declared document is not append-only".into());
                }
            }
            let entry = git_output(project, &["ls-tree", tree, "--", path])?;
            if !entry.starts_with("100644 blob ") {
                return Err("repair required: union requires a regular Markdown file".into());
            }
            let shown = Command::new("git")
                .args(["show", &format!("{tree}:{path}")])
                .current_dir(project)
                .output()
                .map_err(|e| e.to_string())?;
            if !shown.status.success() {
                return Err("repair required: cannot read conflicted document".into());
            }
            let conflicted = String::from_utf8(shown.stdout).map_err(|e| e.to_string())?;
            let merged = union_document(&conflicted)?;
            let mut child = Command::new("git")
                .args(["hash-object", "-w", "--stdin"])
                .current_dir(project)
                .stdin(std::process::Stdio::piped())
                .stdout(std::process::Stdio::piped())
                .stderr(std::process::Stdio::piped())
                .spawn()
                .map_err(|e| e.to_string())?;
            child
                .stdin
                .take()
                .ok_or("hash-object stdin missing")?
                .write_all(merged.as_bytes())
                .map_err(|e| e.to_string())?;
            let output = child.wait_with_output().map_err(|e| e.to_string())?;
            if !output.status.success() {
                return Err("repair required: document blob write failed".into());
            }
            let blob = String::from_utf8(output.stdout).map_err(|e| e.to_string())?;
            indexed(&[
                "update-index",
                "--cacheinfo",
                &format!("100644,{},{}", blob.trim(), path),
            ])?;
        }
        indexed(&["write-tree"])
    }

    fn validate_candidate(&self, progress: &mut IntegrationProgress) -> Result<(), String> {
        let project = Path::new(&progress.project);
        let check_path = Path::new(&progress.validation_workspace);
        if !check_path.exists() {
            let branch = progress
                .validation_ref
                .strip_prefix("refs/heads/")
                .ok_or("invalid validation ref")?;
            let existing = git_output(
                project,
                &[
                    "for-each-ref",
                    "--format=%(objectname)",
                    &progress.validation_ref,
                ],
            )?;
            if existing.is_empty() {
                git_output(
                    project,
                    &[
                        "worktree",
                        "add",
                        "-b",
                        branch,
                        &progress.validation_workspace,
                        &progress.candidate,
                    ],
                )?;
            } else {
                if existing != progress.candidate {
                    return Err("synthetic validation ref changed; repair required".into());
                }
                git_output(
                    project,
                    &["worktree", "add", &progress.validation_workspace, branch],
                )?;
            }
        }
        if clean_artifact(check_path)?.0 != progress.candidate
            || git_output(check_path, &["symbolic-ref", "HEAD"])? != progress.validation_ref
        {
            return Err("synthetic check workspace changed; repair required".into());
        }
        let run = self.snapshot(&progress.request.run_id)?;
        let definition = WorkflowStore::open()?
            .get_published(&run.story_definition_id, run.story_definition_revision)?;
        let mut checks = Vec::new();
        for check in &definition.required_checks {
            let receipt = execute_run_check(check, check_path, &self.db_path, &run.id, false)?;
            if receipt.exit_code != 0 {
                return Err(format!("synthetic merge check {} failed", check.id));
            }
            checks.push(receipt);
        }
        progress.synthetic_checks = checks;
        self.save_integration(progress)?;
        // The check workspace is scratch but never force-remove a check's output.
        // An unclean one is retained with its durable path for explicit recovery.
        git_output(
            project,
            &["worktree", "remove", &progress.validation_workspace],
        )?;
        git_output(
            project,
            &[
                "update-ref",
                "-d",
                &progress.validation_ref,
                &progress.candidate,
            ],
        )?;
        Ok(())
    }

    fn publish_obligation(&self, progress: &mut IntegrationProgress) -> Result<(), String> {
        #[cfg(test)]
        crash(IntegrationFault::Outbox)?;
        let receipt = progress.receipt.as_ref().ok_or("receipt missing")?;
        let files = git_output(
            Path::new(&progress.project),
            &[
                "diff",
                "--name-only",
                "-z",
                "--no-renames",
                &receipt.base_commit,
                &receipt.merge_commit,
            ],
        )?;
        let obligation = IntegrationObligation {
            id: format!(
                "{}:{}:{}",
                progress.request.run_id, progress.request.story_id, receipt.merge_commit
            ),
            run_id: progress.request.run_id.clone(),
            story_id: progress.request.story_id.clone(),
            base_commit: receipt.base_commit.clone(),
            source_commit: receipt.source_commit.clone(),
            merge_commit: receipt.merge_commit.clone(),
            files: files
                .split('\0')
                .filter(|path| !path.is_empty())
                .map(str::to_owned)
                .collect(),
        };
        let mut conn = self.integration_db()?;
        let tx = conn
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .map_err(sql)?;
        tx.execute(
            "INSERT OR IGNORE INTO workflow_integration_outbox VALUES (?1,?2,?3)",
            params![
                obligation.id,
                progress.repository,
                serde_json::to_string(&obligation).map_err(|e| e.to_string())?
            ],
        )
        .map_err(sql)?;
        progress.obligation_published = true;
        tx.execute(
            "UPDATE workflow_integrations SET document=?3 WHERE run_id=?1 AND story_id=?2",
            params![
                progress.request.run_id,
                progress.request.story_id,
                serde_json::to_string(progress).map_err(|e| e.to_string())?
            ],
        )
        .map_err(sql)?;
        tx.commit().map_err(sql)
    }

    fn refresh_integration_dependencies(
        &self,
        progress: &IntegrationProgress,
    ) -> Result<(), String> {
        #[cfg(test)]
        crash(IntegrationFault::DependencyRefresh)?;
        let project = Path::new(&progress.project);
        let receipt = progress.receipt.as_ref().ok_or("receipt missing")?;
        let lock_changed = !git_output(
            project,
            &[
                "diff",
                "--name-only",
                &receipt.base_commit,
                &receipt.merge_commit,
                "--",
                "pnpm-lock.yaml",
            ],
        )?
        .is_empty();
        if lock_changed && project.join("node_modules").is_dir() {
            let mut command = Command::new("pnpm");
            command
                .args(["install", "--offline", "--frozen-lockfile"])
                .current_dir(project)
                .stdin(std::process::Stdio::null())
                .stdout(std::process::Stdio::null())
                .stderr(std::process::Stdio::null());
            let status = super::super::check::execute_process(
                command,
                Duration::from_secs(600),
                Some((self.db_path.clone(), progress.request.run_id.clone())),
                false,
            )
            .map_err(String::from)?;
            if status.exit_code != 0 || status.cancelled {
                return Err("dependency refresh failed; canonical merge is retained".into());
            }
            for name in [
                "vite.config.ts",
                "vite.config.js",
                "vite.config.mts",
                "vite.config.mjs",
            ] {
                let path = project.join(name);
                if path.is_file() {
                    File::options()
                        .write(true)
                        .open(path)
                        .and_then(|f| f.set_modified(SystemTime::now()))
                        .map_err(|e| e.to_string())?;
                }
            }
        }
        StoryStore::open()?
            .reconcile_integrated_dependencies(&self.snapshot(&progress.request.run_id)?.plan_id)?;
        Ok(())
    }

    /// Cleanup is explicitly requested, never implied by an integration receipt.
    /// It proves exact tips and all native ownership before the TUIC removal port.
    pub fn cleanup_integration(
        &self,
        run: &str,
        story: &str,
        state: Option<&crate::AppState>,
    ) -> Result<IntegrationProgress, String> {
        let mut progress = self
            .integration_progress(run, story)?
            .ok_or("integration not found")?;
        let _lock = lock_repository(Path::new(&progress.project))?;
        self.resume_integration(&mut progress)?;
        if !progress.dependencies_refreshed
            || !progress.obligation_published
            || !progress.receipt_recorded
            || progress.cleanup_complete
        {
            return Ok(progress);
        }
        match self.remove_integration_workspace(&progress, state) {
            Ok(()) => {
                progress.cleanup_complete = true;
                progress.outcome = IntegrationOutcome::Integrated;
                progress.reason = None;
            }
            Err(error) => {
                progress.outcome = IntegrationOutcome::IntegratedCleanupPending;
                progress.reason = Some(error);
            }
        }
        self.save_integration(&progress)?;
        Ok(progress)
    }

    fn remove_integration_workspace(
        &self,
        progress: &IntegrationProgress,
        state: Option<&crate::AppState>,
    ) -> Result<(), String> {
        let project = Path::new(&progress.project);
        let workspace = Path::new(&progress.workspace);
        let receipt = progress.receipt.as_ref().ok_or("receipt missing")?;
        git_output(
            project,
            &[
                "merge-base",
                "--is-ancestor",
                &receipt.merge_commit,
                &receipt.canonical_ref,
            ],
        )?;
        let branch = progress
            .source_ref
            .strip_prefix("refs/heads/")
            .ok_or("source branch missing")?;
        let source_tip = git_output(
            project,
            &[
                "for-each-ref",
                "--format=%(objectname)",
                &progress.source_ref,
            ],
        )?;
        if !source_tip.is_empty() && source_tip != receipt.source_commit {
            return Err("source branch has unattributed commits".into());
        }
        // Fence native successor assignment and managed launch/close through the
        // destructive boundary, not just while taking an ownership snapshot.
        let mut ownership = self.connect()?;
        let _ownership = ownership
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .map_err(sql)?;
        let _launch = state
            .map(|state| {
                state
                    .launch_transaction_lock
                    .try_lock()
                    .map_err(|_| "managed launch/close is active; cleanup remains pending")
            })
            .transpose()?;
        self.require_workspace_released(progress, state)?;
        if workspace.exists() {
            if !git_output(
                workspace,
                &[
                    "ls-files",
                    "--others",
                    "--ignored",
                    "--exclude-standard",
                    "-z",
                ],
            )?
            .is_empty()
            {
                return Err("workspace has ignored work without disposal attribution".into());
            }
            if clean_artifact(workspace)?
                != (receipt.source_commit.clone(), receipt.source_tree.clone())
                || git_output(workspace, &["symbolic-ref", "HEAD"])? != progress.source_ref
            {
                return Err("workspace has unattributed work or changed identity".into());
            }
            // Exact-tip archive is durable BEFORE branch retirement, and is never
            // overwritten on replay. Unknown work has already refused above.
            let archive = format!("refs/archive/{}", branch);
            if let Ok(prior) = git_output(project, &["rev-parse", "--verify", &archive]) {
                if prior != receipt.source_commit {
                    return Err("exact-tip archive already belongs to different work".into());
                }
            } else {
                git_output(
                    project,
                    &[
                        "update-ref",
                        &archive,
                        &receipt.source_commit,
                        &"0".repeat(receipt.source_commit.len()),
                    ],
                )?;
            }
            // Reuse TUIC's identity/lifecycle/lock/integration guard, without force.
            let outcome = tuic_git::worktree::remove_worktree_by_workspace_id(
                project.to_str().ok_or("project path encoding")?,
                branch,
                true,
                None,
                false,
            )?;
            if let Some(reason) = outcome.branch_delete_warning {
                return Err(reason);
            }
            if let Some(state) = state {
                state.notify_worktree_removed(crate::state::WorktreeRemovedPayload {
                    repo_path: progress.project.clone(),
                    workspace_id: branch.into(),
                    branch: branch.into(),
                });
            }
        } else if !source_tip.is_empty() {
            let archive = format!("refs/archive/{branch}");
            if git_output(project, &["rev-parse", "--verify", &archive])? != receipt.source_commit {
                return Err("exact-tip archive does not preserve the source commit".into());
            }
            tuic_git::worktree::delete_integrated_local_branch_at_tip(
                project.to_str().ok_or("project path encoding")?,
                branch,
                &receipt.source_commit,
            )?;
        }
        Ok(())
    }

    fn require_workspace_released(
        &self,
        progress: &IntegrationProgress,
        state: Option<&crate::AppState>,
    ) -> Result<(), String> {
        let conn = self.connect()?;
        let mut statement = conn
            .prepare("SELECT snapshot_json FROM workflow_runs")
            .map_err(sql)?;
        let snapshots = statement
            .query_map([], |r| r.get::<_, String>(0))
            .map_err(sql)?;
        for json in snapshots {
            let run: RunSnapshot =
                serde_json::from_str(&json.map_err(sql)?).map_err(|e| e.to_string())?;
            for execution in &run.stories {
                if execution.worktree_path.as_deref() != Some(&progress.workspace) {
                    continue;
                }
                if run.id != progress.request.run_id
                    || execution.story_id != progress.request.story_id
                {
                    return Err("workspace belongs to another run/successor".into());
                }
                for attempt in run
                    .attempts
                    .iter()
                    .filter(|a| a.story_id == execution.story_id)
                {
                    if attempt.state == AttemptState::Running {
                        return Err("successor/worker still owns the workspace".into());
                    }
                    if attempt.agent.is_some() {
                        let state =
                            state.ok_or("managed process ownership needs the owning daemon")?;
                        let closes = crate::mcp_http::launch_transaction::workflow_agent_closes(
                            state, &run,
                        )?;
                        if !closes.as_array().is_some_and(|closes| {
                            closes.iter().any(|close| {
                                close["attemptId"] == attempt.id && close["state"] == "closed"
                            })
                        }) {
                            return Err(
                                "worker process has no durable close/successor receipt".into()
                            );
                        }
                    }
                }
            }
        }
        if let Some(state) = state
            && !crate::worktree::live_sessions_in(state, Path::new(&progress.workspace)).is_empty()
        {
            return Err("workspace has live or suspended users".into());
        }
        Ok(())
    }
}

/// Strict native conflict grammar: malformed/nested/diff3 blocks refuse rather
/// than turning a code edit or retained marker into an apparently clean union.
fn union_document(text: &str) -> Result<String, String> {
    let mut result = String::new();
    let mut state = 0;
    let mut blocks = 0;
    for line in text.lines() {
        if line.starts_with("<<<<<<< ") {
            if state != 0 {
                return Err("repair required: malformed document conflict".into());
            }
            state = 1;
            blocks += 1;
        } else if line == "=======" {
            if state != 1 {
                return Err("repair required: malformed document conflict".into());
            }
            state = 2;
            result.push('\n');
        } else if line.starts_with(">>>>>>> ") {
            if state != 2 {
                return Err("repair required: malformed document conflict".into());
            }
            state = 0;
        } else {
            if ["<<<<<<<", "=======", ">>>>>>>", "|||||||"]
                .iter()
                .any(|marker| line.starts_with(marker))
            {
                return Err("repair required: malformed document conflict".into());
            }
            result.push_str(line);
            result.push('\n');
        }
    }
    if state != 0 || blocks == 0 {
        return Err("repair required: malformed document conflict".into());
    }
    Ok(result)
}
