import type { Disposable, MarkdownProvider } from "./types";
import { appLogger } from "../stores/appLogger";

/**
 * Routes virtual content URIs to registered MarkdownProvider implementations.
 *
 * Each plugin registers a provider keyed by URI scheme. When a user clicks an
 * ActivityItem with a contentUri, resolve() dispatches to the matching provider.
 */
function createMarkdownProviderRegistry() {
  const providers = new Map<string, MarkdownProvider>();

  function register(scheme: string, provider: MarkdownProvider): Disposable {
    const previous = providers.get(scheme);
    providers.set(scheme, provider);

    return {
      dispose() {
        // Only remove if this registration is still the active one
        if (providers.get(scheme) === provider) {
          if (previous) {
            providers.set(scheme, previous);
          } else {
            providers.delete(scheme);
          }
        }
      },
    };
  }

  async function resolve(uriString: string): Promise<string | null> {
    let uri: URL;
    try {
      uri = new URL(uriString);
    } catch {
      appLogger.warn("plugin", `Invalid content URI: ${uriString}`);
      return null;
    }

    // URL.protocol includes the trailing colon, e.g. "plan:"
    const scheme = uri.protocol.replace(/:$/, "");
    if (!scheme) return null;

    const provider = providers.get(scheme);
    if (!provider) {
      appLogger.warn("plugin", `No provider for scheme "${scheme}" (registered: ${[...providers.keys()].join(", ")})`);
      return null;
    }

    return provider.provideContent(uri);
  }

  /** Remove all registrations (for testing). */
  function clear(): void {
    providers.clear();
  }

  return { register, resolve, clear };
}

export const markdownProviderRegistry = createMarkdownProviderRegistry();
