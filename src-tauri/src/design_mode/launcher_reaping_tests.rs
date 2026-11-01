use super::*;
use std::os::unix::fs::PermissionsExt;
use std::time::{Duration, Instant};

const ROLE: &str = "TUIC_DESIGN_REAP_TEST";
const TEST: &str =
    "design_mode::launcher::reaping_tests::design_launch_reaps_child_after_runtime_shutdown";

// Catches: a dropped browser Child loses its wait owner when the launch runtime ends.
#[test]
fn design_launch_reaps_child_after_runtime_shutdown() {
    if let Some(root) = std::env::var_os(ROLE) {
        let root = PathBuf::from(root);
        let _config = crate::config::set_config_dir_override(root.join("config"));
        let state = Arc::new(crate::state::tests_support::make_test_app_state());
        *state.bound_socket_path.write() = root.join("socket");
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap();
        let target = "http://localhost:5173/page?x=1#element";
        let acknowledge = {
            let _entered = runtime.enter();
            acknowledge_test_launch(state.clone(), root.join("repo"))
        };
        let response = runtime
            .block_on(launch(
                &state,
                &root.join("repo"),
                "agent",
                Some("chrome"),
                Some(target),
            ))
            .unwrap();
        runtime.block_on(acknowledge).unwrap();
        assert_eq!(response["url"], target);
        assert_eq!(response["status"], "armed");
        let setup = Instant::now() + Duration::from_secs(120);
        let pid: libc::pid_t = loop {
            if let Ok(text) = std::fs::read_to_string(root.join("pid"))
                && let Ok(pid) = text.parse()
            {
                break pid;
            }
            assert!(Instant::now() < setup, "launch child did not publish PID");
            std::thread::sleep(Duration::from_millis(10));
        };
        let args = std::fs::read_to_string(root.join("args")).unwrap();
        assert_eq!(args.lines().last(), Some(target));
        assert!(args.contains("--user-data-dir="));
        // The gated child is still alive: launch returned without awaiting exit.
        // SAFETY: signal zero only observes this test's child; it cannot kill.
        assert_eq!(unsafe { libc::kill(pid, 0) }, 0);
        drop(runtime);
        std::fs::write(root.join("release"), "exit").unwrap();
        let cleanup = Instant::now() + Duration::from_secs(30);
        loop {
            // SAFETY: signal zero only observes the child this test launched.
            if unsafe { libc::kill(pid, 0) } == -1
                && std::io::Error::last_os_error().raw_os_error() == Some(libc::ESRCH)
            {
                break;
            }
            if Instant::now() >= cleanup {
                let mut status = 0;
                // SAFETY: clean up only this test's child on failure.
                let waited = unsafe { libc::waitpid(pid, &mut status, libc::WNOHANG) };
                panic!(
                    "Design Mode child {pid} unreaped after runtime shutdown; waitpid={waited}; exited={}",
                    waited == pid && libc::WIFEXITED(status)
                );
            }
            std::thread::sleep(Duration::from_millis(10));
        }
        return;
    }
    let dir = tempfile::tempdir_in(crate::test_support::test_temp_root()).unwrap();
    let root = dir.path();
    std::fs::create_dir_all(root.join(".cargo/bin")).unwrap();
    std::fs::create_dir(root.join("repo")).unwrap();
    super::super::discovery::tests::git(&root.join("repo"), &["init"]);
    let bridge = root.join(".cargo/bin/tuic-bridge");
    std::fs::write(
        &bridge,
        "#!/bin/sh\nmkdir -p \"$2/tuic-design-extension\"\n",
    )
    .unwrap();
    let browser = root.join("chrome-test");
    std::fs::write(&browser, "#!/bin/sh\nprintf '%s\\n' \"$@\" > \"$TUIC_DESIGN_REAP_TEST/args\"\nprintf '%s' \"$$\" > \"$TUIC_DESIGN_REAP_TEST/pid\"\nwhile [ ! -f \"$TUIC_DESIGN_REAP_TEST/release\" ]; do sleep 0.01; done\nexit 7\n").unwrap();
    for path in [&bridge, &browser] {
        std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o755)).unwrap();
    }
    let output = std::process::Command::new(std::env::current_exe().unwrap())
        .args(["--exact", TEST, "--nocapture"])
        .env(ROLE, root)
        .env("HOME", root)
        .env("TUIC_DESIGN_CHROME", browser)
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
}
