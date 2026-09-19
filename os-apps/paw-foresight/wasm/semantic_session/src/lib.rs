use temper_wasm_sdk::prelude::*;
mod core {
    include!("../../semantic_core.rs");
}
fn check(ctx: &Context) -> Result<(), String> {
    let id = core::field(&ctx.entity_state, "reasoning_session_id");
    if id.is_empty()
        || !id
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b"-_".contains(&b))
    {
        return Err("Missing reasoning session".into());
    }
    let api = ctx
        .config
        .get("temper_api_url")
        .filter(|url| !url.is_empty() && !url.contains("{secret:"))
        .ok_or("Missing Temper URL")?;
    let r = ctx.http_call(
        "GET",
        &format!("{api}/tdata/Sessions('{id}')?$select=Status,result,error_message,error"),
        &[
            ("x-tenant-id".into(), ctx.tenant.clone()),
            ("x-temper-principal-kind".into(), "agent".into()),
            ("x-temper-principal-id".into(), ctx.entity_id.clone()),
            ("x-temper-agent-type".into(), "system".into()),
        ],
        "",
    )?;
    if r.status != 200 {
        return Err(format!("Reasoning read HTTP {}", r.status));
    }
    if r.body.len() > 2_000_000 {
        return Err("Reasoning response too large".into());
    }
    let s = core::parse(&r.body)?;
    match core::field(&s, "Status") {
        "Completed" => {
            let result = core::field(&s, "result");
            if result.trim().is_empty() {
                return Err("Reasoning completed without an answer".into());
            }
            set_success_result("ReasoningComplete", &json!({"reasoning_result":result}));
        }
        "Failed" | "Cancelled" => {
            let message = core::field(&s, "error_message");
            let error = if message.trim().is_empty() {
                core::field(&s, "error")
            } else {
                message
            };
            return Err(format!("Reasoning session {id} did not complete: {error}"));
        }
        _ => {
            let count = ctx
                .entity_state
                .get("counters")
                .and_then(|v| v.get("check_count"))
                .or_else(|| {
                    ctx.entity_state
                        .get("fields")
                        .and_then(|v| v.get("check_count"))
                })
                .and_then(Value::as_u64)
                .unwrap_or(0);
            if count >= 360 {
                return Err("Reasoning check budget exhausted".into());
            }
            set_success_result("ReasoningPending", &json!({}));
        }
    };
    Ok(())
}
#[unsafe(no_mangle)]
pub extern "C" fn run(_: i32, _: i32) -> i32 {
    match Context::from_host().and_then(|ctx| check(&ctx)) {
        Ok(()) => (),
        Err(e) => set_success_result("Fail", &json!({"error_message":e})),
    };
    0
}
