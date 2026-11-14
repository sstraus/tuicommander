//! The current native tool of one agent PTY (story 2636-5551).
//!
//! A native owner (Claude, Codex and Grok hook receipts, ego `agent-event`
//! frames, the managed Pi extension and OpenCode plugin `agent-tool` frames)
//! starts an item with its native tool call id and name, and ends that id when
//! the tool completes or fails. The readout is the newest item still live.
//!
//! The readout is metadata: it never sets busy, idle or awaiting. An end only
//! removes its own id, so a late completion of an old call cannot clear a newer
//! tool, and an ended id never starts again. Turn and session ends clear all.
//! Only the tool name is kept, cut to [`MAX_LABEL_CHARS`] characters without
//! control characters; tool arguments are never read.

use std::collections::VecDeque;

/// Live tool calls kept at most; the oldest is dropped first.
const MAX_LIVE: usize = 16;
/// Ended ids kept, so a duplicate or late start cannot open them again.
const MAX_RETIRED: usize = 64;
/// Characters kept of a tool name.
pub(crate) const MAX_LABEL_CHARS: usize = 64;
/// Bytes accepted of a native tool call id.
const MAX_ID_BYTES: usize = 256;

/// A tool name fit for display: control characters removed, whitespace
/// collapsed, at most [`MAX_LABEL_CHARS`] characters. `None` when empty.
fn label(name: &str) -> Option<String> {
    let text = name
        .split(|c: char| c.is_whitespace() || c.is_control())
        .filter(|part| !part.is_empty())
        .collect::<Vec<_>>()
        .join(" ");
    let text: String = text.chars().take(MAX_LABEL_CHARS).collect();
    (!text.is_empty()).then_some(text)
}

#[derive(Default)]
pub(crate) struct ToolReadout {
    /// `(id, label)` in start order.
    live: VecDeque<(String, String)>,
    retired: VecDeque<String>,
}

impl ToolReadout {
    /// Start the native call `id` named `name`. A duplicate, an ended id, an
    /// empty or oversized id, or an unusable name changes nothing.
    pub(crate) fn start(&mut self, id: &str, name: &str) {
        if id.is_empty()
            || id.len() > MAX_ID_BYTES
            || self.retired.iter().any(|retired| retired == id)
            || self.live.iter().any(|(live, _)| live == id)
        {
            return;
        }
        let Some(label) = label(name) else {
            return;
        };
        if self.live.len() == MAX_LIVE {
            self.live.pop_front();
        }
        self.live.push_back((id.to_owned(), label));
    }

    /// End the native call `id`; any other live call stays.
    pub(crate) fn end(&mut self, id: &str) {
        if id.is_empty() || id.len() > MAX_ID_BYTES {
            return;
        }
        self.live.retain(|(live, _)| live != id);
        if !self.retired.iter().any(|retired| retired == id) {
            if self.retired.len() == MAX_RETIRED {
                self.retired.pop_front();
            }
            self.retired.push_back(id.to_owned());
        }
    }

    /// Turn, session or process end: no call of it is live any more.
    pub(crate) fn clear(&mut self) {
        let ended: Vec<String> = self.live.drain(..).map(|(id, _)| id).collect();
        for id in ended {
            self.end(&id);
        }
    }

    /// The newest live tool name.
    pub(crate) fn current(&self) -> Option<&str> {
        self.live.back().map(|(_, label)| label.as_str())
    }
}

/// One `agent-tool` frame of the managed Pi extension or OpenCode plugin:
/// `<TUIC_SESSION>:<base64 JSON {"event":"start"|"end","id":…,"name":…}>`.
/// A frame of another TUIC session or of an unknown shape changes nothing.
pub(crate) fn receive_frame(readout: &mut ToolReadout, payload: &str, owner: Option<&str>) {
    use base64::Engine;

    #[derive(serde::Deserialize)]
    struct Frame {
        event: String,
        id: String,
        name: Option<String>,
    }

    let Some((sender, encoded)) = payload.split_once(':') else {
        return;
    };
    if owner != Some(sender) || encoded.len() > 4096 {
        return;
    }
    let Some(frame) = base64::engine::general_purpose::STANDARD
        .decode(encoded)
        .ok()
        .and_then(|bytes| serde_json::from_slice::<Frame>(&bytes).ok())
    else {
        return;
    };
    match (frame.event.as_str(), frame.name) {
        ("start", Some(name)) => readout.start(&frame.id, &name),
        ("end", _) => readout.end(&frame.id),
        _ => {}
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // Catches: a completion of an old call that arrives after a newer call
    // started removes the newer readout, or reopens the old one.
    #[test]
    fn late_tool_end_does_not_clear_new_tool() {
        let mut tools = ToolReadout::default();
        tools.start("a", "Bash");
        tools.clear(); // the turn of `a` ended without its completion
        tools.start("b", "Read");
        tools.end("a");
        assert_eq!(tools.current(), Some("Read"));
        tools.start("a", "Bash");
        assert_eq!(tools.current(), Some("Read"), "an ended id started again");
    }

    // Catches: an end of one parallel call clears the other, or the readout
    // does not fall back to the call that is still live.
    #[test]
    fn parallel_calls_end_only_their_own_item() {
        let mut tools = ToolReadout::default();
        tools.start("a", "Bash");
        tools.start("b", "Grep");
        tools.end("b");
        assert_eq!(tools.current(), Some("Bash"));
        tools.start("c", "Edit");
        tools.end("a");
        assert_eq!(tools.current(), Some("Edit"));
        tools.end("c");
        assert_eq!(tools.current(), None);
    }

    // Catches: a label with arguments, escapes or unbounded length reaches
    // clients, or a flood of starts with no end grows without limit.
    #[test]
    fn labels_and_live_calls_are_bounded() {
        assert_eq!(label("  run\x1b[2J\nnow  ").as_deref(), Some("run [2J now"));
        assert_eq!(label("\x07 \t"), None);
        let long = "x".repeat(500);
        assert_eq!(label(&long).unwrap().chars().count(), MAX_LABEL_CHARS);
        let mut tools = ToolReadout::default();
        for i in 0..MAX_LIVE + 4 {
            tools.start(&i.to_string(), &format!("t{i}"));
        }
        assert_eq!(tools.live.len(), MAX_LIVE);
        assert_eq!(tools.current(), Some("t19"));
        tools.start(&"i".repeat(MAX_ID_BYTES + 1), "Big");
        tools.start("", "Empty");
        assert_eq!(tools.current(), Some("t19"));
    }

    // Catches: an agent-tool frame of another TUIC session, a malformed one,
    // or an end of another id changes the readout.
    #[test]
    fn agent_tool_frames_need_the_owner_and_a_known_shape() {
        use base64::Engine;
        let encode = |json: &str| base64::engine::general_purpose::STANDARD.encode(json);
        let start = format!(
            "tab:{}",
            encode(r#"{"event":"start","id":"call_1","name":"bash"}"#)
        );
        let mut tools = ToolReadout::default();
        receive_frame(&mut tools, &start, Some("other"));
        assert_eq!(tools.current(), None);
        receive_frame(&mut tools, &start, Some("tab"));
        assert_eq!(tools.current(), Some("bash"));
        for frame in [
            format!("tab:{}", encode(r#"{"event":"end","id":"call_2"}"#)),
            format!("tab:{}", encode(r#"{"event":"progress","id":"call_1"}"#)),
            "tab:not base64!".to_owned(),
            "no-owner".to_owned(),
        ] {
            receive_frame(&mut tools, &frame, Some("tab"));
        }
        assert_eq!(tools.current(), Some("bash"));
        receive_frame(
            &mut tools,
            &format!("tab:{}", encode(r#"{"event":"end","id":"call_1"}"#)),
            Some("tab"),
        );
        assert_eq!(tools.current(), None);
    }
}
