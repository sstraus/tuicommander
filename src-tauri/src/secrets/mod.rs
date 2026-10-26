//! In-memory secrets. No raw-value getter, serialization, logging or persistence.
pub(crate) mod account_sink;
#[cfg(test)]
mod account_sink_tests;
pub(crate) mod forms;
mod mask;
pub(crate) mod policy;
mod run;
mod schema;
use schema::valid_name;

/// The one rule for a submitted secret value: non-empty, at most 16 KiB, no NUL.
fn valid_secret_value(value: &str) -> bool {
    !value.is_empty() && value.len() <= 16384 && !value.contains('\0')
}
pub(crate) use schema::{Field, FieldKind, Form};
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

#[derive(Default)]
struct Inner {
    values: BTreeMap<String, Zeroizing<String>>,
    templates: Vec<ApprovedTemplate>,
    pending: Option<Pending>,
}

#[derive(Default)]
pub(crate) struct SecretStore {
    inner: parking_lot::Mutex<Inner>,
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
            for (name, value) in std::mem::take(&mut submission.values) {
                inner.values.insert(name, Zeroizing::new(value));
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
        let inner = self.inner.lock();
        let needles: Vec<_> = inner
            .values
            .values()
            .flat_map(|v| mask::representations(v))
            .collect();
        mask::mask(text, &needles)
    }
    /// Mask registered values in full output without moving source-byte cursors.
    pub(crate) fn mask_preserving_offsets(&self, bytes: &[u8]) -> Zeroizing<Vec<u8>> {
        let inner = self.inner.lock();
        let needles: Vec<_> = inner
            .values
            .values()
            .flat_map(|v| mask::representations(v))
            .collect();
        mask::mask_preserving_offsets(bytes, &needles)
    }
    pub(crate) fn clear(&self) {
        let mut inner = self.inner.lock();
        inner.values.clear();
        inner.templates.clear();
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
    fn environment(&self, names: &[String]) -> Result<BTreeMap<String, Zeroizing<String>>, String> {
        let inner = self.inner.lock();
        names
            .iter()
            .map(|name| {
                inner
                    .values
                    .get(name)
                    .map(|value| (name.clone(), Zeroizing::new(value.to_string())))
                    .ok_or_else(|| "Requested secret is missing".into())
            })
            .collect()
    }
}

pub(crate) use forms::{form_http, submit_http};
pub(crate) use run::handle_secret;

pub(crate) fn tool_definition() -> serde_json::Value {
    serde_json::json!({
        "name": "secret",
        "description": "Request a private sensitive-field form, then run a user-approved argv with named values in the child environment only. Values are never returned. An approved command can exfiltrate. request opens a separate native form; HTTP/mobile entry uses its private one-time capability link. run uses user-approved command templates or asks for exact argv consent; shell/interpreter evaluation and environment dumpers are forbidden. Templates match exact argv, names and directory. Output masks exact values, JSON-escaped values and base64, hex and URL encodings, including line wraps and embedded Basic-auth payloads. Values are shared by name across callers of this backend process and survive MCP reconnects. remove zeroizes named values and returns removed, including already absent names; it does not report prior presence. Requests require a desktop host; the phone can submit the desktop-opened form.",
        "inputSchema": {"type": "object", "properties": {
            "action": {"type": "string", "enum": ["request", "run", "remove"]},
            "fields": {"type": "array", "items": {"type": "object", "properties": {
                "name": {"type": "string"}, "kind": {"type": "string", "enum": ["username", "password", "otp", "sso"]}, "display": {"type": "string", "description": "HTTP(S) SSO link to display without navigating"}
            }, "required": ["name", "kind"], "additionalProperties": false}},
            "reason": {"type": "string"}, "names": {"type": "array", "items": {"type": "string"}},
            "argv": {"type": "array", "items": {"type": "string"}}, "cwd": {"type": "string"}
        }, "required": ["action"], "additionalProperties": false}
    })
}
