// Use a real, small native executable as opaque installation bytes. The full
// libtest runner can exceed 500 MB; copying and hashing it does not add coverage.
// On Unix that is the test-support identity probe, not a copy of `/bin/sh`: the
// agent sandbox SIGKILLs a relocated Apple platform binary on exec.
pub(super) fn copy_native_executable(destination: &std::path::Path) {
    #[cfg(unix)]
    {
        let dir = destination.parent().expect("destination has a parent");
        let name = destination
            .file_name()
            .and_then(|name| name.to_str())
            .expect("destination has a UTF-8 file name");
        tuic_test_support::install_identity_probe(dir, name);
    }
    #[cfg(windows)]
    {
        let source = std::path::PathBuf::from(
            std::env::var_os("SystemRoot").expect("Windows must expose SystemRoot"),
        )
        .join("System32/cmd.exe");
        std::fs::copy(source, destination).unwrap();
    }
}

pub(super) fn assert_runs(executable: &std::path::Path) {
    let mut command = std::process::Command::new(executable);
    #[cfg(unix)]
    command.args(["--print", "tuic-native-fixture"]);
    #[cfg(windows)]
    command.args([
        crate::test_support::host_shell().1,
        "echo tuic-native-fixture",
    ]);
    let output = command.output().unwrap();
    assert!(
        output.status.success(),
        "native fixture exited {}: {}",
        output.status,
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(
        String::from_utf8_lossy(&output.stdout).trim(),
        "tuic-native-fixture"
    );
}
