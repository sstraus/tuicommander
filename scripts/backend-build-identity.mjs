import { createHash } from "node:crypto";
import { execFileSync } from "node:child_process";
import { existsSync, readFileSync, statSync } from "node:fs";
import { dirname, isAbsolute, join, relative, resolve, sep } from "node:path";

// Cargo's final <binary>.d lists the local unit graph, unlike an individual
// rustc .d. Parse Make escaping, including paths containing spaces or Windows
// drive letters; the target separator is a colon followed by whitespace.
function dependencies(text) {
	const line = text.replace(/\\\r?\n/g, "").split(/\r?\n/)[0];
	const separator = line.search(/:\s/);
	if (separator < 0) throw new Error("Invalid Cargo dep-info");
	const words = [];
	let word = "";
	const input = line.slice(separator + 1);
	for (let i = 0; i < input.length; i++) {
		const char = input[i];
		if (char === "\\" && /[\s\\#]/.test(input[i + 1] ?? "")) word += input[++i];
		else if (/\s/.test(char)) {
			if (word) words.push(word);
			word = "";
		} else if (char === "$" && input[i + 1] === "$") { word += "$"; i++; }
		else word += char;
	}
	if (word) words.push(word);
	if (!words.length) throw new Error("Empty Cargo dep-info");
	return words;
}

function localPath(root, path) {
	const rel = relative(root, path);
	return rel !== ".." && !rel.startsWith(`..${sep}`) && !isAbsolute(rel);
}

/** Development-only Cargo staleness check; never compiles or restarts anything. */
export function backendBuildIdentity(cwd, backend) {
	const unavailable = { status: "unavailable", key: "unavailable" };
	if (!backend?.exe || !Number.isFinite(backend.started_at_ms)) return unavailable;
	try {
		const root = resolve(cwd);
		const exe = backend.build_exe || backend.exe;
		if (!isAbsolute(exe)) return unavailable;
		const artifacts = [exe, ...(backend.bridge_exe ? [backend.bridge_exe] : [])];
		const key = createHash("sha256");
		let stale = false;
		let newest = 0;
		let count = 0;
		for (const artifact of artifacts) {
			if (!isAbsolute(artifact)) return unavailable;
			const binary = statSync(artifact);
			if (!binary.isFile()) return unavailable;
			// The copy runner snapshots the main executable and its siblings
			// before entry. Rebuilding an original sibling does not refresh
			// the protected copy that the running app still uses.
			const copiedSibling = exe !== backend.exe && dirname(artifact) === dirname(exe);
			if ((artifact === exe || copiedSibling) && binary.mtimeMs > backend.started_at_ms) stale = true;
			key.update(`${artifact}\0${binary.mtimeMs}\0${binary.size}\0`);
			const depInfo = artifact.replace(/\.exe$/i, "") + ".d";
			const inputs = new Set(dependencies(readFileSync(depInfo, "utf8")).map((file) => {
				// Cargo's final binary dep-info uses absolute paths. Fail closed
				// rather than resolve an unexpected relative path against Vite.
				if (!isAbsolute(file)) throw new Error("Relative Cargo dependency");
				return resolve(file);
			}));
			const visited = new Set();
			for (const file of [...inputs]) {
				let dir = dirname(file);
				while (localPath(root, dir) && !visited.has(dir)) {
					visited.add(dir);
					const manifest = join(dir, "Cargo.toml");
					if (existsSync(manifest)) inputs.add(manifest);
					if (dir === root) break;
					dir = dirname(dir);
				}
			}
			inputs.add(join(root, "src-tauri/Cargo.lock"));
			for (const file of [...inputs].sort()) {
				const info = statSync(file);
				key.update(`${file}\0${info.mtimeMs}\0${info.size}\0`);
				newest = Math.max(newest, info.mtimeMs);
				if (info.mtimeMs > binary.mtimeMs) stale = true;
				count++;
			}
		}
		let gitHash = "dev";
		try { gitHash = execFileSync("git", ["rev-parse", "--short", "HEAD"], { cwd, stdio: ["ignore", "pipe", "ignore"] }).toString().trim(); }
		catch { /* A source archive can still have valid Cargo dep-info. */ }
		return {
			status: stale ? "stale" : "current",
			key: key.digest("hex"),
			backend_id: `${backend.git_hash || "dev"}/${Math.trunc(backend.started_at_ms).toString(36)}`,
			tree_id: `${gitHash}/${Math.trunc(newest).toString(36)}`,
			inputs: count,
		};
	} catch {
		// Missing exe, dep-info or inputs can never establish a match.
		return unavailable;
	}
}
