import { readFileSync } from "node:fs";
import { dirname, resolve } from "node:path";

// Follow the local, unconditional imports used by the app's token entrypoints.
// Reading only the entrypoint loses shared tokens that the browser loads.
export function readStylesheet(path: string): string {
	return readFileSync(path, "utf8").replace(/@import\s+["']([^"']+)["'];/g, (_rule, imported: string) =>
		readStylesheet(resolve(dirname(path), imported)),
	);
}
