#!/usr/bin/env node
// Cross-platform sidecar builder for tuic-bridge.
// Called by `npm run build:sidecar` — works on macOS, Linux, and Windows.
import { execSync } from "child_process";
import { copyFileSync, writeFileSync } from "fs";
import { join, dirname } from "path";
import { fileURLToPath } from "url";

const __dirname = dirname(fileURLToPath(import.meta.url));
const root = join(__dirname, "..");

const target = execSync("rustc --print host-tuple", { encoding: "utf-8" }).trim();
const ext = process.platform === "win32" ? ".exe" : "";
const binName = `tuic-bridge${ext}`;
const sidecarName = `tuic-bridge-${target}${ext}`;
const binDir = join(root, "src-tauri", "binaries");

// Touch placeholder so Tauri's build.rs finds it at compile time
writeFileSync(join(binDir, sidecarName), "");

// Build the release binary
execSync("cargo build --release --bin tuic-bridge", {
  cwd: join(root, "src-tauri"),
  stdio: "inherit",
});

// Copy to sidecar location
copyFileSync(
  join(root, "src-tauri", "target", "release", binName),
  join(binDir, sidecarName),
);

console.log(`Sidecar built: ${sidecarName}`);
