//! Current-document coordinates for saved text edits, independent of repository history.
use gix::diff::blob::{Algorithm, InternedInput, diff_with_slider_heuristics};

/// Compact one-based inclusive ranges; deletions name the surviving adjacent line.
pub fn changed_line_ranges(before: &str, after: &str) -> Option<String> {
    let input = InternedInput::new(before, after);
    let diff = diff_with_slider_heuristics(Algorithm::Myers, &input);
    let last_line = u32::try_from(after.lines().count())
        .unwrap_or(u32::MAX)
        .max(1);
    let mut ranges: Vec<(u32, u32)> = Vec::new();
    for hunk in diff.hunks() {
        // A deletion has no new lines: include its surviving neighbour, including at EOF.
        let start = if hunk.after.is_empty() {
            hunk.after.start.max(1)
        } else {
            hunk.after.start.saturating_add(1)
        }
        .min(last_line);
        let end = hunk.after.end.max(start).min(last_line);
        if let Some(previous) = ranges
            .last_mut()
            .filter(|previous| start <= previous.1.saturating_add(1))
        {
            previous.1 = previous.1.max(end);
        } else {
            ranges.push((start, end));
        }
    }
    if ranges.is_empty() {
        return None;
    }
    Some(
        ranges
            .into_iter()
            .map(|(start, end)| {
                if start == end {
                    start.to_string()
                } else {
                    format!("{start}-{end}")
                }
            })
            .collect::<Vec<_>>()
            .join(", "),
    )
}
