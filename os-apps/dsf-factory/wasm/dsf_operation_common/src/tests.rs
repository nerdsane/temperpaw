use super::*;
use std::collections::VecDeque;
const SHA: &str = "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";
const OP: &str = "01a074e0-61c3-7e51-bc70-2c0130bb73b8";
struct Fake {
    responses: VecDeque<Response>,
    requests: Vec<Request>,
}
impl Fake {
    fn new(values: Vec<Value>) -> Self {
        Self {
            responses: values
                .into_iter()
                .map(|v| Response {
                    status: 200,
                    body: v.to_string(),
                })
                .collect(),
            requests: vec![],
        }
    }
}
impl Host for Fake {
    fn request(&mut self, r: &Request) -> Result<Response, Error> {
        self.requests.push(Request {
            method: r.method,
            url: r.url.clone(),
            headers: r.headers.clone(),
            body: r.body.clone(),
        });
        self.responses.pop_front().ok_or(Error::Transport)
    }
    fn secret(&mut self, _: &str) -> Result<String, Error> {
        Ok("SECRET_NOT_FOR_EVIDENCE".into())
    }
}
fn config_value() -> Value {
    json!({"version":1,"resource_id":"resource-1","effort_id":"effort-1","operation_key":OP,"revision":SHA,"max_cost_cents":0,"not_before_ms":1000,"provider":{"kind":"railway","project_id":"project-1","service_id":"service-1","environment_id":"env-1","secret_name":"railway_token","baseline_deployment_id":"before"},"flow":{"kind":"story","story_id":"story-1","world_id":"world-1"},"datadog":{"site":"datadoghq.com","service":"deep-sci-fi-backend","environment":"production","api_key_secret":"dd_api","app_key_secret":"dd_app"}})
}
fn operation() -> Operation {
    let config = config_value();
    Operation {
        key: OP.into(),
        resource_id: "resource-1".into(),
        effort_id: "effort-1".into(),
        kind: Kind::RailwayDeploy,
        revision: SHA.into(),
        proof_id: "proof-1".into(),
        binding: Binding {
            config_ref: "config-1".into(),
            config_sha256: format!("{:x}", Sha256::digest(config.to_string().as_bytes())),
        },
        execution_id: None,
        execution_attempts: 1,
    }
}
fn resource() -> Value {
    json!({"status":"Operating","active_operation_id":OP,"allowed_operations":"[\"railway_deploy\"]","provider":"railway","provider_id":"service-1"})
}
fn effort() -> Value {
    json!({"status":"Merged","head_sha":SHA,"proof_packet_id":"proof-1","ask_ids":"[]","e2e_ok":true,"proof_attached":true,"review_passed":true,"evaluation_passed":true})
}
fn proof() -> Value {
    json!({"effort_id":"effort-1","status":"Recorded","commit":SHA,"record_present":true,"artifact_ref":"artifact-1","changed_surface":"[\"story-read\"]","blast_radius":"[]","features":"[{\"key\":\"story-read\",\"verification\":\"rerun\",\"verdict\":\"pass\"}]","tests":"{\"result\":\"pass\"}","independent_verifier":"{\"agrees\":true,\"reran\":[\"story-read\"]}"})
}
fn runtime(host: &mut Fake) -> Runtime<'_, Fake> {
    Runtime {
        host,
        base: "https://temper.local",
        tenant: "default",
        key: "TEMPER_SECRET",
        now_ms: 5000,
    }
}
fn valid_reads() -> Vec<Value> {
    vec![
        config_value(),
        resource(),
        effort(),
        proof(),
        json!({"Status":"Ready"}),
        json!({"proof":"actual artifact content"}),
    ]
}
fn records(op: &Operation, r: &Value, e: &Value, p: &Value) -> Result<(), Error> {
    validation::validate_records(
        op,
        &serde_json::from_value(config_value()).unwrap(),
        r,
        e,
        p,
    )
}
#[test]
fn wrong_effort_revision_cannot_deploy() {
    let mut e = effort();
    e["head_sha"] = json!("bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb");
    assert!(records(&operation(), &resource(), &e, &proof()).is_err());
}
#[test]
fn unrelated_proof_is_rejected() {
    let mut p = proof();
    p["effort_id"] = json!("other");
    assert!(records(&operation(), &resource(), &effort(), &p).is_err());
}
#[test]
fn resource_without_ownership_lock_is_rejected() {
    let mut r = resource();
    r["active_operation_id"] = json!("other");
    assert!(records(&operation(), &r, &effort(), &proof()).is_err());
}
#[test]
fn routine_authorized_deploy_needs_no_artificial_ask() {
    let mut fake = Fake::new(valid_reads());
    let result = validate(&mut runtime(&mut fake), &operation()).unwrap();
    assert_eq!(result.action, "ValidationSucceeded");
    assert_eq!(fake.requests.len(), 6);
    assert!(fake.requests.iter().all(|r| r.method == "GET"));
}
#[test]
fn unresolved_blocking_ask_prevents_provider_write() {
    let mut rows = valid_reads();
    rows[2]["ask_ids"] = json!("[\"ask-1\"]");
    rows.insert(
        4,
        json!({"effort_id":"effort-1","status":"Open","stalls":true}),
    );
    let mut fake = Fake::new(rows);
    assert!(matches!(
        validate(&mut runtime(&mut fake), &operation()),
        Err(Error::Blocked(_))
    ));
    assert!(fake.requests.iter().all(|r| r.method == "GET"));
}
#[test]
fn answered_denied_required_ask_is_rejected() {
    let mut cfg = config_value();
    cfg["required_ask_ids"] = json!(["ask-1"]);
    let mut op = operation();
    op.binding.config_sha256 = format!("{:x}", Sha256::digest(cfg.to_string().as_bytes()));
    let mut rows = valid_reads();
    rows[0] = cfg;
    rows[2]["ask_ids"] = json!("[\"ask-1\"]");
    rows.insert(4,json!({"effort_id":"effort-1","status":"Answered","stalls":true,"chose":"deny","who":"human"}));
    assert!(matches!(
        validate(&mut runtime(&mut Fake::new(rows)), &op),
        Err(Error::Blocked(_))
    ));
}
#[test]
fn edited_configuration_file_cannot_retarget_operation() {
    let mut cfg = config_value();
    cfg["provider"]["service_id"] = json!("another-service");
    let mut fake = Fake::new(vec![cfg]);
    assert!(matches!(
        validate(&mut runtime(&mut fake), &operation()),
        Err(Error::Binding("configuration File hash changed"))
    ));
    assert_eq!(fake.requests.len(), 1);
}
#[test]
fn proof_boolean_does_not_substitute_for_actual_passing_record() {
    let mut rows = valid_reads();
    rows[3]["tests"] = json!("{\"result\":\"fail\"}");
    assert!(matches!(
        validate(&mut runtime(&mut Fake::new(rows)), &operation()),
        Err(Error::Proof(_))
    ));
}
#[test]
fn railway_does_not_retry_an_ambiguous_write() {
    let mut rows = valid_reads();
    rows.push(json!({"data":{"deployments":{"edges":[]}}}));
    let mut op = operation();
    op.execution_attempts = 2;
    let mut fake = Fake::new(rows);
    assert!(matches!(
        execute(&mut runtime(&mut fake), &op),
        Err(Error::Pending(_))
    ));
    assert!(!fake.requests.iter().any(|r| r.body.contains("mutation")));
}
#[test]
fn missing_railway_list_is_not_proof_of_absence() {
    let mut fake = Fake::new(vec![
        config_value(),
        json!({"data":{"deployments":{"edges":[]}}}),
    ]);
    assert!(matches!(
        observe(&mut runtime(&mut fake), &operation()),
        Err(Error::Pending(_))
    ));
}
#[test]
fn railway_send_pins_exact_revision_and_receipt() {
    let mut rows = valid_reads();
    rows.extend([
        json!({"data":{"deployments":{"edges":[]}}}),
        json!({"data":{"serviceInstance":{"latestDeployment":{"id":"before"}}}}),
        json!({"data":{"serviceInstanceDeployV2":"deployment-new"}}),
    ]);
    let mut fake = Fake::new(rows);
    let result = execute(&mut runtime(&mut fake), &operation()).unwrap();
    assert_eq!(result.params["provider_execution_id"], "deployment-new");
    let mutations: Vec<_> = fake
        .requests
        .iter()
        .filter(|r| r.body.contains("mutation"))
        .collect();
    assert_eq!(mutations.len(), 1);
    let body: Value = serde_json::from_str(&mutations[0].body).unwrap();
    assert_eq!(body["variables"]["commitSha"], SHA);
    assert!(!result.params.to_string().contains("SECRET"));
}
#[test]
fn wrong_request_or_revision_in_datadog_is_not_verification() {
    let op = operation();
    let dd = serde_json::from_value::<Config>(config_value())
        .unwrap()
        .datadog;
    let mut span = json!({"attributes":{"service":dd.service,"env":dd.environment,"trace_id":"123","custom":{"git":{"commit":{"sha":SHA}},"dsf":{"request_id":OP}}}});
    assert!(verification::matching_span(&span, &op, &dd));
    span["attributes"]["custom"]["dsf"]["request_id"] = json!("other");
    assert!(!verification::matching_span(&span, &op, &dd));
    span["attributes"]["custom"]["dsf"]["request_id"] = json!(OP);
    span["attributes"]["custom"]["git"]["commit"]["sha"] = json!("other");
    assert!(!verification::matching_span(&span, &op, &dd));
}

fn configured_operation(cfg: &Value) -> Operation {
    let mut op = operation();
    let parsed: Config = serde_json::from_value(cfg.clone()).unwrap();
    op.kind = parsed.provider.kind();
    op.binding.config_sha256 = format!("{:x}", Sha256::digest(cfg.to_string().as_bytes()));
    op
}
fn reads_for(cfg: &Value) -> Vec<Value> {
    let config: Config = serde_json::from_value(cfg.clone()).unwrap();
    let mut rows = valid_reads();
    rows[0] = cfg.clone();
    rows[1]["provider"] = json!(config.provider.resource_provider());
    rows[1]["provider_id"] = json!(config.provider.provider_id());
    rows[1]["allowed_operations"] = json!(match config.provider.kind() {
        Kind::RailwayDeploy => "[\"railway_deploy\"]",
        Kind::VercelDeploy => "[\"vercel_deploy\"]",
        Kind::MediaRepair => "[\"media_repair\"]",
    });
    rows
}
fn vercel_config() -> Value {
    let mut cfg = config_value();
    cfg["provider"] = json!({"kind":"vercel","project_id":"prj-1","team_id":"team-1","project_name":"dsf","git_repository_id":12,"secret_name":"vercel_token"});
    cfg
}
fn vercel_deployment() -> Value {
    json!({"id":"deployment-new","projectId":"prj-1","target":"production","readyState":"READY","meta":{"dsfOperationKey":OP,"githubCommitSha":SHA}})
}
fn media_config() -> Value {
    let mut cfg = config_value();
    cfg["max_cost_cents"] = json!(2);
    cfg["provider"] = json!({"kind":"media","secret_name":"dsf_admin","generations":[{"id":"job-1","target_type":"story","target_id":"story-1","media_type":"cover_image","max_cost_cents":2}]});
    cfg["flow"] = json!({"kind":"media"});
    cfg
}
fn media_status(status: &str) -> Value {
    json!({"generation_id":"job-1","target_type":"story","target_id":"story-1","media_type":"cover_image","status":status,"attempt_id":OP,"media_url":"https://media.deep-sci-fi.world/image.webp","cost_usd":0.02})
}
fn media_receipt() -> Value {
    json!({"operation_id":OP,"generation_ids":["job-1"],"endpoint":"/api/media/retry-stuck","response":{"operation_id":OP,"replayed":false,"queued":1,"generations":[{"generation_id":"job-1","outcome":"claimed"}]}})
}
fn railway_found(status: &str) -> Value {
    json!({"data":{"deployments":{"edges":[{"node":{"id":"deployment-new","createdAt":"1970-01-01T00:00:02Z","status":status,"meta":{"commitHash":SHA}}}]}}})
}
fn span_response() -> Value {
    json!({"data":[{"attributes":{"service":"deep-sci-fi-backend","env":"production","trace_id":"123","status":"ok","custom":{"git":{"commit":{"sha":SHA}},"dsf":{"request_id":OP}}}}]})
}

#[test]
fn vercel_create_pins_project_commit_target_and_operation_metadata() {
    let cfg = vercel_config();
    let op = configured_operation(&cfg);
    let mut rows = reads_for(&cfg);
    rows.extend([json!({"deployments":[]}), vercel_deployment()]);
    let mut fake = Fake::new(rows);
    let callback = execute(&mut runtime(&mut fake), &op).unwrap();
    assert_eq!(callback.action, "ExecutionSucceeded");
    let request = fake.requests.last().unwrap();
    assert_eq!(request.method, "POST");
    assert_eq!(
        request.url,
        "https://api.vercel.com/v13/deployments?teamId=team-1"
    );
    let body: Value = serde_json::from_str(&request.body).unwrap();
    assert_eq!(body["project"], "prj-1");
    assert_eq!(body["gitSource"]["sha"], SHA);
    assert_eq!(body["meta"]["dsfOperationKey"], OP);
}
#[test]
fn replayed_vercel_operation_adopts_existing_receipt_without_post() {
    let cfg = vercel_config();
    let op = configured_operation(&cfg);
    let mut rows = reads_for(&cfg);
    rows.extend([
        json!({"deployments":[{"uid":"deployment-new","meta":{"dsfOperationKey":OP}}]}),
        vercel_deployment(),
    ]);
    let mut fake = Fake::new(rows);
    assert_eq!(
        execute(&mut runtime(&mut fake), &op).unwrap().params["provider_execution_id"],
        "deployment-new"
    );
    assert!(fake.requests.iter().all(|r| r.method == "GET"));
}
#[test]
fn media_send_uses_selected_jobs_and_durable_operation_id() {
    let cfg = media_config();
    let op = configured_operation(&cfg);
    let mut fake = Fake::new(reads_for(&cfg));
    fake.responses.push_back(Response {
        status: 404,
        body: "{}".into(),
    });
    fake.responses.push_back(Response {
        status: 200,
        body: media_status("pending").to_string(),
    });
    fake.responses.push_back(Response {
        status: 200,
        body: media_receipt()["response"].to_string(),
    });
    let callback = execute(&mut runtime(&mut fake), &op).unwrap();
    assert_eq!(callback.params["provider_execution_id"], OP);
    let body: Value = serde_json::from_str(&fake.requests.last().unwrap().body).unwrap();
    assert_eq!(body, json!({"operation_id":OP,"generation_ids":["job-1"]}));
}
#[test]
fn media_reconcile_reads_receipt_without_rescheduling() {
    let cfg = media_config();
    let op = configured_operation(&cfg);
    let mut fake = Fake::new(vec![cfg, media_receipt()]);
    assert_eq!(
        observe(&mut runtime(&mut fake), &op).unwrap().action,
        "ProviderFound"
    );
    assert!(fake.requests.iter().all(|r| r.method == "GET"));
}
#[test]
fn media_receipt_for_different_selection_is_rejected() {
    let cfg = media_config();
    let op = configured_operation(&cfg);
    let mut receipt = media_receipt();
    receipt["generation_ids"] = json!(["job-other"]);
    assert!(matches!(
        observe(&mut runtime(&mut Fake::new(vec![cfg, receipt])), &op),
        Err(Error::Binding(_))
    ));
}
#[test]
fn verification_waits_for_asynchronous_provider_completion() {
    let mut op = operation();
    op.execution_id = Some("deployment-new".into());
    let mut fake = Fake::new(vec![config_value(), resource(), railway_found("BUILDING")]);
    assert!(matches!(
        verify(&mut runtime(&mut fake), &op),
        Err(Error::Pending(_))
    ));
    assert_eq!(fake.requests.len(), 3);
}
#[test]
fn health_body_revision_must_match_before_flow_probe() {
    let mut op = operation();
    op.execution_id = Some("deployment-new".into());
    let mut fake = Fake::new(vec![
        config_value(),
        resource(),
        railway_found("SUCCESS"),
        json!({"status":"healthy","git_sha":"other"}),
    ]);
    assert!(matches!(
        verify(&mut runtime(&mut fake), &op),
        Err(Error::Pending(_))
    ));
    assert_eq!(fake.requests.len(), 4);
}
#[test]
fn verification_runs_actual_health_flow_and_correlated_telemetry_reads() {
    let mut op = operation();
    op.execution_id = Some("deployment-new".into());
    let mut fake = Fake::new(vec![
        config_value(),
        resource(),
        railway_found("SUCCESS"),
        json!({"status":"healthy","git_sha":SHA}),
        json!({"story":{"id":"story-1","world_id":"world-1","content":"published story","status":"published"}}),
        span_response(),
    ]);
    let callback = verify(&mut runtime(&mut fake), &op).unwrap();
    assert_eq!(callback.action, "VerificationSucceeded");
    assert_eq!(callback.params["verified_revision"], SHA);
    assert_eq!(
        callback.params["telemetry_evidence_ref"],
        "https://app.datadoghq.com/apm/trace/123"
    );
    assert!(
        fake.requests[3]
            .headers
            .iter()
            .any(|(k, v)| k == "x-request-id" && v == OP)
    );
    assert!(fake.requests[4].url.contains("/api/stories/story-1"));
    assert!(fake.requests[5].body.contains(SHA));
    assert!(fake.requests[5].body.contains(OP));
    assert!(!callback.params.to_string().contains("SECRET"));
}
#[test]
fn no_datadog_evidence_cannot_become_verified() {
    let mut op = operation();
    op.execution_id = Some("deployment-new".into());
    let mut fake = Fake::new(vec![
        config_value(),
        resource(),
        railway_found("SUCCESS"),
        json!({"status":"healthy","git_sha":SHA}),
        json!({"story":{"id":"story-1","world_id":"world-1","content":"published story","status":"published"}}),
        json!({"data":[]}),
    ]);
    assert!(matches!(
        verify(&mut runtime(&mut fake), &op),
        Err(Error::Pending(_))
    ));
}
#[test]
fn media_completion_must_belong_to_the_operation_attempt() {
    let cfg = media_config();
    let mut op = configured_operation(&cfg);
    op.execution_id = Some(OP.into());
    let mut status = media_status("completed");
    status["attempt_id"] = json!("different");
    let mut fake = Fake::new(vec![
        cfg,
        resource(),
        media_receipt(),
        json!({"status":"healthy","git_sha":SHA}),
        status,
    ]);
    assert!(matches!(
        verify(&mut runtime(&mut fake), &op),
        Err(Error::Pending(_))
    ));
    assert!(!fake.requests.iter().any(|r| r.method == "HEAD"));
}
#[test]
fn media_cost_ceiling_rejects_an_unbudgeted_selection_before_send() {
    let mut cfg = media_config();
    cfg["max_cost_cents"] = json!(1);
    let op = configured_operation(&cfg);
    let mut fake = Fake::new(reads_for(&cfg));
    assert!(matches!(
        execute(&mut runtime(&mut fake), &op),
        Err(Error::Binding(_))
    ));
    assert!(fake.requests.iter().all(|r| r.method == "GET"));
}

#[test]
fn datadog_matches_actual_nested_custom_tags() {
    let op = operation();
    let config: Config = serde_json::from_value(config_value()).unwrap();
    let span = json!({"attributes":{"service":"deep-sci-fi-backend","env":"production","status":"ok","trace_id":"123","custom":{"git":{"commit":{"sha":SHA}},"dsf":{"request_id":OP}}}});
    assert!(verification::matching_span(&span, &op, &config.datadog));
    let mut wrong = span;
    wrong["attributes"]["custom"]["dsf"]["request_id"] = json!("another-request");
    assert!(!verification::matching_span(&wrong, &op, &config.datadog));
}

fn two_job_media() -> (Value, Value) {
    let mut cfg = media_config();
    let mut second = cfg["provider"]["generations"][0].clone();
    second["id"] = json!("job-2");
    cfg["provider"]["generations"]
        .as_array_mut()
        .unwrap()
        .push(second);
    cfg["max_cost_cents"] = json!(4);
    let mut receipt = media_receipt();
    receipt["generation_ids"] = json!(["job-1", "job-2"]);
    receipt["response"]["generations"] = json!([
        {"generation_id":"job-1","outcome":"claimed"},
        {"generation_id":"job-2","outcome":"ineligible"}
    ]);
    (cfg, receipt)
}

#[test]
fn partially_claimed_media_cannot_release_while_claimed_job_runs() {
    let (cfg, receipt) = two_job_media();
    let mut op = configured_operation(&cfg);
    op.execution_id = Some(OP.into());
    let mut fake = Fake::new(vec![cfg, resource(), receipt, media_status("generating")]);
    assert!(matches!(
        verify(&mut runtime(&mut fake), &op),
        Err(Error::Pending(_))
    ));
    assert!(fake.requests.last().unwrap().url.ends_with("/job-1/status"));
}

#[test]
fn partial_media_is_terminal_only_after_claimed_jobs_settle() {
    let (cfg, receipt) = two_job_media();
    let mut op = configured_operation(&cfg);
    op.execution_id = Some(OP.into());
    let mut fake = Fake::new(vec![cfg, resource(), receipt, media_status("completed")]);
    assert!(matches!(
        verify(&mut runtime(&mut fake), &op),
        Err(Error::ProviderFailed(_))
    ));
    assert!(fake.requests.last().unwrap().url.ends_with("/job-1/status"));
}

#[test]
fn failed_media_job_does_not_release_other_running_selected_jobs() {
    let (cfg, mut receipt) = two_job_media();
    receipt["response"]["generations"][1]["outcome"] = json!("claimed");
    let mut op = configured_operation(&cfg);
    op.execution_id = Some(OP.into());
    let mut running = media_status("generating");
    running["generation_id"] = json!("job-2");
    let mut fake = Fake::new(vec![
        cfg,
        resource(),
        receipt,
        json!({"status":"healthy","git_sha":SHA}),
        media_status("failed"),
        running,
    ]);
    assert!(matches!(
        verify(&mut runtime(&mut fake), &op),
        Err(Error::Pending(_))
    ));
}

#[test]
fn video_repair_uses_exact_duration_and_rejects_missing_duration() {
    for duration in [json!(15), Value::Null] {
        let mut cfg = media_config();
        cfg["max_cost_cents"] = json!(50);
        cfg["provider"]["generations"][0]["media_type"] = json!("video");
        cfg["provider"]["generations"][0]["max_cost_cents"] = json!(50);
        let op = configured_operation(&cfg);
        let mut fake = Fake::new(reads_for(&cfg));
        fake.responses.push_back(Response {
            status: 404,
            body: "{}".into(),
        });
        let mut status = media_status("pending");
        status["media_type"] = json!("video");
        status["duration_seconds"] = duration;
        fake.responses.push_back(Response {
            status: 200,
            body: status.to_string(),
        });
        fake.responses.push_back(Response {
            status: 200,
            body: media_receipt()["response"].to_string(),
        });
        assert!(matches!(
            execute(&mut runtime(&mut fake), &op),
            Err(Error::Binding(_))
        ));
        assert!(fake.requests.iter().all(|request| request.method == "GET"));
    }
}

#[test]
fn video_repair_accepts_the_exact_fifteen_second_cost_ceiling() {
    let mut cfg = media_config();
    cfg["max_cost_cents"] = json!(75);
    cfg["provider"]["generations"][0]["media_type"] = json!("video");
    cfg["provider"]["generations"][0]["max_cost_cents"] = json!(75);
    let op = configured_operation(&cfg);
    let mut fake = Fake::new(reads_for(&cfg));
    fake.responses.push_back(Response {
        status: 404,
        body: "{}".into(),
    });
    let mut status = media_status("pending");
    status["media_type"] = json!("video");
    status["duration_seconds"] = json!(15.0);
    fake.responses.push_back(Response {
        status: 200,
        body: status.to_string(),
    });
    fake.responses.push_back(Response {
        status: 200,
        body: media_receipt()["response"].to_string(),
    });
    assert_eq!(
        execute(&mut runtime(&mut fake), &op).unwrap().action,
        "ExecutionSucceeded"
    );
    assert_eq!(fake.requests.last().unwrap().method, "POST");
}
