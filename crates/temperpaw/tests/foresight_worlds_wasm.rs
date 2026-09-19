//! Independent whole-world contract tests against actual WASM; provider responses are fixtures.
use serde_json::{Value, json};
use std::{
    collections::BTreeMap,
    path::PathBuf,
    sync::{Arc, Mutex, RwLock},
};
use temper_wasm::{
    SimWasmHost, StreamRegistry, WasmEngine, WasmHost, WasmInvocationContext, WasmResourceLimits,
};

fn bytes(module: &str) -> Vec<u8> {
    if module == "semantic_step" {
        if let Ok(path) = std::env::var("ARN518_STEP_WASM_OVERRIDE") {
            return std::fs::read(path).unwrap();
        }
    }
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
    invoke_with_host(
        engine,
        module,
        ctx,
        Arc::new(SimWasmHost::new().with_default_response(500, "unexpected provider IO")),
        hash,
    )
    .await
}
async fn invoke_with_host(
    engine: &WasmEngine,
    _module: &str,
    ctx: WasmInvocationContext,
    host: Arc<dyn WasmHost>,
    hash: String,
) -> Value {
    let r = engine
        .invoke(
            &hash,
            &ctx,
            host,
            &WasmResourceLimits {
                max_memory: 256 * 1024 * 1024,
                max_fuel: 10_000_000_000,
                ..Default::default()
            },
            Arc::new(RwLock::new(StreamRegistry::default())),
        )
        .await
        .unwrap();
    serde_json::to_value(r).unwrap()
}

#[derive(Default)]
struct WorldProvider {
    requests: Mutex<Vec<Value>>,
}
#[async_trait::async_trait]
impl WasmHost for WorldProvider {
    async fn http_call(
        &self,
        method: &str,
        url: &str,
        _headers: &[(String, String)],
        body: &str,
    ) -> Result<(u16, String), String> {
        assert_eq!(method, "POST");
        assert_eq!(url, "https://api.typesafe.ai/v1/systemone");
        let request: Value = serde_json::from_str(body).unwrap();
        self.requests.lock().unwrap().push(request.clone());
        let q = &request["questions"]["result"];
        let answer = if q["type"] == "noul" {
            json!({"type":"noul","noul":0.23})
        } else {
            let options = q["criteria"].as_object().unwrap();
            let probabilities: serde_json::Map<String, Value> = options
                .keys()
                .map(|k| (k.clone(), json!(if k == "none" { 1.0 } else { 0.0 })))
                .collect();
            json!({"type":"choice","choice":"none","probabilities":probabilities})
        };
        Ok((
            200,
            json!({"model":"jev-1.13.0","answers":{"result":answer}}).to_string(),
        ))
    }
    async fn http_call_binary(
        &self,
        _: &str,
        _: &str,
        _: &[(String, String)],
        _: &[u8],
    ) -> Result<(u16, Vec<u8>), String> {
        Err("Unexpected binary IO".into())
    }
    fn get_secret(&self, _: &str) -> Result<String, String> {
        Ok("fixture-only".into())
    }
    fn log(&self, _: &str, _: &str) {}
}
fn snapshot() -> Value {
    json!({"world":{"Id":"question","target_date":"2027-09-20","last_ingest_date":"2026-09-20","hindcast_mode":"false"},"nodes":[
      {"Id":"e","kind":"evidence","statement":"A clinic already uses an assistant for appointment reminders.","quote":"The assistant sends appointment reminders today.","observed_at":"2026-09-19","claim_type":"observed","source_refs":["https://example.org/clinic"],"edges":"[]"},
      {"Id":"h1","kind":"scenario","statement":"Clinics automate bookings by September 2027.","edges":"[]"},
      {"Id":"h2","kind":"revision","statement":"Patients accept assistant-run bookings by September 2027.","edges":"[]"},
      {"Id":"counter","kind":"scenario","statement":"Patients demand human approval by September 2027.","edges":"[]"},
      {"Id":"w1","kind":"world","statement":"By September 2027 clinics run bookings without reception staff and patients accept this change.","component_ids":["h1","h2"],"counter_ids":["counter"],"resolve_by":"2027-09-20","edges":"[{\"kind\":\"requires\",\"to_id\":\"h1\"},{\"kind\":\"requires\",\"to_id\":\"h2\"},{\"kind\":\"requires\",\"to_id\":\"e\"}]"},
      {"Id":"w2","kind":"world","statement":"By September 2027 clinics automate routine bookings while humans approve every change.","component_ids":["h1","h2"],"counter_ids":["counter"],"resolve_by":"2027-09-20","edges":"[]"}
    ]})
}
fn answer(snapshot: &Value) -> Value {
    let outcome = |index: usize| {
        let n = &snapshot["nodes"][index];
        json!({"id":n["Id"],"world_id":n["Id"],"title":"A different day at the clinic","definition":n["statement"],"component_ids":n["component_ids"],"counter_ids":n["counter_ids"],"scenario_ids":["h1","h2","e"],"probability":0.99,"narrative":"Bookings change who spends time on the phone.","scene":"A receptionist helps a worried patient while bookings arrive automatically.","what_you_can_do":["Ask a clinic how exceptions are handled."],"signals":["Fewer calls to reception"],"falsifiers":["Patients insist on calling"]})
    };
    json!({"schema":"foresight-worlds-v3","probability_model":"overlapping_worlds","probability_basis":"model_implied_world_estimate","calibrated":false,"evaluation_status":"evaluated","evaluation_note":"","headline":"Two possible clinic mornings","summary":"Both worlds build on reminders that already exist.","horizon":"2027-09-20","baseline":{"as_of":"2026-09-20","observed":[{"claim":"Appointment reminders already run automatically.","evidence_ids":["e"]}],"assumptions":[],"unknowns":["Will patients accept unsupervised changes?"]},"evidence_limits":["One observed clinic; model estimates are uncalibrated."],"research_questions":[],"outcomes":[outcome(4),outcome(5)]})
}
#[tokio::test]
async fn fresh_world_reply_is_not_a_component_probability() {
    let engine = WasmEngine::new().unwrap();
    let snapshot = snapshot();
    let host = Arc::new(WorldProvider::default());
    let program = json!({"baseline":answer(&snapshot)["baseline"],"cursor":0,"tasks":[{"nodeId":"w1","function":"estimate_likelihood","depth":1}],"results":{"h1":{"estimate_likelihood":"0.9"},"h2":{"estimate_likelihood":"0.8"}},"evaluations":{}});
    let hash = engine.compile_and_cache(&bytes("semantic_call")).unwrap();
    let ctx = WasmInvocationContext {
        tenant: "test".into(),
        entity_type: "SemanticRun".into(),
        entity_id: "world-test".into(),
        trigger_action: "Evaluate".into(),
        wasm_module: Some("semantic_call".into()),
        trigger_params: json!({}),
        entity_state: json!({"fields":{"snapshot_json":snapshot.to_string(),"program_json":program.to_string(),"trace_json":"[]"}}),
        agent_id: None,
        session_id: None,
        integration_config: BTreeMap::from([("typesafe_api_key".into(), "fixture-only".into())]),
        trace_id: String::new(),
        workflow_root_entity_type: None,
        workflow_root_entity_id: None,
        workflow_run_id: None,
        http_request: None,
    };
    let recorded = invoke_with_host(&engine, "semantic_call", ctx, host.clone(), hash).await;
    assert_eq!(recorded["callback_action"], "Recorded", "{recorded}");
    let program: Value = serde_json::from_str(
        recorded["callback_params"]["program_json"]
            .as_str()
            .unwrap(),
    )
    .unwrap();
    assert_eq!(program["results"]["w1"]["estimate_likelihood"], "0.23");
    let requests = host.requests.lock().unwrap();
    assert_eq!(requests.len(), 1);
    let state = &requests[0]["state"];
    assert_eq!(
        state["node"]["statement"],
        snapshot["nodes"][4]["statement"]
    );
    let encoded = state.to_string();
    for exact in [
        "The assistant sends appointment reminders today.",
        "2026-09-19",
        "Patients demand human approval by September 2027.",
    ] {
        assert!(
            encoded.contains(exact),
            "Missing actual evidence/counter in provider payload: {exact}"
        );
    }
    drop(requests);
    let fields = json!({"phase":"synthesize","snapshot_json":snapshot.to_string(),"program_json":program.to_string(),"reasoning_result":answer(&snapshot).to_string()});
    let completed = invoke(&engine, "semantic_expand", fields).await;
    assert_eq!(completed["callback_action"], "Complete", "{completed}");
    let final_answer: Value =
        serde_json::from_str(completed["callback_params"]["answer"].as_str().unwrap()).unwrap();
    assert_eq!(final_answer["outcomes"][0]["probability"], 0.23);
    assert!(
        final_answer["outcomes"][1]["probability"].is_null(),
        "No score for w2; component estimates must not leak into world odds"
    );
    assert_eq!(final_answer["evaluation_status"], "partial");
    assert_eq!(final_answer["baseline"], answer(&snapshot)["baseline"]);
}
#[tokio::test]
async fn missing_world_scores_stay_null_and_baseline_cannot_cite_a_future() {
    let engine = WasmEngine::new().unwrap();
    let snapshot = snapshot();
    let mut draft = answer(&snapshot);
    let mut fields = json!({"phase":"synthesize","snapshot_json":snapshot.to_string(),"program_json":json!({"baseline":answer(&snapshot)["baseline"],"results":{"h1":{"estimate_likelihood":"0.9"},"h2":{"estimate_likelihood":"0.8"}}}).to_string(),"reasoning_result":draft.to_string()});
    let completed = invoke(&engine, "semantic_expand", fields.clone()).await;
    assert_eq!(completed["callback_action"], "Complete", "{completed}");
    let final_answer: Value =
        serde_json::from_str(completed["callback_params"]["answer"].as_str().unwrap()).unwrap();
    assert_eq!(final_answer["evaluation_status"], "unavailable");
    assert!(
        final_answer["outcomes"]
            .as_array()
            .unwrap()
            .iter()
            .all(|o| o["probability"].is_null())
    );
    draft["baseline"]["observed"][0]["evidence_ids"] = json!(["h1"]);
    fields["reasoning_result"] = json!(draft.to_string());
    let mut invalid_program: Value =
        serde_json::from_str(fields["program_json"].as_str().unwrap()).unwrap();
    invalid_program["baseline"] = draft["baseline"].clone();
    fields["program_json"] = json!(invalid_program.to_string());
    assert_eq!(
        invoke(&engine, "semantic_expand", fields).await["callback_action"],
        "Fail"
    );
}

#[tokio::test]
async fn composition_schedules_new_world_calls_not_component_reuse() {
    let engine = WasmEngine::new().unwrap();
    let full = snapshot();
    let mut snapshot = full.clone();
    snapshot["nodes"].as_array_mut().unwrap().truncate(4);
    let worlds: Vec<Value> = [4usize, 5]
        .iter()
        .map(|&i| {
            let mut w = full["nodes"][i].clone();
            w["id"] = w["Id"].clone();
            w["title"] = json!("A new clinic morning");
            w["mechanism"] =
                json!("Changing both bookings and patient consent changes the staffing model.");
            w["scene"] = json!("The clinic opens with no booking calls waiting.");
            w["narrative"] =
                json!("Staff spend their morning helping patients instead of arranging times.");
            w["signals"] = json!(["Reception call volume falls"]);
            w["falsifiers"] = json!(["Manual booking remains necessary"]);
            w["what_you_can_do"] = json!(["Observe how the clinic handles mistakes"]);
            w
        })
        .collect();
    let old = json!({"cursor":0,"tasks":[],"results":{"h1":{"estimate_likelihood":"0.9"},"h2":{"estimate_likelihood":"0.8"}},"evaluations":{}});
    let mut generated = json!({"baseline":answer(&full)["baseline"],"worlds":worlds});
    let mut fields = json!({"phase":"compose","snapshot_json":snapshot.to_string(),"program_json":old.to_string(),"reasoning_result":generated.to_string()});
    let expanded = invoke(&engine, "semantic_expand", fields.clone()).await;
    assert_eq!(expanded["callback_action"], "Expanded", "{expanded}");
    let p: Value = serde_json::from_str(
        expanded["callback_params"]["program_json"]
            .as_str()
            .unwrap(),
    )
    .unwrap();
    assert_eq!(p["stage"], "worlds");
    let tasks = p["tasks"].as_array().unwrap();
    for id in ["world-w1", "world-w2"] {
        assert!(
            tasks
                .iter()
                .any(|t| t["nodeId"] == id && t["function"] == "estimate_likelihood")
        );
        assert!(
            tasks
                .iter()
                .any(|t| t["nodeId"] == id && t["function"] == "classify_gap")
        );
        assert!(p["results"][id]["estimate_likelihood"].is_null());
    }
    assert!(
        tasks
            .iter()
            .all(|t| t["nodeId"].as_str().unwrap().starts_with("world-")),
        "Components are already evaluated"
    );
    generated["baseline"]["observed"][0]["evidence_ids"] = json!(["h1"]);
    fields["reasoning_result"] = json!(generated.to_string());
    assert_eq!(
        invoke(&engine, "semantic_expand", fields).await["callback_action"],
        "Fail"
    );
}
