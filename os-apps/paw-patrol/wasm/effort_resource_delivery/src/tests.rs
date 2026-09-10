use super::*;
use std::collections::VecDeque;
struct Mock {
    replies: VecDeque<Value>,
    requests: Vec<Request>,
}
impl Host for Mock {
    fn request(&mut self, request: &Request) -> Result<Response, Error> {
        assert_eq!(request.method, "GET");
        self.requests.push(Request {
            method: request.method,
            url: request.url.clone(),
            headers: request.headers.clone(),
            body: request.body.clone(),
        });
        Ok(Response {
            status: 200,
            body: self
                .replies
                .pop_front()
                .expect("unexpected request")
                .to_string(),
        })
    }
    fn secret(&mut self, _: &str) -> Result<String, Error> {
        panic!("no provider credentials")
    }
}
fn runtime(host: &mut Mock) -> Runtime<'_, Mock> {
    Runtime {
        host,
        base: "https://temper.invalid",
        tenant: "default",
        now_ms: 1000,
    }
}
fn expected(kind: &str, id: &str) -> Value {
    json!({"entity_type":kind,"resource_id":id,"action":"Deploy","operation_key":format!("op-{id}"),"operation_sequence":1,"revision":"a".repeat(40),"configuration_sha256":format!("{:x}",Sha256::digest(b"{}")),"proof_ref":format!("proof-{id}")})
}
fn binding() -> Binding {
    Binding::parse("effort-1",&json!({"resource_delivery_plan":json!({"operations":[expected("DsfRailwayServiceInstance","api"),expected("DsfVercelProject","web")]}).to_string(),"resource_delivery_head":"b".repeat(40),"delivery_sequence":3})).unwrap()
}
fn effort(b: &Binding) -> Value {
    json!({"status":"ResourceVerifying","resource_delivery_plan":b.plan,"resource_delivery_head":b.head,"delivery_sequence":b.sequence,"head_sha":b.head,"resource_delivery_merged":true,"deploy_configured":false})
}
fn resource(id: &str) -> Value {
    json!({"status":"Active","operation_verified":true,"deploy_verified":true,"effort_id":"effort-1","operation_key":format!("op-{id}"),"operation_sequence":1,"request_revision":"a".repeat(40),"request_configuration":"{}","proof_ref":format!("proof-{id}"),"verified_resource_id":id,"verified_revision":"a".repeat(40),"provider_evidence_ref":"https://provider.invalid/evidence","flow_evidence_ref":"https://deep-sci-fi.world/probe","telemetry_evidence_ref":"https://app.datadoghq.com/apm/trace/1"})
}
#[test]
fn aggregate_requires_both_exact_resource_operations_and_reads_only_temper() {
    let b = binding();
    let mut host = Mock {
        replies: VecDeque::from([effort(&b), resource("api"), resource("web")]),
        requests: vec![],
    };
    let result = verify(&mut runtime(&mut host), &b).unwrap();
    assert_eq!(result.action, "ResourceDeliveryVerified");
    assert_eq!(host.requests.len(), 3);
    assert!(
        host.requests
            .iter()
            .all(|request| request.url.starts_with("https://temper.invalid/tdata/"))
    );
}
#[test]
fn pending_acknowledged_failed_unrelated_and_missing_evidence_cannot_complete() {
    for (field, value) in [
        ("status", json!("DeployVerifying")),
        ("operation_verified", json!(false)),
        ("deploy_verified", json!(false)),
        ("operation_sequence", json!(2)),
        ("operation_key", json!("another")),
        ("effort_id", json!("another")),
        ("request_configuration", json!("{\"changed\":true}")),
        ("proof_ref", json!("other-proof")),
        ("verified_revision", json!("c".repeat(40))),
        ("telemetry_evidence_ref", json!("")),
    ] {
        let b = binding();
        let mut bad = resource("web");
        bad[field] = value;
        let mut host = Mock {
            replies: VecDeque::from([effort(&b), resource("api"), bad]),
            requests: vec![],
        };
        assert!(verify(&mut runtime(&mut host), &b).is_err(), "{field}");
    }
}
#[test]
fn captured_plan_head_and_sequence_and_legacy_path_are_fenced_before_resource_reads() {
    for (field, value) in [
        ("resource_delivery_plan", json!("{}")),
        ("resource_delivery_head", json!("c".repeat(40))),
        ("delivery_sequence", json!(4)),
        ("head_sha", json!("c".repeat(40))),
        ("resource_delivery_merged", json!(false)),
        ("deploy_configured", json!(true)),
    ] {
        let b = binding();
        let mut row = effort(&b);
        row[field] = value;
        let mut host = Mock {
            replies: VecDeque::from([row]),
            requests: vec![],
        };
        assert!(verify(&mut runtime(&mut host), &b).is_err(), "{field}");
        assert_eq!(host.requests.len(), 1);
    }
}
#[test]
fn plan_types_and_actions_come_from_manifest_and_resources_are_unique() {
    let first = expected("DsfRailwayServiceInstance", "api");
    for ops in [
        vec![],
        vec![first.clone(); 9],
        vec![first.clone(), first.clone()],
        vec![expected("DsfResource", "api")],
        vec![{
            let mut v = first.clone();
            v["action"] = json!("RetrySelected");
            v
        }],
    ] {
        assert!(parse_plan(&json!({"operations":ops}).to_string()).is_err());
    }
}

fn packet(head: &str) -> Value {
    json!({"status":"Recorded","effort_id":"effort-1","commit":head,"record_present":true,"artifact_ref":"artifact-1","changed_surface":["delivery"],"blast_radius":[],"features":[{"key":"delivery","verification":"rerun","verdict":"pass"}],"tests":{"result":"pass"},"independent_verifier":{"agrees":true,"reran":["delivery"]}})
}
fn validation_replies(b: &Binding) -> Vec<Value> {
    let mut row = effort(b);
    row["status"] = json!("Proving");
    row["proof_packet_ids"] = json!(["proof-api", "proof-web"]);
    let mut replies = vec![row];
    for (kind, id) in [
        ("DsfRailwayServiceInstance", "api"),
        ("DsfVercelProject", "web"),
    ] {
        replies.extend([
            json!({"status":"Active","allowed_operations":["Deploy"]}),
            packet(&b.head),
            json!({"status":"Ready"}),
            json!({"resource_change":expected(kind,id)}),
        ]);
    }
    replies
}
#[test]
fn configuration_reads_real_resource_permissions_and_exact_linked_proof_artifacts() {
    let b = binding();
    let mut h = Mock {
        replies: validation_replies(&b).into(),
        requests: vec![],
    };
    assert_eq!(
        validate(&mut runtime(&mut h), &b).unwrap().action,
        "ResourceDeliveryConfigured"
    );
    assert_eq!(h.requests.len(), 9);
    for case in 0..6 {
        let mut replies = validation_replies(&b);
        match case {
            0 => replies[1]["allowed_operations"] = json!([]),
            1 => replies[2]["status"] = json!("Draft"),
            2 => replies[2]["commit"] = json!("c".repeat(40)),
            3 => replies[3]["status"] = json!("Uploading"),
            4 => replies[4]["resource_change"]["operation_sequence"] = json!(2),
            _ => replies[0]["proof_packet_ids"] = json!(["proof-api"]),
        }
        let mut h = Mock {
            replies: replies.into(),
            requests: vec![],
        };
        assert!(validate(&mut runtime(&mut h), &b).is_err(), "case {case}");
    }
}
#[test]
fn resource_merge_runs_the_same_recorded_review_and_proof_checks() {
    let b = binding();
    let mut replies = validation_replies(&b);
    let row = &mut replies[0];
    row["status"] = json!("ResourceMergeChecking");
    row["proof_packet_id"] = json!("proof-api");
    row["review_run_ids"] = json!(["review-1"]);
    for flag in [
        "resource_delivery_configured",
        "review_passed",
        "evaluation_passed",
        "proof_attached",
        "e2e_ok",
        "decisions_file_ready",
        "merge_risk_clear",
    ] {
        row[flag] = json!(true);
    }
    let review = json!({"status":"Recorded","record_present":true,"fix_it_failed":false,"findings":[],"commit":b.head,"reviewers_ran":["codex","grok","fable"]});
    replies.insert(1, review);
    replies.insert(2, packet(&b.head));
    let mut h = Mock {
        replies: replies.clone().into(),
        requests: vec![],
    };
    assert_eq!(
        merge(&mut runtime(&mut h), &b).unwrap().action,
        "ResourceDeliveryMerged"
    );
    for case in 0..4 {
        let mut bad = replies.clone();
        match case {
            0 => bad[1]["commit"] = json!("c".repeat(40)),
            1 => bad[1]["fix_it_failed"] = json!(true),
            2 => bad[2]["tests"] = json!({"result":"fail"}),
            _ => bad[0]["merge_risk_clear"] = json!(false),
        }
        let mut h = Mock {
            replies: bad.into(),
            requests: vec![],
        };
        assert!(merge(&mut runtime(&mut h), &b).is_err(), "case {case}");
    }
}
