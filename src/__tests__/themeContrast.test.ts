import { readFileSync } from "node:fs";
import { describe, expect, it } from "vitest";

interface Theme {
	name: string;
	appChrome: Record<string, string>;
}

const themes = import.meta.glob<Theme>("../../src-tauri/src/themes/*.json", {
	eager: true,
	import: "default",
});
const surfaces = ["background", "surface", "surfaceElevated", "highlight"] as const;

// Independent WCAG 2.x sRGB oracle: do not use the app's color helpers here.
function luminance(hex: string): number {
	const channels = hex.match(/[a-f\d]{2}/gi)!.map((pair) => {
		const value = parseInt(pair, 16) / 255;
		return value <= 0.04045 ? value / 12.92 : ((value + 0.055) / 1.055) ** 2.4;
	});
	return channels[0]! * 0.2126 + channels[1]! * 0.7152 + channels[2]! * 0.0722;
}

function contrast(a: string, b: string): number {
	const values = [luminance(a), luminance(b)].sort((x, y) => x - y);
	return (values[1]! + 0.05) / (values[0]! + 0.05);
}

describe("informational chrome contrast", () => {
	// Catches: a token edit silently makes small secondary/muted text unreadable.
	it("keeps every built-in secondary and muted token at AA on all neutral surfaces", () => {
		expect(Object.keys(themes)).not.toHaveLength(0);
		const failures: string[] = [];
		for (const theme of Object.values(themes)) {
			for (const foreground of ["foregroundSecondary", "mutedForeground"]) {
				for (const surface of surfaces) {
					const ratio = contrast(theme.appChrome[foreground]!, theme.appChrome[surface]!);
					if (ratio < 4.5) failures.push(`${theme.name}: ${foreground} on ${surface} = ${ratio.toFixed(3)}:1`);
				}
			}
		}
		expect(failures).toEqual([]);
	});

	// Catches: fixing muted contrast erases or reverses the text hierarchy.
	it("retains primary above secondary above muted on each theme surface", () => {
		const failures: string[] = [];
		for (const theme of Object.values(themes)) {
			for (const surface of surfaces) {
				const ratios = ["foreground", "foregroundSecondary", "mutedForeground"].map((key) =>
					contrast(theme.appChrome[key]!, theme.appChrome[surface]!),
				);
				if (!(ratios[0]! > ratios[1]! && ratios[1]! > ratios[2]!)) failures.push(`${theme.name}: ${surface}`);
			}
		}
		expect(failures).toEqual([]);
	});

	// Catches: startup CSS drifts from the Commander foregrounds loaded by both clients.
	it("uses Commander foregrounds for the shared desktop and mobile startup defaults", () => {
		const foregroundDefaults = readFileSync("src/styles/foreground-tokens.css", "utf8");
		const commander = Object.entries(themes).find(([path]) => path.endsWith("/commander.json"))![1].appChrome;
		for (const [token, key] of [
			["primary", "foreground"],
			["secondary", "foregroundSecondary"],
			["muted", "mutedForeground"],
		]) {
			const actual = foregroundDefaults.match(new RegExp(`--fg-${token}:\\s*(#[a-f\\d]{6})`, "i"))?.[1];
			expect(actual, token).toBe(commander[key!]);
		}
	});
});
