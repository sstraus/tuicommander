//! Fixed native Git network actions. Credentials exist only in child environment.
use base64::Engine;
use std::path::Path;
use tuic_git::git_cli::{GitCmd, git_cmd};

pub(super) enum Transfer<'a> {
    #[cfg(test)]
    Fetch(&'a str),
    Inspect(&'a str),
    Create {
        branch: &'a str,
        commit: &'a str,
    },
    Delete {
        branch: &'a str,
        commit: &'a str,
    },
}

pub(super) fn transfer(
    path: &Path,
    remote: &str,
    token: &str,
    action: Transfer<'_>,
) -> Result<String, String> {
    // A literal destination must not be rewritten by local or inherited Git config.
    let rewrites = git_cmd(path)
        .args([
            "config",
            "--get-regexp",
            r"^url\..*\.(insteadof|pushinsteadof)$",
        ])
        .run_raw()
        .map_err(|_| "Git configuration could not be checked")?;
    if !rewrites.status.success() && rewrites.status.code() != Some(1) {
        return Err("Git configuration could not be checked".into());
    }
    // An exact identity rewrite wins over host-wide user rewrites. Equal-length
    // competing entries have order-dependent precedence, so refuse those.
    let configured =
        String::from_utf8(rewrites.stdout).map_err(|_| "Git configuration is not UTF-8")?;
    for line in configured.lines() {
        if let Some((key, value)) = line.split_once(char::is_whitespace)
            && value.trim() == remote
            && key != format!("url.{remote}.insteadof")
            && key != format!("url.{remote}.pushinsteadof")
        {
            return Err("Competing exact Git URL rewrite refuses native publication".into());
        }
    }
    let args = match action {
        #[cfg(test)]
        Transfer::Fetch(branch) => vec![
            "fetch".into(),
            "--no-tags".into(),
            "--depth=1".into(),
            "--".into(),
            remote.into(),
            format!("refs/heads/{branch}"),
        ],
        Transfer::Inspect(branch) => vec![
            "ls-remote".into(),
            "--heads".into(),
            "--".into(),
            remote.into(),
            format!("refs/heads/{branch}"),
        ],
        Transfer::Create { branch, commit } => vec![
            "push".into(),
            "--porcelain".into(),
            format!("--force-with-lease=refs/heads/{branch}:"),
            "--".into(),
            remote.into(),
            format!("{commit}:refs/heads/{branch}"),
        ],
        Transfer::Delete { branch, commit } => vec![
            "push".into(),
            "--porcelain".into(),
            format!("--force-with-lease=refs/heads/{branch}:{commit}"),
            "--".into(),
            remote.into(),
            format!(":refs/heads/{branch}"),
        ],
    };
    let result = publication_git(path, remote, token)
        .args(&args)
        .run_raw()
        .map_err(|_| "Native Git publication failed or timed out")?;
    // Never retain raw network stderr: authentication diagnostics can contain secrets.
    if !result.status.success() {
        return Err("Native Git publication refused or failed".into());
    }
    String::from_utf8(result.stdout).map_err(|_| "Native Git response is not UTF-8".into())
}

pub(super) fn absent(path: &Path, remote: &str, token: &str, branch: &str) -> Result<bool, String> {
    Ok(transfer(path, remote, token, Transfer::Inspect(branch))?
        .trim()
        .is_empty())
}

// Command-scoped only: no saved Git configuration is changed.
pub(super) fn publication_git(path: &Path, remote: &str, token: &str) -> GitCmd {
    let basic = base64::engine::general_purpose::STANDARD.encode(format!("x-access-token:{token}"));
    git_cmd(path)
        .env("GIT_TERMINAL_PROMPT", "0")
        .env("GIT_TRACE", "0")
        .env("GIT_TRACE_CURL", "0")
        .env("GIT_CURL_VERBOSE", "0")
        .env("GIT_CONFIG_KEY_0", &format!("http.{remote}.extraHeader"))
        .env(
            "GIT_CONFIG_VALUE_0",
            &format!("Authorization: Basic {basic}"),
        )
        .env("GIT_CONFIG_KEY_1", "credential.helper")
        .env("GIT_CONFIG_VALUE_1", "")
        .env("GIT_CONFIG_KEY_2", "http.followRedirects")
        .env("GIT_CONFIG_VALUE_2", "false")
        .env("GIT_CONFIG_KEY_3", "protocol.allow")
        .env("GIT_CONFIG_VALUE_3", "never")
        .env("GIT_CONFIG_KEY_4", "protocol.https.allow")
        .env("GIT_CONFIG_VALUE_4", "always")
        .env("GIT_CONFIG_KEY_5", &format!("url.{remote}.insteadOf"))
        .env("GIT_CONFIG_VALUE_5", remote)
        .env("GIT_CONFIG_KEY_6", &format!("url.{remote}.pushInsteadOf"))
        .env("GIT_CONFIG_VALUE_6", remote)
        .env("GIT_CONFIG_COUNT", "7")
        .timeout(std::time::Duration::from_secs(120))
}
