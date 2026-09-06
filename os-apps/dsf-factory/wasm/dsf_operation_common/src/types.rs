use crate::{Error, field, required};
use serde::Deserialize;
use serde_json::Value;

#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
/// Provider operations supported by this application.
pub enum Kind {
    RailwayDeploy,
    VercelDeploy,
    MediaRepair,
}

#[derive(Clone, Debug)]
/// Immutable operation fields read from the invoking entity.
pub struct Operation {
    pub key: String,
    pub resource_id: String,
    pub effort_id: String,
    pub kind: Kind,
    pub revision: String,
    pub proof_id: String,
    pub binding: Binding,
    pub execution_id: Option<String>,
    pub execution_attempts: u64,
}
#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
/// A configuration File and the digest of its exact bytes.
pub struct Binding {
    pub config_ref: String,
    pub config_sha256: String,
}

impl Operation {
    /// Parse the invoking entity and bind its operation key to its canonical ID.
    ///
    /// # Errors
    /// Rejects missing fields, changed identity, unsupported kinds and malformed configuration bindings.
    pub fn parse(id: &str, value: &Value) -> Result<Self, Error> {
        let key = required(value, "operation_key")?.to_owned();
        if key != id {
            return Err(Error::Binding("operation key differs from canonical ID"));
        }
        Ok(Self {
            key,
            resource_id: required(value, "resource_id")?.into(),
            effort_id: required(value, "effort_id")?.into(),
            kind: serde_json::from_value(Value::String(required(value, "operation_kind")?.into()))
                .map_err(|_| Error::Binding("unsupported operation kind"))?,
            revision: required(value, "intended_revision")?.into(),
            proof_id: required(value, "proof_ref")?.into(),
            binding: serde_json::from_str(required(value, "intended_configuration")?)
                .map_err(|_| Error::Binding("invalid hashed configuration binding"))?,
            execution_id: field(value, "provider_execution_id")
                .and_then(Value::as_str)
                .filter(|s| !s.is_empty())
                .map(str::to_owned),
            execution_attempts: field(value, "execution_attempts")
                .and_then(Value::as_u64)
                .unwrap_or(0),
        })
    }
}
#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
/// Versioned provider targets, existing authority and verification expectations.
pub struct Config {
    pub version: u64,
    pub resource_id: String,
    pub effort_id: String,
    pub operation_key: String,
    pub revision: String,
    #[serde(default)]
    pub required_ask_ids: Vec<String>,
    pub max_cost_cents: u64,
    pub not_before_ms: i64,
    pub provider: Provider,
    pub flow: Flow,
    pub datadog: Datadog,
}
#[derive(Clone, Debug, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
/// Provider-specific identifiers and named credentials.
pub enum Provider {
    Railway {
        project_id: String,
        service_id: String,
        environment_id: String,
        secret_name: String,
        baseline_deployment_id: String,
    },
    Vercel {
        project_id: String,
        team_id: String,
        project_name: String,
        git_repository_id: u64,
        secret_name: String,
    },
    Media {
        secret_name: String,
        generations: Vec<Generation>,
    },
}
impl Provider {
    /// Return the operation supported by this provider.
    pub fn kind(&self) -> Kind {
        match self {
            Self::Railway { .. } => Kind::RailwayDeploy,
            Self::Vercel { .. } => Kind::VercelDeploy,
            Self::Media { .. } => Kind::MediaRepair,
        }
    }
    /// Return the named credential required by this provider.
    pub fn secret_name(&self) -> &str {
        match self {
            Self::Railway { secret_name, .. }
            | Self::Vercel { secret_name, .. }
            | Self::Media { secret_name, .. } => secret_name,
        }
    }
    /// Return the provider label stored on the resource.
    pub fn resource_provider(&self) -> &str {
        match self {
            Self::Railway { .. } => "railway",
            Self::Vercel { .. } => "vercel",
            Self::Media { .. } => "dsf_operations",
        }
    }
    /// Return the provider identity that must match the resource.
    pub fn provider_id(&self) -> &str {
        match self {
            Self::Railway { service_id, .. } => service_id,
            Self::Vercel { project_id, .. } => project_id,
            Self::Media { .. } => "deep-sci-fi-backend",
        }
    }
}
#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
/// One explicitly selected media job and its cost ceiling.
pub struct Generation {
    pub id: String,
    pub target_type: String,
    pub target_id: String,
    pub media_type: String,
    pub max_cost_cents: u64,
}
#[derive(Clone, Debug, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
/// A bounded product probe chosen for the affected behavior.
pub enum Flow {
    Story {
        story_id: String,
        world_id: String,
    },
    OperationalSnapshot {
        schema_version: String,
        secret_name: String,
    },
    Media {},
}
#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
/// Telemetry identity and named Datadog credentials.
pub struct Datadog {
    pub site: String,
    pub service: String,
    pub environment: String,
    pub api_key_secret: String,
    pub app_key_secret: String,
}
