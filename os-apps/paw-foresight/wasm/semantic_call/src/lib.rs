use sha2::{Digest, Sha256};
use temper_wasm_sdk::prelude::*;
mod core {
    include!("../../semantic_core.rs");
}
fn call(ctx: &Context) -> Result<(), String> {
    let mut p = core::parse(core::field(&ctx.entity_state, "program_json"))?;
    let mut trace = core::parse(core::field(&ctx.entity_state, "trace_json"))?;
    let snapshot = core::parse(core::field(&ctx.entity_state, "snapshot_json"))?;
    let key = ctx
        .config
        .get("typesafe_api_key")
        .filter(|s| !s.is_empty() && !s.contains("{secret:"))
        .ok_or("Configure foresight_typesafe_api_key in Temper settings")?;
    // Record each question separately; independent structural questions share an HTTP call.
    // Rebuild the next batch after recording answers so dependent estimates see audits.
    let mut trace_bytes = trace.to_string().len();
    for _ in 0..8 {
        let cursor = p["cursor"].as_u64().ok_or("Missing cursor")? as usize;
        if cursor >= p["tasks"].as_array().ok_or("Missing tasks")?.len() {
            break;
        }
        if trace.as_array().ok_or("Missing trace")?.len() >= core::call_limit(&p) {
            p["stop_reason"] = json!("call_budget");
            break;
        }
        // Reserve enough for the bounded response and metadata before spending a call.
        if trace_bytes + 192 * 1024 > core::MAX_TRACE_BYTES {
            p["stop_reason"] = json!("trace_budget");
            break;
        }
        if let Ok(started) = core::field(&ctx.entity_state, "started_at_ms").parse::<u64>() {
            if (Context::get_time_millis() as u64).saturating_sub(started) >= core::time_limit(&p) {
                p["stop_reason"] = json!("time_budget");
                break;
            }
        }
        let batch = core::batch::prepare(
            &snapshot,
            &p,
            core::call_limit(&p).saturating_sub(trace.as_array().unwrap().len()),
        )?;
        let request = &batch.request;
        let task = batch.tasks[0].clone();
        let node = task["nodeId"].as_str().ok_or("Missing node identity")?;
        let function = task["function"].as_str().ok_or("Missing function")?;
        let encoded = request.to_string();
        let started = Context::get_time_millis();
        let http_call = p["http_calls"]
            .as_u64()
            .unwrap_or(trace.as_array().unwrap().len() as u64)
            + 1;
        p["http_calls"] = json!(http_call);
        let response_result = (|| -> Result<serde_json::Value, String> {
            let r = ctx
                .http_call(
                    "POST",
                    "https://api.typesafe.ai/v1/systemone",
                    &[
                        ("content-type".into(), "application/json".into()),
                        ("authorization".into(), format!("Bearer {key}")),
                    ],
                    &encoded,
                )
                .map_err(|_| "Semantic provider transport failed")?;
            if !(200..300).contains(&r.status) {
                return Err(format!("Semantic provider HTTP {}", r.status));
            }
            if r.body.len() > 128 * 1024 {
                return Err("Provider response exceeds bound".into());
            }
            let response = core::parse(&r.body)?;
            core::batch::answers(&batch, &response)?;
            Ok(response)
        })();
        let response = match response_result {
            Ok(response) => response,
            Err(error) => {
                let index = trace.as_array().unwrap().len();
                trace.as_array_mut().unwrap().push(json!({"index":index,"nodeId":node,"function":function,"requestHash":format!("{:x}",Sha256::digest(encoded.as_bytes())),"startedAtMs":started,"elapsedMs":Context::get_time_millis()-started,"error":error,"requestFormat":"failed-attempt-hash-only","httpCallId":http_call,"task":task}));
                p["stop_reason"] = json!("provider_error");
                p["last_error"] = json!(error);
                break;
            }
        };
        let answers = core::batch::answers(&batch, &response)?;
        for (offset, (decision, mut evaluation, response)) in answers.into_iter().enumerate() {
            let task = &batch.tasks[offset];
            let node = core::field(task, "nodeId");
            let function = core::field(task, "function");
            let individual = &batch.individual[offset];
            let state = &individual["state"];
            let evidence_ids: Vec<_> = state["source_evidence"]
                .as_array()
                .into_iter()
                .flatten()
                .map(|n| n["Id"].clone())
                .collect();
            let context = json!({"round":p["round"],"world_revision":p["world_revision"],"world_pass":p["world_pass"],"evidence_ids":evidence_ids,"task":task});
            evaluation["context"] = context.clone();
            for key in ["results", "evaluations"] {
                if !p[key].is_object() {
                    p[key] = json!({});
                }
                if !p[key][node].is_object() {
                    p[key][node] = json!({});
                }
            }
            p["results"][node][function] = json!(decision);
            p["evaluations"][node][function] = evaluation.clone();
            p["cursor"] = json!(cursor + offset + 1);
            let index = trace.as_array().unwrap().len();
            let entry = json!({"index":index,"nodeId":node,"function":function,"task":task,"depth":task["depth"],"decision":decision,"startedAtMs":started,"elapsedMs":Context::get_time_millis()-started,"httpCallId":http_call,"questionKey":batch.question_key(offset),"requestHash":format!("{:x}",Sha256::digest(encoded.as_bytes())),"caseHash":format!("{:x}",Sha256::digest(individual.to_string().as_bytes())),"requestFormat":"fanout-case-v1","request":{"model":individual["model"],"questions":individual["questions"],"state_ref":{"nodeId":node,"worldId":snapshot["world"]["Id"],"context":context,"prerequisiteIds":state["prerequisites"].as_array().into_iter().flatten().map(|v|v["id"].clone()).collect::<Vec<_>>(),"prerequisiteAssessments":state["prerequisites"],"comparisonIds":state["comparisons"].as_array().into_iter().flatten().map(|v|v["Id"].clone()).collect::<Vec<_>>(),"assessment":state["assessment"],"evaluations":state["evaluations"]}},"response":response,"forecastProbability":evaluation["probability"]});
            trace_bytes += entry.to_string().len() + 1;
            trace.as_array_mut().ok_or("Missing trace")?.push(entry);
        }
    }
    set_success_result(
        "Recorded",
        &json!({"program_json":p.to_string(),"trace_json":trace.to_string()}),
    );
    Ok(())
}
#[unsafe(no_mangle)]
pub extern "C" fn run(_: i32, _: i32) -> i32 {
    match Context::from_host().and_then(|ctx| call(&ctx)) {
        Ok(()) => (),
        Err(e) => set_success_result("Fail", &json!({"error_message":e})),
    };
    0
}
