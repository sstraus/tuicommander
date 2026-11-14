import { describe, expect, it } from "vitest";
import { isSidebarDensityMode, nextSidebarDensityMode, sidebarDensity } from "../utils/sidebarDensity";

describe("sidebar density modes", () => {
	it("cycles rich, compact, harness, rich — catches Auto surviving in the cycle", () => {
		expect(nextSidebarDensityMode("rich")).toBe("compact");
		expect(nextSidebarDensityMode("compact")).toBe("harness");
		expect(nextSidebarDensityMode("harness")).toBe("rich");
	});

	it("uses rich rows for H and the default — catches H inheriting compact rows", () => {
		expect(sidebarDensity()).toBe("rich");
		expect(sidebarDensity("rich")).toBe("rich");
		expect(sidebarDensity("compact")).toBe("compact");
		expect(sidebarDensity("harness")).toBe("rich");
	});

	it("rejects old Auto and unknown modes — catches invalid stored modes bypassing the default", () => {
		for (const mode of ["rich", "compact", "harness"]) expect(isSidebarDensityMode(mode)).toBe(true);
		for (const mode of ["auto", "touch", "roomy", undefined]) expect(isSidebarDensityMode(mode)).toBe(false);
	});
});
