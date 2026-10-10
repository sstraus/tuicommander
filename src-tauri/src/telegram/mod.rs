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
mod registration;
mod runtime;
pub(crate) mod settings;
mod stop;
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

impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        // No external body, path, URL, credential or chat text crosses this seam.
        f.write_str(match self {
            Self::State => "telegram_invalid_state",
            Self::NotRegistered => "telegram_not_registered: call telegram register first",
            Self::Capacity => "telegram_mail_capacity",
            Self::Config => "telegram_invalid_config",
            Self::PrivateFile => "telegram_private_file_unavailable",
            Self::AlreadyOwned => "telegram_already_owned",
            Self::Transport => "telegram_transport_unavailable",
            Self::Protocol => "telegram_invalid_response",
            Self::Unauthorized => "telegram_unauthorized",
            Self::Conflict => "telegram_owner_conflict",
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
