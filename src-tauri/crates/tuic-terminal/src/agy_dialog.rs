use crate::output_parser::{ChoiceOption, ChoiceSelectionMode, ParsedEvent};

/// agy 1.5.10 pickers: workspace trust, command approval and the native
/// question form (recorded in `agy-1.5.10-*.tcap`). All three end with a
/// navigation footer that starts at column 2, followed by at most one hint and
/// mode row. Enter selects the highlighted row; number keys are not recorded.
///
/// DEFERRED (2026-11-22) — an option label that wraps at a narrow width is not
/// recorded. Such a form returns `None` here and the session shows no wait.
pub fn parse_agy_dialog(screen_rows: &[String]) -> Option<ParsedEvent> {
    lazy_static::lazy_static! {
        static ref OPTION_RE: regex::Regex =
            regex::Regex::new(r"^(> |  )(?:([1-9]\d*)\. )?(\S.*?)\s*$").unwrap();
        static ref QUESTION_RE: regex::Regex =
            regex::Regex::new(r"^Question [1-9]\d*/[1-9]\d*: (\S.*?)\s*$").unwrap();
    }
    const TRUST: &str = "  ↑/↓ Navigate · enter Confirm";
    const APPROVAL: &str = "  ↑/↓ Navigate · tab Amend · ctrl+g edit/expand command";
    const QUESTION: &str = "  ↑/↓ Navigate · enter Select · esc Skip";

    let blank = |index: usize| screen_rows[index].trim().is_empty();
    let content_end = screen_rows.iter().rposition(|row| !row.trim().is_empty())? + 1;
    let footer = (content_end.saturating_sub(2)..content_end)
        .rev()
        .find(|&index| matches!(screen_rows[index].trim_end(), TRUST | APPROVAL | QUESTION))?;
    let kind = screen_rows[footer].trim_end();

    let mut end = footer;
    while end > 0 && blank(end - 1) {
        end -= 1;
    }
    if end == footer {
        return None;
    }
    // The approval question sits directly above its options, so the option
    // block ends at the first row that is not an option.
    let mut first = end;
    let mut options = Vec::new();
    while first > 0 {
        let Some(caps) = OPTION_RE.captures(&screen_rows[first - 1]) else {
            break;
        };
        if caps.get(2).is_some() != (kind != TRUST) {
            break;
        }
        let label = caps[3].to_owned();
        options.push(ChoiceOption {
            key: caps
                .get(2)
                .map_or_else(String::new, |key| key.as_str().to_owned()),
            highlighted: &caps[1] == "> ",
            destructive: label == "No" || label.starts_with("No, "),
            label,
            hint: None,
        });
        first -= 1;
    }
    options.reverse();
    for (index, option) in options.iter_mut().enumerate() {
        let key = (index + 1).to_string();
        if option.key.is_empty() {
            option.key = key;
        } else if option.key != key {
            return None;
        }
    }
    if options.len() < 2 || options.iter().filter(|option| option.highlighted).count() != 1 {
        return None;
    }

    // Paragraphs above the options, nearest first.
    let mut cursor = first;
    let mut paragraph = || {
        while cursor > 0 && blank(cursor - 1) {
            cursor -= 1;
        }
        let end = cursor;
        while cursor > 0 && !blank(cursor - 1) {
            cursor -= 1;
        }
        (cursor < end).then(|| &screen_rows[cursor..end])
    };
    let nearest = paragraph()?;
    let title = if kind == QUESTION {
        let [row] = nearest else { return None };
        QUESTION_RE.captures(row)?[1].to_owned()
    } else if kind == APPROVAL {
        let [question] = nearest else { return None };
        if !question.ends_with('?') || question.starts_with(' ') {
            return None;
        }
        let [header, command @ ..] = paragraph()? else {
            return None;
        };
        if header.trim_end() != "Requesting permission for:" || command.is_empty() {
            return None;
        }
        let command = command
            .iter()
            .map(|row| row.trim())
            .collect::<Vec<_>>()
            .join(" ");
        format!("{} {command}", question.trim_end())
    } else {
        // Trust: `Do you trust …?`, a blank row, then one explanation row.
        let [_explanation] = nearest else { return None };
        let [question] = paragraph()? else {
            return None;
        };
        if !question.starts_with("Do you trust ") || !question.trim_end().ends_with('?') {
            return None;
        }
        question.trim_end().to_owned()
    };
    Some(ParsedEvent::ChoicePrompt {
        title,
        options,
        selection_mode: Some(ChoiceSelectionMode::NavigateEnter),
        dismiss_key: (kind != TRUST).then(|| "cancel".to_owned()),
        amend_key: (kind == APPROVAL).then(|| "amend".to_owned()),
    })
}
