use temper_wasm_sdk::prelude::*;
mod core {
    include!("../../semantic_core.rs");
}
fn provider_failure_without_estimates(program: &Value) -> Option<String> {
    if program["stop_reason"] != "provider_error" {
        return None;
    }
    let has_estimate = program["results"].as_object().is_some_and(|nodes| {
        nodes.values().any(|result| {
            result["estimate_likelihood"]
                .as_str()
                .and_then(|raw| raw.parse::<f64>().ok())
                .is_some_and(|p| p.is_finite() && (0.0..=1.0).contains(&p))
        })
    });
    if has_estimate {
        return None;
    }
    Some(
        program["last_error"]
            .as_str()
            .filter(|s| !s.trim().is_empty())
            .unwrap_or("Semantic provider failed before producing an event estimate")
            .to_owned(),
    )
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
    } else if trace_len >= core::MAX_CALLS {
        "call_budget".into()
    } else if elapsed_ms >= core::MAX_MS {
        "time_budget".into()
    } else if program["round"].as_u64().unwrap_or(0) >= core::MAX_ROUNDS {
        "round_budget".into()
    } else if snapshot["nodes"].as_array().map_or(0, Vec::len) >= core::MAX_NODES {
        "node_budget".into()
    } else {
        String::new()
    };
    if !exhausted.is_empty() {
        program["stop_reason"] = json!(exhausted);
        "synthesize"
    } else if program["continue_exploring"] == false {
        program["stop_reason"] = json!("exploration_converged");
        "synthesize"
    } else {
        program["stop_reason"] = json!("round_evaluated");
        "explore"
    }
}
fn step(ctx: &Context) -> Result<(), String> {
    let mut program = core::parse(core::field(&ctx.entity_state, "program_json"))?;
    let trace = core::parse(core::field(&ctx.entity_state, "trace_json"))?;
    if let Some(error) = provider_failure_without_estimates(&program) {
        return Err(error);
    }

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
    if cursor >= count || calls >= core::MAX_CALLS || elapsed >= core::MAX_MS || stopped {
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
        assert_eq!(next_phase(&s, &mut p, 5000, 1), "synthesize");
        assert_eq!(p["stop_reason"], "call_budget");
    }
    #[test]
    fn generator_can_conclude_without_filling_a_fixed_number() {
        let s = json!({"nodes":[]});
        let mut p = json!({"continue_exploring":false});
        assert_eq!(next_phase(&s, &mut p, 213, 1000), "synthesize");
        assert_eq!(p["stop_reason"], "exploration_converged");
    }
    #[test]
    fn provider_failure_without_event_estimates_preserves_original_error() {
        let mut p = json!({"stop_reason":"provider_error","last_error":"Semantic provider HTTP 402","results":{"e":{"classify_gap":"none"}}});
        assert_eq!(
            provider_failure_without_estimates(&p).as_deref(),
            Some("Semantic provider HTTP 402")
        );
        for invalid in ["NaN", "1.1", "not-a-probability"] {
            p["results"]["h"] = json!({"estimate_likelihood":invalid});
            assert!(provider_failure_without_estimates(&p).is_some());
        }
        for probability in ["0", "0.37", "1"] {
            p["results"]["h"] = json!({"estimate_likelihood":probability});
            assert!(provider_failure_without_estimates(&p).is_none());
            assert_eq!(next_phase(&json!({"nodes":[]}), &mut p, 1, 0), "synthesize");
        }
    }
}
