# Telegram public example

`public-text-update.json` is extracted without schema changes from the first
"Message with text" example in Telegram's public webhook guide:
https://core.telegram.org/bots/webhooks#testing-your-bot-with-updates
Retrieved 2026-10-03. The example already contains fake IDs and names.
Tests may vary IDs/text explicitly to exercise our validation boundaries.
It is a recorded public reference example, not a live mint/Bot API capture.
Fake-server envelopes and HTTP status faults model the documented contract;
real response, draft, callback and Stop captures remain deferred.

The older public sample omits the modern required Chat.type. Inbound tests
explicitly supply `private` alongside their varied fake IDs/text, based on the
current Bot API Chat schema. This adaptation is not claimed as captured traffic.

`workflow-contract.json` records explicitly synthetic examples derived from Bot API
10.4 (2026-11-09), https://core.telegram.org/bots/api, retrieved 2026-11-11. It covers
sendMessage, editMessageText and ReplyParameters. The request assertions and
response parsing protect the local adapter contract, not live provider parity.
Callback/restart checks use the shared ledger with varied synthetic identities.
No authenticated send or live callback capture was authorized for 013-66b3.
