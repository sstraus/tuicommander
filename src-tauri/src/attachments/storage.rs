//! The only attachment filesystem boundary; repository paths never enter here.
use std::io::Write;
use std::path::{Path, PathBuf};

fn owner_file(path: &Path) -> PathBuf {
    path.with_file_name(format!(
        ".owner-{}",
        path.file_name().unwrap_or_default().to_string_lossy()
    ))
}

fn leaf(kind: &str, id: &str) -> Result<String, String> {
    if !["pty", "acp"].contains(&kind)
        || id.is_empty()
        || id.len() > 128
        || !id
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_')
    {
        return Err("invalid attachment session id".into());
    }
    Ok(format!("{kind}-{id}"))
}

fn checked_directory(path: &Path, create: bool) -> Result<(), String> {
    if create && !path.exists() {
        let builder = std::fs::DirBuilder::new();
        #[cfg(unix)]
        let builder = {
            use std::os::unix::fs::DirBuilderExt;
            let mut builder = builder;
            builder.mode(0o700);
            builder
        };
        match builder.create(path) {
            Ok(()) => {}
            Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {}
            Err(error) => return Err(format!("create attachment directory: {error}")),
        }
    }
    let meta = path.symlink_metadata().map_err(|e| e.to_string())?;
    if !meta.file_type().is_dir() {
        return Err("attachment directory is not a real directory".into());
    }
    Ok(())
}

fn root(data_dir: &Path, create: bool) -> Result<PathBuf, String> {
    let data = std::fs::canonicalize(data_dir).map_err(|e| e.to_string())?;
    let root = data.join("attachments");
    checked_directory(&root, create)?;
    Ok(root)
}

pub(crate) fn prepare(data_dir: &Path, kind: &str, id: &str) -> Result<PathBuf, String> {
    let name = leaf(kind, id)?;
    let path = root(data_dir, true)?.join(name);
    checked_directory(&path, true)?;
    let owner = owner_file(&path);
    match std::fs::File::create_new(&owner) {
        Ok(mut file) => file
            .write_all(std::process::id().to_string().as_bytes())
            .map_err(|e| format!("write attachment owner: {e}"))?,
        Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {
            if owner_pid(&path) != Some(std::process::id()) {
                return Err("attachment directory belongs to another process".into());
            }
        }
        Err(error) => return Err(format!("create attachment owner: {error}")),
    }
    Ok(path)
}

pub(crate) fn remove(data_dir: &Path, kind: &str, id: &str) {
    let Ok(name) = leaf(kind, id) else { return };
    let Ok(root) = root(data_dir, false) else {
        return;
    };
    let path = root.join(name);
    // Never follow a root or session symlink. remove_dir_all itself does not
    // follow symlinks inside a real session directory either.
    if checked_directory(&path, false).is_err() {
        return;
    }
    match std::fs::remove_dir_all(&path) {
        Ok(()) => {
            if let Err(error) = std::fs::remove_file(owner_file(&path))
                && error.kind() != std::io::ErrorKind::NotFound
            {
                tracing::warn!(path = %path.display(), %error, "Could not remove attachment owner");
            }
        }
        Err(error) => {
            tracing::warn!(path = %path.display(), %error, "Could not remove session attachments")
        }
    }
}

fn owner_pid(path: &Path) -> Option<u32> {
    let owner = owner_file(path);
    if !owner.symlink_metadata().ok()?.file_type().is_file() {
        return None;
    }
    std::fs::read_to_string(owner).ok()?.parse().ok()
}

#[cfg(unix)]
fn owner_is_dead(pid: u32) -> bool {
    let Ok(pid) = i32::try_from(pid) else {
        return false;
    };
    if pid <= 0 {
        return false;
    }
    // Same conservative existence check as Design Mode's profile owner:
    // permission errors and PID reuse preserve files rather than risk deletion.
    // SAFETY: signal zero only checks existence and sends no signal.
    (unsafe { libc::kill(pid, 0) }) == -1
        && std::io::Error::last_os_error().raw_os_error() == Some(libc::ESRCH)
}

#[cfg(windows)]
fn owner_is_dead(pid: u32) -> bool {
    use windows_sys::Win32::Foundation::{
        CloseHandle, ERROR_INVALID_PARAMETER, GetLastError, STILL_ACTIVE,
    };
    use windows_sys::Win32::System::Threading::{
        GetExitCodeProcess, OpenProcess, PROCESS_QUERY_LIMITED_INFORMATION,
    };
    if pid == 0 {
        return false;
    }
    // SAFETY: query-only handle, valid output pointer, and exactly one close.
    unsafe {
        let handle = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, 0, pid);
        if handle.is_null() {
            return GetLastError() == ERROR_INVALID_PARAMETER;
        }
        let mut code = STILL_ACTIVE as u32;
        let queried = GetExitCodeProcess(handle, &mut code) != 0;
        CloseHandle(handle);
        queried && code != STILL_ACTIVE as u32
    }
}

/// Reclaim only directories with evidence that their owning process ended.
/// Missing/invalid ownership (including an interrupted first write) is preserved.
pub(crate) fn cleanup_orphans(data_dir: &Path) {
    let Ok(root) = root(data_dir, false) else {
        return;
    };
    let Ok(entries) = std::fs::read_dir(&root) else {
        return;
    };
    for entry in entries.flatten() {
        let name = entry.file_name();
        let Some(name) = name.to_str() else { continue };
        let Some((kind, id)) = name.split_once('-') else {
            continue;
        };
        if checked_directory(&entry.path(), false).is_ok()
            && owner_pid(&entry.path()).is_some_and(owner_is_dead)
        {
            remove(data_dir, kind, id);
        }
    }
}
