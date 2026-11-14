#!/usr/bin/env python3
"""Diff indexed Rust production code; test-only scopes cannot trigger capture gates."""
import difflib
import re
import subprocess
import sys


def read_git(ref):
    result = subprocess.run(["git", "show", ref], capture_output=True, check=False)
    return result.stdout if result.returncode == 0 else b""


def lexical_code(source):
    """Hide comments and literals, retaining offsets for balanced Rust item bodies."""
    chars = list(source)
    pattern = re.compile(r'//[^\n]*|/\*|(?:br|r)\#*"|b?"|(?:b)?\'(?:\\.|[^\'\\\n])\'')
    pos = 0
    while match := pattern.search(source, pos):
        start, end = match.span()
        token = match.group()
        if token.startswith("//"):
            pass
        elif token == "/*":
            depth = 1
            while depth and end < len(source):
                if source.startswith("/*", end):
                    depth += 1
                    end += 2
                elif source.startswith("*/", end):
                    depth -= 1
                    end += 2
                else:
                    end += 1
        elif token.endswith('"'):
            if token.startswith(("r", "br")):
                suffix = '"' + "#" * token.count("#")
                close = source.find(suffix, end)
                end = len(source) if close < 0 else close + len(suffix)
            else:
                while end < len(source):
                    if source[end] == "\\":
                        end += 2
                    elif source[end] == '"':
                        end += 1
                        break
                    else:
                        end += 1
        for i in range(start, min(end, len(chars))):
            if chars[i] != "\n":
                chars[i] = " "
        pos = end
    return "".join(chars)


def production(source):
    code = lexical_code(source)
    item = re.compile(r'(?:#\s*\[[^\]]*\]\s*)+(?:pub(?:\([^)]*\))?\s+)?(?:async\s+)?(?:unsafe\s+)?(?:mod|fn)\s+\w+')
    excluded = set()
    for match in item.finditer(code):
        attrs = match.group()
        if not re.search(r'#\s*\[\s*(?:cfg\s*\(\s*test\s*\)|(?:\w+::)?test(?:\s*\([^]]*\))?)\s*\]', attrs):
            continue
        body = re.search(r"[{;]", code[match.end():])
        if body is None:
            continue
        end = match.end() + body.end()
        if body.group() == "{":
            depth = 1
            while depth and end < len(code):
                depth += (code[end] == "{") - (code[end] == "}")
                end += 1
            if depth:
                raise ValueError("unclosed test item; refusing to classify the staged diff")
        first = source.count("\n", 0, match.start())
        last = source.count("\n", 0, end)
        # Preserve production sharing the declaration/end line (e.g. one-line test modules).
        if code[code.rfind("\n", 0, match.start()) + 1:match.start()].strip():
            raise ValueError("mixed production/test declaration line")
        if code[end:code.find("\n", end) if "\n" in code[end:] else len(code)].strip():
            raise ValueError("mixed production/test closing line")
        excluded.update(range(first, last + 1))
    return [line for n, line in enumerate(source.splitlines(True)) if n not in excluded]


def squash(lines):
    """Whitespace outside string literals and rustfmt trailing commas (comma, newline, closer)
    removed, so a reflow compares equal while a literal or a `(x,)` tuple edit does not."""
    text = re.sub(r",(?=[ \t]*\n\s*[)\]}])", "", "".join(lines))
    parts = re.split(r'("(?:\\.|[^"\\])*")', text)
    return "".join(part if n % 2 else re.sub(r"\s+", "", part) for n, part in enumerate(parts))


ITEM_START = re.compile(
    r"\s*(?:#\s*\[[^\]]*\]\s*)*(?:pub(?:\([^)]*\))?\s+)?(?:async\s+)?(?:unsafe\s+)?"
    r"(?:fn|mod|impl|struct|enum|trait|const|static|type)\b")


def whole_item(lines):
    """True when the lines are complete items: they start at an item declaration and
    every brace closes inside them. A bare statement is never an item, so moving it
    out of its condition or past a sibling statement is not a pure move."""
    text = "".join(lines)
    if not ITEM_START.match(text):
        return False
    code = lexical_code(text)
    depth = 0
    for ch in code:
        depth += (ch == "{") - (ch == "}")
        if depth < 0:
            return False
    return depth == 0 and code.rstrip().endswith(("}", ";"))


def recorded_capture_staged():
    paths = subprocess.check_output(["git", "diff", "--cached", "--name-only", "--diff-filter=ACMR",
                                     "--", "src-tauri/src/fixtures/agent_prompts"]).decode().splitlines()
    for path in paths:
        if not path.endswith(".tcap"):
            continue
        data = read_git(":" + path)
        if not data.startswith(b"TUICCAP2\n") or len(data) < 13:
            continue
        rows = int.from_bytes(data[9:11], "little")
        cols = int.from_bytes(data[11:13], "little")
        pos, records, valid = 13, 0, rows > 0 and cols > 0
        while valid and pos < len(data):
            if len(data) - pos < 13 or data[pos] > 1:
                valid = False
                break
            size = int.from_bytes(data[pos + 9:pos + 13], "little")
            pos += 13 + size
            records += 1
            valid = pos <= len(data)
        provenance = read_git(":" + path[:-5] + ".md").decode(errors="replace").lower()
        if valid and records and "captur" in provenance and "sha-256" in provenance:
            return True
    return False


if __name__ == "__main__":
    if sys.argv[1] == "--capture":
        sys.exit(0 if recorded_capture_staged() else 1)
    path = sys.argv[1]
    old = read_git("HEAD:" + path)
    if not old and path.startswith("src-tauri/crates/tuic-terminal/src/"):
        old = read_git("HEAD:src-tauri/src/" + path.rsplit("/", 1)[1])
    new = read_git(":" + path)
    if old == new:
        sys.exit(0)
    before = production(old.decode())
    after = production(new.decode())
    # Give each production hunk its enclosing item, matching git's Rust hunk context.
    # Any item header counts, not only fn: a variant added to an enum that follows a
    # detection fn belongs to the enum (story 2801-8077).
    changes = []
    for group in difflib.SequenceMatcher(None, before, after).get_grouped_opcodes(0):
        first = group[0]
        context = ""
        for line in after[:first[3]] or before[:first[1]]:
            if re.match(r"\s*(?:pub(?:\([^)]*\))?\s+)?(?:async\s+)?(?:unsafe\s+)?"
                        r"(?:fn|mod|impl|struct|enum|union|trait)\b", line):
                context = line.strip()
        removed = [line for tag, i, j, a, b in group if tag in ("replace", "delete") for line in before[i:j]]
        added = [line for tag, i, j, a, b in group if tag in ("replace", "insert") for line in after[a:b]]
        changes.append([context, removed, added, first[3], group[-1][4]])
    # Formatting is not behaviour: drop a hunk whose text only reflowed, then drop a
    # removed whole item that reappears intact as an added item (a pure move).
    changes = [c for c in changes if squash(c[1]) != squash(c[2])]
    for removal in [c for c in changes if c[1] and not c[2]]:
        for insertion in [c for c in changes if c[2] and not c[1]]:
            if squash(removal[1]) == squash(insertion[2]) and whole_item(removal[1]):
                removal[1], insertion[2] = [], []
                break
    # A moved statement that stays in the diff reordered code: show the lines it
    # crossed, so a detection write it moved past is seen by the symbol scan.
    for removal in [c for c in changes if c[1] and not c[2]]:
        for insertion in [c for c in changes if c[2] and not c[1]]:
            if squash(removal[1]) == squash(insertion[2]):
                low, high = sorted((removal[3], insertion[3]))
                insertion[2] = insertion[2] + after[low:high]
                break
    for context, removed, added, *_ in changes:
        if not removed and not added:
            continue
        print("@@ production @@ " + context)
        for line in removed:
            print("-" + line.rstrip("\n"))
        for line in added:
            print("+" + line.rstrip("\n"))
