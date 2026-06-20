import { describe, expect, it } from "vitest";
import { lastItemKey, lastItemSeverity, prTypeSeverity } from "../../components/Toolbar/Toolbar";
import type { ActivityItem } from "../../plugins/types";
import type { PrNotification } from "../../stores/prNotifications";

function activity(overrides: Partial<ActivityItem>): ActivityItem {
	return {
		id: "a1",
		pluginId: "core",
		sectionId: "git-ops",
		title: "git pull failed",
		icon: "<svg/>",
		dismissible: true,
		createdAt: 1000,
		...overrides,
	};
}

function pr(overrides: Partial<PrNotification>): PrNotification {
	return {
		id: "pr1",
		repoPath: "/repo",
		branch: "feat/x",
		prNumber: 1,
		title: "PR",
		type: "ci_failed",
		createdAt: 1000,
		focusedTimeMs: 0,
		dismissed: false,
		...overrides,
	};
}

describe("prTypeSeverity", () => {
	it("maps CI failure to error", () => {
		expect(prTypeSeverity("ci_failed")).toBe("error");
	});
	it("maps blocked / changes requested to warn", () => {
		expect(prTypeSeverity("blocked")).toBe("warn");
		expect(prTypeSeverity("changes_requested")).toBe("warn");
	});
	it("maps merged / recovered / ready to success", () => {
		expect(prTypeSeverity("merged")).toBe("success");
		expect(prTypeSeverity("ci_recovered")).toBe("success");
		expect(prTypeSeverity("ready")).toBe("success");
	});
	it("falls back to info for neutral transitions", () => {
		expect(prTypeSeverity("review_started")).toBe("info");
		expect(prTypeSeverity("closed")).toBe("info");
	});
});

describe("lastItemSeverity", () => {
	it("uses the activity item's own severity", () => {
		expect(lastItemSeverity({ kind: "activity", item: activity({ severity: "error" }) })).toBe("error");
		expect(lastItemSeverity({ kind: "activity", item: activity({ severity: "success" }) })).toBe("success");
	});
	it("defaults activity items without severity to info", () => {
		expect(lastItemSeverity({ kind: "activity", item: activity({}) })).toBe("info");
	});
	it("derives PR severity from the notification type", () => {
		expect(lastItemSeverity({ kind: "pr", notif: pr({ type: "ci_failed" }) })).toBe("error");
		expect(lastItemSeverity({ kind: "pr", notif: pr({ type: "merged" }) })).toBe("success");
	});
	it("treats app updates as neutral info", () => {
		expect(lastItemSeverity({ kind: "update", version: "1.2.3" })).toBe("info");
	});
});

describe("lastItemKey", () => {
	it("is stable and distinct per source", () => {
		expect(lastItemKey({ kind: "activity", item: activity({ id: "x" }) })).toBe("activity:x");
		expect(lastItemKey({ kind: "pr", notif: pr({ id: "y" }) })).toBe("pr:y");
		expect(lastItemKey({ kind: "update", version: "1.2.3" })).toBe("update:1.2.3");
	});
});
