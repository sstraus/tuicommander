// Fails if Rust source (production OR tests) execs a process-inspection tool —
// `ps`, `pgrep`, `pkill` — or builds a shell command line that runs one,
// outside the allow-listed sites below.
//
// Why: `/bin/ps` is setuid root on macOS and sandboxed hosts refuse to exec
// it; `pgrep`/`pkill` need the `sysmond` mach service, which sandboxes do not
// expose. Every consumer that parsed their output went blind in exactly the
// environment TUIC most needs to see its children, and the test helpers that
// shelled out failed there for reasons unrelated to the code under test. The
// replacement is native: `tuic_core::process_info` (libproc / `kern.proc` on
// macOS, `/proc` on Linux).
//
// Two rules, both on non-comment lines of tracked `.rs` files under
// `src-tauri/src` and `src-tauri/crates` (the vendored `src-tauri/patches` are
// not ours and are not scanned):
//   EXEC  — `Command::new("ps" | "pgrep" | "pkill")`, any path prefix
//           (`"/bin/ps"`), std or tokio, including a constructor split across
//           lines (`Command::new(\n // why\n "ps",\n)`).
//   SHELL — `ps -x` / `pgrep -x` / `pkill -x` inside text: an `sh -c` string,
//           a generated hook script, a test literal; the tool may be named by
//           path (`/bin/ps -eo pid`).
//
// The allow-list is keyed by `file::enclosing fn` (the nearest preceding `fn`
// line), so a new call in a different function of an allow-listed file is
// still caught. Every entry must still match something (`--self-test` checks
// the real tree), so a removed site cannot leave a stale exemption behind.
import { spawnSync } from "node:child_process";
import fs from "node:fs";
import path from "node:path";

const selfTest = process.argv.includes("--self-test");

const REAL_ROOTS = ["src-tauri/src", "src-tauri/crates"];

const TOOLS = "ps|pgrep|pkill";
const EXEC_RE = new RegExp(String.raw`Command::new\(\s*"(?:[^"]*/)?(${TOOLS})"\s*\)`);
// A dash option (`ps -eo`, `pgrep -f`, `pgrep --full`), or one of the BSD-style `ps` clusters with no dash
// (`ps aux`, `ps axo pid`, `ps ww`). Only these spellings: an open letter class would read
// prose such as "check ps or top" ("or" as options) as an exec; "the ps fallback" is no hit.
const SHELL_RE = new RegExp(
	String.raw`(?:^|[\s"'\x60(|;&$])(?:[^\s"'\x60;|&()]*/)?(?:(${TOOLS})\s+--?[A-Za-z]|(ps)\s+(?:aux{1}w{0,2}|axu|ax[wo]{0,2}|a?x?ww)(?=$|[\s"'\x60;|&)\\]))`,
);
// The same constructor when `new(`, the name and `)` sit on different lines. Matched against
// the file text with comment lines blanked, so a comment between them cannot hide it.
const EXEC_MULTILINE_RE = new RegExp(String.raw`Command::new\(\s*"(?:[^"]*/)?(${TOOLS})"\s*,?\s*\)`, "g");
// Per line: does it START inside a block comment or inside a (normal or raw) string literal,
// and what is the line's CODE view: the same text with every comment span removed (a `//` to
// end of line, a `/* ... */` span even when only part of the line) but string contents kept,
// since the shell-text rule reads them. A lightweight Rust lexer: line comments, block
// comments, "..." with escapes, r#"..."#, and char literals (so '"' and lifetimes do not open
// a string). Without it a column-zero `}` inside a raw shell-script string closes the Rust
// function scope early, prose between the block-comment delimiters is read as code, and code
// after a closing `*/` or a command-like word in a trailing `// comment` is misjudged.
function lineStates(lines) {
	const states = [];
	let mode = "code";
	let rawHashes = 0;
	for (const line of lines) {
		const state = { inString: mode === "str" || mode === "raw", inBlock: mode === "block", code: "" };
		states.push(state);
		const keep = (from, to) => {
			state.code += line.slice(from, to);
		};
		let j = 0;
		while (j < line.length) {
			const c = line[j];
			if (mode === "block") {
				if (line.startsWith("*/", j)) {
					mode = "code";
					j += 2;
					state.code += " ";
				} else j++;
			} else if (mode === "str") {
				if (c === "\\") {
					keep(j, j + 2);
					j += 2;
				} else {
					if (c === '"') mode = "code";
					keep(j, j + 1);
					j++;
				}
			} else if (mode === "raw") {
				if (c === '"' && line.startsWith("#".repeat(rawHashes), j + 1)) {
					mode = "code";
					keep(j, j + 1 + rawHashes);
					j += 1 + rawHashes;
				} else {
					keep(j, j + 1);
					j++;
				}
			} else if (line.startsWith("//", j)) {
				break;
			} else if (line.startsWith("/*", j)) {
				mode = "block";
				j += 2;
				state.code += " ";
			} else if (c === '"') {
				mode = "str";
				keep(j, j + 1);
				j++;
			} else if (c === "r" && (j === 0 || !/[A-Za-z0-9_]/.test(line[j - 1])) && /^r#*"/.test(line.slice(j))) {
				const hashes = /^r(#*)"/.exec(line.slice(j))[1].length;
				mode = "raw";
				rawHashes = hashes;
				keep(j, j + 2 + hashes);
				j += 2 + hashes;
			} else if (c === "'") {
				const literal = /^'(?:\\.[^']*|[^\\'])'/.exec(line.slice(j));
				const width = literal ? literal[0].length : 1;
				keep(j, j + width);
				j += width;
			} else {
				keep(j, j + 1);
				j++;
			}
		}
	}
	return states;
}

const FN_RE = /\bfn\s+([A-Za-z0-9_]+)/;
const INDENT_RE = /^(\s*)/;

/** `file::fn` -> why this site may keep its `ps` (and nothing else may). */
const ALLOWED = new Map([
	[
		"src-tauri/src/agent_hook_windows_tests.rs::windows_hooks_call_the_cli_and_keep_the_sentinel",
		"asserts the generated Windows hook command does NOT contain POSIX tokens (\"ps -o\" among them): a string list, never executed",
	],
	[
		"src-tauri/src/pty/screen.rs::ps_process_tree_snapshot",
		"the ONE production exec: logged fallback for the native snapshot when it fails or a sudo subtree's argv is unreadable (remove once the native path has soaked)",
	],
	[
		"src-tauri/src/agent_hook.rs::tty_resolve",
		"headless hook shell text run by the AGENT: its live parent tty wins over an inherited $TUIC_PTY_TTY, which is the fallback when this ps cannot run (sandbox) or the parent is detached; TUIC itself execs nothing",
	],
	[
		"src-tauri/crates/tuic-core/src/process_info.rs::native_snapshot_agrees_with_ps",
		"macOS test: compares the native table against the real /bin/ps where it can run, and reports UNCHECKED where it cannot (the sandbox); it is the oracle the native module replaced",
	],
	[
		"src-tauri/src/agent_hook.rs::hook_command_contains_tty_resolve",
		"test asserts on the generated fallback text; execs nothing",
	],
]);

function listTrackedRustFiles(roots, cwd) {
	const result = spawnSync("git", ["ls-files", "-z", "--", ...roots], { cwd, encoding: "utf8" });
	if (result.status !== 0) {
		throw new Error(`git ls-files failed: ${result.stderr}`);
	}
	return result.stdout.split("\0").filter((file) => file.endsWith(".rs"));
}

/**
 * @param {{path: string, text: string}[]} files
 * @returns {{hits: {site: string, line: number, rule: string, text: string}[]}}
 */
function scan(files) {
	const hits = [];
	for (const { path: file, text } of files) {
		// Open function bodies, innermost last. The tree is rustfmt-formatted,
		// so a body ends at the first `}` line back at its `fn` line's indent:
		// a const, static or test literal after it is no longer "inside" that
		// fn (it used to inherit the last allow-listed name it followed).
		const open = [];
		const lines = text.split("\n");
		const enclosingByLine = [];
		const states = lineStates(lines);
		for (let i = 0; i < lines.length; i++) {
			const line = lines[i];
			// The line with its comment spans removed; blank when it is all comment.
			const code = states[i].code;
			if (code.trim() === "") continue;
			const indent = INDENT_RE.exec(line)[1].length;
			// A line that continues a string literal is text, not Rust: its braces and
			// indentation say nothing about where a function ends.
			while (!states[i].inString && open.length > 0) {
				const top = open[open.length - 1];
				if (indent < top.indent) {
					open.pop();
				} else if (indent === top.indent && /^\s*}/.test(code)) {
					open.pop();
					break;
				} else {
					break;
				}
			}
			const fn = states[i].inString ? null : FN_RE.exec(code);
			const enclosing = fn ? fn[1] : open.length > 0 ? open[open.length - 1].name : "<top level>";
			enclosingByLine[i] = enclosing;
			// A body that stays open past this line: not a `;` declaration, not
			// a one-line `fn f() { .. }`.
			if (fn && !/[;}]\s*$/.test(code)) open.push({ name: fn[1], indent });
			const rule = EXEC_RE.test(code) ? "EXEC" : SHELL_RE.test(code) ? "SHELL" : null;
			if (rule) hits.push({ site: `${file}::${enclosing}`, line: i + 1, rule, text: line.trim() });
		}
		const uncommented = states.map((state) => state.code).join("\n");
		for (const match of uncommented.matchAll(EXEC_MULTILINE_RE)) {
			const start = uncommented.slice(0, match.index).split("\n").length - 1;
			const site = `${file}::${enclosingByLine[start] ?? "<top level>"}`;
			// Already reported by the single-line rule when the whole call sits on one line.
			if (hits.some((hit) => hit.site === site && hit.line === start + 1 && hit.rule === "EXEC")) continue;
			hits.push({ site, line: start + 1, rule: "EXEC", text: lines[start].trim() });
		}
	}
	return { hits };
}

function offendersOf(hits, allowed) {
	return hits.filter((hit) => !allowed.has(hit.site));
}

function readTree(cwd) {
	return listTrackedRustFiles(REAL_ROOTS, cwd)
		.map((file) => ({ file, abs: path.join(cwd, file) }))
		.filter(({ abs }) => fs.existsSync(abs))
		.map(({ file, abs }) => ({ path: file, text: fs.readFileSync(abs, "utf8") }));
}

if (selfTest) {
	const fixtures = [
		{
			path: "a.rs",
			text: 'fn probe() {\n    let out = std::process::Command::new("ps").arg("-ax").output();\n}\n',
		},
		{ path: "b.rs", text: 'fn kids() {\n    tokio::process::Command::new("/usr/bin/pgrep");\n}\n' },
		{ path: "c.rs", text: 'fn shelled() {\n    run("sh", "-c", "ps -eo pid,ppid | awk 1");\n}\n' },
		{ path: "d.rs", text: "fn documented() {\n    // ps -o tty= used to be here\n    let ps = 3; let x = ps - 1;\n}\n" },
		{ path: "e.rs", text: 'fn allowed_site() {\n    Command::new("ps");\n}\nfn other() {\n    Command::new("pkill");\n}\n' },
		// A top-level const after an allow-listed fn is not inside it.
		{
			path: "f.rs",
			text: 'fn allowed_site() {\n    Command::new("ps");\n}\n\nconst SNEAKY: &str = "ps -eo pid";\n',
		},
		// Same for a module-level const after a nested allow-listed fn, and a
		// multi-line signature stays inside its fn.
		{
			path: "g.rs",
			text:
				'mod tests {\n    fn allowed_site(\n        a: u8,\n    ) -> u8 {\n        run("ps -o comm=");\n        a\n    }\n    const LATER: &str = "pgrep -f x";\n}\n',
		},
		// BSD-style letters with no dash, and prose that must not match.
		{ path: "h.rs", text: 'fn bsd() {\n    run("sh", "-c", "ps aux | grep x");\n    log("the ps fallback ran");\n}\n' },
		// A one-line fn and a `;` declaration do not swallow what follows.
		{
			path: "i.rs",
			text: 'trait T {\n    fn allowed_site(&self);\n}\nfn allowed_one() -> u8 { 1 }\nstatic S: &str = "ps -A";\n',
		},
		// A constructor split across lines, with a comment between its parts.
		{
			path: "j.rs",
			text: 'fn split() {\n    std::process::Command::new(\n        // Inspect the process table.\n        "ps",\n    )\n    .output();\n}\n',
		},
		// A tool named by absolute path inside shell text.
		{ path: "k.rs", text: 'fn pathed() {\n    run("sh", "-c", "/bin/ps -eo pid");\n}\n' },
		// A dereference statement is code, not a block-comment line.
		{ path: "l.rs", text: 'fn deref() {\n    *child = Some(std::process::Command::new("ps").spawn()?);\n}\n' },
		// Prose that merely contains `ps or ...` is not an option cluster; a real block comment is skipped.
		// A long option in shell text.
		{ path: "n.rs", text: 'fn longopt() {\n    run("sh", "-c", "pgrep --full worker");\n}\n' },
		// Comment prose on undecorated lines inside a block comment is not code.
		{ path: "o.rs", text: "fn quiet() {\n    /*\n    ps -eo pid\n    */\n}\n" },
		// A column-zero `}` inside a raw string must not end the allow-listed fn early.
		{
			path: "p.rs",
			text:
				'fn allowed_site() {\n    let s = r#"\nif x; then\n}\nps -eo pid\n"#;\n}\nfn later() {\n    Command::new("ps");\n}\n',
		},
		// Code after a block comment closes on the same line is still code.
		{ path: "q.rs", text: 'fn probe() {\n    /* explanation\n    */ Command::new("ps");\n}\n' },
		// Command-like text only in a trailing comment is not a call...
		{ path: "r.rs", text: 'fn trailing() {\n    let n = 1; // Command::new("ps")\n}\n' },
		// ...but a real constructor before a trailing comment is.
		{ path: "s.rs", text: 'fn real_then_comment() {\n    Command::new("ps"); // why\n}\n' },
		{
			path: "m.rs",
			text: 'fn prose() {\n    log::info!("check ps or top");\n    /*\n     * ps -eo pid is documented here\n     */\n}\n',
		},
	];
	const allowed = new Map([
		["e.rs::allowed_site", "fixture"],
		["f.rs::allowed_site", "fixture"],
		["g.rs::allowed_site", "fixture"],
		["i.rs::allowed_site", "fixture"],
		["i.rs::allowed_one", "fixture"],
		["p.rs::allowed_site", "fixture"],
	]);
	const { hits } = scan(fixtures);
	const sites = offendersOf(hits, allowed).map((hit) => `${hit.site}:${hit.rule}`);
	const expected = [
		"a.rs::probe:EXEC",
		"b.rs::kids:EXEC",
		"c.rs::shelled:SHELL",
		"e.rs::other:EXEC",
		"f.rs::<top level>:SHELL",
		"g.rs::<top level>:SHELL",
		"h.rs::bsd:SHELL",
		"i.rs::<top level>:SHELL",
		"j.rs::split:EXEC",
		"k.rs::pathed:SHELL",
		"l.rs::deref:EXEC",
		"n.rs::longopt:SHELL",
		"p.rs::later:EXEC",
		"q.rs::probe:EXEC",
		"s.rs::real_then_comment:EXEC",
	];
	if (JSON.stringify(sites) !== JSON.stringify(expected)) {
		throw new Error(`expected offenders ${JSON.stringify(expected)}, got ${JSON.stringify(sites)}`);
	}
	process.stdout.write(
		"process-exec checker: exec/shell/BSD-style/path-prefixed calls caught, comments, prose and arithmetic ignored, allow-list is per function body\n",
	);

	// The fixture run above would stay green if REAL_ROOTS stopped covering the
	// crates or an allow-list entry went stale; check both against the real tree.
	const real = readTree(process.cwd());
	if (!real.some((file) => file.path.startsWith("src-tauri/crates/"))) {
		throw new Error("REAL_ROOTS found zero Rust files under src-tauri/crates/");
	}
	const realHits = scan(real).hits;
	const stale = [...ALLOWED.keys()].filter((site) => !realHits.some((hit) => hit.site === site));
	if (stale.length > 0) {
		throw new Error(`allow-list entries that no longer match anything (remove them): ${stale.join(", ")}`);
	}
	process.stdout.write(`process-exec checker: all ${ALLOWED.size} allow-list entries still match a real site\n`);
	process.exit(0);
}

const offenders = offendersOf(scan(readTree(process.cwd())).hits, ALLOWED);
if (offenders.length > 0) {
	process.stderr.write("Rust source execs (or scripts) a process-inspection tool outside the allow-list:\n");
	for (const hit of offenders) process.stderr.write(`  ${hit.site} (line ${hit.line}, ${hit.rule}): ${hit.text}\n`);
	process.stderr.write(
		"Use tuic_core::process_info (native libproc / /proc) instead — sandboxed hosts cannot exec ps/pgrep/pkill.\n" +
			"If a site genuinely must keep one, add it to ALLOWED in scripts/check-no-process-exec.mjs with the reason.\n",
	);
	process.exit(1);
}
process.stdout.write("process-exec checker: no ps/pgrep/pkill exec outside the allow-list\n");
