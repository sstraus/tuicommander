//! Windows-only transport experiment. No application state or parser is involved.
//! Uses the same portable-pty 0.9.0 native seam as TUIC's PTY lifecycle.
use portable_pty::{CommandBuilder, PtySize, native_pty_system};
use std::ffi::c_void;
use std::fs::{self, File};
use std::io::{Read, Write};
use std::path::Path;
use std::process::{Command, Stdio};
use std::sync::mpsc;
use std::time::{Duration, Instant};

type Handle = *mut c_void;
#[link(name = "kernel32")]
unsafe extern "system" {
    fn CreateFileW(
        name: *const u16,
        access: u32,
        share: u32,
        security: *const c_void,
        creation: u32,
        flags: u32,
        template: Handle,
    ) -> Handle;
    fn GetConsoleMode(handle: Handle, mode: *mut u32) -> i32;
    fn WriteFile(
        handle: Handle,
        buffer: *const u8,
        length: u32,
        written: *mut u32,
        overlapped: *mut c_void,
    ) -> i32;
    fn CloseHandle(handle: Handle) -> i32;
}

fn payload(terminator: &str) -> Vec<u8> {
    let end = if terminator == "bel" {
        "\x07"
    } else {
        "\x1b\\"
    };
    ["busy", "awaiting", "prompt", "idle"]
        .iter()
        .flat_map(|state| format!("\x1b]7770;state={state}{end}").into_bytes())
        .collect()
}

fn hook(directory: &Path, terminator: &str) -> Result<(), Box<dyn std::error::Error>> {
    let start = Instant::now();
    let name: Vec<u16> = "CONOUT$\0".encode_utf16().collect();
    let handle = unsafe {
        CreateFileW(
            name.as_ptr(),
            0xc0000000, // GENERIC_READ (GetConsoleMode) | GENERIC_WRITE
            3,
            std::ptr::null(),
            3,
            0,
            std::ptr::null_mut(),
        )
    };
    let mut log = File::create(directory.join("hook.txt"))?;
    writeln!(
        log,
        "hook_pid={} terminator={terminator}",
        std::process::id()
    )?;
    if handle as isize == -1 {
        writeln!(log, "open_error={}", std::io::Error::last_os_error())?;
        return Err("CONOUT$ open failed".into());
    }
    let mut mode = 0;
    let mode_ok = unsafe { GetConsoleMode(handle, &mut mode) };
    let mode_error = if mode_ok == 0 {
        Some(std::io::Error::last_os_error())
    } else {
        None
    };
    writeln!(
        log,
        "console_mode_ok={mode_ok} console_mode=0x{mode:x} error={mode_error:?}"
    )?;
    // Do not change modes or attach a console to obtain a pass.
    let mut bytes = b"HOOK-CONSOLE-BEGIN\r\n".to_vec();
    bytes.extend(payload(terminator));
    bytes.extend(b"HOOK-CONSOLE-END\r\n");
    fs::write(directory.join("write-request.bin"), &bytes)?;
    let mut written = 0;
    let ok = unsafe {
        WriteFile(
            handle,
            bytes.as_ptr(),
            bytes.len() as u32,
            &mut written,
            std::ptr::null_mut(),
        )
    };
    let error = if ok == 0 {
        Some(std::io::Error::last_os_error())
    } else {
        None
    };
    unsafe {
        CloseHandle(handle);
    }
    writeln!(
        log,
        "write_ok={ok} requested={} written={written} error={error:?} elapsed_us={}",
        bytes.len(),
        start.elapsed().as_micros()
    )?;
    println!("HOOK-STDOUT-CONTROL");
    if ok == 0 || written as usize != bytes.len() {
        return Err("console write failed".into());
    }
    Ok(())
}

fn agent(directory: &Path, terminator: &str, pipe: bool) -> Result<(), Box<dyn std::error::Error>> {
    let mut command = Command::new(std::env::current_exe()?);
    command.arg("hook").arg(directory).arg(terminator);
    fs::write(
        directory.join("agent.txt"),
        format!("agent_pid={} hook_stdout_pipe={pipe}\n", std::process::id()),
    )?;
    // Ordinary console child: no DETACHED_PROCESS, CREATE_NEW_CONSOLE or attachment.
    if pipe {
        let output = command
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .output()?;
        fs::write(directory.join("hook-stdout.bin"), output.stdout)?;
        fs::write(directory.join("hook-stderr.bin"), output.stderr)?;
        if !output.status.success() {
            return Err("hook failed".into());
        }
    } else if !command.status()?.success() {
        return Err("hook failed".into());
    }
    Ok(())
}

fn capture(
    directory: &Path,
    terminator: &str,
    pipe: bool,
) -> Result<(), Box<dyn std::error::Error>> {
    fs::create_dir_all(directory)?;
    let pair = native_pty_system().openpty(PtySize {
        rows: 41,
        cols: 128,
        pixel_width: 0,
        pixel_height: 0,
    })?;
    let mut reader = pair.master.try_clone_reader()?;
    let mut writer = pair.master.take_writer()?;
    let (tx, rx) = mpsc::channel();
    let destination = directory.to_path_buf();
    std::thread::spawn(move || {
        let result = (|| -> std::io::Result<()> {
            let mut raw = File::create(destination.join("conpty.bin"))?;
            let mut frames = File::create(destination.join("frames.tsv"))?;
            let start = Instant::now();
            let mut offset = 0;
            let mut buffer = [0u8; 4096];
            let mut query_tail = Vec::new();
            let mut replies = File::create(destination.join("terminal-input.bin"))?;
            loop {
                let count = reader.read(&mut buffer)?;
                if count == 0 {
                    break;
                }
                raw.write_all(&buffer[..count])?;
                raw.flush()?;
                writeln!(frames, "{}\t{offset}\t{count}", start.elapsed().as_micros())?;
                offset += count;
                // The first real runner capture stopped at ConPTY's startup DSR.
                // Supply the origin cursor response a terminal would send. Record
                // it separately; never filter or rewrite the captured output.
                for byte in &buffer[..count] {
                    query_tail.push(*byte);
                    if query_tail.ends_with(b"\x1b[6n") {
                        writer.write_all(b"\x1b[1;1R")?;
                        writer.flush()?;
                        replies.write_all(b"\x1b[1;1R")?;
                        replies.flush()?;
                    }
                    if query_tail.len() > 4 {
                        query_tail.remove(0);
                    }
                }
            }
            Ok(())
        })();
        let _ = tx.send(result);
    });
    let mut command = CommandBuilder::new(std::env::current_exe()?);
    command.arg("agent");
    command.arg(directory);
    command.arg(terminator);
    command.arg(if pipe { "pipe" } else { "inherit" });
    let mut child = pair.slave.spawn_command(command)?;
    drop(pair.slave);
    let start = Instant::now();
    let status = loop {
        if let Some(status) = child.try_wait()? {
            break status;
        }
        if start.elapsed() > Duration::from_secs(30) {
            child.kill()?;
            return Err("agent process exceeded 30 seconds".into());
        }
        std::thread::sleep(Duration::from_millis(20));
    };
    drop(pair.master);
    rx.recv_timeout(Duration::from_secs(15))??;
    fs::write(directory.join("status.txt"), format!("{status:?}\n"))?;
    if !status.success() {
        return Err("agent failed".into());
    }
    Ok(())
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<String> = std::env::args().collect();
    match args.get(1).map(String::as_str) {
        Some("hook") => hook(Path::new(&args[2]), &args[3]),
        Some("agent") => agent(Path::new(&args[2]), &args[3], args[4] == "pipe"),
        Some("capture") => {
            let root = fs::canonicalize(&args[2])?;
            for terminator in ["bel", "st"] {
                for pipe in [false, true] {
                    let name = format!("{terminator}-{}", if pipe { "pipe" } else { "inherit" });
                    capture(&root.join(name), terminator, pipe)?;
                }
            }
            Ok(())
        }
        _ => Err("usage: windows-hook-osc capture EXISTING_OUTPUT_DIRECTORY".into()),
    }
}
