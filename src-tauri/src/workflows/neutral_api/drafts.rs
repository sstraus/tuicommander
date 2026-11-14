use super::model::{EditorMetadata, NeutralDraft, PublicationView};
use crate::workflows::{WorkflowStore, neutral_publication::NeutralDefinition};
use rusqlite::{OptionalExtension, params};

pub(crate) const SCHEMA: &str = "CREATE TABLE IF NOT EXISTS neutral_workflow_drafts (
    project TEXT NOT NULL, id TEXT NOT NULL, name TEXT NOT NULL,
    revision INTEGER NOT NULL CHECK(revision > 0), definition_json TEXT NOT NULL, editor_json TEXT NOT NULL,
    PRIMARY KEY(project,id)
);
CREATE TABLE IF NOT EXISTS neutral_workflow_presentation (
    project TEXT NOT NULL, id TEXT NOT NULL, revision INTEGER NOT NULL,
    draft_revision INTEGER NOT NULL, editor_json TEXT NOT NULL,
    PRIMARY KEY(project,id,revision)
);";

pub(crate) fn check_draft(
    name: &str,
    definition: &NeutralDefinition,
    editor: &EditorMetadata,
) -> Result<(), String> {
    if name.trim().is_empty() || name.len() > 200 {
        return Err("neutral draft requires a bounded nonempty name".into());
    }
    if definition.instances.len() > 128
        || definition.control.nodes.len() > 256
        || serde_json::to_vec(definition)
            .map_err(|e| e.to_string())?
            .len()
            > 256_000
    {
        return Err("neutral draft is too large".into());
    }
    for (id, value) in editor.labels.iter().chain(&editor.presets) {
        if !definition.control.nodes.contains_key(id) || value.len() > 4096 {
            return Err(
                "editor metadata requires an existing control node and bounded text".into(),
            );
        }
    }
    Ok(())
}

impl WorkflowStore {
    pub(crate) fn create_neutral_draft(
        &self,
        project: &str,
        name: String,
        definition: NeutralDefinition,
        editor: EditorMetadata,
    ) -> Result<NeutralDraft, String> {
        check_draft(&name, &definition, &editor)?;
        let id = uuid::Uuid::new_v4().to_string();
        let document = serde_json::to_string(&definition).map_err(|e| e.to_string())?;
        let editor_document = serde_json::to_string(&editor).map_err(|e| e.to_string())?;
        self.connect()?.execute("INSERT INTO neutral_workflow_drafts(project,id,name,revision,definition_json,editor_json) VALUES (?1,?2,?3,1,?4,?5)", params![project,id,name,document,editor_document]).map_err(|e| format!("create neutral draft: {e}"))?;
        Ok(NeutralDraft {
            id,
            project: project.into(),
            name,
            revision: 1,
            latest_published_revision: 0,
            definition,
            editor,
        })
    }

    pub(crate) fn get_neutral_draft(
        &self,
        project: &str,
        id: &str,
    ) -> Result<NeutralDraft, String> {
        let row: Option<(String,i64,String,i64,String)> = self.connect()?.query_row(
            "SELECT name,revision,definition_json,(SELECT COALESCE(MAX(revision),0) FROM neutral_workflow_publications WHERE project=?1 AND id=?2) ,editor_json FROM neutral_workflow_drafts WHERE project=?1 AND id=?2",
            params![project,id], |r| Ok((r.get(0)?,r.get(1)?,r.get(2)?,r.get(3)?,r.get(4)?))
        ).optional().map_err(|e| format!("read neutral draft: {e}"))?;
        let (name, revision, document, latest_published_revision, editor_document) =
            row.ok_or("neutral draft not found in project")?;
        let definition =
            serde_json::from_str(&document).map_err(|e| format!("decode neutral draft: {e}"))?;
        Ok(NeutralDraft {
            id: id.into(),
            project: project.into(),
            name,
            revision,
            latest_published_revision,
            definition,
            editor: serde_json::from_str(&editor_document).map_err(|e| e.to_string())?,
        })
    }

    pub(crate) fn list_neutral_drafts(&self, project: &str) -> Result<Vec<NeutralDraft>, String> {
        let conn = self.connect()?;
        let mut query = conn
            .prepare("SELECT id FROM neutral_workflow_drafts WHERE project=?1 ORDER BY name,id")
            .map_err(|e| e.to_string())?;
        let ids = query
            .query_map([project], |r| r.get::<_, String>(0))
            .map_err(|e| e.to_string())?
            .collect::<Result<Vec<_>, _>>()
            .map_err(|e| e.to_string())?;
        ids.iter()
            .map(|id| self.get_neutral_draft(project, id))
            .collect()
    }

    pub(crate) fn update_neutral_draft(
        &self,
        project: &str,
        id: &str,
        expected_revision: i64,
        name: String,
        definition: NeutralDefinition,
        editor: EditorMetadata,
    ) -> Result<NeutralDraft, String> {
        check_draft(&name, &definition, &editor)?;
        let revision = expected_revision
            .checked_add(1)
            .filter(|r| *r > 1)
            .ok_or("invalid neutral draft revision")?;
        let document = serde_json::to_string(&definition).map_err(|e| e.to_string())?;
        let editor_document = serde_json::to_string(&editor).map_err(|e| e.to_string())?;
        let count = self.connect()?.execute("UPDATE neutral_workflow_drafts SET name=?1,revision=?2,definition_json=?3,editor_json=?7 WHERE project=?4 AND id=?5 AND revision=?6", params![name,revision,document,project,id,expected_revision,editor_document]).map_err(|e| format!("update neutral draft: {e}"))?;
        if count != 1 {
            return Err("neutral draft revision changed or project does not own draft".into());
        }
        let latest_published_revision = self
            .get_neutral_draft(project, id)?
            .latest_published_revision;
        Ok(NeutralDraft {
            id: id.into(),
            project: project.into(),
            name,
            revision,
            latest_published_revision,
            definition,
            editor,
        })
    }
}

impl WorkflowStore {
    pub(crate) fn neutral_publication_view(
        &self,
        publication: crate::workflows::neutral_publication::NeutralPublication,
    ) -> Result<PublicationView, String> {
        let document:Option<String>=self.connect()?.query_row("SELECT editor_json FROM neutral_workflow_presentation WHERE project=?1 AND id=?2 AND revision=?3",params![publication.project,publication.id,publication.revision],|r|r.get(0)).optional().map_err(|e|e.to_string())?;
        let editor = document
            .map(|s| serde_json::from_str(&s))
            .transpose()
            .map_err(|e| format!("decode workflow presentation: {e}"))?
            .unwrap_or_default();
        Ok(PublicationView {
            publication,
            editor,
        })
    }
}
