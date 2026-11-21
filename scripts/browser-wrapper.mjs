// The browser-stealth wrapper (ab-stealth.sh) the live browser checks drive:
// TUIC_BROWSER_WRAPPER, else the first executable ab-stealth.sh on PATH. It is
// maintainer tooling outside this repo, so there is no home-relative default.
import { accessSync, constants } from "node:fs";
import { delimiter, join } from "node:path";

const NAME = "ab-stealth.sh";

export function browserWrapper(env = process.env) {
	const explicit = env.TUIC_BROWSER_WRAPPER;
	const candidates = explicit ? [explicit] : (env.PATH ?? "").split(delimiter).filter(Boolean).map((dir) => join(dir, NAME));
	for (const candidate of candidates) {
		try {
			accessSync(candidate, constants.X_OK);
			return candidate;
		} catch {}
	}
	throw new Error(
		explicit
			? `TUIC_BROWSER_WRAPPER is not executable: ${explicit}`
			: `${NAME} not found on PATH; set TUIC_BROWSER_WRAPPER to the browser-stealth wrapper`,
	);
}
