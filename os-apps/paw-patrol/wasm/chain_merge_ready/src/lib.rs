//! chain_merge_ready — one concern: Merge head_sha matches Recorded rows
//! that already pass the review and proof doors.
//!
//! Fired by Effort.Merge. GETs review_run_ids and proof_packet_id. Pins
//! every commit to the Merge head_sha param. On any miss, set_error_result
//! so on_failure returns Proving. The ship child is an entity trigger on
//! Merge (TemperDeploy.Request), not this module.
//!
//! Does not dispatch. Does not call GitHub.

use std::collections::{BTreeMap, BTreeSet};

use temper_wasm_sdk::prelude::*;

const MODEL_REVIEWERS: &[&str] = &["grok", "codex", "fable"];
const MIN_MODELS: usize = 2;

#[cfg(not(feature = "library"))]
#[unsafe(no_mangle)]
pub extern "C" fn run(_ctx_ptr: i32, _ctx_len: i32) -> i32 {
    let result = (|| -> Result<(), String> {
        let ctx = Context::from_host()?;
        let fields = ctx.entity_state.get("fields").cloned().unwrap_or(json!({}));
        let head = ctx
            .trigger_params
            .get("head_sha")
            .and_then(|v| v.as_str())
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .ok_or_else(|| "chain_merge_ready: missing head_sha".to_string())?;
        if !is_full_sha(head) {
            return Err("chain_merge_ready: head_sha is not a 40-char sha".to_string());
        }
        let review_ids = string_list(&fields, "review_run_ids");
        let proof_id = str_field(&fields, "proof_packet_id")
            .or_else(|| str_field(&fields, "ProofPacketId"))
            .or_else(|| string_list(&fields, "proof_packet_ids").into_iter().last())
            .ok_or_else(|| "chain_merge_ready: missing proof_packet_id".to_string())?;
        if review_ids.is_empty() {
            return Err("chain_merge_ready: review_run_ids is empty".to_string());
        }
        let base_url = resolve_api_url(&ctx);
        let headers = odata_headers(&ctx);
        let mut runs = Vec::new();
        for id in &review_ids {
            runs.push(get_entity(&ctx, &base_url, &headers, "ReviewRuns", id)?);
        }
        let packet = get_entity(&ctx, &base_url, &headers, "ProofPackets", &proof_id)?;
        review_panel_holds(&runs, Some(head))?;
        proof_packet_holds(&packet, Some(head))?;
        ctx.log("info", &format!("chain_merge_ready: rows hold for {head}"));
        set_success_result("", &json!({ "status": "merge_ready", "head_sha": head }));
        Ok(())
    })();
    if let Err(error) = result {
        set_error_result(&error);
    }
    0
}

pub fn review_panel_holds(runs: &[Value], require_commit: Option<&str>) -> Result<(), String> {
    if runs.is_empty() {
        return Err("chain_merge_ready: no ReviewRuns".to_string());
    }
    let mut commits = BTreeSet::new();
    let mut reviewers = BTreeSet::new();
    for (i, run) in runs.iter().enumerate() {
        let fields = run.get("fields").unwrap_or(run);
        let status = str_field(fields, "status").or_else(|| str_field(fields, "Status"));
        if !matches!(status.as_deref(), Some("Recorded" | "Superseded")) {
            return Err(format!("chain_merge_ready: ReviewRun[{i}] is not Recorded"));
        }
        if !bool_of(fields, "record_present") {
            return Err(format!(
                "chain_merge_ready: ReviewRun[{i}] record_present is false"
            ));
        }
        // Superseded rows retain evidence but no longer vote in the active panel.
        if status.as_deref() == Some("Superseded") {
            continue;
        }
        if bool_of(fields, "fix_it_failed") {
            return Err(format!("chain_merge_ready: ReviewRun[{i}] fix-it failed"));
        }
        let findings = json_field(fields, "findings")?;
        let open = findings
            .as_array()
            .map(|items| {
                items
                    .iter()
                    .filter(|f| {
                        f.get("severity").and_then(|s| s.as_str()) == Some("act-on")
                            && f.get("resolved").and_then(|r| r.as_bool()) != Some(true)
                    })
                    .count()
            })
            .unwrap_or(0);
        if open > 0 {
            return Err(format!(
                "chain_merge_ready: ReviewRun[{i}] has an open act-on"
            ));
        }
        let commit = str_field(fields, "commit")
            .or_else(|| str_field(fields, "Commit"))
            .unwrap_or_default();
        if !is_full_sha(&commit) {
            return Err(format!("chain_merge_ready: ReviewRun[{i}] commit is bad"));
        }
        commits.insert(commit);
        if let Some(reviewer) =
            str_field(fields, "reviewer_id").or_else(|| str_field(fields, "ReviewerId"))
        {
            reviewers.insert(reviewer);
        }
        for reviewer in string_list(fields, "reviewers_ran") {
            reviewers.insert(reviewer);
        }
    }
    if commits.len() != 1 {
        return Err("chain_merge_ready: ReviewRuns do not share one commit".to_string());
    }
    let commit = commits.iter().next().expect("one commit");
    if let Some(required) = require_commit
        && required != commit
    {
        return Err(format!(
            "chain_merge_ready: review commit {commit} != {required}"
        ));
    }
    let models = MODEL_REVIEWERS
        .iter()
        .filter(|m| reviewers.iter().any(|r| r == *m))
        .count();
    if models < MIN_MODELS {
        return Err(format!(
            "chain_merge_ready: {models}/{} model reviewers ran",
            MODEL_REVIEWERS.len()
        ));
    }
    Ok(())
}

pub fn proof_packet_holds(packet: &Value, require_commit: Option<&str>) -> Result<(), String> {
    let fields = packet.get("fields").unwrap_or(packet);
    if str_field(fields, "status")
        .or_else(|| str_field(fields, "Status"))
        .as_deref()
        != Some("Recorded")
    {
        return Err("chain_merge_ready: ProofPacket is not Recorded".to_string());
    }
    if !bool_of(fields, "record_present") {
        return Err("chain_merge_ready: ProofPacket record_present is false".to_string());
    }
    let commit = str_field(fields, "commit")
        .or_else(|| str_field(fields, "Commit"))
        .unwrap_or_default();
    if !is_full_sha(&commit) {
        return Err("chain_merge_ready: ProofPacket commit is bad".to_string());
    }
    if let Some(required) = require_commit
        && required != commit
    {
        return Err(format!(
            "chain_merge_ready: proof commit {commit} != {required}"
        ));
    }
    let changed = str_array(fields, "changed_surface")?;
    if changed.is_empty() {
        return Err("chain_merge_ready: changed_surface is empty".to_string());
    }
    let blast = str_array(fields, "blast_radius")?;
    let features = match json_field(fields, "features")? {
        Value::Array(items) => items,
        _ => return Err("chain_merge_ready: features is not an array".to_string()),
    };
    let mut verification_by_key: BTreeMap<String, String> = BTreeMap::new();
    for f in &features {
        let key = f.get("key").and_then(|v| v.as_str()).unwrap_or("");
        let verification = f.get("verification").and_then(|v| v.as_str()).unwrap_or("");
        if f.get("verdict").and_then(|v| v.as_str()) == Some("fail") {
            return Err(format!("chain_merge_ready: feature '{key}' failed"));
        }
        verification_by_key.insert(key.to_string(), verification.to_string());
    }
    let tests = json_field(fields, "tests")?;
    if tests.get("result").and_then(|v| v.as_str()) != Some("pass") {
        return Err("chain_merge_ready: tests.result is not pass".to_string());
    }
    let iv = json_field(fields, "independent_verifier")?;
    if iv.get("agrees").and_then(|v| v.as_bool()) != Some(true) {
        return Err("chain_merge_ready: independent_verifier.agrees is false".to_string());
    }
    let reran = match iv.get("reran") {
        Some(Value::Array(items)) => items
            .iter()
            .filter_map(|v| v.as_str().map(str::to_string))
            .collect::<Vec<_>>(),
        _ => Vec::new(),
    };
    for key in changed.iter().chain(blast.iter()) {
        match verification_by_key.get(key) {
            Some(v) if v == "rerun" => {}
            _ => {
                return Err(format!("chain_merge_ready: feature '{key}' was not rerun"));
            }
        }
        if !reran.iter().any(|r| r == key) {
            return Err(format!(
                "chain_merge_ready: independent_verifier missed '{key}'"
            ));
        }
    }
    Ok(())
}

fn is_full_sha(commit: &str) -> bool {
    commit.len() == 40
        && commit
            .bytes()
            .all(|b| matches!(b, b'0'..=b'9' | b'a'..=b'f'))
}

fn json_field(fields: &Value, name: &str) -> Result<Value, String> {
    let pascal = pascal(name);
    let raw = fields
        .get(name)
        .or_else(|| fields.get(&pascal))
        .cloned()
        .unwrap_or(json!([]));
    match raw {
        Value::String(s) => {
            serde_json::from_str(&s).map_err(|e| format!("chain_merge_ready: {name}: {e}"))
        }
        other => Ok(other),
    }
}

fn str_array(fields: &Value, name: &str) -> Result<Vec<String>, String> {
    match json_field(fields, name)? {
        Value::Array(items) => Ok(items
            .into_iter()
            .filter_map(|v| v.as_str().map(str::to_string))
            .collect()),
        _ => Ok(Vec::new()),
    }
}

fn string_list(fields: &Value, name: &str) -> Vec<String> {
    json_field(fields, name)
        .ok()
        .and_then(|v| match v {
            Value::Array(items) => Some(
                items
                    .into_iter()
                    .filter_map(|i| i.as_str().map(str::to_string))
                    .collect(),
            ),
            Value::String(s) if !s.is_empty() => Some(vec![s]),
            _ => None,
        })
        .unwrap_or_default()
}

fn str_field(fields: &Value, name: &str) -> Option<String> {
    fields
        .get(name)
        .and_then(|v| v.as_str())
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
}

fn bool_of(fields: &Value, name: &str) -> bool {
    let pascal = pascal(name);
    fields
        .get(name)
        .or_else(|| fields.get(&pascal))
        .and_then(|v| v.as_bool())
        == Some(true)
}

fn pascal(field: &str) -> String {
    field
        .split('_')
        .map(|part| {
            let mut chars = part.chars();
            match chars.next() {
                Some(first) => first.to_uppercase().collect::<String>() + chars.as_str(),
                None => String::new(),
            }
        })
        .collect()
}

#[cfg(not(feature = "library"))]
fn resolve_api_url(ctx: &Context) -> String {
    ctx.config
        .get("temper_api_url")
        .filter(|value| !value.is_empty() && !value.contains("{secret:"))
        .cloned()
        .unwrap_or_else(|| "http://127.0.0.1:3000".to_string())
}

#[cfg(not(feature = "library"))]
fn odata_headers(ctx: &Context) -> Vec<(String, String)> {
    vec![
        ("content-type".to_string(), "application/json".to_string()),
        ("x-tenant-id".to_string(), ctx.tenant.clone()),
        ("x-temper-principal-kind".to_string(), "agent".to_string()),
        ("x-temper-principal-id".to_string(), ctx.entity_id.clone()),
        ("x-temper-agent-type".to_string(), "system".to_string()),
    ]
}

#[cfg(not(feature = "library"))]
fn get_entity(
    ctx: &Context,
    base_url: &str,
    headers: &[(String, String)],
    set: &str,
    id: &str,
) -> Result<Value, String> {
    if id.is_empty() || id.contains('\'') || id.contains('/') {
        return Err(format!("chain_merge_ready: bad {set} id"));
    }
    let url = format!("{}/tdata/{set}('{id}')", base_url.trim_end_matches('/'));
    let resp = ctx.http_call("GET", &url, headers, "")?;
    if resp.status >= 400 {
        return Err(format!(
            "chain_merge_ready: GET {set} {id} HTTP {}",
            resp.status
        ));
    }
    serde_json::from_str(&resp.body).map_err(|e| format!("chain_merge_ready: {set} {id} body: {e}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    const SHA: &str = "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";

    fn run(reviewer: &str) -> Value {
        json!({
            "fields": {
                "status": "Recorded",
                "record_present": true,
                "fix_it_failed": false,
                "commit": SHA,
                "reviewer_id": reviewer,
                "findings": "[]",
            }
        })
    }

    fn packet() -> Value {
        json!({
            "fields": {
                "status": "Recorded",
                "record_present": true,
                "commit": SHA,
                "changed_surface": "[\"door\"]",
                "blast_radius": "[]",
                "features": "[{\"key\":\"door\",\"verification\":\"rerun\",\"verdict\":\"pass\"}]",
                "tests": "{\"result\":\"pass\"}",
                "independent_verifier": "{\"agrees\":true,\"reran\":[\"door\"]}",
            }
        })
    }

    #[test]
    fn merge_holds_when_rows_match_head() {
        let runs = vec![run("grok"), run("codex")];
        assert!(review_panel_holds(&runs, Some(SHA)).is_ok());
        assert!(proof_packet_holds(&packet(), Some(SHA)).is_ok());
    }

    #[test]
    fn merge_rejects_other_head() {
        let other = "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb";
        let runs = vec![run("grok"), run("codex")];
        assert!(review_panel_holds(&runs, Some(other)).is_err());
        assert!(proof_packet_holds(&packet(), Some(other)).is_err());
    }
    fn history_run(status: &str, reviewer: &str, commit: &str, failed: bool) -> Value {
        json!({"fields": {
            "status": status, "record_present": true, "commit": commit,
            "reviewer_id": reviewer, "fix_it_failed": failed,
            "findings": if failed { "[{\"severity\":\"act-on\",\"resolved\":false}]" } else { "[]" }
        }})
    }

    fn current_panel() -> Vec<Value> {
        vec![
            history_run("Recorded", "grok", SHA, false),
            history_run("Recorded", "codex", SHA, false),
            history_run("Recorded", "fable", SHA, false),
        ]
    }

    #[test]
    fn superseded_failed_round_preserves_history_without_blocking_current_panel() {
        let old_sha = "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb";
        let historical = history_run("Superseded", "grok", old_sha, true);
        let unchanged = historical.clone();
        let mut runs = current_panel();
        runs.insert(0, historical);
        assert!(review_panel_holds(&runs, Some(SHA)).is_ok());
        assert_eq!(runs[0], unchanged);
        assert!(review_panel_holds(&runs, Some(old_sha)).is_err());
    }

    #[test]
    fn historical_rows_never_supply_the_active_panel() {
        assert!(review_panel_holds(&[], Some(SHA)).is_err());
        let superseded = history_run("Superseded", "grok", SHA, false);
        assert!(review_panel_holds(std::slice::from_ref(&superseded), Some(SHA)).is_err());
        assert!(
            review_panel_holds(
                &[superseded, history_run("Recorded", "codex", SHA, false)],
                Some(SHA)
            )
            .is_err()
        );
    }

    #[test]
    fn superseded_without_preserved_record_is_rejected() {
        let mut runs = current_panel();
        let mut missing = history_run("Superseded", "grok", SHA, false);
        missing["fields"]["record_present"] = json!(false);
        runs.push(missing);
        assert!(review_panel_holds(&runs, Some(SHA)).is_err());
    }

    #[test]
    fn only_explicit_supersession_retires_history() {
        for status in ["Requested", "Failed", "Recorded", "unknown"] {
            let mut runs = current_panel();
            runs.push(history_run(
                status,
                "grok",
                "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb",
                true,
            ));
            assert!(review_panel_holds(&runs, Some(SHA)).is_err(), "{status}");
        }
    }

    #[test]
    fn active_failure_and_open_findings_remain_blocking() {
        for failed in [true, false] {
            let mut runs = current_panel();
            let mut failure = history_run("Recorded", "grok", SHA, failed);
            failure["fields"]["findings"] = json!("[{\"severity\":\"act-on\",\"resolved\":false}]");
            runs.push(failure);
            assert!(review_panel_holds(&runs, Some(SHA)).is_err());
        }
    }

    #[test]
    fn active_panel_requires_one_valid_commit_and_preserved_records() {
        for (field, value) in [
            ("commit", json!("bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb")),
            ("commit", json!("not-a-sha")),
            ("record_present", json!(false)),
        ] {
            let mut runs = current_panel();
            runs[0]["fields"][field] = value;
            assert!(review_panel_holds(&runs, Some(SHA)).is_err(), "{field}");
        }
    }

    #[test]
    fn repeated_or_unknown_reviewers_do_not_supply_a_majority() {
        let runs = vec![
            history_run("Recorded", "codex", SHA, false),
            history_run("Recorded", "codex", SHA, false),
            history_run("Recorded", "unknown", SHA, false),
        ];
        assert!(review_panel_holds(&runs, Some(SHA)).is_err());
    }
}
