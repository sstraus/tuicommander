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
	fill: string;
	inside: boolean;
};

// Browser geometry is required: happy-dom does not implement SVG getBBox.
describe.skipIf(!process.env.TUIC_TEST_CHROME)("production icon artwork geometry", () => {
	let samples: {
		theme: string;
		data: {
			foreground: string;
			agents: IconMeasurement[];
			globe: {
				size: number;
				height: number;
				openings: { x: number; y: number; filled: boolean }[];
				structure: { x: number; y: number; filled: boolean }[];
			}[];
		};
	}[];
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
		expect(samples.map((sample) => sample.theme)).toEqual(["icons", "icons-light"]);
		for (const { data } of samples) {
			expect(data.agents).toHaveLength(28);
			for (const icon of data.agents) {
				expect.soft(icon.paintHeight / icon.size, JSON.stringify(icon)).toBeGreaterThanOrEqual(0.9);
				expect.soft(icon.paintHeight / icon.size, JSON.stringify(icon)).toBeLessThanOrEqual(1.001);
				expect.soft(icon.paintWidth / icon.size, JSON.stringify(icon)).toBeGreaterThanOrEqual(0.75);
			}
		}
	});
	it("catches malformed globe cutouts filling or unbalancing the mirrored meridian openings", () => {
		for (const { data } of samples) {
			expect(data.globe.map((icon) => icon.size)).toEqual([14, 22, 96]);
			for (const icon of data.globe) {
				expect(icon.height).toBe(icon.size);
				for (const point of icon.openings)
					expect.soft(point.filled, JSON.stringify({ size: icon.size, ...point })).toBe(false);
				for (const point of icon.structure)
					expect.soft(point.filled, JSON.stringify({ size: icon.size, ...point })).toBe(true);
			}
		}
	});
	it("catches clipped artwork or hardcoded identity colors in light and dark themes", () => {
		for (const { data } of samples)
			for (const icon of data.agents) {
				expect.soft(icon.inside, JSON.stringify(icon)).toBe(true);
				expect.soft(icon.fill, JSON.stringify(icon)).toBe(data.foreground);
			}
	});
});
