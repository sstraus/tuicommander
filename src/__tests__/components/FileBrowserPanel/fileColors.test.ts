import { describe, expect, it } from "vitest";
import { getFileIconColor } from "../../../components/FileBrowserPanel/fileColors";

describe("getFileIconColor", () => {
	it("gives folders a muted gray-blue tone", () => {
		expect(getFileIconColor("src", true)).toBe("#8f98a7");
	});

	it("maps known extensions to pastel colors", () => {
		expect(getFileIconColor("main.ts", false)).toBe("#60a5fa");
		expect(getFileIconColor("App.tsx", false)).toBe("#60a5fa");
		expect(getFileIconColor("index.js", false)).toBe("#facc15");
		expect(getFileIconColor("lib.rs", false)).toBe("#fb7185");
		expect(getFileIconColor("main.go", false)).toBe("#22d3ee");
		expect(getFileIconColor("app.py", false)).toBe("#7dd3fc");
		expect(getFileIconColor("style.css", false)).toBe("#818cf8");
		expect(getFileIconColor("index.html", false)).toBe("#fb923c");
		expect(getFileIconColor("package.json", false)).toBe("#f59e0b");
		expect(getFileIconColor("config.yaml", false)).toBe("#f87171");
		expect(getFileIconColor("Cargo.toml", false)).toBe("#d97757");
		expect(getFileIconColor("README.md", false)).toBe("#a78bfa");
		expect(getFileIconColor("run.sh", false)).toBe("#86efac");
		expect(getFileIconColor("logo.svg", false)).toBe("#fbbf24");
		expect(getFileIconColor("photo.png", false)).toBe("#4ade80");
	});

	it("matches extensions case-insensitively", () => {
		expect(getFileIconColor("PHOTO.PNG", false)).toBe("#4ade80");
		expect(getFileIconColor("Main.TS", false)).toBe("#60a5fa");
	});

	it("recognizes well-known filenames", () => {
		expect(getFileIconColor("Dockerfile", false)).toBe("#38bdf8");
		expect(getFileIconColor("Dockerfile.prod", false)).toBe("#38bdf8");
		expect(getFileIconColor("Makefile", false)).toBe("var(--fg-muted)");
		expect(getFileIconColor("justfile", false)).toBe("var(--fg-muted)");
	});

	it("mutes dotfiles and lockfiles", () => {
		expect(getFileIconColor(".gitignore", false)).toBe("var(--fg-muted)");
		expect(getFileIconColor(".env", false)).toBe("var(--fg-muted)");
		expect(getFileIconColor("Cargo.lock", false)).toBe("var(--fg-muted)");
		expect(getFileIconColor("yarn.lock", false)).toBe("var(--fg-muted)");
		expect(getFileIconColor("pnpm-lock.yaml", false)).toBe("var(--fg-muted)");
		expect(getFileIconColor("package-lock.json", false)).toBe("var(--fg-muted)");
		expect(getFileIconColor("bun.lockb", false)).toBe("var(--fg-muted)");
	});

	it("returns undefined for unknown or extension-less files", () => {
		expect(getFileIconColor("data.xyz", false)).toBeUndefined();
		expect(getFileIconColor("noext", false)).toBeUndefined();
		expect(getFileIconColor("LICENSE", false)).toBeUndefined();
	});
});
