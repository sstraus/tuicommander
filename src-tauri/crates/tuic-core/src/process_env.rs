//! Read initial environment variables from a child process by PID.
//!
//! Only reads the env snapshot at exec() time — runtime `setenv()` changes are
//! NOT visible on macOS/Linux (and that's fine for our use case: we need env vars
//! like `CLAUDE_CONFIG_DIR` that are set by the shell before launching the agent).
//!
//! On Windows, the PEB environment block IS updated by `SetEnvironmentVariable`,
//! so it reflects the current state — but initial vars are always present.

/// Read a single environment variable from a process by PID.
///
/// Returns `None` if the process has exited, permission is denied,
/// the variable is not set, or the platform read fails.
pub fn read_process_env_var(pid: u32, name: &str) -> Option<String> {
    let entries = read_environ_raw(pid).ok()?;
    entries.into_iter().find_map(|entry| {
        let (key, value) = entry.split_once('=')?;
        // Windows environment names are case-insensitive, and the block keeps
        // whatever case the parent wrote: `Path`, not `PATH`. A case-sensitive
        // match there reports the most common variables as unset.
        let found = if cfg!(windows) {
            key.eq_ignore_ascii_case(name)
        } else {
            key == name
        };
        found.then(|| value.to_owned())
    })
}

/// Read the process environment while preserving read failures as errors.
/// A missing `CLAUDE_CONFIG_DIR` means the default profile only when the
/// environment itself was readable.
pub fn read_process_environment(pid: u32) -> Result<Vec<String>, std::io::Error> {
    read_environ_raw(pid)
}

/// Read `argv[0]` of a process by PID.
///
/// Needed to identify a CLI that runs inside a script interpreter: the executable
/// path names the interpreter (`node`), while argv[0] names the tool the user
/// actually launched (`pi`). Returns `None` on any platform read failure, and on
/// Windows, where the process-snapshot API exposes no argv.
#[cfg(any(target_os = "macos", target_os = "linux"))]
pub fn read_process_argv0(pid: u32) -> Option<String> {
    read_process_argv(pid)?
        .into_iter()
        .next()
        .filter(|s| !s.is_empty())
}

/// Read the full `argv` of a process by PID.
///
/// A shell alias is expanded before `exec`, so argv is the only place the real
/// binary and its flags survive: `c2` never reaches the process table, but
/// `claude --dangerously-skip-permissions` does. Returns `None` on any platform
/// read failure, and on Windows, where the process-snapshot API exposes no argv.
#[cfg(target_os = "macos")]
pub fn read_process_argv(pid: u32) -> Option<Vec<String>> {
    let buf = read_procargs2_raw(pid).ok()?;
    let int_size = std::mem::size_of::<libc::c_int>();
    if buf.len() < int_size {
        return None;
    }
    let mut n_args: libc::c_int = 0;
    unsafe {
        std::ptr::copy_nonoverlapping(buf.as_ptr(), (&raw mut n_args).cast::<u8>(), int_size);
    }
    if !(0..=65535).contains(&n_args) {
        return None;
    }
    // Layout: [i32 argc] [exec_path\0 ...padding...] [argv[0]\0 ... argv[n-1]\0] [env…]
    let mut data = skip_cstring(&buf[int_size..]);
    let mut argv = Vec::with_capacity(n_args as usize);
    for _ in 0..n_args {
        data = skip_leading_nulls(data);
        if data.is_empty() {
            break;
        }
        let end = data.iter().position(|&b| b == 0).unwrap_or(data.len());
        argv.push(String::from_utf8_lossy(&data[..end]).into_owned());
        data = &data[end..];
    }
    (!argv.is_empty()).then_some(argv)
}

#[cfg(target_os = "linux")]
pub fn read_process_argv(pid: u32) -> Option<Vec<String>> {
    let buf = std::fs::read(format!("/proc/{pid}/cmdline")).ok()?;
    let argv: Vec<String> = buf
        .split(|&b| b == 0)
        .filter(|s| !s.is_empty())
        .map(|s| String::from_utf8_lossy(s).into_owned())
        .collect();
    (!argv.is_empty()).then_some(argv)
}

#[cfg(target_os = "windows")]
pub fn read_process_argv(_pid: u32) -> Option<Vec<String>> {
    None
}

// ─── macOS ──────────────────────────────────────────────────────────────────

/// Fetch the raw `KERN_PROCARGS2` blob for a PID: `[i32 argc] [exec_path\0 …padding…]
/// [argv…] [env…]`. Shared by the argv[0] and environment readers.
#[cfg(target_os = "macos")]
fn read_procargs2_raw(pid: u32) -> Result<Vec<u8>, std::io::Error> {
    use std::io::Error;

    let mut mib: [libc::c_int; 3] = [libc::CTL_KERN, libc::KERN_PROCARGS2, pid as libc::c_int];

    // Step 1: query required buffer size
    let mut buf_size: libc::size_t = 0;
    let ret = unsafe {
        libc::sysctl(
            mib.as_mut_ptr(),
            3,
            std::ptr::null_mut(),
            &mut buf_size,
            std::ptr::null_mut(),
            0,
        )
    };
    if ret != 0 || buf_size == 0 {
        return Err(Error::last_os_error());
    }

    // Safety margin — XNU can under-report by a few bytes (see sysinfo crate).
    buf_size += 512;

    // Step 2: read the buffer
    let mut buf: Vec<u8> = vec![0u8; buf_size];
    let ret = unsafe {
        libc::sysctl(
            mib.as_mut_ptr(),
            3,
            buf.as_mut_ptr().cast::<libc::c_void>(),
            &mut buf_size,
            std::ptr::null_mut(),
            0,
        )
    };
    if ret != 0 {
        return Err(Error::last_os_error());
    }
    buf.truncate(buf_size);
    Ok(buf)
}

#[cfg(target_os = "macos")]
fn read_environ_raw(pid: u32) -> Result<Vec<String>, std::io::Error> {
    use std::io::{Error, ErrorKind};

    let buf = read_procargs2_raw(pid)?;

    // Step 3: parse the KERN_PROCARGS2 layout
    //   [i32 argc] [exec_path\0 ...padding...] [argv[0]\0 ... argv[n-1]\0] [env[0]\0 ...]
    let int_size = std::mem::size_of::<libc::c_int>();
    if buf.len() < int_size {
        return Err(Error::new(ErrorKind::InvalidData, "buffer too small"));
    }

    let mut n_args: libc::c_int = 0;
    unsafe {
        std::ptr::copy_nonoverlapping(buf.as_ptr(), (&raw mut n_args).cast::<u8>(), int_size);
    }
    if !(0..=65535).contains(&n_args) {
        return Err(Error::new(ErrorKind::InvalidData, "implausible argc"));
    }

    let mut data = &buf[int_size..];

    // Skip exec_path
    data = skip_cstring(data);

    // Skip n_args argv strings (with possible padding nulls between them)
    for _ in 0..n_args {
        data = skip_leading_nulls(data);
        if data.is_empty() {
            break;
        }
        data = skip_cstring(data);
    }

    // Remaining data is the env section
    Ok(collect_cstrings(data))
}

// ─── Linux ──────────────────────────────────────────────────────────────────

#[cfg(target_os = "linux")]
fn read_environ_raw(pid: u32) -> Result<Vec<String>, std::io::Error> {
    let buf = std::fs::read(format!("/proc/{pid}/environ"))?;
    Ok(buf
        .split(|&b| b == 0)
        .filter(|s| !s.is_empty())
        .map(|s| String::from_utf8_lossy(s).into_owned())
        .collect())
}

// ─── Windows ────────────────────────────────────────────────────────────────

#[cfg(target_os = "windows")]
fn read_environ_raw(pid: u32) -> Result<Vec<String>, std::io::Error> {
    use std::io::Error;
    use windows_sys::Win32::Foundation::CloseHandle;
    use windows_sys::Win32::System::Threading::{
        OpenProcess, PROCESS_QUERY_INFORMATION, PROCESS_VM_READ,
    };

    unsafe {
        let handle = OpenProcess(PROCESS_QUERY_INFORMATION | PROCESS_VM_READ, 0, pid);
        if handle.is_null() {
            return Err(Error::last_os_error());
        }

        let result = read_environ_from_handle(handle);
        CloseHandle(handle);
        result
    }
}

#[cfg(target_os = "windows")]
unsafe fn read_environ_from_handle(
    handle: windows_sys::Win32::Foundation::HANDLE,
) -> Result<Vec<String>, std::io::Error> {
    use std::io::Error;
    use windows_sys::Wdk::System::Threading::{NtQueryInformationProcess, ProcessBasicInformation};
    use windows_sys::Win32::System::Diagnostics::Debug::ReadProcessMemory;

    // Step 1: Get PEB address via NtQueryInformationProcess
    #[repr(C)]
    struct ProcessBasicInfo {
        reserved1: usize,
        peb_base_address: usize,
        reserved2: [usize; 4],
    }

    let mut pbi: ProcessBasicInfo = unsafe { std::mem::zeroed() };
    let mut return_length: u32 = 0;
    let status = unsafe {
        NtQueryInformationProcess(
            handle,
            ProcessBasicInformation,
            (&raw mut pbi).cast(),
            std::mem::size_of::<ProcessBasicInfo>() as u32,
            &mut return_length,
        )
    };
    if status != 0 {
        return Err(Error::other(format!(
            "NtQueryInformationProcess failed: {status:#x}"
        )));
    }

    // Step 2: Read RTL_USER_PROCESS_PARAMETERS pointer from PEB
    // PEB layout: at offset 0x20 (64-bit) is ProcessParameters pointer
    let params_ptr_addr = pbi.peb_base_address + 0x20;
    let mut params_ptr: usize = 0;
    let mut bytes_read: usize = 0;
    if unsafe {
        ReadProcessMemory(
            handle,
            params_ptr_addr as *const _,
            (&raw mut params_ptr).cast(),
            std::mem::size_of::<usize>(),
            &mut bytes_read,
        )
    } == 0
    {
        return Err(Error::last_os_error());
    }

    // Step 3: Read Environment pointer and length from RTL_USER_PROCESS_PARAMETERS
    // At offset 0x80 (64-bit): Environment (pointer), EnvironmentSize not directly available
    // UNICODE_STRING at offset 0x60 is EnvironmentVersion, Environment is at 0x80
    let env_ptr_addr = params_ptr + 0x80;
    let mut env_ptr: usize = 0;
    if unsafe {
        ReadProcessMemory(
            handle,
            env_ptr_addr as *const _,
            (&raw mut env_ptr).cast(),
            std::mem::size_of::<usize>(),
            &mut bytes_read,
        )
    } == 0
    {
        return Err(Error::last_os_error());
    }

    if env_ptr == 0 {
        return Ok(Vec::new());
    }

    // Step 4: Read the environment block (UTF-16 null-terminated strings,
    // double-null terminated) a page at a time, stopping at the terminator.
    //
    // One big read is what the first version did, and it never worked: the
    // block is a few kilobytes at the end of its own region, so a 32 KB request
    // runs off the end of the mapping, `ReadProcessMemory` fails the whole call
    // with ERROR_PARTIAL_COPY and every lookup — `CLAUDE_CONFIG_DIR`, the
    // agent's own launch env — answered "not set" on Windows. Page-sized reads
    // stay inside the mapping, and a read that does fail after the first ends
    // the walk with what the earlier ones already returned rather than
    // discarding it.
    const PAGE: usize = 2048; // u16s: one 4 KB page
    let mut env_buf: Vec<u16> = Vec::new();
    loop {
        let mut page = [0u16; PAGE];
        let ok = unsafe {
            ReadProcessMemory(
                handle,
                (env_ptr + env_buf.len() * 2) as *const _,
                page.as_mut_ptr().cast(),
                PAGE * 2,
                &mut bytes_read,
            )
        } != 0;
        if !ok {
            // A failed page after a good one is the end of the mapping, not an
            // error: the entries already read are the answer.
            if env_buf.is_empty() {
                return Err(Error::last_os_error());
            }
            break;
        }
        let words_read = bytes_read / 2;
        if words_read == 0 {
            break;
        }
        // The block ends at a double null; anything past it belongs to whatever
        // the process put there next. Look from one word before the new page so
        // a terminator split across the boundary is still seen.
        let scan_from = env_buf.len().saturating_sub(1);
        env_buf.extend_from_slice(&page[..words_read]);
        if env_buf[scan_from..].windows(2).any(|w| w == [0, 0]) {
            break;
        }
    }

    // Parse: split on null u16, stop at double-null (empty string)
    let mut entries = Vec::new();
    let mut start = 0;
    for i in 0..env_buf.len() {
        if env_buf[i] == 0 {
            if i == start {
                break; // double-null → end of env block
            }
            let s = String::from_utf16_lossy(&env_buf[start..i]);
            if !s.is_empty() {
                entries.push(s);
            }
            start = i + 1;
        }
    }

    Ok(entries)
}

// ─── Shared helpers ─────────────────────────────────────────────────────────

#[cfg(target_os = "macos")]
fn skip_cstring(data: &[u8]) -> &[u8] {
    match data.iter().position(|&b| b == 0) {
        Some(pos) => &data[pos + 1..],
        None => &[],
    }
}

#[cfg(target_os = "macos")]
fn skip_leading_nulls(data: &[u8]) -> &[u8] {
    let n = data.iter().take_while(|&&b| b == 0).count();
    &data[n..]
}

#[cfg(target_os = "macos")]
fn collect_cstrings(mut data: &[u8]) -> Vec<String> {
    let mut result = Vec::new();
    loop {
        let n = data.iter().take_while(|&&b| b == 0).count();
        data = &data[n..];
        if data.is_empty() {
            break;
        }
        let end = data.iter().position(|&b| b == 0).unwrap_or(data.len());
        if end == 0 {
            break;
        }
        result.push(String::from_utf8_lossy(&data[..end]).into_owned());
        data = &data[end..];
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn read_own_env() {
        // We can always read our own process env
        let pid = std::process::id();
        let path = read_process_env_var(pid, "PATH");
        assert!(path.is_some(), "PATH should always be set");
        assert_ne!(path.unwrap(), "");
    }

    #[test]
    fn read_nonexistent_var() {
        let pid = std::process::id();
        let val = read_process_env_var(pid, "TUIC_TEST_NONEXISTENT_VAR_12345");
        assert!(val.is_none());
    }

    #[test]
    fn read_dead_process() {
        // PID 0 or a very high PID should fail gracefully
        let val = read_process_env_var(99999999, "PATH");
        assert!(val.is_none());
    }

    #[cfg(any(target_os = "macos", target_os = "linux"))]
    #[test]
    fn read_own_argv0() {
        // Our own argv[0] is always readable and never empty — the property the
        // interpreter fallback in pty.rs depends on.
        let argv0 = read_process_argv0(std::process::id());
        assert!(argv0.is_some_and(|s| !s.is_empty()));
    }

    #[cfg(any(target_os = "macos", target_os = "linux"))]
    #[test]
    fn read_argv0_of_dead_process() {
        assert!(read_process_argv0(99999999).is_none());
    }

    #[cfg(any(target_os = "macos", target_os = "linux"))]
    #[test]
    fn read_own_argv_starts_with_argv0() {
        // Full argv is what a rebuilt launch command is made of: the flags an alias
        // hid live in argv[1..], so reading only argv[0] would drop them.
        let pid = std::process::id();
        let argv = read_process_argv(pid).expect("own argv is readable");
        assert_ne!(argv, [] as [std::string::String; 0]);
        assert_eq!(Some(&argv[0]), read_process_argv0(pid).as_ref());
    }

    #[test]
    fn read_argv_of_dead_process() {
        assert!(read_process_argv(99999999).is_none());
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn parse_procargs2_env_basic() {
        // Simulate a minimal KERN_PROCARGS2 buffer:
        // argc=1, exec="/bin/sh\0", argv[0]="sh\0", env "FOO=bar\0" "BAZ=qux\0"
        let mut buf = Vec::new();
        buf.extend_from_slice(&1i32.to_ne_bytes()); // argc = 1
        buf.extend_from_slice(b"/bin/sh\0"); // exec_path
        buf.extend_from_slice(b"sh\0"); // argv[0]
        buf.extend_from_slice(b"FOO=bar\0"); // env[0]
        buf.extend_from_slice(b"BAZ=qux\0"); // env[1]

        let entries = collect_cstrings(&buf[4 + 8 + 3..]); // skip argc + exec + argv
        // The real parser uses skip_cstring logic; test collect_cstrings directly
        assert!(entries.contains(&"FOO=bar".to_string()));
        assert!(entries.contains(&"BAZ=qux".to_string()));
    }
}
