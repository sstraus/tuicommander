import type { Disposable } from "./types";

interface FilePreviewHandler {
	pluginId: string;
	onOpen: (ctx: FilePreviewContext) => void;
}

export interface FilePreviewContext {
	filePath: string;
	repoPath: string;
	fsRoot: string;
}

function createFilePreviewRegistry() {
	const registry = new Map<string, FilePreviewHandler>();

	function register(pluginId: string, extensions: string[], handler: FilePreviewHandler["onOpen"]): Disposable {
		const normalized = extensions.map((e) => e.toLowerCase());
		for (const ext of normalized) {
			registry.set(ext, { pluginId, onOpen: handler });
		}
		return {
			dispose() {
				for (const ext of normalized) {
					if (registry.get(ext)?.pluginId === pluginId) {
						registry.delete(ext);
					}
				}
			},
		};
	}

	function getHandler(filePath: string): FilePreviewHandler | undefined {
		const dot = filePath.lastIndexOf(".");
		if (dot === -1) return undefined;
		return registry.get(filePath.slice(dot + 1).toLowerCase());
	}

	function clear(): void {
		registry.clear();
	}

	return { register, getHandler, clear };
}

export const filePreviewRegistry = createFilePreviewRegistry();
