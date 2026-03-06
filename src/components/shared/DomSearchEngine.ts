/** Options for text search in DOM content */
export interface SearchOptions {
  caseSensitive: boolean;
  regex: boolean;
  wholeWord: boolean;
}

/** Abstract search engine interface */
export interface SearchEngine {
  search(term: string, opts: SearchOptions): number;
  next(): number;
  prev(): number;
  clear(): void;
  activeIndex(): number;
  matchCount(): number;
}

/** Maximum number of matches to highlight (performance cap) */
const MAX_HIGHLIGHTS = 1000;

const MATCH_CLASS = "search-match";
const ACTIVE_CLASS = "search-match-active";

/** Inline elements whose text should be concatenated for cross-element matching */
const INLINE_TAGS = new Set([
  "A", "ABBR", "B", "BDI", "BDO", "CITE", "CODE", "DEL", "DFN", "EM",
  "I", "INS", "KBD", "MARK", "Q", "S", "SAMP", "SMALL", "SPAN", "STRONG",
  "SUB", "SUP", "TIME", "U", "VAR",
]);

interface TextNodeInfo {
  node: Text;
  /** Absolute start offset in the run's concatenated text */
  start: number;
  /** Length of this text node */
  length: number;
}

interface TextRun {
  nodes: TextNodeInfo[];
  text: string;
}

/** Collect runs of adjacent inline text nodes.
 *  Block-level boundaries (div, p, li, etc.) break runs. */
function collectTextRuns(root: HTMLElement): TextRun[] {
  const runs: TextRun[] = [];
  let currentNodes: TextNodeInfo[] = [];
  let offset = 0;

  const walker = document.createTreeWalker(root, NodeFilter.SHOW_TEXT | NodeFilter.SHOW_ELEMENT);
  let node = walker.nextNode();

  while (node) {
    if (node.nodeType === Node.TEXT_NODE) {
      const text = node as Text;
      const content = text.textContent || "";
      if (content.length > 0) {
        currentNodes.push({ node: text, start: offset, length: content.length });
        offset += content.length;
      }
    } else if (node.nodeType === Node.ELEMENT_NODE) {
      const el = node as Element;
      if (!INLINE_TAGS.has(el.tagName)) {
        if (currentNodes.length > 0) {
          runs.push({ nodes: currentNodes, text: currentNodes.map((n) => n.node.textContent || "").join("") });
          currentNodes = [];
          offset = 0;
        }
      }
    }
    node = walker.nextNode();
  }

  if (currentNodes.length > 0) {
    runs.push({ nodes: currentNodes, text: currentNodes.map((n) => n.node.textContent || "").join("") });
  }

  return runs;
}

interface MatchRange {
  /** Absolute start in run text */
  start: number;
  /** Length of match */
  length: number;
  /** Global match index (for active tracking) */
  globalIndex: number;
}

/** For a single text node, determine which match segments overlap with it
 *  and replace it with [text, mark, text, mark, text] fragments. */
function replaceNodeWithHighlights(
  nodeInfo: TextNodeInfo,
  matches: MatchRange[],
  activeGlobalIndex: number,
): void {
  const { node, start: nodeStart, length: nodeLen } = nodeInfo;
  const nodeEnd = nodeStart + nodeLen;
  const nodeText = node.textContent || "";
  const parent = node.parentNode;
  if (!parent) return;

  // Collect segments that overlap this node
  const segments: { localStart: number; localEnd: number; globalIndex: number }[] = [];
  for (const m of matches) {
    const mEnd = m.start + m.length;
    // Overlap check
    if (m.start < nodeEnd && mEnd > nodeStart) {
      const localStart = Math.max(0, m.start - nodeStart);
      const localEnd = Math.min(nodeLen, mEnd - nodeStart);
      segments.push({ localStart, localEnd, globalIndex: m.globalIndex });
    }
  }

  if (segments.length === 0) return;

  // Build replacement fragments: [text, mark, text, mark, text]
  const frag = document.createDocumentFragment();
  let cursor = 0;

  for (const seg of segments) {
    // Text before this segment
    if (seg.localStart > cursor) {
      frag.appendChild(document.createTextNode(nodeText.slice(cursor, seg.localStart)));
    }
    // The highlighted segment
    const mark = document.createElement("mark");
    const isActive = seg.globalIndex === activeGlobalIndex;
    mark.className = isActive ? `${MATCH_CLASS} ${ACTIVE_CLASS}` : MATCH_CLASS;
    mark.textContent = nodeText.slice(seg.localStart, seg.localEnd);
    frag.appendChild(mark);
    cursor = seg.localEnd;
  }

  // Text after last segment
  if (cursor < nodeLen) {
    frag.appendChild(document.createTextNode(nodeText.slice(cursor)));
  }

  parent.replaceChild(frag, node);
}

/** Search engine that operates on rendered HTML DOM content */
export class DomSearchEngine implements SearchEngine {
  private container: HTMLElement;
  private _activeIndex = -1;
  private _matchCount = 0;

  constructor(container: HTMLElement) {
    this.container = container;
  }

  search(term: string, opts: SearchOptions): number {
    this.clear();

    if (!term) return 0;

    let pattern: RegExp;
    try {
      let src = opts.regex ? term : term.replace(/[.*+?^${}()|[\]\\]/g, "\\$&");
      if (opts.wholeWord) src = `\\b${src}\\b`;
      const flags = opts.caseSensitive ? "g" : "gi";
      pattern = new RegExp(src, flags);
    } catch {
      return 0;
    }

    const runs = collectTextRuns(this.container);

    // Phase 1: Find all matches across all runs
    const allMatches: { runIdx: number; start: number; length: number }[] = [];
    for (let ri = 0; ri < runs.length; ri++) {
      const run = runs[ri];
      let m: RegExpExecArray | null;
      pattern.lastIndex = 0;
      while ((m = pattern.exec(run.text)) !== null) {
        if (m[0].length === 0) { pattern.lastIndex++; continue; }
        allMatches.push({ runIdx: ri, start: m.index, length: m[0].length });
      }
    }

    this._matchCount = allMatches.length;
    if (this._matchCount === 0) return 0;

    // Phase 2: Apply highlights (capped)
    const toHighlight = allMatches.slice(0, MAX_HIGHLIGHTS);

    // Group by run index
    const byRun = new Map<number, MatchRange[]>();
    for (let i = 0; i < toHighlight.length; i++) {
      const { runIdx, start, length } = toHighlight[i];
      if (!byRun.has(runIdx)) byRun.set(runIdx, []);
      byRun.get(runIdx)!.push({ start, length, globalIndex: i });
    }

    // For each run, process text nodes in REVERSE order to preserve earlier sibling references
    for (const [runIdx, matches] of byRun) {
      const run = runs[runIdx];
      // Process nodes from last to first
      for (let ni = run.nodes.length - 1; ni >= 0; ni--) {
        replaceNodeWithHighlights(run.nodes[ni], matches, 0);
      }
    }

    this._activeIndex = 0;
    this.autoExpandDetails();

    return this._matchCount;
  }

  next(): number {
    if (this._matchCount === 0) return -1;
    this.setActive(this._activeIndex, false);
    this._activeIndex = (this._activeIndex + 1) % this._matchCount;
    this.setActive(this._activeIndex, true);
    this.scrollToActive();
    return this._activeIndex;
  }

  prev(): number {
    if (this._matchCount === 0) return -1;
    this.setActive(this._activeIndex, false);
    this._activeIndex = (this._activeIndex - 1 + this._matchCount) % this._matchCount;
    this.setActive(this._activeIndex, true);
    this.scrollToActive();
    return this._activeIndex;
  }

  clear(): void {
    const marks = this.container.querySelectorAll(`mark.${MATCH_CLASS}`);
    for (const mark of marks) {
      const parent = mark.parentNode;
      if (!parent) continue;
      while (mark.firstChild) {
        parent.insertBefore(mark.firstChild, mark);
      }
      parent.removeChild(mark);
    }
    this.container.normalize();

    this._activeIndex = -1;
    this._matchCount = 0;
  }

  activeIndex(): number {
    return this._activeIndex;
  }

  matchCount(): number {
    return this._matchCount;
  }

  /** Get all mark elements in document order */
  private getOrderedMarks(): HTMLElement[] {
    return Array.from(this.container.querySelectorAll(`mark.${MATCH_CLASS}`));
  }

  private setActive(index: number, active: boolean): void {
    const marks = this.getOrderedMarks();
    if (index >= 0 && index < marks.length) {
      if (active) {
        marks[index].classList.add(ACTIVE_CLASS);
      } else {
        marks[index].classList.remove(ACTIVE_CLASS);
      }
    }
  }

  private scrollToActive(): void {
    const marks = this.getOrderedMarks();
    const active = marks[this._activeIndex];
    if (active) {
      active.scrollIntoView({ block: "center", behavior: "smooth" });
    }
  }

  /** Auto-expand <details> ancestors of matches */
  private autoExpandDetails(): void {
    const marks = this.container.querySelectorAll(`mark.${MATCH_CLASS}`);
    for (const mark of marks) {
      let el: HTMLElement | null = mark.parentElement;
      while (el && el !== this.container) {
        if (el.tagName === "DETAILS") {
          el.setAttribute("open", "");
        }
        el = el.parentElement;
      }
    }
  }
}
