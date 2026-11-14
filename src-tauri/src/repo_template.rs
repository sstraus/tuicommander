//! Repository-owned Markdown templates (`.tuic-start-work.md`, `.tuic-child-contract.md`).

/// The template text, or `None` when the repository has no such file.
pub(crate) fn read(repo_path: &str, file: &str) -> Result<Option<String>, String> {
    let path = std::path::Path::new(repo_path).join(file);
    match std::fs::symlink_metadata(&path) {
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(error) => return Err(format!("Cannot inspect template \"{file}\": {error}")),
        Ok(_) => {}
    }
    // Repository content picks this file and its text goes to an agent without
    // an owner click, so unlike files the owner opens it must not follow a link
    // out of the repository (a link to a credential file would leave the machine).
    let canonical_repo = std::path::Path::new(repo_path)
        .canonicalize()
        .map_err(|e| format!("Failed to resolve repo path: {e}"))?;
    let canonical = path
        .canonicalize()
        .map_err(|e| format!("Failed to resolve file path: {e}"))?;
    if !canonical.starts_with(&canonical_repo) {
        return Err(format!(
            "Cannot open symbolic link \"{file}\": target \"{}\" is outside the repository.",
            canonical.display()
        ));
    }
    // Reuse the repo reader's UTF-8 and size guards.
    crate::read_file_impl(repo_path.to_owned(), file.to_owned()).map(Some)
}

/// Replace each `{{token}}` once. Unknown tokens stay literal, and substituted
/// text is never scanned again, so a value holding a placeholder stays as written.
pub(crate) fn substitute(template: &str, value: impl Fn(&str) -> Option<String>) -> String {
    let mut result = String::new();
    let mut rest = template;
    while let Some(index) = rest.find("{{") {
        result.push_str(&rest[..index]);
        rest = &rest[index..];
        let Some(end) = rest.find("}}") else { break };
        let token = &rest[..end + 2];
        match value(&token[2..end]) {
            Some(text) => result.push_str(&text),
            None => result.push_str(token),
        }
        rest = &rest[end + 2..];
    }
    result.push_str(rest);
    result
}
