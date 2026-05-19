import { describe, it, expect } from "vitest";
import { formatRelativeTime } from "../utils/formatRelativeTime";

describe("formatRelativeTime", () => {
	it("returns 'just now' for <5s", () => {
		expect(formatRelativeTime(0)).toBe("just now");
		expect(formatRelativeTime(4999)).toBe("just now");
	});

	it("returns Xs for 5s..59s", () => {
		expect(formatRelativeTime(5000)).toBe("5s");
		expect(formatRelativeTime(59999)).toBe("59s");
	});

	it("returns Xm for 60s..59m", () => {
		expect(formatRelativeTime(60000)).toBe("1m");
		expect(formatRelativeTime(3599999)).toBe("59m");
	});

	it("returns Xh for 1h..23h", () => {
		expect(formatRelativeTime(3600000)).toBe("1h");
		expect(formatRelativeTime(86399999)).toBe("23h");
	});

	it("returns Xd for >=24h", () => {
		expect(formatRelativeTime(86400000)).toBe("1d");
		expect(formatRelativeTime(86400000 * 7)).toBe("7d");
	});

	it("handles negative values gracefully", () => {
		expect(formatRelativeTime(-1000)).toBe("just now");
	});
});
