import { describe, expect, it } from "vitest";
import { audiencePasses, levelPassesThreshold } from "../../components/ErrorLogPanel/ErrorLogPanel";
import type { AppLogLevel } from "../../stores/appLogger";

const ALL_LEVELS: AppLogLevel[] = ["debug", "info", "warn", "error"];

describe("levelPassesThreshold", () => {
	it("shows every level when the filter is 'all'", () => {
		for (const level of ALL_LEVELS) {
			expect(levelPassesThreshold(level, "all")).toBe(true);
		}
	});

	it("shows the selected level and everything more severe", () => {
		// "warn" intermingles warn + error, hides debug + info
		expect(levelPassesThreshold("error", "warn")).toBe(true);
		expect(levelPassesThreshold("warn", "warn")).toBe(true);
		expect(levelPassesThreshold("info", "warn")).toBe(false);
		expect(levelPassesThreshold("debug", "warn")).toBe(false);
	});

	it("shows only errors at the highest threshold", () => {
		expect(levelPassesThreshold("error", "error")).toBe(true);
		expect(levelPassesThreshold("warn", "error")).toBe(false);
		expect(levelPassesThreshold("info", "error")).toBe(false);
		expect(levelPassesThreshold("debug", "error")).toBe(false);
	});

	it("shows all levels at the lowest threshold (debug)", () => {
		for (const level of ALL_LEVELS) {
			expect(levelPassesThreshold(level, "debug")).toBe(true);
		}
	});
});

describe("audiencePasses", () => {
	it("default 'user' tab shows user entries and untagged (undefined) entries", () => {
		expect(audiencePasses("user", "user")).toBe(true);
		expect(audiencePasses(undefined, "user")).toBe(true); // legacy entries default to user
	});

	it("default 'user' tab hides diagnostic telemetry", () => {
		expect(audiencePasses("diagnostic", "user")).toBe(false);
	});

	it("'diagnostic' tab shows only telemetry", () => {
		expect(audiencePasses("diagnostic", "diagnostic")).toBe(true);
		expect(audiencePasses("user", "diagnostic")).toBe(false);
		expect(audiencePasses(undefined, "diagnostic")).toBe(false);
	});

	it("'all' tab shows everything", () => {
		expect(audiencePasses("user", "all")).toBe(true);
		expect(audiencePasses("diagnostic", "all")).toBe(true);
		expect(audiencePasses(undefined, "all")).toBe(true);
	});
});
