import { describe, expect, it } from "vitest";
import { mapCommandToHttp } from "../../transport";

describe("plugin binary range transport", () => {
	// Catches: HTTP dropping the offset/length or routing binary ranges as whole-file reads.
	it("preserves file identity and exact range coordinates over HTTP", () => {
		expect(mapCommandToHttp("plugin_file_size", { pluginId: "preview", path: "/data/a b.parquet" })).toMatchObject({
			method: "GET",
			path: "/api/plugins/preview/fs/size?path=%2Fdata%2Fa%20b.parquet",
		});
		expect(
			mapCommandToHttp("plugin_read_file_range", {
				pluginId: "preview",
				path: "/data/a b.parquet",
				offset: 8192,
				length: 128,
			}),
		).toMatchObject({
			method: "GET",
			path: "/api/plugins/preview/fs/range?path=%2Fdata%2Fa%20b.parquet&offset=8192&length=128",
		});
	});
});
