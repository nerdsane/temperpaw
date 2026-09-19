use temper_wasm_sdk::prelude::*;
// Preparation uses only the snapshot planner from the shared evaluator contract.
#[allow(dead_code, unused_imports)]
mod core {
    include!("../../semantic_core.rs");
}
fn read(ctx: &Context, path: &str) -> Result<Value, String> {
    read_bounded(ctx, path, 2_000_000)
}
fn read_bounded(ctx: &Context, path: &str, max_bytes: usize) -> Result<Value, String> {
    let api = ctx
        .config
        .get("temper_api_url")
        .filter(|s| !s.is_empty() && !s.contains("{secret:"))
        .ok_or("Missing Temper URL")?;
    let response = ctx.http_call(
        "GET",
        &format!("{api}/tdata/{path}"),
        &[
            ("x-tenant-id".into(), ctx.tenant.clone()),
            ("x-temper-principal-kind".into(), "agent".into()),
            ("x-temper-principal-id".into(), ctx.entity_id.clone()),
            ("x-temper-agent-type".into(), "system".into()),
        ],
        "",
    )?;
    if response.status != 200 {
        return Err(format!("Snapshot read HTTP {}", response.status));
    }
    if response.body.len() > max_bytes {
        return Err(format!(
            "Snapshot read exceeds {max_bytes} bytes; refusing truncation"
        ));
    }
    core::parse(&response.body)
}
fn snapshot_node(n: &Value) -> Value {
    let mut safe = json!({});
    for k in [
        "Id",
        "statement",
        "edges",
        "source_refs",
        "resolve_by",
        "provenance",
        "probability",
        "resolution",
        "Status",
    ] {
        safe[k] = json!(core::field(n, k));
    }
    // EventNode's empty default means it declares no dependency edges.
    if core::field(&safe, "edges").trim().is_empty() {
        safe["edges"] = json!("[]");
    }
    safe["kind"] = json!(if core::field(n, "provenance") == "hypothesis" {
        "scenario"
    } else {
        "evidence"
    });
    safe
}
fn valid_id(id: &str) -> bool {
    !id.is_empty()
        && id.len() <= 200
        && id
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b"-_".contains(&b))
}

/// Resume only trusted persisted state; no caller-provided graph or evaluations.
fn resume_checkpoint(record: &Value, world_id: &str, now_ms: u64) -> Result<Value, String> {
    if !matches!(core::field(record, "Status"), "Failed" | "Cancelled") {
        return Err("Only failed or cancelled exploration can be resumed".into());
    }
    if core::field(record, "world_id") != world_id {
        return Err("Resume world mismatch".into());
    }
    let snapshot_raw = core::field(record, "snapshot_json");
    let program_raw = core::field(record, "program_json");
    let trace_raw = core::field(record, "trace_json");
    let snapshot = core::parse(snapshot_raw)?;
    let mut program = core::parse(program_raw)?;
    let trace = core::parse(trace_raw)?;
    if core::field(&snapshot["world"], "Id") != world_id {
        return Err("Resume snapshot world mismatch".into());
    }
    let nodes = snapshot["nodes"].as_array().ok_or("Missing resume nodes")?;
    core::plan(nodes)?;
    if program["schema"] != "foresight-open-semantic-v2" {
        return Err("Unsupported resume program".into());
    }
    let tasks = program["tasks"].as_array().ok_or("Missing resume tasks")?;
    let cursor = program["cursor"]
        .as_u64()
        .filter(|n| *n <= tasks.len() as u64)
        .ok_or("Invalid resume cursor")?;
    if !program["results"].is_object()
        || !program["evaluations"].is_object()
        || !program["rounds"].is_array()
    {
        return Err("Invalid resume evaluations or history".into());
    }
    let ids: std::collections::BTreeSet<_> = nodes.iter().map(|n| core::field(n, "Id")).collect();
    for task in tasks {
        if !ids.contains(core::field(task, "nodeId")) {
            return Err("Resume task references missing node".into());
        }
    }
    let traces = trace.as_array().ok_or("Invalid resume trace")?;
    if traces.len() > core::MAX_CALLS || trace_raw.len() > core::MAX_TRACE_BYTES {
        return Err("Resume trace exceeds budget".into());
    }
    for (index, item) in traces.iter().enumerate() {
        if item["index"].as_u64() != Some(index as u64)
            || !ids.contains(core::field(item, "nodeId"))
            || !((item["response"].is_object() && item["request"].is_object())
                || (item["requestFormat"] == "failed-attempt-hash-only"
                    && item["error"].as_str().is_some_and(|s| !s.is_empty())
                    && item["requestHash"].as_str().is_some_and(|s| {
                        s.len() == 64 && s.bytes().all(|b| b.is_ascii_hexdigit())
                    })))
        {
            return Err("Corrupt resume trace".into());
        }
    }
    let started_raw = core::field(record, "started_at_ms");
    let started = started_raw
        .parse::<u64>()
        .ok()
        .filter(|n| *n > 0 && *n <= now_ms)
        .ok_or("Invalid resume start time")?;
    let phase = core::field(record, "phase");
    if !matches!(phase, "seed" | "explore" | "synthesize") {
        return Err("Invalid resume phase".into());
    }
    let evaluated = nodes.iter().any(|n| {
        matches!(core::field(n, "kind"), "scenario" | "revision")
            && program["results"][core::field(n, "Id")]["estimate_likelihood"]
                .as_str()
                .and_then(|v| v.parse::<f64>().ok())
                .is_some_and(|v| v.is_finite() && (0.0..=1.0).contains(&v))
    });
    let exhausted = now_ms - started >= core::MAX_MS
        || traces.len() >= core::MAX_CALLS
        || nodes.len() >= core::MAX_NODES
        || program["round"].as_u64().unwrap_or(0) >= core::MAX_ROUNDS;
    let phase = if exhausted {
        if !evaluated {
            return Err("Exhausted checkpoint has no evaluated hypothesis to synthesize".into());
        }
        "synthesize"
    } else {
        phase
    };
    if phase == "synthesize" && !evaluated {
        return Err("Resume synthesis needs an evaluated hypothesis".into());
    }
    let agent_id = core::field(record, "agent_id");
    let model = core::field(record, "model");
    let provider = core::field(record, "provider");
    if !valid_id(agent_id) || model.trim().is_empty() || provider.trim().is_empty() {
        return Err("Missing resume agent or provider metadata".into());
    }
    // Keep the program exactly unless a spent budget needs its honest stop reason.
    let pending_tasks = cursor < tasks.len() as u64;
    let program_raw = if exhausted {
        program["stop_reason"] = json!(if now_ms - started >= core::MAX_MS {
            "time_budget"
        } else if traces.len() >= core::MAX_CALLS {
            "call_budget"
        } else if nodes.len() >= core::MAX_NODES {
            "node_budget"
        } else {
            "round_budget"
        });
        program.to_string()
    } else if pending_tasks && program["stop_reason"] == "provider_error" {
        // The failed attempt remains in trace and consumes budget; only its
        // transient stop marker clears so the exact unfinished task can retry.
        program["stop_reason"] = json!("resumed_after_provider_error");
        program.to_string()
    } else {
        program_raw.to_owned()
    };
    Ok(
        json!({"world_id":world_id,"agent_id":agent_id,"model":model,"provider":provider,
        "snapshot_json":snapshot_raw,"program_json":program_raw,"trace_json":trace_raw,
        "started_at_ms":started_raw,"phase":phase}),
    )
}

fn resume_transition(prepared: &Value) -> Result<&'static str, String> {
    let program = core::parse(core::field(prepared, "program_json"))?;
    let cursor = program["cursor"].as_u64().ok_or("Missing resume cursor")?;
    let count = program["tasks"]
        .as_array()
        .ok_or("Missing resume tasks")?
        .len() as u64;
    Ok(
        if core::field(prepared, "phase") != "synthesize" && cursor < count {
            "ResumePrepared"
        } else {
            "Prepared"
        },
    )
}

fn run_inner(ctx: &Context) -> Result<(), String> {
    let direct = core::field(&ctx.entity_state, "world_id");
    let id = if direct.is_empty() {
        core::field(&ctx.entity_state, "parent_id")
    } else {
        direct
    };
    if !valid_id(id) {
        return Err("Invalid world identity".into());
    }
    let resume_id = core::field(&ctx.entity_state, "resume_run_id");
    if !resume_id.is_empty() {
        if !valid_id(resume_id) || resume_id == ctx.entity_id {
            return Err("Invalid resume identity".into());
        }
        // Leave overhead beneath the SDK's 4 MiB HTTP buffer; select only the
        // checkpoint payload and trusted launch metadata, never session prompts.
        let path = format!(
            "SemanticRuns('{resume_id}')?$select=Id,Status,world_id,agent_id,model,provider,snapshot_json,program_json,trace_json,started_at_ms,phase"
        );
        let record = read_bounded(ctx, &path, 3_500_000)?;
        let prepared = resume_checkpoint(&record, id, Context::get_time_millis() as u64)?;
        set_success_result(resume_transition(&prepared)?, &prepared);
        return Ok(());
    }
    let world = read(ctx, &format!("Worlds('{id}')"))?;
    if core::field(&world, "Status") != "Active" {
        return Err("Semantic exploration needs an active world".into());
    }
    let rows = read(
        ctx,
        &format!("EventNodes?$filter=world_id%20eq%20'{id}'&$top=513"),
    )?;
    if rows["value"].as_array().ok_or("Missing event list")?.len() > 512 {
        return Err(
            "World exceeds the 512-event snapshot limit; refusing silent truncation".into(),
        );
    }
    let mut nodes = vec![];
    for n in rows["value"].as_array().ok_or("Missing event list")? {
        if core::field(n, "world_id") != id {
            return Err("Cross-world snapshot rejected".into());
        }
        let safe = snapshot_node(n);
        nodes.push(safe);
    }
    let program = core::plan(&nodes)?;
    let session_id = core::field(&world, "research_session_id");
    if session_id.is_empty()
        || !session_id
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b"-_".contains(&b))
    {
        return Err("World has no verified research session".into());
    }
    let session = read(ctx, &format!("Sessions('{session_id}')"))?;
    let agent_id = core::field(&session, "agent_id");
    if agent_id.is_empty() {
        return Err("Research agent is unavailable".into());
    }
    let mut safe_world = json!({});
    for k in [
        "Id",
        "name",
        "domain",
        "description",
        "last_ingest_date",
        "target_date",
        "learning_mode",
        "hindcast_mode",
    ] {
        safe_world[k] = json!(core::field(&world, k));
    }
    let snapshot = json!({"world":safe_world,"nodes":nodes});
    if snapshot.to_string().len() > 2 * 1024 * 1024 {
        return Err("World snapshot exceeds 2 MB".into());
    }
    set_success_result(
        "Prepared",
        &json!({"world_id":id,"agent_id":agent_id,"model":core::field(&world,"agent_model"),"provider":core::field(&world,"agent_provider"),"snapshot_json":snapshot.to_string(),"program_json":program.to_string(),"trace_json":"[]","started_at_ms":Context::get_time_millis().to_string(),"phase":"seed"}),
    );
    Ok(())
}
#[unsafe(no_mangle)]
pub extern "C" fn run(_: i32, _: i32) -> i32 {
    match Context::from_host().and_then(|ctx| run_inner(&ctx)) {
        Ok(()) => (),
        Err(e) => set_success_result("Fail", &json!({"error_message":e})),
    };
    0
}

#[cfg(test)]
mod tests {
    use super::*;
    fn checkpoint() -> Value {
        let snapshot = json!({"world":{"Id":"w"},"nodes":[{"Id":"h","kind":"scenario","statement":"Future","edges":"[]"}]});
        let program = json!({"schema":"foresight-open-semantic-v2","tasks":[],"cursor":0,"results":{"h":{"estimate_likelihood":"0.37"}},"evaluations":{},"rounds":[],"round":2});
        json!({"Status":"Failed","world_id":"w","snapshot_json":snapshot.to_string(),"program_json":program.to_string(),"trace_json":"[]","started_at_ms":"1000","phase":"explore","agent_id":"agent-a","model":"model-a","provider":"provider-a"})
    }
    #[test]
    fn resume_preserves_checkpoint_and_rejects_running_cross_world_or_corrupt_data() {
        let record = checkpoint();
        let prepared = resume_checkpoint(&record, "w", 2000).unwrap();
        for key in [
            "snapshot_json",
            "program_json",
            "trace_json",
            "started_at_ms",
            "agent_id",
            "model",
            "provider",
            "phase",
        ] {
            assert_eq!(prepared[key], record[key]);
        }
        assert!(resume_checkpoint(&record, "another-world", 2000).is_err());
        for status in ["Created", "Reasoning", "Completed"] {
            let mut bad = record.clone();
            bad["Status"] = json!(status);
            assert!(resume_checkpoint(&bad, "w", 2000).is_err());
        }
        let mut bad = record.clone();
        bad["snapshot_json"] = json!("{}");
        assert!(resume_checkpoint(&bad, "w", 2000).is_err());
        bad = record.clone();
        bad["trace_json"] = json!("not JSON");
        assert!(resume_checkpoint(&bad, "w", 2000).is_err());
        bad = record.clone();
        bad["trace_json"] = json!("[{\"index\":99}]");
        assert!(resume_checkpoint(&bad, "w", 2000).is_err());
    }
    #[test]
    fn unfinished_evaluations_resume_at_existing_cursor_without_losing_failed_attempt() {
        let mut record = checkpoint();
        let mut program: Value =
            serde_json::from_str(record["program_json"].as_str().unwrap()).unwrap();
        program["tasks"] = json!([{"nodeId":"h","function":"estimate_likelihood"}]);
        program["stop_reason"] = json!("provider_error");
        record["program_json"] = json!(program.to_string());
        record["trace_json"]=json!(json!([{"index":0,"nodeId":"h","requestFormat":"failed-attempt-hash-only","requestHash":"a".repeat(64),"error":"HTTP503"}]).to_string());
        let prepared = resume_checkpoint(&record, "w", 2000).unwrap();
        assert_eq!(resume_transition(&prepared).unwrap(), "ResumePrepared");
        assert_eq!(prepared["trace_json"], record["trace_json"]);
        let resumed: Value =
            serde_json::from_str(prepared["program_json"].as_str().unwrap()).unwrap();
        assert_eq!(resumed["cursor"], 0);
        assert_eq!(resumed["tasks"], program["tasks"]);
        assert_eq!(resumed["results"], program["results"]);
        assert_eq!(resumed["stop_reason"], "resumed_after_provider_error");
        let completed = resume_checkpoint(&checkpoint(), "w", 2000).unwrap();
        assert_eq!(resume_transition(&completed).unwrap(), "Prepared");
    }

    #[test]
    fn elapsed_budget_synthesizes_only_actual_evaluated_hypotheses() {
        let record = checkpoint();
        let prepared = resume_checkpoint(&record, "w", 1000 + core::MAX_MS).unwrap();
        assert_eq!(prepared["phase"], "synthesize");
        assert_eq!(prepared["started_at_ms"], "1000");
        let mut bad = record.clone();
        let mut p: Value = serde_json::from_str(record["program_json"].as_str().unwrap()).unwrap();
        p["results"] = json!({});
        bad["program_json"] = json!(p.to_string());
        assert!(resume_checkpoint(&bad, "w", 1000 + core::MAX_MS).is_err());
    }
    #[test]
    #[ignore = "Requires explicit local saved checkpoint fixture"]
    fn saved_252_call_checkpoint_is_preserved_exactly() {
        let path = std::env::var("FORESIGHT_RESUME_FIXTURE").expect("explicit fixture path");
        let record: Value = serde_json::from_str(&std::fs::read_to_string(path).unwrap()).unwrap();
        let now = core::field(&record, "started_at_ms")
            .parse::<u64>()
            .unwrap()
            + 1000;
        let prepared = resume_checkpoint(&record, core::field(&record, "world_id"), now).unwrap();
        assert_eq!(
            serde_json::from_str::<Value>(prepared["trace_json"].as_str().unwrap())
                .unwrap()
                .as_array()
                .unwrap()
                .len(),
            252
        );
        for key in [
            "snapshot_json",
            "program_json",
            "trace_json",
            "started_at_ms",
        ] {
            assert_eq!(prepared[key], core::field(&record, key));
        }
    }

    #[test]
    fn researched_hypotheses_remain_hypotheses_in_the_evaluation_graph() {
        let h = snapshot_node(
            &json!({"Id":"h","provenance":"hypothesis","statement":"An inferred future"}),
        );
        let e = snapshot_node(
            &json!({"Id":"e","provenance":"contested","statement":"A disputed source claim"}),
        );
        assert_eq!(h["kind"], "scenario");
        assert_eq!(e["kind"], "evidence");
        assert_eq!(e["provenance"], "contested");
        let plan = core::plan(&[h, e]).unwrap();
        assert_eq!(
            plan["tasks"]
                .as_array()
                .unwrap()
                .iter()
                .filter(|t| t["nodeId"] == "h")
                .count(),
            5
        );
    }
}
