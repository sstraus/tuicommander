#!/usr/bin/env node
// Cross-platform sidecar builder for tuic-bridge and tuic CLI.
// Called by `pnpm build:sidecar` — works on macOS, Linux, and Windows.
// Skips rebuild if the source crate hasn't changed since last build.
import { execSync } from "child_process";
import { copyFileSync, writeFileSync, statSync, existsSync } from "fs";
import { join, dirname, isAbsolute } from "path";
import { fileURLToPath } from "url";

const __dirname = dirname(fileURLToPath(import.meta.url));
const root = join(__dirname, "..");
const srcTauri = join(root, "src-tauri");

const target = execSync("rustc --print host-tuple", { encoding: "utf-8" }).trim();
const ext = process.platform === "win32" ? ".exe" : "";
const binDir = join(root, "src-tauri", "binaries");
const force = process.argv.includes("--force");

// Respect CARGO_TARGET_DIR — CI uses it on Windows to dodge MAX_PATH
const envTargetDir = process.env.CARGO_TARGET_DIR;
const targetDir = envTargetDir
  ? (isAbsolute(envTargetDir) ? envTargetDir : join(srcTauri, envTargetDir))
  : join(srcTauri, "target");

const sidecars = [
  { pkg: "tuic-bridge", bin: "tuic-bridge", crate: "crates/tuic-bridge" },
  { pkg: "tuic-cli", bin: "tuic", crate: "crates/tuic-cli" },
];

for (const { pkg, bin, crate: cratePath } of sidecars) {
  const binName = `${bin}${ext}`;
  const sidecarName = `${bin}-${target}${ext}`;
  const sidecarPath = join(binDir, sidecarName);
  const releaseBin = join(targetDir, "release", binName);

  // Touch placeholder so Tauri's build.rs finds it at compile time
  if (!existsSync(sidecarPath)) {
    writeFileSync(sidecarPath, "");
  }

  // Skip rebuild if the release binary is newer than all source files
  if (!force && existsSync(releaseBin)) {
    const binMtime = statSync(releaseBin).mtimeMs;
    const srcDir = join(srcTauri, cratePath, "src");
    const cargoToml = join(srcTauri, cratePath, "Cargo.toml");
    let needsRebuild = false;

    for (const checkPath of [cargoToml]) {
      if (existsSync(checkPath) && statSync(checkPath).mtimeMs > binMtime) {
        needsRebuild = true;
        break;
      }
    }

    if (!needsRebuild && existsSync(srcDir)) {
      try {
        const srcFiles = execSync(`find "${srcDir}" -name "*.rs" -newer "${releaseBin}"`, {
          encoding: "utf-8",
        }).trim();
        needsRebuild = srcFiles.length > 0;
      } catch {
        needsRebuild = true;
      }
    }

    if (!needsRebuild) {
      // Ensure sidecar is up to date even if we skip the build
      const sidecarSize = existsSync(sidecarPath) ? statSync(sidecarPath).size : 0;
      const releaseSize = statSync(releaseBin).size;
      if (sidecarSize === releaseSize) {
        console.log(`Sidecar up to date: ${sidecarName} (skipped)`);
        continue;
      }
    }
  }

  execSync(`cargo build --release --package ${pkg}`, {
    cwd: srcTauri,
    stdio: "inherit",
  });

  copyFileSync(releaseBin, sidecarPath);

  console.log(`Sidecar built: ${sidecarName}`);
}
