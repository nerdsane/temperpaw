use crate::providers::{DSF, check_generation};
use crate::*;

pub(super) fn verify(runtime: &mut Runtime<impl Host>, op: &Operation) -> Result<Callback, Error> {
    let config = runtime.load(op)?;
    let resource = runtime.row("DsfResources", &op.resource_id)?;
    if required(&resource, "active_operation_id")? != op.key {
        return Err(Error::Binding("resource no longer owns this operation"));
    }
    let receipt = providers::find(runtime, op, &config)?
        .ok_or(Error::Pending("provider execution not yet visible"))?;
    if receipt.status == "partial" {
        let Provider::Media {
            generations,
            secret_name,
        } = &config.provider
        else {
            return Err(Error::Binding("partial result from non-media provider"));
        };
        let claimed: Vec<_> = generations
            .iter()
            .filter(|g| receipt.claimed_generation_ids.contains(&g.id))
            .collect();
        settled_media(runtime, op, &claimed, secret_name)?;
        return Err(Error::ProviderFailed(
            "provider could not claim every selected media job",
        ));
    }
    if ["FAILED", "CRASHED", "REMOVED", "ERROR", "CANCELED"].contains(&receipt.status.as_str()) {
        return Err(Error::ProviderFailed("provider reports terminal failure"));
    }
    if !["SUCCESS", "READY", "claimed"].contains(&receipt.status.as_str()) {
        return Err(Error::Pending("provider is still deploying"));
    }
    if receipt.revision != op.revision || op.execution_id.as_ref() != Some(&receipt.id) {
        return Err(Error::Binding("provider execution identity changed"));
    }
    health(runtime, op)?;
    let flow_url = flow(runtime, op, &config)?;
    let telemetry = datadog(runtime, op, &config)?;
    Ok(Callback {
        action: "VerificationSucceeded",
        params: json!({"operation_key":op.key,"verified_resource_id":op.resource_id,"verified_revision":op.revision,"provider_evidence_ref":receipt.url,"flow_evidence_ref":flow_url,"telemetry_evidence_ref":telemetry}),
    })
}

fn probe(
    runtime: &mut Runtime<impl Host>,
    op: &Operation,
    url: &str,
    secret: Option<&str>,
) -> Result<Value, Error> {
    let mut headers = vec![
        ("x-request-id".into(), op.key.clone()),
        ("accept".into(), "application/json".into()),
        ("cache-control".into(), "no-cache".into()),
    ];
    if let Some(name) = secret {
        let credential = runtime.host.secret(name)?;
        headers.push(("authorization".into(), format!("Bearer {credential}")));
    }
    json_body(
        runtime.host.request(&Request {
            method: "GET",
            url: url.into(),
            headers,
            body: String::new(),
        })?,
        "DSF probe",
    )
}

fn health(runtime: &mut Runtime<impl Host>, op: &Operation) -> Result<(), Error> {
    let origin = if op.kind == Kind::VercelDeploy {
        "https://deep-sci-fi.world"
    } else {
        DSF
    };
    let value = probe(runtime, op, &format!("{origin}/api/health"), None)?;
    if value.get("status").and_then(Value::as_str) != Some("healthy")
        || value.get("git_sha").and_then(Value::as_str) != Some(&op.revision)
    {
        return Err(Error::Pending(
            "health body is not healthy at the exact revision",
        ));
    }
    Ok(())
}

fn flow(
    runtime: &mut Runtime<impl Host>,
    op: &Operation,
    config: &Config,
) -> Result<String, Error> {
    match &config.flow {
        Flow::Story { story_id, world_id } => {
            identifier(story_id)?;
            identifier(world_id)?;
            let url = format!("{DSF}/api/stories/{story_id}");
            let value = probe(runtime, op, &url, None)?;
            let story = value.get("story").ok_or(Error::Response("story read"))?;
            if required(story, "id")? != story_id
                || required(story, "world_id")? != world_id
                || story
                    .get("content")
                    .and_then(Value::as_str)
                    .is_none_or(|s| s.is_empty())
                || !["published", "acclaimed"].contains(&required(story, "status")?)
            {
                return Err(Error::Pending(
                    "configured story flow does not match expected published resource",
                ));
            }
            Ok(url)
        }
        Flow::OperationalSnapshot {
            schema_version,
            secret_name,
        } => {
            let url = format!("{DSF}/api/operations/snapshot?participant_limit=1&job_limit=1");
            let value = probe(runtime, op, &url, Some(secret_name))?;
            if value.get("snapshot_version") != Some(&json!(1))
                || value.get("revision").and_then(Value::as_str) != Some(&op.revision)
                || value.pointer("/schema/is_current") != Some(&json!(true))
                || value
                    .pointer("/schema/current_version")
                    .and_then(Value::as_str)
                    != Some(schema_version)
            {
                return Err(Error::Pending("operational schema probe has not passed"));
            }
            Ok(url)
        }
        Flow::Media {} => verify_media(runtime, op, config),
    }
}

fn verify_media(
    runtime: &mut Runtime<impl Host>,
    op: &Operation,
    config: &Config,
) -> Result<String, Error> {
    let Provider::Media {
        generations,
        secret_name,
    } = &config.provider
    else {
        return Err(Error::Binding("media probe without selected jobs"));
    };
    let selected: Vec<_> = generations.iter().collect();
    let states = settled_media(runtime, op, &selected, secret_name)?;
    if states
        .iter()
        .any(|value| value.get("status").and_then(Value::as_str) == Some("failed"))
    {
        return Err(Error::ProviderFailed("selected media generation failed"));
    }
    for (generation, value) in generations.iter().zip(states) {
        let url = required(&value, "media_url")?;
        validate_artifact_url(url)?;
        let response = runtime.host.request(&Request {
            method: "HEAD",
            url: url.into(),
            headers: vec![],
            body: String::new(),
        })?;
        if response.status != 200 {
            return Err(Error::Pending("completed media artifact is not accessible"));
        }
        let cost = value
            .get("cost_usd")
            .and_then(Value::as_f64)
            .ok_or(Error::Pending("completed media cost is not recorded"))?;
        if !cost.is_finite()
            || cost < 0.0
            || cost * 100.0 > generation.max_cost_cents as f64 + 0.000001
        {
            return Err(Error::ProviderFailed(
                "completed media exceeded the selected cost ceiling",
            ));
        }
    }
    Ok(format!("{DSF}/api/media/recovery-operations/{}", op.key))
}

fn settled_media(
    runtime: &mut Runtime<impl Host>,
    op: &Operation,
    generations: &[&Generation],
    secret_name: &str,
) -> Result<Vec<Value>, Error> {
    let mut states = Vec::with_capacity(generations.len());
    for generation in generations {
        let value = probe(
            runtime,
            op,
            &format!("{DSF}/api/media/{}/status", encoded(&generation.id)),
            Some(secret_name),
        )?;
        check_generation(generation, &value)?;
        if value.get("attempt_id").and_then(Value::as_str) != Some(&op.key) {
            return Err(Error::Pending(
                "selected media job belongs to a different attempt",
            ));
        }
        if !["completed", "failed"].contains(&required(&value, "status")?) {
            return Err(Error::Pending("selected media work is still active"));
        }
        states.push(value);
    }
    Ok(states)
}

fn validate_artifact_url(url: &str) -> Result<(), Error> {
    let authority = url
        .strip_prefix("https://")
        .and_then(|s| s.split('/').next())
        .ok_or(Error::Binding("media artifact is not HTTPS"))?;
    if authority.contains(['@', ':', '?', '#'])
        || !(authority == "media.deep-sci-fi.world" || authority.ends_with(".r2.dev"))
    {
        return Err(Error::Binding("media artifact host is not permitted"));
    }
    Ok(())
}

fn datadog(
    runtime: &mut Runtime<impl Host>,
    op: &Operation,
    config: &Config,
) -> Result<String, Error> {
    let dd = &config.datadog;
    if ![
        "datadoghq.com",
        "us3.datadoghq.com",
        "us5.datadoghq.com",
        "datadoghq.eu",
        "ap1.datadoghq.com",
        "ap2.datadoghq.com",
        "uk1.datadoghq.com",
    ]
    .contains(&dd.site.as_str())
    {
        return Err(Error::Binding("Datadog site is not permitted"));
    }
    identifier(&dd.service)?;
    identifier(&dd.environment)?;
    let query = format!(
        "service:{} env:{} @git.commit.sha:{} @dsf.request_id:{} -status:error",
        dd.service, dd.environment, op.revision, op.key
    );
    let url = format!("https://api.{}/api/v2/spans/events/search", dd.site);
    let body = json!({"data":{"type":"search_request","attributes":{"filter":{"from":config.not_before_ms.max(runtime.now_ms-1_800_000).to_string(),"to":runtime.now_ms.to_string(),"query":query},"page":{"limit":20},"sort":"-timestamp"}}});
    let api_key = runtime.host.secret(&dd.api_key_secret)?;
    let app_key = runtime.host.secret(&dd.app_key_secret)?;
    let response = runtime.host.request(&Request {
        method: "POST",
        url,
        headers: vec![
            ("DD-API-KEY".into(), api_key),
            ("DD-APPLICATION-KEY".into(), app_key),
            ("content-type".into(), "application/json".into()),
        ],
        body: body.to_string(),
    })?;
    let value = json_body(response, "Datadog")?;
    let spans = value
        .get("data")
        .and_then(Value::as_array)
        .ok_or(Error::Response("Datadog spans"))?;
    let found = spans
        .iter()
        .take(20)
        .find(|span| matching_span(span, op, dd));
    let span = found.ok_or(Error::Pending(
        "no Datadog span proves the exact revision and probe request",
    ))?;
    let attrs = span
        .get("attributes")
        .ok_or(Error::Response("Datadog span attributes"))?;
    let trace_id = required(attrs, "trace_id")?;
    Ok(format!(
        "https://app.{}/apm/trace/{}",
        dd.site,
        encoded(trace_id)
    ))
}

pub(super) fn matching_span(span: &Value, op: &Operation, dd: &Datadog) -> bool {
    let Some(attrs) = span.get("attributes") else {
        return false;
    };
    if attrs.get("service").and_then(Value::as_str) != Some(&dd.service)
        || attrs.get("env").and_then(Value::as_str) != Some(&dd.environment)
        || attrs.get("status").and_then(Value::as_str) == Some("error")
    {
        return false;
    }
    attrs
        .pointer("/custom/git/commit/sha")
        .and_then(Value::as_str)
        == Some(op.revision.as_str())
        && attrs
            .pointer("/custom/dsf/request_id")
            .and_then(Value::as_str)
            == Some(op.key.as_str())
}
