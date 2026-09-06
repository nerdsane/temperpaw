//! Bounded provider reads. IOA owns observations, projection, retries and scheduling.
use chrono::{DateTime, SecondsFormat};
use serde::Deserialize;
use serde_json::{Value, json};

const MAX_BODY: usize = 1_048_576;
const MAX_CONFIG: usize = 16_384;

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Config {
    pub provider_id: String,
    pub secret_name: String,
    pub interval_seconds: u64,
    pub source: Source,
}

#[derive(Debug, Deserialize)]
#[serde(tag = "provider", rename_all = "snake_case", deny_unknown_fields)]
pub enum Source {
    Railway {
        environment_id: String,
    },
    Vercel {
        team_id: String,
        target: String,
    },
    Supabase {},
    CloudflareR2 {
        account_id: String,
    },
    Datadog {
        site: String,
        app_key_secret: String,
        query: String,
        window_seconds: u64,
        max_age_seconds: u64,
    },
    DsfOperations {
        service: String,
        environment: String,
        max_age_seconds: u64,
    },
    Github {
        owner: String,
        repository: String,
        git_ref: String,
    },
}

impl Source {
    fn name(&self) -> &'static str {
        match self {
            Self::Railway { .. } => "railway",
            Self::Vercel { .. } => "vercel",
            Self::Supabase {} => "supabase",
            Self::CloudflareR2 { .. } => "cloudflare_r2",
            Self::Datadog { .. } => "datadog",
            Self::Github { .. } => "github",
            Self::DsfOperations { .. } => "dsf_operations",
        }
    }
}

/// The only injected boundary: the host supplies HTTP and named secrets.
pub trait Host {
    fn request(&mut self, request: &Request) -> Result<Response, String>;
    fn secret(&mut self, name: &str) -> Result<String, String>;
}

#[derive(Debug)]
pub struct Request {
    pub method: &'static str,
    pub url: String,
    pub headers: Vec<(String, String)>,
    pub body: String,
}

pub struct Response {
    pub status: u16,
    pub body: String,
}

#[derive(Debug)]
pub struct Callback {
    pub action: &'static str,
    pub params: Value,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Coverage {
    Measured,
    Absent,
    Stale,
    Inaccessible,
}

struct Facts {
    coverage: Coverage,
    outcome: String,
    revision: String,
    facts: Value,
    sample_kind: &'static str,
}

fn identifier(raw: &str) -> Result<&str, String> {
    if raw.is_empty()
        || raw.len() > 160
        || !raw
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b"_-.:".contains(&b))
    {
        return Err("invalid pinned identifier".into());
    }
    Ok(raw)
}

fn encoded(raw: &str) -> String {
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

fn token(host: &mut impl Host, name: &str) -> Result<String, String> {
    identifier(name)?;
    let value = host
        .secret(name)
        .map_err(|_| "named provider credential unavailable".to_string())?;
    if value.is_empty() || value.contains(['\r', '\n']) {
        return Err("invalid provider credential".into());
    }
    Ok(value)
}

fn provider_request(config: &Config, now_ms: i64) -> Result<Request, String> {
    identifier(&config.provider_id)?;
    if !(60..=86_400).contains(&config.interval_seconds) {
        return Err("interval_seconds must be 60..86400".into());
    }
    let mut request = Request {
        method: "GET",
        url: String::new(),
        headers: vec![("accept".into(), "application/json".into())],
        body: String::new(),
    };
    request.url = match &config.source {
        Source::Railway { environment_id } => {
            identifier(environment_id)?;
            request.method = "POST";
            request.body = json!({"query":"query DsfCollect($serviceId: String!) { service(id: $serviceId) { id serviceInstances { edges { node { environmentId latestDeployment { id status createdAt meta } } } } } }", "variables":{"serviceId":config.provider_id}}).to_string();
            request
                .headers
                .push(("content-type".into(), "application/json".into()));
            "https://backboard.railway.com/graphql/v2".into()
        }
        Source::Vercel { team_id, target } => {
            identifier(team_id)?;
            if target != "production" {
                return Err("Vercel collector requires production target".into());
            }
            format!(
                "https://api.vercel.com/v9/projects/{}?teamId={}",
                encoded(&config.provider_id),
                encoded(team_id)
            )
        }
        Source::Supabase {} => "https://api.supabase.com/v1/projects".into(),
        Source::CloudflareR2 { account_id } => {
            identifier(account_id)?;
            format!(
                "https://api.cloudflare.com/client/v4/accounts/{}/r2/buckets/{}",
                encoded(account_id),
                encoded(&config.provider_id)
            )
        }
        Source::Datadog {
            site,
            query,
            window_seconds,
            max_age_seconds,
            ..
        } => datadog_url(site, query, window_seconds, max_age_seconds, now_ms)?,
        Source::DsfOperations {
            service,
            environment,
            max_age_seconds,
        } => {
            identifier(service)?;
            if service != &config.provider_id {
                return Err("DSF service selector differs from provider identity".into());
            }
            identifier(environment)?;
            if !(1..=3600).contains(max_age_seconds) {
                return Err("invalid snapshot maximum age".into());
            }
            "https://api.deep-sci-fi.world/api/operations/snapshot?participant_limit=200&job_limit=20".into()
        }
        Source::Github {
            owner,
            repository,
            git_ref,
        } => github_url(&mut request, owner, repository, git_ref)?,
    };
    Ok(request)
}

fn datadog_url(
    site: &str,
    query: &str,
    window_seconds: &u64,
    max_age_seconds: &u64,
    now_ms: i64,
) -> Result<String, String> {
    if ![
        "datadoghq.com",
        "us3.datadoghq.com",
        "us5.datadoghq.com",
        "datadoghq.eu",
        "ap1.datadoghq.com",
        "ap2.datadoghq.com",
        "uk1.datadoghq.com",
    ]
    .contains(&site)
    {
        return Err("Datadog site is not allowed".into());
    }
    if !(60..=3600).contains(window_seconds)
        || *max_age_seconds == 0
        || max_age_seconds > window_seconds
        || query.is_empty()
        || query.len() > 2048
        || query.chars().any(char::is_control)
    {
        return Err("invalid bounded Datadog query".into());
    }
    let end = now_ms / 1000;
    Ok(format!(
        "https://api.{site}/api/v1/query?from={}&to={end}&query={}",
        end - *window_seconds as i64,
        encoded(query)
    ))
}

fn github_url(
    request: &mut Request,
    owner: &str,
    repository: &str,
    git_ref: &str,
) -> Result<String, String> {
    identifier(owner)?;
    identifier(repository)?;
    if git_ref.is_empty() || git_ref.len() > 256 || git_ref.chars().any(char::is_control) {
        return Err("invalid Git ref".into());
    }
    request
        .headers
        .push(("user-agent".into(), "temper-dsf-factory".into()));
    request
        .headers
        .push(("X-GitHub-Api-Version".into(), "2022-11-28".into()));
    Ok(format!(
        "https://api.github.com/repos/{}/{}/commits/{}",
        encoded(owner),
        encoded(repository),
        encoded(git_ref)
    ))
}

fn authorize_request(
    request: &mut Request,
    config: &Config,
    host: &mut impl Host,
) -> Result<(), String> {
    if let Source::Datadog { app_key_secret, .. } = &config.source {
        request
            .headers
            .push(("DD-APPLICATION-KEY".into(), token(host, app_key_secret)?));
    }
    let secret = token(host, &config.secret_name)?;
    request
        .headers
        .push(if matches!(config.source, Source::Datadog { .. }) {
            ("DD-API-KEY".into(), secret)
        } else {
            ("authorization".into(), format!("Bearer {secret}"))
        });
    Ok(())
}

fn text_at<'a>(value: &'a Value, pointer: &str) -> Result<&'a str, String> {
    value
        .pointer(pointer)
        .and_then(Value::as_str)
        .filter(|s| !s.is_empty() && s.len() <= 4096)
        .ok_or_else(|| format!("provider response missing {pointer}"))
}

fn picked(value: &Value, keys: &[&str]) -> Value {
    let mut out = serde_json::Map::new();
    for key in keys {
        if let Some(item) = value
            .get(*key)
            .filter(|v| v.is_string() || v.is_number() || v.is_boolean() || v.is_null())
        {
            out.insert((*key).into(), item.clone());
        }
    }
    Value::Object(out)
}

fn facts(coverage: Coverage, outcome: &str, revision: &str, facts: Value) -> Facts {
    Facts {
        coverage,
        outcome: outcome.into(),
        revision: revision.into(),
        facts,
        sample_kind: "provider_snapshot",
    }
}

fn parse_source(config: &Config, response: &Value, now_ms: i64) -> Result<Facts, String> {
    match &config.source {
        Source::Railway { environment_id } => providers::railway(config, response, environment_id),
        Source::Vercel { .. } => providers::vercel(config, response),
        Source::Supabase {} => providers::supabase(config, response),
        Source::CloudflareR2 { .. } => providers::cloudflare_r2(config, response),
        Source::Github { .. } => providers::github(response),
        Source::Datadog {
            window_seconds,
            max_age_seconds,
            ..
        } => providers::datadog(response, now_ms, window_seconds, max_age_seconds),
        Source::DsfOperations {
            service,
            environment,
            max_age_seconds,
        } => snapshot::parse(response, service, environment, *max_age_seconds, now_ms),
    }
}

fn field<'a>(fields: &'a Value, name: &str) -> Option<&'a Value> {
    let pascal: String = name
        .split('_')
        .map(|s| {
            let mut chars = s.chars();
            chars.next().map_or(String::new(), |c| {
                c.to_uppercase().collect::<String>() + chars.as_str()
            })
        })
        .collect();
    fields.get(name).or_else(|| fields.get(pascal))
}

fn string_field<'a>(fields: &'a Value, name: &str) -> Result<&'a str, String> {
    field(fields, name)
        .and_then(Value::as_str)
        .filter(|s| !s.is_empty())
        .ok_or_else(|| format!("missing {name}"))
}

fn counter(fields: &Value, name: &str) -> Result<u64, String> {
    field(fields, name)
        .and_then(Value::as_u64)
        .ok_or_else(|| format!("missing integer {name}"))
}

fn timestamp(ms: i64) -> Result<String, String> {
    DateTime::from_timestamp_millis(ms)
        .map(|t| t.to_rfc3339_opts(SecondsFormat::Millis, true))
        .ok_or_else(|| "invalid host timestamp".into())
}

fn parse_json(response: Response, limit: usize) -> Result<Value, String> {
    if response.status != 200 {
        return Err(format!("Temper read returned HTTP {}", response.status));
    }
    if response.body.len() > limit {
        return Err("response size limit exceeded".into());
    }
    serde_json::from_str(&response.body).map_err(|_| "invalid JSON response".into())
}

/// Read two trusted Temper records and one fixed provider endpoint.
///
/// # Errors
/// Rejects malformed bindings or an unavailable Temper record. Provider access
/// errors produce an inaccessible observation callback instead.
pub fn collect(
    host: &mut impl Host,
    base: &str,
    api_key: &str,
    tenant: &str,
    sync_id: &str,
    fields: &Value,
    now_ms: i64,
) -> Result<Callback, String> {
    identifier(sync_id)?;
    if now_ms <= 0 {
        return Err("invalid host time".into());
    }
    let sequence = counter(fields, "sync_sequence")?;
    let (config, resource_sequence) = read_binding(host, base, api_key, tenant, fields)?;
    let mut request = provider_request(&config, now_ms)?;
    let observed = read_provider(host, &config, &mut request, now_ms);
    callback(
        &config,
        &request,
        now_ms,
        sync_id,
        sequence,
        resource_sequence,
        observed,
    )
}

fn read_binding(
    host: &mut impl Host,
    base: &str,
    api_key: &str,
    tenant: &str,
    fields: &Value,
) -> Result<(Config, u64), String> {
    // Base is trusted integration configuration, never supplied by the source File.
    if !(base.starts_with("https://")
        || base.starts_with("http://127.0.0.1:")
        || base.starts_with("http://localhost:"))
        || base.contains(['\r', '\n', '?', '#'])
    {
        return Err("invalid Temper API base".into());
    }
    let config_id = identifier(string_field(fields, "source_config_ref")?)?;
    let resource_id = identifier(string_field(fields, "resource_id")?)?;
    let headers = vec![
        ("authorization".into(), format!("Bearer {api_key}")),
        ("x-tenant-id".into(), tenant.into()),
        ("accept".into(), "application/json".into()),
    ];
    let read = |path: String| Request {
        method: "GET",
        url: format!("{}{path}", base.trim_end_matches('/')),
        headers: headers.clone(),
        body: String::new(),
    };
    let config_value = parse_json(
        host.request(&read(format!("/tdata/Files('{config_id}')/$value")))?,
        MAX_CONFIG,
    )?;
    let config: Config = serde_json::from_value(config_value)
        .map_err(|_| "invalid source configuration".to_string())?;
    let resource = parse_json(
        host.request(&read(format!("/tdata/DsfResources('{resource_id}')")))?,
        MAX_BODY,
    )?;
    let resource = resource.get("fields").unwrap_or(&resource);
    if string_field(resource, "provider")? != config.source.name()
        || string_field(resource, "provider_id")? != config.provider_id
        || string_field(fields, "source_kind")? != config.source.name()
    {
        return Err("source and resource binding mismatch".into());
    }
    Ok((config, counter(resource, "observed_sequence")?))
}

fn read_provider(
    host: &mut impl Host,
    config: &Config,
    request: &mut Request,
    now_ms: i64,
) -> Facts {
    let credentials_available = authorize_request(request, config, host).is_ok();
    let response = if credentials_available {
        host.request(request)
    } else {
        Err("credential_unavailable".into())
    };
    match response {
        Err(_) if !credentials_available => facts(
            Coverage::Inaccessible,
            "credential_unavailable",
            "",
            json!({"credential_available":false,"request_attempted":false}),
        ),
        Err(_) => facts(
            Coverage::Inaccessible,
            "transport_error",
            "",
            json!({"transport":"failed"}),
        ),
        Ok(r) if r.status != 200 => facts(
            Coverage::Inaccessible,
            "provider_http_error",
            "",
            json!({"http_status":r.status}),
        ),
        Ok(r) if r.body.len() > MAX_BODY => facts(
            Coverage::Inaccessible,
            "response_limit_exceeded",
            "",
            json!({"maximum_bytes":MAX_BODY}),
        ),
        Ok(r) => match serde_json::from_str::<Value>(&r.body) {
            Ok(body) => parse_source(config, &body, now_ms).unwrap_or_else(|reason| {
                facts(
                    Coverage::Inaccessible,
                    "provider_shape_error",
                    "",
                    json!({"http_status":200,"parsed":false,"reason":reason}),
                )
            }),
            Err(_) => facts(
                Coverage::Inaccessible,
                "invalid_json",
                "",
                json!({"http_status":200,"parsed":false}),
            ),
        },
    }
}

fn callback(
    config: &Config,
    request: &Request,
    now_ms: i64,
    sync_id: &str,
    sequence: u64,
    resource_sequence: u64,
    observed: Facts,
) -> Result<Callback, String> {
    let now = timestamp(now_ms)?;
    let start_ms = match config.source {
        Source::Datadog { window_seconds, .. } => now_ms - window_seconds as i64 * 1000,
        _ => now_ms,
    };
    let observation_id = format!("{sync_id}-{sequence}");
    let query = if request.body.is_empty() {
        request.url.clone()
    } else {
        request.body.clone()
    };
    let mut params = json!({
        "expected_sequence":sequence,"observation_id":observation_id,"source_event_id":observation_id,
        "query":query,"window_start":timestamp(start_ms)?,"window_end":now,"sample_kind":observed.sample_kind,
        "outcome":observed.outcome,"summary":observed.facts.to_string(),"evidence_ref":request.url,
        "observed_at_ms":now_ms,"expected_resource_sequence":resource_sequence,
    });
    let action = match observed.coverage {
        Coverage::Inaccessible => {
            params["error_message"] = json!(observed.outcome);
            "CollectionInaccessible"
        }
        coverage => {
            params["source_cursor"] = json!(observation_id);
            params["last_success_at"] = json!(now);
            params["next_due_at"] =
                json!(timestamp(now_ms + config.interval_seconds as i64 * 1000)?);
            match coverage {
                Coverage::Measured => {
                    params["observed_configuration"] = json!(observed.facts.to_string());
                    params["observed_revision"] = json!(observed.revision);
                    "CollectionSucceeded"
                }
                Coverage::Absent => "CollectionAbsent",
                Coverage::Stale => "CollectionStale",
                Coverage::Inaccessible => unreachable!(),
            }
        }
    };
    Ok(Callback { action, params })
}

#[cfg(target_arch = "wasm32")]
mod guest {
    use super::*;
    use temper_wasm_sdk::{Context, set_error_result, set_success_result};

    struct Guest(Context);
    impl Host for Guest {
        fn request(&mut self, request: &Request) -> Result<Response, String> {
            self.0
                .http_call(
                    request.method,
                    &request.url,
                    &request.headers,
                    &request.body,
                )
                .map(|r| Response {
                    status: r.status,
                    body: r.body,
                })
        }
        fn secret(&mut self, name: &str) -> Result<String, String> {
            self.0.get_secret(name)
        }
    }

    #[unsafe(no_mangle)]
    pub extern "C" fn run(_ctx_ptr: i32, _ctx_len: i32) -> i32 {
        let result = (|| {
            let ctx = Context::from_host()?;
            let base = ctx
                .config
                .get("temper_api_url")
                .cloned()
                .ok_or("missing temper_api_url")?;
            let api_key = ctx
                .config
                .get("temper_api_key")
                .cloned()
                .ok_or("missing temper_api_key")?;
            let fields = ctx
                .entity_state
                .get("fields")
                .cloned()
                .ok_or("missing entity fields")?;
            let tenant = ctx.tenant.clone();
            let id = ctx.entity_id.clone();
            collect(
                &mut Guest(ctx),
                &base,
                &api_key,
                &tenant,
                &id,
                &fields,
                Context::get_time_millis(),
            )
        })();
        match result {
            Ok(callback) => set_success_result(callback.action, &callback.params),
            Err(error) => set_error_result(&error),
        }
        0
    }
}

mod providers;
mod snapshot;

#[cfg(test)]
mod tests;
