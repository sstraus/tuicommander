//! TUIC protocol markers in an answer filed to the journal.
//!
//! Agents prefix answer sentences with `💬`, open a turn with `intent: … (Title)`
//! and close it with `suggest: [ A | B | C ]`, sometimes after the connection
//! acknowledgement. The terminal parser consumes them; a journal answer is read
//! as prose, so they are removed. Fenced and indented code keeps them: an
//! answer that explains the protocol quotes them on purpose.

/// The answer marker, removed with the spaces that follow it.
const ANSWER: char = '💬';

/// The answer text without protocol markers.
pub(crate) fn strip(text: &str) -> String {
    let lines: Vec<&str> = text.lines().collect();
    let last = lines.iter().rposition(|l| !l.trim().is_empty());
    let mut out: Vec<String> = Vec::with_capacity(lines.len());
    let mut fence: Option<char> = None;
    for (index, line) in lines.iter().enumerate() {
        let trimmed = line.trim_start();
        if let Some(marker) = fence_marker(line) {
            match fence {
                None => fence = Some(marker),
                Some(open) if open == marker => fence = None,
                Some(_) => {}
            }
            out.push((*line).to_owned());
            continue;
        }
        if fence.is_some() || line.starts_with("    ") || line.starts_with('\t') {
            out.push((*line).to_owned());
            continue;
        }
        if is_ack(trimmed) || is_intent(trimmed) || is_suggest(trimmed) {
            continue;
        }
        let line = if Some(index) == last {
            strip_trailing_suggest(line)
        } else {
            line
        };
        out.push(strip_answer_marker(line));
    }
    // Removed lines leave blank runs at the edges.
    out.join("\n").trim().to_owned()
}

fn fence_marker(line: &str) -> Option<char> {
    let indent = line.len() - line.trim_start_matches(' ').len();
    let rest = &line[indent..];
    (indent <= 3)
        .then(|| rest.chars().next())
        .flatten()
        .filter(|c| (*c == '`' || *c == '~') && rest.starts_with(&c.to_string().repeat(3)))
}

/// A list bullet or a terminal bullet before a marker.
fn after_bullet(line: &str) -> &str {
    let line = line.trim_start();
    for bullet in ["- ", "* ", "● ", "⏺ ", "• "] {
        if let Some(rest) = line.strip_prefix(bullet) {
            return rest.trim_start();
        }
    }
    line
}

fn is_ack(line: &str) -> bool {
    line.starts_with("TUICommander v") && line.trim_end().ends_with("is connected.")
}

fn is_intent(line: &str) -> bool {
    after_bullet(line)
        .strip_prefix("intent:")
        .is_some_and(|rest| rest.starts_with([' ', '\t']))
}

fn is_suggest(line: &str) -> bool {
    suggest_items(after_bullet(line).trim_end()).is_some()
}

/// `suggest: [ A | B ]` with two to four items, the whole string.
fn suggest_items(s: &str) -> Option<usize> {
    let inner = s
        .strip_prefix("suggest:")?
        .trim_start()
        .strip_prefix('[')?
        .strip_suffix(']')?;
    if inner.contains(['[', ']']) {
        return None;
    }
    let count = inner.split('|').filter(|i| !i.trim().is_empty()).count();
    (2..=4).contains(&count).then_some(count)
}

/// The last line may end with the suggestion on the same row as prose.
fn strip_trailing_suggest(line: &str) -> &str {
    let trimmed = line.trim_end();
    match trimmed.rfind(" suggest:") {
        Some(at) if suggest_items(trimmed[at + 1..].trim_end()).is_some() => {
            trimmed[..at].trim_end()
        }
        _ => line,
    }
}

/// `💬 text` → `text`, keeping a list prefix: `- 💬 text` → `- text`.
fn strip_answer_marker(line: &str) -> String {
    let body = after_bullet(line);
    let Some(rest) = body.strip_prefix(ANSWER) else {
        return line.to_owned();
    };
    let prefix = &line[..line.len() - body.len()];
    format!("{prefix}{}", rest.trim_start())
}

#[cfg(test)]
#[path = "markers_tests.rs"]
mod tests;
