// --- Phase 1.2 bundled-font assets for the render worker (main-thread side) ---
//
// The terminal renders ONE selected family at a time as a CSS stack:
//   system Nerd Font (OS-provided) -> bundled family -> bundled Symbols Nerd
//   Font Mono -> monospace
// Only the BUNDLED entries need FontFace registration in the worker (the worker
// has no CSS). So for a given selection we fetch the active family's woff2
// face(s) + the Symbols Nerd Font Mono fallback — NOT all 11 families. The
// system Nerd Font is best-effort (OS) and not bundled.
//
// Faces below mirror the @font-face blocks in src/global.css verbatim,
// including unicode-range for the split families (JetBrains Mono, Fira Code).

import type { FontType } from "../../stores/settings";
import type { FontPayload } from "./workerProtocol";

export interface FaceAsset {
	family: string;
	url: string;
	weight: string;
	style?: string;
	unicodeRange?: string;
}

// Shared unicode-range strings (identical in global.css for the latin /
// latin-ext split faces of JetBrains Mono and Fira Code).
const LATIN =
	"U+0000-00FF, U+0131, U+0152-0153, U+02BB-02BC, U+02C6, U+02DA, U+02DC, U+0304, U+0308, U+0329, U+2000-206F, U+20AC, U+2122, U+2191, U+2193, U+2212, U+2215, U+FEFF, U+FFFD";
const LATIN_EXT =
	"U+0100-02BA, U+02BD-02C5, U+02C7-02CC, U+02CE-02D7, U+02DD-02FF, U+0304, U+0308, U+0329, U+1D00-1DBF, U+1E00-1E9F, U+1EF2-1EFF, U+2020, U+20A0-20AB, U+20AD-20C0, U+2113, U+2C60-2C7F, U+A720-A7FF";

/** Always-loaded powerline/symbol fallback (last in every stack). */
export const NERD_FALLBACK_FACE: FaceAsset = {
	family: "Symbols Nerd Font Mono",
	url: "/fonts/symbols-nerd-font-mono.woff2",
	weight: "400",
};

/** Bundled woff2 faces per selectable family, keyed by FontType (= family name). */
export const BUNDLED_FONT_FACES: Record<FontType, FaceAsset[]> = {
	"JetBrains Mono": [
		{
			family: "JetBrains Mono",
			url: "/fonts/jetbrains-mono-extralight-latin.woff2",
			weight: "200",
			unicodeRange: LATIN,
		},
		{
			family: "JetBrains Mono",
			url: "/fonts/jetbrains-mono-extralight-latin-ext.woff2",
			weight: "200",
			unicodeRange: LATIN_EXT,
		},
		{ family: "JetBrains Mono", url: "/fonts/jetbrains-mono-latin.woff2", weight: "400 700", unicodeRange: LATIN },
		{
			family: "JetBrains Mono",
			url: "/fonts/jetbrains-mono-latin-ext.woff2",
			weight: "400 700",
			unicodeRange: LATIN_EXT,
		},
	],
	"Fira Code": [
		{ family: "Fira Code", url: "/fonts/fira-code-latin.woff2", weight: "400 700", unicodeRange: LATIN },
		{ family: "Fira Code", url: "/fonts/fira-code-latin-ext.woff2", weight: "400 700", unicodeRange: LATIN_EXT },
	],
	Hack: [
		{ family: "Hack", url: "/fonts/hack-regular.woff2", weight: "400" },
		{ family: "Hack", url: "/fonts/hack-bold.woff2", weight: "700" },
	],
	"Cascadia Code": [{ family: "Cascadia Code", url: "/fonts/cascadia-code.woff2", weight: "200 700" }],
	Iosevka: [
		{ family: "Iosevka", url: "/fonts/iosevka-regular.woff2", weight: "400" },
		{ family: "Iosevka", url: "/fonts/iosevka-bold.woff2", weight: "700" },
	],
	"Source Code Pro": [
		{ family: "Source Code Pro", url: "/fonts/source-code-pro-regular.woff2", weight: "400" },
		{ family: "Source Code Pro", url: "/fonts/source-code-pro-bold.woff2", weight: "700" },
	],
	Inconsolata: [
		{ family: "Inconsolata", url: "/fonts/inconsolata-regular.woff2", weight: "400" },
		{ family: "Inconsolata", url: "/fonts/inconsolata-bold.woff2", weight: "700" },
	],
	"IBM Plex Mono": [
		{ family: "IBM Plex Mono", url: "/fonts/ibm-plex-mono-regular.woff2", weight: "400" },
		{ family: "IBM Plex Mono", url: "/fonts/ibm-plex-mono-bold.woff2", weight: "700" },
	],
	"Monaspace Neon": [
		{ family: "Monaspace Neon", url: "/fonts/monaspace-neon-regular.woff2", weight: "400" },
		{ family: "Monaspace Neon", url: "/fonts/monaspace-neon-bold.woff2", weight: "700" },
	],
	"Commit Mono": [
		{ family: "Commit Mono", url: "/fonts/commit-mono-regular.woff2", weight: "400" },
		{ family: "Commit Mono", url: "/fonts/commit-mono-bold.woff2", weight: "700" },
	],
	"Geist Mono": [
		{ family: "Geist Mono", url: "/fonts/geist-mono-regular.woff2", weight: "400" },
		{ family: "Geist Mono", url: "/fonts/geist-mono-bold.woff2", weight: "700" },
	],
};

/** Faces to register in the worker for a selection: family faces + Nerd fallback. */
export function resolveWorkerFontFaces(font: FontType): FaceAsset[] {
	return [...BUNDLED_FONT_FACES[font], NERD_FALLBACK_FACE];
}

function descriptorsFor(face: FaceAsset) {
	const d: { weight?: string; style?: string; unicodeRange?: string } = { weight: face.weight };
	if (face.style) d.style = face.style;
	if (face.unicodeRange) d.unicodeRange = face.unicodeRange;
	return d;
}

/**
 * Prefetch the woff2 bytes on the main thread (Tauri serves them as bundled
 * assets at /fonts/...), returning transferable payloads for the worker.
 * `fetchFn` is injectable for tests; defaults to the global `fetch`.
 */
export async function fetchFontPayloads(faces: FaceAsset[], fetchFn: typeof fetch = fetch): Promise<FontPayload[]> {
	return Promise.all(
		faces.map(async (face) => {
			const res = await fetchFn(face.url);
			const source = await res.arrayBuffer();
			return { family: face.family, source, descriptors: descriptorsFor(face) };
		}),
	);
}
