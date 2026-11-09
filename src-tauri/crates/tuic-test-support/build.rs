//! Compile `src/identity_probe_main.rs` into `$OUT_DIR/tuic-identity-probe`
//! and export its path as `TUIC_IDENTITY_PROBE_BIN` (read by
//! `tuic_test_support::identity_probe_bin`).
//!
//! A plain `rustc` call, not a Cargo `[[bin]]`: a library crate's binaries are
//! not built for its dependents' tests, and an artifact dependency is still
//! unstable. Everything comes from what Cargo hands a build script (`RUSTC`,
//! `TARGET`, `OUT_DIR`), so it follows `CARGO_TARGET_DIR`, cross targets and
//! whatever toolchain the build itself uses. `RUSTC_WRAPPER` is deliberately
//! skipped: the probe is one small file and a caching wrapper only adds a way
//! for it to fail.

use std::path::PathBuf;
use std::process::Command;

const SOURCE: &str = "src/identity_probe_main.rs";

fn main() {
    println!("cargo:rerun-if-changed={SOURCE}");
    println!("cargo:rerun-if-changed=build.rs");

    let env = |key: &str| std::env::var(key).unwrap_or_else(|_| panic!("cargo sets {key}"));
    let rustc = std::env::var_os("RUSTC").unwrap_or_else(|| "rustc".into());
    let target = env("TARGET");
    let out_dir = PathBuf::from(env("OUT_DIR"));
    let manifest = PathBuf::from(env("CARGO_MANIFEST_DIR"));
    let suffix = if env("CARGO_CFG_TARGET_OS") == "windows" {
        ".exe"
    } else {
        ""
    };
    let output = out_dir.join(format!("tuic-identity-probe{suffix}"));

    let mut command = Command::new(&rustc);
    command
        .arg("--edition=2024")
        .arg("--crate-type=bin")
        .arg("--crate-name=tuic_identity_probe")
        .arg("-O")
        .arg("--target")
        .arg(&target)
        .arg("-o")
        .arg(&output)
        .arg(manifest.join(SOURCE));
    // Keep the probe linkable wherever the crate itself links: a configured
    // linker for this target (`CARGO_TARGET_<TRIPLE>_LINKER`) applies here too.
    let linker_key = format!(
        "CARGO_TARGET_{}_LINKER",
        target.to_uppercase().replace(['-', '.'], "_")
    );
    println!("cargo:rerun-if-env-changed={linker_key}");
    // Cargo hands build scripts the linker it resolved for the target
    // (`RUSTC_LINKER`, which also covers `.cargo/config.toml`'s `target.<triple>.linker`);
    // the environment variable is the fallback for an older Cargo.
    println!("cargo:rerun-if-env-changed=RUSTC_LINKER");
    if let Some(linker) = std::env::var_os("RUSTC_LINKER")
        .filter(|value| !value.is_empty())
        .or_else(|| std::env::var_os(&linker_key))
    {
        command.arg("-C").arg({
            let mut arg = std::ffi::OsString::from("linker=");
            arg.push(linker);
            arg
        });
    }
    let result = command
        .output()
        .unwrap_or_else(|error| panic!("run {} for the identity probe: {error}", rustc.display()));
    assert!(
        result.status.success(),
        "compiling the identity probe ({}) failed:\n{}",
        manifest.join(SOURCE).display(),
        String::from_utf8_lossy(&result.stderr)
    );
    println!(
        "cargo:rustc-env=TUIC_IDENTITY_PROBE_BIN={}",
        output.display()
    );
}
