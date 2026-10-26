//! Opt-in Telegram adapter with native peer mail and correlated replies.
mod api;
mod backoff;
mod callbacks;
mod config;
mod inbound;
mod mail;
mod native;
mod notifications;
mod offset;
mod outbound;
pub(crate) mod recent;
mod registration;
mod runtime;
mod sender;
#[cfg(test)]
mod sender_tests;
pub(crate) mod settings;
mod stop;
mod streaming;
mod tool;
pub(crate) use native::start;
pub(crate) use tool::{definition as tool_definition, handle as handle_tool};

pub(crate) use api::BotApi;
pub(crate) use config::{Config, Owner, Paths};
#[cfg(test)]
pub(crate) use inbound::Poll;
#[cfg(test)]
type Inbound = inbound::Inbound<mail::TestInbox>;
#[cfg(test)]
pub(crate) use mail::{MailPort, PendingMail};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Error {
    Config,
    State,
    NotRegistered,
    NonOwnerButtons,
    SenderRateLimited,
    InvalidSenderName,
    Capacity,
    PrivateFile,
    AlreadyOwned,
    Transport,
    Protocol,
    Unauthorized,
    Conflict,
    RateLimited(u64),
    Rejected(u16),
}

impl Error {
    /// Faults that stop a worker until setup changes. A 409 is not one: it
    /// clears when the other process polling the same token leaves.
    pub(crate) fn is_fatal(self) -> bool {
        matches!(self, Self::Unauthorized | Self::Rejected(403 | 404))
    }
}

impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        // No external body, path, URL, credential or chat text crosses this seam.
        f.write_str(match self {
            Self::State => "telegram_invalid_state",
            Self::NotRegistered => "telegram_not_registered: call telegram register first",
            Self::NonOwnerButtons => "telegram_non_owner_buttons: only the inbound owner may send buttons",
            Self::SenderRateLimited => "telegram_sender_rate_limited: non-owner sends require a 30-second interval",
            Self::InvalidSenderName => "telegram_invalid_sender_name: use a non-empty name without brackets or control characters (maximum 128 UTF-16 units)",
            Self::Capacity => "telegram_mail_capacity",
            Self::Config => "telegram_invalid_config",
            Self::PrivateFile => "telegram_private_file_unavailable",
            Self::AlreadyOwned => "telegram_already_owned",
            Self::Transport => "telegram_transport_unavailable",
            Self::Protocol => "telegram_invalid_response",
            Self::Unauthorized => "telegram_unauthorized",
            Self::Conflict => {
                "telegram_owner_conflict: another process polls getUpdates with the same bot token"
            }
            Self::RateLimited(_) => "telegram_rate_limited",
            Self::Rejected(_) => "telegram_request_rejected",
        })
    }
}
impl std::error::Error for Error {}

#[cfg(test)]
mod adversarial_tests;
#[cfg(test)]
mod tests;

#[cfg(test)]
mod critic_setup_tests;

pub(crate) use notifications::automation_notice;

#[cfg(test)]
mod streaming_tests;
