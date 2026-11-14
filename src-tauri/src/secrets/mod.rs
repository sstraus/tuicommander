//! In-memory secrets. No raw-value getter, serialization, logging or persistence.
pub(crate) mod account_sink;
#[cfg(test)]
mod account_sink_tests;
pub(crate) mod forms;
mod mask;
pub(crate) mod policy;
pub(crate) mod provider;
pub(crate) mod provider_op;
#[cfg(test)]
mod provider_tests;
mod run;
mod schema;
#[cfg(test)]
mod stream_query_tests;
mod task;
mod terminal_queries;
pub(crate) use mask::StreamMask;
use schema::valid_name;

/// The one rule for a submitted secret value: non-empty, at most 16 KiB, no NUL.
fn valid_secret_value(value: &str) -> bool {
    !value.is_empty() && value.len() <= 16384 && !value.contains('\0')
}
pub(crate) use schema::{Field, FieldKind, Form};
#[cfg(test)]
mod argv_consent_tests;
#[cfg(test)]
mod tests;
#[cfg(test)]
mod tests_critic1435;
#[cfg(test)]
mod tests_critic1435s;
#[cfg(test)]
mod tests_critic1435s3;

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use zeroize::{Zeroize, Zeroizing};

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct Submission {
    pub nonce: String,
    pub status: String,
    #[serde(default)]
    pub values: BTreeMap<String, String>,
    pub template: Option<Vec<String>>,
}
impl Drop for Submission {
    fn drop(&mut self) {
        self.nonce.zeroize();
        for value in self.values.values_mut() {
            value.zeroize();
        }
    }
}
#[cfg(test)]
impl Submission {
    fn stored(values: BTreeMap<String, String>) -> Self {
        Self {
            nonce: String::new(),
            status: "stored".into(),
            values,
            template: None,
        }
    }
    fn declined() -> Self {
        Self {
            nonce: String::new(),
            status: "declined".into(),
            values: BTreeMap::new(),
            template: None,
        }
    }
}

#[derive(Clone, Serialize)]
pub(crate) struct Status {
    pub names: Vec<String>,
    pub status: String,
}

struct Pending {
    form: Form,
    sink: Option<account_sink::Request>,
    response: Option<tokio::sync::oneshot::Sender<Status>>,
}

struct ApprovedTemplate {
    argv: policy::Template,
    names: Vec<String>,
    cwd: String,
}

#[derive(Clone)]
struct FormValue {
    value: Zeroizing<String>,
    kind: FieldKind,
}

impl FormValue {
    fn as_str(&self) -> &str {
        &self.value
    }
    fn mask_representations(&self) -> Vec<Zeroizing<String>> {
        if self.kind == FieldKind::Username {
            Vec::new()
        } else {
            mask::representations(&self.value)
        }
    }
}

#[derive(Default)]
struct Inner {
    values: BTreeMap<String, FormValue>,
    templates: Vec<ApprovedTemplate>,
    scopes: BTreeMap<String, task::ScopedValues>,
    pending: Option<Pending>,
}

/// Owned snapshot of the mask needles of every stored secret.
pub(crate) struct Masker(Vec<Zeroizing<String>>);

impl Masker {
    pub(crate) fn mask(&self, text: &str) -> String {
        mask::mask(text, &self.0)
    }
}

#[derive(Default)]
pub(crate) struct SecretStore {
    inner: parking_lot::Mutex<Inner>,
    session_masks: dashmap::DashMap<String, std::sync::Arc<Vec<Zeroizing<String>>>>,
    // PTY sessions whose owner granted literal diagnostic reads (#2643-6b38).
    // Set only through the desktop IPC owner control; never persisted.
    literal_reads: dashmap::DashSet<String>,
    // Window lifetime outlives the pending reply. Inspection stays blocked until
    // native destruction, including a submit/close race and failed destruction.
    // open() allows one form, so at most one window exists.
    window: parking_lot::Mutex<Option<String>>,
}

impl SecretStore {
    pub(crate) fn tools_blocked(&self) -> bool {
        self.inner.lock().pending.is_some() || self.window.lock().is_some()
    }
    // DEFERRED (2026-10-11) — test-only: production opens forms through open_with_sink.
    #[cfg(test)]
    pub(crate) fn open(&self, form: Form) -> Result<Form, String> {
        self.open_with_sink(form, None)
    }
    fn open_with_sink(
        &self,
        form: Form,
        sink: Option<account_sink::Request>,
    ) -> Result<Form, String> {
        let mut inner = self.inner.lock();
        if inner.pending.is_some() || self.window.lock().is_some() {
            return Err("A secret form is already open".into());
        }
        inner.pending = Some(Pending {
            form: form.clone(),
            sink,
            response: None,
        });
        Ok(form)
    }
    #[cfg(any(feature = "desktop", test))]
    fn has_window(&self, label: &str) -> bool {
        self.window.lock().as_deref() == Some(label)
    }
    #[cfg(feature = "desktop")]
    fn release_window(&self, label: &str) {
        let mut window = self.window.lock();
        if window.as_deref() == Some(label) {
            *window = None;
        }
    }
    fn public_form(&self, nonce: &str) -> Result<Form, String> {
        let inner = self.inner.lock();
        inner
            .pending
            .as_ref()
            .filter(|p| p.form.nonce == nonce && p.sink.is_none())
            .map(|p| p.form.clone())
            .ok_or_else(|| "Unknown or expired secret form".into())
    }
    fn form(&self, nonce: &str) -> Result<Form, String> {
        let inner = self.inner.lock();
        inner
            .pending
            .as_ref()
            .filter(|p| p.form.nonce == nonce)
            .map(|p| p.form.clone())
            .ok_or_else(|| "Unknown or expired secret form".into())
    }
    pub(crate) fn submit(&self, nonce: &str, mut submission: Submission) -> Result<Status, String> {
        let mut inner = self.inner.lock();
        if inner.pending.as_ref().is_some_and(|p| p.sink.is_some()) {
            return Err("Account entry requires the private native window".into());
        }
        let form = &inner
            .pending
            .as_ref()
            .filter(|p| p.form.nonce == nonce)
            .ok_or("Unknown or expired secret form")?
            .form;
        let names: Vec<String> = form.fields.iter().map(|f| f.name.clone()).collect();
        let mut template = None;
        if submission.status == "declined" {
            if !submission.values.is_empty() || submission.template.is_some() {
                return Err("Decline must not contain values or templates".into());
            }
        } else if form.argv.is_some() {
            if submission.status != "approved" || !submission.values.is_empty() {
                return Err("Invalid command approval".into());
            }
            if let Some(argv) = submission.template.take() {
                let candidate = policy::Template::new(argv)?;
                if !candidate.matches(form.argv.as_ref().ok_or("Missing command")?) {
                    return Err("Template does not match this exact command".into());
                }
                template = Some(candidate);
            }
        } else {
            if submission.status != "stored" || submission.template.is_some() {
                return Err("Invalid secret submission".into());
            }
            let expected: Vec<&str> = form
                .fields
                .iter()
                .filter(|f| f.kind != FieldKind::Sso)
                .map(|f| f.name.as_str())
                .collect();
            if submission.values.len() != expected.len()
                || expected.iter().any(|name| {
                    !submission
                        .values
                        .get(*name)
                        .is_some_and(|v| valid_secret_value(v))
                })
            {
                return Err("Values must match the requested fields".into());
            }
        }
        // Validate the whole payload before consuming the nonce or mutating values.
        if submission.status == "stored" {
            let kinds: BTreeMap<_, _> = form
                .fields
                .iter()
                .map(|field| (field.name.clone(), field.kind.clone()))
                .collect();
            for (name, value) in std::mem::take(&mut submission.values) {
                let kind = kinds[&name].clone();
                inner.values.insert(
                    name,
                    FormValue {
                        value: Zeroizing::new(value),
                        kind,
                    },
                );
            }
        }
        if let Some(template) = template {
            let cwd = inner
                .pending
                .as_ref()
                .and_then(|p| p.form.cwd.clone())
                .ok_or("Missing approval directory")?;
            inner.templates.push(ApprovedTemplate {
                argv: template,
                names: names.clone(),
                cwd,
            });
        }
        let result = Status {
            names,
            status: submission.status.clone(),
        };
        if let Some(mut pending) = inner.pending.take()
            && let Some(tx) = pending.response.take()
        {
            let _ = tx.send(result.clone());
        }
        Ok(result)
    }
    pub(crate) fn mask(&self, text: &str) -> String {
        self.masker().mask(text)
    }
    /// Snapshot the needles under the lock; masking with the result holds no lock,
    /// so a large text never stalls other secret operations.
    pub(crate) fn masker(&self) -> Masker {
        let inner = self.inner.lock();
        Masker(
            inner
                .values
                .values()
                .flat_map(FormValue::mask_representations)
                .collect(),
        )
    }
    /// Mask registered values in full output without moving source-byte cursors.
    pub(crate) fn mask_preserving_offsets(&self, bytes: &[u8]) -> Zeroizing<Vec<u8>> {
        let inner = self.inner.lock();
        let needles: Vec<_> = inner
            .values
            .values()
            .flat_map(FormValue::mask_representations)
            .collect();
        mask::mask_preserving_offsets(bytes, &needles)
    }
    pub(crate) fn clear(&self) {
        let mut inner = self.inner.lock();
        inner.values.clear();
        inner.templates.clear();
        inner.scopes.clear();
        inner.pending.take();
    }
    fn allowed(&self, argv: &[String], names: &[String], cwd: &str) -> bool {
        self.inner
            .lock()
            .templates
            .iter()
            .any(|t| t.argv.matches(argv) && t.names == names && t.cwd == cwd)
    }
    pub(crate) fn remove(&self, names: &[String]) {
        let mut inner = self.inner.lock();
        for name in names {
            inner.values.remove(name);
        }
    }
    fn environment(&self, names: &[String]) -> Result<BTreeMap<String, FormValue>, String> {
        let inner = self.inner.lock();
        names
            .iter()
            .map(|name| {
                inner
                    .values
                    .get(name)
                    .map(|value| (name.clone(), value.clone()))
                    .ok_or_else(|| "Requested secret is missing".into())
            })
            .collect()
    }
}

pub(crate) use forms::{form_http, submit_http};

/// Owner grant for literal diagnostic reads of one PTY session (#2643-6b38).
/// Desktop IPC only: models reach loopback HTTP and MCP, never the WebView IPC.
#[cfg(feature = "desktop")]
#[tauri::command]
pub(crate) fn set_terminal_literal_reads(
    state: tauri::State<'_, std::sync::Arc<crate::AppState>>,
    session_id: String,
    enabled: bool,
) -> serde_json::Value {
    state.secrets.set_literal_reads(&session_id, enabled);
    serde_json::json!({
        "session_id": session_id,
        "enabled": state.secrets.literal_reads_granted(&session_id),
    })
}
pub(crate) use run::handle_secret;

pub(crate) fn tool_definition() -> serde_json::Value {
    serde_json::json!({
        "name": "secret",
        "description": "Put task-scoped write-only values with a TTL in seconds, or request a private sensitive-field form, then run a user-approved argv with named values in the child environment only. Values are never returned. An approved command can exfiltrate. On a desktop host request opens a separate native form; HTTP/mobile entry uses its private one-time capability link. run takes names, argv, cwd and an optional reason, and uses user-approved command templates or asks for exact argv consent; consent, not option or program names, decides, and the program is spawned directly without a shell. Templates match exact argv, names and directory. Username fields are injected but never registered as output masks. Password, OTP and put values remain masked. Output masks exact sensitive values, JSON-escaped values and base64, hex and URL encodings, including line wraps and embedded Basic-auth payloads. put may take op:// references instead of values: they resolve once at put time through the same scope store, and a failed resolve stores nothing. Put values are selected with agent spawn scope and injected only into child env. TTL revokes new injection; masks survive until session teardown. Form values are shared by name across callers of this backend process and survive MCP reconnects. remove clears form-store query masks; existing terminal streams retain learned masks until session teardown, and in-flight approved commands retain their captured masks until completion. Already sanitized retained output is unchanged. remove zeroizes named values and returns removed, including already absent names; it does not report prior presence. On a headless host request opens no window: Web Push sends the owner its private one-time phone link and the call waits for submission; with a desktop the phone can also submit the desktop-opened form.",
        "inputSchema": {"type": "object", "properties": {
            "action": {"type": "string", "enum": ["request", "run", "remove", "put"]},
            "fields": {"type": "array", "items": {"type": "object", "properties": {
                "name": {"type": "string"}, "kind": {"type": "string", "enum": ["username", "password", "otp", "sso"]}, "display": {"type": "string", "description": "HTTP(S) SSO link to display without navigating"}
            }, "required": ["name", "kind"], "additionalProperties": false}},
            "values": {"type": "array", "items": {"type": "string"}, "description": "Write-only values in names order (put)"},
            "references": {"type": "array", "items": {"type": "object", "properties": {
                "name": {"type": "string", "description": "Env var name in the child"},
                "op": {"type": "string", "description": "op://vault/item/field, resolved now with the 1Password CLI"},
                "account": {"type": "string", "description": "1Password account shorthand (op only)"}
            }, "required": ["name", "op"], "additionalProperties": false}, "description": "put: resolve references instead of taking values; the reply carries names only"},
            "scope": {"type": "string", "description": "Task id (put); agent spawn scope selects these values"},
            "ttl": {"type": "integer", "minimum": 1, "description": "Seconds until new injection is revoked; existing session masks survive until teardown"},
            "reason": {"type": "string"}, "names": {"type": "array", "items": {"type": "string"}},
            "argv": {"type": "array", "items": {"type": "string"}}, "cwd": {"type": "string"}
        }, "required": ["action"], "additionalProperties": false}
    })
}

#[cfg(test)]
mod username_tests;
