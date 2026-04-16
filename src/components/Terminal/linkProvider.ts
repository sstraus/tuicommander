/**
 * File-path link provider for xterm.js terminals.
 *
 * Extracts clickable file paths from terminal output lines, resolves them
 * via Rust IPC, and registers them as xterm ILinkProvider entries.
 */
import type { Terminal, ILink, ILinkProvider } from "@xterm/xterm";
import { invoke } from "../../invoke";
import { terminalsStore } from "../../stores/terminals";

/** Known source/config/doc extensions — used in the path regex boundary. */
export const CODING_EXT = "rs|ts|tsx|js|jsx|mjs|cjs|py|go|java|kt|kts|swift|c|h|cpp|hpp|cc|cs|rb|php|lua|zig|nim|ex|exs|erl|hs|ml|mli|fs|fsx|scala|clj|cljs|r|R|jl|dart|v|sv|vhdl|sol|move|css|scss|sass|less|html|htm|vue|svelte|astro|json|jsonc|json5|yaml|yml|toml|ini|cfg|conf|env|xml|plist|csv|tsv|sql|graphql|gql|proto|thrift|avsc|md|mdx|txt|rst|tex|adoc|org|sh|bash|zsh|fish|ps1|psm1|bat|cmd|dockerfile|containerfile|tf|tfvars|hcl|nix|cmake|make|mk|gradle|sbt|cabal|gemspec|podspec|lock|sum|mod|workspace|editorconfig|gitignore|gitattributes|dockerignore|eslintrc|prettierrc|babelrc|nvmrc|tool-versions|pdf|png|jpg|jpeg|gif|webp|svg|avif|ico|bmp|mp4|webm|mov|ogg|mp3|wav|flac|aac|m4a|log";

/** Factory — returns a fresh RegExp (has lastIndex state, not safe to share). */
export function filePathRegex(): RegExp {
  return new RegExp(
    `(?:^|[\\s"'\`(\\[{])` +
    `((?:~/|/|\\.\\.?/|[\\w@.-]+/)` +
    `[\\w./@-]*` +
    `\\.(?:${CODING_EXT})` +
    `(?::\\d+(?::\\d+)?)?)` +
    `(?=[\\s"'\`),;.!?:\\]}>]|$)`,
    "g",
  );
}

/** Factory — returns a fresh file:// URL regex. */
export function fileUrlRegex(): RegExp {
  return /\bfile:\/\/(\/[^\s"'`<>()[\]{}]+)/g;
}

/**
 * Install the file-path link provider on an xterm terminal instance.
 *
 * @param terminal  xterm Terminal instance
 * @param termId    terminal store ID (used to look up cwd)
 * @param onOpenFilePath  callback when user clicks a resolved file link
 */
export function installLinkProvider(
  terminal: Terminal,
  termId: string,
  onOpenFilePath: (path: string, line?: number, col?: number) => void,
): void {
  const fpRegex = filePathRegex();
  const fuRegex = fileUrlRegex();

  // LRU cache: resolved links per line to avoid flicker from async IPC on every mouse move.
  // Key: "lineNumber:lineText", value: resolved ILink[] or undefined.
  // Capped at 200 entries; cleared wholesale when full (lines rarely re-hover after scroll).
  const linkCache = new Map<string, ILink[] | undefined>();
  const cacheSet = (key: string, val: ILink[] | undefined) => {
    if (linkCache.size >= 200) linkCache.clear();
    linkCache.set(key, val);
  };

  const provider: ILinkProvider = {
    provideLinks(bufferLineNumber: number, callback: (links: ILink[] | undefined) => void) {
      const bufLine = terminal.buffer.active.getLine(bufferLineNumber - 1);
      if (!bufLine) { callback(undefined); return; }
      const lineText = bufLine.translateToString();

      const cacheKey = `${bufferLineNumber}:${lineText}`;
      if (linkCache.has(cacheKey)) {
        callback(linkCache.get(cacheKey));
        return;
      }

      const matches: { text: string; candidate: string; index: number }[] = [];
      let match: RegExpExecArray | null;
      fpRegex.lastIndex = 0;
      while ((match = fpRegex.exec(lineText)) !== null) {
        const idx = lineText.indexOf(match[1], match.index);
        matches.push({ text: match[1], candidate: match[1], index: idx });
      }
      fuRegex.lastIndex = 0;
      while ((match = fuRegex.exec(lineText)) !== null) {
        matches.push({ text: match[0], candidate: match[1], index: match.index });
      }
      if (matches.length === 0) {
        cacheSet(cacheKey, undefined);
        callback(undefined);
        return;
      }

      const termData = terminalsStore.get(termId);
      const cwd = termData?.cwd || "";

      Promise.all(
        matches.map(async (m) => {
          try {
            const resolved = await invoke<{ absolute_path: string; is_directory: boolean } | null>(
              "resolve_terminal_path",
              { cwd, candidate: m.candidate },
            );
            return resolved ? { ...m, resolved } : null;
          } catch {
            return null;
          }
        }),
      ).then((results) => {
        const links: ILink[] = [];
        for (const r of results) {
          if (!r) continue;
          const startCol = r.index + 1;
          let line: number | undefined;
          let col: number | undefined;
          const lineColMatch = r.candidate.match(/:(\d+)(?::(\d+))?$/);
          if (lineColMatch) {
            line = parseInt(lineColMatch[1], 10);
            if (lineColMatch[2]) col = parseInt(lineColMatch[2], 10);
          }
          links.push({
            range: {
              start: { x: startCol, y: bufferLineNumber },
              end: { x: startCol + r.text.length - 1, y: bufferLineNumber },
            },
            text: r.text,
            activate: (event: MouseEvent) => {
              if (event.button !== 0) return;
              onOpenFilePath(r.resolved.absolute_path, line, col);
            },
          });
        }
        const result = links.length > 0 ? links : undefined;
        cacheSet(cacheKey, result);
        callback(result);
      }).catch(() => {
        cacheSet(cacheKey, undefined);
        callback(undefined);
      });
    },
  };

  terminal.registerLinkProvider(provider);
}
