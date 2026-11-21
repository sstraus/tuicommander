import assert from "node:assert/strict";
import { chmodSync, mkdtempSync, rmSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { delimiter, join } from "node:path";
import test from "node:test";
import { browserWrapper } from "./browser-wrapper.mjs";

// Catches: the live browser harnesses falling back to a home-relative wrapper path.
test("browserWrapper resolves TUIC_BROWSER_WRAPPER, else ab-stealth.sh on PATH, else fails", () => {
	const dir = mkdtempSync(join(tmpdir(), "browser-wrapper-"));
	try {
		const wrapper = join(dir, "ab-stealth.sh");
		writeFileSync(wrapper, "#!/bin/sh\n");
		chmodSync(wrapper, 0o755);
		const empty = join(dir, "empty");
		assert.equal(browserWrapper({ PATH: [empty, dir].join(delimiter) }), wrapper);
		assert.equal(browserWrapper({ TUIC_BROWSER_WRAPPER: wrapper, PATH: "" }), wrapper);
		assert.throws(() => browserWrapper({ PATH: empty, HOME: dir }), /set TUIC_BROWSER_WRAPPER/);
		assert.throws(() => browserWrapper({ TUIC_BROWSER_WRAPPER: join(dir, "missing"), PATH: dir }), /not executable/);
	} finally {
		rmSync(dir, { recursive: true, force: true });
	}
});
