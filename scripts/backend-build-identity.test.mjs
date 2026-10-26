import assert from "node:assert/strict";
import { execFileSync } from "node:child_process";
import { copyFileSync, mkdirSync, mkdtempSync, readFileSync, renameSync, rmSync, statSync, utimesSync, writeFileSync } from "node:fs";
import { homedir, tmpdir } from "node:os";
import { basename, dirname, join } from "node:path";
import { after, before, test } from "node:test";
import { backendBuildIdentity } from "./backend-build-identity.mjs";

let root;
let backend;
let inputs;
let binaryTime;
const touch = (file) => utimesSync(file, new Date(binaryTime + 2000), new Date(binaryTime + 2000));
const check = () => backendBuildIdentity(root, backend);
const depInfoPath = (exe) => exe.replace(/\.exe$/i, "") + ".d";

// Cargo, not a handwritten dependency list, provides the external-system fixture.
// This tiny build runs through the caller's mbx shim and build-slot admission.
before(() => {
	root = mkdtempSync(join(tmpdir(), "backend dep-info "));
	const write = (file, content) => {
		mkdirSync(dirname(join(root, file)), { recursive: true });
		writeFileSync(join(root, file), content);
	};
	write("src-tauri/Cargo.toml", '[package]\nname="skew-fixture"\nversion="0.1.0"\nedition="2021"\n[workspace]\n[dependencies]\nlocal-input={path="local-input"}\n');
	write("src-tauri/local-input/Cargo.toml", '[package]\nname="local-input"\nversion="0.1.0"\nedition="2021"\n');
	write("src-tauri/local-input/src/lib.rs", 'pub const VALUE: &str = "local";\n');
	write("src-tauri/src/main.rs", 'fn main() { std::hint::black_box((include_str!("../../plugins/plan/main.js"), include_str!("../tests/fixtures/agent-help/ego.txt"), local_input::VALUE)); println!("{}", std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_millis()); }\n#[cfg(test)]\nmod catalogue_tests;\n');
	write("src-tauri/src/catalogue_tests.rs", '#[test] fn test_only() {}\n');
	write("src-tauri/src/bin/tuic-bridge.rs", 'fn main() { println!("{}", include_str!("../../../browser-extension/design-mode/main.js")); }\n');
	write("plugins/plan/main.js", "compiled plan asset\n");
	write("src-tauri/tests/fixtures/agent-help/ego.txt", "compiled fallback help\n");
	write("browser-extension/design-mode/main.js", "compiled bridge asset\n");
	write("src/frontend.ts", "frontend only\n");
	write("src-tauri/README.md", "not compiled\n");
	const output = execFileSync("cargo", ["build", "--manifest-path", join(root, "src-tauri/Cargo.toml"), "--message-format=json"], { encoding: "utf8", stdio: ["ignore", "pipe", "pipe"], cwd: join(root, "src-tauri"), env: { ...process.env, TMPDIR: join(homedir(), "Gits/.tmp/") } });
	const artifacts = output.trim().split("\n").map((line) => JSON.parse(line)).filter((entry) => entry.reason === "compiler-artifact" && entry.executable);
	const copies = {};
	for (const artifact of artifacts) {
		const source = artifact.executable;
		const destination = join(root, "built", basename(source));
		mkdirSync(dirname(destination), { recursive: true });
		copyFileSync(source, destination);
		const time = statSync(source).mtime;
		utimesSync(destination, time, time);
		const depInfo = source.replace(/\.exe$/, "") + ".d";
		copyFileSync(depInfo, destination.replace(/\.exe$/, "") + ".d");
		copies[artifact.target.name] = destination;
	}
	// Keep the pre-redesign implementation callable for the recorded RED replay.
	execFileSync("git", ["init", "--quiet"], { cwd: root });
	execFileSync("git", ["add", "src-tauri/Cargo.toml", "src-tauri/src", "plugins", "browser-extension"], { cwd: root });
	execFileSync("git", ["-c", "user.name=Test", "-c", "user.email=test@example.invalid", "commit", "--quiet", "-m", "test: record compiler fixture"], { cwd: root });
	const exe = copies["skew-fixture"];
	binaryTime = statSync(exe).mtimeMs;
	const startedAt = Number(execFileSync(exe, { encoding: "utf8" }).trim());
	assert.ok(Number.isFinite(startedAt) && startedAt >= binaryTime);
	backend = { exe, started_at_ms: startedAt, bridge_exe: copies["tuic-bridge"], git_hash: "compiled" };
	inputs = ["plugins/plan/main.js", "src-tauri/tests/fixtures/agent-help/ego.txt", "browser-extension/design-mode/main.js", "src-tauri/local-input/src/lib.rs", "src-tauri/local-input/Cargo.toml", "src-tauri/Cargo.toml", "src-tauri/Cargo.lock"];
	// Verify that these assets really reached the compiler-generated dep-info.
	assert.match(readFileSync(depInfoPath(exe), "utf8"), /plugins\/plan\/main\.js/);
	assert.match(readFileSync(depInfoPath(backend.bridge_exe), "utf8"), /browser-extension\/design-mode\/main\.js/);
});
after(() => { if (root) rmSync(root, { recursive: true, force: true }); });

// Catches: source-path allowlists omit embedded fixture/out-of-tree assets or path crates.
test("warns for every compiler-recorded runtime input and local Cargo manifest or lockfile", () => {
	assert.equal(check().status, "current");
	for (const file of inputs) {
		const path = join(root, file);
		const original = statSync(path);
		touch(path);
		assert.equal(check().status, "stale", `${file} must request a restart`);
		utimesSync(path, original.atime, original.mtime);
	}
});

// Catches: test-only modules, documentation and unrelated frontend saves request restarts.
test("keeps non-compiled frontend, test and README churn quiet", () => {
	for (const file of ["src/frontend.ts", "src-tauri/src/catalogue_tests.rs", "src-tauri/README.md"]) touch(join(root, file));
	assert.equal(check().status, "current");
});

// Catches: a rebuilt executable is treated as the binary still held by the process.
test("warns when the original build executable is replaced after process entry", () => {
	const original = statSync(backend.exe);
	utimesSync(backend.exe, new Date(backend.started_at_ms + 1000), new Date(backend.started_at_ms + 1000));
	assert.equal(check().status, "stale");
	utimesSync(backend.exe, original.atime, original.mtime);
});

// Catches: the macOS copy runner checks its protected copy instead of Cargo's artifact.
test("uses the copy runner original executable binding even when current_exe has no dep-info", () => {
	const result = backendBuildIdentity(root, { ...backend, exe: join(root, "protected-copy"), build_exe: backend.exe });
	assert.equal(result.status, "current");
});

// Catches: missing artifact metadata silently reports a match.
test("reports unavailable for missing executable, dep-info, or pre-feature backend", () => {
	assert.equal(backendBuildIdentity(root).status, "unavailable");
	assert.equal(backendBuildIdentity(root, { ...backend, exe: join(root, "missing") }).status, "unavailable");
	for (const exe of [backend.exe, backend.bridge_exe]) {
		const dep = depInfoPath(exe);
		renameSync(dep, dep + ".saved");
		try { assert.equal(check().status, "unavailable"); }
		finally { renameSync(dep + ".saved", dep); }
	}
});

// Catches: bridge inputs are incorrectly compared to the older main executable.
test("compares bridge source to the bridge build rather than the main build", () => {
	const source = join(root, "browser-extension/design-mode/main.js");
	const oldSource = statSync(source);
	const oldBridge = statSync(backend.bridge_exe);
	try {
		touch(source);
		const rebuilt = new Date(binaryTime + 4000);
		utimesSync(backend.bridge_exe, rebuilt, rebuilt);
		assert.equal(check().status, "current");
	} finally {
		utimesSync(source, oldSource.atime, oldSource.mtime);
		utimesSync(backend.bridge_exe, oldBridge.atime, oldBridge.mtime);
	}
});

// Catches: rebuilding the bridge clears skew even though the copy runner's
// protected bridge still holds the version copied before application entry.
test("warns when the copy runner bridge is rebuilt without refreshing its launch copy", () => {
	const original = statSync(backend.bridge_exe);
	try {
		const rebuilt = new Date(backend.started_at_ms + 1000);
		utimesSync(backend.bridge_exe, rebuilt, rebuilt);
		assert.equal(backendBuildIdentity(root, { ...backend, exe: join(root, "protected-copy"), build_exe: backend.exe }).status, "stale");
	} finally { utimesSync(backend.bridge_exe, original.atime, original.mtime); }
});
