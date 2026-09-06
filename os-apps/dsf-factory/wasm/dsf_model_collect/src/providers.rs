//! Provider response parsing retains only operational evidence.
use super::{Config, Coverage, Facts, facts, picked, text_at};
use serde_json::{Value, json};

pub(super) fn railway(
    config: &Config,
    response: &Value,
    environment_id: &str,
) -> Result<Facts, String> {
    if response
        .get("errors")
        .and_then(Value::as_array)
        .is_some_and(|v| !v.is_empty())
    {
        return Err("Railway GraphQL query failed".into());
    }
    if text_at(response, "/data/service/id")? != config.provider_id {
        return Err("Railway service identity mismatch".into());
    }
    let edges = response
        .pointer("/data/service/serviceInstances/edges")
        .and_then(Value::as_array)
        .ok_or("Railway instances missing")?;
    let instance = edges
        .iter()
        .filter_map(|e| e.get("node"))
        .find(|n| n.get("environmentId").and_then(Value::as_str) == Some(environment_id));
    let Some(deployment) = instance
        .and_then(|v| v.get("latestDeployment"))
        .filter(|v| !v.is_null())
    else {
        return Ok(facts(
            Coverage::Absent,
            "no_deployment",
            "",
            json!({"service_id":config.provider_id,"environment_id":environment_id,"latest_deployment":null}),
        ));
    };
    let status = text_at(deployment, "/status")?;
    let id = text_at(deployment, "/id")?;
    // Railway's deployment metadata is the deployment provider's commit record.
    let revision = deployment
        .pointer("/meta/commitHash")
        .and_then(Value::as_str)
        .unwrap_or("");
    Ok(facts(
        Coverage::Measured,
        status,
        revision,
        json!({"service_id":config.provider_id,"environment_id":environment_id,"deployment_id":id,"status":status,"created_at":deployment.get("createdAt"),"commit_hash":revision}),
    ))
}

pub(super) fn vercel(config: &Config, response: &Value) -> Result<Facts, String> {
    if text_at(response, "/id")? != config.provider_id {
        return Err("Vercel project identity mismatch".into());
    }
    let Some(deployment) = response
        .pointer("/targets/production")
        .filter(|v| !v.is_null())
    else {
        return Ok(facts(
            Coverage::Absent,
            "no_production_target",
            "",
            json!({"project_id":config.provider_id,"production_target":null}),
        ));
    };
    let status = text_at(deployment, "/readyState")?;
    let id = text_at(deployment, "/id")?;
    let revision = deployment
        .pointer("/meta/githubCommitSha")
        .and_then(Value::as_str)
        .or_else(|| deployment.pointer("/gitSource/sha").and_then(Value::as_str))
        .unwrap_or("");
    Ok(facts(
        Coverage::Measured,
        status,
        revision,
        json!({"project_id":config.provider_id,"deployment_id":id,"ready_state":status,"commit_sha":revision,"url":deployment.get("url")}),
    ))
}

pub(super) fn supabase(config: &Config, response: &Value) -> Result<Facts, String> {
    let projects = response.as_array().ok_or("Supabase project list missing")?;
    let project = projects.iter().find(|p| {
        p.get("ref").or_else(|| p.get("id")).and_then(Value::as_str)
            == Some(config.provider_id.as_str())
    });
    let Some(project) = project else {
        return Ok(facts(
            Coverage::Absent,
            "project_not_visible",
            "",
            json!({"project_ref":config.provider_id,"visible":false}),
        ));
    };
    let status = text_at(project, "/status")?;
    Ok(facts(
        Coverage::Measured,
        status,
        "",
        picked(project, &["id", "ref", "region", "status", "created_at"]),
    ))
}

pub(super) fn cloudflare_r2(config: &Config, response: &Value) -> Result<Facts, String> {
    if response.get("success") != Some(&Value::Bool(true)) {
        return Err("Cloudflare bucket read failed".into());
    }
    let bucket = response.get("result").ok_or("Cloudflare bucket missing")?;
    if text_at(bucket, "/name")? != config.provider_id {
        return Err("Cloudflare bucket identity mismatch".into());
    }
    Ok(facts(
        Coverage::Measured,
        "exists",
        "",
        picked(
            bucket,
            &[
                "name",
                "creation_date",
                "jurisdiction",
                "location",
                "storage_class",
            ],
        ),
    ))
}

pub(super) fn github(response: &Value) -> Result<Facts, String> {
    let sha = text_at(response, "/sha")?;
    Ok(facts(
        Coverage::Measured,
        "commit_resolved",
        sha,
        json!({"sha":sha,"commit_date":response.pointer("/commit/committer/date"),"tree_sha":response.pointer("/commit/tree/sha")}),
    ))
}

pub(super) fn datadog(
    response: &Value,
    now_ms: i64,
    window_seconds: &u64,
    max_age_seconds: &u64,
) -> Result<Facts, String> {
    if response.get("status").and_then(Value::as_str) != Some("ok") {
        return Err("Datadog metric query failed".into());
    }
    let series = response
        .get("series")
        .and_then(Value::as_array)
        .ok_or("Datadog series missing")?;
    if series.len() > 100 {
        return Err("Datadog series limit exceeded".into());
    }
    let mut latest = None;
    let mut count = 0usize;
    let mut samples = Vec::new();
    for series in series {
        let points = series
            .get("pointlist")
            .and_then(Value::as_array)
            .ok_or("Datadog pointlist missing")?;
        if points.len() > 3600 {
            return Err("Datadog point limit exceeded".into());
        }
        let mut series_latest: Option<(i64, f64)> = None;
        let mut series_count = 0;
        for point in points {
            let point = point
                .as_array()
                .filter(|p| p.len() == 2)
                .ok_or("invalid Datadog point")?;
            let at = point[0]
                .as_f64()
                .filter(|n| n.is_finite())
                .ok_or("invalid Datadog point timestamp")? as i64;
            if at < now_ms - *window_seconds as i64 * 1000 || at > now_ms {
                continue;
            }
            if let Some(value) = point[1].as_f64().filter(|n| n.is_finite()) {
                latest = Some(latest.map_or(at, |old: i64| old.max(at)));
                count += 1;
                series_count += 1;
                if series_latest.is_none_or(|(previous, _)| at > previous) {
                    series_latest = Some((at, value));
                }
            } else if !point[1].is_null() {
                return Err("invalid Datadog point value".into());
            }
        }
        // Bound stored evidence; preserve exact latest numeric point per returned series.
        if let Some((at, value)) = series_latest {
            samples.push(json!({"metric":series.get("metric"),"scope":series.get("scope"),"latest_point":[at,value],"numeric_point_count":series_count}));
        }
    }
    let coverage = match latest {
        None => Coverage::Absent,
        Some(at) if now_ms - at > *max_age_seconds as i64 * 1000 => Coverage::Stale,
        Some(_) => Coverage::Measured,
    };
    let outcome = match coverage {
        Coverage::Absent => "no_numeric_points",
        Coverage::Stale => "stale_numeric_points",
        _ => "numeric_points_present",
    };
    Ok(Facts {
        coverage,
        outcome: outcome.into(),
        revision: String::new(),
        facts: json!({"series_returned":series.len(),"numeric_point_count":count,"latest_at_ms":latest,"series":samples}),
        sample_kind: "metric_timeseries",
    })
}
