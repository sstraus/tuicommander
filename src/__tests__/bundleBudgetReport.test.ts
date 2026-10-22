import { spawnSync } from "node:child_process";
import { randomBytes } from "node:crypto";
import { mkdirSync, mkdtempSync, rmSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { afterEach, describe, expect, it } from "vitest";
// @ts-expect-error -- plain .mjs build script, no type declarations
import { budgetStatus, WARN_FRACTION } from "../../scripts/report-frontend-bundles.mjs";

const SCRIPT = join(process.cwd(), "scripts/report-frontend-bundles.mjs");
const MOBILE_CAP = 100 * 1024;

describe("budgetStatus", () => {
	it("warns from 98% of the cap and fails only above the cap", () => {
		expect(WARN_FRACTION).toBe(0.98);
		expect(budgetStatus(Math.floor(MOBILE_CAP * 0.98) - 1, MOBILE_CAP)).toBe("ok");
		expect(budgetStatus(Math.floor(MOBILE_CAP * 0.98), MOBILE_CAP)).toBe("warn");
		expect(budgetStatus(MOBILE_CAP, MOBILE_CAP)).toBe("warn");
		expect(budgetStatus(MOBILE_CAP + 1, MOBILE_CAP)).toBe("over");
	});
});

describe("report-frontend-bundles.mjs --check", () => {
	let dir: string | undefined;
	afterEach(() => {
		if (dir) rmSync(dir, { recursive: true, force: true });
		dir = undefined;
	});

	/** A fake `dist/` whose mobile entry gzips to roughly `mobileRawBytes` (random bytes don't compress). */
	function run(mobileRawBytes: number) {
		dir = mkdtempSync(join(tmpdir(), "bundle-report-"));
		mkdirSync(join(dir, "dist/assets"), { recursive: true });
		writeFileSync(join(dir, "dist/assets/index.js"), "export {};\n");
		writeFileSync(join(dir, "dist/assets/mobile.js"), randomBytes(mobileRawBytes));
		writeFileSync(join(dir, "dist/index.html"), '<script src="/assets/index.js"></script>');
		writeFileSync(join(dir, "dist/mobile.html"), '<script src="/assets/mobile.js"></script>');
		return spawnSync(process.execPath, [SCRIPT, "--check"], { cwd: dir, encoding: "utf8" });
	}

	it("passes quietly well under the cap", () => {
		const r = run(50_000);
		expect(r.status).toBe(0);
		expect(r.stderr).not.toMatch(/WARNING/);
	});

	it("passes with a WARNING between 98% and 100% of the cap", () => {
		const r = run(101_000);
		expect(r.status).toBe(0);
		expect(r.stderr).toMatch(/WARNING dist\/mobile\.html: .* over 98% of the 102400-byte budget/);
	});

	it("fails over the hard cap", () => {
		const r = run(103_000);
		expect(r.status).toBe(1);
		expect(r.stderr).toMatch(/exceeds the 102400-byte budget/);
	});
});
