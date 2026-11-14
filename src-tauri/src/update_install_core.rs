//! Installation diagnostics shared by the desktop command and isolated probes.
use std::error::Error;
#[cfg(any(target_os = "macos", all(test, unix)))]
use std::io;

/// Preserve wrappers such as tar::TarError, whose Display omits the IO cause.
pub(crate) fn error_chain(error: &(dyn Error + 'static)) -> String {
    let mut messages = Vec::new();
    let mut current = Some(error);
    while let Some(error) = current {
        messages.push(error.to_string());
        current = error.source();
    }
    messages.join("\nCaused by: ")
}

#[cfg(any(target_os = "macos", all(test, unix)))]
pub(crate) fn archive_space_check(bytes: &[u8], root: &std::path::Path) -> Result<(), String> {
    let (available, block) = volume_space(root).map_err(|e| error_chain(&e))?;
    let required = archive_required_space(bytes, block).map_err(|e| error_chain(&e))?;
    check_capacity(required, available, root)
}

#[cfg(any(target_os = "macos", all(test, unix)))]
fn volume_space(path: &std::path::Path) -> io::Result<(u64, u64)> {
    use std::os::unix::ffi::OsStrExt;
    let path = std::ffi::CString::new(path.as_os_str().as_bytes())?;
    let mut stats = std::mem::MaybeUninit::<libc::statvfs>::uninit();
    // SAFETY: path is NUL terminated; statvfs writes the entire result on success.
    if unsafe { libc::statvfs(path.as_ptr(), stats.as_mut_ptr()) } != 0 {
        return Err(io::Error::last_os_error());
    }
    // SAFETY: statvfs succeeded above.
    let stats = unsafe { stats.assume_init() };
    let block = stats.f_frsize.max(1);
    // f_bavail is u32 on macOS and u64 on Linux, so the cast is needed on one target.
    #[allow(clippy::unnecessary_cast)]
    let available = (stats.f_bavail as u64)
        .checked_mul(block)
        .ok_or_else(|| io::Error::other("Update free-space measurement overflow"))?;
    Ok((available, block))
}

/// Account for the extracted files, directory allocation and 1 MiB of metadata
/// headroom. The compressed download stays in memory; backup and install use
/// renames on the same volume, so they do not require a second bundle copy.
#[cfg(any(target_os = "macos", all(test, unix)))]
fn archive_required_space(bytes: &[u8], block: u64) -> io::Result<u64> {
    let mut archive = tar::Archive::new(flate2::read::GzDecoder::new(bytes));
    let mut required = 1024_u64 * 1024;
    for entry in archive.entries()? {
        let entry = entry?;
        let allocated = entry
            .size()
            .max(1)
            .div_ceil(block)
            .checked_mul(block)
            .ok_or_else(|| io::Error::other("Update archive size overflow"))?;
        required = required
            .checked_add(allocated)
            .ok_or_else(|| io::Error::other("Update archive size overflow"))?;
    }
    Ok(required)
}

#[cfg(any(target_os = "macos", all(test, unix)))]
fn check_capacity(required: u64, available: u64, root: &std::path::Path) -> Result<(), String> {
    if available < required {
        return Err(format!(
            "Not enough free space to install the update on {}: requires {required} bytes, available {available} bytes. Free space and retry.",
            root.display()
        ));
    }
    Ok(())
}

#[cfg(target_os = "macos")]
pub(crate) fn extraction_root(
    app: &std::path::Path,
    temp: &std::path::Path,
) -> io::Result<std::path::PathBuf> {
    use std::os::unix::fs::MetadataExt;
    // Match updater 2.14's volume selection and do not follow the install symlink.
    let app_dev = std::fs::symlink_metadata(app)?.dev();
    [Some(temp), app.parent()]
        .into_iter()
        .flatten()
        .find(|path| path.metadata().is_ok_and(|m| m.dev() == app_dev))
        .map(std::path::Path::to_path_buf)
        .ok_or_else(|| io::Error::other("Update temporary directory is not on the app's volume"))
}

#[cfg(test)]
#[path = "update_install_core_tests.rs"]
mod tests;
