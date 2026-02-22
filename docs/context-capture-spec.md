# Claude Code Context Capture & Parse — Implementation Specification

> Design document for a Rust module that captures, parses, and exposes Claude Code conversation context from within tuicommander — both as a standalone local module and as a compatible client for the Happy ecosystem.
> **Status:** Draft — implementation not started
> **Target location:** `src-tauri/src/context_capture/` + `src-tauri/src/happy_client/`

---

## Table of Contents

### Part I — Local Context Capture (standalone)

1. [Goal](#1-goal)
2. [Reference Architecture (Happy app)](#2-reference-architecture-happy-app)
3. [Proposed Architecture (tuicommander)](#3-proposed-architecture-tuicommander)
4. [Data Model](#4-data-model)
5. [Module: `conversation_reader`](#5-module-conversation_reader)
6. [Module: `message_parser`](#6-module-message_parser)
7. [Module: `context_formatter`](#7-module-context_formatter)
8. [Module: `context_store`](#8-module-context_store)
9. [Integration Points](#9-integration-points)
10. [API Surface](#10-api-surface)
11. [Configuration](#11-configuration)
12. [Crate Dependencies](#12-crate-dependencies)
13. [Test Strategy](#13-test-strategy)
14. [Differences from Happy App](#14-differences-from-happy-app)
15. [Open Questions](#15-open-questions)

### Part II — Happy Infrastructure Integration (network client)

16. [Happy Ecosystem Overview](#16-happy-ecosystem-overview)
17. [Authentication & Key Exchange Protocol](#17-authentication--key-exchange-protocol)
18. [Encryption Layer](#18-encryption-layer)
19. [WebSocket Sync Protocol](#19-websocket-sync-protocol)
20. [HTTP API Endpoints](#20-http-api-endpoints)
21. [Machine Daemon Protocol](#21-machine-daemon-protocol)
22. [Session & Message Lifecycle](#22-session--message-lifecycle)
23. [Module: `happy_client`](#23-module-happy_client)
24. [Module: `happy_crypto`](#24-module-happy_crypto)
25. [Module: `happy_sync`](#25-module-happy_sync)
26. [Module: `happy_daemon`](#26-module-happy_daemon)
27. [Bridging Local Capture → Happy](#27-bridging-local-capture--happy)
28. [Additional Crate Dependencies](#28-additional-crate-dependencies)
29. [Security Considerations](#29-security-considerations)
30. [Happy Integration Open Questions](#30-happy-integration-open-questions)

---

## 1. Goal

Provide a Rust-native module in tuicommander that:

1. **Captures** Claude Code conversation data (messages, tool calls, permissions, session metadata) from live PTY sessions
2. **Parses** raw output into structured conversation events
3. **Formats** the structured data into context strings suitable for transmission to external consumers (voice assistants, MCP clients, other agents)
4. **Exposes** the context via HTTP/WebSocket endpoints and Tauri events

The module must be **paritetico** (equivalent in capability) to the Happy app's TypeScript implementation but adapted to tuicommander's PTY-first architecture — no remote server required, no encryption layer, direct local capture.

---

## 2. Reference Architecture (Happy app)

The Happy app implements this flow:

```
Remote machine (Claude Code)
    → Happy backend server (WebSocket sync, encrypted messages)
    → Happy app (decrypt → normalize → reduce → format)
    → ElevenLabs Realtime API (voice assistant context)
```

### Key components in Happy:

| Component | File | Role |
|-----------|------|------|
| Message types | `sync/types.ts` | `Message` union: `user-text`, `agent-text`, `tool-call`, `agent-event` |
| Raw types | `sync/typesRaw.ts` | `RawRecord` with encrypted content, `normalizeRawMessage()` |
| Reducer | `sync/reducer/reducer.ts` | 5-phase pipeline: permissions → events → text → tools → sidechains |
| Storage | `sync/storage.ts` | In-memory `SessionMessages` with `messages[]` and `messagesMap{}` |
| Context formatters | `realtime/hooks/contextFormatters.ts` | 9 formatting functions producing plain text |
| Voice hooks | `realtime/hooks/voiceHooks.ts` | Event routing with dedup and config flags |
| Voice config | `realtime/hooks/voiceConfig.ts` | Feature flags controlling what gets transmitted |
| Voice session | `realtime/RealtimeVoiceSession.tsx` | ElevenLabs `useConversation` wrapper |
| Voice API | `sync/apiVoice.ts` | Token fetch from backend |

### Happy's message wire format (after decryption):

```json
{
    "role": "agent",
    "content": {
        "type": "output",
        "data": {
            "type": "assistant",
            "uuid": "msg-uuid",
            "message": {
                "role": "assistant",
                "model": "claude-sonnet-4-20250514",
                "content": [
                    {"type": "text", "text": "Response text"},
                    {"type": "tool_use", "id": "tool-id", "name": "Read", "input": {"file_path": "/foo"}}
                ],
                "usage": {"input_tokens": 100, "output_tokens": 50}
            }
        }
    }
}
```

### Happy's context format templates:

**Full session context (`formatSessionFull`):**
```
# Session ID: {session.id}
# Project path: {session.metadata.path}
# Session summary:
{session.metadata.summary.text}

## Session Summary
{session.metadata.summary.text}

## Our interaction history so far

{formatHistory(session.id, messages)}
```

**Single message formats (`formatMessage`):**
```
// agent-text
Claude Code:
<text>{message.text}</text>

// user-text
User sent message:
<text>{message.text}</text>

// tool-call (limited mode — no args)
Claude Code is using {tool.name} - {tool.description}

// tool-call (full mode — with args)
Claude Code is using {tool.name} - {tool.description} (tool_use_id: {id}) with arguments: <arguments>{JSON(tool.input)}</arguments>
```

**New messages (`formatNewMessages`):**
```
New messages in session: {sessionId}

{formatted_message_1}

{formatted_message_2}
```

**History (`formatHistory`):**
```
History of messages in session: {sessionId}

{formatted_message_1}

{formatted_message_2}

...up to MAX_HISTORY_MESSAGES (default 50)
```

**Permission request (`formatPermissionRequest`):**
```
Claude Code is requesting permission to use {toolName} (session {sessionId}):
<request_id>{requestId}</request_id>
<tool_name>{toolName}</tool_name>
<tool_args>{JSON(toolArgs)}</tool_args>
```

**Event notifications:**
```
Session went offline: {sessionId}
Session came online: {sessionId}
Session became focused: {sessionId}
Claude Code done working in session: {sessionId}. The previous message(s) are the summary of the work done. Report this to the human immediately.
```

### Happy's voice config flags:

```typescript
DISABLE_TOOL_CALLS: false        // Include tool calls in context
LIMITED_TOOL_CALLS: true         // Exclude tool arguments (only name + description)
DISABLE_PERMISSION_REQUESTS: false
DISABLE_SESSION_STATUS: true     // Skip online/offline notifications
DISABLE_MESSAGES: false
DISABLE_SESSION_FOCUS: false
DISABLE_READY_EVENTS: false
MAX_HISTORY_MESSAGES: 50
ENABLE_DEBUG_LOGGING: true
```

---

## 3. Proposed Architecture (tuicommander)

tuicommander has a fundamental advantage: **direct PTY access**. No remote server, no encryption, no WebSocket sync. We read Claude Code's output directly.

### Architectural difference

```
Happy:  Claude Code → server → WebSocket → decrypt → normalize → reduce → format
TUI:    Claude Code → PTY → reader thread → parse → store → format
```

### Source of truth

Claude Code emits **two types of output** to the PTY:

1. **Streaming text** — visible terminal output (ANSI-escaped, human-readable)
2. **JSONL structured events** — when launched with `--output-format stream-json`, Claude Code emits one JSON object per line containing typed events

tuicommander's `output_parser.rs` already parses the streaming text for events (rate limits, questions, status lines). The context capture module builds on top of this by additionally:

- Parsing JSONL events when available (preferred, lossless)
- Falling back to heuristic text parsing when JSONL is not available
- Accumulating parsed events into a conversation model
- Formatting the conversation into context strings

### Module layout

```
src-tauri/src/context_capture/
├── mod.rs                  // Public API, re-exports
├── types.rs                // Data model (Message, Session, ToolCall, etc.)
├── conversation_reader.rs  // PTY output → structured ConversationEvent stream
├── message_parser.rs       // Raw JSONL/text → Message objects
├── context_formatter.rs    // Messages → formatted context strings
└── context_store.rs        // Per-session conversation accumulator
```

---

## 4. Data Model

### `types.rs`

```rust
/// Unique session identifier (maps to tuicommander's PTY session ID)
pub type SessionId = String;

/// A conversation message in its final, structured form.
#[derive(Clone, Debug, Serialize)]
#[serde(tag = "kind")]
pub enum Message {
    #[serde(rename = "user-text")]
    UserText {
        id: String,
        created_at: u64,       // Unix ms
        text: String,
    },
    #[serde(rename = "agent-text")]
    AgentText {
        id: String,
        created_at: u64,
        text: String,
    },
    #[serde(rename = "tool-call")]
    ToolCall {
        id: String,
        created_at: u64,
        tool: ToolCallInfo,
        children: Vec<Message>,  // Tool results, nested messages
    },
    #[serde(rename = "agent-event")]
    AgentEvent {
        id: String,
        created_at: u64,
        event: AgentEventKind,
    },
}

#[derive(Clone, Debug, Serialize)]
pub struct ToolCallInfo {
    pub name: String,
    pub state: ToolState,
    pub input: serde_json::Value,
    pub description: Option<String>,
    pub result: Option<serde_json::Value>,
    pub permission: Option<PermissionInfo>,
}

#[derive(Clone, Debug, Serialize)]
pub enum ToolState {
    Running,
    Completed,
    Error,
}

#[derive(Clone, Debug, Serialize)]
pub struct PermissionInfo {
    pub id: String,
    pub status: PermissionStatus,
}

#[derive(Clone, Debug, Serialize)]
pub enum PermissionStatus {
    Pending,
    Approved,
    Denied,
}

#[derive(Clone, Debug, Serialize)]
pub enum AgentEventKind {
    ModeSwitch { mode: String },
    Ready,
    SessionStart,
    SessionEnd,
}

/// Metadata about a Claude Code session
#[derive(Clone, Debug, Default, Serialize)]
pub struct SessionMetadata {
    pub project_path: Option<String>,
    pub summary: Option<String>,
    pub model: Option<String>,
    pub cwd: Option<String>,
}

/// Raw event from Claude Code JSONL output
/// These map to Claude Code's `--output-format stream-json` events
#[derive(Clone, Debug, Deserialize)]
#[serde(tag = "type")]
pub enum ClaudeJsonEvent {
    /// System/init message
    #[serde(rename = "system")]
    System {
        subtype: String,         // "init"
        session_id: Option<String>,
        tools: Option<Vec<serde_json::Value>>,
        model: Option<String>,
        cwd: Option<String>,
    },
    /// Assistant text/tool_use content block
    #[serde(rename = "assistant")]
    Assistant {
        uuid: String,
        message: AssistantMessage,
    },
    /// User message
    #[serde(rename = "user")]
    User {
        uuid: String,
        message: UserMessage,
    },
    /// Tool result
    #[serde(rename = "result")]
    Result {
        uuid: String,
        tool_use_id: String,
        content: serde_json::Value,
        is_error: Option<bool>,
    },
}

#[derive(Clone, Debug, Deserialize)]
pub struct AssistantMessage {
    pub role: String,
    pub model: Option<String>,
    pub content: Vec<ContentBlock>,
    pub usage: Option<UsageInfo>,
    pub stop_reason: Option<String>,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(tag = "type")]
pub enum ContentBlock {
    #[serde(rename = "text")]
    Text { text: String },
    #[serde(rename = "tool_use")]
    ToolUse {
        id: String,
        name: String,
        input: serde_json::Value,
    },
}

#[derive(Clone, Debug, Deserialize)]
pub struct UserMessage {
    pub role: String,
    pub content: Vec<ContentBlock>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct UsageInfo {
    pub input_tokens: Option<u64>,
    pub output_tokens: Option<u64>,
    pub cache_read_input_tokens: Option<u64>,
    pub cache_creation_input_tokens: Option<u64>,
}

/// Configuration for what gets included in context output
#[derive(Clone, Debug)]
pub struct ContextConfig {
    pub include_tool_calls: bool,       // default: true
    pub include_tool_args: bool,        // default: false (limited mode)
    pub include_permissions: bool,      // default: true
    pub include_session_status: bool,   // default: false
    pub include_focus_events: bool,     // default: true
    pub include_ready_events: bool,     // default: true
    pub max_history_messages: usize,    // default: 50
    pub debug_logging: bool,            // default: false
}
```

---

## 5. Module: `conversation_reader`

### Responsibility

Consumes raw PTY output (bytes) and produces a stream of `ConversationEvent` values. Two parsing modes:

### Mode A: JSONL parsing (preferred)

When a PTY session is launched with `--output-format stream-json`, Claude Code emits one JSON object per line. Each line is a complete JSON object matching `ClaudeJsonEvent`.

**Algorithm:**

```
1. Maintain a line buffer per session
2. On each PTY chunk:
   a. Append to line buffer
   b. Split on '\n'
   c. For each complete line:
      - Try serde_json::from_str::<ClaudeJsonEvent>(line)
      - If Ok → emit ConversationEvent::JsonEvent(event)
      - If Err → treat as plain text, emit ConversationEvent::Text(line)
   d. Keep incomplete trailing line in buffer
```

**Edge cases:**
- UTF-8 boundary splits: handled by existing `Utf8ReadBuffer` in `pty.rs`
- Mixed output: Claude Code can emit both JSONL and ANSI text interleaved (the JSONL lines are distinct from terminal UI)
- Partial JSON: a very long tool_use input may produce a single line >64KB — must not truncate

### Mode B: Heuristic text parsing (fallback)

When JSONL is not available (agent launched in normal interactive mode), extract conversation structure from terminal text using patterns:

| Pattern | Event |
|---------|-------|
| `❯ ` or `> ` at start of line after silence | User input prompt |
| Text following user prompt until next agent response | `UserText` |
| Multi-line text blocks between tool uses | `AgentText` |
| `* Tool: name...` or spinner + tool name | `ToolCall` start |
| `✓` or `✗` after tool output | `ToolCall` completion |
| Permission prompt (existing `Question` event) | `Permission` pending |
| `Y`/`N` response to permission | `Permission` resolved |

**Note:** Heuristic mode is inherently lossy. JSONL mode should be preferred for context capture accuracy.

### Interface

```rust
pub enum ConversationEvent {
    /// Structured JSONL event from Claude Code
    JsonEvent(ClaudeJsonEvent),
    /// Plain text output (terminal UI, non-JSON)
    Text(String),
    /// Agent is waiting for input (from output_parser.rs Question event)
    Question(String),
    /// Agent finished processing (silence-based or explicit)
    Ready,
}

pub struct ConversationReader {
    line_buffer: String,
    mode: ReaderMode,
}

pub enum ReaderMode {
    /// Expect JSONL lines (--output-format stream-json)
    JsonLines,
    /// Heuristic text parsing (interactive mode)
    Heuristic,
    /// Auto-detect: try JSON first, fall back to heuristic
    Auto,
}

impl ConversationReader {
    pub fn new(mode: ReaderMode) -> Self;

    /// Feed a chunk of PTY output, returns parsed events
    pub fn feed(&mut self, chunk: &str) -> Vec<ConversationEvent>;

    /// Flush any buffered incomplete data (call on session close)
    pub fn flush(&mut self) -> Vec<ConversationEvent>;
}
```

### Integration with existing reader thread

The `ConversationReader` is instantiated per PTY session inside the reader thread (`pty.rs`). After the existing output_parser runs, the conversation_reader processes the same text:

```rust
// In pty.rs reader thread (pseudocode):
let mut conv_reader = ConversationReader::new(ReaderMode::Auto);

loop {
    let chunk = read_from_pty();

    // Existing: parse events (rate limits, questions, etc.)
    let parsed_events = output_parser.parse(&chunk);

    // New: parse conversation events
    let conv_events = conv_reader.feed(&chunk);
    for event in conv_events {
        context_store.push_event(session_id, event);
    }
}
```

---

## 6. Module: `message_parser`

### Responsibility

Converts `ConversationEvent` values into the structured `Message` model. This is equivalent to Happy's reducer (5-phase pipeline) but simpler because we don't need deduplication, encryption, or remote sync reconciliation.

### Processing pipeline

```
ConversationEvent
    ↓
Phase 1: Event classification
    - JsonEvent::System → update SessionMetadata
    - JsonEvent::Assistant → extract text blocks and tool_use blocks
    - JsonEvent::User → extract user text
    - JsonEvent::Result → match to pending tool call by tool_use_id
    - Text → if in heuristic mode, classify by pattern
    - Question → AgentEvent (permission or input request)
    - Ready → AgentEvent::Ready
    ↓
Phase 2: Message construction
    - Text content blocks → Message::AgentText or Message::UserText
    - tool_use blocks → Message::ToolCall (state: Running)
    - Result events → find matching ToolCall, update state to Completed/Error, attach result
    ↓
Phase 3: Permission resolution
    - If a ToolCall has a pending permission, track it
    - When user approves/denies, update PermissionInfo
    ↓
Output: Vec<Message>
```

### Interface

```rust
pub struct MessageParser {
    /// Pending tool calls awaiting results
    pending_tools: HashMap<String, usize>,  // tool_use_id → index in messages
    /// Auto-incrementing message counter
    next_id: u64,
}

impl MessageParser {
    pub fn new() -> Self;

    /// Process a conversation event, returns 0 or more new messages
    pub fn process(&mut self, event: ConversationEvent) -> ProcessResult;

    /// Get updated session metadata (if changed by this event)
    pub fn take_metadata_update(&mut self) -> Option<SessionMetadata>;
}

pub struct ProcessResult {
    /// New messages to append
    pub new_messages: Vec<Message>,
    /// Existing messages that were updated (tool result arrived)
    pub updated_indices: Vec<usize>,
}
```

---

## 7. Module: `context_formatter`

### Responsibility

Transforms the structured `Message` list and `SessionMetadata` into plain text context strings, exactly matching Happy's output format for compatibility with consumers that expect it.

### Functions

Each function corresponds 1:1 to a Happy formatter:

```rust
impl ContextFormatter {
    /// Full session context — used at voice session start
    /// Equivalent to Happy's formatSessionFull()
    pub fn format_session_full(
        session_id: &str,
        metadata: &SessionMetadata,
        messages: &[Message],
        config: &ContextConfig,
    ) -> String;

    /// Format a single message
    /// Equivalent to Happy's formatMessage()
    pub fn format_message(
        message: &Message,
        config: &ContextConfig,
    ) -> Option<String>;

    /// Format new messages arriving in a session
    /// Equivalent to Happy's formatNewMessages()
    pub fn format_new_messages(
        session_id: &str,
        messages: &[Message],
        config: &ContextConfig,
    ) -> Option<String>;

    /// Format conversation history (with max limit)
    /// Equivalent to Happy's formatHistory()
    pub fn format_history(
        session_id: &str,
        messages: &[Message],
        config: &ContextConfig,
    ) -> String;

    /// Format a permission request
    /// Equivalent to Happy's formatPermissionRequest()
    pub fn format_permission_request(
        session_id: &str,
        request_id: &str,
        tool_name: &str,
        tool_args: &serde_json::Value,
    ) -> String;

    /// Session status notifications
    pub fn format_session_online(session_id: &str) -> String;
    pub fn format_session_offline(session_id: &str) -> String;
    pub fn format_session_focus(session_id: &str) -> String;
    pub fn format_ready(session_id: &str) -> String;
}
```

### Output format specification

Every function must produce output **byte-identical** to the Happy TypeScript version (see Section 2 templates). This ensures consumers that already work with Happy's format continue to work.

**`format_message` logic:**

```
match message {
    UserText { text, .. } => Some(format!("User sent message: \n<text>{text}</text>")),

    AgentText { text, .. } => Some(format!("Claude Code: \n<text>{text}</text>")),

    ToolCall { tool, .. } => {
        if !config.include_tool_calls { return None }

        let desc = tool.description
            .as_ref()
            .map(|d| format!(" - {d}"))
            .unwrap_or_default();

        if config.include_tool_args {
            let args = serde_json::to_string(&tool.input).unwrap_or_default();
            Some(format!(
                "Claude Code is using {}{} (tool_use_id: {}) with arguments: <arguments>{}</arguments>",
                tool.name, desc, message.id, args
            ))
        } else {
            Some(format!("Claude Code is using {}{}", tool.name, desc))
        }
    },

    AgentEvent { .. } => None,
}
```

**`format_history` logic:**

```
1. Take messages.len().min(config.max_history_messages) from the START of the list
2. Map each through format_message()
3. Filter None values
4. Join with "\n\n"
5. Prepend "History of messages in session: {session_id}\n\n"
```

**`format_session_full` logic:**

```
# Session ID: {session_id}
# Project path: {metadata.project_path.unwrap_or("")}
# Session summary:
{metadata.summary.unwrap_or("")}

## Session Summary
{metadata.summary.unwrap_or("")}

## Our interaction history so far

{format_history(session_id, messages, config)}
```

---

## 8. Module: `context_store`

### Responsibility

Per-session accumulator that holds conversation state and provides the interface between the reader thread (producer) and the HTTP/event API (consumer).

### Data structure

```rust
pub struct ContextStore {
    /// Per-session conversation state, keyed by PTY session ID
    sessions: DashMap<SessionId, SessionContext>,
}

pub struct SessionContext {
    pub metadata: SessionMetadata,
    pub messages: Vec<Message>,
    pub parser: MessageParser,
    pub config: ContextConfig,
    /// Tracks which messages have been sent as context updates
    pub last_reported_index: usize,
    /// Whether this session is currently focused in the UI
    pub is_focused: bool,
    /// Whether Claude Code is currently processing (not idle)
    pub is_busy: bool,
}

impl ContextStore {
    pub fn new() -> Self;

    /// Create a new session context
    pub fn create_session(&self, session_id: &str, config: ContextConfig);

    /// Remove a session (on PTY close)
    pub fn remove_session(&self, session_id: &str);

    /// Push a conversation event from the reader thread
    /// Returns formatted context update string if there are new messages to report
    pub fn push_event(
        &self,
        session_id: &str,
        event: ConversationEvent,
    ) -> Option<ContextUpdate>;

    /// Get full formatted context for a session (for initial connection)
    pub fn get_full_context(&self, session_id: &str) -> Option<String>;

    /// Get all messages for a session (structured)
    pub fn get_messages(&self, session_id: &str) -> Option<Vec<Message>>;

    /// Get session metadata
    pub fn get_metadata(&self, session_id: &str) -> Option<SessionMetadata>;

    /// Mark session as focused/unfocused
    pub fn set_focused(&self, session_id: &str, focused: bool) -> Option<ContextUpdate>;

    /// Mark session as busy/idle (maps to ready events)
    pub fn set_busy(&self, session_id: &str, busy: bool) -> Option<ContextUpdate>;

    /// List all session IDs with active context
    pub fn list_sessions(&self) -> Vec<SessionId>;
}

/// A context update to be dispatched to consumers
pub struct ContextUpdate {
    pub session_id: SessionId,
    pub update_type: ContextUpdateType,
    pub formatted: String,
}

pub enum ContextUpdateType {
    /// Full session context (initial load or reconnect)
    Full,
    /// New messages added
    NewMessages,
    /// Permission request
    PermissionRequest { request_id: String, tool_name: String },
    /// Session came online
    Online,
    /// Session went offline
    Offline,
    /// Session gained focus
    Focus,
    /// Claude Code finished processing
    Ready,
}
```

### Thread safety

- `DashMap` for concurrent session access (reader threads write, HTTP handlers read)
- Individual `SessionContext` fields are only written by one reader thread per session
- Consumers read via `DashMap::get()` which provides a `Ref` guard

---

## 9. Integration Points

### 9.1 PTY Reader Thread (`pty.rs`)

Add `ConversationReader` + `ContextStore` to the reader thread:

```rust
// Existing reader thread in pty.rs
// After line ~180 where reader.read() happens:

// NEW: conversation reader per session
let mut conv_reader = ConversationReader::new(
    if agent_uses_json { ReaderMode::JsonLines } else { ReaderMode::Auto }
);

// In the read loop, after existing output_parser.parse():
let conv_events = conv_reader.feed(&text);
for event in conv_events {
    if let Some(update) = context_store.push_event(&session_id, event) {
        // Emit Tauri event for UI
        app.emit(&format!("context-update-{}", session_id), &update);
        // Broadcast to WebSocket clients subscribed to context
        broadcast_context_update(&state, &session_id, &update);
    }
}
```

### 9.2 Agent Spawning (`agent.rs`)

When spawning Claude Code specifically, prefer `--output-format stream-json` for lossless context capture:

```rust
// In agent spawn logic:
if agent.name == "claude" && context_capture_enabled {
    args.push("--output-format".to_string());
    args.push("stream-json".to_string());
}
```

**Important:** `stream-json` changes the PTY output. The existing `xterm.js` renderer in the frontend will receive JSON lines instead of ANSI terminal output. This requires either:

- **Option A:** Dual-pipe — launch with stream-json, render parsed content in a custom UI (not xterm.js)
- **Option B:** Post-process — launch in normal mode, use heuristic parsing
- **Option C:** Tee — use a Claude Code hook or wrapper that tees structured output to a sideband while keeping terminal output normal

Decision on which option to use is deferred to implementation.

### 9.3 HTTP API (`mcp_http/`)

New endpoints:

```
GET  /sessions/{id}/context          → Full formatted context string
GET  /sessions/{id}/context/messages → Structured message list (JSON)
GET  /sessions/{id}/context/metadata → Session metadata (JSON)
GET  /sessions/{id}/context/stream   → WebSocket: real-time ContextUpdate events
```

### 9.4 MCP Tools

New tools exposed via MCP:

```
get_session_context(session_id)     → Full formatted context string
get_session_messages(session_id)    → Structured messages JSON
subscribe_context(session_id)       → SSE stream of context updates
```

### 9.5 Tauri Events

```
context-update-{session_id}   → ContextUpdate (for frontend voice UI)
context-ready-{session_id}    → Claude Code finished processing
context-permission-{session_id} → Permission request requiring user action
```

### 9.6 AppState (`state.rs`)

Add `ContextStore` to the global state:

```rust
pub struct AppState {
    // ... existing fields ...
    pub context_store: ContextStore,
}
```

---

## 10. API Surface

### Public module API (`context_capture/mod.rs`)

```rust
pub mod types;
pub mod conversation_reader;
pub mod message_parser;
pub mod context_formatter;
pub mod context_store;

// Re-exports for convenience
pub use types::*;
pub use context_store::{ContextStore, ContextUpdate, ContextUpdateType};
pub use conversation_reader::{ConversationReader, ReaderMode};
pub use context_formatter::ContextFormatter;
```

### HTTP response types

```rust
/// GET /sessions/{id}/context
/// Content-Type: text/plain
/// Returns the full formatted context string

/// GET /sessions/{id}/context/messages
#[derive(Serialize)]
pub struct MessagesResponse {
    pub session_id: String,
    pub metadata: SessionMetadata,
    pub messages: Vec<Message>,
    pub total_count: usize,
}

/// WebSocket /sessions/{id}/context/stream
/// Each frame is a JSON-serialized ContextUpdate
```

---

## 11. Configuration

### Per-session context config

Configurable via `AppConfig` (persisted in `~/.tuicommander/config.json`):

```rust
pub struct ContextCaptureConfig {
    /// Enable context capture for new sessions
    pub enabled: bool,                     // default: true

    /// Default reader mode for new sessions
    pub default_reader_mode: ReaderMode,   // default: Auto

    /// Include tool calls in context
    pub include_tool_calls: bool,          // default: true

    /// Include tool arguments (verbose)
    pub include_tool_args: bool,           // default: false

    /// Include permission requests
    pub include_permissions: bool,         // default: true

    /// Include online/offline notifications
    pub include_session_status: bool,      // default: false

    /// Include focus change notifications
    pub include_focus_events: bool,        // default: true

    /// Include "ready" (Claude done) events
    pub include_ready_events: bool,        // default: true

    /// Max messages in history context
    pub max_history_messages: usize,       // default: 50

    /// Max total size of stored messages per session (bytes)
    pub max_store_size_bytes: usize,       // default: 10_000_000 (10MB)
}
```

---

## 12. Crate Dependencies

All dependencies are **already in tuicommander's Cargo.toml** — no new crates needed:

| Crate | Use |
|-------|-----|
| `serde` + `serde_json` | JSON parsing and serialization |
| `regex` + `lazy_static` | Text pattern matching (heuristic mode) |
| `dashmap` | Concurrent per-session storage |
| `strip-ansi-escapes` | ANSI code removal for clean text |
| `uuid` | Message ID generation |
| `tokio` | Async broadcast channels for context updates |

---

## 13. Test Strategy

### Unit tests

| Module | Test focus |
|--------|-----------|
| `conversation_reader` | JSONL line splitting, partial line buffering, UTF-8 boundaries, mode auto-detection |
| `message_parser` | All `ClaudeJsonEvent` variants → correct `Message` output, tool result matching, permission tracking |
| `context_formatter` | Output format byte-for-byte matches Happy's templates, config flag behavior, edge cases (empty sessions, no metadata) |
| `context_store` | Concurrent access, session lifecycle, update emission, message limits |

### Integration tests

1. **JSONL replay:** Record a real Claude Code `--output-format stream-json` session, replay through the full pipeline, verify formatted output
2. **Heuristic replay:** Record a normal Claude Code interactive session's PTY output, replay through heuristic parser, verify reasonable message extraction
3. **Concurrent sessions:** Spawn 10+ sessions, feed simultaneously, verify no cross-contamination

### Test data

Create `src-tauri/src/context_capture/testdata/`:
- `session_jsonl_simple.txt` — basic user/agent/tool exchange
- `session_jsonl_complex.txt` — permissions, sidechains, multi-tool
- `session_interactive.txt` — raw ANSI terminal output from normal mode
- `session_mixed.txt` — JSONL with interspersed terminal escape codes

### Property tests (optional)

Use `proptest` to verify:
- `format_message` never panics on arbitrary `Message` values
- `ConversationReader` never loses data (input bytes == output bytes when concatenated)
- `ContextStore` message count never exceeds `max_history_messages`

---

## 14. Differences from Happy App

| Aspect | Happy (TypeScript) | tuicommander (Rust) |
|--------|-------------------|---------------------|
| **Data source** | Remote server via WebSocket, encrypted | Local PTY, plaintext |
| **Encryption** | tweetnacl decrypt per message | None needed |
| **Sync protocol** | Server push + polling + invalidation | Direct PTY read |
| **Deduplication** | `localId` + `messageId` + `permissionId` | Not needed (single source of truth) |
| **Reducer complexity** | 5-phase pipeline with sidechains | 3-phase (classify → construct → resolve) |
| **Consumer** | ElevenLabs voice API (WebSocket) | HTTP/MCP/Tauri events (multiple consumers) |
| **Session discovery** | Server-managed session list | Local PTY session map |
| **Message normalization** | `RawRecord` → `NormalizedMessage` → `Message` | `ClaudeJsonEvent` → `Message` (single step) |
| **Thread model** | Single-threaded React (batched updates) | Multi-threaded (reader thread per PTY + DashMap) |
| **Memory management** | JS garbage collection | Explicit `max_store_size_bytes` limit |

### What we skip

- **Encryption/decryption layer** — not needed, local PTY
- **`localId` deduplication** — single writer per session
- **Sidechain handling** — Claude Code's sidechains are visible in JSONL but don't require separate UI routing in the context string
- **Voice token API** — consumers authenticate via tuicommander's existing auth, not ElevenLabs
- **`shownSessions` dedup set** — consumer-side concern, not the store's responsibility

### What we add

- **JSONL parsing** — Happy doesn't parse JSONL; it receives pre-structured messages from its server
- **Heuristic fallback** — Happy never needs this; tuicommander must support non-JSON agent output
- **Multi-consumer support** — Happy sends to one consumer (ElevenLabs); we expose to MCP, HTTP, Tauri events simultaneously
- **Configurable per-session** — Happy uses global voice config; we allow per-session overrides

---

## 15. Open Questions

1. **JSONL vs interactive mode:** Should we force `--output-format stream-json` for Claude Code sessions when context capture is enabled? This changes the terminal rendering. Need to decide on Option A/B/C from Section 9.2.

2. **Claude Code JSONL schema stability:** The `stream-json` format is not officially documented as a stable API. Need to verify compatibility across Claude Code versions and handle unknown event types gracefully.

3. **Memory budget:** With 50 concurrent sessions and 50 messages each, worst case is ~50 * 50 * 10KB = 25MB. Is this acceptable? Should we implement LRU eviction or summarization?

4. **Heuristic parsing accuracy:** How reliable is text-based message extraction? Should we invest heavily in heuristic mode or treat it as best-effort?

5. **Consumer protocol:** Should the WebSocket context stream use the same format as Happy's ElevenLabs integration (for drop-in compatibility), or a more structured JSON protocol?

6. **Sidechain visibility:** Should tool call sidechains (nested agent conversations) be included in the context, or flattened/omitted?

---
---

# Part II — Happy Infrastructure Integration

This section specifies how tuicommander can act as a **full participant in the Happy ecosystem**, replacing the `happy-cli` daemon with a Rust-native implementation that uses Happy's server infrastructure for encrypted sync, remote session control, and cross-device communication.

---

## 16. Happy Ecosystem Overview

The Happy ecosystem consists of three components:

```
┌─────────────────────┐     ┌──────────────────────┐     ┌─────────────────────┐
│   happy-coder       │     │   happy-server        │     │   happy-cli          │
│   (mobile/web app)  │◄───►│   (backend)           │◄───►│   (machine daemon)   │
│   React Native      │     │   cluster-fluster.com │     │   Node.js CLI        │
└─────────────────────┘     └──────────────────────┘     └─────────────────────┘
        ▲                                                         ▲
        │                                                         │
        │            ┌──────────────────────┐                     │
        └────────────│  THIS DOCUMENT       │─────────────────────┘
                     │  tuicommander as    │
                     │  Rust replacement    │
                     │  for happy-cli       │
                     └──────────────────────┘
```

### What each component does:

| Component | Repo | Role |
|-----------|------|------|
| **happy-coder** | `slopus/happy-coder` | Mobile/web UI — displays sessions, messages, voice assistant |
| **happy-server** | `slopus/happy-server` | Central relay — encrypted message storage, WebSocket push, auth |
| **happy-cli** | `slopus/happy-cli` | Machine daemon — wraps Claude Code, pushes data to server |

### What tuicommander replaces:

**`happy-cli`** — the machine-side daemon. Instead of running `happy` as a wrapper around `claude`, tuicommander already manages Claude Code sessions via PTY. Adding Happy integration means tuicommander can:

1. **Register as a machine** with the Happy server
2. **Push session data** (encrypted) to the server in real-time
3. **Receive commands** (RPC) from the mobile app (abort, permission responses, messages)
4. **Report machine status** (online/offline, daemon state)

The mobile app (`happy-coder`) continues working unchanged — it doesn't know or care whether the machine runs `happy-cli` (Node.js) or tuicommander (Rust).

---

## 17. Authentication & Key Exchange Protocol

### Overview

Happy uses **QR code-based authentication** with a **Curve25519 key exchange** to establish a 32-byte master secret. All subsequent encryption derives from this secret.

### Protocol Flow (New Device Registration)

```
┌──────────────┐          ┌──────────────┐          ┌──────────────┐
│  tuicommander│          │  happy-server │          │  mobile app  │
│  (new device) │          │              │          │  (approver)  │
└──────┬───────┘          └──────┬───────┘          └──────┬───────┘
       │                         │                         │
       │ 1. Generate Curve25519  │                         │
       │    keypair (pk, sk)     │                         │
       │                         │                         │
       │ 2. Display QR code:     │                         │
       │    "happy:///account?"  │                         │
       │    + base64url(pk)      │                         │
       │                         │                         │
       │ 3. POST /v1/auth/       │                         │
       │    account/request      │                         │
       │    { publicKey: pk }    │                         │
       │────────────────────────►│                         │
       │                         │                         │
       │                         │  4. User scans QR       │
       │                         │     on mobile app       │
       │                         │◄────────────────────────│
       │                         │                         │
       │                         │  5. Mobile approves     │
       │                         │     Server encrypts     │
       │                         │     master_secret with  │
       │                         │     pk using Box        │
       │                         │◄────────────────────────│
       │                         │                         │
       │ 6. Poll returns:        │                         │
       │    state: "authorized"  │                         │
       │    token: JWT           │                         │
       │    response: encrypted  │                         │
       │◄────────────────────────│                         │
       │                         │                         │
       │ 7. Decrypt response     │                         │
       │    with sk → get        │                         │
       │    master_secret (32B)  │                         │
       │                         │                         │
       │ 8. Store credentials:   │                         │
       │    { token, secret }    │                         │
       └────────────────────────────────────────────────────
```

### Step-by-step implementation:

**Step 1 — Keypair generation:**
```rust
// libsodium: crypto_box_seed_keypair
let seed = rand::random::<[u8; 32]>();
let (pk, sk) = crypto_box_seed_keypair(&seed);
```

**Step 2 — QR code encoding:**
```
QR data = "happy:///account?" + base64url_encode(pk)
```
Display in terminal using a QR code library (e.g., `qrcode` crate → unicode blocks).

**Step 3 — Server request:**
```
POST https://api.cluster-fluster.com/v1/auth/account/request
Content-Type: application/json

{ "publicKey": "<base64(pk)>" }
```

**Step 4–5 — Wait for mobile approval (polling):**
Repeat Step 3 until response contains `state: "authorized"`.

**Step 6 — Extract response:**
```json
{
    "state": "authorized",
    "token": "eyJhbG...",
    "response": "<base64(encrypted_bundle)>"
}
```

**Step 7 — Decrypt master secret:**
```rust
// encrypted_bundle layout:
//   [ephemeral_pk: 32 bytes] [nonce: 24 bytes] [ciphertext: N bytes]
let ephemeral_pk = &bundle[0..32];
let nonce = &bundle[32..56];
let ciphertext = &bundle[56..];

// libsodium: crypto_box_open_easy
let master_secret = crypto_box_open_easy(ciphertext, nonce, ephemeral_pk, &sk);
// master_secret is 32 bytes
```

**Step 8 — Store credentials:**
```rust
struct HappyCredentials {
    token: String,          // JWT for HTTP/WebSocket auth
    secret: [u8; 32],      // Master secret for encryption
}
// Persist to ~/.tuicommander/happy-credentials.json (encrypted at rest)
// or OS keychain via keyring crate
```

### Token Refresh (Challenge-Response)

When the JWT expires, refresh using Ed25519 signing:

```rust
// Derive signing keypair from master secret
let (sign_pk, sign_sk) = crypto_sign_seed_keypair(&master_secret);

// Generate random challenge
let challenge = rand::random::<[u8; 32]>();

// Sign it
let signature = crypto_sign_detached(&challenge, &sign_sk);

// POST /v1/auth
// { challenge: base64(challenge), signature: base64(signature), publicKey: base64(sign_pk) }
// Response: { token: "new_jwt" }
```

### Secret Key Backup Format

For manual restore (no QR code), the master secret can be encoded as a human-readable backup:

```
Format: XXXXX-XXXXX-XXXXX-XXXXX-XXXXX-XXXXX-XXXXX (11 groups, base32, dashes)

Encode: base32_encode(secret) → split into 5-char groups → join with '-'
Decode: strip dashes → character substitution (0→O, 1→I, 8→B, 9→G) → base32_decode → 32 bytes
```

---

## 18. Encryption Layer

### Key Hierarchy

```
master_secret (32 bytes, from auth)
    │
    ├── deriveKey("Happy EnCoder", ["content"]) → contentDataKey (32 bytes)
    │       │
    │       └── crypto_box_seed_keypair(contentDataKey) → contentKeyPair
    │               │
    │               ├── Wraps per-session data encryption keys
    │               └── Wraps per-machine data encryption keys
    │
    └── deriveKey("Happy Coder", ["analytics", "id"]) → anonId (first 16 hex chars)
            └── Anonymous analytics identifier
```

### Key Derivation Function

HMAC-SHA512 tree-based derivation:

```rust
fn derive_key(master: &[u8; 32], usage: &str, path: &[&str]) -> [u8; 32] {
    // 1. Root: HMAC-SHA512(key = "{usage} Master Seed", data = master)
    let root_key = format!("{} Master Seed", usage);
    let hmac_result = hmac_sha512(root_key.as_bytes(), master);
    let mut key = hmac_result[..32];      // first 32 bytes
    let mut chain_code = hmac_result[32..]; // last 32 bytes

    // 2. For each path segment:
    for segment in path {
        let data = [&[0x00], segment.as_bytes()].concat();
        let hmac_result = hmac_sha512(&chain_code, &data);
        key = hmac_result[..32];
        chain_code = hmac_result[32..];
    }

    key
}
```

### Three Encryption Algorithms Used

| Algorithm | Library | Use Case |
|-----------|---------|----------|
| **crypto_box** (Curve25519+ChaCha20+Poly1305) | libsodium | Key exchange, wrapping data encryption keys |
| **crypto_secretbox** (XSalsa20+Poly1305) | libsodium | Legacy message encryption (being phased out) |
| **AES-256-GCM** | native platform crypto | Primary message/metadata encryption |

### Per-Entity Encryption Keys

Each session and machine has its own AES-256 data encryption key, stored on the server wrapped with the content keypair:

```
Server stores:   session.dataEncryptionKey = base64(encryptBox(aes_key, contentKeyPair.publicKey))

Client unwraps:  aes_key = decryptBox(base64_decode(wrapped), contentKeyPair.secretKey)

Client uses:     encrypted_content = aes_256_gcm_encrypt(aes_key, json_stringify(data))
```

### Message Encryption Format

```
Wire format:
{
    "id": "msg-uuid",
    "seq": 42,
    "localId": "local-uuid",
    "content": {
        "t": "encrypted",
        "c": "<base64(version_byte + aes_gcm_ciphertext)>"
    },
    "createdAt": 1708454400000
}

version_byte = 0x00 (AES-256-GCM)

Decrypted content (RawRecord):
{
    "role": "agent" | "user",
    "content": { ... },  // text, tool_use, tool_result
    "meta": { "sentFrom": "cli", "permissionMode": "default" }
}
```

### Metadata Encryption

Session metadata and machine metadata are also encrypted with their respective data encryption keys:

```
encrypted_metadata = aes_256_gcm_encrypt(entity_key, json_stringify(metadata))
wire_format = base64(0x00 + encrypted_metadata)
```

---

## 19. WebSocket Sync Protocol

### Connection

```
Endpoint: wss://api.cluster-fluster.com/v1/updates
Transport: Socket.io v4 (WebSocket only, no long-polling)
Auth: { token: "jwt", clientType: "machine-scoped" }
Reconnection: automatic, 1s-5s delay, infinite attempts
```

**Client types:**
- `"user-scoped"` — mobile/web app (receives all user data)
- `"machine-scoped"` — machine daemon (receives machine-specific data + commands)

### Server → Client Events

#### Event: `update` (persistent state changes)

Discriminated union on `body.t`:

| Type | Payload | Description |
|------|---------|-------------|
| `new-message` | `{ sid, message: { id, seq, localId, content: {t,c}, createdAt } }` | New message in session |
| `new-session` | `{ id, createdAt, updatedAt }` | Session created |
| `delete-session` | `{ sid }` | Session deleted |
| `update-session` | `{ id, agentState?: {version, value}, metadata?: {version, value} }` | Session state changed |
| `update-account` | `{ id, settings?, firstName?, lastName?, avatar?, github? }` | User profile changed |
| `update-machine` | `{ machineId, metadata?, daemonState?, active?, activeAt? }` | Machine status changed |
| `new-artifact` | `{ id, header, body?, dataEncryptionKey }` | Artifact created |
| `update-artifact` | `{ id, header?, body? }` | Artifact updated |
| `delete-artifact` | `{ id }` | Artifact deleted |
| `relationship-updated` | `{ fromUserId, toUserId, status, action, timestamp }` | Friend status changed |
| `new-feed-post` | `{ id, body, cursor, createdAt }` | Feed item |
| `kv-batch-update` | `{ changes: [{key, value, version}] }` | KV store changes |

#### Event: `ephemeral` (real-time, non-persistent)

| Type | Payload | Description |
|------|---------|-------------|
| `activity` | `{ id: sessionId, active, activeAt, thinking }` | Session activity |
| `usage` | `{ id, key, timestamp, tokens: {total,in,out,cache_r,cache_w}, cost: {total,in,out} }` | Token usage |
| `machine-activity` | `{ id: machineId, active, activeAt }` | Machine heartbeat |

### Client → Server Events

#### Event: `message` (send message to session)

```json
{
    "sid": "session-uuid",
    "message": "<base64(encrypted_raw_record)>",
    "localId": "local-uuid",
    "sentFrom": "cli",
    "permissionMode": "default"
}
```

`sentFrom` values: `"web"`, `"ios"`, `"android"`, `"mac"`, `"cli"`

#### Event: `rpc-call` (with acknowledgement)

For session-scoped RPCs:
```json
{
    "method": "{sessionId}:{methodName}",
    "params": "<base64(encrypted_params)>"
}

// Ack response:
{ "ok": true, "result": "<base64(encrypted_result)>" }
```

**Session RPC methods** (callable FROM mobile app TO machine):

| Method | Params | Description |
|--------|--------|-------------|
| `abort` | `{}` | Stop current Claude Code operation |
| `permission` | `{ id, decision, reason? }` | Respond to permission request |
| `switch` | `{ mode }` | Switch session mode |
| `bash` | `{ command }` | Execute shell command |
| `readFile` | `{ path }` | Read file contents |
| `writeFile` | `{ path, content }` | Write file |
| `listDirectory` | `{ path }` | List directory |
| `getDirectoryTree` | `{ path, depth? }` | Get directory tree |
| `ripgrep` | `{ pattern, path?, flags? }` | Search files |
| `killSession` | `{}` | Terminate session |

**Machine RPC methods** (callable FROM mobile app TO machine daemon):

| Method | Params | Description |
|--------|--------|-------------|
| `spawn-happy-session` | `{ path?, model?, args? }` | Start new Claude Code session |
| `stop-daemon` | `{}` | Shutdown daemon |
| `bash` | `{ command }` | Execute shell command on machine |

#### Event: `machine-update-metadata` (with acknowledgement)

```json
{
    "machineId": "machine-uuid",
    "metadata": "<base64(encrypted_metadata)>",
    "expectedVersion": 5
}

// Ack response:
{ "result": "success", "version": 6 }
// or: { "result": "version-mismatch", "version": 5, "metadata": "..." }
```

---

## 20. HTTP API Endpoints

All requests require `Authorization: Bearer {token}`.
Server URL: `https://api.cluster-fluster.com` (configurable).

### Authentication

| Method | Endpoint | Body | Response |
|--------|----------|------|----------|
| POST | `/v1/auth/account/request` | `{ publicKey }` | `{ state, token?, response? }` |
| POST | `/v1/auth/account/response` | `{ publicKey, response }` | `{ success }` |
| POST | `/v1/auth` | `{ challenge, signature, publicKey }` | `{ token }` |

### Sessions

| Method | Endpoint | Body | Response |
|--------|----------|------|----------|
| GET | `/v1/sessions/{id}/messages` | — | `{ messages: ApiMessage[] }` |
| DELETE | `/v1/sessions/{id}` | — | `{ success }` |

### Machines

| Method | Endpoint | Body | Response |
|--------|----------|------|----------|
| GET | `/v1/machines` | — | `Machine[]` |

### Key-Value Store

| Method | Endpoint | Body | Response |
|--------|----------|------|----------|
| GET | `/v1/kv/{key}` | — | `{ key, value, version }` |
| GET | `/v1/kv?prefix=&limit=` | — | `{ items: KvItem[] }` |
| POST | `/v1/kv/bulk` | `{ keys: string[] }` | `{ values: KvItem[] }` |
| POST | `/v1/kv` | `{ mutations: [{key,value,version}] }` | `{ success, results }` |

### Voice (optional)

| Method | Endpoint | Body | Response |
|--------|----------|------|----------|
| POST | `/v1/voice/token` | `{ sessionId, agentId }` | `{ allowed, token?, agentId? }` |

### User/Social (optional for daemon)

| Method | Endpoint | Body | Response |
|--------|----------|------|----------|
| GET | `/v1/user/{id}` | — | `{ user }` |
| GET | `/v1/friends` | — | `{ friends }` |
| POST | `/v1/usage/query` | `{ sessionId?, startTime?, endTime?, groupBy }` | `{ usage }` |

---

## 21. Machine Daemon Protocol

This is the core of what tuicommander replaces: the machine-side daemon that feeds data into the Happy ecosystem.

### Machine Registration

On first launch with Happy credentials:

1. Generate a unique `machineId` (UUID v4, persisted locally)
2. Encrypt machine metadata with machine data encryption key
3. Connect via WebSocket with `clientType: "machine-scoped"`
4. Server creates machine record on first connection

### Machine Metadata

The daemon periodically updates its metadata:

```rust
struct MachineMetadata {
    host: String,                    // hostname
    platform: String,                // "darwin" | "linux" | "win32"
    happy_cli_version: String,       // our version string
    home_dir: String,                // user home directory
    happy_home_dir: String,          // ~/.happy or equivalent
    username: Option<String>,
    arch: Option<String>,            // "arm64" | "x64"
    display_name: Option<String>,    // user-friendly machine name
    daemon_last_known_status: Option<String>,  // "running" | "shutting-down"
    daemon_last_known_pid: Option<u32>,
    shutdown_source: Option<String>, // "happy-app" | "happy-cli" | "os-signal"
}
```

### Daemon State

Reported via `machine-update-metadata`:

```rust
struct DaemonState {
    pid: u32,
    http_port: Option<u16>,
    start_time: u64,
    started_with_cli_version: String,
}
```

### Session Lifecycle (Machine Side)

When Claude Code starts in a PTY session:

1. **Create session on server:**
   - Generate session ID (or receive from server)
   - Generate per-session AES-256 data encryption key
   - Wrap key with contentKeyPair and send to server

2. **Push session metadata (encrypted):**
   ```rust
   SessionMetadata {
       path: String,              // project directory
       host: String,              // hostname
       name: Option<String>,      // session name/summary
       version: Option<String>,   // Claude Code version
       os: Option<String>,
       machine_id: String,
       claude_session_id: Option<String>,
       home_dir: String,
       happy_home_dir: String,
       host_pid: Option<u32>,
       tools: Option<Vec<String>>,
       slash_commands: Option<Vec<String>>,
       flavor: Option<String>,    // "claude" | "codex" etc.
   }
   ```

3. **Push messages in real-time:**
   - Each conversation event → encrypt as `RawRecord` → emit `message` event

4. **Report activity:**
   - Emit `ephemeral` activity events (active, thinking) periodically

5. **Handle incoming RPCs:**
   - Listen for `rpc-call` events targeted at this session
   - Execute locally (abort, permission, file ops, etc.)
   - Return encrypted response

6. **Session end:**
   - Mark session inactive
   - Update metadata with final summary

### Message Push Format

Each message the daemon pushes to the server:

```rust
// 1. Build RawRecord
let raw = RawRecord {
    role: "agent",  // or "user"
    content: RawContent::Output {
        type_: "assistant",
        data: AssistantData {
            uuid: generate_uuid(),
            message: {
                role: "assistant",
                model: "claude-sonnet-4-20250514",
                content: vec![
                    ContentBlock::Text { text: "..." },
                    ContentBlock::ToolUse { id: "...", name: "Read", input: json!({...}) },
                ],
                usage: Some(UsageInfo { input_tokens: 100, output_tokens: 50, .. }),
            },
        },
    },
    meta: Some(MessageMeta {
        sent_from: "cli",
        permission_mode: "default",
    }),
};

// 2. Serialize to JSON
let json = serde_json::to_string(&raw)?;

// 3. Encrypt with session's AES key
let encrypted = aes_256_gcm_encrypt(&session_key, json.as_bytes());

// 4. Encode to base64
let wire = base64_encode(&[0x00].iter().chain(encrypted.iter()).collect::<Vec<_>>());

// 5. Emit via Socket.io
socket.emit("message", json!({
    "sid": session_id,
    "message": wire,
    "localId": generate_uuid(),
    "sentFrom": "cli",
    "permissionMode": "default"
}));
```

---

## 22. Session & Message Lifecycle

### Full Flow: User Types on Phone → Claude Code Receives

```
Mobile app                    Server                      tuicommander
    │                            │                              │
    │  1. User types message     │                              │
    │  2. Encrypt with           │                              │
    │     session AES key        │                              │
    │  3. emit("message",{...})  │                              │
    │───────────────────────────►│                              │
    │                            │  4. Store encrypted msg      │
    │                            │  5. Push to machine via      │
    │                            │     "update" event           │
    │                            │─────────────────────────────►│
    │                            │                              │
    │                            │  6. Decrypt message          │
    │                            │  7. Write to Claude Code     │
    │                            │     PTY stdin                │
    │                            │                              │
    │                            │  8. Claude Code processes    │
    │                            │     and responds via PTY     │
    │                            │                              │
    │                            │  9. Capture PTY output       │
    │                            │ 10. Parse into Messages      │
    │                            │ 11. Encrypt as RawRecord     │
    │                            │ 12. emit("message",{...})    │
    │                            │◄─────────────────────────────│
    │                            │                              │
    │ 13. Push to mobile via     │                              │
    │     "update" event         │                              │
    │◄───────────────────────────│                              │
    │                            │                              │
    │ 14. Decrypt, normalize,    │                              │
    │     reduce, display        │                              │
```

### Permission Request Flow

```
tuicommander                 Server                      Mobile app
    │                            │                              │
    │  Claude Code asks for      │                              │
    │  tool permission           │                              │
    │                            │                              │
    │  1. Push permission as     │                              │
    │     agentState update      │                              │
    │     (encrypted)            │                              │
    │───────────────────────────►│  2. Push to mobile           │
    │                            │─────────────────────────────►│
    │                            │                              │
    │                            │  3. User approves/denies     │
    │                            │  4. RPC: permission          │
    │                            │◄─────────────────────────────│
    │                            │                              │
    │  5. Receive RPC            │                              │
    │◄───────────────────────────│                              │
    │                            │                              │
    │  6. Write Y/N to PTY      │                              │
    │  7. Update agentState      │                              │
```

---

## 23. Module: `happy_client`

### Target location: `src-tauri/src/happy_client/`

```
src-tauri/src/happy_client/
├── mod.rs              // Public API, HappyClient struct
├── auth.rs             // QR auth, token refresh, secret key backup
├── credentials.rs      // Credential storage and retrieval
├── socket.rs           // Socket.io WebSocket connection
├── api.rs              // HTTP API client
├── types.rs            // Server protocol types
└── config.rs           // Happy-specific configuration
```

### Core struct

```rust
pub struct HappyClient {
    credentials: Option<HappyCredentials>,
    crypto: HappyCrypto,
    socket: Option<HappySocket>,
    http: HappyHttpClient,
    machine_id: String,
    config: HappyConfig,
}

impl HappyClient {
    /// Create client (does not connect)
    pub fn new(config: HappyConfig) -> Self;

    /// Check if authenticated
    pub fn is_authenticated(&self) -> bool;

    /// Start QR-based authentication flow
    /// Returns QR code data string for terminal display
    pub async fn start_auth(&mut self) -> Result<String>;

    /// Poll for auth completion (call after QR is scanned)
    pub async fn poll_auth(&mut self) -> Result<AuthStatus>;

    /// Restore from backup secret key
    pub async fn restore_from_secret(&mut self, secret_key: &str) -> Result<()>;

    /// Connect WebSocket (requires auth)
    pub async fn connect(&mut self) -> Result<()>;

    /// Disconnect
    pub async fn disconnect(&mut self);

    /// Register/update machine metadata
    pub async fn update_machine_metadata(&self, metadata: &MachineMetadata) -> Result<()>;

    /// Push a message to a session
    pub async fn push_message(
        &self,
        session_id: &str,
        raw_record: &RawRecord,
    ) -> Result<()>;

    /// Update session metadata
    pub async fn update_session_metadata(
        &self,
        session_id: &str,
        metadata: &SessionMetadata,
    ) -> Result<()>;

    /// Update session agent state
    pub async fn update_agent_state(
        &self,
        session_id: &str,
        state: &serde_json::Value,
    ) -> Result<()>;

    /// Report session activity (ephemeral)
    pub fn report_activity(
        &self,
        session_id: &str,
        active: bool,
        thinking: bool,
    );

    /// Report machine activity (ephemeral heartbeat)
    pub fn report_machine_heartbeat(&self);

    /// Subscribe to incoming events (RPC calls, messages from mobile)
    pub fn on_event(&self) -> broadcast::Receiver<HappyEvent>;
}

pub enum AuthStatus {
    Pending,
    Authorized(HappyCredentials),
    Rejected,
}

pub enum HappyEvent {
    /// Mobile user sent a message to a session
    IncomingMessage { session_id: String, raw_record: RawRecord },
    /// Mobile user sent an RPC call
    RpcCall { session_id: String, method: String, params: serde_json::Value, respond: oneshot::Sender<Result<serde_json::Value>> },
    /// Machine RPC (spawn session, stop daemon, etc.)
    MachineRpc { method: String, params: serde_json::Value, respond: oneshot::Sender<Result<serde_json::Value>> },
    /// Server requests machine metadata update
    MetadataRequest,
}
```

---

## 24. Module: `happy_crypto`

### Target location: `src-tauri/src/happy_client/crypto.rs`

Implements the complete Happy encryption protocol in Rust.

```rust
pub struct HappyCrypto {
    master_secret: Option<[u8; 32]>,
    content_data_key: Option<[u8; 32]>,
    content_keypair: Option<(BoxPublicKey, BoxSecretKey)>,
    /// Per-entity encryption keys, keyed by entity ID (session/machine)
    entity_keys: DashMap<String, [u8; 32]>,
}

impl HappyCrypto {
    pub fn new() -> Self;

    /// Initialize from master secret (after auth)
    pub fn init(&mut self, master_secret: [u8; 32]);

    // --- Key derivation ---

    /// HMAC-SHA512 tree-based key derivation
    fn derive_key(master: &[u8; 32], usage: &str, path: &[&str]) -> [u8; 32];

    /// Derive content data key and keypair
    fn derive_content_keypair(&self) -> (BoxPublicKey, BoxSecretKey);

    /// Derive anonymous analytics ID
    pub fn derive_anon_id(&self) -> String;

    // --- Box encryption (asymmetric, for key wrapping) ---

    /// Encrypt with recipient's public key (ephemeral keypair)
    pub fn encrypt_box(&self, data: &[u8], recipient_pk: &BoxPublicKey) -> Vec<u8>;

    /// Decrypt with own secret key
    pub fn decrypt_box(&self, bundle: &[u8], own_sk: &BoxSecretKey) -> Result<Vec<u8>>;

    // --- AES-256-GCM encryption (symmetric, for content) ---

    /// Encrypt JSON-serializable data with entity key
    pub fn encrypt_entity<T: Serialize>(&self, entity_id: &str, data: &T) -> Result<String>;

    /// Decrypt base64 blob with entity key, return deserialized
    pub fn decrypt_entity<T: DeserializeOwned>(&self, entity_id: &str, encrypted: &str) -> Result<T>;

    /// Batch decrypt messages
    pub fn decrypt_messages(&self, session_id: &str, messages: &[ApiMessage]) -> Result<Vec<RawRecord>>;

    // --- Key management ---

    /// Unwrap an entity's data encryption key using content keypair
    pub fn unwrap_entity_key(&self, wrapped_key: &str) -> Result<[u8; 32]>;

    /// Wrap a new data encryption key for storage on server
    pub fn wrap_entity_key(&self, key: &[u8; 32]) -> String;

    /// Register an entity's unwrapped key for use
    pub fn register_entity_key(&self, entity_id: &str, key: [u8; 32]);

    /// Generate a new random AES-256 key for a new entity
    pub fn generate_entity_key(&self) -> [u8; 32];
}
```

### Crate mapping

| Happy (JS) | Rust crate |
|-------------|-----------|
| `tweetnacl` / `libsodium-wrappers` | `sodiumoxide` or `crypto_box` + `crypto_secretbox` from RustCrypto |
| `rn-encryption` (AES-GCM) | `aes-gcm` from RustCrypto |
| HMAC-SHA512 | `hmac` + `sha2` from RustCrypto |
| Random bytes | `rand` |
| Base64 | `base64` (already in Cargo.toml) |

---

## 25. Module: `happy_sync`

Manages the two-way data flow between tuicommander and the Happy server.

```rust
pub struct HappySync {
    client: Arc<HappyClient>,
    context_store: Arc<ContextStore>,  // From Part I
    /// Maps PTY session_id → Happy session_id
    session_map: DashMap<String, String>,
}

impl HappySync {
    /// Start syncing a PTY session to Happy
    /// Creates a new Happy session, generates encryption key, registers
    pub async fn register_session(
        &self,
        pty_session_id: &str,
        metadata: &SessionMetadata,
    ) -> Result<String>; // Returns Happy session ID

    /// Called by context_store when new messages arrive
    /// Encrypts and pushes to Happy server
    pub async fn on_context_update(&self, update: &ContextUpdate);

    /// Called when RPC arrives from mobile
    /// Routes to correct PTY session
    pub async fn handle_rpc(&self, event: HappyEvent);

    /// Handle incoming message from mobile (user types on phone)
    /// Decrypts and writes to PTY stdin
    pub async fn handle_incoming_message(
        &self,
        session_id: &str,
        raw_record: &RawRecord,
    ) -> Result<()>;

    /// Periodic heartbeat
    pub async fn heartbeat(&self);
}
```

---

## 26. Module: `happy_daemon`

Wraps the full daemon lifecycle, equivalent to `happy-cli`'s daemon mode.

```rust
pub struct HappyDaemon {
    client: Arc<HappyClient>,
    sync: Arc<HappySync>,
    state: Arc<AppState>,       // tuicommander's global state
}

impl HappyDaemon {
    /// Start the daemon
    /// - Connects to Happy server
    /// - Reports machine metadata
    /// - Starts heartbeat loop
    /// - Listens for incoming RPCs
    pub async fn start(&self) -> Result<()>;

    /// Stop the daemon gracefully
    pub async fn stop(&self);

    /// Handle machine-level RPCs
    async fn handle_machine_rpc(&self, method: &str, params: serde_json::Value) -> Result<serde_json::Value> {
        match method {
            "spawn-happy-session" => {
                // Use existing agent spawning from agent.rs
                // Register new session with Happy
            },
            "stop-daemon" => {
                // Graceful shutdown
            },
            "bash" => {
                // Execute in a PTY, return output
            },
            _ => Err(anyhow!("Unknown method: {}", method)),
        }
    }

    /// Handle session-level RPCs (routed from HappySync)
    async fn handle_session_rpc(
        &self,
        pty_session_id: &str,
        method: &str,
        params: serde_json::Value,
    ) -> Result<serde_json::Value> {
        match method {
            "abort" => {
                // Send Ctrl+C to PTY
                write_pty(pty_session_id, "\x03")?;
                Ok(json!({}))
            },
            "permission" => {
                // Write Y or N to PTY based on decision
                let decision = params["decision"].as_str().unwrap_or("denied");
                let input = if decision.starts_with("approved") { "y\n" } else { "n\n" };
                write_pty(pty_session_id, input)?;
                Ok(json!({}))
            },
            "readFile" => {
                let path = params["path"].as_str().ok_or(anyhow!("missing path"))?;
                let content = tokio::fs::read_to_string(path).await?;
                Ok(json!({ "content": content }))
            },
            "writeFile" => {
                let path = params["path"].as_str().ok_or(anyhow!("missing path"))?;
                let content = params["content"].as_str().ok_or(anyhow!("missing content"))?;
                tokio::fs::write(path, content).await?;
                Ok(json!({}))
            },
            "listDirectory" => {
                // Use existing fs.rs
            },
            "ripgrep" => {
                // Shell out to rg
            },
            "killSession" => {
                close_pty(pty_session_id)?;
                Ok(json!({}))
            },
            _ => Err(anyhow!("Unknown session method: {}", method)),
        }
    }
}
```

---

## 27. Bridging Local Capture → Happy

The key architectural question: how do the two parts connect?

### Architecture

```
                        ┌─────────────────────────────┐
                        │       context_store          │
PTY reader thread ─────►│  (Part I — local capture)    │
                        │                              │
                        │  Messages accumulate here    │
                        └──────────┬──────────────────┘
                                   │
                          on_context_update()
                                   │
                     ┌─────────────▼─────────────┐
                     │        happy_sync          │
                     │  (Part II — network push)  │
                     │                            │
                     │  Encrypt → push to server  │
                     └─────────────┬─────────────┘
                                   │
                          emit("message", {...})
                                   │
                     ┌─────────────▼─────────────┐
                     │       happy-server         │
                     │  (remote, not our code)    │
                     └─────────────┬─────────────┘
                                   │
                         push to mobile app
                                   │
                     ┌─────────────▼─────────────┐
                     │       happy-coder          │
                     │  (mobile app, unchanged)   │
                     └───────────────────────────┘
```

### Event flow

1. **PTY output captured** → `ConversationReader` → `ConversationEvent`
2. **Event stored locally** → `ContextStore` → `Message` objects + `ContextUpdate`
3. **If Happy connected** → `HappySync.on_context_update()`:
   a. Convert `Message` → `RawRecord` (Happy's wire format)
   b. Encrypt with session AES key
   c. Emit via Socket.io
4. **If Happy not connected** → local-only mode (Part I still works)

### Conversion: `Message` → `RawRecord`

```rust
fn message_to_raw_record(message: &Message) -> RawRecord {
    match message {
        Message::AgentText { text, .. } => RawRecord {
            role: "agent",
            content: RawContent::Output {
                type_: "assistant",
                data: AssistantData {
                    message: ApiMessage {
                        role: "assistant",
                        content: vec![ContentBlock::Text { text: text.clone() }],
                        ..
                    }
                }
            },
            meta: default_meta(),
        },
        Message::UserText { text, .. } => RawRecord {
            role: "user",
            content: RawContent::Input {
                type_: "human",
                data: UserData {
                    message: ApiMessage {
                        role: "user",
                        content: vec![ContentBlock::Text { text: text.clone() }],
                        ..
                    }
                }
            },
            meta: default_meta(),
        },
        Message::ToolCall { tool, .. } => RawRecord {
            role: "agent",
            content: RawContent::Output {
                type_: "assistant",
                data: AssistantData {
                    message: ApiMessage {
                        role: "assistant",
                        content: vec![ContentBlock::ToolUse {
                            id: tool.id.clone(),
                            name: tool.name.clone(),
                            input: tool.input.clone(),
                        }],
                        ..
                    }
                }
            },
            meta: default_meta(),
        },
        _ => { /* skip events */ }
    }
}
```

---

## 28. Additional Crate Dependencies

For Part II, these crates are needed (not currently in Cargo.toml):

| Crate | Version | Use |
|-------|---------|-----|
| `sodiumoxide` or `crypto_box` | latest | Curve25519 key exchange, Box encryption |
| `aes-gcm` | 0.10+ | AES-256-GCM message encryption |
| `hmac` + `sha2` | 0.12+ | HMAC-SHA512 key derivation |
| `rust_socketio` | 0.6+ | Socket.io client (WebSocket) |
| `qrcode` | 0.14+ | QR code generation for terminal |
| `keyring` | 3+ | OS keychain credential storage |
| `data-encoding` | 2+ | Base32 encoding for secret key backup |

**Already available:** `reqwest`, `tokio`, `serde`, `serde_json`, `base64`, `uuid`, `dashmap`, `rand`.

---

## 29. Security Considerations

### Credential Storage

- **Primary:** OS keychain via `keyring` crate (macOS Keychain, Linux Secret Service, Windows Credential Manager)
- **Fallback:** Encrypted file at `~/.tuicommander/happy-credentials.enc`
- **Never:** Plain text on disk

### Memory Safety

- Master secret and AES keys are `[u8; 32]`, not `String` — no accidental logging
- Use `zeroize` crate to clear secret memory on drop
- Never serialize secrets to JSON/logs

### Transport Security

- All connections over TLS (HTTPS/WSS)
- Server certificate validation via `reqwest`'s default TLS backend
- No HTTP fallback

### Trust Boundary

- tuicommander trusts the Happy server (it's the user's own backend)
- tuicommander does NOT trust incoming RPC content — validate all file paths, commands
- Sandbox file operations to the session's working directory where possible

---

## 30. Happy Integration Open Questions

1. **Socket.io crate maturity:** `rust_socketio` is the most maintained Rust Socket.io client, but it may not support all Socket.io v4 features. Evaluate if we need a custom WebSocket implementation with Socket.io v4 framing.

2. **Crypto crate choice:** Should we use `sodiumoxide` (wraps libsodium C library, battle-tested) or pure-Rust `crypto_box`/`crypto_secretbox` from RustCrypto (no C dependency, easier cross-compilation)?

3. **happy-server API stability:** The server API is not formally versioned. Changes could break the client. Should we implement a version negotiation handshake?

4. **Dual-mode operation:** When Happy is connected, should local context capture (Part I) still run independently? Or should Happy sync be the sole consumer? Recommendation: always run Part I locally, with Happy sync as an optional overlay.

5. **Session ID mapping:** tuicommander uses UUID v4 for PTY sessions. Happy uses its own session IDs. Do we reuse tuicommander's IDs or let Happy assign them?

6. **Offline buffering:** If the WebSocket disconnects, should we buffer messages locally and replay on reconnect? What's the max buffer size?

7. **Multiple machines:** Can one tuicommander instance manage multiple machine identities? Or one machine per installation?

8. **happy-cli compatibility:** Should we aim for exact behavioral compatibility with `happy-cli` (so the mobile app can't tell the difference), or is "functionally equivalent" sufficient?

9. **Authentication UX in TUI:** The QR code flow works well in a terminal (unicode block QR), but what about headless/SSH scenarios? The secret key backup restore flow may be the primary auth method for tuicommander.

10. **Scope:** Do we need to implement the full social features (friends, feed, artifacts) or just session/message sync? Recommendation: start with session/message/machine sync only — that's what `happy-cli` does.
