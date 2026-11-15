use super::NATIVE_CAPABILITIES;
use std::path::{Path, PathBuf};

fn production_text(path: &Path) -> String {
    let text = std::fs::read_to_string(path).unwrap();
    // Inline test modules sit at the end of a file; tests may open the native store directly.
    let lines: Vec<&str> = text.lines().collect();
    let end = lines
        .windows(2)
        .position(|pair| {
            pair[0].trim() == "#[cfg(test)]"
                && pair[1].starts_with("mod ")
                && pair[1].trim_end().ends_with('{')
        })
        .unwrap_or(lines.len());
    lines[..end].join("\n")
}

fn production_files(dir: &Path, out: &mut Vec<PathBuf>) {
    for entry in std::fs::read_dir(dir).unwrap() {
        let path = entry.unwrap().path();
        let name = path.file_name().unwrap().to_string_lossy().to_string();
        if name.contains("tests") {
            continue;
        }
        if path.is_dir() {
            production_files(&path, out);
        } else if name.ends_with(".rs") {
            out.push(path);
        }
    }
}

/// catches: a new caller in the story API or the workflow run code that opens the SQLite store
/// directly, so a repository bound to another backend silently reads and writes native data.
#[test]
fn story_api_and_workflow_run_code_reach_stories_only_through_the_port() {
    let src = Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
    let mut files = vec![src.join("stories/api.rs")];
    production_files(&src.join("workflows/run"), &mut files);
    assert!(
        files.len() > 20,
        "scan found too few files: {}",
        files.len()
    );
    let offenders: Vec<_> = files
        .iter()
        .filter(|path| production_text(path).contains("StoryStore::open("))
        .map(|path| path.strip_prefix(&src).unwrap().display().to_string())
        .collect();
    assert!(
        offenders.is_empty(),
        "use stories::story_backend() or stories::execution_ledger(): {offenders:?}"
    );
}

/// catches: the probe drifting from the shape the Stories dialog reads, which shows the
/// "restart the backend" message on a working backend.
#[test]
fn native_capabilities_serialize_as_the_descriptor_the_dialog_reads() {
    let json = serde_json::to_value(NATIVE_CAPABILITIES).unwrap();
    assert_eq!(json["backend"], "native");
    let operations = json["operations"].as_object().unwrap();
    for key in [
        "create_plan",
        "update_plan",
        "archive_plan",
        "create_story",
        "update_story",
        "archive_story",
        "dependencies",
        "notes",
        "transitions",
    ] {
        assert_eq!(operations[key], true, "native operation changed: {key}");
    }
    assert_eq!(
        operations["story_edits"],
        serde_json::json!({
            "title":true,"priority":true,"problem":true,"criteria_add":true,"single_change":false
        })
    );
    // Catches: an additive capability making an old descriptor invalid or
    // granting individual flags merely because the object/flag was absent.
    let mut old = json["operations"].clone();
    old.as_object_mut().unwrap().remove("story_edits");
    let old: super::StoryOperations = serde_json::from_value(old).unwrap();
    assert!(old.update_story);
    assert!(!old.story_edits.title && !old.story_edits.single_change);
    let partial: super::StoryEdits =
        serde_json::from_value(serde_json::json!({"title":true})).unwrap();
    assert!(partial.title);
    assert!(!partial.priority && !partial.problem && !partial.criteria_add);
}
