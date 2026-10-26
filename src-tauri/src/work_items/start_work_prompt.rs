//! Repository-owned Tasks briefs. All substitutions happen before provider actions.
use serde::Deserialize;

const TEMPLATE_FILE: &str = ".tuic-start-work.md";

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct PromptInput {
    pub key: String,
    pub url: Option<String>,
    pub title: String,
    pub body: String,
}

// Preserve TAB and LF for readable Markdown; remove CR, ESC and the other C0/C1 controls.
fn item_text(text: &str) -> String {
    text.chars()
        .filter(|c| !c.is_control() || matches!(c, '\t' | '\n'))
        .collect()
}

fn build(repo_path: String, input: PromptInput) -> Result<String, String> {
    let key = item_text(&input.key);
    let title = item_text(&input.title);
    let url = item_text(input.url.as_deref().unwrap_or_default());
    let body = item_text(&input.body);
    let path = std::path::Path::new(&repo_path).join(TEMPLATE_FILE);
    match std::fs::symlink_metadata(&path) {
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            return Ok(format!("{key}: {title}\n\n{body}").trim_end().to_owned());
        }
        Err(error) => return Err(format!("Cannot inspect start-work template: {error}")),
        Ok(_) => {}
    }
    // Reuse the repo reader's canonical containment, UTF-8 and size guards.
    let template = crate::read_file_impl(repo_path, TEMPLATE_FILE.to_owned())?;
    // Scan the template once: item text containing another placeholder remains literal.
    let mut result = String::new();
    let mut rest = template.as_str();
    while let Some(index) = rest.find("{{") {
        result.push_str(&rest[..index]);
        rest = &rest[index..];
        let Some(end) = rest.find("}}") else { break };
        let token = &rest[..end + 2];
        result.push_str(match token {
            "{{key}}" => &key,
            "{{url}}" => &url,
            "{{title}}" => &title,
            _ => token,
        });
        rest = &rest[end + 2..];
    }
    result.push_str(rest);
    Ok(result)
}

#[cfg_attr(feature = "desktop", tauri::command)]
pub(crate) async fn build_start_work_prompt(
    repo_path: String,
    input: PromptInput,
) -> Result<String, String> {
    crate::fs::spawn_blocking_fs(move || build(repo_path, input)).await
}

#[cfg(test)]
mod tests {
    use super::*;

    fn input() -> PromptInput {
        PromptInput {
            key: "TASK-1".into(),
            title: "Safe\u{1b}[201~\rtitle {{url}}".into(),
            url: Some("https://example.test/1\u{85}".into()),
            body: "Body\u{0}\ntext".into(),
        }
    }

    // Catches: raw controls reaching the provider and replacement text being expanded twice.
    #[tokio::test]
    async fn start_work_prompt_template_sanitizes_fields_and_substitutes_once() {
        let dir = tempfile::tempdir_in(crate::test_support::test_temp_root()).unwrap();
        std::fs::write(
            dir.path().join(TEMPLATE_FILE),
            "{{key}} | {{title}} | {{url}} | {{title}} | {{other}}",
        )
        .unwrap();
        assert_eq!(
            build_start_work_prompt(dir.path().to_string_lossy().into_owned(), input())
                .await
                .unwrap(),
            "TASK-1 | Safe[201~title {{url}} | https://example.test/1 | Safe[201~title {{url}} | {{other}}"
        );
    }

    // Catches: a missing template dropping the body or changing the legacy brief layout.
    #[tokio::test]
    async fn start_work_prompt_missing_template_preserves_default_and_strips_controls() {
        let dir = tempfile::tempdir_in(crate::test_support::test_temp_root()).unwrap();
        assert_eq!(
            build_start_work_prompt(dir.path().to_string_lossy().into_owned(), input())
                .await
                .unwrap(),
            "TASK-1: Safe[201~title {{url}}\n\nBody\ntext"
        );
    }

    // Catches: absent URLs becoming null text and incomplete/unknown placeholders disappearing.
    #[tokio::test]
    async fn start_work_prompt_null_url_and_literal_tokens_survive() {
        let dir = tempfile::tempdir_in(crate::test_support::test_temp_root()).unwrap();
        std::fs::write(dir.path().join(TEMPLATE_FILE), "{{url}} {{unknown}} {{").unwrap();
        let mut value = input();
        value.url = None;
        assert_eq!(
            build_start_work_prompt(dir.path().to_string_lossy().into_owned(), value)
                .await
                .unwrap(),
            " {{unknown}} {{"
        );
    }

    // Catches: unreadable templates silently falling back and hiding repository mistakes.
    #[tokio::test]
    async fn start_work_prompt_invalid_utf8_is_an_error() {
        let dir = tempfile::tempdir_in(crate::test_support::test_temp_root()).unwrap();
        std::fs::write(dir.path().join(TEMPLATE_FILE), [0xff]).unwrap();
        assert!(
            build_start_work_prompt(dir.path().to_string_lossy().into_owned(), input())
                .await
                .is_err()
        );
    }

    // Catches: a repo template symlink reading files outside the repository.
    #[cfg(unix)]
    #[tokio::test]
    async fn start_work_prompt_rejects_template_symlink_outside_repo() {
        let dir = tempfile::tempdir_in(crate::test_support::test_temp_root()).unwrap();
        let outside = tempfile::tempdir_in(crate::test_support::test_temp_root()).unwrap();
        let file = outside.path().join("brief.md");
        let outside_content = "OUTSIDE_TEMPLATE_CONTENT_MUST_NOT_BE_RETURNED";
        std::fs::write(&file, outside_content).unwrap();
        std::os::unix::fs::symlink(&file, dir.path().join(TEMPLATE_FILE)).unwrap();
        let error = build_start_work_prompt(dir.path().to_string_lossy().into_owned(), input())
            .await
            .unwrap_err();
        assert_eq!(
            error,
            format!(
                "Cannot open symbolic link \"{TEMPLATE_FILE}\": target \"{}\" is outside the repository.",
                file.canonicalize().unwrap().display()
            )
        );
        assert!(!error.contains(outside_content));
    }
}
