use super::{PLAN_DOCUMENT_NOT_FOUND, PlanSource};
use std::path::{Path, PathBuf};

const PLAN_DIRS: [&str; 2] = ["plans", ".claude/plans"];

pub(super) fn list_plan_sources(project: &Path) -> Result<Vec<PlanSource>, String> {
    let mut sources = Vec::new();
    for directory in PLAN_DIRS {
        let root = project.join(directory);
        let entries = match std::fs::read_dir(&root) {
            Ok(entries) => entries,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => continue,
            Err(error) => return Err(format!("read plan directory {}: {error}", root.display())),
        };
        for entry in entries {
            let entry = entry.map_err(|error| format!("read plan directory entry: {error}"))?;
            let path = entry.path();
            if !path.is_file() || path.extension().and_then(|ext| ext.to_str()) != Some("md") {
                continue;
            }
            let Some(name) = path.file_name().and_then(|name| name.to_str()) else {
                continue;
            };
            let source = format!("{directory}/{name}");
            if let Ok(title) = title_for_source(project, &source) {
                sources.push(PlanSource { title, source });
            }
        }
    }
    sources.sort_by(|a, b| a.title.cmp(&b.title).then(a.source.cmp(&b.source)));
    Ok(sources)
}

pub(super) fn title_for_source(project: &Path, source: &str) -> Result<String, String> {
    let path = PathBuf::from(source);
    let path = if path.is_absolute() {
        path
    } else {
        project.join(path)
    };
    let document = std::fs::read_to_string(&path).map_err(|error| {
        if error.kind() == std::io::ErrorKind::NotFound {
            format!("{PLAN_DOCUMENT_NOT_FOUND}{}", path.display())
        } else {
            format!("read plan document {}: {error}", path.display())
        }
    })?;
    title_from_document(&document).ok_or_else(|| {
        format!(
            "plan document has no front-matter title or heading: {}",
            path.display()
        )
    })
}

fn title_from_document(document: &str) -> Option<String> {
    let mut lines = document.trim_start_matches('\u{feff}').lines();
    let first = lines.next()?;
    if first.trim() == "---" {
        for line in lines.by_ref() {
            if line.trim() == "---" {
                break;
            }
            if let Some(title) = line.trim().strip_prefix("title:") {
                let title = title.trim().trim_matches(['"', '\'']).trim();
                if !title.is_empty() {
                    return Some(title.to_string());
                }
            }
        }
    } else if let Some(title) = heading(first) {
        return Some(title.to_string());
    }
    lines.find_map(heading).map(str::to_string)
}

fn heading(line: &str) -> Option<&str> {
    let rest = line.trim_start().trim_start_matches('#');
    if rest.len() == line.trim_start().len() || !rest.starts_with(char::is_whitespace) {
        return None;
    }
    let title = rest.trim();
    (!title.is_empty()).then_some(title)
}
