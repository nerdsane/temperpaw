//! Controlled provider evidence, live Jev, packaged WASM, production IOA evaluator.
//! Run with TYPESAFE_API_KEY_FILE pointing to a private file outside the checkout.
use axum::{
    Json, Router,
    extract::State,
    http::{HeaderMap, StatusCode},
    response::Html,
    routing::{get, post},
};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::{
    collections::BTreeMap,
    sync::{Arc, Mutex, RwLock},
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};
use temper_jit::table::TransitionTable;
use temper_runtime::scheduler::{FaultConfig, SimActorSystem, SimActorSystemConfig};
use temper_server::entity_actor::sim_handler::EntityActorHandler;
use temper_wasm::{
    ProductionWasmHost, StreamRegistry, WasmEngine, WasmInvocationContext, WasmResourceLimits,
};

const IOA: &str = include_str!("../../../os-apps/dsf-twin/specs/railway_service_instance.ioa.toml");
const HTML: &str = include_str!("jev_twin/index.html");
const SHA: &str = "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";
const ID: &str = "demo-dsf-backend";
const ORIGIN: &str = "https://demo.deep-sci-fi.invalid";
const MODEL_URL: &str = "https://api.typesafe.ai/v1/systemone";
const MODULE: &str = "dsf_railway_deploy_verify";

struct App {
    engine: WasmEngine,
    module_hash: String,
    key: String,
    origin: String,
    busy: tokio::sync::Mutex<()>,
}
fn step(
    sim: &mut SimActorSystem,
    history: &mut Vec<Value>,
    action: &str,
    params: Value,
) -> Result<Value, String> {
    let state = sim.step(ID, action, &params.to_string())?;
    history.push(json!({"action":action,"status":sim.status(ID)}));
    Ok(state)
}
fn events(scenario: &str, now: i64, request_id: &str) -> Result<Vec<Value>, String> {
    let evidence: Vec<(&str, &str, &str)> = match scenario {
        "recovery" => vec![
            (
                "startup.connect_database",
                "error",
                "Database connection refused during startup; retry scheduled.",
            ),
            (
                "startup.connect_database",
                "ok",
                "Retry connected to database; migrations complete.",
            ),
            (
                "POST /api/stories",
                "ok",
                "Story Aurora persisted successfully with id story-42.",
            ),
            (
                "GET /api/stories/story-42",
                "ok",
                "Readback returned the complete saved story Aurora with expected text and world id.",
            ),
        ],
        "regression" => vec![
            (
                "POST /api/stories",
                "ok",
                "Request accepted with HTTP 202; creation queued.",
            ),
            (
                "worker.persist_story",
                "error",
                "Database write failed: column story_body does not exist. Transaction rolled back; no story saved.",
            ),
            (
                "GET /api/stories/story-42",
                "error",
                "HTTP 404: requested saved story does not exist; user sees an empty result.",
            ),
        ],
        "missing" => vec![(
            "POST /api/stories",
            "ok",
            "Request accepted with HTTP 202. Worker and readback telemetry unavailable; completion unknown.",
        )],
        _ => return Err("unknown scenario".into()),
    };
    let mut spans = Vec::new();
    for (index, (operation, status, outcome)) in evidence.iter().enumerate() {
        let at = now - 50_000 + index as i64 * 10_000;
        // RFC3339 via time crate, already used by the server.
        let start = time::OffsetDateTime::from_unix_timestamp_nanos(at as i128 * 1_000_000)
            .map_err(|e| e.to_string())?
            .format(&time::format_description::well_known::Rfc3339)
            .map_err(|e| e.to_string())?;
        spans.push(
            json!({"attributes":{"service":"deep-sci-fi-backend","env":"demo","start":start,
            "status":status,"resource_name":operation,"trace_id":format!("demo-{index}"),
            "custom":{"git":{"commit":{"sha":SHA}},"dsf":{"outcome":outcome}}}}),
        );
    }
    let start = time::OffsetDateTime::from_unix_timestamp_nanos((now - 1000) as i128 * 1_000_000)
        .map_err(|e| e.to_string())?
        .format(&time::format_description::well_known::Rfc3339)
        .map_err(|e| e.to_string())?;
    spans.push(json!({"attributes":{"service":"deep-sci-fi-backend","env":"demo","start":start,
        "status":"ok","resource_name":"GET /api/health","trace_id":"demo-health",
        "custom":{"git":{"commit":{"sha":SHA}},"dsf":{"request_id":request_id,"outcome":"Health endpoint healthy at requested revision."},
        "http":{"url":format!("{ORIGIN}/api/health"),"status_code":200}}}}));
    Ok(spans)
}

async fn run(app: Arc<App>, scenario: String) -> Result<Value, String> {
    let _guard = app.busy.lock().await;
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(|e| e.to_string())?
        .as_millis() as i64;
    let operation = format!("demo-{now}");
    let request_id = format!("dsf-{:x}", Sha256::digest(format!("{ID}:{operation}:1")));
    let spans = events(&scenario, now, &request_id)?;
    let config = json!({"version":3,"resource_id":ID,
        "target":{"project_id":"demo-project","service_id":"demo-service","environment_id":"demo-env","token_secret":"fixture-railway"},
        "verification":{"application":{"kind":"railway","resource_id":ID,"origin":ORIGIN},
        "flow":{"kind":"provider_configuration"},
        "datadog":{"site":"datadoghq.com","service":"deep-sci-fi-backend","environment":"demo","api_key_secret":"fixture-dd","app_key_secret":"fixture-dd-app"},
        "semantic":{"api_key_secret":"dsf_typesafe_api_key","outcome":"A user can create a Deep Sci-Fi story and retrieve the complete saved story afterward."}}}).to_string();
    let digest = format!("{:x}", Sha256::digest(&config));
    let mut sim = SimActorSystem::new(SimActorSystemConfig {
        seed: 1,
        faults: FaultConfig::none(),
        ..Default::default()
    });
    sim.register_actor(
        ID,
        Box::new(
            EntityActorHandler::new(
                "DsfRailwayServiceInstance",
                ID,
                Arc::new(TransitionTable::from_ioa_source(IOA)),
            )
            .with_ioa_invariants(IOA),
        ),
    );
    let mut history = Vec::new();
    step(
        &mut sim,
        &mut history,
        "Register",
        json!({"project_id":"demo-project","service_id":"demo-service","environment_id":"demo-env","config_ref":"demo-config","config_sha256":digest,"intended_configuration":"demo"}),
    )?;
    step(
        &mut sim,
        &mut history,
        "Deploy",
        json!({"operation_key":operation,"expected_operation_sequence":0,"effort_id":"controlled-demo","request_revision":SHA,
        "request_configuration":json!({"baseline_deployment_id":"baseline","not_before_ms":now-60_000}).to_string(),"proof_ref":"controlled-proof"}),
    )?;
    step(
        &mut sim,
        &mut history,
        "DeployValidationSucceeded",
        json!({"operation_key":operation,"expected_operation_sequence":1,"validation_evidence_ref":"fixture:validation","intended_revision":SHA}),
    )?;
    step(&mut sim, &mut history, "DeployExecute", json!({}))?;
    step(
        &mut sim,
        &mut history,
        "DeployExecutionSucceeded",
        json!({"operation_key":operation,"expected_operation_sequence":1,"provider_execution_id":"demo-deployment","provider_evidence_ref":"fixture:railway"}),
    )?;
    let state = step(&mut sim, &mut history, "DeployVerify", json!({}))?;
    // Guest expects the same flattened entity fields that production dispatch supplies.
    let mut guest_state = state["fields"].clone();
    guest_state["status"] = state["status"].clone();
    for (key, value) in state["counters"].as_object().ok_or("missing counters")? {
        guest_state[key] = value.clone();
    }
    let captures = Arc::new(Mutex::new(Vec::<Value>::new()));
    let recorded = captures.clone();
    let fixtures = spans.clone();
    let secrets = BTreeMap::from([
        ("dsf_typesafe_api_key".into(), app.key.clone()),
        ("fixture-railway".into(), "controlled".into()),
        ("fixture-dd".into(), "controlled".into()),
        ("fixture-dd-app".into(), "controlled".into()),
    ]);
    let host = ProductionWasmHost::with_timeout(secrets, Duration::from_secs(30)).with_text_http_interceptor(Arc::new(move |method,url,_headers,body| {
        let state=guest_state.clone(); let config=config.clone(); let spans=fixtures.clone(); let captures=recorded.clone();
        Box::pin(async move {
            if url == MODEL_URL && method == "POST" {
                if let Ok(value) = serde_json::from_str::<Value>(&body) { captures.lock().unwrap().push(value); }
                return None; // The only real outbound request: TypeSafe inference.
            }
            let response = if url == format!("https://temper.demo.invalid/tdata/DsfRailwayServiceInstances('{ID}')") { state }
            else if url == "https://temper.demo.invalid/tdata/Files('demo-config')/$value" { return Some(Ok((200,config))); }
            else if url == "https://backboard.railway.com/graphql/v2" && method == "POST" {
                if body.contains("mutation") { return Some(Err("demo must never write to a provider".into())); }
                json!({"data":{
                    "deployment":{"id":"demo-deployment","status":"SUCCESS","projectId":"demo-project","serviceId":"demo-service","environmentId":"demo-env","meta":{"commitHash":SHA}},
                    "service":{"id":"demo-service","projectId":"demo-project"},
                    "serviceInstance":{"serviceId":"demo-service","environmentId":"demo-env","activeDeployments":[{"id":"demo-deployment"}],
                    "domains":{"customDomains":[{"domain":"demo.deep-sci-fi.invalid","projectId":"demo-project","serviceId":"demo-service","environmentId":"demo-env","deletedAt":null}],"serviceDomains":[]}}
                }})
            } else if url == format!("{ORIGIN}/api/health") { json!({"status":"healthy","git_sha":SHA}) }
            else if url == "https://api.datadoghq.com/api/v2/spans/events/search" { json!({"data":spans}) }
            else { return Some(Err("unexpected outbound request refused by controlled demo".into())); };
            Some(Ok((200,response.to_string())))
        })
    }));
    let context = WasmInvocationContext {
        tenant: "demo".into(),
        entity_type: "DsfRailwayServiceInstance".into(),
        entity_id: ID.into(),
        trigger_action: "DeployVerify".into(),
        wasm_module: Some(MODULE.into()),
        trigger_params: json!({}),
        entity_state: {
            let mut s = state["fields"].clone();
            s["status"] = state["status"].clone();
            for (k, v) in state["counters"].as_object().ok_or("missing counters")? {
                s[k] = v.clone();
            }
            s
        },
        agent_id: None,
        session_id: None,
        integration_config: BTreeMap::from([(
            "temper_api_url".into(),
            "https://temper.demo.invalid".into(),
        )]),
        trace_id: operation.clone(),
        workflow_root_entity_type: None,
        workflow_root_entity_id: None,
        workflow_run_id: None,
        http_request: None,
    };
    let started = Instant::now();
    let result = app
        .engine
        .invoke(
            &app.module_hash,
            &context,
            Arc::new(host),
            &WasmResourceLimits::default(),
            Arc::new(RwLock::new(StreamRegistry::default())),
        )
        .await
        .map_err(|e| e.to_string())?;
    if !result.success {
        return Err(format!("WASM invocation failed: {result:?}"));
    }
    let final_state = step(
        &mut sim,
        &mut history,
        &result.callback_action,
        result.callback_params.clone(),
    )?;
    if sim.has_violations() {
        return Err("Temper invariant violation".into());
    }
    let judgment = if let Some(message) = result.callback_params["error_message"].as_str() {
        message
            .find('{')
            .and_then(|at| serde_json::from_str::<Value>(&message[at..]).ok())
    } else {
        result.callback_params["telemetry_evidence_ref"]
            .as_str()
            .and_then(|s| s.split_once("#jev="))
            .and_then(|(_, raw)| urlencoding::decode(raw).ok())
            .and_then(|s| serde_json::from_str::<Value>(&s).ok())
    };
    Ok(
        json!({"scenario":scenario,"operation":operation,"module_sha256":app.module_hash,"duration_ms":started.elapsed().as_millis(),"provider_evidence":"controlled fixtures",
        "runtime":"Temper production actor evaluator + packaged WASM", "history":history,"status":sim.status(ID),"verified":final_state["booleans"]["deploy_verified"],
        "callback":result.callback_action,"callback_params":result.callback_params,"judgment":judgment,"model_requests":captures.lock().unwrap().clone(),"spans":spans}),
    )
}

async fn evaluate(
    State(app): State<Arc<App>>,
    headers: HeaderMap,
    Json(body): Json<Value>,
) -> Result<Json<Value>, (StatusCode, String)> {
    if headers
        .get("origin")
        .and_then(|h| h.to_str().ok())
        .is_some_and(|origin| origin != app.origin)
    {
        return Err((StatusCode::FORBIDDEN, "same-origin requests only".into()));
    }
    let scenario = body["scenario"]
        .as_str()
        .ok_or((StatusCode::BAD_REQUEST, "scenario required".into()))?
        .to_owned();
    tokio::task::spawn_blocking(move || {
        tokio::runtime::Handle::current().block_on(run(app, scenario))
    })
    .await
    .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?
    .map(Json)
    .map_err(|e| (StatusCode::BAD_REQUEST, e))
}
#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let key = std::fs::read_to_string(std::env::var("TYPESAFE_API_KEY_FILE")?)?
        .trim()
        .to_owned();
    let path=std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../os-apps/dsf-twin/wasm/dsf_railway_deploy_verify/target/wasm32-wasip1/release/dsf_railway_deploy_verify.wasm");
    let engine = WasmEngine::new()?;
    let module_hash = engine.compile_and_cache(&std::fs::read(path)?)?;
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await?;
    let origin = format!("http://{}", listener.local_addr()?);
    let app = Arc::new(App {
        engine,
        module_hash,
        key,
        origin: origin.clone(),
        busy: tokio::sync::Mutex::new(()),
    });
    if let Some(scenario) = std::env::args().nth(1) {
        println!(
            "{}",
            serde_json::to_string_pretty(
                &run(app, scenario).await.map_err(std::io::Error::other)?
            )?
        );
        return Ok(());
    }
    println!("Jev twin demo: {origin}");
    axum::serve(
        listener,
        Router::new()
            .route("/", get(|| async { Html(HTML) }))
            .route("/api/evaluate", post(evaluate))
            .with_state(app),
    )
    .await?;
    Ok(())
}
