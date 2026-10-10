import assert from "node:assert/strict";
import { execFileSync } from "node:child_process";
import { copyFileSync, existsSync, linkSync, mkdirSync, mkdtempSync, readFileSync, readdirSync, rmSync, statSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";
import { test } from "node:test";

const script = join(dirname(fileURLToPath(import.meta.url)), "build-sidecar.mjs");

test("sidecar build keeps an existing reader on the old inode", (t) => {
  const fixture = mkdtempSync(join(tmpdir(), "tuic-sidecar-"));
  t.after(() => rmSync(fixture, { recursive: true, force: true }));
  const tauri = join(fixture, "src-tauri");
  const binaries = join(tauri, "binaries");
  const release = join(tauri, "target", "release");
  const tools = join(fixture, "tools");
  mkdirSync(binaries, { recursive: true });
  mkdirSync(release, { recursive: true });
  mkdirSync(tools);
  copyFileSync(script, join(tauri, "build-sidecar.mjs"));
  writeFileSync(join(tools, "rustc"), "#!/bin/sh\necho fixture-target\n", { mode: 0o755 });
  writeFileSync(join(tools, "cargo"), "#!/bin/sh\nexit 0\n", { mode: 0o755 });

  for (const bin of ["tuic", "tuic-bridge"]) {
    const installed = join(binaries, `${bin}-fixture-target`);
    writeFileSync(installed, "old executable", { mode: 0o755 });
    linkSync(installed, join(fixture, `${bin}-running-reader`));
    writeFileSync(join(release, bin), "complete new executable", { mode: 0o755 });
  }

  execFileSync(process.execPath, [join(tauri, "build-sidecar.mjs")], {
    env: { ...process.env, PATH: `${tools}:${process.env.PATH}` },
  });

  for (const bin of ["tuic", "tuic-bridge"]) {
    const installed = join(binaries, `${bin}-fixture-target`);
    const reader = join(fixture, `${bin}-running-reader`);
    assert.equal(readFileSync(reader, "utf8"), "old executable");
    assert.equal(readFileSync(installed, "utf8"), "complete new executable");
    assert.notEqual(statSync(installed).ino, statSync(reader).ino);
  }
  assert.deepEqual(readdirSync(binaries).filter((name) => name.includes(".update.")), []);

  rmSync(join(release, "tuic"));
  assert.throws(() => execFileSync(process.execPath, [join(tauri, "build-sidecar.mjs")], {
    env: { ...process.env, PATH: `${tools}:${process.env.PATH}` },
    stdio: "pipe",
  }));
  assert.equal(readFileSync(join(binaries, "tuic-fixture-target"), "utf8"), "complete new executable");
  assert.deepEqual(readdirSync(binaries).filter((name) => name.includes(".update.")), []);
});

test("sidecar build fails when the release binary is an empty placeholder", (t) => {
  const fixture = mkdtempSync(join(tmpdir(), "tuic-sidecar-"));
  t.after(() => rmSync(fixture, { recursive: true, force: true }));
  const tauri = join(fixture, "src-tauri");
  const release = join(tauri, "target", "release");
  const tools = join(fixture, "tools");
  mkdirSync(join(tauri, "binaries"), { recursive: true });
  mkdirSync(release, { recursive: true });
  mkdirSync(tools);
  copyFileSync(script, join(tauri, "build-sidecar.mjs"));
  writeFileSync(join(tools, "rustc"), "#!/bin/sh\necho fixture-target\n", { mode: 0o755 });
  writeFileSync(join(tools, "cargo"), "#!/bin/sh\nexit 0\n", { mode: 0o755 });
  writeFileSync(join(release, "tuic-bridge"), "complete new executable", { mode: 0o755 });
  writeFileSync(join(release, "tuic"), "", { mode: 0o755 });

  assert.throws(
    () => execFileSync(process.execPath, [join(tauri, "build-sidecar.mjs")], {
      env: { ...process.env, PATH: `${tools}:${process.env.PATH}` },
      stdio: "pipe",
    }),
    (error) => /placeholder/.test(String(error.stderr)),
  );
  assert.equal(existsSync(join(tauri, "binaries", "tuic-fixture-target")), false);
  const bridge = join(tauri, "binaries", "tuic-bridge-fixture-target");
  assert.equal(readFileSync(bridge, "utf8"), "complete new executable");
  assert.notEqual(statSync(bridge).mode & 0o111, 0);
});

test("sidecar_publication_keeps_installed_cli_executable", (t) => {
  const fixture = mkdtempSync(join(tmpdir(), "tuic-sidecar-mode-"));
  t.after(() => rmSync(fixture, { recursive: true, force: true }));
  const tauri = join(fixture, "src-tauri");
  const binaries = join(tauri, "binaries");
  const release = join(tauri, "target", "release");
  const tools = join(fixture, "tools");
  for (const directory of [binaries, release, tools]) mkdirSync(directory, { recursive: true });
  copyFileSync(script, join(tauri, "build-sidecar.mjs"));
  writeFileSync(join(tools, "rustc"), "#!/bin/sh\necho fixture-target\n", { mode: 0o755 });
  writeFileSync(join(tools, "cargo"), "#!/bin/sh\nexit 0\n", { mode: 0o755 });
  writeFileSync(join(release, "tuic-bridge"), "valid bridge", { mode: 0o755 });
  writeFileSync(join(release, "tuic"), "invalid CLI without execute bits", { mode: 0o644 });
  const installed = join(binaries, "tuic-fixture-target");
  writeFileSync(installed, "previous executable CLI", { mode: 0o755 });
  assert.throws(() => execFileSync(process.execPath, [join(tauri, "build-sidecar.mjs")], {
    env: { ...process.env, PATH: `${tools}:${process.env.PATH}` }, stdio: "pipe",
  }), (error) => /placeholder/.test(String(error.stderr)));
  assert.equal(readFileSync(installed, "utf8"), "previous executable CLI");
  assert.notEqual(statSync(installed).mode & 0o111, 0);
  assert.deepEqual(readdirSync(binaries).filter((name) => name.includes(".update.")), []);
});
