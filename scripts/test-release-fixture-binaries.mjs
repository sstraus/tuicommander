import assert from "node:assert/strict";
import { execFileSync } from "node:child_process";
import { cpSync, mkdirSync, mkdtempSync, readdirSync, writeFileSync } from "node:fs";
import { homedir } from "node:os";
import { resolve, join } from "node:path";
import { test } from "node:test";

// Exercise the installed Tauri bundler with the actual manifest binary declarations.
// Stale fixture executables deliberately exist in the build output, as after tests.
// Catches: a fixture leaks into a release despite not appearing in externalBin.
test("release app excludes test fixtures even when their binaries already exist", { timeout: 90000 }, () => {
  assert.equal(process.platform, "darwin", "This packaging probe requires macOS");
  const root = resolve(".tmp"); mkdirSync(root, { recursive: true });
  const project = mkdtempSync(join(root, "release-fixture-binaries-"));
  const tauri = join(project, "src-tauri"); mkdirSync(join(tauri, "src"), { recursive: true });
  const manifest = execFileSync(process.env.PYTHON ?? "python3", ["-c", `
import pathlib,tomllib,json
m=tomllib.loads(pathlib.Path('src-tauri/Cargo.toml').read_text())
print(json.dumps({'bins':m['bin'],'features':m['features']}))
`], { encoding: "utf8" });
  const { bins, features } = JSON.parse(manifest);
  let cargo = '[workspace]\n[package]\nname="tuicommander"\nversion="0.0.0"\nedition="2024"\ndefault-run="tuicommander"\n[features]\n';
  for (const [name, enabled] of Object.entries(features)) cargo += `${name}=${JSON.stringify(enabled.filter(f => !f.includes("/") && !f.startsWith("dep:") && features[f]))}\n`;
  for (const bin of bins) {
    cargo += `[[bin]]\nname=${JSON.stringify(bin.name)}\npath=${JSON.stringify(`src/${bin.name}.rs`)}\n`;
    if (bin["required-features"]) cargo += `required-features=${JSON.stringify(bin["required-features"])}\n`;
    writeFileSync(join(tauri, "src", `${bin.name}.rs`), "fn main() {}\n");
  }
  writeFileSync(join(tauri, "src/main.rs"), "fn main() {}\n");
  writeFileSync(join(tauri, "Cargo.toml"), cargo);
  const output = join(tauri, "target/release"); mkdirSync(output, { recursive: true });
  for (const bin of ["tuicommander", ...bins.map(b => b.name)]) cpSync("/usr/bin/true", join(output, bin));
  writeFileSync(join(tauri, "tauri.conf.json"), JSON.stringify({
    productName: "Fixture Bundle", version: "0.0.0", identifier: "invalid.tuic.fixtureprobe",
    build: { frontendDist: "../dist" }, bundle: { active: true, targets: ["app"], createUpdaterArtifacts: false },
  }));
  mkdirSync(join(project, "dist")); writeFileSync(join(project, "dist/index.html"), "fixture");
  const result = execFileSync(resolve("node_modules/.bin/tauri"), ["bundle", "--bundles", "app"], {
    cwd: project, encoding: "utf8", env: { ...process.env, PATH: `${homedir()}/Library/Application Support/mbx/bin:${process.env.PATH}` },
    timeout: 80000, stdio: ["ignore", "pipe", "pipe"],
  });
  const executables = readdirSync(join(output, "bundle/macos/Fixture Bundle.app/Contents/MacOS"));
  const withFixtures = execFileSync(resolve("node_modules/.bin/tauri"), ["bundle", "--bundles", "app", "--features", "test-fixtures"], {
    cwd: project, encoding: "utf8", env: { ...process.env, PATH: `${homedir()}/Library/Application Support/mbx/bin:${process.env.PATH}` },
    timeout: 80000, stdio: ["ignore", "pipe", "pipe"],
  });
  const testExecutables = readdirSync(join(output, "bundle/macos/Fixture Bundle.app/Contents/MacOS"));
  writeFileSync(join(project, "evidence.json"), JSON.stringify({ executables, testExecutables, result, withFixtures }, null, 2));
  assert.ok(testExecutables.includes("tuic-acp-fixture-agent"));
  assert.ok(testExecutables.includes("tuic-mcp-fixture-server"));
  assert.ok(executables.includes("tuicommander"));
  assert.ok(executables.includes("tuic-remote"));
  assert.ok(!executables.includes("tuic-acp-fixture-agent"), JSON.stringify(executables));
  assert.ok(!executables.includes("tuic-mcp-fixture-server"), JSON.stringify(executables));
});
