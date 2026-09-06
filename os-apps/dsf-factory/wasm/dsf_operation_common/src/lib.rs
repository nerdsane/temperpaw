//! Typed boundaries shared by four single-concern operation integrations.
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use thiserror::Error;
mod providers;
pub mod types;
mod validation;
mod verification;
pub use types::*;

#[derive(Debug, Error)]
/// Typed refusal, unavailable evidence, or confirmed provider failure.
pub enum Error {
    #[error("invalid operation binding: {0}")]
    Binding(&'static str),
    #[error("missing field: {0}")]
    Field(String),
    #[error("provider transport unavailable")]
    Transport,
    #[error("HTTP {0} from {1}")]
    Http(u16, &'static str),
    #[error("invalid {0} response")]
    Response(&'static str),
    #[error("proof does not pass: {0}")]
    Proof(String),
    #[error("required Ask {0} is unresolved or declines the operation")]
    Blocked(String),
    #[error("provider operation has failed: {0}")]
    ProviderFailed(&'static str),
    #[error("verification pending: {0}")]
    Pending(&'static str),
}

/// One bounded HTTP request issued by an integration.
pub struct Request {
    pub method: &'static str,
    pub url: String,
    pub headers: Vec<(String, String)>,
    pub body: String,
}
/// HTTP status and body supplied by the Temper host.
pub struct Response {
    pub status: u16,
    pub body: String,
}
/// HTTP and named-secret capabilities supplied at the external boundary.
pub trait Host {
    /// Send one request.
    ///
    /// # Errors
    /// Returns transport failure when no trustworthy HTTP response is available.
    fn request(&mut self, request: &Request) -> Result<Response, Error>;
    /// Resolve a configured credential name.
    ///
    /// # Errors
    /// Returns a binding or availability error without exposing the credential.
    fn secret(&mut self, name: &str) -> Result<String, Error>;
}
/// A declared IOA callback and its typed JSON parameters.
pub struct Callback {
    pub action: &'static str,
    pub params: Value,
}
/// Invocation authority, time and host capabilities.
pub struct Runtime<'a, H> {
    pub host: &'a mut H,
    pub base: &'a str,
    pub tenant: &'a str,
    pub key: &'a str,
    pub now_ms: i64,
}

pub(crate) fn field<'a>(value: &'a Value, name: &str) -> Option<&'a Value> {
    let value = value.get("fields").unwrap_or(value);
    let pascal: String = name
        .split('_')
        .map(|s| {
            let mut c = s.chars();
            c.next().map_or(String::new(), |f| {
                f.to_uppercase().collect::<String>() + c.as_str()
            })
        })
        .collect();
    value.get(name).or_else(|| value.get(pascal))
}
pub(crate) fn required<'a>(value: &'a Value, name: &str) -> Result<&'a str, Error> {
    field(value, name)
        .and_then(Value::as_str)
        .filter(|s| !s.is_empty())
        .ok_or_else(|| Error::Field(name.into()))
}
pub(crate) fn decoded(value: &Value, name: &str) -> Result<Value, Error> {
    let value = field(value, name).ok_or_else(|| Error::Field(name.into()))?;
    if let Some(raw) = value.as_str() {
        serde_json::from_str(raw).map_err(|_| Error::Field(name.into()))
    } else {
        Ok(value.clone())
    }
}
pub(crate) fn identifier(raw: &str) -> Result<&str, Error> {
    if raw.is_empty()
        || raw.len() > 160
        || !raw
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b"_-.:".contains(&b))
    {
        Err(Error::Binding("invalid identifier"))
    } else {
        Ok(raw)
    }
}
pub(crate) fn full_sha(raw: &str) -> bool {
    [40, 64].contains(&raw.len())
        && raw
            .bytes()
            .all(|b| b.is_ascii_hexdigit() && !b.is_ascii_uppercase())
}
pub(crate) fn encoded(raw: &str) -> String {
    raw.bytes()
        .map(|b| {
            if b.is_ascii_alphanumeric() || b"-_.~".contains(&b) {
                char::from(b).to_string()
            } else {
                format!("%{b:02X}")
            }
        })
        .collect()
}
pub(crate) fn json_body(response: Response, source: &'static str) -> Result<Value, Error> {
    if !(200..300).contains(&response.status) {
        return Err(Error::Http(response.status, source));
    }
    if response.body.len() > 1_048_576 {
        return Err(Error::Response(source));
    }
    serde_json::from_str(&response.body).map_err(|_| Error::Response(source))
}

impl<H: Host> Runtime<'_, H> {
    pub(crate) fn read(&mut self, set: &str, id: &str, file: bool) -> Result<Response, Error> {
        identifier(id)?;
        if !self.base.starts_with("https://") && !self.base.starts_with("http://127.0.0.1:") {
            return Err(Error::Binding("invalid Temper base"));
        }
        self.host.request(&Request {
            method: "GET",
            url: format!(
                "{}/tdata/{set}('{id}'){}",
                self.base.trim_end_matches('/'),
                if file { "/$value" } else { "" }
            ),
            headers: vec![
                ("authorization".into(), format!("Bearer {}", self.key)),
                ("x-tenant-id".into(), self.tenant.into()),
            ],
            body: String::new(),
        })
    }
    pub(crate) fn row(&mut self, set: &str, id: &str) -> Result<Value, Error> {
        json_body(self.read(set, id, false)?, "Temper")
    }
    pub(crate) fn load(&mut self, op: &Operation) -> Result<Config, Error> {
        let raw = self.read("Files", &op.binding.config_ref, true)?;
        if raw.status != 200 || raw.body.len() > 32_768 {
            return Err(Error::Response("configuration File"));
        }
        let hash = format!("{:x}", Sha256::digest(raw.body.as_bytes()));
        if hash != op.binding.config_sha256 {
            return Err(Error::Binding("configuration File hash changed"));
        }
        let config: Config = serde_json::from_str(&raw.body)
            .map_err(|_| Error::Binding("invalid operation configuration"))?;
        if config.version != 1
            || config.resource_id != op.resource_id
            || config.effort_id != op.effort_id
            || config.operation_key != op.key
            || config.revision != op.revision
            || config.provider.kind() != op.kind
            || !full_sha(&op.revision)
        {
            return Err(Error::Binding(
                "configuration targets differ from operation",
            ));
        }
        if config.not_before_ms <= 0 || config.not_before_ms > self.now_ms + 5000 {
            return Err(Error::Binding("invalid operation time bound"));
        }
        Ok(config)
    }
    pub(crate) fn provider(
        &mut self,
        config: &Config,
        method: &'static str,
        url: String,
        body: Value,
    ) -> Result<Value, Error> {
        let secret = self.host.secret(config.provider.secret_name())?;
        if secret.is_empty() || secret.contains(['\r', '\n']) {
            return Err(Error::Binding("invalid provider credential"));
        }
        let request = Request {
            method,
            url,
            headers: vec![
                ("authorization".into(), format!("Bearer {secret}")),
                ("content-type".into(), "application/json".into()),
            ],
            body: if method == "GET" || method == "HEAD" {
                String::new()
            } else {
                body.to_string()
            },
        };
        let response = self.host.request(&request)?;
        let value = json_body(response, "provider")?;
        if value
            .get("errors")
            .and_then(Value::as_array)
            .is_some_and(|errors| !errors.is_empty())
        {
            return Err(Error::Response("GraphQL"));
        }
        Ok(value)
    }
}

/// Validate the linked authorization, exact target and existing proof.
///
/// # Errors
/// Returns a typed binding, evidence, transport, pending or provider failure.
pub fn validate(runtime: &mut Runtime<impl Host>, op: &Operation) -> Result<Callback, Error> {
    validation::validate(runtime, op)
}
/// Read correlation, then submit an authorized provider operation.
///
/// # Errors
/// Returns a typed binding, evidence, transport, pending or provider failure.
pub fn execute(runtime: &mut Runtime<impl Host>, op: &Operation) -> Result<Callback, Error> {
    providers::execute(runtime, op)
}
/// Read the provider receipt without resubmitting work.
///
/// # Errors
/// Returns a typed binding, evidence, transport, pending or provider failure.
pub fn observe(runtime: &mut Runtime<impl Host>, op: &Operation) -> Result<Callback, Error> {
    providers::observe(runtime, op)
}
/// Require provider completion, revision health, affected flow and telemetry.
///
/// # Errors
/// Returns a typed binding, evidence, transport, pending or provider failure.
pub fn verify(runtime: &mut Runtime<impl Host>, op: &Operation) -> Result<Callback, Error> {
    verification::verify(runtime, op)
}
#[cfg(target_arch = "wasm32")]
pub mod guest;
#[cfg(test)]
mod tests;
