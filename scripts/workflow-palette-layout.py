#!/usr/bin/env python3
"""Native palette regression. Open Designer on an isolated agent-test instance first."""
import argparse
import importlib.util
import json
from pathlib import Path

TEST = "workflow_palette_labels_fit_without_handle_overlap"

# Reuse the native observer's nonce-correlated WebKit evaluation, not a DOM mock.
spec = importlib.util.spec_from_file_location(
    "native_observer", Path(__file__).with_name("agent-test-visible.py"))
observer = importlib.util.module_from_spec(spec)
spec.loader.exec_module(observer)

PROBE = r"""
const sidebar=document.querySelector('[aria-label="Workflow drafts"]');
if(!sidebar)throw new Error('Open Workflow Designer before the palette check');
const heading=[...sidebar.querySelectorAll('h4')].find(n=>n.textContent.trim()==='Nodes');
sidebar.scrollTop+=heading.getBoundingClientRect().top-sidebar.getBoundingClientRect().top;
// Reading the rectangles flushes layout even when an occluded WebView pauses rAF.
return {viewport:{width:innerWidth,height:innerHeight},sidebar:sidebar.getBoundingClientRect().toJSON(),
 entries:[...sidebar.querySelectorAll('button')].filter(b=>b.textContent.trim().startsWith('Add ')).map(button=>{
 const range=document.createRange();range.selectNodeContents(button);
 const row=button.parentElement,handle=row.querySelector('[data-palette-handle]');
 return {label:button.textContent.trim(),button:button.getBoundingClientRect().toJSON(),
 text:range.getBoundingClientRect().toJSON(),row:row.getBoundingClientRect().toJSON(),
 handle:handle?.getBoundingClientRect().toJSON()??null};
 })};
"""


def overlaps(a, b):
    return (a["left"] < b["right"] and a["right"] > b["left"]
            and a["top"] < b["bottom"] and a["bottom"] > b["top"])


def workflow_palette_labels_fit_without_handle_overlap(socket, evidence):
    # Catches: real text exceeding narrow buttons, handles painted over text,
    # inconsistent typed/preset rows, or controls entering the scrollbar band.
    evidence.mkdir(parents=True, exist_ok=True)
    result = observer.evaluate(socket, PROBE, evidence)
    entries = result["entries"]
    failures = []
    if not entries:
        failures.append("palette has no Add entries")
    for item in entries:
        button, text, handle = item["button"], item["text"], item["handle"]
        if (text["left"] < button["left"] - .5 or text["right"] > button["right"] + .5
                or text["top"] < button["top"] - .5 or text["bottom"] > button["bottom"] + .5):
            failures.append(f"{item['label']}: text exceeds button")
        if handle and overlaps(text, handle):
            failures.append(f"{item['label']}: text overlaps handle")
        if entries and (abs(button["left"] - entries[0]["button"]["left"]) > .5
                        or abs(button["right"] - entries[0]["button"]["right"]) > .5):
            failures.append(f"{item['label']}: inconsistent row columns")
        # The native overlay scrollbar must have a clear 16px edge band.
        if button["right"] > result["sidebar"]["right"] - 16:
            failures.append(f"{item['label']}: button enters scrollbar edge band")
    result["failures"] = failures
    (evidence / "palette-geometry.json").write_text(json.dumps(result, indent=2))
    if failures:
        raise AssertionError("; ".join(failures))
    print(f"PASS {TEST}: {len(entries)} entries; evidence={evidence}")


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--socket", required=True)
    parser.add_argument("--evidence-dir", type=Path, required=True)
    args = parser.parse_args()
    workflow_palette_labels_fit_without_handle_overlap(args.socket, args.evidence_dir)


if __name__ == "__main__":
    main()
