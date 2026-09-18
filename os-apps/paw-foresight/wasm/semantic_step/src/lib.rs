use temper_wasm_sdk::prelude::*;
mod core {
    include!("../../semantic_core.rs");
}
fn step(ctx: &Context) -> Result<(), String> {
    let mut p = core::parse(core::field(&ctx.entity_state, "program_json"))?;
    let trace = core::parse(core::field(&ctx.entity_state, "trace_json"))?;
    let cursor = p["cursor"].as_u64().ok_or("Missing cursor")? as usize;
    let started = core::field(&ctx.entity_state, "started_at_ms")
        .parse::<u64>()
        .map_err(|_| "Missing start time")?;
    let now = Context::get_time_millis() as u64;
    let count = p["tasks"].as_array().ok_or("Missing tasks")?.len();
    if cursor >= count
        || trace.as_array().ok_or("Missing trace")?.len() >= core::MAX_CALLS
        || now.saturating_sub(started) >= core::MAX_MS
    {
        p["stop_reason"] = json!(if cursor >= count {
            "traversal_complete"
        } else {
            "budget_exhausted"
        });
        p["remaining_calls"] = json!(count.saturating_sub(cursor));
        let snapshot = core::parse(core::field(&ctx.entity_state, "snapshot_json"))?;
        let budget_available = trace.as_array().unwrap().len() < core::MAX_CALLS
            && now.saturating_sub(started) < core::MAX_MS;
        let phase = if core::field(&ctx.entity_state, "phase") == "seed"
            && budget_available
            && !core::repair_candidates(&snapshot, &p).is_empty()
        {
            "deepen"
        } else {
            "synthesize"
        };
        set_success_result(
            "Reason",
            &json!({"phase":phase,"program_json":p.to_string(),"trace_json":trace.to_string()}),
        );
    } else {
        let snapshot = core::parse(core::field(&ctx.entity_state, "snapshot_json"))?;
        let request = core::request(&snapshot, &p)?;
        set_success_result(
            "Evaluate",
            &json!({"request_json":request.to_string(),"program_json":p.to_string(),"trace_json":trace.to_string()}),
        );
    }
    Ok(())
}
#[unsafe(no_mangle)]
pub extern "C" fn run(_: i32, _: i32) -> i32 {
    match Context::from_host().and_then(|ctx| step(&ctx)) {
        Ok(()) => (),
        Err(e) => set_success_result("Fail", &json!({"error_message":e})),
    };
    0
}
