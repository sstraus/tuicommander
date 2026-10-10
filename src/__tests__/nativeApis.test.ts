import { describe, expect, it } from "vitest";

const sources = import.meta.glob<string>(
	["../**/*.ts", "../**/*.tsx", "!../**/__tests__/**", "!../**/*.test.*", "!../transport*.ts"],
	{ query: "?raw", import: "default", eager: true },
);

function matches(pattern: RegExp): string[] {
	return Object.entries(sources).flatMap(([file, source]) =>
		Array.from(source.matchAll(pattern), (match) => `${file}: ${match[0]}`),
	);
}

describe("ES2024 native API conventions", () => {
	it("prevents reintroducing global literal regex replacements", () => {
		expect(matches(/\.replace\(\/(?:[^\\/.*+?^$()[\]{}|]|\\[\\/nrt])+\/g\s*,/g)).toEqual([]);
	});
	it("prevents redundant copies before sorting arrays", () => {
		const redundant = Object.entries(sources).flatMap(([file, source]) =>
			Array.from(source.matchAll(/\[\.\.\.([^\]\n]+)\]\.sort\(/g), (match) => ({ file, expression: match[1] })).filter(
				({ expression }) => !/\.keys\(\)|\.entries\(\)|^(found|starts)$|,|\.\.\./.test(expression),
			),
		);
		expect(redundant).toEqual([]);
	});
	it("prevents restoring hand-built prompt and activity buckets", () => {
		expect(
			matches(
				/new Map<string, (?:SavedPrompt|VarDef)\[\]>|const (?:recent|earlier|today|older): ActivityItemData\[\]/g,
			),
		).toEqual([]);
	});
	it("prevents promise executors used only to export a resolver", () => {
		expect(matches(/new Promise(?:<[^>]+>)?\(\(resolve\) => \{\s*\w+(?:Resolve|Resolver) = resolve/g)).toEqual([]);
		expect(sources["../components/Terminal/canvasTerminalTransport.ts"]).not.toMatch(/rejectConnect = reject/);
	});
	it("prevents JSON round trips for the plain IPC-loaded app config", () => {
		expect(sources["../utils/updateAppConfig.ts"]).not.toMatch(/JSON\.parse\(JSON\.stringify/);
	});
});
