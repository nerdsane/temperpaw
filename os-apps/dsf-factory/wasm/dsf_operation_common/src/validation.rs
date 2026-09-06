use crate::*;

pub(super) fn validate(
    runtime: &mut Runtime<impl Host>,
    op: &Operation,
) -> Result<Callback, Error> {
    let config = runtime.load(op)?;
    validate_loaded(runtime, op, &config)?;
    Ok(Callback {
        action: "ValidationSucceeded",
        params: json!({"operation_key":op.key,"validation_evidence_ref":format!("{}/tdata/ProofPackets('{}')",runtime.base,op.proof_id)}),
    })
}

pub(super) fn validate_loaded(
    runtime: &mut Runtime<impl Host>,
    op: &Operation,
    config: &Config,
) -> Result<(), Error> {
    let resource = runtime.row("DsfResources", &op.resource_id)?;
    let effort = runtime.row("Efforts", &op.effort_id)?;
    let proof = runtime.row("ProofPackets", &op.proof_id)?;
    validate_records(op, config, &resource, &effort, &proof)?;
    validate_asks(runtime, config, &effort)?;
    chain_proof_ready::proof_packet_holds(&proof, Some(&op.revision)).map_err(Error::Proof)?;
    let artifact = required(&proof, "artifact_ref")?;
    let file = runtime.row("Files", artifact)?;
    if required(&file, "status")? != "Ready" {
        return Err(Error::Proof("artifact File is not Ready".into()));
    }
    let bytes = runtime.read("Files", artifact, true)?;
    if bytes.status != 200 || bytes.body.trim().is_empty() || bytes.body.len() > 1_048_576 {
        return Err(Error::Proof("artifact File cannot be read".into()));
    }
    validate_cost_and_selection(config)?;
    Ok(())
}

pub(super) fn validate_records(
    op: &Operation,
    config: &Config,
    resource: &Value,
    effort: &Value,
    proof: &Value,
) -> Result<(), Error> {
    if required(resource, "status")? != "Operating"
        || required(resource, "active_operation_id")? != op.key
    {
        return Err(Error::Binding("resource ownership lock differs"));
    }
    let allowed = decoded(resource, "allowed_operations")?;
    let kind = match op.kind {
        Kind::RailwayDeploy => "railway_deploy",
        Kind::VercelDeploy => "vercel_deploy",
        Kind::MediaRepair => "media_repair",
    };
    if !allowed
        .as_array()
        .is_some_and(|a| a.iter().any(|v| v.as_str() == Some(kind)))
    {
        return Err(Error::Binding("resource does not allow this operation"));
    }
    if required(resource, "provider")? != config.provider.resource_provider()
        || required(resource, "provider_id")? != config.provider.provider_id()
    {
        return Err(Error::Binding("provider differs from resource"));
    }
    if !["Merged", "Deploying", "Verified"].contains(&required(effort, "status")?)
        || required(effort, "head_sha")? != op.revision
    {
        return Err(Error::Binding(
            "Effort is not ready at the intended revision",
        ));
    }
    for flag in [
        "proof_attached",
        "e2e_ok",
        "review_passed",
        "evaluation_passed",
    ] {
        if field(effort, flag) != Some(&Value::Bool(true)) {
            return Err(Error::Binding(
                "Effort review or proof gate is not satisfied",
            ));
        }
    }
    let proof_ids = field(effort, "proof_packet_ids")
        .map(|_| decoded(effort, "proof_packet_ids"))
        .transpose()?
        .unwrap_or(json!([]));
    if field(effort, "proof_packet_id").and_then(Value::as_str) != Some(op.proof_id.as_str())
        && !proof_ids
            .as_array()
            .is_some_and(|v| v.iter().any(|id| id.as_str() == Some(&op.proof_id)))
    {
        return Err(Error::Binding("proof is not attached to Effort"));
    }
    if required(proof, "effort_id")? != op.effort_id || required(proof, "commit")? != op.revision {
        return Err(Error::Binding(
            "proof belongs to another Effort or revision",
        ));
    }
    Ok(())
}

fn validate_asks(
    runtime: &mut Runtime<impl Host>,
    config: &Config,
    effort: &Value,
) -> Result<(), Error> {
    let raw = field(effort, "ask_ids")
        .map(|_| decoded(effort, "ask_ids"))
        .transpose()?
        .unwrap_or(json!([]));
    let ids = raw
        .as_array()
        .ok_or(Error::Binding("Effort Ask IDs are not an array"))?;
    if ids.len() > 32 || config.required_ask_ids.len() > 32 {
        return Err(Error::Binding("Ask read budget exceeded"));
    }
    for required in &config.required_ask_ids {
        if !ids.iter().any(|id| id.as_str() == Some(required)) {
            return Err(Error::Binding("required Ask is not linked to Effort"));
        }
    }
    for id in ids {
        let id = id.as_str().ok_or(Error::Binding("invalid Ask ID"))?;
        let ask = runtime.row("Asks", id)?;
        if required(&ask, "effort_id")? != config.effort_id {
            return Err(Error::Binding("Ask belongs to another Effort"));
        }
        let status = required(&ask, "status")?;
        let blocking = field(&ask, "stalls") == Some(&Value::Bool(true));
        if status == "Open" && blocking {
            return Err(Error::Blocked(id.into()));
        }
        if config.required_ask_ids.iter().any(|r| r == id) {
            if status != "Answered" {
                return Err(Error::Blocked(id.into()));
            }
            let choice = required(&ask, "chose")?;
            required(&ask, "who")?;
            if [
                "no",
                "deny",
                "denied",
                "reject",
                "rejected",
                "cancel",
                "cancelled",
            ]
            .contains(&choice.trim().to_ascii_lowercase().as_str())
            {
                return Err(Error::Blocked(id.into()));
            }
        }
    }
    Ok(())
}

fn validate_cost_and_selection(config: &Config) -> Result<(), Error> {
    match &config.provider {
        Provider::Media { generations, .. } => {
            if generations.is_empty() || generations.len() > 20 {
                return Err(Error::Binding("media selection must contain 1..20 jobs"));
            }
            let mut total = 0u64;
            let mut ids = std::collections::BTreeSet::new();
            for job in generations {
                identifier(&job.id)?;
                identifier(&job.target_id)?;
                if !ids.insert(&job.id) || job.max_cost_cents == 0 {
                    return Err(Error::Binding("duplicate or unbudgeted media job"));
                }
                total = total
                    .checked_add(job.max_cost_cents)
                    .ok_or(Error::Binding("cost overflow"))?;
            }
            if total > config.max_cost_cents {
                return Err(Error::Binding(
                    "selected repair exceeds its authorized cost ceiling",
                ));
            }
            if !matches!(config.flow, Flow::Media {}) {
                return Err(Error::Binding(
                    "media repair requires selected-job verification",
                ));
            }
        }
        Provider::Railway { .. } | Provider::Vercel { .. } => {
            if config.max_cost_cents != 0 {
                return Err(Error::Binding(
                    "deployment adapter uses existing allocations only",
                ));
            }
        }
    }
    Ok(())
}
