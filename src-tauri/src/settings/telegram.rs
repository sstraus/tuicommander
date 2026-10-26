//! Telegram owner seam: `config.json` is written only through the telegram setup owner's locks.
//! The adapter never opens `owner.lock`, so a settings write cannot start a second poller; it
//! only wakes the poller that already owns the bot.
use super::{
    request::*,
    service::Store,
    transaction::{self, Prepared},
};
use crate::telegram::{Config, Paths, settings as setup};
use serde_json::Value;

const ID: &str = "telegram";

#[derive(Default)]
pub struct TelegramSettingsStore {
    paths: Option<Paths>,
}

impl TelegramSettingsStore {
    #[cfg(all(test, unix))]
    pub(crate) fn at_paths(paths: Paths) -> Self {
        Self { paths: Some(paths) }
    }

    fn paths(&self) -> Result<Paths, Failure> {
        match &self.paths {
            Some(paths) => Ok(paths.clone()),
            None => setup::paths().map_err(|_| Error::new(ID, "store_failed")),
        }
    }
}

fn to_value(config: &Config) -> Result<Value, Failure> {
    serde_json::to_value(config).map_err(|_| Error::new(ID, "store_failed"))
}

fn document(paths: &Paths, config: &Config) -> Result<Value, Failure> {
    let mut value = to_value(config)?;
    let owner = setup::owner_read(paths).map_err(|_| Error::new(ID, "store_failed"))?;
    value["bot.token"] = if owner["token"]["status"] == "set" {
        Value::Bool(true)
    } else {
        Value::Null
    };
    value["allowed_chat_ids"] = owner["chats"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(Value::as_str)
        .map(|id| (id.to_owned(), Value::Bool(true)))
        .collect::<serde_json::Map<String, Value>>()
        .into();
    Ok(value)
}

/// Same shape `Config::load` accepts; the alias normally comes from the bot itself.
fn valid_alias(alias: &str) -> bool {
    alias.len() <= 64
        && alias
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b"_-".contains(&b))
}

impl Store for TelegramSettingsStore {
    fn id(&self) -> &str {
        ID
    }
    fn target(&self) -> Target {
        Target::Machine
    }

    fn read(&self) -> Result<Value, Failure> {
        let paths = self.paths()?;
        document(
            &paths,
            &setup::read_config(&paths).map_err(|_| Error::new(ID, "store_failed"))?,
        )
    }

    fn commit(
        &self,
        prepare: &mut dyn FnMut(&Value) -> Result<Prepared, Failure>,
    ) -> Result<CommitReply, Failure> {
        let paths = self.paths()?;
        setup::update_config(&paths, |config, token_set| {
            let latest = document(&paths, config)?;
            let Prepared {
                mut value,
                mut reply,
            } = prepare(&latest)?;
            value
                .as_object_mut()
                .expect("config object")
                .remove("bot.token");
            value
                .as_object_mut()
                .expect("config object")
                .remove("allowed_chat_ids");
            let next: Config =
                serde_json::from_value(value).map_err(|_| Error::new(ID, "invalid_type"))?;
            let canonical = document(&paths, &next)?;
            // Enabling needs a bot the owner can actually poll with: a token file and its alias.
            if !valid_alias(&next.bot_alias)
                || (next.enabled && (next.bot_alias.is_empty() || !token_set))
            {
                return Err(Error::new(ID, "out_of_range"));
            }
            reply.revision = transaction::revision(&canonical)?;
            let changed = !reply.changed_ids.is_empty();
            reply.persisted = changed;
            Ok((reply, changed.then_some(next)))
        })
        .map_err(|_| Error::new(ID, "store_failed"))?
    }

    fn apply(
        &self,
        _: &CommitReply,
    ) -> impl std::future::Future<Output = Result<bool, Failure>> + Send {
        // The running owner re-reads config.json on this wake; nothing is spawned here.
        setup::notify_changed();
        std::future::ready(Ok(true))
    }
}

// Input-only: no Debug/Serialize implementation for token-bearing actions.
#[derive(serde::Deserialize, schemars::JsonSchema)]
#[serde(tag = "action", rename_all = "snake_case", deny_unknown_fields)]
pub enum TelegramAction {
    SetToken {
        #[schemars(extend("writeOnly" = true, "x-sensitivity" = "secret"))]
        token: String,
    },
    ClearToken,
    AddChat {
        chat_id: String,
    },
    RemoveChat {
        chat_id: String,
    },
}

/// Owner actions complement the config transaction store, like GitHub login actions.
#[derive(Default)]
pub struct TelegramOwnerSettings {
    paths: Option<Paths>,
}
impl TelegramOwnerSettings {
    #[cfg(all(test, unix))]
    pub(crate) fn at_paths(paths: Paths) -> Self {
        Self { paths: Some(paths) }
    }

    pub fn action_schema() -> Value {
        super::catalogue::schema::<TelegramAction>()
    }

    fn paths(&self) -> Result<Paths, Failure> {
        match &self.paths {
            Some(paths) => Ok(paths.clone()),
            None => setup::paths().map_err(|_| Error::new(ID, "store_failed")),
        }
    }

    pub fn read(&self) -> Result<Value, Failure> {
        setup::owner_read(&self.paths()?).map_err(|_| Error::new(ID, "store_failed"))
    }

    pub async fn perform(&self, action: TelegramAction) -> Result<Value, Failure> {
        let change = match action {
            TelegramAction::SetToken { token } => setup::Change::Token { token },
            TelegramAction::ClearToken => setup::Change::ClearToken,
            TelegramAction::AddChat { chat_id } => setup::Change::AddChat { chat_id },
            TelegramAction::RemoveChat { chat_id } => setup::Change::RemoveChat { chat_id },
        };
        let mut persisted = false;
        let changed = setup::change_owner_with_phase(self.paths()?, change, &mut persisted)
            .await
            .map_err(|_| ());
        if changed.is_ok() {
            persisted = true;
        }
        changed
            .and_then(|_| self.read().map_err(|_| ()))
            .map_err(|_| {
                // Setup or final readback can fail after persistence. Never replay blind.
                let mut error = Error::new(
                    ID,
                    if persisted {
                        "apply_failed"
                    } else {
                        "store_failed"
                    },
                );
                error.persisted = persisted;
                error.next_action = "read Telegram owner state before retrying".into();
                error
            })
    }
}

pub(super) fn owner_schema() -> Value {
    let mut schema = super::catalogue::schema::<Config>();
    schema["properties"]["bot.token"] = serde_json::json!({"type":"string","writeOnly":true,
        "x-setting":{"sensitivity":"secret","apply":"live","effect":"Wakes the existing Telegram owner.","operations":["get","set_secret","clear_secret"],"confirmationClass":"ordinary"}});
    schema["properties"]["allowed_chat_ids"] = serde_json::json!({"type":"object","additionalProperties":{"type":"boolean",
        "x-setting":{"sensitivity":"private","apply":"live","effect":"Updates the Telegram chat allowlist through setup.","operations":["get","create","remove"],"confirmationClass":"ordinary"}}});
    schema
}
