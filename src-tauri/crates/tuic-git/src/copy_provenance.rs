//! Copy-time proof for non-ignored payloads introduced by worktree warming.
//! Ignored build artifacts keep Git's normal ignored-file semantics.

use crate::git_cli::git_cmd;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::BTreeMap;
use std::path::{Component, Path, PathBuf};

const MARKER: &str = "tuic-copy-provenance.json";

#[derive(Serialize, Deserialize)]
struct Receipt {
    checkout: PathBuf,
    git_dir: PathBuf,
    source: PathBuf,
    files: BTreeMap<PathBuf, String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct CopyInventoryEntry {
    pub path: PathBuf,
    pub classification: String,
}

fn marker(checkout: &Path) -> Result<PathBuf, String> {
    crate::git::resolve_git_dir(checkout)
        .map(|admin| admin.join(MARKER))
        .ok_or_else(|| "Cannot resolve copy provenance Git directory".into())
}

fn regular_hash(root: &Path, relative: &Path) -> Result<String, String> {
    if std::fs::symlink_metadata(root)
        .map_err(|e| e.to_string())?
        .file_type()
        .is_symlink()
    {
        return Err("Symlink at copied root".into());
    }
    let mut path = root.to_path_buf();
    for component in relative.components() {
        if !matches!(component, Component::Normal(_)) {
            return Err("Invalid copied path".into());
        }
        path.push(component);
        let metadata = std::fs::symlink_metadata(&path).map_err(|e| e.to_string())?;
        if metadata.file_type().is_symlink() {
            return Err("Symlink in copied path".into());
        }
    }
    if !std::fs::symlink_metadata(&path)
        .map_err(|e| e.to_string())?
        .is_file()
    {
        return Err("Copied path is not a regular file".into());
    }
    let mut file = std::fs::File::open(path).map_err(|e| e.to_string())?;
    let mut hash = Sha256::new();
    use std::io::Read;
    let mut buffer = [0u8; 65536];
    loop {
        let size = file.read(&mut buffer).map_err(|e| e.to_string())?;
        if size == 0 {
            break;
        }
        hash.update(&buffer[..size]);
    }
    Ok(hex::encode(hash.finalize()))
}

/// Snapshot before copying, not after: a source edit during the copy must not
/// acquire proof for bytes that were never copied. Never hash ignored caches.
pub(crate) fn snapshot(
    source: &Path,
    checkout: &Path,
    relative: &Path,
) -> Result<BTreeMap<PathBuf, String>, String> {
    fn visit(
        source: &Path,
        checkout: &Path,
        relative: &Path,
        files: &mut BTreeMap<PathBuf, String>,
    ) -> Result<(), String> {
        let metadata =
            std::fs::symlink_metadata(source.join(relative)).map_err(|e| e.to_string())?;
        let mut ignore_path = relative.as_os_str().to_os_string();
        if metadata.is_dir() {
            // Git path syntax uses a trailing slash to classify an absent directory.
            ignore_path.push("/");
        }
        let ignored = git_cmd(checkout)
            .args(["check-ignore", "-q", "--"])
            .args([ignore_path])
            .run_raw()
            .map_err(|e| e.to_string())?;
        match ignored.status.code() {
            Some(0) => return Ok(()),
            Some(1) => {}
            _ => return Err("Cannot inspect copied path ignore rules".into()),
        }
        if metadata.is_dir() {
            for entry in std::fs::read_dir(source.join(relative)).map_err(|e| e.to_string())? {
                let entry = entry.map_err(|e| e.to_string())?;
                visit(source, checkout, &relative.join(entry.file_name()), files)?;
            }
        } else if metadata.is_file() {
            files.insert(relative.to_path_buf(), regular_hash(source, relative)?);
        }
        // Symlinks never receive copy proof; Git's ordinary dirty gate owns them.
        Ok(())
    }
    let mut files = BTreeMap::new();
    visit(source, checkout, relative, &mut files)?;
    Ok(files)
}

/// An atomic receipt is published only for bytes verified on both sides.
/// If copying or receipt persistence fails, ordinary untracked safety remains.
pub(crate) fn record(
    source: &Path,
    checkout: &Path,
    files: BTreeMap<PathBuf, String>,
) -> Result<(), String> {
    if files.is_empty() {
        return Ok(());
    }
    let checkout = checkout.canonicalize().map_err(|e| e.to_string())?;
    let source = source.canonicalize().map_err(|e| e.to_string())?;
    let path = marker(&checkout)?;
    let mut receipt = match read(&checkout)? {
        Some(receipt) if receipt.source == source => receipt,
        Some(_) => return Err("Copy provenance source changed".into()),
        None => Receipt {
            checkout: checkout.clone(),
            git_dir: path
                .parent()
                .unwrap()
                .canonicalize()
                .map_err(|e| e.to_string())?,
            source: source.clone(),
            files: BTreeMap::new(),
        },
    };
    // A copied .gitignore can change destination rules. Only paths Git actually
    // reports as untracked after the copy may receive an exemption.
    let visible = git_cmd(&checkout)
        .args(["ls-files", "--others", "--exclude-standard", "-z"])
        .run()
        .map_err(|e| e.to_string())?;
    let visible: std::collections::HashSet<_> = visible
        .stdout
        .split('\0')
        .filter(|path| !path.is_empty())
        .map(PathBuf::from)
        .collect();
    for (relative, hash) in files {
        if visible.contains(&relative)
            && regular_hash(&source, &relative).as_ref() == Ok(&hash)
            && regular_hash(&checkout, &relative).as_ref() == Ok(&hash)
        {
            receipt.files.insert(relative, hash);
        }
    }
    let mut file =
        tempfile::NamedTempFile::new_in(path.parent().unwrap()).map_err(|e| e.to_string())?;
    use std::io::Write;
    file.write_all(&serde_json::to_vec(&receipt).map_err(|e| e.to_string())?)
        .and_then(|()| file.as_file().sync_all())
        .map_err(|e| e.to_string())?;
    file.persist(path).map_err(|e| e.to_string())?;
    Ok(())
}

fn read(checkout: &Path) -> Result<Option<Receipt>, String> {
    let path = marker(checkout)?;
    let bytes = match std::fs::read(&path) {
        Ok(bytes) => bytes,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(e) => return Err(format!("Cannot read copy provenance: {e}")),
    };
    let receipt: Receipt =
        serde_json::from_slice(&bytes).map_err(|e| format!("Invalid copy provenance: {e}"))?;
    // Git moves retain their administrative directory; a recycled pathname
    // with a new registration must not inherit the old checkout's proof.
    if receipt.git_dir
        != path
            .parent()
            .unwrap()
            .canonicalize()
            .map_err(|e| e.to_string())?
    {
        return Err("Copy provenance checkout identity changed".into());
    }
    Ok(Some(receipt))
}

/// Shared by fresh lifecycle and final no-force removal. Status is Git's -z
/// porcelain format, so quoted names, spaces and newlines cannot bypass safety.
pub(crate) fn inspect(
    checkout: &Path,
    status: &str,
) -> Result<(usize, Vec<CopyInventoryEntry>), String> {
    let receipt = read(checkout)?;
    let mut inventory = BTreeMap::new();
    if let Some(receipt) = &receipt {
        for (relative, expected) in &receipt.files {
            let classification = match regular_hash(checkout, relative) {
                Ok(hash) if &hash != expected => "modified",
                Ok(_) if regular_hash(&receipt.source, relative).as_ref() != Ok(expected) => {
                    "source-changed"
                }
                Ok(_) => "unchanged-copy",
                Err(_) => {
                    let mut path = checkout.to_path_buf();
                    let symlink = relative.components().any(|component| {
                        path.push(component);
                        path.symlink_metadata()
                            .is_ok_and(|m| m.file_type().is_symlink())
                    });
                    if symlink {
                        "symlink"
                    } else if !checkout.join(relative).exists() {
                        "missing"
                    } else {
                        "unreadable-or-nonregular"
                    }
                }
            };
            inventory.insert(relative.clone(), classification.to_string());
        }
    }
    let mut dirty = 0;
    let mut reported = std::collections::HashSet::new();
    let mut entries = status.split('\0').filter(|entry| !entry.is_empty());
    while let Some(entry) = entries.next() {
        let Some(relative) = entry.get(3..) else {
            return Err("Cannot parse worktree status".into());
        };
        let path = PathBuf::from(relative);
        reported.insert(path.clone());
        if !entry.starts_with("?? ")
            || inventory.get(&path).map(String::as_str) != Some("unchanged-copy")
        {
            dirty += 1;
        }
        if entry.starts_with("?? ") {
            inventory.entry(path).or_insert_with(|| {
                if receipt.is_some() {
                    "new"
                } else {
                    "uncertain-no-copy-proof"
                }
                .into()
            });
        }
        if entry
            .as_bytes()
            .get(..2)
            .is_some_and(|xy| xy.contains(&b'R') || xy.contains(&b'C'))
        {
            entries.next(); // -z rename/copy source path, not a second status entry.
        }
    }
    // A deleted copy holds no bytes that removal could lose, so it is not a
    // change. Every other unproven copy still blocks (story 3006-ea19).
    for (path, classification) in &inventory {
        if !matches!(classification.as_str(), "unchanged-copy" | "missing")
            && !reported.contains(path)
        {
            dirty += 1;
        }
    }
    Ok((
        dirty,
        inventory
            .into_iter()
            .map(|(path, classification)| CopyInventoryEntry {
                path,
                classification,
            })
            .collect(),
    ))
}

pub fn inventory(checkout: &Path) -> Result<Vec<CopyInventoryEntry>, String> {
    let status = git_cmd(checkout)
        .args([
            "status",
            "--porcelain",
            "-z",
            "--untracked-files=all",
            "--ignore-submodules=none",
        ])
        .run()
        .map_err(|e| e.to_string())?;
    inspect(checkout, &status.stdout).map(|(_, inventory)| inventory)
}
