//! Chat Registry — Rust-side source of truth for AI chat conversation state.
//!
//! Each chat has a `ChatSlot` holding the conversation state and a list of
//! `Subscriber` channels. Fan-out sends events to all subscribers **outside**
//! the slot mutex to prevent slow renderers from blocking writers.

use dashmap::DashMap;
use serde::Serialize;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use tokio::sync::Mutex;

// ---------------------------------------------------------------------------
// Public types
// ---------------------------------------------------------------------------

/// Events pushed to each subscriber's Channel.
#[derive(Clone, Debug, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum ChatEvent {
    Snapshot(ConversationStateSnapshot),
    Chunk { delta: String },
    Error { message: String },
    Cleared,
}

/// Serializable snapshot of a chat's full state.
#[derive(Clone, Debug, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ConversationStateSnapshot {
    pub messages: Vec<ChatMessage>,
    pub is_streaming: bool,
    pub streaming_text: String,
    pub error: Option<String>,
    pub attached_session_id: Option<String>,
    pub pinned: bool,
}

/// A single chat message stored in the registry.
#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ChatMessage {
    pub role: String,
    pub content: String,
    pub timestamp: u64,
}

pub type SubscriptionId = u64;

/// Result returned from `subscribe`.
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SubscribeResult {
    pub subscription_id: SubscriptionId,
    pub snapshot: ConversationStateSnapshot,
}

// ---------------------------------------------------------------------------
// Internal types
// ---------------------------------------------------------------------------

const MAX_MESSAGES: usize = 100;

struct Subscriber {
    id: SubscriptionId,
    channel: tauri::ipc::Channel<ChatEvent>,
}

struct ChatSlot {
    state: ConversationState,
    subscribers: Vec<Subscriber>,
}

/// Mutable conversation state (not directly serialized — use `snapshot()`).
#[derive(Debug, Default)]
pub(crate) struct ConversationState {
    messages: Vec<ChatMessage>,
    is_streaming: bool,
    streaming_text: String,
    error: Option<String>,
    attached_session_id: Option<String>,
    pinned: bool,
}

impl ConversationState {
    fn snapshot(&self) -> ConversationStateSnapshot {
        ConversationStateSnapshot {
            messages: self.messages.clone(),
            is_streaming: self.is_streaming,
            streaming_text: self.streaming_text.clone(),
            error: self.error.clone(),
            attached_session_id: self.attached_session_id.clone(),
            pinned: self.pinned,
        }
    }
}

// ---------------------------------------------------------------------------
// ChatRegistry
// ---------------------------------------------------------------------------

pub struct ChatRegistry {
    chats: DashMap<String, Arc<Mutex<ChatSlot>>>,
    next_sub_id: AtomicU64,
}

impl ChatRegistry {
    pub fn new() -> Self {
        Self {
            chats: DashMap::new(),
            next_sub_id: AtomicU64::new(1),
        }
    }

    /// Get or create a chat slot for the given ID.
    fn get_or_create(&self, chat_id: &str) -> Arc<Mutex<ChatSlot>> {
        self.chats
            .entry(chat_id.to_string())
            .or_insert_with(|| {
                Arc::new(Mutex::new(ChatSlot {
                    state: ConversationState::default(),
                    subscribers: Vec::new(),
                }))
            })
            .clone()
    }

    /// Take a snapshot of the current conversation state.
    pub async fn snapshot(&self, chat_id: &str) -> ConversationStateSnapshot {
        let slot = self.get_or_create(chat_id);
        let guard = slot.lock().await;
        guard.state.snapshot()
    }

    /// Mutate conversation state via a closure and return the new snapshot.
    pub async fn update<F>(&self, chat_id: &str, f: F) -> ConversationStateSnapshot
    where
        F: FnOnce(&mut ConversationState),
    {
        let slot = self.get_or_create(chat_id);
        let mut guard = slot.lock().await;
        f(&mut guard.state);
        // Cap messages
        if guard.state.messages.len() > MAX_MESSAGES {
            let excess = guard.state.messages.len() - MAX_MESSAGES;
            guard.state.messages.drain(..excess);
        }
        guard.state.snapshot()
    }

    /// Subscribe a Channel to receive events for a chat.
    /// Returns the subscription ID and a snapshot of the current state
    /// (taken under the same lock to prevent races).
    pub async fn subscribe(
        &self,
        chat_id: &str,
        channel: tauri::ipc::Channel<ChatEvent>,
    ) -> SubscribeResult {
        let slot = self.get_or_create(chat_id);
        let mut guard = slot.lock().await;
        let id = self.next_sub_id.fetch_add(1, Ordering::Relaxed);
        let snapshot = guard.state.snapshot();
        guard.subscribers.push(Subscriber {
            id,
            channel,
        });
        SubscribeResult {
            subscription_id: id,
            snapshot,
        }
    }

    /// Remove a subscriber by ID.
    pub async fn unsubscribe(&self, chat_id: &str, sub_id: SubscriptionId) {
        if let Some(slot) = self.chats.get(chat_id) {
            let mut guard = slot.lock().await;
            guard.subscribers.retain(|s| s.id != sub_id);
        }
    }

    /// Fan-out an event to all subscribers of a chat.
    ///
    /// **Critical invariant:** `channel.send()` is called **outside** the slot
    /// mutex. We clone the subscriber handles under the lock, drop it, then
    /// send. Dead channels (send returns Err) are garbage-collected via a
    /// short re-lock.
    pub async fn fan_out(&self, chat_id: &str, event: ChatEvent) {
        let slot = match self.chats.get(chat_id) {
            Some(s) => s.clone(),
            None => return,
        };

        // Step 1: clone subscriber handles under lock
        let subs: Vec<(SubscriptionId, tauri::ipc::Channel<ChatEvent>)> = {
            let guard = slot.lock().await;
            guard
                .subscribers
                .iter()
                .map(|s| (s.id, s.channel.clone()))
                .collect()
        };
        // Lock is dropped here

        if subs.is_empty() {
            return;
        }

        // Step 2: send outside lock, collect dead IDs
        let mut dead_ids = Vec::new();
        for (id, channel) in &subs {
            if channel.send(event.clone()).is_err() {
                dead_ids.push(*id);
            }
        }

        // Step 3: GC dead subscribers
        if !dead_ids.is_empty() {
            let mut guard = slot.lock().await;
            guard.subscribers.retain(|s| !dead_ids.contains(&s.id));
        }
    }

    /// Convenience: update state + fan_out a Snapshot event.
    pub async fn update_and_notify<F>(&self, chat_id: &str, f: F)
    where
        F: FnOnce(&mut ConversationState),
    {
        let snap = self.update(chat_id, f).await;
        self.fan_out(chat_id, ChatEvent::Snapshot(snap)).await;
    }

    /// Append a message to the chat.
    pub async fn append_message(&self, chat_id: &str, msg: ChatMessage) {
        self.update_and_notify(chat_id, |s| {
            s.messages.push(msg);
        })
        .await;
    }

    /// Update streaming text (append delta) and fan-out a Chunk event.
    pub async fn append_streaming_chunk(&self, chat_id: &str, delta: &str) {
        {
            let slot = self.get_or_create(chat_id);
            let mut guard = slot.lock().await;
            guard.state.streaming_text.push_str(delta);
        }
        self.fan_out(chat_id, ChatEvent::Chunk { delta: delta.to_string() })
            .await;
    }

    /// Clear conversation state and notify subscribers.
    pub async fn clear(&self, chat_id: &str) {
        self.update(chat_id, |s| {
            s.messages.clear();
            s.is_streaming = false;
            s.streaming_text.clear();
            s.error = None;
        })
        .await;
        self.fan_out(chat_id, ChatEvent::Cleared).await;
    }

    /// Number of active chats (for testing/debug).
    pub fn chat_count(&self) -> usize {
        self.chats.len()
    }

    /// Number of subscribers for a chat (for testing/debug).
    pub async fn subscriber_count(&self, chat_id: &str) -> usize {
        match self.chats.get(chat_id) {
            Some(slot) => slot.lock().await.subscribers.len(),
            None => 0,
        }
    }
}

// ---------------------------------------------------------------------------
// Tauri commands
// ---------------------------------------------------------------------------

fn validate_id(id: &str) -> Result<(), String> {
    if id.is_empty() || id.len() > 64 {
        return Err("ID must be 1-64 characters".to_string());
    }
    if !id.chars().all(|c| c.is_alphanumeric() || c == '-' || c == '_') {
        return Err("ID must be alphanumeric, dash, or underscore".to_string());
    }
    Ok(())
}

#[tauri::command]
pub(crate) async fn chat_subscribe(
    registry: tauri::State<'_, ChatRegistry>,
    chat_id: String,
    on_event: tauri::ipc::Channel<ChatEvent>,
) -> Result<SubscribeResult, String> {
    validate_id(&chat_id)?;
    Ok(registry.subscribe(&chat_id, on_event).await)
}

#[tauri::command]
pub(crate) async fn chat_unsubscribe(
    registry: tauri::State<'_, ChatRegistry>,
    chat_id: String,
    subscription_id: SubscriptionId,
) -> Result<(), String> {
    validate_id(&chat_id)?;
    registry.unsubscribe(&chat_id, subscription_id).await;
    Ok(())
}

#[tauri::command]
pub(crate) async fn chat_get_state(
    registry: tauri::State<'_, ChatRegistry>,
    chat_id: String,
) -> Result<ConversationStateSnapshot, String> {
    validate_id(&chat_id)?;
    Ok(registry.snapshot(&chat_id).await)
}

#[tauri::command]
pub(crate) async fn chat_attach_terminal(
    registry: tauri::State<'_, ChatRegistry>,
    chat_id: String,
    session_id: String,
) -> Result<(), String> {
    validate_id(&chat_id)?;
    validate_id(&session_id)?;
    registry
        .update_and_notify(&chat_id, |s| {
            s.set_attached_session_id(Some(session_id));
        })
        .await;
    Ok(())
}

#[tauri::command]
pub(crate) async fn chat_detach_terminal(
    registry: tauri::State<'_, ChatRegistry>,
    chat_id: String,
) -> Result<(), String> {
    validate_id(&chat_id)?;
    registry
        .update_and_notify(&chat_id, |s| {
            s.set_attached_session_id(None);
        })
        .await;
    Ok(())
}

#[tauri::command]
pub(crate) async fn chat_clear(
    registry: tauri::State<'_, ChatRegistry>,
    chat_id: String,
) -> Result<(), String> {
    validate_id(&chat_id)?;
    registry.clear(&chat_id).await;
    Ok(())
}

#[tauri::command]
pub(crate) async fn chat_set_pinned(
    registry: tauri::State<'_, ChatRegistry>,
    chat_id: String,
    pinned: bool,
) -> Result<(), String> {
    validate_id(&chat_id)?;
    registry
        .update_and_notify(&chat_id, |s| {
            s.set_pinned(pinned);
        })
        .await;
    Ok(())
}

#[tauri::command]
pub(crate) async fn chat_push_message(
    registry: tauri::State<'_, ChatRegistry>,
    chat_id: String,
    role: String,
    content: String,
) -> Result<(), String> {
    validate_id(&chat_id)?;
    let ts = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64;
    let msg = ChatMessage {
        role,
        content,
        timestamp: ts,
    };
    registry.append_message(&chat_id, msg).await;
    Ok(())
}

// Expose ConversationState fields for update closures in other modules
impl ConversationState {
    pub fn set_streaming(&mut self, streaming: bool) {
        self.is_streaming = streaming;
    }
    pub fn set_streaming_text(&mut self, text: String) {
        self.streaming_text = text;
    }
    pub fn set_error(&mut self, error: Option<String>) {
        self.error = error;
    }
    pub fn set_attached_session_id(&mut self, id: Option<String>) {
        self.attached_session_id = id;
    }
    pub fn set_pinned(&mut self, pinned: bool) {
        self.pinned = pinned;
    }
    pub fn push_message(&mut self, msg: ChatMessage) {
        self.messages.push(msg);
    }
    pub fn messages(&self) -> &[ChatMessage] {
        &self.messages
    }
    pub fn streaming_text(&self) -> &str {
        &self.streaming_text
    }
    pub fn is_streaming(&self) -> bool {
        self.is_streaming
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    fn make_msg(role: &str, content: &str) -> ChatMessage {
        ChatMessage {
            role: role.to_string(),
            content: content.to_string(),
            timestamp: 0,
        }
    }

    #[tokio::test]
    async fn test_get_or_create_returns_empty_state() {
        let registry = ChatRegistry::new();
        let snap = registry.snapshot("chat-1").await;
        assert!(snap.messages.is_empty());
        assert!(!snap.is_streaming);
        assert!(snap.streaming_text.is_empty());
        assert!(snap.error.is_none());
        assert!(snap.attached_session_id.is_none());
        assert!(!snap.pinned);
    }

    #[tokio::test]
    async fn test_append_message_and_cap() {
        let registry = ChatRegistry::new();
        for i in 0..110 {
            let snap = registry
                .update("chat-1", |s| {
                    s.push_message(make_msg("user", &format!("msg-{i}")));
                })
                .await;
            if i < MAX_MESSAGES - 1 {
                assert_eq!(snap.messages.len(), i + 1);
            }
        }
        let snap = registry.snapshot("chat-1").await;
        assert_eq!(snap.messages.len(), MAX_MESSAGES);
        assert_eq!(snap.messages[0].content, "msg-10");
        assert_eq!(snap.messages[MAX_MESSAGES - 1].content, "msg-109");
    }

    #[tokio::test]
    async fn test_update_streaming_text_concurrent() {
        let registry = Arc::new(ChatRegistry::new());
        let mut handles = Vec::new();
        for i in 0..10 {
            let reg = registry.clone();
            handles.push(tokio::spawn(async move {
                reg.update("chat-1", |s| {
                    s.streaming_text.push_str(&format!("chunk-{i}"));
                })
                .await;
            }));
        }
        for h in handles {
            h.await.unwrap();
        }
        let snap = registry.snapshot("chat-1").await;
        // All 10 chunks should be present (order may vary)
        for i in 0..10 {
            assert!(
                snap.streaming_text.contains(&format!("chunk-{i}")),
                "Missing chunk-{i} in: {}",
                snap.streaming_text
            );
        }
    }

    #[tokio::test]
    async fn test_snapshot_serializable() {
        let registry = ChatRegistry::new();
        registry
            .update("chat-1", |s| {
                s.push_message(make_msg("user", "hello"));
                s.is_streaming = true;
                s.streaming_text = "partial".to_string();
                s.pinned = true;
            })
            .await;
        let snap = registry.snapshot("chat-1").await;
        let json = serde_json::to_string(&snap).unwrap();
        assert!(json.contains("\"isStreaming\":true"));
        assert!(json.contains("\"pinned\":true"));
        assert!(json.contains("\"role\":\"user\""));
    }

    #[tokio::test]
    async fn test_clear_resets_state() {
        let registry = ChatRegistry::new();
        registry
            .update("chat-1", |s| {
                s.push_message(make_msg("user", "hello"));
                s.is_streaming = true;
                s.streaming_text = "partial".to_string();
                s.error = Some("oops".to_string());
            })
            .await;
        registry.clear("chat-1").await;
        let snap = registry.snapshot("chat-1").await;
        assert!(snap.messages.is_empty());
        assert!(!snap.is_streaming);
        assert!(snap.streaming_text.is_empty());
        assert!(snap.error.is_none());
    }

    #[tokio::test]
    async fn test_chat_count() {
        let registry = ChatRegistry::new();
        assert_eq!(registry.chat_count(), 0);
        registry.snapshot("a").await;
        assert_eq!(registry.chat_count(), 1);
        registry.snapshot("b").await;
        assert_eq!(registry.chat_count(), 2);
    }

    #[tokio::test]
    async fn test_chat_event_serialization() {
        let chunk = ChatEvent::Chunk {
            delta: "hello".to_string(),
        };
        let json = serde_json::to_string(&chunk).unwrap();
        assert!(json.contains("\"kind\":\"chunk\""));
        assert!(json.contains("\"delta\":\"hello\""));

        let cleared = ChatEvent::Cleared;
        let json = serde_json::to_string(&cleared).unwrap();
        assert!(json.contains("\"kind\":\"cleared\""));

        let error = ChatEvent::Error {
            message: "fail".to_string(),
        };
        let json = serde_json::to_string(&error).unwrap();
        assert!(json.contains("\"kind\":\"error\""));

        let snap = ChatEvent::Snapshot(ConversationStateSnapshot::default());
        let json = serde_json::to_string(&snap).unwrap();
        assert!(json.contains("\"kind\":\"snapshot\""));
        assert!(json.contains("\"isStreaming\":false"));
    }

    #[test]
    fn test_validate_id_accepts_valid() {
        assert!(validate_id("chat-1").is_ok());
        assert!(validate_id("abc_123-def").is_ok());
        assert!(validate_id("a").is_ok());
    }

    #[test]
    fn test_validate_id_rejects_invalid() {
        assert!(validate_id("").is_err());
        assert!(validate_id(&"x".repeat(65)).is_err());
        assert!(validate_id("has spaces").is_err());
        assert!(validate_id("path/traversal").is_err());
        assert!(validate_id("dots.bad").is_err());
    }

    #[tokio::test]
    async fn test_subscribe_returns_unique_ids() {
        let registry = ChatRegistry::new();
        // We can't create real Channel<T> in tests without a webview,
        // but we can verify the AtomicU64 counter increments.
        let id1 = registry.next_sub_id.fetch_add(1, Ordering::Relaxed);
        let id2 = registry.next_sub_id.fetch_add(1, Ordering::Relaxed);
        assert_ne!(id1, id2);
        assert_eq!(id2, id1 + 1);
    }

    #[tokio::test]
    async fn test_append_streaming_chunk() {
        let registry = ChatRegistry::new();
        // Prepare a chat in streaming mode
        registry
            .update("chat-1", |s| {
                s.set_streaming(true);
            })
            .await;
        registry.append_streaming_chunk("chat-1", "hello ").await;
        registry.append_streaming_chunk("chat-1", "world").await;
        let snap = registry.snapshot("chat-1").await;
        assert_eq!(snap.streaming_text, "hello world");
    }

    #[tokio::test]
    async fn test_attach_detach_terminal() {
        let registry = ChatRegistry::new();
        registry
            .update("chat-1", |s| {
                s.set_attached_session_id(Some("sess-42".to_string()));
            })
            .await;
        let snap = registry.snapshot("chat-1").await;
        assert_eq!(snap.attached_session_id.as_deref(), Some("sess-42"));

        registry
            .update("chat-1", |s| {
                s.set_attached_session_id(None);
            })
            .await;
        let snap = registry.snapshot("chat-1").await;
        assert!(snap.attached_session_id.is_none());
    }

    #[tokio::test]
    async fn test_set_pinned() {
        let registry = ChatRegistry::new();
        registry
            .update("chat-1", |s| {
                s.set_pinned(true);
            })
            .await;
        let snap = registry.snapshot("chat-1").await;
        assert!(snap.pinned);

        registry
            .update("chat-1", |s| {
                s.set_pinned(false);
            })
            .await;
        let snap = registry.snapshot("chat-1").await;
        assert!(!snap.pinned);
    }
}
