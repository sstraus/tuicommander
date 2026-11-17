// Test scratch root for Node-side tooling (vitest workers, the cycle checker's
// self-test), resolved the same way scripts/with-test-tmp.sh and
// tuic-test-support resolve theirs:
//
//   TUIC_TEST_TMP_ROOT      the per-run dir a wrapper already chose, else
//   TUIC_TEST_TMP_BASE      opt-in base (e.g. <checkout>/.tmp/tuic-tests), else
//   <host temp>/tuic-tests  host = TUIC_TEST_HOST_TMPDIR, else the OS temp dir.
//
// Never derived from $HOME. `node scripts/test-tmp-root.mjs --self-test` checks it.
import os from "node:os";
import path from "node:path";
import { fileURLToPath } from "node:url";

/**
 * @param {Record<string, string | undefined>} env
 * @param {string} osTmpdir what `os.tmpdir()` returns for this process
 * @returns {string}
 */
export function testTmpRoot(env = process.env, osTmpdir = os.tmpdir()) {
	if (env.TUIC_TEST_TMP_ROOT) return path.resolve(env.TUIC_TEST_TMP_ROOT);
	if (env.TUIC_TEST_TMP_BASE) return path.resolve(env.TUIC_TEST_TMP_BASE);
	return path.join(path.resolve(env.TUIC_TEST_HOST_TMPDIR || osTmpdir), "tuic-tests");
}

function selfTest() {
	const cases = [
		[{ TUIC_TEST_TMP_ROOT: "/r/run.1/" }, "/os", "/r/run.1"],
		[{ TUIC_TEST_TMP_BASE: "/checkout/.tmp/tuic-tests" }, "/os", "/checkout/.tmp/tuic-tests"],
		[{ TUIC_TEST_HOST_TMPDIR: "/host/" }, "/os", "/host/tuic-tests"],
		[{}, "/var/folders/x/T/", "/var/folders/x/T/tuic-tests"],
		// Catches: the old HOME-relative Gits allow-list deciding anything.
		[{ HOME: "/home/dev", TMPDIR: "/home/dev/Gits/.tmp" }, "/home/dev/Gits/.tmp", "/home/dev/Gits/.tmp/tuic-tests"],
		[{ HOME: "/home/dev" }, "/tmp", "/tmp/tuic-tests"],
	];
	for (const [env, tmp, expected] of cases) {
		const actual = testTmpRoot(env, tmp);
		if (actual !== expected) {
			throw new Error(`testTmpRoot(${JSON.stringify(env)}, ${tmp}) = ${actual}, expected ${expected}`);
		}
	}
	process.stdout.write(`test-tmp-root: ${cases.length} resolution cases pass\n`);
}

if (process.argv[1] && path.resolve(process.argv[1]) === fileURLToPath(import.meta.url)) {
	if (process.argv.includes("--self-test")) selfTest();
	else process.stdout.write(`${testTmpRoot()}\n`);
}
