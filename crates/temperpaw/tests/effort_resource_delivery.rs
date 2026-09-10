//! Real actor contract for legacy and resource-owned Effort delivery.
use serde_json::{Value, json};
use std::{fs, path::PathBuf, sync::Arc};
use temper_jit::table::TransitionTable;
use temper_runtime::scheduler::{FaultConfig, SimActorSystem, SimActorSystemConfig};
use temper_server::entity_actor::sim_handler::EntityActorHandler;
fn source() -> String {
    fs::read_to_string(
        PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../../os-apps/paw-patrol/specs/effort.ioa.toml"),
    )
    .unwrap()
}
fn step(sim: &mut SimActorSystem, name: &str, params: Value) -> Value {
    sim.step("effort", name, &params.to_string())
        .unwrap_or_else(|error| panic!("{name}: {error}"))
}
fn proving() -> SimActorSystem {
    let ioa = source();
    let handler = EntityActorHandler::new(
        "Effort",
        "effort",
        Arc::new(TransitionTable::from_ioa_source(&ioa)),
    )
    .with_ioa_invariants(&ioa);
    let mut sim = SimActorSystem::new(SimActorSystemConfig {
        seed: 467,
        faults: FaultConfig::none(),
        ..Default::default()
    });
    sim.register_actor("effort", Box::new(handler));
    step(
        &mut sim,
        "Seed",
        json!({"intent_id":"intent-1","intent_ref":"intent.md","factory_case_id":"case-1","task_summary":"delivery","task_detail":"API and web","risk_lane":"L1","repo":"owner/repo","branch":"codex/delivery"}),
    );
    step(&mut sim, "AttachSpec", json!({"spec_ref":"spec.md"}));
    step(&mut sim, "Specify", json!({}));
    step(&mut sim, "AttachPlan", json!({"plan_ref":"plan.md"}));
    step(&mut sim, "Plan", json!({"plan_summary":"deliver"}));
    step(&mut sim, "StartBuild", json!({}));
    step(&mut sim, "WorkerDone", json!({}));
    step(&mut sim, "SubmitForReview", json!({}));
    step(
        &mut sim,
        "AttachReviewRun",
        json!({"reviewer_run_id":"review-1","review_run_ids":"review-1"}),
    );
    step(&mut sim, "MarkFixItClear", json!({}));
    step(&mut sim, "MarkRiskClear", json!({}));
    step(
        &mut sim,
        "PassReview",
        json!({"reviewer_run_id":"review-1"}),
    );
    step(&mut sim, "ReportE2e", json!({"e2e_summary":"live proof"}));
    step(
        &mut sim,
        "PassEvaluation",
        json!({"evaluation_run_id":"evaluation-1"}),
    );
    step(
        &mut sim,
        "AttachProofPacket",
        json!({"proof_packet_id":"proof-1","proof_packet_ids":"proof-1"}),
    );
    step(
        &mut sim,
        "AttachDecisions",
        json!({"decisions_ref":"decisions.md"}),
    );
    sim
}
fn configuration() -> Value {
    json!({"resource_delivery_plan":"{\"operations\":[\"exact-plan\"]}","resource_delivery_head":"a".repeat(40)})
}
fn callback(sequence: u64) -> Value {
    json!({"expected_delivery_plan":configuration()["resource_delivery_plan"],"expected_delivery_head":"a".repeat(40),"expected_delivery_sequence":sequence})
}
#[test]
fn resource_delivery_uses_review_gates_and_cannot_use_the_legacy_completion_callback() {
    let mut sim = proving();
    step(&mut sim, "ConfigureResourceDelivery", configuration());
    assert!(
        sim.step(
            "effort",
            "MergeResourceDelivery",
            &json!({"pr_number":"1","head_sha":"a".repeat(40)}).to_string()
        )
        .is_err()
    );
    step(&mut sim, "ResourceDeliveryConfigured", callback(1));
    assert!(
        sim.step(
            "effort",
            "MergeResourceDelivery",
            &json!({"pr_number":"1","head_sha":"b".repeat(40)}).to_string()
        )
        .is_err()
    );
    step(
        &mut sim,
        "MergeResourceDelivery",
        json!({"pr_number":"1","head_sha":"a".repeat(40)}),
    );
    assert!(
        sim.step("effort", "ResourceDeliveryMerged", &callback(1).to_string())
            .is_err()
    );
    step(&mut sim, "ResourceDeliveryMerged", callback(2));
    assert!(sim.step("effort", "MarkDeployVerified", "{}").is_err());
    step(&mut sim, "VerifyResourceDelivery", json!({}));
    let mut stale = callback(2);
    stale["resource_delivery_evidence"] = json!("old evidence");
    assert!(
        sim.step("effort", "ResourceDeliveryVerified", &stale.to_string())
            .is_err()
    );
    let mut verified = callback(3);
    verified["resource_delivery_evidence"] = json!("exact resource evidence");
    step(&mut sim, "ResourceDeliveryVerified", verified);
    sim.assert_status("effort", "Verified");
}
#[test]
fn legacy_deploy_path_still_verifies_and_cannot_use_resource_checker() {
    let mut sim = proving();
    step(
        &mut sim,
        "ConfigureDeploy",
        json!({"computer_id":"computer-1","image_tag":"ghcr.io/owner/paw:head","deploy_max_checks":"60","probe_id":"probe-1"}),
    );
    step(
        &mut sim,
        "Merge",
        json!({"pr_number":"1","head_sha":"a".repeat(40)}),
    );
    assert!(sim.step("effort", "VerifyResourceDelivery", "{}").is_err());
    step(&mut sim, "MarkDeployVerified", json!({}));
    sim.assert_status("effort", "Verified");
}
#[test]
fn reconfiguration_and_retry_refuse_old_callbacks_and_do_not_skip_a_gate() {
    let mut sim = proving();
    step(&mut sim, "ConfigureResourceDelivery", configuration());
    step(&mut sim, "ConfigureResourceDelivery", configuration());
    assert!(
        sim.step(
            "effort",
            "ResourceDeliveryConfigured",
            &callback(1).to_string()
        )
        .is_err()
    );
    step(&mut sim, "ResourceDeliveryConfigured", callback(2));
    step(
        &mut sim,
        "MergeResourceDelivery",
        json!({"pr_number":"1","head_sha":"a".repeat(40)}),
    );
    step(&mut sim, "ResourceDeliveryMerged", callback(3));
    step(&mut sim, "VerifyResourceDelivery", json!({}));
    let mut pending = callback(4);
    pending["error_message"] = json!("resource pending");
    step(&mut sim, "ResourceDeliveryPending", pending);
    step(&mut sim, "VerifyResourceDelivery", json!({}));
    let mut stale = callback(4);
    stale["resource_delivery_evidence"] = json!("old success");
    assert!(
        sim.step("effort", "ResourceDeliveryVerified", &stale.to_string())
            .is_err()
    );
    sim.assert_status("effort", "ResourceVerifying");
}

#[tokio::test]
async fn native_intent_accept_seeds_strict_effort_and_legacy_merge_creates_temper_deploy() {
    use temper_runtime::{ActorSystem, tenant::TenantId};
    use temper_server::{
        registry::SpecRegistry,
        request_context::AgentContext,
        state::{DispatchCommand, ServerState},
    };
    let directory =
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../os-apps/paw-patrol/specs");
    let xml = fs::read_to_string(directory.join("model.csdl.xml")).unwrap();
    let sources: Vec<_> = [
        ("Intent", "intent"),
        ("Effort", "effort"),
        ("TemperDeploy", "temper_deploy"),
        ("ReviewRun", "review_run"),
        ("ProofPacket", "proof_packet"),
    ]
    .into_iter()
    .map(|(name, file)| {
        (
            name,
            github_fixture_boundary(
                &fs::read_to_string(directory.join(format!("{file}.ioa.toml"))).unwrap(),
            ),
        )
    })
    .collect();
    let mut registry = SpecRegistry::new();
    registry.register_tenant(
        "default",
        temper_spec::csdl::parse_csdl(&xml).unwrap(),
        xml,
        &sources
            .iter()
            .map(|(name, source)| (*name, source.as_str()))
            .collect::<Vec<_>>(),
    );
    let server = ServerState::from_registry(ActorSystem::new("effort-producer-contract"), registry);
    server.rebuild_reaction_dispatcher();
    server
        .authz
        .reload_tenant_policies(
            "default",
            &fs::read_to_string(directory.join("../policies/patrol.cedar")).unwrap(),
        )
        .unwrap();
    let tenant = TenantId::from("default".to_owned());
    let context = AgentContext::for_service("patrol-intake-service");
    for module in [
        "chain_review_ready",
        "chain_proof_ready",
        "chain_merge_ready",
    ] {
        let bytes = fs::read(directory.join(format!("../wasm/{module}/{module}.wasm")))
            .expect("Build paw-patrol gate WASMs before this proof");
        let hash = server.wasm_engine.compile_and_cache(&bytes).unwrap();
        server
            .wasm_module_registry
            .write()
            .unwrap()
            .register(&tenant, module, &hash);
    }

    server
        .get_or_create_tenant_entity(&tenant, "Intent", "intent-1", json!({}))
        .await
        .unwrap();
    let dispatch = |entity, id, action, params| {
        server.dispatch(DispatchCommand {
            tenant: &tenant,
            entity_type: entity,
            entity_id: id,
            action,
            params,
            agent_ctx: &context,
            await_integration: false,
            await_reactions: true,
        })
    };

    server
        .get_or_create_tenant_entity(&tenant, "ReviewRun", "review-1", json!({}))
        .await
        .unwrap();
    let result=dispatch("ReviewRun","review-1","IngestRecord",json!({"commit":"a".repeat(40),"reviewers_ran":"[\"codex\",\"grok\",\"fable\"]","findings":"[]","risk":"{}","open_act_on_count":"0"})).await.unwrap();
    assert!(result.success, "{:?}", result.error);
    server
        .get_or_create_tenant_entity(&tenant, "ProofPacket", "proof-1", json!({}))
        .await
        .unwrap();
    let result=dispatch("ProofPacket","proof-1","IngestProof",json!({"commit":"a".repeat(40),"changed_surface":"[\"delivery\"]","blast_radius":"[]","features":"[{\"key\":\"delivery\",\"verification\":\"rerun\",\"verdict\":\"pass\"}]","tests":"{\"result\":\"pass\"}","independent_verifier":"{\"agrees\":true,\"reran\":[\"delivery\"]}"})).await.unwrap();
    assert!(result.success, "{:?}", result.error);
    for (action, params) in [
        (
            "Submit",
            json!({"source":"chat","request_text":"deliver","requester_id":"user"}),
        ),
        (
            "Triage",
            json!({"triage_summary":"accepted scope","task_summary":"delivery","task_detail":"API and web","risk_lane":"L1","repo":"owner/repo","branch":"codex/delivery","intent_ref":"docs/efforts/ARN-467/intent.md"}),
        ),
        (
            "AttachIntentFile",
            json!({"intent_ref":"docs/efforts/ARN-467/intent.md"}),
        ),
        ("Accept", json!({"factory_case_id":"case-1"})),
    ] {
        if action == "AttachIntentFile" {
            prove_github_file("intent_ref", "docs/efforts/ARN-467/intent.md").await;
        }
        let result = dispatch("Intent", "intent-1", action, params)
            .await
            .unwrap();
        assert!(result.success, "{action}: {:?}", result.error);
    }
    let efforts = server.list_entity_ids(&tenant, "Effort");
    assert_eq!(efforts.len(), 1);
    let id = &efforts[0];
    let row = server
        .get_tenant_entity_state(&tenant, "Effort", id)
        .await
        .unwrap();
    assert_eq!(row.state.fields["intent_id"], "intent-1");
    assert_eq!(row.state.fields["repo"], "owner/repo");
    assert_eq!(row.state.fields["risk_lane"], "L1");
    for (action, params) in [
        (
            "AttachSpec",
            json!({"spec_ref":"docs/efforts/ARN-467/spec.md"}),
        ),
        ("Specify", json!({})),
        (
            "AttachPlan",
            json!({"plan_ref":"docs/efforts/ARN-467/plan.md"}),
        ),
        ("Plan", json!({"plan_summary":"deliver"})),
        ("StartBuild", json!({})),
        ("WorkerDone", json!({})),
        ("SubmitForReview", json!({})),
        (
            "AttachReviewRun",
            json!({"reviewer_run_id":"review-1","review_run_ids":"review-1"}),
        ),
        ("MarkFixItClear", json!({})),
        ("MarkRiskClear", json!({})),
        ("PassReview", json!({"reviewer_run_id":"review-1"})),
        ("ReportE2e", json!({"e2e_summary":"live proof"})),
        (
            "PassEvaluation",
            json!({"evaluation_run_id":"evaluation-1"}),
        ),
        (
            "AttachProofPacket",
            json!({"proof_packet_id":"proof-1","proof_packet_ids":"proof-1"}),
        ),
        (
            "AttachDecisions",
            json!({"decisions_ref":"docs/efforts/ARN-467/decisions.md"}),
        ),
        (
            "ConfigureDeploy",
            json!({"computer_id":"computer-1","image_tag":"ghcr.io/owner/paw:head","deploy_max_checks":"60","probe_id":"probe-1"}),
        ),
        ("Merge", json!({"pr_number":"1","head_sha":"a".repeat(40)})),
    ] {
        for (attach, field, path) in [
            ("AttachSpec", "spec_ref", "docs/efforts/ARN-467/spec.md"),
            ("AttachPlan", "plan_ref", "docs/efforts/ARN-467/plan.md"),
            (
                "AttachDecisions",
                "decisions_ref",
                "docs/efforts/ARN-467/decisions.md",
            ),
        ] {
            if action == attach {
                prove_github_file(field, path).await;
            }
        }
        let result = dispatch("Effort", id, action, params).await.unwrap();
        assert!(result.success, "{action}: {:?}", result.error);
    }
    let deployments = server.list_entity_ids(&tenant, "TemperDeploy");
    assert_eq!(deployments.len(), 1);
    let deployment = server
        .get_tenant_entity_state(&tenant, "TemperDeploy", &deployments[0])
        .await
        .unwrap();
    assert_eq!(deployment.state.fields["effort_id"], id.as_str());
    assert_eq!(
        deployment.state.fields["image_tag"],
        "ghcr.io/owner/paw:head"
    );
    assert_eq!(deployment.state.fields["expected_sha"], "a".repeat(40));
}

// ServerState's production host has no external HTTP fixture injection. Keep its
// real actors/reactions, but drive this external boundary through WasmEngine with
// recorded HTTP responses before each Attach action. No GitHub network proof is
// claimed by this deterministic test.
fn github_fixture_boundary(source: &str) -> String {
    let mut output = String::new();
    for (index, chunk) in source.split("[[action.triggers]]").enumerate() {
        if index == 0 {
            output.push_str(chunk);
            continue;
        }
        let end = chunk.find("\n[[action]]").unwrap_or(chunk.len());
        if chunk[..end].contains("module = \"chain_github_ready\"") {
            output.push_str(&chunk[end..]);
        } else {
            output.push_str("[[action.triggers]]");
            output.push_str(chunk);
        }
    }
    output
}
async fn prove_github_file(field: &str, path: &str) {
    use std::collections::BTreeMap;
    use std::sync::RwLock;
    use temper_wasm::{
        SimWasmHost, StreamRegistry, WasmEngine, WasmInvocationContext, WasmResourceLimits,
    };
    let bytes = fs::read(
        PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../../os-apps/paw-patrol/wasm/chain_github_ready/chain_github_ready.wasm"),
    )
    .unwrap();
    let engine = WasmEngine::new().unwrap();
    let hash = engine.compile_and_cache(&bytes).unwrap();
    let ctx = WasmInvocationContext {
        tenant: "default".into(),
        entity_type: "Effort".into(),
        entity_id: "fixture".into(),
        trigger_action: "Attach".into(),
        wasm_module: Some("chain_github_ready".into()),
        trigger_params: json!({}),
        entity_state: json!({"fields":{field:path,"repo":"owner/repo","branch":"codex/delivery"}}),
        agent_id: None,
        session_id: None,
        integration_config: BTreeMap::from([
            ("path_field".into(), field.into()),
            ("github_token".into(), "fixture-only".into()),
        ]),
        trace_id: String::new(),
        workflow_root_entity_type: None,
        workflow_root_entity_id: None,
        workflow_run_id: None,
        http_request: None,
    };
    for present in [false, true] {
        let host = SimWasmHost::new()
            .with_default_response(500, "unexpected request")
            .with_response(
                "https://api.github.com/repos/owner/repo",
                200,
                "{\"id\":42,\"full_name\":\"owner/repo\"}",
            )
            .with_response(
                &format!(
                    "https://api.github.com/repos/owner/repo/contents/{path}?ref=codex%2Fdelivery"
                ),
                if present { 200 } else { 404 },
                "{\"type\":\"file\",\"sha\":\"fixture-blob-sha\"}",
            );
        let result = engine
            .invoke(
                &hash,
                &ctx,
                Arc::new(host),
                &WasmResourceLimits::default(),
                Arc::new(RwLock::new(StreamRegistry::default())),
            )
            .await
            .unwrap();
        assert_eq!(result.success, present, "{result:?}");
    }
}

#[tokio::test]
async fn production_effort_http_verbs_cannot_fabricate_delivery_state() {
    use axum::{
        body::Body,
        http::{Request, StatusCode},
    };
    use temper_authz::AuthenticatedRequestContext;
    use temper_runtime::{ActorSystem, tenant::TenantId};
    use temper_server::{
        build_router,
        registry::{EntityVerificationResult, SpecRegistry, VerificationStatus},
        request_context::AgentContext,
        state::ServerState,
    };
    use tower::ServiceExt;
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../os-apps/paw-patrol");
    let xml = fs::read_to_string(root.join("specs/model.csdl.xml")).unwrap();
    let mut registry = SpecRegistry::new();
    registry.register_tenant(
        "default",
        temper_spec::csdl::parse_csdl(&xml).unwrap(),
        xml,
        &[("Effort", source().as_str())],
    );
    registry.set_verification_status(
        &TenantId::default(),
        "Effort",
        VerificationStatus::Completed(EntityVerificationResult {
            all_passed: true,
            levels: vec![],
            verified_at: "2026-09-06T00:00:00Z".into(),
        }),
    );
    let state = ServerState::from_registry(ActorSystem::new("effort-http-contract"), registry);
    state
        .authz
        .reload_tenant_policies(
            "default",
            &fs::read_to_string(root.join("policies/patrol.cedar")).unwrap(),
        )
        .unwrap();
    let ctx = AgentContext::for_service("patrol-intake-service")
        .security_ctx
        .unwrap();
    for (method, path, body, status) in [
        (
            "POST",
            "/tdata/Efforts",
            json!({"id":"forged","ResourceDeliveryMerged":true}),
            StatusCode::BAD_REQUEST,
        ),
        (
            "POST",
            "/tdata/Efforts",
            json!({"id":"effort-http"}),
            StatusCode::CREATED,
        ),
        (
            "PATCH",
            "/tdata/Efforts('effort-http')",
            json!({"DeployVerified":true}),
            StatusCode::FORBIDDEN,
        ),
        (
            "PUT",
            "/tdata/Efforts('effort-http')",
            json!({"Status":"Verified"}),
            StatusCode::FORBIDDEN,
        ),
        (
            "PUT",
            "/tdata/Efforts('effort-http')",
            json!({"ResourceDeliveryPlan":"forged"}),
            StatusCode::FORBIDDEN,
        ),
        (
            "DELETE",
            "/tdata/Efforts('effort-http')",
            json!({}),
            StatusCode::FORBIDDEN,
        ),
    ] {
        let mut request = Request::builder()
            .method(method)
            .uri(path)
            .header("content-type", "application/json")
            .body(Body::from(body.to_string()))
            .unwrap();
        let request_ctx = ctx.clone();
        request
            .extensions_mut()
            .insert(AuthenticatedRequestContext::new(
                TenantId::default(),
                request_ctx,
            ));
        let response = build_router(state.clone()).oneshot(request).await.unwrap();
        let actual_status = response.status();
        let body = axum::body::to_bytes(response.into_body(), 8192)
            .await
            .unwrap();
        assert_eq!(
            actual_status,
            status,
            "{method} {path}: {}",
            String::from_utf8_lossy(&body)
        );
    }
    // A separate explicit grant tests the state-machine boundary after Cedar.
    // Even a policy that allows generic writes cannot change this strict entity.
    state.authz.reload_tenant_policies("default", "permit(principal, action in [Action::\"update\", Action::\"delete\"], resource is Effort);").unwrap();
    for method in ["PATCH", "PUT", "DELETE"] {
        let mut request = Request::builder()
            .method(method)
            .uri("/tdata/Efforts('effort-http')")
            .header("content-type", "application/json")
            .body(Body::from(json!({"DeployVerified":true}).to_string()))
            .unwrap();
        request
            .extensions_mut()
            .insert(AuthenticatedRequestContext::new(
                TenantId::default(),
                ctx.clone(),
            ));
        let response = build_router(state.clone()).oneshot(request).await.unwrap();
        let status = response.status();
        let bytes = axum::body::to_bytes(response.into_body(), 8192)
            .await
            .unwrap();
        assert_eq!(
            status,
            StatusCode::METHOD_NOT_ALLOWED,
            "{method}: {}",
            String::from_utf8_lossy(&bytes)
        );
        let body: Value = serde_json::from_slice(&bytes).unwrap();
        assert_eq!(body["error"]["code"], "StrictActionContract");
    }
    assert!(!state.entity_exists(&TenantId::default(), "Effort", "forged"));
    let actual = state
        .get_tenant_entity_state(&TenantId::default(), "Effort", "effort-http")
        .await
        .unwrap();
    assert_eq!(actual.state.status, "Intended");
    assert_ne!(
        actual.state.fields.get("deploy_verified"),
        Some(&json!(true))
    );
}

#[tokio::test]
async fn packaged_resource_delivery_gates_return_fenced_callbacks_and_verify_both_resources() {
    use sha2::{Digest, Sha256};
    use std::{collections::BTreeMap, sync::RwLock};
    use temper_wasm::{
        SimWasmHost, StreamRegistry, WasmEngine, WasmInvocationContext, WasmResourceLimits,
    };
    let sha = "a".repeat(40);
    let expected = |entity: &str, id: &str| json!({"entity_type":entity,"resource_id":id,"action":"Deploy","operation_key":format!("op-{id}"),"operation_sequence":1,"revision":sha,"configuration_sha256":format!("{:x}",Sha256::digest(b"{}")),"proof_ref":format!("proof-{id}")});
    let plan=json!({"operations":[expected("DsfRailwayServiceInstance","api"),expected("DsfVercelProject","web")]}).to_string();
    let row = json!({"status":"ResourceVerifying","resource_delivery_plan":plan,"resource_delivery_head":sha,"delivery_sequence":3,"head_sha":sha,"resource_delivery_merged":true,"deploy_configured":false});
    let resource = |id: &str| json!({"status":"Active","operation_verified":true,"deploy_verified":true,"effort_id":"effort-1","operation_key":format!("op-{id}"),"operation_sequence":1,"request_revision":sha,"request_configuration":"{}","proof_ref":format!("proof-{id}"),"verified_resource_id":id,"verified_revision":sha,"provider_evidence_ref":"https://provider.test/deploy","flow_evidence_ref":"https://deep-sci-fi.world/probe","telemetry_evidence_ref":"https://app.datadoghq.com/apm/trace/test"});
    let engine = WasmEngine::new().unwrap();
    for (stage, failure) in [
        ("validate", "ResourceDeliveryRejected"),
        ("merge", "ResourceDeliveryMergeRejected"),
        ("verify", "ResourceDeliveryPending"),
    ] {
        let module = format!("effort_resource_delivery_{stage}");
        let bytes = fs::read(PathBuf::from(env!("CARGO_MANIFEST_DIR")).join(format!(
            "../../os-apps/paw-patrol/wasm/{module}/{module}.wasm"
        )))
        .unwrap();
        let hash = engine.compile_and_cache(&bytes).unwrap();
        let ctx = WasmInvocationContext {
            tenant: "default".into(),
            entity_type: "Effort".into(),
            entity_id: "effort-1".into(),
            trigger_action: "Fixture".into(),
            wasm_module: Some(module),
            trigger_params: json!({}),
            entity_state: json!({"fields":row}),
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
        };
        for valid in [false, true] {
            if valid && stage != "verify" {
                continue;
            }
            let host = if valid {
                SimWasmHost::new()
                    .with_default_response(500, "unexpected request")
                    .with_response(
                        "https://temper.test/tdata/Efforts('effort-1')",
                        200,
                        &row.to_string(),
                    )
                    .with_response(
                        "https://temper.test/tdata/DsfRailwayServiceInstances('api')",
                        200,
                        &resource("api").to_string(),
                    )
                    .with_response(
                        "https://temper.test/tdata/DsfVercelProjects('web')",
                        200,
                        &resource("web").to_string(),
                    )
            } else {
                SimWasmHost::new().with_default_response(503, "unavailable")
            };
            let result = engine
                .invoke(
                    &hash,
                    &ctx,
                    Arc::new(host),
                    &WasmResourceLimits::default(),
                    Arc::new(RwLock::new(StreamRegistry::default())),
                )
                .await
                .unwrap();
            assert!(result.success, "{result:?}");
            assert_eq!(
                result.callback_action,
                if valid {
                    "ResourceDeliveryVerified"
                } else {
                    failure
                },
                "{result:?}"
            );
            assert_eq!(result.callback_params["expected_delivery_plan"], plan);
            assert_eq!(result.callback_params["expected_delivery_head"], sha);
            assert_eq!(result.callback_params["expected_delivery_sequence"], 3);
            assert!(!result.callback_params.to_string().contains("fixture-only"));
            if valid {
                let evidence: Value = serde_json::from_str(
                    result.callback_params["resource_delivery_evidence"]
                        .as_str()
                        .unwrap(),
                )
                .unwrap();
                assert_eq!(evidence.as_array().unwrap().len(), 2);
            }
        }
    }
}

#[test]
fn every_declared_effort_producer_matches_strict_parameters() {
    let script = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../os-apps/paw-patrol/wasm/effort_resource_delivery/audit_callers.py");
    assert!(
        std::process::Command::new("python3")
            .arg(script)
            .status()
            .unwrap()
            .success()
    );
}
