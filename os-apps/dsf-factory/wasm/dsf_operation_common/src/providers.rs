use crate::*;
use chrono::DateTime;

pub(super) const RAILWAY: &str = "https://backboard.railway.com/graphql/v2";
pub(super) const DSF: &str = "https://api.deep-sci-fi.world";

pub(super) struct Receipt {
    pub id: String,
    pub url: String,
    pub status: String,
    pub revision: String,
    pub claimed_generation_ids: Vec<String>,
}

pub(super) fn execute(runtime: &mut Runtime<impl Host>, op: &Operation) -> Result<Callback, Error> {
    let config = runtime.load(op)?;
    validation::validate_loaded(runtime, op, &config)?;
    // A provider read may adopt an already accepted matching operation.
    if let Some(receipt) = find(runtime, op, &config)? {
        return Ok(receipt_callback("ExecutionSucceeded", op, receipt));
    }
    if op.kind != Kind::MediaRepair && op.execution_attempts != 1 {
        return Err(Error::Pending(
            "deployment send is never repeated after ambiguity",
        ));
    }
    let receipt = match &config.provider {
        Provider::Railway {
            service_id,
            environment_id,
            baseline_deployment_id,
            ..
        } => {
            let current = railway_latest(runtime, &config, service_id, environment_id)?;
            if required(&current, "id")? != baseline_deployment_id {
                return Err(Error::Binding("Railway baseline changed before deployment"));
            }
            let value=runtime.provider(&config,"POST",RAILWAY.into(),json!({"query":"mutation DsfDeploy($serviceId:String!,$environmentId:String!,$commitSha:String!){serviceInstanceDeployV2(serviceId:$serviceId,environmentId:$environmentId,commitSha:$commitSha)}","variables":{"serviceId":service_id,"environmentId":environment_id,"commitSha":op.revision}}))?;
            let id = value
                .pointer("/data/serviceInstanceDeployV2")
                .and_then(Value::as_str)
                .ok_or(Error::Response("Railway deployment ID"))?;
            Receipt {
                id: id.into(),
                url: railway_evidence(&config, id),
                status: "accepted".into(),
                revision: op.revision.clone(),
                claimed_generation_ids: Vec::new(),
            }
        }
        Provider::Vercel {
            project_id,
            team_id,
            project_name,
            git_repository_id,
            ..
        } => {
            let url = format!(
                "https://api.vercel.com/v13/deployments?teamId={}",
                encoded(team_id)
            );
            let value=runtime.provider(&config,"POST",url,json!({"name":project_name,"project":project_id,"target":"production","gitSource":{"type":"github","repoId":git_repository_id,"sha":op.revision},"meta":{"dsfOperationKey":op.key,"dsfEffortId":op.effort_id}}))?;
            vercel_receipt(&config, op, &value)?
        }
        Provider::Media { generations, .. } => {
            validate_media_selection(runtime, &config, generations)?;
            let value=runtime.provider(&config,"POST",format!("{DSF}/api/media/retry-stuck"),json!({"operation_id":op.key,"generation_ids":generations.iter().map(|g|&g.id).collect::<Vec<_>>()}))?;
            media_response(op, &config, &value)?
        }
    };
    Ok(receipt_callback("ExecutionSucceeded", op, receipt))
}

pub(super) fn observe(runtime: &mut Runtime<impl Host>, op: &Operation) -> Result<Callback, Error> {
    let config = runtime.load(op)?;
    match find(runtime, op, &config)? {
        Some(receipt) => Ok(receipt_callback("ProviderFound", op, receipt)),
        None if op.kind == Kind::MediaRepair => Ok(Callback {
            action: "ProviderAbsent",
            params: json!({"operation_key":op.key,"absence_evidence_ref":format!("{DSF}/api/media/recovery-operations/{}",op.key)}),
        }),
        None => Err(Error::Pending(
            "no unique correlated provider deployment; absence is not established",
        )),
    }
}

fn receipt_callback(action: &'static str, op: &Operation, receipt: Receipt) -> Callback {
    Callback {
        action,
        params: json!({"operation_key":op.key,"provider_execution_id":receipt.id,"provider_evidence_ref":receipt.url}),
    }
}

pub(super) fn find(
    runtime: &mut Runtime<impl Host>,
    op: &Operation,
    config: &Config,
) -> Result<Option<Receipt>, Error> {
    match &config.provider {
        Provider::Railway {
            project_id,
            service_id,
            environment_id,
            baseline_deployment_id,
            ..
        } => find_railway(
            runtime,
            op,
            config,
            project_id,
            service_id,
            environment_id,
            baseline_deployment_id,
        ),
        Provider::Vercel {
            project_id,
            team_id,
            ..
        } => find_vercel(runtime, op, config, project_id, team_id),
        Provider::Media { generations, .. } => find_media(runtime, op, config, generations),
    }
}

fn find_railway(
    runtime: &mut Runtime<impl Host>,
    op: &Operation,
    config: &Config,
    project_id: &str,
    service_id: &str,
    environment_id: &str,
    baseline_deployment_id: &str,
) -> Result<Option<Receipt>, Error> {
    let value=runtime.provider(config,"POST",RAILWAY.into(),json!({"query":"query DsfDeployments($input:DeploymentListInput!){deployments(input:$input,first:50){edges{node{id status createdAt meta}}}}","variables":{"input":{"projectId":project_id,"serviceId":service_id,"environmentId":environment_id}}}))?;
    let nodes = value
        .pointer("/data/deployments/edges")
        .and_then(Value::as_array)
        .ok_or(Error::Response("Railway deployment list"))?;
    let mut candidates = Vec::new();
    for edge in nodes.iter().take(50) {
        let node = edge
            .get("node")
            .ok_or(Error::Response("Railway deployment"))?;
        let id = required(node, "id")?;
        let revision = node
            .pointer("/meta/commitHash")
            .and_then(Value::as_str)
            .unwrap_or("");
        if revision != op.revision {
            continue;
        }
        if let Some(expected) = &op.execution_id {
            if id != expected {
                continue;
            }
        } else if id == baseline_deployment_id
            || DateTime::parse_from_rfc3339(required(node, "createdAt")?)
                .map_err(|_| Error::Response("Railway creation time"))?
                .timestamp_millis()
                < config.not_before_ms
        {
            continue;
        }
        candidates.push(Receipt {
            id: id.into(),
            url: railway_evidence(config, id),
            status: required(node, "status")?.into(),
            revision: revision.into(),
            claimed_generation_ids: Vec::new(),
        });
    }
    unique(candidates)
}

fn find_vercel(
    runtime: &mut Runtime<impl Host>,
    op: &Operation,
    config: &Config,
    project_id: &str,
    team_id: &str,
) -> Result<Option<Receipt>, Error> {
    let url = format!(
        "https://api.vercel.com/v6/deployments?projectId={}&teamId={}&limit=20&meta-dsfOperationKey={}",
        encoded(project_id),
        encoded(team_id),
        encoded(&op.key)
    );
    let value = runtime.provider(config, "GET", url, json!({}))?;
    let deployments = value
        .get("deployments")
        .and_then(Value::as_array)
        .ok_or(Error::Response("Vercel deployment list"))?;
    let mut candidates = Vec::new();
    for deployment in deployments.iter().take(20) {
        if deployment
            .pointer("/meta/dsfOperationKey")
            .and_then(Value::as_str)
            != Some(op.key.as_str())
        {
            continue;
        }
        let id = deployment
            .get("uid")
            .or_else(|| deployment.get("id"))
            .and_then(Value::as_str)
            .ok_or(Error::Response("Vercel deployment ID"))?;
        if op
            .execution_id
            .as_ref()
            .is_some_and(|expected| expected != id)
        {
            continue;
        }
        let detail = runtime.provider(
            config,
            "GET",
            format!(
                "https://api.vercel.com/v13/deployments/{}?teamId={}",
                encoded(id),
                encoded(team_id)
            ),
            json!({}),
        )?;
        candidates.push(vercel_receipt(config, op, &detail)?);
        if candidates.len() > 1 {
            return Err(Error::Pending(
                "multiple deployments share the operation key",
            ));
        }
    }
    unique(candidates)
}

fn find_media(
    runtime: &mut Runtime<impl Host>,
    op: &Operation,
    config: &Config,
    generations: &[Generation],
) -> Result<Option<Receipt>, Error> {
    let result = runtime.provider(
        config,
        "GET",
        format!("{DSF}/api/media/recovery-operations/{}", encoded(&op.key)),
        json!({}),
    );
    let value = match result {
        Err(Error::Http(404, _)) => return Ok(None),
        other => other?,
    };
    if required(&value, "operation_id")? != op.key
        || required(&value, "endpoint")? != "/api/media/retry-stuck"
    {
        return Err(Error::Binding("media receipt belongs to another operation"));
    }
    let expected: std::collections::BTreeSet<_> =
        generations.iter().map(|g| g.id.as_str()).collect();
    let actual: std::collections::BTreeSet<_> = value
        .get("generation_ids")
        .and_then(Value::as_array)
        .ok_or(Error::Response("media receipt selection"))?
        .iter()
        .map(|v| v.as_str().ok_or(Error::Response("media receipt ID")))
        .collect::<Result<_, _>>()?;
    if expected != actual {
        return Err(Error::Binding("media receipt selection changed"));
    }
    Ok(Some(media_response(
        op,
        config,
        value
            .get("response")
            .ok_or(Error::Response("media receipt response"))?,
    )?))
}

fn unique(mut receipts: Vec<Receipt>) -> Result<Option<Receipt>, Error> {
    if receipts.len() > 1 {
        Err(Error::Pending("multiple correlated provider executions"))
    } else {
        Ok(receipts.pop())
    }
}
fn railway_evidence(config: &Config, id: &str) -> String {
    if let Provider::Railway {
        project_id,
        service_id,
        environment_id,
        ..
    } = &config.provider
    {
        format!(
            "https://railway.com/project/{project_id}/service/{service_id}?environmentId={environment_id}&id={id}"
        )
    } else {
        unreachable!()
    }
}
fn railway_latest(
    runtime: &mut Runtime<impl Host>,
    config: &Config,
    service: &str,
    environment: &str,
) -> Result<Value, Error> {
    let value=runtime.provider(config,"POST",RAILWAY.into(),json!({"query":"query DsfCurrent($serviceId:String!,$environmentId:String!){serviceInstance(serviceId:$serviceId,environmentId:$environmentId){latestDeployment{id status meta}}}","variables":{"serviceId":service,"environmentId":environment}}))?;
    value
        .pointer("/data/serviceInstance/latestDeployment")
        .cloned()
        .ok_or(Error::Response("Railway current deployment"))
}

fn vercel_receipt(config: &Config, op: &Operation, value: &Value) -> Result<Receipt, Error> {
    let Provider::Vercel {
        project_id,
        team_id,
        ..
    } = &config.provider
    else {
        return Err(Error::Binding("wrong Vercel configuration"));
    };
    let returned_project = value
        .get("projectId")
        .and_then(Value::as_str)
        .or_else(|| value.pointer("/project/id").and_then(Value::as_str));
    if returned_project != Some(project_id.as_str())
        || value.get("target").and_then(Value::as_str) != Some("production")
        || value
            .pointer("/meta/dsfOperationKey")
            .and_then(Value::as_str)
            != Some(&op.key)
    {
        return Err(Error::Binding(
            "Vercel deployment target or operation differs",
        ));
    }
    let revision = value
        .pointer("/meta/githubCommitSha")
        .or_else(|| value.pointer("/gitSource/sha"))
        .and_then(Value::as_str)
        .ok_or(Error::Response("Vercel commit"))?;
    if revision != op.revision {
        return Err(Error::Binding("Vercel deployed another revision"));
    }
    let id = required(value, "id")?;
    Ok(Receipt {
        id: id.into(),
        url: format!(
            "https://api.vercel.com/v13/deployments/{}?teamId={}",
            encoded(id),
            encoded(team_id)
        ),
        status: required(value, "readyState")?.into(),
        revision: revision.into(),
        claimed_generation_ids: Vec::new(),
    })
}

fn media_response(op: &Operation, config: &Config, value: &Value) -> Result<Receipt, Error> {
    if required(value, "operation_id")? != op.key {
        return Err(Error::Binding("media operation ID differs"));
    }
    let Provider::Media { generations, .. } = &config.provider else {
        return Err(Error::Binding("wrong media configuration"));
    };
    let returned = value
        .get("generations")
        .and_then(Value::as_array)
        .ok_or(Error::Response("media recovery results"))?;
    if returned.len() != generations.len() {
        return Err(Error::Binding("media recovery selection differs"));
    }
    let expected: std::collections::BTreeSet<_> =
        generations.iter().map(|g| g.id.as_str()).collect();
    let actual: std::collections::BTreeSet<_> = returned
        .iter()
        .map(|v| required(v, "generation_id"))
        .collect::<Result<_, _>>()?;
    if expected != actual {
        return Err(Error::Binding("media recovery selection differs"));
    }
    for item in returned {
        if !["claimed", "ineligible", "missing"].contains(&required(item, "outcome")?) {
            return Err(Error::Response("media recovery outcome"));
        }
    }
    Ok(Receipt {
        id: op.key.clone(),
        url: format!("{DSF}/api/media/recovery-operations/{}", op.key),
        status: if returned
            .iter()
            .all(|v| v.get("outcome").and_then(Value::as_str) == Some("claimed"))
        {
            "claimed"
        } else {
            "partial"
        }
        .into(),
        revision: op.revision.clone(),
        claimed_generation_ids: returned
            .iter()
            .filter(|v| v.get("outcome").and_then(Value::as_str) == Some("claimed"))
            .map(|v| required(v, "generation_id").map(str::to_owned))
            .collect::<Result<_, _>>()?,
    })
}

fn validate_media_selection(
    runtime: &mut Runtime<impl Host>,
    config: &Config,
    generations: &[Generation],
) -> Result<(), Error> {
    for generation in generations {
        let value = runtime.provider(
            config,
            "GET",
            format!("{DSF}/api/media/{}/status", encoded(&generation.id)),
            json!({}),
        )?;
        check_generation(generation, &value)?;
        if !["pending", "failed", "generating"].contains(&required(&value, "status")?) {
            return Err(Error::Binding("selected job cannot be repaired"));
        }
        let estimated = match generation.media_type.as_str() {
            "cover_image" => 2,
            "video" => {
                let duration = value
                    .get("duration_seconds")
                    .and_then(Value::as_f64)
                    .filter(|value| {
                        value.is_finite() && (5.0..=15.0).contains(value) && value.fract() == 0.0
                    })
                    .ok_or(Error::Binding(
                        "video cost requires a valid configured duration",
                    ))?;
                duration as u64 * 5
            }
            _ => return Err(Error::Binding("unsupported media type")),
        };
        if estimated > generation.max_cost_cents {
            return Err(Error::Binding("selected job price exceeds its ceiling"));
        }
    }
    Ok(())
}

pub(super) fn check_generation(expected: &Generation, value: &Value) -> Result<(), Error> {
    if required(value, "generation_id")? != expected.id
        || required(value, "target_type")? != expected.target_type
        || required(value, "target_id")? != expected.target_id
        || required(value, "media_type")? != expected.media_type
    {
        return Err(Error::Binding("media resource selection changed"));
    }
    Ok(())
}
