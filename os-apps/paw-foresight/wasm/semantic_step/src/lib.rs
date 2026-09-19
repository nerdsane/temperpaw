use temper_wasm_sdk::prelude::*;
mod core {
    include!("../../semantic_core.rs");
}
fn next_phase(
    snapshot: &Value,
    program: &mut Value,
    trace_len: usize,
    elapsed_ms: u64,
) -> &'static str {
    let exhausted = if matches!(
        program["stop_reason"].as_str(),
        Some("trace_budget" | "provider_error" | "time_budget")
    ) {
        program["stop_reason"].as_str().unwrap().to_owned()
    } else if trace_len >= core::call_limit(program) {
        "call_budget".into()
    } else if elapsed_ms >= core::time_limit(program) {
        "time_budget".into()
    } else if program["stage"] != "worlds"
        && program["round"].as_u64().unwrap_or(0) >= core::MAX_ROUNDS
    {
        "round_budget".into()
    } else if program["stage"] != "worlds"
        && snapshot["nodes"].as_array().map_or(0, Vec::len) >= core::MAX_NODES - 6
    {
        "node_budget".into()
    } else {
        String::new()
    };
    if program["stage"] == "worlds" {
        program["stop_reason"] = json!(if exhausted.is_empty() {
            "worlds_evaluated"
        } else {
            &exhausted
        });
        return "synthesize";
    }
    if !exhausted.is_empty() {
        program["stop_reason"] = json!(exhausted);
        "compose"
    } else if program["continue_exploring"] == false {
        program["stop_reason"] = json!("exploration_converged");
        "compose"
    } else {
        program["stop_reason"] = json!("round_evaluated");
        "explore"
    }
}
fn step(ctx: &Context) -> Result<(), String> {
    let mut program = core::parse(core::field(&ctx.entity_state, "program_json"))?;
    let trace = core::parse(core::field(&ctx.entity_state, "trace_json"))?;

    let cursor = program["cursor"].as_u64().ok_or("Missing cursor")? as usize;
    let started = core::field(&ctx.entity_state, "started_at_ms")
        .parse::<u64>()
        .map_err(|_| "Missing run start time")?;
    let elapsed = (Context::get_time_millis() as u64).saturating_sub(started);
    let count = program["tasks"].as_array().ok_or("Missing tasks")?.len();
    let calls = trace.as_array().ok_or("Missing trace")?.len();
    let stopped = matches!(
        program["stop_reason"].as_str(),
        Some("trace_budget" | "provider_error" | "time_budget" | "call_budget")
    );
    let snapshot = core::parse(core::field(&ctx.entity_state, "snapshot_json"))?;
    if cursor >= count
        || calls >= core::call_limit(&program)
        || elapsed >= core::time_limit(&program)
        || stopped
    {
        program["remaining_calls"] = json!(core::MAX_CALLS.saturating_sub(calls));
        program["remaining_round_tasks"] = json!(count.saturating_sub(cursor));
        let phase = next_phase(&snapshot, &mut program, calls, elapsed);
        set_success_result(
            "Reason",
            &json!({"phase":phase,"program_json":program.to_string(),"trace_json":trace.to_string()}),
        );
    } else {
        let request = core::request(&snapshot, &program)?;
        set_success_result("Evaluate", &json!({"request_json":request.to_string()}));
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
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn evaluates_multiple_rounds_instead_of_one_deepening() {
        let s = json!({"nodes":[]});
        let mut p = json!({"round":3,"continue_exploring":true});
        assert_eq!(next_phase(&s, &mut p, 1800, 900000), "explore");
        assert_eq!(p["stop_reason"], "round_evaluated");
    }
    #[test]
    fn operational_budget_stops_without_claiming_convergence() {
        let s = json!({"nodes":[]});
        let mut p = json!({"continue_exploring":true});
        assert_eq!(next_phase(&s, &mut p, 5000, 1), "compose");
        assert_eq!(p["stop_reason"], "call_budget");
    }
    #[test]
    fn generator_can_conclude_without_filling_a_fixed_number() {
        let s = json!({"nodes":[]});
        let mut p = json!({"continue_exploring":false});
        assert_eq!(next_phase(&s, &mut p, 213, 1000), "compose");
        assert_eq!(p["stop_reason"], "exploration_converged");
    }
    #[test]
    fn reserves_calls_and_time_for_world_evaluation() {
        let snapshot = json!({"nodes":[]});
        for (calls, time) in [(core::MAX_CALLS - 32, 0), (0, core::MAX_MS - 600_000)] {
            let mut program = json!({"stage":"exploration","continue_exploring":true});
            assert_eq!(next_phase(&snapshot, &mut program, calls, time), "compose");
        }
        let program = json!({"stage":"worlds"});
        assert_eq!(core::call_limit(&program), core::MAX_CALLS);
        assert_eq!(core::time_limit(&program), core::MAX_MS);
    }
    #[test]
    fn provider_failure_composes_qualitative_worlds_without_retrying_provider() {
        let mut p = json!({"stop_reason":"provider_error","results":{}});
        assert_eq!(next_phase(&json!({"nodes":[]}), &mut p, 1, 0), "compose");
        p["stage"] = json!("worlds");
        assert_eq!(next_phase(&json!({"nodes":[]}), &mut p, 1, 0), "synthesize");
    }
}
