//! A large reasoning input must not make polling its small result fail.
use serde_json::{Value, json};
use std::{
    collections::BTreeMap,
    sync::{Arc, RwLock},
};
use temper_wasm::{
    SimWasmHost, StreamRegistry, WasmEngine, WasmInvocationContext, WasmResourceLimits,
};
#[tokio::test]
async fn polling_projects_result_without_echoing_three_megabyte_prompt() {
    let root = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
    let output=std::process::Command::new("bash").current_dir(&root).args(["-c","source os-apps/wasm-build-env.sh; temperpaw_build_wasm os-apps/paw-foresight/wasm/semantic_session wasm32-unknown-unknown --locked"]).output().unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let artifact = std::env::var("ARN518_SESSION_WASM_OVERRIDE")
        .unwrap_or_else(|_| String::from_utf8(output.stdout).unwrap().trim().into());
    let engine = WasmEngine::new().unwrap();
    let hash = engine
        .compile_and_cache(&std::fs::read(artifact).unwrap())
        .unwrap();
    let source = json!({"Status":"Completed","result":"{\"hypotheses\":[]}","error_message":"","user_message":"x".repeat(3*1024*1024),"conversation":"private-unneeded-conversation"});
    let selected = json!({"Status":source["Status"],"result":source["result"],"error_message":source["error_message"]});
    let host = SimWasmHost::new()
        .with_default_response(500, "unexpected route")
        .with_response(
            "http://fixture/tdata/Sessions('child')",
            200,
            &source.to_string(),
        )
        .with_response(
            "http://fixture/tdata/Sessions('child')?$select=Status,result,error_message",
            200,
            &selected.to_string(),
        );
    let ctx = WasmInvocationContext {
        tenant: "test".into(),
        entity_type: "SemanticRun".into(),
        entity_id: "run".into(),
        trigger_action: "CheckReasoning".into(),
        wasm_module: Some("semantic_session".into()),
        trigger_params: json!({}),
        entity_state: json!({"fields":{"reasoning_session_id":"child"}}),
        agent_id: None,
        session_id: None,
        integration_config: BTreeMap::from([("temper_api_url".into(), "http://fixture".into())]),
        trace_id: String::new(),
        workflow_root_entity_type: None,
        workflow_root_entity_id: None,
        workflow_run_id: None,
        http_request: None,
    };
    let result: Value = serde_json::to_value(
        engine
            .invoke(
                &hash,
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
            .unwrap(),
    )
    .unwrap();
    assert_eq!(result["callback_action"], "ReasoningComplete", "{result}");
    assert_eq!(
        result["callback_params"]["reasoning_result"],
        source["result"]
    );
}
