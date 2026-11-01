import { describe, expect, it, vi } from "vitest";

vi.mock("../../transport", () => ({ isTauri: () => true }));
vi.mock("../../stores/repositories", () => ({ locateFile: vi.fn() }));
vi.mock("../../transportRuntime", () => ({
	getRemoteBaseUrl: vi.fn(),
	getRepoConnection: vi.fn(),
	withRemoteToken: vi.fn(),
}));
vi.mock("@tauri-apps/api/core", () => ({
	convertFileSrc: (path: string) => `asset://localhost/${encodeURIComponent(path)}`,
}));

import { rewriteLocalImages } from "../../utils/repoImageUrl";

// Catches: Tauri refuses the joined ../ path and the document's image never loads.
it.each([
	["/repo/docs/product", "../../assets/shot.png", "/repo/assets/shot.png"],
	["/repo/docs", "./screens/../my%20shot.png", "/repo/docs/my shot.png"],
	["C:\\repo\\docs", "..\\assets\\shot.png", "C:/repo/assets/shot.png"],
	["\\\\server\\share\\docs", "../assets/shot.png", "//server/share/assets/shot.png"],
	["/repo/docs", "/repo/./assets/../shot.png", "/repo/shot.png"],
])("local_parent_segments_resolve_before_asset_conversion: %s + %s", (base, source, expected) => {
	const html = rewriteLocalImages(`<img src="${source}">`, base);
	const image = new DOMParser().parseFromString(html, "text/html").querySelector("img");
	expect(image?.getAttribute("src")).toBe(`asset://localhost/${encodeURIComponent(expected)}`);
});

describe("desktop image transport boundaries", () => {
	it.each(["https://example.test/../shot.png", "data:image/png;base64,YQ==", "asset://localhost/shot.png"])(
		"keeps already routed images unchanged: %s",
		(source) => {
			const html = `<img src="${source}">`;
			expect(rewriteLocalImages(html, "/repo/docs")).toBe(html);
		},
	);
	it("keeps an explicit image resolver in control of its path", () => {
		const resolver = vi.fn(() => "https://remote.test/shot.png");
		rewriteLocalImages('<img src="../shot.png">', "/repo/docs", resolver);
		expect(resolver).toHaveBeenCalledWith("../shot.png");
	});
});
