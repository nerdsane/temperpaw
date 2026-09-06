use super::*;
use std::collections::VecDeque;

struct FakeHost {
    responses: VecDeque<Response>,
    requests: Vec<Request>,
}
impl FakeHost {
    fn new(bodies: Vec<Value>) -> Self {
        Self {
            responses: bodies
                .into_iter()
                .map(|v| Response {
                    status: 200,
                    body: v.to_string(),
                })
                .collect(),
            requests: Vec::new(),
        }
    }
}
impl Host for FakeHost {
    fn request(&mut self, request: &Request) -> Result<Response, String> {
        self.requests.push(Request {
            method: request.method,
            url: request.url.clone(),
            headers: request.headers.clone(),
            body: request.body.clone(),
        });
        self.responses
            .pop_front()
            .ok_or("unexpected HTTP call".into())
    }
    fn secret(&mut self, _: &str) -> Result<String, String> {
        Ok("DO_NOT_RECORD_THIS_TOKEN".into())
    }
}
fn config(source: Value, id: &str) -> Config {
    serde_json::from_value(config_json(source, id)).unwrap()
}
fn config_json(source: Value, id: &str) -> Value {
    json!({"provider_id":id,"secret_name":"provider_token","interval_seconds":300,"source":source})
}
fn dd() -> Value {
    json!({"provider":"datadog","site":"datadoghq.com","app_key_secret":"dd_app_key","query":"sum:trace.http.request.hits{service:dsf}.as_count()","window_seconds":300,"max_age_seconds":120})
}
fn sync() -> Value {
    json!({"source_config_ref":"config-1","resource_id":"resource-1","sync_sequence":7,"source_kind":"vercel"})
}
fn resource(provider: &str, id: &str) -> Value {
    json!({"Provider":provider,"ProviderId":id,"ObservedSequence":4})
}
fn vercel() -> Value {
    json!({"id":"prj_1","targets":{"production":{"id":"dpl_1","readyState":"READY","url":"dsf.vercel.app","meta":{"githubCommitSha":"abc123","password":"DO_NOT_RECORD"},"env":{"TOKEN":"DO_NOT_RECORD"}}}})
}

#[test]
fn vercel_uses_provider_commit_and_discards_secrets() {
    let result = parse_source(
        &config(
            json!({"provider":"vercel","team_id":"team-1","target":"production"}),
            "prj_1",
        ),
        &vercel(),
        1_000_000,
    )
    .unwrap();
    assert_eq!(result.revision, "abc123");
    assert!(!result.facts.to_string().contains("DO_NOT_RECORD"));
}
#[test]
fn datadog_empty_series_is_absent_not_zero() {
    let result = parse_source(
        &config(dd(), "api"),
        &json!({"status":"ok","series":[]}),
        1_000_000,
    )
    .unwrap();
    assert_eq!(result.coverage, Coverage::Absent);
    assert_eq!(result.revision, "");
    assert_eq!(result.outcome, "no_numeric_points");
}
#[test]
fn railway_scopes_latest_deployment_to_environment() {
    let result = parse_source(&config(json!({"provider":"railway","environment_id":"prod"}),"svc"),&json!({"data":{"service":{"id":"svc","serviceInstances":{"edges":[{"node":{"environmentId":"preview","latestDeployment":{"id":"wrong","status":"SUCCESS","meta":{"commitHash":"wrong"}}}},{"node":{"environmentId":"prod","latestDeployment":{"id":"right","status":"SUCCESS","meta":{"commitHash":"right"}}}}]}}}}),1_000_000).unwrap();
    assert_eq!(result.revision, "right");
}
#[test]
fn measured_zero_is_distinct_from_null_or_old_data() {
    let cfg = config(dd(), "api");
    let body = |point| json!({"status":"ok","series":[{"metric":"hits","scope":"service:dsf","pointlist":[point]}]});
    assert_eq!(
        parse_source(&cfg, &body(json!([990000, 0])), 1_000_000)
            .unwrap()
            .coverage,
        Coverage::Measured
    );
    assert_eq!(
        parse_source(&cfg, &body(json!([990000, null])), 1_000_000)
            .unwrap()
            .coverage,
        Coverage::Absent
    );
    assert_eq!(
        parse_source(&cfg, &body(json!([800000, 4])), 1_000_000)
            .unwrap()
            .coverage,
        Coverage::Stale
    );
    assert_eq!(
        parse_source(&cfg, &body(json!([600000, 4])), 1_000_000)
            .unwrap()
            .coverage,
        Coverage::Absent
    );
}
#[test]
fn http_failure_records_status_but_not_response_body() {
    let cfg = config_json(
        json!({"provider":"vercel","team_id":"team-1","target":"production"}),
        "prj_1",
    );
    let mut host = FakeHost::new(vec![cfg, resource("vercel", "prj_1")]);
    host.responses.push_back(Response {
        status: 403,
        body: "DO_NOT_RECORD_PRIVATE_BODY".into(),
    });
    let out = collect(
        &mut host,
        "https://temper.example",
        "secret",
        "default",
        "sync-1",
        &sync(),
        1_000_000,
    )
    .unwrap();
    assert_eq!(out.action, "CollectionInaccessible");
    assert!(out.params["summary"].as_str().unwrap().contains("403"));
    assert!(!out.params.to_string().contains("DO_NOT_RECORD"));
    assert!(out.params.get("last_success_at").is_none());
    assert!(out.params.get("observed_configuration").is_none());
}
#[test]
fn binding_mismatch_cannot_read_provider_or_retarget_resource() {
    let mut host = FakeHost::new(vec![config_json(dd(), "api"), resource("railway", "api")]);
    assert!(
        collect(
            &mut host,
            "https://temper.example",
            "secret",
            "default",
            "sync-1",
            &sync(),
            1_000_000
        )
        .unwrap_err()
        .contains("binding mismatch")
    );
    assert_eq!(host.requests.len(), 2);
}
#[test]
fn callback_has_numeric_cas_and_real_inspectable_evidence() {
    let mut host = FakeHost::new(vec![
        config_json(
            json!({"provider":"vercel","team_id":"team-1","target":"production"}),
            "prj_1",
        ),
        resource("vercel", "prj_1"),
        vercel(),
    ]);
    let out = collect(
        &mut host,
        "https://temper.example",
        "secret",
        "default",
        "sync-1",
        &sync(),
        1_000_000,
    )
    .unwrap();
    assert_eq!(out.action, "CollectionSucceeded");
    assert_eq!(out.params["expected_sequence"], 7);
    assert_eq!(out.params["expected_resource_sequence"], 4);
    assert_eq!(out.params["observed_at_ms"], 1_000_000);
    assert_eq!(out.params["observation_id"], "sync-1-7");
    assert_eq!(out.params["evidence_ref"], host.requests[2].url);
    assert_eq!(out.params["observed_revision"], "abc123");
    assert_eq!(out.params["next_due_at"], "1970-01-01T00:21:40.000Z");
    assert!(!out.params.to_string().contains("DO_NOT_RECORD"));
    assert!(out.params.get("resource_id").is_none());
    assert_eq!(host.requests.len(), 3);
    assert!(host.requests.iter().all(|r| r.method == "GET"));
}
#[test]
fn configuration_cannot_inject_endpoints_or_secret_headers() {
    let mut json = config_json(dd(), "api");
    json["endpoint"] = json!("https://attacker.example");
    assert!(serde_json::from_value::<Config>(json).is_err());
    let mut json = dd();
    json["site"] = json!("datadoghq.com@attacker.example");
    assert!(provider_request(&config(json, "api"), 1_000_000).is_err());
    let json =
        json!({"provider":"vercel","team_id":"team-1&endpoint=attacker","target":"production"});
    assert!(provider_request(&config(json, "prj_1"), 1_000_000).is_err());
}
#[test]
fn datadog_request_is_bounded_and_url_encoded() {
    let cfg = config(dd(), "api");
    let mut req = provider_request(&cfg, 1_000_000).unwrap();
    authorize_request(&mut req, &cfg, &mut FakeHost::new(vec![])).unwrap();
    assert!(
        req.url
            .starts_with("https://api.datadoghq.com/api/v1/query?from=700&to=1000&query=sum%3A")
    );
    assert_eq!(req.method, "GET");
    assert_eq!(
        req.headers
            .iter()
            .filter(|(k, _)| k.starts_with("DD-"))
            .count(),
        2
    );
}
#[test]
fn railway_transport_contains_only_a_query_and_bound_variables() {
    let req = provider_request(
        &config(json!({"provider":"railway","environment_id":"env"}), "svc"),
        1_000_000,
    )
    .unwrap();
    assert_eq!(req.url, "https://backboard.railway.com/graphql/v2");
    let body: Value = serde_json::from_str(&req.body).unwrap();
    assert!(body["query"].as_str().unwrap().starts_with("query "));
    assert_eq!(body["variables"]["serviceId"], "svc");
    assert!(!req.body.contains("mutation"));
}
#[test]
fn supabase_and_r2_pick_only_pinned_metadata() {
    let supa = parse_source(&config(json!({"provider":"supabase"}),"project"),&json!([{"id":"other","status":"INACTIVE"},{"id":"project","status":"ACTIVE_HEALTHY","region":"us-east-1","database":{"password":"DO_NOT_RECORD"}}]),1_000_000).unwrap();
    assert_eq!(supa.outcome, "ACTIVE_HEALTHY");
    assert!(!supa.facts.to_string().contains("DO_NOT_RECORD"));
    let r2 = parse_source(&config(json!({"provider":"cloudflare_r2","account_id":"acct"}),"media"),&json!({"success":true,"result":{"name":"media","location":"enam","private":"DO_NOT_RECORD"}}),1_000_000).unwrap();
    assert_eq!(r2.outcome, "exists");
    assert!(!r2.facts.to_string().contains("DO_NOT_RECORD"));
}
#[test]
fn wrong_provider_identity_is_never_success() {
    let cfg = config(
        json!({"provider":"vercel","team_id":"team","target":"production"}),
        "different",
    );
    assert!(parse_source(&cfg, &vercel(), 1_000_000).is_err());
    let cfg = config(
        json!({"provider":"cloudflare_r2","account_id":"acct"}),
        "bucket",
    );
    assert!(
        parse_source(
            &cfg,
            &json!({"success":true,"result":{"name":"other"}}),
            1_000_000
        )
        .is_err()
    );
}

fn operational_snapshot() -> Value {
    let jobs =
        json!({"counts":{"pending":2},"oldest_unfinished_at":null,"jobs":[],"has_more":false});
    json!({
        "snapshot_version":1,"observed_at":"1970-01-01T00:16:30Z","revision":"abc123",
        "service":"deep-sci-fi-backend","environment":"production",
        "schema":{"current_version":"old","expected_version":"new","is_current":false},
        "participant_summary":{"total":201,"agents":200,"humans":1,"active_last_24h":2,"heartbeat_last_24h":1},
        "participants":{"items":[],"next_cursor":"participant-200"},
        "action_queue":jobs,"media":jobs,"notifications":jobs,
        "private_product_content":"DO_NOT_RECORD"
    })
}
#[test]
fn dsf_snapshot_is_paginated_and_does_not_infer_outages() {
    let cfg = config(
        json!({"provider":"dsf_operations","service":"deep-sci-fi-backend","environment":"production","max_age_seconds":120}),
        "deep-sci-fi-backend",
    );
    let parsed = parse_source(&cfg, &operational_snapshot(), 1_000_000).unwrap();
    assert_eq!(parsed.coverage, Coverage::Measured);
    assert_eq!(parsed.outcome, "snapshot_present");
    assert_eq!(parsed.facts["participant_inventory_complete"], false);
    assert_eq!(
        parsed.facts["participants"]["next_cursor"],
        "participant-200"
    );
    assert_eq!(parsed.facts["schema"]["is_current"], false);
    assert_eq!(parsed.facts["notifications"]["counts"]["pending"], 2);
    assert!(!parsed.facts.to_string().contains("DO_NOT_RECORD"));
}
#[test]
fn dsf_snapshot_unknown_version_or_wrong_environment_cannot_be_current() {
    let cfg = config(
        json!({"provider":"dsf_operations","service":"deep-sci-fi-backend","environment":"production","max_age_seconds":120}),
        "deep-sci-fi-backend",
    );
    let mut snapshot = operational_snapshot();
    snapshot["snapshot_version"] = json!(2);
    assert!(parse_source(&cfg, &snapshot, 1_000_000).is_err());
    snapshot["snapshot_version"] = json!(1);
    snapshot["environment"] = json!("staging");
    assert!(parse_source(&cfg, &snapshot, 1_000_000).is_err());
    snapshot["environment"] = json!("production");
    assert_eq!(
        parse_source(&cfg, &snapshot, 1_200_000).unwrap().coverage,
        Coverage::Stale
    );
}
#[test]
fn datadog_unsorted_points_retain_the_evidence_for_latest_timestamp() {
    let parsed = parse_source(
        &config(dd(), "api"),
        &json!({"status":"ok","series":[{"metric":"hits","pointlist":[[990000,4],[800000,2]]}]}),
        1_000_000,
    )
    .unwrap();
    assert_eq!(
        parsed.facts["series"][0]["latest_point"],
        json!([990000, 4.0])
    );
    assert_eq!(parsed.facts["latest_at_ms"], 990000);
}
#[test]
fn absent_provider_credentials_still_materialize_access_evidence() {
    struct MissingSecret(FakeHost);
    impl Host for MissingSecret {
        fn request(&mut self, r: &Request) -> Result<Response, String> {
            self.0.request(r)
        }
        fn secret(&mut self, _: &str) -> Result<String, String> {
            Err("private host diagnostic".into())
        }
    }
    let mut host = MissingSecret(FakeHost::new(vec![
        config_json(dd(), "api"),
        resource("datadog", "api"),
    ]));
    let mut fields = sync();
    fields["source_kind"] = json!("datadog");
    let out = collect(
        &mut host,
        "https://temper.example",
        "secret",
        "default",
        "sync-1",
        &fields,
        1_000_000,
    )
    .unwrap();
    assert_eq!(out.action, "CollectionInaccessible");
    assert_eq!(out.params["outcome"], "credential_unavailable");
    assert!(
        out.params["evidence_ref"]
            .as_str()
            .unwrap()
            .starts_with("https://api.datadoghq.com/")
    );
    assert_eq!(host.0.requests.len(), 2);
    assert!(!out.params.to_string().contains("private host diagnostic"));
}
#[test]
fn github_git_ref_is_one_path_segment_and_commit_metadata_is_redacted() {
    let cfg = config(
        json!({"provider":"github","owner":"org","repository":"repo","git_ref":"feature/topic?bad=1"}),
        "repo-1",
    );
    let req = provider_request(&cfg, 1_000_000).unwrap();
    assert_eq!(
        req.url,
        "https://api.github.com/repos/org/repo/commits/feature%2Ftopic%3Fbad%3D1"
    );
    let parsed = parse_source(&cfg,&json!({"sha":"abc","commit":{"message":"DO_NOT_RECORD","committer":{"email":"DO_NOT_RECORD","date":"2026-09-06T00:00:00Z"},"tree":{"sha":"tree"}}}),1_000_000).unwrap();
    assert_eq!(parsed.revision, "abc");
    assert!(!parsed.facts.to_string().contains("DO_NOT_RECORD"));
}

#[test]
fn dsf_service_selector_must_match_bound_provider_identity() {
    let cfg = config(
        json!({"provider":"dsf_operations","service":"other-service","environment":"production","max_age_seconds":120}),
        "deep-sci-fi-backend",
    );
    assert!(provider_request(&cfg, 1_000_000).is_err());
}
