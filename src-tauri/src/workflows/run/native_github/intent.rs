//! Migration may translate explicit intents, never infer publication authority.
use super::GitHubAction;
use crate::workflows::PublicationAction;

/// A source converter supplies exact pinned target data. Matching the intent
/// does not dispatch it: the normal native effect/approval path still owns it.
pub(super) fn require(intent: PublicationAction, action: &GitHubAction) -> Result<(), String> {
    if !matches!(
        (intent, action),
        (PublicationAction::Push, GitHubAction::PushBranch { .. })
            | (
                PublicationAction::CreateDraftPr,
                GitHubAction::OpenPr { .. }
            )
            | (PublicationAction::MarkReady, GitHubAction::MarkReady { .. })
    ) {
        return Err("source publication intent differs from exact native action".into());
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    // Catches source MarkReady silently translated to another publication effect.
    #[test]
    fn review_publication_source_intents_do_not_change_actions() {
        for (intent, action) in [
            (
                PublicationAction::Push,
                GitHubAction::PushBranch {
                    branch: "candidate".into(),
                },
            ),
            (
                PublicationAction::CreateDraftPr,
                GitHubAction::OpenPr {
                    branch: "candidate".into(),
                    base: "main".into(),
                    title: "Candidate".into(),
                    body: String::new(),
                },
            ),
            (
                PublicationAction::MarkReady,
                GitHubAction::MarkReady { number: 3 },
            ),
        ] {
            require(intent, &action).unwrap();
            let other = if intent == PublicationAction::Push {
                PublicationAction::MarkReady
            } else {
                PublicationAction::Push
            };
            assert!(require(other, &action).is_err());
        }
    }
}
