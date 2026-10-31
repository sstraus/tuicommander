// @vitest-environment node
import { spawnSync } from "node:child_process";
import { readFileSync } from "node:fs";
import { resolve } from "node:path";
import { beforeAll, describe, expect, it } from "vitest";

type IconMeasurement = {
	agent: string;
	size: number;
	paintWidth: number;
	paintHeight: number;
};

// Browser geometry is required: happy-dom does not implement SVG getBBox.
describe.skipIf(!process.env.TUIC_TEST_CHROME)("production icon artwork geometry", () => {
	let samples: { agents: IconMeasurement[] };
	beforeAll(() => {
		const directory = resolve(process.env.TUIC_ICON_EVIDENCE ?? ".tmp/visual-icon-geometry");
		const result = spawnSync(process.execPath, ["src/__tests__/browser/visualIconGeometry.mjs"], {
			encoding: "utf8",
			timeout: 60000,
			env: { ...process.env, TUIC_ICON_EVIDENCE: directory },
		});
		if (result.status !== 0) throw new Error(result.stderr || String(result.error));
		samples = JSON.parse(
			readFileSync(resolve(directory, `${process.env.TUIC_ICON_PHASE ?? "after"}-metrics.json`), "utf8"),
		);
	}, 65000);

	it("catches padded favicon artboards shrinking Droid and OpenCode beside sibling identities", () => {
		expect(samples.agents).toHaveLength(28);
		for (const icon of samples.agents) {
			expect.soft(icon.paintHeight / icon.size, JSON.stringify(icon)).toBeGreaterThanOrEqual(0.9);
			expect.soft(icon.paintHeight / icon.size, JSON.stringify(icon)).toBeLessThanOrEqual(1.001);
			expect.soft(icon.paintWidth / icon.size, JSON.stringify(icon)).toBeGreaterThanOrEqual(0.75);
		}
	});
});
