//! The user message sent to a fork.
//!
//! Every instruction lives here, in the user message, never in a system
//! prompt flag: the fork must keep the main session's cached prefix.

use super::Exchange;

/// Most chars of earlier exchanges a follow-up prompt replays. The prompt is
/// one process argument, and Windows caps a command line at 32,767 chars.
const PRIOR_BUDGET_CHARS: usize = 8_000;

/// Wrap a side question (or a follow-up in the same thread).
pub(crate) fn wrap_question(question: &str) -> String {
    wrap("", question)
}

/// Wrap a follow-up for a fork that does not keep the thread: the earlier
/// exchanges go before the question, the newest kept first when the budget
/// is short.
pub(crate) fn wrap_follow_up(prior: &[Exchange], question: &str) -> String {
    let mut left = PRIOR_BUDGET_CHARS;
    let mut kept = Vec::new();
    for exchange in prior.iter().rev() {
        let text = format!(
            "Q: {}\nA: {}\n",
            exchange.question.trim(),
            exchange.answer.trim()
        );
        let len = text.chars().count();
        if len > left {
            // The newest exchange is cut to fit; older ones are dropped whole.
            if kept.is_empty() {
                kept.push(text.chars().take(left).chain("…\n".chars()).collect());
            }
            break;
        }
        left -= len;
        kept.push(text);
    }
    if kept.is_empty() {
        return wrap("", question);
    }
    kept.reverse();
    let earlier = format!(
        "Earlier questions and answers of this side thread, oldest first:\n<earlier>\n{}</earlier>\n\n",
        kept.join("\n")
    );
    wrap(&earlier, question)
}

fn wrap(earlier: &str, question: &str) -> String {
    format!(
        "<side-question>\n\
The user asks this side question while the main session may still be working.\n\
You are a read-only fork of that session. Do not modify files, do not run \
commands that change state, and do not continue the main task.\n\
A tool call without a result is still running in the main session: it is in \
progress, not interrupted.\n\
Start your reply with exactly one header line:\n\
[answer|action|unknown] <short title>\n\
- answer: an explanation;\n\
- action: the main agent should change something;\n\
- unknown: the conversation does not contain the answer.\n\
Then give the answer, in the language of the question.\n\
\n\
{earlier}Question:\n\
{}\n\
</side-question>",
        question.trim()
    )
}
