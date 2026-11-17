// Fails if code or tooling writes, creates or binds something under a
// hard-coded system temp directory: `/tmp`, `/var/tmp`, `/private/tmp`,
// `/dev/shm`.
//
// Why: every scratch location must derive from the caller's TMPDIR
// (`std::env::temp_dir()`, `os.tmpdir()`, `${TMPDIR:-/tmp}`). A literal `/tmp`
// is shared by every local user (a predictable name there is a swap/read
// target), is denied inside agent sandboxes, and silently ignores the temp dir
// the caller chose. See plans/main-review/tmp-dir-cleanup-assessment.md.
//
// Scope: code and tooling — src-tauri/ (Rust, its scripts and nextest config;
// not the vendored patches/), src/, scripts/, tools/, .github/, .claude/
// (hooks, and the fenced code blocks of SKILL.md files: they are commands
// agents run), tests/terminal-stress/, website/public/*.sh, Makefile,
// package.json, vite/vitest configs. Other docs are not scanned. Full-line
// comments are ignored, and so is the std fallback spelling `${TMPDIR:-/tmp}`.
//
// Every remaining mention of a temp-dir literal is one of:
//   WRITE   — write-shaped on its line (tempdir_in/create_dir/File::create/
//             fs::write/OpenOptions/bind/cwd/current_dir/set_var("TMPDIR"),
//             mktemp/mkdir/touch/`>`/tee/-o/cd/`X=/tmp` in shell,
//             mkdtemp/writeFile/mkdir/open in JS/Python). Always fails unless
//             its `file::enclosing fn` is in ALLOWED.
//   MENTION — anything else (a fixture string, a classifier, a protocol
//             allow-list). Allowed by an ALLOWED `file::fn` entry, or counted
//             per file against scripts/hardcoded-tmp-baseline.json, a
//             shrink-only ratchet: a count above its baseline fails, and so
//             does a count below it (lower the baseline in the same change,
//             `--write-baseline` does it and refuses to raise anything).
//
import { spawnSync } from "node:child_process";
import fs from "node:fs";
import os from "node:os";
import path from "node:path";

const args = process.argv.slice(2);
const selfTest = args.includes("--self-test");
const writeBaseline = args.includes("--write-baseline");
const BASELINE_FILE = "scripts/hardcoded-tmp-baseline.json";

const REAL_ROOTS = [
	"src-tauri/src",
	"src-tauri/crates",
	"src-tauri/tests",
	"src-tauri/scripts",
	"src-tauri/.config",
	"src-tauri/build.rs",
	"src-tauri/build_sidecars.rs",
	"src-tauri/build-sidecar.mjs",
	"src",
	"scripts",
	"tools",
	".github",
	".claude",
	"tests/terminal-stress",
	"website/public",
	"Makefile",
	"package.json",
	"vite.config.ts",
	"vitest.config.ts",
];

const SKIP = [
	/^src-tauri\/(patches|icons|binaries|gen)\//,
	/\.(tcap|raw|png|jpe?g|gif|svg|icns|ico|wav|mp3|ogg|bin|lock|pdf|ttf|woff2?|html|css|txt|jsonl)$/i,
	/(^|\/)\.claude\/(hook-debug\.log.*|settings.*\.json|worktrees\/)/,
	/^scripts\/check-no-hardcoded-tmp\.mjs$/, // this guard's own fixtures
	/^scripts\/hardcoded-tmp-baseline\.json$/,
];

/** `file::fn` -> why this site may keep a hard-coded temp-dir literal. */
const ALLOWED = new Map([
	// Production: read-only uses of the literal (classify, refuse, delete
	// client files) — nothing is written, created or bound there by us.
	["src-tauri/crates/tuic-core/src/temp_paths.rs::recognized_temp_roots", "classifies ghost repo rows already in users' repositories.json (read-only)"],
	["src-tauri/src/agent_mcp.rs::bridge_location_is_stable", "refuses to register a bridge exe living under a temp root (read-only)"],
	["src-tauri/crates/tuic-cli/src/main.rs::disposable_roots", "refuses to register temp dirs as repos (read-only)"],
	["src-tauri/src/tunnels/agent.rs::launchd_socket", "macOS SSH_AUTH_SOCK discovery glob (connect only)"],
	// Tests that assert a path is NOT under a shared temp dir.
	[
		"src-tauri/crates/tuic-test-support/tests/socket_root.rs::socket_root_is_a_private_marked_dir_under_the_host_temp_dir_never_tmp",
		"asserts the socket root is under none of them",
	],
	["scripts/test-run-remote-fixture.sh::no_tmp_mention", "asserts the launcher's output never names /tmp"],
]);

const TMP_LITERAL = /(?<![\w.~$}-])(?:\/private)?(?:\/var)?\/tmp(?![\w-])|\/dev\/shm(?![\w-])/;
const STD_FALLBACK = /\$\{TMPDIR:?-\/tmp\/?\}/g;

const RUST_WRITE =
	/tempdir_in\(|tempfile_in\(|create_dir|File::create|fs::write|OpenOptions|\bbind\(|\.cwd\(|current_dir\(|set_var\(\s*"(?:TMPDIR|TMP|TEMP)"|\bsymlink\(|\.persist\(/;
const SHELL_WRITE =
	/\bmktemp\b|\bmkdir\b|\btouch\b|\btee\b|\bcd\s+["']?(?:\/private)?(?:\/var)?\/tmp|>>?\s*["']?(?:\/private)?(?:\/var)?\/tmp|\s-o\s+["']?(?:\/private)?(?:\/var)?\/tmp|(?:^|[\s;(])(?:export\s+)?[A-Za-z_][A-Za-z0-9_]*=["']?(?:\/private)?(?:\/var)?\/tmp|\b(?:cp|mv|ln)\s/;
const JS_WRITE =
	/mkdtemp|writeFile|appendFile|mkdirSync|\bmkdir\(|createWriteStream|openSync|\.listen\(|\bmktemp\b|(?:TMPDIR|TMP|TEMP)\s*[:=]\s*["'`]\/tmp/;
const PY_WRITE = /mkdtemp|mkdir|\bopen\(|write_text|write_bytes|TemporaryDirectory\(\s*dir|environ\[["']TMPDIR/;

function languageOf(file) {
	if (file.endsWith(".rs")) return "rust";
	if (/\.(mjs|cjs|js|ts|tsx|jsx)$/.test(file)) return "js";
	if (file.endsWith(".py")) return "py";
	if (/(^|\/)SKILL\.md$/.test(file)) return "skill";
	if (/\.(md|markdown)$/i.test(file)) return null;
	return "shell"; // sh/bash/ps1, Makefile, yml, json, toml, extension-less scripts
}

const WRITE_BY_LANGUAGE = { rust: RUST_WRITE, js: JS_WRITE, py: PY_WRITE, shell: SHELL_WRITE, skill: SHELL_WRITE };

const COMMENT_BY_LANGUAGE = {
	rust: /^\s*(\/\/|\/\*|\*)/,
	js: /^\s*(\/\/|\/\*|\*)/,
	py: /^\s*#/,
	shell: /^\s*(#|\/\/|<#)/,
	skill: /^\s*#/,
};

const FN_BY_LANGUAGE = {
	rust: /\bfn\s+([A-Za-z0-9_]+)/,
	js: /\bfunction\s*\*?\s*([A-Za-z0-9_$]+)|\b(?:const|let)\s+([A-Za-z0-9_$]+)\s*=\s*(?:async\s*)?(?:function|\([^)]*\)\s*=>|[A-Za-z0-9_$]+\s*=>)/,
	py: /^\s*(?:async\s+)?def\s+([A-Za-z0-9_]+)/,
	shell: /^\s*(?:function\s+)?([A-Za-z_][A-Za-z0-9_-]*)\s*\(\)\s*\{?|^([A-Za-z0-9_.-]+):(?!=)/,
	skill: /^#+\s+(.*\S)/,
};

function listTrackedFiles(roots, cwd) {
	const result = spawnSync("git", ["ls-files", "-z", "--", ...roots], { cwd, encoding: "utf8" });
	if (result.status !== 0) throw new Error(`git ls-files failed: ${result.stderr}`);
	return result.stdout.split("\0").filter(Boolean);
}

/** @returns {{file: string, site: string, line: number, rule: "WRITE"|"MENTION", text: string}[]} */
function scanFile(file, text) {
	const language = languageOf(file);
	if (!language) return [];
	const hits = [];
	let enclosing = "<top level>";
	let inFence = false;
	const lines = text.split("\n");
	for (let i = 0; i < lines.length; i++) {
		const raw = lines[i];
		if (language === "skill") {
			if (/^\s*(```|~~~)/.test(raw)) {
				inFence = !inFence;
				continue;
			}
			const heading = FN_BY_LANGUAGE.skill.exec(raw);
			if (!inFence && heading) enclosing = heading[1];
			if (!inFence) continue;
		} else {
			const fn = FN_BY_LANGUAGE[language].exec(raw);
			if (fn) enclosing = fn.slice(1).find(Boolean);
		}
		if (COMMENT_BY_LANGUAGE[language].test(raw)) continue;
		const line = raw.replace(STD_FALLBACK, "${TMPDIR}");
		if (!TMP_LITERAL.test(line)) continue;
		const rule = WRITE_BY_LANGUAGE[language].test(line) ? "WRITE" : "MENTION";
		hits.push({ file, site: `${file}::${enclosing}`, line: i + 1, rule, text: raw.trim() });
	}
	return hits;
}

function shouldScan(file) {
	return !SKIP.some((re) => re.test(file));
}

function readTree(roots, cwd) {
	const files = [];
	for (const file of listTrackedFiles(roots, cwd)) {
		if (!shouldScan(file)) continue;
		const abs = path.join(cwd, file);
		if (!fs.existsSync(abs) || !fs.statSync(abs).isFile()) continue;
		const buf = fs.readFileSync(abs);
		if (buf.includes(0)) continue; // binary
		files.push({ file, text: buf.toString("utf8") });
	}
	return files;
}

/**
 * @returns {{writes: object[], counts: Map<string, number>, hits: object[]}}
 */
function classify(files, allowed) {
	const hits = files.flatMap(({ file, text }) => scanFile(file, text));
	const writes = [];
	const counts = new Map();
	for (const hit of hits) {
		if (allowed.has(hit.site)) continue;
		if (hit.rule === "WRITE") writes.push(hit);
		else counts.set(hit.file, (counts.get(hit.file) ?? 0) + 1);
	}
	return { writes, counts, hits };
}

/** @returns {string[]} problems */
function check(files, allowed, baseline) {
	const { writes, counts, hits } = classify(files, allowed);
	const problems = [];
	for (const hit of writes) {
		problems.push(`${hit.file}:${hit.line}  WRITE to a hard-coded temp dir (${hit.site}): ${hit.text}`);
	}
	for (const [file, count] of counts) {
		const allowedCount = baseline[file] ?? 0;
		if (count > allowedCount) {
			const lines = hits
				.filter((hit) => hit.file === file && hit.rule === "MENTION" && !allowed.has(hit.site))
				.map((hit) => `      ${hit.line}: ${hit.text}`)
				.join("\n");
			problems.push(`${file}: ${count} hard-coded temp-dir literal(s), baseline allows ${allowedCount}\n${lines}`);
		}
	}
	for (const [file, allowedCount] of Object.entries(baseline)) {
		const count = counts.get(file) ?? 0;
		if (count < allowedCount) {
			problems.push(`${file}: baseline ${allowedCount} is stale (now ${count}); lower it in ${BASELINE_FILE}`);
		}
	}
	for (const [site, reason] of allowed) {
		if (!hits.some((hit) => hit.site === site)) {
			problems.push(`allow-list entry ${site} no longer matches anything; remove it (${reason})`);
		}
	}
	return problems;
}

function readBaseline(cwd) {
	const abs = path.join(cwd, BASELINE_FILE);
	if (!fs.existsSync(abs)) return null;
	const parsed = JSON.parse(fs.readFileSync(abs, "utf8"));
	return parsed.counts ?? {};
}

function sortedObject(map) {
	return Object.fromEntries([...map].sort(([a], [b]) => a.localeCompare(b)));
}

if (selfTest) {
	const fixtureRoot = fs.mkdtempSync(path.join(os.tmpdir(), "tuic-hardcoded-tmp-checker-"));
	const git = (...gitArgs) => spawnSync("git", gitArgs, { cwd: fixtureRoot, encoding: "utf8" });
	try {
		git("init", "-q");
		const writes = {
			"w1.rs": 'fn scratch() {\n    let d = tempfile::Builder::new().tempdir_in("/tmp").unwrap();\n}\n',
			"w2.rs": 'fn plant() {\n    std::fs::write(Path::new("/tmp").join("x"), b"y").unwrap();\n}\n',
			"w3.rs": 'fn sock() {\n    UnixListener::bind("/private/tmp/s.sock").unwrap();\n}\n',
			"w4.sh": 'f() {\n  TMP=$(mktemp /tmp/tuicommander-XXXXXX.deb)\n}\n',
			"w5.sh": 'make dev > /tmp/dev.log 2>&1\n',
			"w6.mjs": 'const d = fs.mkdtempSync("/tmp/x-");\n',
			"w7.py": 'with open("/var/tmp/out.txt", "w") as f:\n    pass\n',
			"w8.sh": 'export HOME_DIR=/tmp/stress-home\n',
			"SKILL.md": "Never use a literal /tmp in prose here.\n\n```bash\ngit diff > /tmp/review.txt\n```\n",
		};
		const clean = {
			"ok.rs": 'fn fixture() {\n    // writes nothing to /tmp\n    insert_session(&state, "/tmp");\n}\n',
			"ok.sh": 'work=$(mktemp -d "${TMPDIR:-/tmp}/t.XXXXXX")\n',
			"ok.mjs": 'const d = fs.mkdtempSync(path.join(os.tmpdir(), "x-"));\n',
			"NOTES.md": "Old layout: `mktemp /tmp/x` and `> /tmp/log`.\n",
			"allowed.rs": 'fn temp_roots() {\n    std::fs::create_dir_all("/dev/shm/x");\n}\n',
		};
		for (const [name, text] of Object.entries({ ...writes, ...clean })) {
			fs.writeFileSync(path.join(fixtureRoot, name), text);
		}
		git("add", "-A");
		const files = readTree(["."], fixtureRoot);
		const allowed = new Map([["allowed.rs::temp_roots", "fixture"]]);

		// 1. Every write shape is caught; mentions within the baseline pass.
		const problems = check(files, allowed, { "ok.rs": 1 });
		const writeFiles = [...new Set(problems.filter((p) => p.includes("WRITE")).map((p) => p.split(":")[0]))].sort();
		const expected = Object.keys(writes).sort();
		if (JSON.stringify(writeFiles) !== JSON.stringify(expected) || problems.length !== expected.length) {
			throw new Error(`expected exactly the WRITE fixtures ${JSON.stringify(expected)}, got ${JSON.stringify(problems, null, 1)}`);
		}
		// 2. A fixture string grows past its baseline.
		const grown = check(files, allowed, { "ok.rs": 0 }).filter((p) => !p.includes("WRITE"));
		if (!(grown.length === 1 && grown[0].startsWith("ok.rs: 1 hard-coded"))) {
			throw new Error(`a count above the baseline was not caught: ${JSON.stringify(grown)}`);
		}
		// 3. A stale baseline count and a stale allow-list entry.
		const stale = check(files, new Map([...allowed, ["gone.rs::nothing", "fixture"]]), { "ok.rs": 2, "gone.rs": 1 }).filter(
			(p) => !p.includes("WRITE"),
		);
		const staleOk =
			stale.length === 3 &&
			stale.some((p) => p.startsWith("ok.rs: baseline 2 is stale")) &&
			stale.some((p) => p.startsWith("gone.rs: baseline 1 is stale")) &&
			stale.some((p) => p.startsWith("allow-list entry gone.rs::nothing"));
		if (!staleOk) throw new Error(`stale baseline/allow-list not caught: ${JSON.stringify(stale)}`);
		process.stdout.write(
			`hardcoded-tmp checker: ${expected.length} write shapes caught; growth, stale counts and stale allow-list entries caught; fixture strings within the baseline pass\n`,
		);

		// The fixtures cannot see a shrunken scope or a stale real allow-list.
		const real = listTrackedFiles(REAL_ROOTS, process.cwd());
		for (const required of [
			"website/public/install.sh",
			"src-tauri/crates/tuic-test-support/src/lib.rs",
			"tests/terminal-stress/SKILL.md",
			"scripts/with-test-tmp.sh",
			"Makefile",
		]) {
			if (!real.includes(required)) throw new Error(`REAL_ROOTS no longer covers ${required}`);
		}
		const realProblems = check(readTree(REAL_ROOTS, process.cwd()), ALLOWED, readBaseline(process.cwd()) ?? {});
		const staleReal = realProblems.filter((p) => p.startsWith("allow-list entry"));
		if (staleReal.length > 0) throw new Error(staleReal.join("\n"));
		process.stdout.write(`hardcoded-tmp checker: REAL_ROOTS covers ${real.length} tracked file(s); ${ALLOWED.size} allow-list entries are live\n`);
	} finally {
		fs.rmSync(fixtureRoot, { recursive: true, force: true });
	}
	process.exit(0);
}

const cwd = process.cwd();
const files = readTree(REAL_ROOTS, cwd);

if (args.includes("--report")) {
	for (const hit of files.flatMap(({ file, text }) => scanFile(file, text))) {
		process.stdout.write(`${hit.rule}\t${hit.site}\t${hit.line}\t${hit.text}\n`);
	}
	process.exit(0);
}

if (writeBaseline) {
	const previous = readBaseline(cwd);
	const { counts } = classify(files, ALLOWED);
	if (previous) {
		const grown = [...counts].filter(([file, count]) => count > (previous[file] ?? 0));
		if (grown.length > 0) {
			process.stderr.write(
				`Refusing to raise the baseline (new hard-coded temp-dir literals): ${grown.map(([f, c]) => `${f} ${previous[f] ?? 0}->${c}`).join(", ")}\n` +
					"Use os.tmpdir()/std::env::temp_dir()/a fake root instead, or edit the baseline by hand with a reason in the commit.\n",
			);
			process.exit(1);
		}
	}
	const body = {
		comment:
			"Shrink-only per-file count of hard-coded /tmp-family literals that are NOT write-shaped (fixture strings, classifier inputs). Maintained by scripts/check-no-hardcoded-tmp.mjs --write-baseline, which refuses to raise a count.",
		counts: sortedObject(counts),
	};
	fs.writeFileSync(path.join(cwd, BASELINE_FILE), `${JSON.stringify(body, null, "\t")}\n`);
	process.stdout.write(`hardcoded-tmp checker: wrote ${BASELINE_FILE} (${counts.size} file(s))\n`);
	process.exit(0);
}

const baseline = readBaseline(cwd);
if (!baseline) {
	process.stderr.write(`${BASELINE_FILE} is missing; run node scripts/check-no-hardcoded-tmp.mjs --write-baseline\n`);
	process.exit(1);
}
const problems = check(files, ALLOWED, baseline);
if (problems.length > 0) {
	process.stderr.write("Hard-coded system temp dirs in code or tooling:\n");
	for (const problem of problems) process.stderr.write(`  ${problem}\n`);
	process.stderr.write(
		"Derive scratch from the caller's temp dir instead: std::env::temp_dir()/tempfile, os.tmpdir(), \"${TMPDIR:-/tmp}\", the test roots (TUIC_TEST_TMP_ROOT).\n",
	);
	process.exit(1);
}
process.stdout.write("hardcoded-tmp checker: no hard-coded temp-dir writes; literal mentions within the baseline\n");
