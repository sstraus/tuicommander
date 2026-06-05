import { describe, expect, it, vi } from "vitest";

import { BUNDLED_FONT_FACES, fetchFontPayloads, NERD_FALLBACK_FACE, resolveWorkerFontFaces } from "../fontAssets";

describe("resolveWorkerFontFaces", () => {
	it("returns the bundled family faces plus the Nerd fallback for JetBrains Mono", () => {
		const faces = resolveWorkerFontFaces("JetBrains Mono");
		const jb = faces.filter((f) => f.family === "JetBrains Mono");
		// 4 declared faces: extralight latin/latin-ext + variable latin/latin-ext
		expect(jb).toHaveLength(4);
		// split faces carry unicode-range (matches global.css)
		expect(jb.every((f) => typeof f.unicodeRange === "string" && f.unicodeRange.length > 0)).toBe(true);
		// exactly one Nerd fallback face, present
		expect(faces.filter((f) => f.family === NERD_FALLBACK_FACE.family)).toHaveLength(1);
	});

	it("returns regular+bold (no unicode-range) plus Nerd fallback for Hack", () => {
		const faces = resolveWorkerFontFaces("Hack");
		const hack = faces.filter((f) => f.family === "Hack");
		expect(hack).toHaveLength(2);
		expect(hack.map((f) => f.weight).sort()).toEqual(["400", "700"]);
		expect(hack.every((f) => f.unicodeRange === undefined)).toBe(true);
		expect(faces.some((f) => f.family === NERD_FALLBACK_FACE.family)).toBe(true);
	});

	it("loads only the active family + Nerd fallback, never other families", () => {
		const faces = resolveWorkerFontFaces("Hack");
		expect(faces.some((f) => f.url.includes("jetbrains"))).toBe(false);
		expect(faces.some((f) => f.url.includes("fira"))).toBe(false);
		expect(faces.some((f) => f.url.includes("geist"))).toBe(false);
	});

	it("maps every selectable FontType to at least one bundled face", () => {
		const fonts = Object.keys(BUNDLED_FONT_FACES) as Array<keyof typeof BUNDLED_FONT_FACES>;
		expect(fonts.length).toBe(11);
		for (const font of fonts) {
			expect(BUNDLED_FONT_FACES[font].length).toBeGreaterThan(0);
		}
	});
});

describe("fetchFontPayloads", () => {
	it("fetches each face URL and returns payloads carrying source buffers + descriptors", async () => {
		const fetchFn = vi.fn(async () => ({ arrayBuffer: async () => new ArrayBuffer(8) }) as unknown as Response);
		const faces = [
			{ family: "Hack", url: "/fonts/hack-regular.woff2", weight: "400" },
			{ family: "Hack", url: "/fonts/hack-bold.woff2", weight: "700" },
		];

		const payloads = await fetchFontPayloads(faces, fetchFn as unknown as typeof fetch);

		expect(fetchFn).toHaveBeenCalledTimes(2);
		expect(fetchFn).toHaveBeenCalledWith("/fonts/hack-regular.woff2");
		expect(fetchFn).toHaveBeenCalledWith("/fonts/hack-bold.woff2");
		expect(payloads).toHaveLength(2);
		expect(payloads[0]).toMatchObject({ family: "Hack", descriptors: { weight: "400" } });
		expect(payloads[0].source).toBeInstanceOf(ArrayBuffer);
	});

	it("propagates unicode-range into descriptors when present", async () => {
		const fetchFn = vi.fn(async () => ({ arrayBuffer: async () => new ArrayBuffer(8) }) as unknown as Response);
		const faces = [{ family: "JetBrains Mono", url: "/fonts/jetbrains-mono-latin.woff2", weight: "400 700", unicodeRange: "U+0000-00FF" }];

		const [payload] = await fetchFontPayloads(faces, fetchFn as unknown as typeof fetch);

		expect(payload.descriptors).toMatchObject({ weight: "400 700", unicodeRange: "U+0000-00FF" });
	});
});
