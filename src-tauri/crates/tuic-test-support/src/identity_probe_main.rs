//! `tuic-identity-probe`: a tiny native executable tests copy under whatever
//! name they need a process to have (`claude`, `bash`, `tuic-remote`, …).
//!
//! Compiled by this crate's `build.rs` and installed with
//! `tuic_test_support::install_identity_probe`. Std only: it is built with a
//! bare `rustc` invocation, outside Cargo's dependency graph.
//!
//! Why it exists: a process's identity is its executable path (macOS
//! `proc_pidpath`, Linux `/proc/<pid>/exe`), so a test that needs a process
//! named `claude` must exec a real file named `claude`. Copying an Apple
//! platform binary (`/bin/cat`, `/bin/sh`, `/bin/sleep`) gets the copy
//! SIGKILLed by the sandbox, and symlinks, scripts and `exec -a` all report
//! the wrong identity. A binary this checkout compiled itself runs anywhere.
//!
//! Personalities:
//! - default: copy stdin to stdout until EOF, then exit 0. With a pipe or PTY
//!   on stdin that nobody writes to, it stays alive blocked in `read`, like
//!   `cat`.
//! - `--print <words…>`: print the words joined by spaces and a newline, exit 0.
//! - `ps` (invoked under the file name `ps`, or with `--ps` first): the subset
//!   `-p <pid> -o comm=` prints that pid's executable path (no header) and
//!   exits 0, or exits 1 when no such process is visible. Read natively, so it
//!   works where the real setuid `/bin/ps` cannot be executed.

use std::io::{Read, Write};
use std::path::PathBuf;
use std::process::ExitCode;

fn main() -> ExitCode {
    let mut args: Vec<String> = std::env::args().collect();
    let program = if args.is_empty() {
        String::new()
    } else {
        args.remove(0)
    };
    if invoked_as_ps(&program) {
        return ps(&args);
    }
    match args.first().map(String::as_str) {
        Some("--ps") => ps(&args[1..]),
        Some("--print") => {
            println!("{}", args[1..].join(" "));
            ExitCode::SUCCESS
        }
        _ => echo_stdin(),
    }
}

/// `ps`, or `ps.exe` on Windows, whatever directory it was run from.
fn invoked_as_ps(program: &str) -> bool {
    let name = std::path::Path::new(program)
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("");
    let name = name.strip_suffix(".exe").unwrap_or(name);
    name == "ps"
}

fn echo_stdin() -> ExitCode {
    let mut stdin = std::io::stdin().lock();
    let mut stdout = std::io::stdout().lock();
    let mut buf = [0u8; 8192];
    loop {
        match stdin.read(&mut buf) {
            Ok(0) => return ExitCode::SUCCESS,
            Ok(n) => {
                if stdout.write_all(&buf[..n]).and_then(|()| stdout.flush()).is_err() {
                    return ExitCode::from(1);
                }
            }
            Err(error) if error.kind() == std::io::ErrorKind::Interrupted => {}
            Err(_) => return ExitCode::from(1),
        }
    }
}

/// Exit 2 with a usage line, like `ps` does for an option it does not know.
fn usage(reason: &str) -> ExitCode {
    eprintln!("tuic-identity-probe ps: {reason}; supported: -p <pid> -o comm=");
    ExitCode::from(2)
}

fn ps(args: &[String]) -> ExitCode {
    let mut pid = None;
    let mut format = None;
    let mut iter = args.iter();
    while let Some(arg) = iter.next() {
        let (flag, inline) = match arg.as_str() {
            "-p" | "-o" => (arg.as_str(), None),
            other if other.starts_with("-p") || other.starts_with("-o") => {
                (&other[..2], Some(other[2..].to_string()))
            }
            other => return usage(&format!("unsupported argument {other:?}")),
        };
        let Some(value) = inline.or_else(|| iter.next().cloned()) else {
            return usage(&format!("{flag} needs a value"));
        };
        if flag == "-p" {
            match value.trim().parse::<u32>() {
                Ok(parsed) => pid = Some(parsed),
                Err(_) => return usage(&format!("bad pid {value:?}")),
            }
        } else {
            format = Some(value);
        }
    }
    if format.as_deref() != Some("comm=") {
        return usage("only `-o comm=` is implemented");
    }
    let Some(pid) = pid else {
        return usage("missing -p <pid>");
    };
    match executable_of(pid) {
        Some(path) => {
            println!("{}", path.display());
            ExitCode::SUCCESS
        }
        None => ExitCode::from(1),
    }
}

#[cfg(target_os = "macos")]
fn executable_of(pid: u32) -> Option<PathBuf> {
    use std::os::unix::ffi::OsStrExt;

    // libproc is part of libSystem, which every macOS executable links.
    unsafe extern "C" {
        fn proc_pidpath(pid: i32, buffer: *mut std::ffi::c_void, buffersize: u32) -> i32;
    }
    // PROC_PIDPATHINFO_MAXSIZE (4 * MAXPATHLEN).
    let mut buf = vec![0u8; 4096];
    let pid = i32::try_from(pid).ok()?;
    // SAFETY: `buf` is a live, writable allocation of exactly the size passed.
    let len = unsafe { proc_pidpath(pid, buf.as_mut_ptr().cast(), buf.len() as u32) };
    if len <= 0 {
        return None;
    }
    buf.truncate(len as usize);
    Some(PathBuf::from(std::ffi::OsStr::from_bytes(&buf)))
}

#[cfg(target_os = "linux")]
fn executable_of(pid: u32) -> Option<PathBuf> {
    std::fs::read_link(format!("/proc/{pid}/exe"))
        .ok()
        .or_else(|| {
            // A same-user process whose exe link is unreadable still has a comm.
            let comm = std::fs::read_to_string(format!("/proc/{pid}/comm")).ok()?;
            let comm = comm.trim();
            (!comm.is_empty()).then(|| PathBuf::from(comm))
        })
}

#[cfg(not(any(target_os = "macos", target_os = "linux")))]
fn executable_of(_pid: u32) -> Option<PathBuf> {
    None
}
