use sha2::{Digest, Sha256};
use temper_wasm_sdk::prelude::*;
mod core {
    include!("../../semantic_core.rs");
}
fn call(ctx: &Context) -> Result<(), String> {
    let mut p = core::parse(core::field(&ctx.entity_state, "program_json"))?;
    let mut trace = core::parse(core::field(&ctx.entity_state, "trace_json"))?;
    let request = core::parse(core::field(&ctx.entity_state, "request_json"))?;
    let cursor = p["cursor"].as_u64().ok_or("Missing cursor")? as usize;
    if trace.as_array().ok_or("Missing trace")?.len() >= core::MAX_CALLS {
        return Err("Provider call budget exhausted".into());
    }
    let task = p["tasks"][cursor].clone();
    let node = task["nodeId"].as_str().ok_or("Missing node identity")?;
    let function = task["function"].as_str().ok_or("Missing function")?;
    let key = ctx
        .config
        .get("typesafe_api_key")
        .filter(|s| !s.is_empty() && !s.contains("{secret:"))
        .ok_or("Configure foresight_typesafe_api_key in Temper settings")?;
    let encoded = request.to_string();
    let started = Context::get_time_millis();
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
    if r.body.len() > 32 * 1024 {
        return Err("Provider response exceeds bound".into());
    }
    let response = core::parse(&r.body)?;
    let decision = core::validate(&request, &response)?;
    if p["results"].get(node).is_none() {
        p["results"][node] = json!({});
    }
    p["results"][node][function] = json!(decision);
    p["cursor"] = json!(cursor + 1);
    let index = trace.as_array().unwrap().len();
    trace.as_array_mut().ok_or("Missing trace")?.push(json!({"index":index,"nodeId":node,"function":function,"depth":task["depth"],"decision":decision,"startedAtMs":started,"elapsedMs":Context::get_time_millis()-started,"requestHash":format!("{:x}",Sha256::digest(encoded.as_bytes())),"request":request,"response":response,"forecastProbability":null}));
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
