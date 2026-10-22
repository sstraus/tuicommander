import { readFileSync } from "node:fs";
import { pathToFileURL } from "node:url";
import { gzipSync } from "node:zlib";

/** Fraction of a gzip budget at which `--check` starts warning (still passes). */
export const WARN_FRACTION = 0.98;

/**
 * Where `gzipBytes` sits against `maxGzipBytes`: "over" fails `--check`,
 * "warn" (at or above {@link WARN_FRACTION} of the cap) prints a warning so a
 * shrinking headroom is noticed before the hard cap blocks a build.
 */
export function budgetStatus(gzipBytes, maxGzipBytes) {
	if (gzipBytes > maxGzipBytes) return "over";
	if (gzipBytes >= Math.floor(maxGzipBytes * WARN_FRACTION)) return "warn";
	return "ok";
}

const CHECK = process.argv.includes("--check");

const entries = [
	{ page: "dist/index.html", maxGzipBytes: 500 * 1024 },
	{ page: "dist/mobile.html", maxGzipBytes: 100 * 1024 },
];

const deferredAssetPattern =
	/(?:CodeEditorTab|createCodeMirror|DiffFileList|ContentRenderer|MarkdownTab|ComposePanel|PrDiffTab|katex|cytoscape|mermaid)/i;

function main() {
	let failed = false;

	for (const entry of entries) {
		const html = readFileSync(entry.page, "utf8");
		const assets = [
			...new Set(
				[...html.matchAll(/(?:src|href)="\/(assets\/[^\"]+\.(?:js|css))"/g)].map((match) => match[1]),
			),
		];

		let rawBytes = 0;
		let gzipBytes = 0;
		for (const asset of assets) {
			const content = readFileSync(`dist/${asset}`);
			rawBytes += content.length;
			gzipBytes += gzipSync(content).length;
		}

		console.log(`${entry.page}: ${assets.length} assets, ${rawBytes} bytes raw, ${gzipBytes} bytes gzip`);

		if (!CHECK) continue;

		const eagerDeferredAssets = assets.filter((asset) => deferredAssetPattern.test(asset));
		if (eagerDeferredAssets.length > 0) {
			failed = true;
			console.error(`${entry.page}: optional assets returned to the initial load graph:`);
			for (const asset of eagerDeferredAssets) console.error(`  - ${asset}`);
		}

		const status = budgetStatus(gzipBytes, entry.maxGzipBytes);
		if (status === "over") {
			failed = true;
			console.error(`${entry.page}: ${gzipBytes} gzip bytes exceeds the ${entry.maxGzipBytes}-byte budget`);
		} else if (status === "warn") {
			console.warn(
				`WARNING ${entry.page}: ${gzipBytes} gzip bytes is over ${WARN_FRACTION * 100}% of the ${entry.maxGzipBytes}-byte budget (${entry.maxGzipBytes - gzipBytes} B headroom)`,
			);
		}
	}

	if (failed) process.exitCode = 1;
}

if (process.argv[1] && import.meta.url === pathToFileURL(process.argv[1]).href) main();
