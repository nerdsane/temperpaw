//! Native research/deepening boundaries, without any real provider calls.
use serde_json::{Value, json};
use std::{
    collections::BTreeMap,
    path::PathBuf,
    sync::{Arc, RwLock},
};
use temper_wasm::{
    SimWasmHost, StreamRegistry, WasmEngine, WasmInvocationContext, WasmResourceLimits,
};

fn bytes(module: &str) -> Vec<u8> {
    if module == "semantic_expand" {
        if let Ok(path) = std::env::var("ARN518_OUTLOOK_EXPAND_WASM_OVERRIDE") {
            return std::fs::read(path).unwrap();
        }
    }
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
    let output=std::process::Command::new("bash").current_dir(&root).args(["-c",&format!("set -euo pipefail; source os-apps/wasm-build-env.sh; temperpaw_build_wasm os-apps/paw-foresight/wasm/{module} wasm32-unknown-unknown --locked")]).output().expect("build semantic WASM");
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    std::fs::read(String::from_utf8(output.stdout).unwrap().trim()).unwrap()
}
async fn invoke(engine: &WasmEngine, module: &str, fields: Value) -> Value {
    let hash = engine.compile_and_cache(&bytes(module)).unwrap();
    let ctx = WasmInvocationContext {
        tenant: "test".into(),
        entity_type: "SemanticRun".into(),
        entity_id: "run-fixture".into(),
        trigger_action: "Next".into(),
        wasm_module: Some(module.into()),
        trigger_params: json!({}),
        entity_state: json!({"fields":fields}),
        agent_id: None,
        session_id: None,
        integration_config: BTreeMap::new(),
        trace_id: String::new(),
        workflow_root_entity_type: None,
        workflow_root_entity_id: None,
        workflow_run_id: None,
        http_request: None,
    };
    let r = engine
        .invoke(
            &hash,
            &ctx,
            Arc::new(SimWasmHost::new().with_default_response(500, "unexpected provider IO")),
            &WasmResourceLimits::default(),
            Arc::new(RwLock::new(StreamRegistry::default())),
        )
        .await
        .unwrap();
    serde_json::to_value(r).unwrap()
}
#[tokio::test]
async fn research_recommendation_deepens_and_gets_a_fresh_bounded_recheck_window() {
    let engine = WasmEngine::new().unwrap();
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_millis() as u64;
    let snapshot = json!({"world":{"target_date":"2027-09-19","hindcast_mode":"false"},"nodes":[{"Id":"e","edges":"[]"},{"Id":"s","kind":"scenario","edges":"[{\"kind\":\"requires\",\"to_id\":\"e\"}]"}]});
    let program = json!({"cursor":0,"tasks":[],"results":{"s":{"classify_gap":"evidence","choose_next_operation":"research"},"e":{"classify_gap":"uncertain","choose_next_operation":"research"}},"issues":[]});
    let mut fields = json!({"phase":"seed","snapshot_json":snapshot.to_string(),"program_json":program.to_string(),"trace_json":"[]","started_at_ms":now.to_string(),"reasoning_session_id":"research-session"});
    let step = invoke(&engine, "semantic_step", fields.clone()).await;
    assert_eq!(step["callback_action"], "Reason", "{step}");
    assert_eq!(step["callback_params"]["phase"], "deepen");
    fields["phase"] = json!("deepen");
    fields["program_json"] = step["callback_params"]["program_json"].clone();
    let reasoning = invoke(&engine, "semantic_reasoning", fields.clone()).await;
    assert_eq!(
        reasoning["callback_action"], "LaunchReasoning",
        "{reasoning}"
    );
    assert_eq!(
        reasoning["callback_params"]["tools_enabled"],
        "temper_web_search,temper_web_fetch"
    );
    assert_eq!(reasoning["callback_params"]["max_turns"], "8");
    assert_eq!(reasoning["callback_params"]["tool_choice"], "auto");
    fields["started_at_ms"] = json!((now - 3_600_000).to_string());
    fields["reasoning_result"]=json!(json!({"research_evidence":[],"revisions":[{"id":"r1","parent":"s","statement":"A revised hypothetical mechanism","requires":["e"],"research_question":"What adoption evidence is still missing?"}]}).to_string());
    let expanded = invoke(&engine, "semantic_expand", fields.clone()).await;
    assert_eq!(expanded["callback_action"], "Expanded", "{expanded}");
    let saved: Value = serde_json::from_str(
        expanded["callback_params"]["snapshot_json"]
            .as_str()
            .unwrap(),
    )
    .unwrap();
    assert_eq!(saved["nodes"][1], snapshot["nodes"][1]);
    assert_eq!(saved["nodes"][2]["before_gap"], "evidence");
    assert_eq!(saved["nodes"][2]["research_status"], "unresolved");
    assert_eq!(saved["nodes"][2]["source_session_id"], "research-session");
    fields["snapshot_json"] = expanded["callback_params"]["snapshot_json"].clone();
    fields["program_json"] = expanded["callback_params"]["program_json"].clone();
    let recheck = invoke(&engine, "semantic_step", fields).await;
    assert_eq!(
        recheck["callback_action"], "Evaluate",
        "research wait must not consume the fresh follow-up window: {recheck}"
    );
    let request: Value =
        serde_json::from_str(recheck["callback_params"]["request_json"].as_str().unwrap()).unwrap();
    assert_eq!(request["state"]["node"]["Id"], "r1");
    assert_eq!(
        request["state"]["prerequisites"][0]["assessment"]["classify_gap"],
        "uncertain"
    );
}

#[tokio::test]
async fn synthesis_rejects_invalid_probability_mass_at_the_wasm_boundary() {
    let engine = WasmEngine::new().unwrap();
    let snapshot = json!({"world":{"target_date":"2027-09-19"},"nodes":[{"Id":"s1","kind":"scenario"},{"Id":"s2","kind":"scenario"}]});
    let bucket = |id: &str, scenario: &str, p: f64| json!({"id":id,"title":"A future","definition":"A mutually exclusive measurable bucket","probability":p,"scenario_ids":if scenario.is_empty(){vec![]}else{vec![scenario]},"narrative":"An explicitly hypothetical future","signals":["A dated observable signal"],"falsifiers":["An observable disconfirmation"]});
    let answer = json!({"schema":"foresight-outlook-v1","headline":"A clear outlook","summary":"Subjective judgments under uncertainty","horizon":"2027-09-19","probability_basis":"subjective_model_estimate","calibrated":false,"evidence_limits":["Not calibrated; incomplete evidence"],"research_questions":[],"outcomes":[bucket("a","s1",0.5),bucket("b","s2",0.35),bucket("other","",0.15)]});
    let mut fields = json!({"phase":"synthesize","snapshot_json":snapshot.to_string(),"reasoning_result":answer.to_string()});
    let good = invoke(&engine, "semantic_expand", fields.clone()).await;
    assert_eq!(good["callback_action"], "Complete", "{good}");
    let mut malformed = answer;
    malformed["outcomes"][0]["probability"] = json!(0.99);
    fields["reasoning_result"] = json!(malformed.to_string());
    let bad = invoke(&engine, "semantic_expand", fields).await;
    assert_eq!(
        bad["callback_action"], "Fail",
        "must not publish unnormalized probabilities: {bad}"
    );
}
