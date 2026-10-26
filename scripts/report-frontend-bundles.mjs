import { readFileSync } from "node:fs";
import { gzipSync } from "node:zlib";

const CHECK = process.argv.includes("--check");

const entries = [
	{ page: "dist/index.html", maxGzipBytes: 500 * 1024 },
	// 2026-10-25: Boss raised the budget for feature growth; measured mobile size: 76169 gzip bytes.
	{ page: "dist/mobile.html", maxGzipBytes: 150 * 1024 },
];

const deferredAssetPattern =
	/(?:CodeEditorTab|createCodeMirror|DiffFileList|ContentRenderer|MarkdownTab|ComposePanel|PrDiffTab|katex|cytoscape|mermaid)/i;

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

	if (gzipBytes > entry.maxGzipBytes) {
		failed = true;
		console.error(`${entry.page}: ${gzipBytes} gzip bytes exceeds the ${entry.maxGzipBytes}-byte budget`);
	}
}

if (failed) process.exitCode = 1;
