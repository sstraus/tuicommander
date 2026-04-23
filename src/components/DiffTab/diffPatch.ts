/** Extract individual hunks from a unified diff string.
 *  Each hunk includes the diff header (diff --git, ---, +++) and one @@ block. */
export function extractHunks(diff: string): string[] {
  const lines = diff.split("\n");
  // Collect the file header lines (everything before first @@)
  let headerEnd = 0;
  for (let i = 0; i < lines.length; i++) {
    if (lines[i].startsWith("@@")) { headerEnd = i; break; }
  }
  const fileHeader = lines.slice(0, headerEnd).join("\n");

  // Split on @@ boundaries
  const hunks: string[] = [];
  let hunkStart = -1;
  for (let i = headerEnd; i < lines.length; i++) {
    if (lines[i].startsWith("@@")) {
      if (hunkStart >= 0) {
        hunks.push(fileHeader + "\n" + lines.slice(hunkStart, i).join("\n"));
      }
      hunkStart = i;
    }
  }
  if (hunkStart >= 0) {
    hunks.push(fileHeader + "\n" + lines.slice(hunkStart).join("\n"));
  }
  return hunks;
}

export interface SelectedLineInfo {
  lineNumber: number;
  content: string;
  type: "+" | "-" | " ";
}

/**
 * Extract the content and line numbers of selected lines from a hunk.
 * Line numbers are from the new file (+side) for additions/context,
 * and from the old file (-side) for deletions.
 */
export function extractSelectedLines(
  fullDiff: string,
  hunkIdx: number,
  selectedLines: Set<number>,
): { lines: SelectedLineInfo[]; startLine: number; endLine: number } {
  if (selectedLines.size === 0) return { lines: [], startLine: 0, endLine: 0 };

  const allLines = fullDiff.split("\n");
  let headerEnd = 0;
  for (let i = 0; i < allLines.length; i++) {
    if (allLines[i].startsWith("@@")) { headerEnd = i; break; }
  }

  const hunkStarts: number[] = [];
  for (let i = headerEnd; i < allLines.length; i++) {
    if (allLines[i].startsWith("@@")) hunkStarts.push(i);
  }
  if (hunkIdx < 0 || hunkIdx >= hunkStarts.length) return { lines: [], startLine: 0, endLine: 0 };

  const hunkLineIdx = hunkStarts[hunkIdx];
  const hunkEndIdx = hunkIdx + 1 < hunkStarts.length ? hunkStarts[hunkIdx + 1] : allLines.length;
  const hunkHeader = allLines[hunkLineIdx];
  const bodyLines = allLines.slice(hunkLineIdx + 1, hunkEndIdx);

  if (bodyLines.length > 0 && bodyLines[bodyLines.length - 1] === "") bodyLines.pop();

  const headerMatch = hunkHeader.match(/@@ -(\d+)(?:,\d+)? \+(\d+)(?:,\d+)? @@/);
  let oldLine = headerMatch ? parseInt(headerMatch[1]) : 1;
  let newLine = headerMatch ? parseInt(headerMatch[2]) : 1;

  const result: SelectedLineInfo[] = [];
  let startLine = Infinity;
  let endLine = 0;

  for (let i = 0; i < bodyLines.length; i++) {
    const line = bodyLines[i];
    const isAdd = line.startsWith("+");
    const isDel = line.startsWith("-");

    if (selectedLines.has(i)) {
      const ln = isDel ? oldLine : newLine;
      const type = isAdd ? "+" : isDel ? "-" : " ";
      result.push({ lineNumber: ln, content: line.slice(1), type: type as "+" | "-" | " " });
      if (ln < startLine) startLine = ln;
      if (ln > endLine) endLine = ln;
    }

    if (isAdd) newLine++;
    else if (isDel) oldLine++;
    else { oldLine++; newLine++; }
  }

  if (result.length === 0) return { lines: [], startLine: 0, endLine: 0 };
  return { lines: result, startLine, endLine };
}

export function buildPartialPatch(
  fullDiff: string,
  hunkIdx: number,
  selectedLines: Set<number>,
): string {
  if (selectedLines.size === 0) return "";

  const allLines = fullDiff.split("\n");

  // Extract file header (everything before first @@)
  let headerEnd = 0;
  for (let i = 0; i < allLines.length; i++) {
    if (allLines[i].startsWith("@@")) { headerEnd = i; break; }
  }
  const fileHeader = allLines.slice(0, headerEnd);

  // Find the target hunk's @@ line and body
  const hunkStarts: number[] = [];
  for (let i = headerEnd; i < allLines.length; i++) {
    if (allLines[i].startsWith("@@")) hunkStarts.push(i);
  }
  if (hunkIdx < 0 || hunkIdx >= hunkStarts.length) return "";

  const hunkLineIdx = hunkStarts[hunkIdx];
  const hunkEndIdx = hunkIdx + 1 < hunkStarts.length ? hunkStarts[hunkIdx + 1] : allLines.length;
  const hunkHeader = allLines[hunkLineIdx];
  const bodyLines = allLines.slice(hunkLineIdx + 1, hunkEndIdx);

  // Remove trailing empty line if present (artifact of split)
  if (bodyLines.length > 0 && bodyLines[bodyLines.length - 1] === "") {
    bodyLines.pop();
  }

  // Parse the original @@ header to get start positions
  const headerMatch = hunkHeader.match(/@@ -(\d+)(?:,\d+)? \+(\d+)(?:,\d+)? @@/);
  const newStart = headerMatch ? parseInt(headerMatch[2]) : 1;

  // Build partial hunk body
  const patchBody: string[] = [];
  let oldCount = 0;
  let newCount = 0;

  for (let i = 0; i < bodyLines.length; i++) {
    const line = bodyLines[i];
    const isAddition = line.startsWith("+");
    const isDeletion = line.startsWith("-");
    const isSelected = selectedLines.has(i);

    if (!isAddition && !isDeletion) {
      // Context line — always keep
      patchBody.push(line);
      oldCount++;
      newCount++;
    } else if (isAddition && isSelected) {
      // Selected addition — keep as + (reverse will remove it)
      patchBody.push(line);
      newCount++;
    } else if (isAddition && !isSelected) {
      // Unselected addition — convert to context (it's in the file, keep it)
      patchBody.push(" " + line.slice(1));
      oldCount++;
      newCount++;
    } else if (isDeletion && isSelected) {
      // Selected deletion — keep as - (reverse will restore it)
      patchBody.push(line);
      oldCount++;
    }
    // Unselected deletion — drop entirely (already removed, stay removed)
  }

  // Check if there are any actual changes in the patch
  const hasChanges = patchBody.some((l) => l.startsWith("+") || l.startsWith("-"));
  if (!hasChanges) return "";

  // Build new @@ header
  const newHeader = `@@ -${newStart},${oldCount} +${newStart},${newCount} @@`;

  return [...fileHeader, newHeader, ...patchBody].join("\n");
}
