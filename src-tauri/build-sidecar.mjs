#!/usr/bin/env node
// Cross-platform sidecar builder for tuic-bridge.
// Called by `npm run build:sidecar` — works on macOS, Linux, and Windows.
import { execSync } from "child_process";
import { copyFileSync, writeFileSync } from "fs";
import { join, dirname, isAbsolute } from "path";
import { fileURLToPath } from "url";

const __dirname = dirname(fileURLToPath(import.meta.url));
const root = join(__dirname, "..");
const srcTauri = join(root, "src-tauri");

const target = execSync("rustc --print host-tuple", { encoding: "utf-8" }).trim();
const ext = process.platform === "win32" ? ".exe" : "";
const binName = `tuic-bridge${ext}`;
const sidecarName = `tuic-bridge-${target}${ext}`;
const binDir = join(root, "src-tauri", "binaries");

// Respect CARGO_TARGET_DIR — CI uses it on Windows to dodge MAX_PATH
// (target dir forced to C:/t). Cargo resolves relative values against its
// cwd (here src-tauri), absolute values as-is.
const envTargetDir = process.env.CARGO_TARGET_DIR;
const targetDir = envTargetDir
  ? (isAbsolute(envTargetDir) ? envTargetDir : join(srcTauri, envTargetDir))
  : join(srcTauri, "target");

// Touch placeholder so Tauri's build.rs finds it at compile time
writeFileSync(join(binDir, sidecarName), "");

// Build the release binary from the standalone sub-crate
execSync("cargo build --release --package tuic-bridge", {
  cwd: srcTauri,
  stdio: "inherit",
});

// Copy to sidecar location
copyFileSync(
  join(targetDir, "release", binName),
  join(binDir, sidecarName),
);

console.log(`Sidecar built: ${sidecarName}`);
