//! Derive callback routes from retained effects, never a live peer registry.
use super::neutral_effects::{EffectCommand, EffectOutcome};
use super::neutral_execution::NeutralResult;
use super::neutral_telegram_contract::NotificationReference;
use super::runtime::RuntimeOwner;

impl RuntimeOwner {
    pub(super) fn telegram_answer(
        &self,
        bot: &str,
        chat: i64,
        message: i64,
        token: &str,
    ) -> Result<bool, String> {
        let Some(run) = token
            .strip_prefix("w:")
            .and_then(|s| s.split_once(':'))
            .and_then(|(run, _)| uuid::Uuid::parse_str(run).ok())
        else {
            return Ok(false);
        };
        let run = run.to_string();
        let snapshot = self.store.neutral_execution(&run)?;
        let mut action = None;
        let mut missing_receipt = false;
        for notification in snapshot.effects.values() {
            if !notification.dispatched
                || !notification
                    .invocation
                    .descriptor
                    .handler
                    .name
                    .starts_with("telegram.")
            {
                continue;
            }
            let Ok(request) = super::neutral_telegram::request(
                &notification.invocation.descriptor,
                &notification.invocation.inputs,
            ) else {
                continue;
            };
            if request.chat != chat {
                continue;
            }
            for candidate in &request.actions {
                if candidate.token(&run)? != token {
                    continue;
                }
                let Some(super::neutral_effects::EffectObservation {
                    outcome:
                        EffectOutcome::Completed {
                            result: NeutralResult::Completed { outputs, .. },
                        },
                    ..
                }) = &notification.observation
                else {
                    missing_receipt = true;
                    continue;
                };
                let receipt: NotificationReference = serde_json::from_value(
                    outputs
                        .get("notification")
                        .cloned()
                        .ok_or("Telegram notification missing")?,
                )
                .map_err(|_| "invalid Telegram notification receipt")?;
                if receipt.effect == notification.key
                    && receipt.message.bot == bot
                    && receipt.message.chat == chat
                    && receipt.message.message == message
                {
                    action = Some(candidate.clone());
                }
            }
        }
        let Some(action) = action else {
            return if missing_receipt {
                Err("Telegram notification receipt is not retained; decision blocked".into())
            } else {
                Ok(false)
            };
        };
        let target_run = self.store.neutral_execution(&action.effect.fence.run)?;
        let target = target_run.effect(&action.effect)?;
        if target.question.as_ref().map(|q| q.revision) != Some(action.revision)
            || target.answer.is_some()
            || target.dispatched
        {
            return Ok(false);
        }
        let result = self.neutral_effect_command(
            &action.effect.fence.run.clone(),
            EffectCommand::Answer {
                key: action.effect,
                revision: action.revision,
                approved: action.approved,
            },
        );
        // A deadline or concurrent answer can retire this exact question between
        // lookup and the shared transaction; it cannot select another wait.
        match result {
            Ok(_) => Ok(true),
            Err(error) if error.contains("expired, stale or already answered") => Ok(false),
            Err(error) => Err(error),
        }
    }
}
