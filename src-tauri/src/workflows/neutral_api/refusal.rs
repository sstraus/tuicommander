//! Caller refusals of the neutral workflow action, told apart from real faults.
//!
//! The domain reports every failure as a `String`, so the refusal texts are
//! owned here: each producing site builds its message from one of these
//! constants and `classify` reads the same constants. A reworded refusal
//! cannot silently turn back into a server fault. Texts not listed are faults.
//!
//! DEFERRED (2026-11-16) — unknown effect keys still return 500. Their error text
//! also describes an internal callback identity mismatch, so classifying that
//! text wholesale would hide faults.

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Refusal {
    /// The named object does not exist.
    NotFound,
    /// The request was valid but the current state refuses it (stale or replayed).
    Conflict,
    /// The request itself is malformed.
    Invalid,
    /// The caller may not act on this object.
    Forbidden,
}

pub(crate) const PUBLICATION_NOT_FOUND: &str = "neutral publication not found: ";
pub(crate) const DRAFT_NOT_FOUND: &str = "neutral draft not found in project";
pub(crate) const PRESET_NOT_FOUND: &str = "activity preset not found";
/// The text an unknown run has always carried: the SQLite no-row error under
/// the identity read. Kept byte for byte (3076 keeps the body text unchanged);
/// every other `read neutral run identity:` error stays a fault.
pub(crate) const RUN_NOT_FOUND: &str = "read neutral run identity: Query returned no rows";

pub(crate) const ANSWER_BINDING_NOT_FOUND: &str = "native trusted caller/run binding unavailable";
pub(crate) const OPERATOR_ANSWER_CONFLICT: &str =
    "operator answer differs from exact current wait/sequence/reason";
pub(crate) const OPERATOR_SUBJECT_STALE: &str =
    "operator wait is no longer the current owned subject";

pub(crate) const APPROVAL_STALE: &str = "neutral approval is expired, stale or already answered";
pub(crate) const DRAFT_REVISION_CHANGED: &str = "neutral draft revision changed";
pub(crate) const SEQUENCE_CHANGED: &str =
    "neutral execution sequence changed; retry from current snapshot";
pub(crate) const COMMAND_IDENTITY_REUSED: &str =
    "neutral command identity reused with different arguments";
pub(crate) const REQUEST_IDENTITY_REUSED: &str =
    "neutral request identity was reused with different arguments";
pub(crate) const RUN_TERMINAL: &str = "neutral run is terminal";
pub(crate) const RUN_DEADLINE_ELAPSED: &str = "neutral run deadline elapsed";
pub(crate) const OPERATION_VERSION_STALE: &str = "stale reusable operation version";

pub(crate) const PROJECT_NOT_ABSOLUTE: &str = "project must be an absolute path";
pub(crate) const INVALID_CURSOR: &str = "invalid neutral event cursor or limit";
pub(crate) const COMMAND_NEEDS_IDENTITY: &str = "neutral command requires an identity";
pub(crate) const DRAFT_NAME_INVALID: &str = "neutral draft requires a bounded nonempty name";
pub(crate) const DRAFT_TOO_LARGE: &str = "neutral draft is too large";
pub(crate) const STORY_ID_INVALID: &str = "story id must be nonempty and contain no ':'";
pub(crate) const DELIVERY_KEY_INVALID: &str = "delivery key must be nonempty and at most 200 bytes";
pub(crate) const CHILD_CROSS_PROJECT: &str = "cross-project child dispatch is not supported";
pub(crate) const OPERATOR_WAIT_TYPED: &str =
    "operator wait requires typed operator_answer, not effect approval";

pub(crate) const RUN_FOREIGN_PROJECT: &str = "neutral run does not belong to project";
pub(crate) const PUBLICATION_FOREIGN_PROJECT: &str =
    "neutral publication does not belong to project";
pub(crate) const OPERATOR_CONSENT_OWNER: &str =
    "operator consent requires authenticated human owner and exact run";
pub(crate) const ANSWER_AUTHORITY: &str =
    "native answer requires authenticated owner or bound Operator agent";

/// Some refusals are prefixes: they carry the offending id after the constant.
const NOT_FOUND: &[&str] = &[
    PUBLICATION_NOT_FOUND,
    DRAFT_NOT_FOUND,
    PRESET_NOT_FOUND,
    RUN_NOT_FOUND,
    ANSWER_BINDING_NOT_FOUND,
];
const CONFLICT: &[&str] = &[
    APPROVAL_STALE,
    OPERATOR_ANSWER_CONFLICT,
    OPERATOR_SUBJECT_STALE,
    DRAFT_REVISION_CHANGED,
    SEQUENCE_CHANGED,
    COMMAND_IDENTITY_REUSED,
    REQUEST_IDENTITY_REUSED,
    RUN_TERMINAL,
    RUN_DEADLINE_ELAPSED,
    OPERATION_VERSION_STALE,
];
const INVALID: &[&str] = &[
    PROJECT_NOT_ABSOLUTE,
    INVALID_CURSOR,
    COMMAND_NEEDS_IDENTITY,
    DRAFT_NAME_INVALID,
    DRAFT_TOO_LARGE,
    STORY_ID_INVALID,
    DELIVERY_KEY_INVALID,
    CHILD_CROSS_PROJECT,
    OPERATOR_WAIT_TYPED,
];
const FORBIDDEN: &[&str] = &[
    RUN_FOREIGN_PROJECT,
    PUBLICATION_FOREIGN_PROJECT,
    OPERATOR_CONSENT_OWNER,
    ANSWER_AUTHORITY,
];

/// `None` means a fault: the caller did nothing wrong that this action can name.
pub(crate) fn classify(error: &str) -> Option<Refusal> {
    [
        (Refusal::NotFound, NOT_FOUND),
        (Refusal::Conflict, CONFLICT),
        (Refusal::Invalid, INVALID),
        (Refusal::Forbidden, FORBIDDEN),
    ]
    .into_iter()
    .find(|(_, texts)| texts.iter().any(|text| error.starts_with(text)))
    .map(|(kind, _)| kind)
}

#[cfg(test)]
mod tests {
    use super::*;

    // Catches: a refusal constant added without a class, a fault whose text
    // merely resembles a refusal, and a refusal that carries an id suffix.
    #[test]
    fn classify_names_every_refusal_and_leaves_faults_alone() {
        for (text, kind) in [
            ("neutral publication not found: p:id@3", Refusal::NotFound),
            (DRAFT_NOT_FOUND, Refusal::NotFound),
            (PRESET_NOT_FOUND, Refusal::NotFound),
            (
                "read neutral run identity: Query returned no rows",
                Refusal::NotFound,
            ),
            (
                "native trusted caller/run binding unavailable",
                Refusal::NotFound,
            ),
            (
                "operator answer differs from exact current wait/sequence/reason",
                Refusal::Conflict,
            ),
            (
                "operator wait is no longer the current owned subject",
                Refusal::Conflict,
            ),
            (APPROVAL_STALE, Refusal::Conflict),
            (
                "neutral draft revision changed or project does not own draft",
                Refusal::Conflict,
            ),
            (SEQUENCE_CHANGED, Refusal::Conflict),
            (COMMAND_IDENTITY_REUSED, Refusal::Conflict),
            (REQUEST_IDENTITY_REUSED, Refusal::Conflict),
            (RUN_TERMINAL, Refusal::Conflict),
            (RUN_DEADLINE_ELAPSED, Refusal::Conflict),
            (OPERATION_VERSION_STALE, Refusal::Conflict),
            (PROJECT_NOT_ABSOLUTE, Refusal::Invalid),
            (INVALID_CURSOR, Refusal::Invalid),
            (COMMAND_NEEDS_IDENTITY, Refusal::Invalid),
            (DRAFT_NAME_INVALID, Refusal::Invalid),
            (DRAFT_TOO_LARGE, Refusal::Invalid),
            (STORY_ID_INVALID, Refusal::Invalid),
            (DELIVERY_KEY_INVALID, Refusal::Invalid),
            (CHILD_CROSS_PROJECT, Refusal::Invalid),
            (OPERATOR_WAIT_TYPED, Refusal::Invalid),
            (RUN_FOREIGN_PROJECT, Refusal::Forbidden),
            (PUBLICATION_FOREIGN_PROJECT, Refusal::Forbidden),
            (OPERATOR_CONSENT_OWNER, Refusal::Forbidden),
            (ANSWER_AUTHORITY, Refusal::Forbidden),
        ] {
            assert_eq!(classify(text), Some(kind), "{text}");
        }
        for fault in [
            "read neutral run identity: database is locked",
            "native catalog not installed",
            "native answer store differs from trusted binding",
            "native binding unavailable",
            "callback identity or invocation digest mismatch",
            "save neutral decision: disk full",
            "decode neutral publication p@1: eof",
        ] {
            assert_eq!(classify(fault), None, "{fault}");
        }
    }
}
