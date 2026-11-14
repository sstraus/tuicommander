//! Typed HTTP configuration and pure request/response projection.
#![cfg_attr(
    not(test),
    expect(dead_code, reason = "009 HTTP awaits host composition in 008")
)]

use super::catalog::{ActivityDescriptor, ObjectSchema, ValueType};
use super::connector::{BoundedValue, ConnectorContract, safe_header};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::BTreeMap;

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct HttpOperation {
    pub method: String,
    pub endpoint: String,
    pub path: Vec<BoundedValue>,
    pub query: BTreeMap<String, BoundedValue>,
    pub headers: BTreeMap<String, BoundedValue>,
    pub body: Option<BoundedValue>,
    /// Output name to JSON pointer; schema is the ordinary descriptor output.
    pub response: BTreeMap<String, String>,
    /// Cumulative response bytes across every page, not a per-page allowance.
    pub max_response_bytes: usize,
}

#[derive(Clone, Debug)]
pub(crate) struct ResolvedHttp {
    pub url: url::Url,
    pub headers: BTreeMap<String, String>,
    pub body: Option<String>,
}

impl HttpOperation {
    pub(crate) fn parse(config: &BTreeMap<String, Value>) -> Result<Self, String> {
        if config
            .keys()
            .any(|key| key != "request" && key != "connector")
        {
            return Err(
                "HTTP configuration requires request and installed connector metadata".into(),
            );
        }
        serde_json::from_value(config.get("request").ok_or("HTTP request missing")?.clone())
            .map_err(|e| format!("invalid typed HTTP request: {e}"))
    }

    pub(crate) fn validate(
        &self,
        base: &str,
        descriptor: &ActivityDescriptor,
        contract: &ConnectorContract,
    ) -> Result<(), String> {
        contract.validate()?;
        self.endpoint_under(base)?;
        if self.max_response_bytes == 0
            || self.max_response_bytes > 10 * 1024 * 1024
            || self.path.len() > 32
            || self.query.len() > 64
            || self.headers.len() > 32
            || self.endpoint.len() > 8192
            || self.response.len() > 256
            || self
                .query
                .keys()
                .any(|name| name.is_empty() || name.len() > 128)
        {
            return Err("HTTP request exceeds supported bounds".into());
        }
        let mut headers = std::collections::BTreeSet::new();
        for name in self.headers.keys() {
            safe_header(name)?;
            if !headers.insert(name.to_ascii_lowercase())
                || contract
                    .callback
                    .as_ref()
                    .is_some_and(|c| c.header.eq_ignore_ascii_case(name))
            {
                return Err("duplicate or reserved callback header".into());
            }
        }
        if let Some(p) = &contract.pagination
            && (self.query.contains_key(&p.query_parameter)
                || self.response.contains_key(&p.output)
                || descriptor
                    .outputs
                    .fields
                    .get(&p.output)
                    .map(|f| &f.value_type)
                    != Some(&ValueType::Array))
        {
            return Err("pagination query/output conflicts with request mapping".into());
        }
        for value in self
            .path
            .iter()
            .chain(self.query.values())
            .chain(self.headers.values())
            .chain(self.body.iter())
        {
            value.validate()?;
            if let BoundedValue::Input { field, .. } = value
                && !descriptor.inputs.fields.contains_key(field)
            {
                return Err("HTTP dynamic field is not a declared input".into());
            }
        }
        for (name, pointer) in &self.response {
            if !descriptor.outputs.fields.contains_key(name)
                || (!pointer.is_empty() && !pointer.starts_with('/'))
                || pointer.len() > 1024
            {
                return Err("invalid typed response mapping".into());
            }
        }
        for (name, field) in &descriptor.outputs.fields {
            if field.required
                && field.default.is_none()
                && !self.response.contains_key(name)
                && contract
                    .pagination
                    .as_ref()
                    .is_none_or(|p| &p.output != name)
            {
                return Err("required HTTP output has no mapping".into());
            }
        }
        // Check every known literal without requiring unresolved dynamic inputs.
        for value in self
            .path
            .iter()
            .chain(self.query.values())
            .chain(self.headers.values())
        {
            if let BoundedValue::Literal { value } = value {
                scalar(value)?;
            }
        }
        for value in &self.path {
            if let BoundedValue::Literal { value } = value {
                path_segment(value)?;
            }
        }
        for value in self.headers.values() {
            if let BoundedValue::Literal { value } = value {
                header_value(value)?;
            }
        }
        Ok(())
    }

    fn endpoint_under(&self, base: &str) -> Result<url::Url, String> {
        let base = url::Url::parse(base).map_err(|_| "invalid connector base URL")?;
        let endpoint = url::Url::parse(&self.endpoint).map_err(|_| "invalid HTTP endpoint")?;
        if !matches!(base.scheme(), "http" | "https")
            || !base.path().ends_with('/')
            || base.query().is_some()
            || base.fragment().is_some()
            || !base.username().is_empty()
            || base.password().is_some()
            || endpoint.origin() != base.origin()
            || !endpoint.path().starts_with(base.path())
            || !endpoint.username().is_empty()
            || endpoint.password().is_some()
            || endpoint.query().is_some()
            || endpoint.fragment().is_some()
        {
            return Err("HTTP endpoint is outside installed credential destination".into());
        }
        Ok(endpoint)
    }

    pub(crate) fn resolve(
        &self,
        base: &str,
        inputs: &BTreeMap<String, Value>,
    ) -> Result<ResolvedHttp, String> {
        let mut url = self.endpoint_under(base)?;
        if !self.path.is_empty() {
            let mut segments = url
                .path_segments_mut()
                .map_err(|_| "HTTP URL has no path")?;
            segments.pop_if_empty();
            for value in &self.path {
                segments.push(&path_segment(&value.resolve(inputs)?)?);
            }
        }
        for (name, value) in &self.query {
            url.query_pairs_mut()
                .append_pair(name, &scalar(&value.resolve(inputs)?)?);
        }
        let headers = self
            .headers
            .iter()
            .map(|(name, value)| {
                safe_header(name)?;
                Ok((name.clone(), header_value(&value.resolve(inputs)?)?))
            })
            .collect::<Result<_, String>>()?;
        let body = self
            .body
            .as_ref()
            .map(|value| serde_json::to_string(&value.resolve(inputs)?).map_err(|e| e.to_string()))
            .transpose()?;
        Ok(ResolvedHttp { url, headers, body })
    }

    pub(crate) fn outputs(
        &self,
        response: &Value,
        schema: &ObjectSchema,
        collection: Option<(&str, Vec<Value>)>,
    ) -> Result<BTreeMap<String, Value>, String> {
        let mut values = BTreeMap::new();
        for (name, pointer) in &self.response {
            if let Some(value) = response.pointer(pointer) {
                values.insert(name.clone(), value.clone());
            }
        }
        if let Some((name, items)) = collection {
            values.insert(name.into(), Value::Array(items));
        }
        schema.resolve(&values, "HTTP response")
    }
}

fn scalar(value: &Value) -> Result<String, String> {
    match value {
        Value::String(value) => Ok(value.clone()),
        Value::Number(_) | Value::Bool(_) => Ok(value.to_string()),
        _ => Err("HTTP field requires a scalar value".into()),
    }
}

fn path_segment(value: &Value) -> Result<String, String> {
    let value = scalar(value)?;
    if value.is_empty() || matches!(value.as_str(), "." | "..") {
        return Err("HTTP path segment is empty or traverses its boundary".into());
    }
    Ok(value)
}

fn header_value(value: &Value) -> Result<String, String> {
    let value = scalar(value)?;
    reqwest::header::HeaderValue::from_str(&value).map_err(|_| "invalid safe header value")?;
    Ok(value)
}
