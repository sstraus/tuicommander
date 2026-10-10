import { createSignal } from "solid-js";
import { reportPluginCallbackError } from "./pluginCallbackGuard";
import type { Disposable, FileIconProvider } from "./types";

/**
 * Registry for file icon providers.
 *
 * Plugins register a FileIconProvider that maps filenames/extensions to
 * inline SVG strings. Components query resolve() to get the icon for a
 * file entry. Last registered provider wins (with restore on dispose).
 *
 * The `version` signal increments on register/unregister so reactive
 * components re-render when the active provider changes.
 */
function createFileIconRegistry() {
	const [version, setVersion] = createSignal(0);
	// Registration stack — the top (last element) is the active provider.
	// Register pushes; dispose removes by identity, so out-of-order disposal
	// of any registration keeps the remaining chain intact (mirrors
	// markdownProviderRegistry's per-registration restore semantics).
	const providers: Array<{ pluginId?: string; provider: FileIconProvider }> = [];
	// Resolved icons per `${isDir}:${name}`. A directory listing mounts one row per
	// entry and each row resolves its icon, so without this the active provider is
	// asked again for every repeated name, on every re-render. Dropped whenever the
	// active provider can change — a different provider answers differently.
	const resolved = new Map<string, string | null>();

	function register(provider: FileIconProvider, pluginId?: string): Disposable {
		const entry = { pluginId, provider };
		providers.push(entry);
		resolved.clear();
		setVersion((v) => v + 1);

		return {
			dispose() {
				const index = providers.lastIndexOf(entry);
				if (index !== -1) {
					providers.splice(index, 1);
					resolved.clear();
					setVersion((v) => v + 1);
				}
			},
		};
	}

	function resolve(name: string, isDir: boolean): string | null {
		const activeProvider = providers.at(-1)!;
		if (!activeProvider) return null;
		const key = `${isDir ? "d" : "f"}:${name}`;
		if (resolved.has(key)) return resolved.get(key) ?? null;
		try {
			const icon = activeProvider.provider.resolveFileIcon(name, isDir);
			resolved.set(key, icon);
			return icon;
		} catch (err) {
			// Not cached: a throwing provider stays visible in the logs, and a
			// transient failure is not turned into a permanently missing icon.
			reportPluginCallbackError(activeProvider.pluginId, "file icon resolve", err);
			return null;
		}
	}

	/** Reactive version number — read this in components to trigger re-render on provider change */
	function getVersion(): number {
		return version();
	}

	/** Remove all registrations (for testing). */
	function clear(): void {
		providers.length = 0;
		resolved.clear();
		setVersion(0);
	}

	return { register, resolve, getVersion, clear };
}

export const fileIconRegistry = createFileIconRegistry();
