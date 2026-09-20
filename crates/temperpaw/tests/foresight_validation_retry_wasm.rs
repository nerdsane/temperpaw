//! Actual guest proof: malformed typed replies never become evaluations.
use serde_json::{Value, json};
use std::{
    collections::BTreeMap,
    sync::{Arc, RwLock},
};
use temper_wasm::{
    SimWasmHost, StreamRegistry, WasmEngine, WasmInvocationContext, WasmResourceLimits,
};
async fn invoke(
    engine: &WasmEngine,
    hash: &str,
    fields: Value,
    status: u16,
    response: &Value,
) -> Value {
    let host = SimWasmHost::new().with_default_response(status, &response.to_string());
    let ctx = WasmInvocationContext {
        tenant: "test".into(),
        entity_type: "SemanticRun".into(),
        entity_id: "retry-fixture".into(),
        trigger_action: "Evaluate".into(),
        wasm_module: Some("semantic_call".into()),
        trigger_params: json!({}),
        entity_state: json!({"fields":fields}),
        agent_id: None,
        session_id: None,
        integration_config: BTreeMap::from([("typesafe_api_key".into(), "fixture-key".into())]),
        trace_id: String::new(),
        workflow_root_entity_type: None,
        workflow_root_entity_id: None,
        workflow_run_id: None,
        http_request: None,
    };
    let r = engine
        .invoke(
            hash,
            &ctx,
            Arc::new(host),
            &WasmResourceLimits {
                max_memory: 256 * 1024 * 1024,
                max_fuel: 10_000_000_000,
                ..Default::default()
            },
            Arc::new(RwLock::new(StreamRegistry::default())),
        )
        .await
        .unwrap();
    assert_eq!(r.callback_action, "Recorded", "{:?}", r.callback_params);
    r.callback_params
}
fn fixture() -> Value {
    json!({"snapshot_json":json!({"world":{"Id":"w"},"nodes":[{"Id":"h","kind":"scenario","statement":"A dated event happens in 2027","edges":"[]"}]}).to_string(),"program_json":json!({"cursor":0,"tasks":[{"nodeId":"h","function":"classify_gap"}],"results":{},"evaluations":{}}).to_string(),"trace_json":"[]"})
}
fn response(valid: bool) -> Value {
    json!({"model":"jev-1.13.0","answers":{"result":{"type":"choice","choice":if valid {"evidence"} else {"timing"},"probabilities":{"evidence":0.8,"timing":0.1,"none":0.1,"prerequisite":0.0,"uncertain":0.0}}}})
}
#[tokio::test]
async fn inconsistent_choice_retries_then_only_valid_answer_advances() {
    let engine = WasmEngine::new().unwrap();
    let path=std::env::var("ARN518_VALIDATION_WASM").unwrap_or_else(|_|format!("{}/../../os-apps/paw-foresight/wasm/semantic_call/target/wasm32-unknown-unknown/release/semantic_call.wasm",env!("CARGO_MANIFEST_DIR")));
    let hash = engine
        .compile_and_cache(&std::fs::read(path).unwrap())
        .unwrap();
    let initial = fixture();
    let first = invoke(&engine, &hash, initial.clone(), 200, &response(false)).await;
    let p: Value = serde_json::from_str(first["program_json"].as_str().unwrap()).unwrap();
    assert_eq!(p["stop_reason"], "validation_retry");
    assert_eq!(p["cursor"], 0);
    assert_eq!(p["evaluations"], json!({}));
    let t: Value = serde_json::from_str(first["trace_json"].as_str().unwrap()).unwrap();
    assert_eq!(t[0]["rejectedResponse"], response(false));
    assert!(
        t[0]["error"]
            .as_str()
            .unwrap()
            .contains("selected_probability=0.1")
    );
    let mut fields = initial.clone();
    fields["program_json"] = first["program_json"].clone();
    fields["trace_json"] = first["trace_json"].clone();
    let success = invoke(&engine, &hash, fields.clone(), 200, &response(true)).await;
    let p: Value = serde_json::from_str(success["program_json"].as_str().unwrap()).unwrap();
    assert_eq!(p["cursor"], 1);
    assert_eq!(p["validation_failures"], 0);
    assert_eq!(p["results"]["h"]["classify_gap"], "evidence");
    let t: Value = serde_json::from_str(success["trace_json"].as_str().unwrap()).unwrap();
    assert_eq!(t.as_array().unwrap().len(), 2);
    assert_eq!(t[0]["requestHash"], t[1]["requestHash"]);
    for attempt in 2..=3 {
        let next = invoke(&engine, &hash, fields.clone(), 200, &response(false)).await;
        let p: Value = serde_json::from_str(next["program_json"].as_str().unwrap()).unwrap();
        assert_eq!(p["cursor"], 0);
        assert_eq!(p["evaluations"], json!({}));
        assert_eq!(
            p["stop_reason"],
            if attempt == 2 {
                "validation_retry"
            } else {
                "provider_error"
            }
        );
        fields["program_json"] = next["program_json"].clone();
        fields["trace_json"] = next["trace_json"].clone();
    }
    let denied = invoke(&engine, &hash, initial, 403, &json!({"message":"denied"})).await;
    let p: Value = serde_json::from_str(denied["program_json"].as_str().unwrap()).unwrap();
    assert_eq!(p["stop_reason"], "provider_error");
    assert!(p["validation_failures"].is_null());
}
