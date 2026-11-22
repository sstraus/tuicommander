//! The classification header a fork puts on the first line of its answer.
//!
//! `[answer|action|unknown] <short title>`. The header is the only structure
//! asked of the model: a JSON schema or a tool call would change the cached
//! prefix (see `plans/discussion.md`).

use serde::{Deserialize, Serialize};

/// Longest title kept from a header or a fallback first line, in chars.
const MAX_TITLE_CHARS: usize = 80;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub(crate) enum Category {
    /// A plain explanation.
    Answer,
    /// The main agent should change something.
    Action,
    /// The conversation does not contain the answer.
    Unknown,
}

impl Category {
    pub(crate) fn as_str(self) -> &'static str {
        match self {
            Category::Answer => "answer",
            Category::Action => "action",
            Category::Unknown => "unknown",
        }
    }

    fn parse(word: &str) -> Option<Self> {
        match word.trim().to_ascii_lowercase().as_str() {
            "answer" => Some(Category::Answer),
            "action" => Some(Category::Action),
            "unknown" => Some(Category::Unknown),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Classified {
    pub category: Category,
    pub title: String,
    /// The answer without the header line.
    pub body: String,
    /// False when the header was missing or malformed and the fallback ran.
    pub header_found: bool,
}

/// Split the header from an answer. A missing or malformed header falls back
/// to `answer`, with the first non-empty line as the title and the whole text
/// as the body; the caller logs `header_found == false`.
pub(crate) fn classify(text: &str) -> Classified {
    let text = text.trim();
    let (first, rest) = text.split_once('\n').unwrap_or((text, ""));
    if let Some((category, title)) = parse_header(first)
        && !title.is_empty()
    {
        return Classified {
            category,
            title: truncate(title),
            body: rest.trim().to_owned(),
            header_found: true,
        };
    }
    Classified {
        category: Category::Answer,
        title: truncate(strip_markup(first)),
        body: text.to_owned(),
        header_found: false,
    }
}

fn parse_header(line: &str) -> Option<(Category, &str)> {
    // Models sometimes wrap the line in bold or a code span.
    let line = strip_markup(line);
    let inner = line.strip_prefix('[')?;
    let (word, title) = inner.split_once(']')?;
    Some((Category::parse(word)?, strip_markup(title)))
}

fn strip_markup(line: &str) -> &str {
    line.trim()
        .trim_matches(|c| c == '*' || c == '`' || c == '#')
        .trim()
}

fn truncate(title: &str) -> String {
    match title.char_indices().nth(MAX_TITLE_CHARS) {
        Some((end, _)) => format!("{}…", &title[..end]),
        None => title.to_owned(),
    }
}
