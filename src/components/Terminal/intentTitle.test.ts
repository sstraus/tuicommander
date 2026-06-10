import { describe, expect, it } from "vitest";
import { shouldApplyIntentTitle } from "./intentTitle";

describe("shouldApplyIntentTitle", () => {
	const base = { title: "Writing tests", globalEnabled: true, perAgentEnabled: true, nameIsCustom: false };

	it("applies the intent title under default conditions", () => {
		expect(shouldApplyIntentTitle(base)).toBe(true);
	});

	it("never overwrites a user-renamed tab", () => {
		expect(shouldApplyIntentTitle({ ...base, nameIsCustom: true })).toBe(false);
	});

	it("does nothing when the global setting is off", () => {
		expect(shouldApplyIntentTitle({ ...base, globalEnabled: false })).toBe(false);
	});

	it("does nothing when the per-agent override is off", () => {
		expect(shouldApplyIntentTitle({ ...base, perAgentEnabled: false })).toBe(false);
	});

	it("does nothing without a title", () => {
		expect(shouldApplyIntentTitle({ ...base, title: "" })).toBe(false);
		expect(shouldApplyIntentTitle({ ...base, title: null })).toBe(false);
		expect(shouldApplyIntentTitle({ ...base, title: undefined })).toBe(false);
	});
});
