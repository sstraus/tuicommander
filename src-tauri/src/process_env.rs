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
    let prefix = format!("{name}=");
    entries
        .into_iter()
        .find_map(|entry| entry.strip_prefix(&prefix).map(str::to_owned))
}

// ─── macOS ──────────────────────────────────────────────────────────────────

#[cfg(target_os = "macos")]
fn read_environ_raw(pid: u32) -> Result<Vec<String>, std::io::Error> {
    use std::io::{Error, ErrorKind};

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

    // Step 4: Read the environment block (UTF-16 null-terminated strings, double-null terminated)
    // Read in 32KB chunks until we find the double-null terminator
    let mut env_buf: Vec<u16> = vec![0u16; 16384];
    if unsafe {
        ReadProcessMemory(
            handle,
            env_ptr as *const _,
            env_buf.as_mut_ptr().cast(),
            env_buf.len() * 2,
            &mut bytes_read,
        )
    } == 0
    {
        return Err(Error::last_os_error());
    }
    let words_read = bytes_read / 2;
    env_buf.truncate(words_read);

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
        assert!(!path.unwrap().is_empty());
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
