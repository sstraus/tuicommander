//! Pre-spawn attribution evidence. This is an observation, not permission to
//! discard a checkout: unsaved buffers and concurrent writers are not observable
//! through Git, and close/removal must independently prove current ownership.
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::io::Read;
use std::path::Path;
use std::process::{Command, Stdio};

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
pub(super) struct WorkspaceBaseline {
    /// Canonical common Git directory, shared by linked checkouts and aliases.
    pub repository_key: String,
    /// Canonical per-checkout Git directory; independent of its branch name.
    pub workspace_key: String,
    pub head: String,
    /// Exact porcelain-v1 -z bytes, hex encoded to preserve unusual path bytes.
    pub status_hex: String,
    pub staged_sha256: String,
    pub unstaged_sha256: String,
    pub untracked: Vec<UntrackedBaseline>,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
pub(super) struct UntrackedBaseline {
    pub path: String,
    pub kind: String,
    pub sha256: String,
}

fn git(path: &Path, args: &[&str]) -> Result<Vec<u8>, String> {
    let output = Command::new("git")
        .current_dir(path)
        .args(args)
        .env("GIT_OPTIONAL_LOCKS", "0")
        .output()
        .map_err(|error| format!("read launch baseline: {error}"))?;
    if !output.status.success() {
        return Err(format!(
            "read launch baseline: git {} failed: {}",
            args.join(" "),
            String::from_utf8_lossy(&output.stderr).trim()
        ));
    }
    Ok(output.stdout)
}

fn git_path(path: &Path, arg: &str) -> Result<String, String> {
    let bytes = git(path, &["rev-parse", "--path-format=absolute", arg])?;
    let text = std::str::from_utf8(&bytes).map_err(|error| error.to_string())?;
    let canonical = Path::new(text.trim_end_matches(['\r', '\n']))
        .canonicalize()
        .map_err(|error| format!("resolve launch baseline identity: {error}"))?;
    canonical
        .to_str()
        .map(str::to_owned)
        .ok_or_else(|| "launch baseline identity is not UTF-8".into())
}

fn hash_reader(mut reader: impl Read) -> Result<String, String> {
    let mut hash = Sha256::new();
    let mut buffer = [0_u8; 65_536];
    loop {
        let count = reader
            .read(&mut buffer)
            .map_err(|error| format!("hash launch baseline: {error}"))?;
        if count == 0 {
            break;
        }
        hash.update(&buffer[..count]);
    }
    Ok(hex::encode(hash.finalize()))
}

fn diff_hash(path: &Path, staged: bool) -> Result<String, String> {
    let mut command = Command::new("git");
    command
        .current_dir(path)
        .args(["diff", "--binary", "--no-ext-diff", "--no-textconv"]);
    if staged {
        command.arg("--cached");
    }
    let mut child = command
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .map_err(|error| format!("read launch baseline diff: {error}"))?;
    let digest = hash_reader(child.stdout.take().ok_or("missing baseline diff pipe")?);
    let status = child
        .wait()
        .map_err(|error| format!("wait for launch baseline diff: {error}"))?;
    if !status.success() {
        return Err("git diff failed while capturing launch baseline".into());
    }
    digest
}

pub(super) fn capture(path: &Path) -> Result<WorkspaceBaseline, String> {
    let repository_key = git_path(path, "--git-common-dir")?;
    let workspace_key = git_path(path, "--git-dir")?;
    // Workflow workers require an attributable base commit. Missing/corrupt
    // HEAD is not a clean/default baseline and cannot authorize a new worker.
    let head = String::from_utf8(git(path, &["rev-parse", "--verify", "HEAD"])?)
        .map_err(|error| error.to_string())?
        .trim()
        .into();
    let status = git(
        path,
        &["status", "--porcelain=v1", "-z", "--untracked-files=all"],
    )?;
    let staged_sha256 = diff_hash(path, true)?;
    let unstaged_sha256 = diff_hash(path, false)?;
    let names = git(path, &["ls-files", "--others", "--exclude-standard", "-z"])?;
    let mut untracked = Vec::new();
    for name in names
        .split(|byte| *byte == 0)
        .filter(|name| !name.is_empty())
    {
        let name = std::str::from_utf8(name).map_err(|_| "untracked baseline path is not UTF-8")?;
        let file = path.join(name);
        let metadata = file
            .symlink_metadata()
            .map_err(|error| format!("stat baseline {name}: {error}"))?;
        let (kind, sha256) = if metadata.file_type().is_symlink() {
            let target = std::fs::read_link(&file).map_err(|error| error.to_string())?;
            let target = target
                .to_str()
                .ok_or("baseline symlink target is not UTF-8")?;
            ("symlink", hex::encode(Sha256::digest(target.as_bytes())))
        } else if metadata.is_file() {
            (
                "file",
                hash_reader(std::fs::File::open(file).map_err(|error| error.to_string())?)?,
            )
        } else {
            return Err(format!("unsupported untracked baseline entry: {name}"));
        };
        untracked.push(UntrackedBaseline {
            path: name.into(),
            kind: kind.into(),
            sha256,
        });
    }
    // Refuse an observed concurrent change, rather than pinning a mixed status.
    if git(
        path,
        &["status", "--porcelain=v1", "-z", "--untracked-files=all"],
    )? != status
    {
        return Err("workspace changed while capturing launch baseline".into());
    }
    Ok(WorkspaceBaseline {
        repository_key,
        workspace_key,
        head,
        status_hex: hex::encode(status),
        staged_sha256,
        unstaged_sha256,
        untracked,
    })
}
