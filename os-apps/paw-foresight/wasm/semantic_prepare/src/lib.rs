use temper_wasm_sdk::prelude::*;
mod core {
    include!("../../semantic_core.rs");
}
fn read(ctx: &Context, path: &str) -> Result<Value, String> {
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
    if response.body.len() > 2_000_000 {
        return Err("Snapshot read exceeds 2 MB".into());
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
fn run_inner(ctx: &Context) -> Result<(), String> {
    let direct = core::field(&ctx.entity_state, "world_id");
    let id = if direct.is_empty() {
        core::field(&ctx.entity_state, "parent_id")
    } else {
        direct
    };
    if id.is_empty()
        || !id
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b"-_".contains(&b))
    {
        return Err("Invalid world identity".into());
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
        &json!({"world_id":id,"agent_id":agent_id,"model":core::field(&world,"agent_model"),"provider":core::field(&world,"agent_provider"),"snapshot_json":snapshot.to_string(),"program_json":program.to_string(),"trace_json":"[]","started_at_ms":Context::get_time_millis().to_string()}),
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
