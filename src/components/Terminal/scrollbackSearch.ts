import type { LogLine, LogSpan } from "../../mobile/utils/logLine";
import { lineText } from "../../mobile/utils/logLine";
import type { SearchOptions } from "../shared/DomSearchEngine";

export interface SearchMatch {
  lineIndex: number;
  colStart: number;
  colEnd: number;
}

const MAX_MATCHES = 1000;

export function searchLogLines(
  lines: LogLine[],
  query: string,
  opts: SearchOptions,
): SearchMatch[] {
  if (!query) return [];
  let pattern: RegExp;
  try {
    let src = opts.regex ? query : query.replace(/[.*+?^${}()|[\]\\]/g, "\\$&");
    if (opts.wholeWord) src = `\\b${src}\\b`;
    pattern = new RegExp(src, opts.caseSensitive ? "g" : "gi");
  } catch {
    return [];
  }

  const matches: SearchMatch[] = [];
  for (let i = 0; i < lines.length && matches.length < MAX_MATCHES; i++) {
    const text = lineText(lines[i]);
    pattern.lastIndex = 0;
    let m: RegExpExecArray | null;
    while ((m = pattern.exec(text)) !== null && matches.length < MAX_MATCHES) {
      if (m[0].length === 0) { pattern.lastIndex++; continue; }
      matches.push({ lineIndex: i, colStart: m.index, colEnd: m.index + m[0].length });
    }
  }
  return matches;
}

export interface HighlightSegment {
  text: string;
  span: LogSpan;
  highlight: boolean;
  active: boolean;
}

export function highlightSpans(
  line: LogLine,
  matches: SearchMatch[],
  activeMatchGlobal: number,
  globalOffset: number,
): HighlightSegment[] {
  if (matches.length === 0) {
    return line.spans.map((sp) => ({ text: sp.text, span: sp, highlight: false, active: false }));
  }
  const segments: HighlightSegment[] = [];
  let charPos = 0;
  for (const sp of line.spans) {
    const spanStart = charPos;
    const spanLen = sp.text.length;
    let cursor = 0;
    for (let mi = 0; mi < matches.length; mi++) {
      const m = matches[mi];
      const mStart = Math.max(0, m.colStart - spanStart);
      const mEnd = Math.min(spanLen, m.colEnd - spanStart);
      if (mStart >= spanLen || mEnd <= 0) continue;
      if (mStart > cursor) {
        segments.push({ text: sp.text.slice(cursor, mStart), span: sp, highlight: false, active: false });
      }
      segments.push({
        text: sp.text.slice(mStart, mEnd),
        span: sp,
        highlight: true,
        active: (globalOffset + mi) === activeMatchGlobal,
      });
      cursor = mEnd;
    }
    if (cursor < spanLen) {
      segments.push({ text: sp.text.slice(cursor), span: sp, highlight: false, active: false });
    }
    charPos += spanLen;
  }
  return segments;
}
