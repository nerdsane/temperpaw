//! Invoke real Foresight WASMs with deterministic HTTP fixtures at the host boundary.
use serde_json::{Value, json};
use std::{
    collections::BTreeMap,
    path::PathBuf,
    sync::{Arc, Mutex, OnceLock, RwLock},
};
use temper_wasm::{
    SimWasmHost, StreamRegistry, WasmEngine, WasmHost, WasmInvocationContext, WasmInvocationResult,
    WasmResourceLimits,
};

fn module_bytes(module: &str) -> Vec<u8> {
    static MODULES: OnceLock<Mutex<BTreeMap<String, Vec<u8>>>> = OnceLock::new();
    let mut modules = MODULES
        .get_or_init(|| Mutex::new(BTreeMap::new()))
        .lock()
        .unwrap();
    modules.entry(module.into()).or_insert_with(|| {
        let path = if let Some(dir) = std::env::var_os("FORESIGHT_WASM_DIR") {
            // Explicit directories let the same regression exercise frozen before/after bytes.
            PathBuf::from(dir).join(format!("{module}.wasm"))
        } else {
            let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
            let output = std::process::Command::new("bash")
                .current_dir(&root)
                .args(["-c", "set -euo pipefail; source os-apps/wasm-build-env.sh; temperpaw_build_wasm \"$1\" wasm32-unknown-unknown", "foresight-wasm-test"])
                .arg(root.join(format!("os-apps/paw-foresight/wasm/{module}")))
                .output().expect("run canonical WASM build helper");
            assert!(output.status.success(), "build {module}: {}", String::from_utf8_lossy(&output.stderr));
            PathBuf::from(String::from_utf8(output.stdout).unwrap().trim())
        };
        std::fs::read(&path)
            .unwrap_or_else(|e| panic!("Read built Foresight WASM {}: {e}", path.display()))
    }).clone()
}

fn context(module: &str, trigger: &str, fields: Value) -> WasmInvocationContext {
    WasmInvocationContext {
        tenant: "deep-sci-fi".into(),
        entity_type: "World".into(),
        entity_id: "world-1".into(),
        trigger_action: trigger.into(),
        wasm_module: Some(module.into()),
        trigger_params: json!({}),
        entity_state: json!({"status":"RegisteringForecasts","fields":fields,"events":[{"action":trigger,"timestamp":"2026-09-16T04:00:00.123456Z"}]}),
        agent_id: None,
        session_id: None,
        integration_config: BTreeMap::from([(
            "temper_api_url".into(),
            "https://temper.test".into(),
        )]),
        trace_id: String::new(),
        workflow_root_entity_type: None,
        workflow_root_entity_id: None,
        workflow_run_id: None,
        http_request: None,
    }
}
async fn invoke(
    module: &str,
    ctx: WasmInvocationContext,
    host: impl WasmHost + 'static,
) -> WasmInvocationResult {
    let engine = WasmEngine::new().unwrap();
    let hash = engine.compile_and_cache(&module_bytes(module)).unwrap();
    engine
        .invoke(
            &hash,
            &ctx,
            Arc::new(host),
            &WasmResourceLimits::default(),
            Arc::new(RwLock::new(StreamRegistry::default())),
        )
        .await
        .unwrap()
}
fn registration_host() -> SimWasmHost {
    SimWasmHost::new().with_default_response(404,"missing")
      .with_response("https://temper.test/tdata/EventNodes?$filter=world_id eq 'world-1'&$top=513",200,&json!({"value":[{"Id":"node-1","Status":"Proposed","Probability":"0.55","Provenance":"authored","ResolveBy":"2027-06-01T00:00:00Z","Statement":"Synthetic future event"}]}).to_string())
      .with_response("https://temper.test/tdata/Forecasts?$filter=event_node_id eq 'node-1'&$top=513",200,"{\"value\":[]}")
}
fn world(mode: &str) -> Value {
    json!({"learning_mode":mode,"frontier_date":"2028-01-01T00:00:00Z","last_ingest_date":"2026-09-16T00:00:00Z","model_json":"","adopted_learning_run_id":""})
}

#[tokio::test(flavor = "multi_thread")]
async fn observed_registration_uses_host_event_time_even_without_ingest_date() {
    for supplied in ["", "2000-01-01T00:00:00Z"] {
        let mut fields = world("observed");
        fields["last_ingest_date"] = json!(supplied);
        let result = invoke(
            "register_forecasts",
            context("register_forecasts", "PathsScored", fields),
            registration_host(),
        )
        .await;
        assert!(result.success, "{result:?}");
        assert_eq!(result.callback_action, "ForecastPrepared", "{result:?}");
        assert_eq!(
            result.callback_params["forecast_registered_at"],
            "2026-09-16T04:00:00Z"
        );
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn registration_input_and_lookup_failures_return_recoverable_callback() {
    for (date, host) in [
        ("", registration_host()),
        (
            "2026-09-16T00:00:00Z",
            SimWasmHost::new().with_default_response(503, "temporary"),
        ),
    ] {
        let mut fields = world("historical");
        fields["last_ingest_date"] = json!(date);
        let result = invoke(
            "register_forecasts",
            context("register_forecasts", "RegisterForecasts", fields),
            host,
        )
        .await;
        assert!(
            result.success,
            "failure is a declared recovery callback: {result:?}"
        );
        assert_eq!(result.callback_action, "ForecastRegistrationFailed");
        assert!(
            !result.callback_params["error_message"]
                .as_str()
                .unwrap()
                .is_empty()
        );
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn authoritative_registered_identity_advances_when_collection_lags() {
    let fields = world("simulated");
    let first = invoke(
        "register_forecasts",
        context("register_forecasts", "RegisterForecasts", fields.clone()),
        registration_host(),
    )
    .await;
    assert_eq!(first.callback_action, "ForecastPrepared", "{first:?}");
    let id = first.callback_params["forecast_id"].as_str().unwrap();
    let key = &first.callback_params["forecast_registration_key"];
    let existing =
        json!({"entity_id":id,"status":"Preregistered","fields":{"registration_key":key}});
    let host = registration_host().with_response(
        &format!("https://temper.test/tdata/Forecasts('{id}')"),
        200,
        &existing.to_string(),
    );
    let retry = invoke(
        "register_forecasts",
        context("register_forecasts", "RegisterForecasts", fields),
        host,
    )
    .await;
    assert_eq!(
        retry.callback_action, "ForecastRegistrationComplete",
        "{retry:?}"
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn historical_grading_scores_every_revision_but_requests_learning_once() {
    for evidence in ["eligible", "undated", "unverified", "proxy", "bad_source"] {
        let mut rows = vec![];
        let mut host = SimWasmHost::new().with_default_response(503, "unexpected request");
        for (id, event) in [("f1", "e1"), ("f1-revision", "e1"), ("f2", "e2")] {
            rows.push(json!({"Id":id,"Status":"Preregistered","EventNodeId":event,"Question":event,"Probability":"0.55","BaseProbability":"0.55","EvidenceKind":if evidence=="proxy"{"proxy"}else{"historical"},"RegisteredAt":"2025-03-01T00:00:00Z"}));
            host = host
                .with_response(
                    &format!("https://temper.test/tdata/Forecasts('{id}')/TemperPaw.Resolve"),
                    200,
                    "{}",
                )
                .with_response(
                    &format!("https://temper.test/tdata/Forecasts('{id}')/TemperPaw.ScoreBatch"),
                    200,
                    "{}",
                );
        }
        let actuals = json!([{"event_node_id":if evidence=="unverified"{""}else{"e1"},"question_contains":"e1","outcome":"yes","resolved_at":if evidence!="undated"{"2025-06-01T00:00:00Z"}else{""},"source_refs":[if evidence=="bad_source"{"ftp://invalid.test/actual"}else{"https://example.test/actual"}]},{"event_node_id":if evidence=="unverified"{""}else{"e2"},"question_contains":"e2","outcome":"no","resolved_at":if evidence!="undated"{"2025-06-02T00:00:00Z"}else{""},"source_refs":[if evidence=="bad_source"{"ftp://invalid.test/actual"}else{"https://example.test/actual"}]}]);
        host = host
            .with_response(
                "https://temper.test/tdata/Files('actuals')/$value",
                200,
                &actuals.to_string(),
            )
            .with_response(
                "https://temper.test/tdata/Forecasts?$filter=world_id eq 'world-1'&$top=513",
                200,
                &json!({"value":rows}).to_string(),
            );
        let mut ctx = context(
            "grade_hindcast",
            "Grade",
            json!({"world_id":"world-1","actuals_file_id":"actuals"}),
        );
        ctx.entity_type = "Hindcast".into();
        let result = invoke("grade_hindcast", ctx, host).await;
        assert_eq!(result.callback_action, "ScoreComplete", "{result:?}");
        assert_eq!(result.callback_params["graded_count"], "3", "{result:?}");
        assert_eq!(
            result.callback_params["learning_as_of"],
            if evidence == "eligible" {
                "2025-06-02T00:00:00Z"
            } else {
                ""
            }
        );
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn adoption_during_registration_keeps_the_batch_model_snapshot() {
    let frozen = json!({"version":"old-model","mode":"simulated","slope":1.0,"intercept":0.0,"evaluated_through":"2025-01-01T00:00:00Z","used_event_ids":[]});
    let newer = json!({"version":"new-model","mode":"simulated","slope":2.0,"intercept":0.0,"evaluated_through":"2025-02-01T00:00:00Z","used_event_ids":[]});
    let mut fields = world("simulated");
    fields["model_json"] = json!(newer.to_string());
    fields["adopted_learning_run_id"] = json!("new-run");
    fields["registration_model_json"] = json!(frozen.to_string());
    fields["forecast_learning_run_id"] = json!("old-run");
    let result = invoke(
        "register_forecasts",
        context("register_forecasts", "ForecastRegistered", fields),
        registration_host(),
    )
    .await;
    assert_eq!(result.callback_action, "ForecastPrepared", "{result:?}");
    assert_eq!(
        result.callback_params["forecast_model_version"],
        "old-model"
    );
    assert_eq!(
        result.callback_params["forecast_learning_run_id"],
        "old-run"
    );
    assert_eq!(result.callback_params["forecast_probability"], "0.55");
    assert_eq!(
        serde_json::from_str::<Value>(
            result.callback_params["registration_model_json"]
                .as_str()
                .unwrap()
        )
        .unwrap(),
        frozen
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn legacy_hindcast_registers_at_explicit_clock_then_prepares_historical_learning() {
    let mut fields = world("observed");
    fields["hindcast_mode"] = json!("true");
    fields["last_ingest_date"] = json!("");
    let missing = invoke(
        "register_forecasts",
        context("register_forecasts", "PathsScored", fields.clone()),
        registration_host(),
    )
    .await;
    assert_eq!(
        missing.callback_action, "ForecastRegistrationFailed",
        "{missing:?}"
    );
    fields["last_ingest_date"] = json!("2025-03-01T00:00:00Z");
    let registration = invoke(
        "register_forecasts",
        context("register_forecasts", "RegisterForecasts", fields.clone()),
        registration_host(),
    )
    .await;
    assert_eq!(
        registration.callback_action, "ForecastPrepared",
        "{registration:?}"
    );
    assert_eq!(
        registration.callback_params["forecast_registered_at"],
        "2025-03-01T00:00:00Z"
    );
    assert_eq!(
        registration.callback_params["forecast_evidence_kind"],
        "historical"
    );
    assert_eq!(registration.callback_params["learning_mode"], "historical");
    for (key, value) in registration.callback_params.as_object().unwrap() {
        fields[key] = value.clone();
    }
    let forecast = json!({"status":"Scored","fields":{
        "event_node_id":"node-1","base_probability":"0.55","outcome":"yes",
        "registered_at":registration.callback_params["forecast_registered_at"],
        "resolved_at":"2025-06-01T00:00:00Z",
        "evidence_kind":registration.callback_params["forecast_evidence_kind"],
        "outcome_evidence_kind":"historical",
        "outcome_source_refs":"[\"https://example.test/recorded-actual\"]"
    }});
    let host = SimWasmHost::new()
        .with_default_response(503, "unexpected request")
        .with_response(
            "https://temper.test/tdata/Worlds('world-1')",
            200,
            &json!({"status":"Active","fields":fields}).to_string(),
        )
        .with_response(
            "https://temper.test/tdata/Forecasts?$filter=world_id eq 'world-1'&$top=513",
            200,
            &json!({"value":[forecast]}).to_string(),
        );
    let mut ctx = context(
        "learning_prepare",
        "Start",
        json!({"world_id":"world-1","mode":"historical","as_of":"2025-06-01T00:00:00Z","dataset_json":""}),
    );
    ctx.entity_type = "LearningRun".into();
    let prepared = invoke("learning_prepare", ctx, host).await;
    assert_eq!(prepared.callback_action, "Prepared", "{prepared:?}");
    let report: Value =
        serde_json::from_str(prepared.callback_params["prepared_json"].as_str().unwrap()).unwrap();
    assert_eq!(report["validation"].as_array().unwrap().len(), 1);
    assert_eq!(report["training"].as_array().unwrap().len(), 0);
}

struct SessionConfigureHost {
    inner: SimWasmHost,
    configure_requests: Arc<Mutex<Vec<Value>>>,
}

#[async_trait::async_trait]
impl WasmHost for SessionConfigureHost {
    async fn http_call(
        &self,
        method: &str,
        url: &str,
        headers: &[(String, String)],
        body: &str,
    ) -> Result<(u16, String), String> {
        if method == "POST" && url.ends_with("/TemperPaw.Configure") {
            self.configure_requests
                .lock()
                .unwrap()
                .push(serde_json::from_str(body).expect("Session.Configure body must be JSON"));
        }
        self.inner.http_call(method, url, headers, body).await
    }

    async fn http_call_binary(
        &self,
        method: &str,
        url: &str,
        headers: &[(String, String)],
        body: &[u8],
    ) -> Result<(u16, Vec<u8>), String> {
        self.inner
            .http_call_binary(method, url, headers, body)
            .await
    }

    fn get_secret(&self, key: &str) -> Result<String, String> {
        self.inner.get_secret(key)
    }

    fn log(&self, level: &str, message: &str) {
        self.inner.log(level, message);
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn seed_world_records_research_session_from_the_create_response() {
    let host = SimWasmHost::new()
        .with_default_response(404, "unexpected request")
        .with_response(
            "https://temper.test/tdata/Workspaces",
            201,
            r#"{"entity_id":"workspace-1"}"#,
        )
        .with_response(
            "https://temper.test/tdata/Agents",
            201,
            r#"{"entity_id":"agent-1"}"#,
        )
        .with_response(
            "https://temper.test/tdata/Sessions",
            201,
            r#"{"entity_id":"session-1"}"#,
        )
        .with_response(
            "https://temper.test/tdata/Sessions('session-1')/TemperPaw.Configure",
            200,
            "{}",
        );
    let mut ctx = context(
        "seed_world",
        "Seed",
        json!({"agent_model":"fixture-model","agent_provider":"fixture-provider"}),
    );
    ctx.entity_state["status"] = json!("Seeding");
    ctx.entity_state["counters"] = json!({"research_attempt":7});
    let configure_requests = Arc::new(Mutex::new(Vec::new()));
    let host = SessionConfigureHost {
        inner: host,
        configure_requests: Arc::clone(&configure_requests),
    };
    let result = invoke("seed_world", ctx, host).await;
    let requests = configure_requests.lock().unwrap();
    assert_eq!(
        requests.len(),
        1,
        "seed must configure its research session"
    );
    assert_eq!(
        requests[0]["tool_choice"], "required",
        "research must use explicit tool completion instead of a plain-text end turn"
    );
    assert!(result.success, "{result:?}");
    assert_eq!(
        result.callback_action, "ResearchSessionStarted",
        "{result:?}"
    );
    assert_eq!(result.callback_params["research_session_id"], "session-1");
    assert_ne!(result.callback_params["research_session_id"], "agent-1");
    assert_eq!(result.callback_params["expected_research_attempt"], 7);
}
