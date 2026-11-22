use super::strip;

// Catches: a captured answer shown in the journal with the terminal protocol
// still in it (the 💬 prefix, the intent line, the trailing suggestion).
#[test]
fn removes_the_answer_intent_and_suggest_markers_of_a_reply() {
    let reply = "intent: Explaining the gap (Capture gap)\n\
                 💬 No technical limit.\n\
                 \n\
                 - **Codex:** already bound.\n\
                 - 💬 Grok too.\n\
                 \n\
                 suggest: [ Extend capture | Unify shortcut | Restart ]";
    assert_eq!(
        strip(reply),
        "No technical limit.\n\n- **Codex:** already bound.\n- Grok too."
    );
}

// Catches: the connection acknowledgement filed as the answer's first line.
#[test]
fn removes_the_connection_acknowledgement() {
    let reply = "TUICommander v2.1.1 is connected.\nintent: Reading (Read)\n💬 Done.";
    assert_eq!(strip(reply), "Done.");
}

// Catches: an answer that explains the protocol losing its own examples.
#[test]
fn keeps_markers_inside_fenced_and_indented_code() {
    let reply = "💬 Use this form:\n```text\nsuggest: [ A | B ]\n💬 answer\n```\n    intent: x (Y)";
    assert_eq!(
        strip(reply),
        "Use this form:\n```text\nsuggest: [ A | B ]\n💬 answer\n```\n    intent: x (Y)"
    );
}

// Catches: prose that merely mentions `suggest:` or uses brackets being deleted.
#[test]
fn keeps_prose_that_is_not_a_marker() {
    let reply = "The suggest: [ token ] needs two items.\nintent:without space stays.";
    assert_eq!(strip(reply), reply);
}

// Catches: a suggestion glued to the last prose line surviving in the answer.
#[test]
fn removes_a_suggestion_on_the_same_line_as_the_last_sentence() {
    assert_eq!(strip("💬 Done. suggest: [ A | B | C ]"), "Done.");
}
