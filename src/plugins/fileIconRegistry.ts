import { createSignal } from "solid-js";
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
  let activeProvider: FileIconProvider | null = null;
  let previousProvider: FileIconProvider | null = null;

  function register(provider: FileIconProvider): Disposable {
    previousProvider = activeProvider;
    activeProvider = provider;
    setVersion((v) => v + 1);

    return {
      dispose() {
        if (activeProvider === provider) {
          activeProvider = previousProvider;
          previousProvider = null;
          setVersion((v) => v + 1);
        }
      },
    };
  }

  function resolve(name: string, isDir: boolean): string | null {
    if (!activeProvider) return null;
    return activeProvider.resolveFileIcon(name, isDir);
  }

  /** Reactive version number — read this in components to trigger re-render on provider change */
  function getVersion(): number {
    return version();
  }

  /** Remove all registrations (for testing). */
  function clear(): void {
    activeProvider = null;
    previousProvider = null;
    setVersion(0);
  }

  return { register, resolve, getVersion, clear };
}

export const fileIconRegistry = createFileIconRegistry();
