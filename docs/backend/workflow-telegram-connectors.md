# Telegram workflow connector (013-66b3)

The installed Telegram host provides version-1 `telegram.send`, `telegram.update`
and `telegram.reply` descriptors. `telegram_host(bot_alias)` returns the host and
its `descriptors` map. Host composition registers these pins and handlers with the
same neutral owner. Installation remains with 008; public execution remains with
015. This slice does not enable a new public command or change the peer tool.

## Support inventory

| Operation | Existing integration | Workflow delivery |
|---|---|---|
| Notification | Peer and automation `sendMessage` through Outbound | One bounded send, retained notification/effect and message reference |
| Text update | BotApi request client; no installed workflow activity | `editMessageText` for exact bot/chat/message, with keyboard replacement |
| Message reply | Volatile peer/request correlation | `sendMessage` with exact `reply_parameters`, no fallback to unrelated send |
| Button decision | Current message handles, native peer mail | Durable action reference and exact effect/question answer transaction |
| Callback acknowledgment | `answerCallbackQuery` through Outbound | Same client, once after the durable decision |
| Authentication and polling | Existing Telegram configuration and sole polling owner | Reused; no workflow token value, second poller or credential store |
| Uncertain send | Existing automation claim | Neutral effect remains uncertain; no send replay |

## Typed values and ownership

Inputs are `chat` (private chat integer), `text` (1..4096 UTF-16 units), optional
`message` and `actions` (default empty). A message is
`{bot, chat, message}`. Send requires no message; update and reply require one for
the same bot and chat. Output `notification` is `{effect, message}` with the exact
precommitted send EffectKey and the returned message reference. Long text fails
before the request; one activity never becomes several partly delivered sends.

Each action is `{effect, revision, approved, label}`. `effect` is the full target
EffectKey, including its attempt fence and invocation/preparation digest. It binds
resolved arguments and subject. The target can be a different run: an operator
notification can be composed after an existing wait is prepared. The host that
creates that notification obtains references from the retained wait; text or a
model label does not create approval authority.

Connector metadata pins the bot alias, operation, credential **reference** and
resource schema with the descriptor. Instances cannot replace it. Current bot
configuration and authorized chat are checked during admission and again by the
existing serialized Telegram worker before its BotApi call. The resource key
serializes workflow mutations per bot/chat. The token stays in the existing
private Telegram configuration and is read only by BotApi.

## Durable callbacks

Callback data is 63 ASCII bytes: a notification-run UUID and a 112-bit digest of
that run, the full target EffectKey, question revision and boolean decision. The
full action is retained in the send invocation, not in a callback database. On
input the current Telegram worker checks its existing private-chat authorization,
sender, accessible message and callback ID. It routes workflow callbacks before
volatile peer selection, then the current workflow owner replays the notification
run and checks its committed message receipt. It answers the target via the same
EffectCommand::Answer transaction as other approval consumers.

Wrong bot/chat/message, stale revisions, changed arguments/subject and duplicate
answers do not select a new wait. Deadlines and competing answers remain guarded
by the shared owner transaction. Restart needs the ledger and existing Telegram
configuration; no live peer, PTY or agent registration is required. Receipt loss
blocks the decision rather than guessing which message was sent. Undecidable taps
are consumed as rejected, with best-effort acknowledgment when current Telegram
configuration permits it. Missing executor/receipt, invalid policy and ack failures
cannot poison the polling cursor or starve later messages. Acknowledgment failure
does not undo or repeat consent.

## External assumptions and validation gaps

Contract: [Telegram Bot API](https://core.telegram.org/bots/api), version 10.4
(2026-11-09), retrieved 2026-11-11: `sendMessage`, `editMessageText`,
`ReplyParameters`, `Message`, `CallbackQuery`, `InlineKeyboardButton` and
`answerCallbackQuery`. The fixture `telegram/fixtures/workflow-contract.json`
contains synthetic schema examples with provenance. It is not a live capture.
The existing public webhook fixture retains its separate recorded provenance.

The API supplies no idempotent send key or general `getMessage` lookup. After a
lost acknowledgment, recovery retains uncertainty and performs no network call.
A retained original-service receipt/operator investigation is required; retrying a
run is not evidence that the first send failed. Rate limits and protocol errors
also never cause this adapter to resend.

No bot token, authorized destination or permission for external writes was
supplied. Live send/update/reply, callback acknowledgment, real-client rendering
and a real restart remain unverified. They require Boss to configure a bot and
private chat, authorize the test, install the composed host, then check one send,
one edit, one reply and one exact question before and after restart. No synthetic
response is claimed as success against Telegram.
