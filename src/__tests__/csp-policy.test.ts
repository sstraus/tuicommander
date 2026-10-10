import { readFileSync } from "node:fs";
import { expect, it } from "vitest";

it("csp_must_stay_wide_open_boss_decision", () => {
	// AGENTS.md, Accepted Security Decisions: the user is the trust boundary.
	// Per-directive restrictions blank URL tabs and break plugin scripts/styles.
	// CI: .github/workflows/csp-policy.yml, "Guard permissive CSP" (every push/PR).
	// Also collected by ci.yml, frontend job, "Tests".
	const {
		app: { security },
	} = JSON.parse(readFileSync("src-tauri/tauri.conf.json", "utf8"));
	const directives = security.csp
		.split(";")
		.map((value: string) => value.trim())
		.filter(Boolean);
	expect(
		directives.map((value: string) => value.split(/\s+/)[0]),
		"AGENTS.md forbids every per-directive CSP restriction, including frame-src",
	).toEqual(["default-src"]);
	const sources = directives[0].split(/\s+/).slice(1);
	expect(sources, "AGENTS.md requires a permissive default-src").toEqual(
		expect.arrayContaining([
			"'self'",
			"'unsafe-inline'",
			"https:",
			"http:",
			"data:",
			"blob:",
			"asset:",
			"http://asset.localhost",
			"ipc:",
			"tauri:",
			"plugin:",
		]),
	);
	expect(
		security.dangerousDisableAssetCspModification,
		"AGENTS.md requires disabling Tauri script/style CSP hash injection",
	).toEqual(expect.arrayContaining(["style-src", "script-src"]));
});
