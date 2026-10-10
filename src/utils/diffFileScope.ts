export type DiffFileScope = "lockfile" | "generated" | "test";

const LOCKFILES = new Set([
	"package-lock.json",
	"npm-shrinkwrap.json",
	"pnpm-lock.yaml",
	"yarn.lock",
	"bun.lock",
	"bun.lockb",
	"Cargo.lock",
	"Gemfile.lock",
	"poetry.lock",
	"uv.lock",
	"Pipfile.lock",
	"composer.lock",
	"go.sum",
	"Package.resolved",
	"Podfile.lock",
	"flake.lock",
]);

/** Built-in patterns for files nobody reviews line by line. */
const GENERATED = [
	/\.min\.(js|css)$/,
	/\.map$/,
	/\.snap$/,
	/\.generated\./,
	/\.pb\.go$/,
	/_pb2(_grpc)?\.py$/,
	/\.d\.ts$/,
];
// build/dist/vendor only at the repo root: a nested scripts/build is hand-written code.
// node_modules and __snapshots__ are generated wherever they sit.
const GENERATED_DIRS = /^(dist|build|vendor)\/|(^|\/)(node_modules|__snapshots__)\//;

const TESTS = [
	/(^|\/)(__tests__|tests?|spec|e2e)\//,
	/\.(test|spec)\.[a-z0-9]+$/i,
	/_test\.(go|rs|py|rb)$/,
	/(^|\/)test_[^/]+\.py$/,
];

/** Translate a gitattributes pattern to a RegExp. A pattern with no slash matches the
 *  basename at any depth; `*` stops at `/`, `**` crosses it. */
export function globToRegExp(pattern: string): RegExp {
	const anchored = pattern.startsWith("/") || pattern.slice(0, -1).includes("/");
	const body = pattern.replace(/^\//, "").replace(/\/$/, "/**");
	let re = "";
	for (let i = 0; i < body.length; i++) {
		const c = body[i];
		if (c === "*" && body[i + 1] === "*") {
			// `**/` is zero or more WHOLE segments, so `**/gen.ts` must not match `xgen.ts`.
			if (body[i + 2] === "/") {
				re += "(?:.*/)?";
				i += 2;
			} else {
				re += ".*";
				i++;
			}
		} else if (c === "*") re += "[^/]*";
		else if (c === "?") re += "[^/]";
		else if (c === "[") {
			// Character class `[abc]`, `[a-z]`, `[!abc]`; an unterminated `[` is a literal.
			const end = body.indexOf("]", i + 2);
			if (end === -1) re += "\\[";
			else {
				const cls = body.slice(i + 1, end);
				re += cls.startsWith("!") ? `[^${cls.slice(1).replaceAll("\\", "\\\\")}]` : `[${cls.replaceAll("\\", "\\\\")}]`;
				i = end;
			}
		} else re += c.replace(/[.+^${}()|[\]\\]/g, "\\$&");
	}
	return new RegExp(anchored ? `^${re}$` : `(^|/)${re}$`);
}

/** Patterns that set `linguist-generated` (set, or `=true`), in file order. A later line that
 *  unsets or resets it (`-linguist-generated`, `!linguist-generated`, `=false`) is kept as
 *  `!pattern` so that, as in gitattributes, the LAST matching line wins. Gitattributes patterns cannot start with `!`, so the prefix is
 *  unambiguous. An unset line with nothing before it to override is dropped. */
export function parseLinguistGenerated(gitattributes: string): string[] {
	const patterns: string[] = [];
	for (const raw of gitattributes.split("\n")) {
		const line = raw.trim();
		if (!line || line.startsWith("#")) continue;
		const [pattern, ...attrs] = line.split(/\s+/);
		// The last token naming the attribute decides for that line.
		const decisive = attrs.filter((a) => /^[-!]?linguist-generated(=.*)?$/.test(a)).pop();
		if (!decisive) continue;
		if (decisive === "linguist-generated" || decisive === "linguist-generated=true") patterns.push(pattern);
		else if (patterns.length > 0) patterns.push(`!${pattern}`);
	}
	return patterns;
}

/** Last matching line wins; `!pattern` entries come from parseLinguistGenerated. */
function isLinguistGenerated(path: string, patterns: string[]): boolean {
	let generated = false;
	for (const p of patterns) {
		const negated = p.startsWith("!");
		if (globToRegExp(negated ? p.slice(1) : p).test(path)) generated = !negated;
	}
	return generated;
}

/** Why a file starts collapsed in the PR diff, or null when it should be read. */
export function classifyDiffFile(path: string, generatedPatterns: string[] = []): DiffFileScope | null {
	const base = path.slice(path.lastIndexOf("/") + 1);
	if (LOCKFILES.has(base)) return "lockfile";
	if (GENERATED_DIRS.test(path) || GENERATED.some((r) => r.test(path))) return "generated";
	if (isLinguistGenerated(path, generatedPatterns)) return "generated";
	if (TESTS.some((r) => r.test(path))) return "test";
	return null;
}
