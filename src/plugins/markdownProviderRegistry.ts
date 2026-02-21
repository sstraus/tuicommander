import type { Disposable, MarkdownProvider } from "./types";

/**
 * Routes virtual content URIs to registered MarkdownProvider implementations.
 *
 * Follows the VS Code TextDocumentContentProvider pattern: each plugin registers
 * a provider keyed by URI scheme. When a user clicks an ActivityItem with a
 * contentUri, resolve() dispatches to the matching provider.
 *
 * Multiple registrations for the same scheme stack — the most recent wins.
 * Disposing a registration restores the previous provider for that scheme.
 */
function createMarkdownProviderRegistry() {
  // Stack per scheme: last entry is the active provider
  const stacks = new Map<string, MarkdownProvider[]>();

  function register(scheme: string, provider: MarkdownProvider): Disposable {
    if (!stacks.has(scheme)) {
      stacks.set(scheme, []);
    }
    stacks.get(scheme)!.push(provider);

    return {
      dispose() {
        const stack = stacks.get(scheme);
        if (!stack) return;
        const idx = stack.lastIndexOf(provider);
        if (idx >= 0) stack.splice(idx, 1);
        if (stack.length === 0) stacks.delete(scheme);
      },
    };
  }

  async function resolve(uriString: string): Promise<string | null> {
    let uri: URL;
    try {
      uri = new URL(uriString);
    } catch {
      return null;
    }

    // URL.protocol includes the trailing colon, e.g. "plan:"
    const scheme = uri.protocol.replace(/:$/, "");
    if (!scheme) return null;

    const stack = stacks.get(scheme);
    if (!stack || stack.length === 0) return null;

    const provider = stack[stack.length - 1];
    return provider.provideContent(uri);
  }

  /** Remove all registrations (for testing). */
  function clear(): void {
    stacks.clear();
  }

  return { register, resolve, clear };
}

export const markdownProviderRegistry = createMarkdownProviderRegistry();
