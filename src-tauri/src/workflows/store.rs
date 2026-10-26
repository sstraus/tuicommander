use super::definition::*;
use rusqlite::{Connection, OptionalExtension, TransactionBehavior, params};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use std::time::Duration;
use uuid::Uuid;

#[derive(Clone, Copy, Debug, Default, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum WorkflowClosure {
    #[default]
    Human,
    Automatic,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct CheckDefinition {
    pub id: String,
    pub argv: Vec<String>,
    pub timeout_secs: u32,
}

fn validate_checks(checks: &[CheckDefinition]) -> Result<(), String> {
    if checks.len() > 16 {
        return Err("too many workflow checks".into());
    }
    let mut ids = std::collections::HashSet::new();
    for check in checks {
        if check.id.is_empty()
            || check.id.len() > 80
            || !check
                .id
                .bytes()
                .all(|byte| byte.is_ascii_alphanumeric() || byte == b'_' || byte == b'-')
            || !ids.insert(&check.id)
        {
            return Err("invalid or duplicate workflow check id".into());
        }
        if check.argv.is_empty()
            || check.argv.len() > 32
            || !(1..=600).contains(&check.timeout_secs)
            || check
                .argv
                .iter()
                .any(|arg| arg.is_empty() || arg.len() > 4096 || arg.contains('\0'))
        {
            return Err("invalid workflow check command".into());
        }
        let executable = Path::new(&check.argv[0])
            .file_name()
            .and_then(|name| name.to_str())
            .unwrap_or("")
            .to_ascii_lowercase();
        if matches!(
            executable.as_str(),
            "sh" | "bash"
                | "zsh"
                | "fish"
                | "cmd"
                | "cmd.exe"
                | "powershell"
                | "powershell.exe"
                | "pwsh"
                | "pwsh.exe"
        ) {
            return Err("workflow checks must invoke an executable without a shell".into());
        }
    }
    Ok(())
}

impl WorkflowClosure {
    fn as_str(self) -> &'static str {
        match self {
            Self::Human => "human",
            Self::Automatic => "automatic",
        }
    }
    fn parse(value: &str) -> Result<Self, String> {
        match value {
            "human" => Ok(Self::Human),
            "automatic" => Ok(Self::Automatic),
            _ => Err("invalid workflow closure in store".into()),
        }
    }
}

/// A draft with no edit since its last publication cannot be published again.
pub const PUBLISH_UNCHANGED_ERROR: &str = "draft revision has already been published";

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct WorkflowDraft {
    pub id: String,
    pub project: String,
    pub name: String,
    pub kind: WorkflowKind,
    pub closure: WorkflowClosure,
    #[serde(default)]
    pub required_checks: Vec<CheckDefinition>,
    pub graph: WorkflowGraph,
    pub draft_revision: i64,
    pub latest_published_revision: i64,
    pub last_published_draft_revision: i64,
    pub builtin_key: Option<String>,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct PublishedWorkflow {
    pub id: String,
    pub project: String,
    pub name: String,
    pub kind: WorkflowKind,
    pub closure: WorkflowClosure,
    #[serde(default)]
    pub required_checks: Vec<CheckDefinition>,
    pub graph: WorkflowGraph,
    pub revision: i64,
}

#[derive(Clone, Debug)]
pub struct WorkflowStore {
    db_path: PathBuf,
}

impl WorkflowStore {
    pub fn open() -> Result<Self, String> {
        Self::open_at(&crate::config::config_dir().join("workflows.sqlite3"))
    }

    pub(crate) fn open_at(path: &Path) -> Result<Self, String> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)
                .map_err(|e| format!("create workflow store directory: {e}"))?;
        }
        let store = Self {
            db_path: path.to_path_buf(),
        };
        store.connect()?;
        Ok(store)
    }

    fn connect(&self) -> Result<Connection, String> {
        let conn =
            Connection::open(&self.db_path).map_err(|e| format!("open workflow store: {e}"))?;
        conn.busy_timeout(Duration::from_secs(5))
            .map_err(|e| format!("workflow busy timeout: {e}"))?;
        conn.pragma_update(None, "journal_mode", "WAL")
            .map_err(|e| format!("workflow WAL: {e}"))?;
        conn.pragma_update(None, "foreign_keys", "ON")
            .map_err(|e| format!("workflow foreign keys: {e}"))?;
        conn.execute_batch(
            "CREATE TABLE IF NOT EXISTS workflow_definitions (
                id TEXT PRIMARY KEY,
                project TEXT NOT NULL,
                name TEXT NOT NULL,
                kind TEXT NOT NULL CHECK(kind IN ('plan','story')),
                closure TEXT NOT NULL DEFAULT 'human' CHECK(closure IN ('human','automatic')),
                checks_json TEXT NOT NULL DEFAULT '[]',
                graph_json TEXT NOT NULL,
                draft_revision INTEGER NOT NULL,
                latest_published_revision INTEGER NOT NULL DEFAULT 0,
                last_published_draft_revision INTEGER NOT NULL DEFAULT 0,
                builtin_key TEXT,
                UNIQUE(project,builtin_key)
            );
            CREATE INDEX IF NOT EXISTS workflow_definitions_project ON workflow_definitions(project);
            CREATE TABLE IF NOT EXISTS workflow_published (
                id TEXT NOT NULL REFERENCES workflow_definitions(id),
                revision INTEGER NOT NULL,
                project TEXT NOT NULL,
                name TEXT NOT NULL,
                kind TEXT NOT NULL,
                closure TEXT NOT NULL DEFAULT 'human' CHECK(closure IN ('human','automatic')),
                checks_json TEXT NOT NULL DEFAULT '[]',
                graph_json TEXT NOT NULL,
                PRIMARY KEY(id,revision)
            );",
        ).map_err(|e| format!("prepare workflow schema: {e}"))?;
        for table in ["workflow_definitions", "workflow_published"] {
            let has_closure: bool = conn
                .query_row(
                    &format!(
                        "SELECT count(*) FROM pragma_table_info('{table}') WHERE name='closure'"
                    ),
                    [],
                    |row| row.get::<_, i64>(0),
                )
                .map_err(|e| format!("inspect workflow closure schema: {e}"))?
                != 0;
            if !has_closure {
                conn.execute_batch(&format!("ALTER TABLE {table} ADD COLUMN closure TEXT NOT NULL DEFAULT 'human' CHECK(closure IN ('human','automatic'))"))
                    .map_err(|e| format!("migrate workflow closure schema: {e}"))?;
            }
            let has_checks: bool = conn
                .query_row(
                    &format!(
                        "SELECT count(*) FROM pragma_table_info('{table}') WHERE name='checks_json'"
                    ),
                    [],
                    |row| row.get::<_, i64>(0),
                )
                .map_err(|e| format!("inspect workflow check schema: {e}"))?
                != 0;
            if !has_checks {
                conn.execute_batch(&format!(
                    "ALTER TABLE {table} ADD COLUMN checks_json TEXT NOT NULL DEFAULT '[]'"
                ))
                .map_err(|e| format!("migrate workflow check schema: {e}"))?;
            }
        }
        Ok(conn)
    }

    pub fn create_draft(
        &self,
        project: &str,
        name: &str,
        kind: WorkflowKind,
        graph: WorkflowGraph,
    ) -> Result<WorkflowDraft, String> {
        validate_identity(project, name)?;
        let draft = WorkflowDraft {
            id: Uuid::now_v7().to_string(),
            project: project.into(),
            name: name.trim().into(),
            kind,
            closure: WorkflowClosure::Human,
            required_checks: vec![],
            graph,
            draft_revision: 1,
            latest_published_revision: 0,
            last_published_draft_revision: 0,
            builtin_key: None,
        };
        self.connect()?.execute(
            "INSERT INTO workflow_definitions(id,project,name,kind,graph_json,draft_revision) VALUES (?1,?2,?3,?4,?5,1)",
            params![draft.id, draft.project, draft.name, kind_str(draft.kind), encode_graph(&draft.graph)?],
        ).map_err(|e| format!("create workflow draft: {e}"))?;
        Ok(draft)
    }

    pub fn get_draft(&self, id: &str) -> Result<WorkflowDraft, String> {
        read_draft(&self.connect()?, id)
    }

    pub fn list_drafts(&self, project: &str) -> Result<Vec<WorkflowDraft>, String> {
        let conn = self.connect()?;
        let mut stmt = conn
            .prepare("SELECT id FROM workflow_definitions WHERE project=?1 ORDER BY rowid")
            .map_err(|e| format!("prepare workflow list: {e}"))?;
        let ids = stmt
            .query_map([project], |row| row.get::<_, String>(0))
            .map_err(|e| format!("list workflow drafts: {e}"))?
            .collect::<Result<Vec<_>, _>>()
            .map_err(|e| format!("read workflow ids: {e}"))?;
        ids.iter().map(|id| read_draft(&conn, id)).collect()
    }

    pub fn update_draft(
        &self,
        id: &str,
        expected_revision: i64,
        graph: WorkflowGraph,
    ) -> Result<WorkflowDraft, String> {
        let mut conn = self.connect()?;
        let tx = conn
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .map_err(|e| format!("begin workflow edit: {e}"))?;
        let draft = read_draft(&tx, id)?;
        if draft.draft_revision != expected_revision {
            return Err("stale workflow draft revision".into());
        }
        tx.execute("UPDATE workflow_definitions SET graph_json=?1,draft_revision=draft_revision+1 WHERE id=?2 AND draft_revision=?3",
            params![encode_graph(&graph)?, id, expected_revision])
            .map_err(|e| format!("update workflow draft: {e}"))?;
        let updated = read_draft(&tx, id)?;
        tx.commit()
            .map_err(|e| format!("commit workflow edit: {e}"))?;
        Ok(updated)
    }

    pub fn update_closure(
        &self,
        id: &str,
        expected_revision: i64,
        closure: WorkflowClosure,
    ) -> Result<WorkflowDraft, String> {
        let mut conn = self.connect()?;
        let tx = conn
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .map_err(|e| format!("begin workflow closure edit: {e}"))?;
        let draft = read_draft(&tx, id)?;
        if draft.draft_revision != expected_revision {
            return Err("stale workflow draft revision".into());
        }
        tx.execute("UPDATE workflow_definitions SET closure=?1,draft_revision=draft_revision+1 WHERE id=?2 AND draft_revision=?3",
            params![closure.as_str(), id, expected_revision])
            .map_err(|e| format!("update workflow closure: {e}"))?;
        let updated = read_draft(&tx, id)?;
        tx.commit()
            .map_err(|e| format!("commit workflow closure edit: {e}"))?;
        Ok(updated)
    }

    pub fn update_checks(
        &self,
        id: &str,
        expected_revision: i64,
        checks: Vec<CheckDefinition>,
    ) -> Result<WorkflowDraft, String> {
        validate_checks(&checks)?;
        let mut conn = self.connect()?;
        let tx = conn
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .map_err(|e| format!("begin workflow check edit: {e}"))?;
        let draft = read_draft(&tx, id)?;
        if draft.draft_revision != expected_revision {
            return Err("stale workflow draft revision".into());
        }
        tx.execute("UPDATE workflow_definitions SET checks_json=?1,draft_revision=draft_revision+1 WHERE id=?2 AND draft_revision=?3",
            params![serde_json::to_string(&checks).map_err(|e| format!("encode workflow checks: {e}"))?, id, expected_revision])
            .map_err(|e| format!("update workflow checks: {e}"))?;
        let updated = read_draft(&tx, id)?;
        tx.commit()
            .map_err(|e| format!("commit workflow check edit: {e}"))?;
        Ok(updated)
    }

    pub fn publish(
        &self,
        id: &str,
        expected_draft_revision: i64,
    ) -> Result<PublishedWorkflow, String> {
        let mut conn = self.connect()?;
        let tx = conn
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .map_err(|e| format!("begin workflow publish: {e}"))?;
        let draft = read_draft(&tx, id)?;
        if draft.draft_revision != expected_draft_revision {
            return Err("stale workflow draft revision".into());
        }
        let last: i64 = tx
            .query_row(
                "SELECT last_published_draft_revision FROM workflow_definitions WHERE id=?1",
                [id],
                |row| row.get(0),
            )
            .map_err(|e| format!("read workflow publication state: {e}"))?;
        if last == draft.draft_revision {
            return Err(PUBLISH_UNCHANGED_ERROR.into());
        }
        validate_graph(&draft.graph, draft.kind)?;
        validate_checks(&draft.required_checks)?;
        if draft.kind == WorkflowKind::Story && draft.required_checks.is_empty() {
            return Err("a story workflow needs at least one required check to publish".into());
        }
        if draft.closure == WorkflowClosure::Automatic {
            return Err(
                "automatic closure cannot be published until its evidence gate exists".into(),
            );
        }
        validate_pinned_templates(&tx, &draft)?;
        let revision = draft.latest_published_revision + 1;
        tx.execute("INSERT INTO workflow_published(id,revision,project,name,kind,graph_json,closure,checks_json) VALUES (?1,?2,?3,?4,?5,?6,?7,?8)",
            params![id, revision, draft.project, draft.name, kind_str(draft.kind), encode_graph(&draft.graph)?, draft.closure.as_str(), serde_json::to_string(&draft.required_checks).map_err(|e| format!("encode workflow checks: {e}"))?])
            .map_err(|e| format!("publish workflow revision: {e}"))?;
        tx.execute("UPDATE workflow_definitions SET latest_published_revision=?1,last_published_draft_revision=?2 WHERE id=?3",
            params![revision, draft.draft_revision, id])
            .map_err(|e| format!("advance workflow publication: {e}"))?;
        let published = read_published(&tx, id, revision)?;
        tx.commit()
            .map_err(|e| format!("commit workflow publication: {e}"))?;
        Ok(published)
    }

    pub fn get_published(&self, id: &str, revision: i64) -> Result<PublishedWorkflow, String> {
        read_published(&self.connect()?, id, revision)
    }

    /// Seed the pre-check-policy built-ins (story_delivery rev 1 with no checks).
    #[cfg(test)]
    pub(crate) fn seed_pre_policy_templates(&self, project: &str) -> Result<(), String> {
        let conn = self.connect()?;
        let story_id = Uuid::now_v7().to_string();
        let plan_id = Uuid::now_v7().to_string();
        insert_seed(
            &conn,
            &story_id,
            project,
            "Story delivery",
            WorkflowKind::Story,
            "story_delivery",
            frozen_graph(FROZEN_STORY_SEED),
            &[],
        )?;
        insert_seed(
            &conn,
            &plan_id,
            project,
            "Resolve plan",
            WorkflowKind::Plan,
            "resolve_plan",
            frozen_graph(&FROZEN_PLAN_SEED.replace("STORY_ID", &story_id)),
            &[],
        )
    }

    /// Seed the two built-in templates atomically and only once per project.
    pub fn seed_templates(&self, project: &str) -> Result<Vec<WorkflowDraft>, String> {
        validate_identity(project, "Resolve plan")?;
        let mut conn = self.connect()?;
        let tx = conn
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .map_err(|e| format!("begin template seed: {e}"))?;
        let existing: Option<String> = tx.query_row(
            "SELECT id FROM workflow_definitions WHERE project=?1 AND builtin_key='resolve_plan'", [project], |row| row.get(0)
        ).optional().map_err(|e| format!("find workflow templates: {e}"))?;
        if existing.is_none() {
            let story_id = Uuid::now_v7().to_string();
            let plan_id = Uuid::now_v7().to_string();
            insert_seed(
                &tx,
                &story_id,
                project,
                "Story delivery",
                WorkflowKind::Story,
                "story_delivery",
                story_template_graph(),
                &default_repository_checks(),
            )?;
            insert_seed(
                &tx,
                &plan_id,
                project,
                "Resolve plan",
                WorkflowKind::Plan,
                "resolve_plan",
                resolve_plan_graph(&story_id, 1),
                &default_repository_checks(),
            )?;
        } else {
            migrate_unchecked_story_seed(&tx, project)?;
        }
        let ids = {
            let mut stmt = tx.prepare("SELECT id FROM workflow_definitions WHERE project=?1 AND builtin_key IS NOT NULL ORDER BY builtin_key")
                .map_err(|e| format!("read template ids: {e}"))?;
            stmt.query_map([project], |row| row.get::<_, String>(0))
                .map_err(|e| format!("list templates: {e}"))?
                .collect::<Result<Vec<_>, _>>()
                .map_err(|e| format!("collect templates: {e}"))?
        };
        let templates = ids
            .iter()
            .map(|id| read_draft(&tx, id))
            .collect::<Result<Vec<_>, _>>()?;
        tx.commit()
            .map_err(|e| format!("commit template seed: {e}"))?;
        Ok(templates)
    }
}

/// Seeds created before the check policy shipped hold story_delivery rev 1 with
/// no checks, which makes every integration fail. When that definition (and the
/// resolve_plan seed pinning it) is byte-for-byte the original seed, publish
/// revision 2 with the default policy. Anything the user edited is left alone;
/// integration then reports the missing checks by name.
fn migrate_unchecked_story_seed(conn: &Connection, project: &str) -> Result<(), String> {
    let seed_id = |key: &str| -> Result<Option<String>, String> {
        conn.query_row(
            "SELECT id FROM workflow_definitions WHERE project=?1 AND builtin_key=?2",
            params![project, key],
            |row| row.get(0),
        )
        .optional()
        .map_err(|e| format!("find built-in workflow: {e}"))
    };
    let (Some(story_id), Some(plan_id)) = (seed_id("story_delivery")?, seed_id("resolve_plan")?)
    else {
        return Ok(());
    };
    let is_original = |id: &str, graph: &WorkflowGraph, checks: &[CheckDefinition]| {
        let draft = read_draft(conn, id)?;
        let published = read_published(conn, id, 1)?;
        Ok::<_, String>(
            draft.draft_revision == 1
                && draft.latest_published_revision == 1
                && draft.closure == WorkflowClosure::Human
                && draft.graph == *graph
                && draft.required_checks == checks
                && published.graph == *graph
                && published.closure == WorkflowClosure::Human
                && published.required_checks == checks,
        )
    };
    let legacy_graph = |mut graph: WorkflowGraph| {
        for node in &mut graph.nodes {
            if let NodeKind::Pause { resume_to } = &mut node.kind {
                *resume_to = None;
            }
        }
        graph
    };
    if !is_original(&story_id, &legacy_graph(story_template_graph()), &[])?
        || !is_original(
            &plan_id,
            &legacy_graph(resolve_plan_graph(&story_id, 1)),
            &[],
        )?
    {
        return Ok(());
    }
    republish_seed(
        conn,
        &story_id,
        &story_template_graph(),
        &default_repository_checks(),
    )?;
    republish_seed(conn, &plan_id, &resolve_plan_graph(&story_id, 2), &[])
}

fn republish_seed(
    conn: &Connection,
    id: &str,
    graph: &WorkflowGraph,
    checks: &[CheckDefinition],
) -> Result<(), String> {
    let raw = encode_graph(graph)?;
    let checks_raw =
        serde_json::to_string(checks).map_err(|e| format!("encode workflow checks: {e}"))?;
    conn.execute("UPDATE workflow_definitions SET graph_json=?1,checks_json=?2,draft_revision=2,latest_published_revision=2,last_published_draft_revision=2 WHERE id=?3",
        params![raw, checks_raw, id]).map_err(|e| format!("migrate built-in workflow: {e}"))?;
    conn.execute("INSERT INTO workflow_published(id,revision,project,name,kind,graph_json,closure,checks_json) SELECT id,2,project,name,kind,?1,closure,?2 FROM workflow_published WHERE id=?3 AND revision=1",
        params![raw, checks_raw, id]).map_err(|e| format!("publish migrated built-in workflow: {e}"))?;
    Ok(())
}

fn validate_identity(project: &str, name: &str) -> Result<(), String> {
    if !crate::fs::is_absolute_on_any_platform(project) || project.len() > 4096 {
        return Err("workflow project must be an absolute path".into());
    }
    if name.trim().is_empty() || name.len() > 200 {
        return Err("invalid workflow name".into());
    }
    Ok(())
}

fn kind_str(kind: WorkflowKind) -> &'static str {
    match kind {
        WorkflowKind::Plan => "plan",
        WorkflowKind::Story => "story",
    }
}
fn parse_kind(raw: &str) -> Result<WorkflowKind, String> {
    match raw {
        "plan" => Ok(WorkflowKind::Plan),
        "story" => Ok(WorkflowKind::Story),
        _ => Err("invalid workflow kind in store".into()),
    }
}
fn encode_graph(graph: &WorkflowGraph) -> Result<String, String> {
    serde_json::to_string(graph).map_err(|e| format!("encode workflow graph: {e}"))
}
fn decode_graph(raw: String) -> Result<WorkflowGraph, String> {
    serde_json::from_str(&raw).map_err(|e| format!("decode workflow graph: {e}"))
}

fn read_draft(conn: &Connection, id: &str) -> Result<WorkflowDraft, String> {
    let row = conn.query_row(
        "SELECT id,project,name,kind,graph_json,draft_revision,latest_published_revision,builtin_key,closure,checks_json,last_published_draft_revision FROM workflow_definitions WHERE id=?1",
        [id], |row| Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?, row.get::<_, String>(2)?, row.get::<_, String>(3)?,
            row.get::<_, String>(4)?, row.get::<_, i64>(5)?, row.get::<_, i64>(6)?, row.get::<_, Option<String>>(7)?, row.get::<_, String>(8)?, row.get::<_, String>(9)?, row.get::<_, i64>(10)?))
    ).optional().map_err(|e| format!("read workflow draft: {e}"))?.ok_or("workflow draft not found")?;
    Ok(WorkflowDraft {
        id: row.0,
        project: row.1,
        name: row.2,
        kind: parse_kind(&row.3)?,
        closure: WorkflowClosure::parse(&row.8)?,
        required_checks: serde_json::from_str(&row.9)
            .map_err(|e| format!("decode workflow checks: {e}"))?,
        graph: decode_graph(row.4)?,
        draft_revision: row.5,
        latest_published_revision: row.6,
        last_published_draft_revision: row.10,
        builtin_key: row.7,
    })
}

fn read_published(conn: &Connection, id: &str, revision: i64) -> Result<PublishedWorkflow, String> {
    let row = conn.query_row(
        "SELECT id,project,name,kind,graph_json,revision,closure,checks_json FROM workflow_published WHERE id=?1 AND revision=?2",
        params![id, revision], |row| Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?, row.get::<_, String>(2)?, row.get::<_, String>(3)?,
            row.get::<_, String>(4)?, row.get::<_, i64>(5)?, row.get::<_, String>(6)?, row.get::<_, String>(7)?))
    ).optional().map_err(|e| format!("read published workflow: {e}"))?.ok_or("published workflow revision not found")?;
    Ok(PublishedWorkflow {
        id: row.0,
        project: row.1,
        name: row.2,
        kind: parse_kind(&row.3)?,
        closure: WorkflowClosure::parse(&row.6)?,
        required_checks: serde_json::from_str(&row.7)
            .map_err(|e| format!("decode workflow checks: {e}"))?,
        graph: decode_graph(row.4)?,
        revision: row.5,
    })
}

fn validate_pinned_templates(conn: &Connection, draft: &WorkflowDraft) -> Result<(), String> {
    for node in &draft.graph.nodes {
        if let NodeKind::StoryDispatch {
            story_template_id,
            story_revision,
        } = &node.kind
        {
            let target = read_published(conn, story_template_id, *story_revision)?;
            if target.project != draft.project || target.kind != WorkflowKind::Story {
                return Err(
                    "Story Dispatch must pin a published story template in the same project".into(),
                );
            }
        }
    }
    Ok(())
}

/// Project-independent default policy for seeded story and plan workflows: the repository
/// object graph must be intact at the checked commit. Projects pin their own
/// build/test checks in a new revision.
fn default_repository_checks() -> Vec<CheckDefinition> {
    vec![CheckDefinition {
        id: "repository-integrity".into(),
        argv: ["git", "fsck", "--connectivity-only", "--no-progress"]
            .map(String::from)
            .into(),
        timeout_secs: 300,
    }]
}

// Keep the independently supplied boundary fields explicit; grouping changes this contract.
#[expect(
    clippy::too_many_arguments,
    reason = "seed insertion mirrors the persisted workflow fields"
)]
fn insert_seed(
    conn: &Connection,
    id: &str,
    project: &str,
    name: &str,
    kind: WorkflowKind,
    key: &str,
    graph: WorkflowGraph,
    checks: &[CheckDefinition],
) -> Result<(), String> {
    validate_graph(&graph, kind)?;
    validate_checks(checks)?;
    let raw = encode_graph(&graph)?;
    let checks_raw =
        serde_json::to_string(checks).map_err(|e| format!("encode workflow checks: {e}"))?;
    conn.execute("INSERT INTO workflow_definitions(id,project,name,kind,graph_json,draft_revision,latest_published_revision,last_published_draft_revision,builtin_key,checks_json)
                  VALUES (?1,?2,?3,?4,?5,1,1,1,?6,?7)", params![id, project, name, kind_str(kind), raw, key, checks_raw])
        .map_err(|e| format!("insert built-in workflow: {e}"))?;
    conn.execute("INSERT INTO workflow_published(id,revision,project,name,kind,graph_json,checks_json) VALUES (?1,1,?2,?3,?4,?5,?6)",
        params![id, project, name, kind_str(kind), raw, checks_raw]).map_err(|e| format!("publish built-in workflow: {e}"))?;
    Ok(())
}

fn edge(from: &str, to: &str, outcome: Option<&str>) -> Edge {
    Edge {
        from: from.into(),
        to: to.into(),
        outcome: outcome.map(str::to_owned),
    }
}
fn node(id: &str, kind: NodeKind) -> Node {
    Node {
        id: id.into(),
        kind,
    }
}
fn agent(role: AgentRole, prompt_template: &str, capabilities: &[&str]) -> NodeKind {
    NodeKind::Agent {
        role,
        prompt_template: prompt_template.into(),
        capabilities: capabilities.iter().map(|s| (*s).into()).collect(),
    }
}
fn story_template_graph() -> WorkflowGraph {
    WorkflowGraph {
        nodes: vec![
            node("start", NodeKind::Start),
            node(
                "implement",
                agent(
                    AgentRole::Implementer,
                    "Implement {{story.title}} and report evidence for {{story.id}}.",
                    &["story_read", "story_report"],
                ),
            ),
            node(
                "review",
                agent(
                    AgentRole::Reviewer,
                    "Review {{story.id}} against its criteria and report findings.",
                    &["story_read", "story_report"],
                ),
            ),
            node("judge", NodeKind::Judge),
            node("repair", NodeKind::Loop { max_iterations: 3 }),
            node(
                "pause",
                NodeKind::Pause {
                    resume_to: Some("implement".into()),
                },
            ),
            node("end", NodeKind::End),
        ],
        edges: vec![
            edge("start", "implement", None),
            edge("implement", "review", None),
            edge("review", "judge", None),
            edge("judge", "end", Some("yes")),
            edge("judge", "repair", Some("no")),
            edge("judge", "pause", Some("uncertain")),
            edge("repair", "implement", Some("repeat")),
            edge("repair", "pause", Some("exhausted")),
        ],
    }
}
fn resolve_plan_graph(story_template_id: &str, story_revision: i64) -> WorkflowGraph {
    WorkflowGraph {
        nodes: vec![
            node("start", NodeKind::Start),
            node(
                "coordinate",
                agent(
                    AgentRole::Coordinator,
                    "Resolve plan {{plan.id}} from the current snapshot.",
                    &["story_read", "story_create", "agent_spawn"],
                ),
            ),
            node("create", NodeKind::CreateStories),
            node(
                "dispatch",
                NodeKind::StoryDispatch {
                    story_template_id: story_template_id.into(),
                    story_revision,
                },
            ),
            node("judge", NodeKind::Judge),
            node("replan", NodeKind::Loop { max_iterations: 8 }),
            node(
                "pause",
                NodeKind::Pause {
                    resume_to: Some("create".into()),
                },
            ),
            node("end", NodeKind::End),
        ],
        edges: vec![
            edge("start", "coordinate", None),
            edge("coordinate", "create", None),
            edge("create", "dispatch", None),
            edge("dispatch", "judge", Some("completed")),
            edge("dispatch", "pause", Some("blocked")),
            edge("judge", "end", Some("yes")),
            edge("judge", "replan", Some("no")),
            edge("judge", "pause", Some("uncertain")),
            edge("replan", "create", Some("repeat")),
            edge("replan", "pause", Some("exhausted")),
        ],
    }
}

/// The rev-1 seed graphs as shipped before the check policy, frozen as literals
/// so a later edit of the templates cannot silently make real legacy seeds
/// non-migratable while the migration test stays green.
#[cfg(test)]
const FROZEN_STORY_SEED: &str = r#"{"nodes":[
{"id":"start","kind":{"type":"start"}},
{"id":"implement","kind":{"type":"agent","role":"implementer","capabilities":["story_read","story_report"],"prompt_template":"Implement {{story.title}} and report evidence for {{story.id}}."}},
{"id":"review","kind":{"type":"agent","role":"reviewer","capabilities":["story_read","story_report"],"prompt_template":"Review {{story.id}} against its criteria and report findings."}},
{"id":"judge","kind":{"type":"judge"}},
{"id":"repair","kind":{"type":"loop","max_iterations":3}},
{"id":"pause","kind":{"type":"pause"}},
{"id":"end","kind":{"type":"end"}}],
"edges":[
{"from":"start","to":"implement","outcome":null},
{"from":"implement","to":"review","outcome":null},
{"from":"review","to":"judge","outcome":null},
{"from":"judge","to":"end","outcome":"yes"},
{"from":"judge","to":"repair","outcome":"no"},
{"from":"judge","to":"pause","outcome":"uncertain"},
{"from":"repair","to":"implement","outcome":"repeat"},
{"from":"repair","to":"pause","outcome":"exhausted"}]}"#;

#[cfg(test)]
const FROZEN_PLAN_SEED: &str = r#"{"nodes":[
{"id":"start","kind":{"type":"start"}},
{"id":"coordinate","kind":{"type":"agent","role":"coordinator","capabilities":["story_read","story_create","agent_spawn"],"prompt_template":"Resolve plan {{plan.id}} from the current snapshot."}},
{"id":"create","kind":{"type":"create_stories"}},
{"id":"dispatch","kind":{"type":"story_dispatch","story_template_id":"STORY_ID","story_revision":1}},
{"id":"judge","kind":{"type":"judge"}},
{"id":"replan","kind":{"type":"loop","max_iterations":8}},
{"id":"pause","kind":{"type":"pause"}},
{"id":"end","kind":{"type":"end"}}],
"edges":[
{"from":"start","to":"coordinate","outcome":null},
{"from":"coordinate","to":"create","outcome":null},
{"from":"create","to":"dispatch","outcome":null},
{"from":"dispatch","to":"judge","outcome":"completed"},
{"from":"dispatch","to":"pause","outcome":"blocked"},
{"from":"judge","to":"end","outcome":"yes"},
{"from":"judge","to":"replan","outcome":"no"},
{"from":"judge","to":"pause","outcome":"uncertain"},
{"from":"replan","to":"create","outcome":"repeat"},
{"from":"replan","to":"pause","outcome":"exhausted"}]}"#;

#[cfg(test)]
fn frozen_graph(raw: &str) -> WorkflowGraph {
    serde_json::from_str(raw).expect("frozen seed graph")
}
