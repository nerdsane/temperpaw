use temper_wasm_sdk::prelude::*;
// Each phase includes the shared evaluator contract but uses only its own subset.
#[allow(dead_code, unused_imports)]
mod core {
    include!("../../semantic_core.rs");
}
mod outlook {
    include!("../../semantic_outlook.rs");
}
// The producer projects aliases; the consumer resolves them. Both share one mapping.
#[allow(dead_code)]
mod references {
    include!("../../semantic_references.rs");
}
fn identifier(v: &Value) -> Result<&str, String> {
    let id = v
        .as_str()
        .filter(|s| !s.is_empty() && s.len() < 100)
        .ok_or("Missing generated identity")?;
    if !id
        .bytes()
        .all(|b| b.is_ascii_alphanumeric() || b"-_".contains(&b))
    {
        return Err("Invalid generated identity".into());
    }
    Ok(id)
}
fn generated_node(
    v: &Value,
    kind: &str,
    known: &std::collections::BTreeSet<String>,
) -> Result<Value, String> {
    let id = identifier(&v["id"])?;
    let statement = v["statement"]
        .as_str()
        .filter(|s| !s.trim().is_empty() && s.len() < 2000)
        .ok_or("Invalid hypothetical statement")?;
    let deps = v["requires"].as_array().ok_or("Missing prerequisites")?;
    if deps.len() > 12 {
        return Err("Too many generated prerequisites".into());
    }
    let mut edges = vec![];
    for dep in deps {
        let target = identifier(dep)?;
        if !known.contains(target) {
            return Err(format!(
                "Generated prerequisite {target} is not in frozen evidence"
            ));
        }
        edges.push(json!({"kind":"requires","to_id":target}));
    }
    Ok(
        json!({"Id":id,"statement":statement,"kind":kind,"Status":"Hypothesis","provenance":"generated_hypothesis","edges":serde_json::to_string(&edges).unwrap(),"source_refs":"[]","evidence_note":v["evidence_note"],"signal":v["signal"],"falsifier":v["falsifier"],"scene":v["scene"],"parent":v["parent"],"research_question":v["research_question"]}),
    )
}
fn expand(
    snapshot: &mut Value,
    generated: &Value,
    phase: &str,
    program: &Value,
) -> Result<(), String> {
    if !matches!(phase, "seed" | "explore") {
        return Err("Unknown exploration phase".into());
    }
    let mut generated = generated.clone();
    references::References::new(snapshot)?.resolve_generated(&mut generated);
    let hypotheses = generated["hypotheses"]
        .as_array()
        .ok_or("Missing hypotheses")?;
    let reports = generated["research_evidence"]
        .as_array()
        .ok_or("Missing research evidence")?;
    if hypotheses.len() + reports.len() > 128 {
        return Err("Exploration batch exceeds memory budget".into());
    }
    generated["continue_exploring"]
        .as_bool()
        .ok_or("Missing continuation decision")?;
    generated["exploration_note"]
        .as_str()
        .filter(|s| !s.trim().is_empty() && s.len() <= 4000)
        .ok_or("Missing exploration rationale")?;
    if core::field(&snapshot["world"], "hindcast_mode") != "false" && !reports.is_empty() {
        return Err("Frozen world cannot add fresh research".into());
    }
    let nodes = snapshot["nodes"].as_array().ok_or("Missing nodes")?;
    if nodes.len() + hypotheses.len() + reports.len() > core::MAX_NODES {
        return Err("Node budget exceeded".into());
    }
    let mut known: std::collections::BTreeSet<String> = nodes
        .iter()
        .map(|n| core::field(n, "Id").to_owned())
        .collect();
    let mut mapped = std::collections::BTreeMap::new();
    let round = program["round"].as_u64().unwrap_or(0) + 1;
    for v in reports.iter().chain(hypotheses) {
        let local = identifier(&v["id"])?;
        if local.starts_with(references::PREFIX) {
            return Err("Generated identity uses reserved reference namespace".into());
        }
        let id = format!("r{round}-{local}");
        if known.contains(local)
            || mapped.insert(local.to_owned(), id.clone()).is_some()
            || !known.insert(id)
        {
            return Err("Duplicate generated identity".into());
        }
    }
    let resolve = |id: &str| mapped.get(id).cloned().unwrap_or_else(|| id.to_owned());
    let mut hypothesis_ids: std::collections::BTreeSet<String> = nodes
        .iter()
        .filter(|n| matches!(core::field(n, "kind"), "scenario" | "revision"))
        .map(|n| core::field(n, "Id").to_owned())
        .collect();
    for hypothesis in hypotheses {
        hypothesis_ids.insert(resolve(identifier(&hypothesis["id"])?));
    }
    let mut lineage: std::collections::BTreeMap<String, String> = nodes
        .iter()
        .filter_map(|n| {
            n["parent"]
                .as_str()
                .filter(|p| !p.is_empty())
                .map(|parent| (core::field(n, "Id").to_owned(), parent.to_owned()))
        })
        .collect();
    for hypothesis in hypotheses {
        if let Some(parent) = hypothesis["parent"].as_str().filter(|p| !p.is_empty()) {
            let parent = resolve(identifier(&json!(parent))?);
            if !hypothesis_ids.contains(&parent) {
                return Err("Unknown hypothesis parent".into());
            }
            lineage.insert(resolve(identifier(&hypothesis["id"])?), parent);
        }
    }
    // Parent is lineage, not an implicit causal dependency. It may name a new
    // hypothesis in any batch order, but it must never create a lineage cycle.
    for hypothesis in hypotheses {
        let mut current = resolve(identifier(&hypothesis["id"])?);
        let mut visited = std::collections::BTreeSet::new();
        while let Some(parent) = lineage.get(&current) {
            if !visited.insert(current.clone()) {
                return Err("Cyclic hypothesis parent lineage".into());
            }
            current = parent.clone();
        }
    }
    let mut added = vec![];
    for report in reports {
        let statement = report["statement"]
            .as_str()
            .filter(|s| !s.trim().is_empty() && s.len() < 2000)
            .ok_or("Invalid research report")?;
        let url = report["url"]
            .as_str()
            .filter(|s| {
                s.starts_with("https://") && s.len() <= 2000 && !s.chars().any(char::is_whitespace)
            })
            .ok_or("Research needs exact HTTPS source")?;
        let quote = report["quote"]
            .as_str()
            .filter(|s| {
                !s.trim().is_empty()
                    && s.chars().count() <= 200
                    && s.split_whitespace().count() <= 25
            })
            .ok_or("Research excerpt exceeds quotation limit")?;
        added.push(json!({"Id":resolve(identifier(&report["id"])?),"statement":statement,"kind":"research_evidence","Status":"Reported","provenance":"session_research_report","claim_type":report["provenance"],"edges":"[]","source_refs":json!([url]).to_string(),"quote":quote,"observed_at":report["observed_at"],"evidence_note":"Retrieved report, not proof of a future event."}));
    }
    for hypothesis in hypotheses {
        let mut v = hypothesis.clone();
        v["id"] = json!(resolve(identifier(&v["id"])?));
        let requires = hypothesis["requires"]
            .as_array()
            .ok_or("Missing prerequisites")?
            .iter()
            .map(|id| identifier(id).map(|id| json!(resolve(id))))
            .collect::<Result<Vec<_>, _>>()?;
        v["requires"] = json!(requires);
        let parent = hypothesis["parent"]
            .as_str()
            .filter(|s| !s.is_empty())
            .map(resolve);
        v["parent"] = json!(parent);
        let mut node = generated_node(
            &v,
            if parent.is_some() {
                "revision"
            } else {
                "scenario"
            },
            &known,
        )?;
        node["title"] = v["title"].clone();
        node["mechanism"] = v["mechanism"].clone();
        if let Some(parent) = parent {
            node["before_gap"] = program["results"][&parent]["classify_gap"].clone();
            node["operation"] = program["results"][&parent]["choose_next_operation"].clone();
        }
        node["research_status"] = json!(if reports.is_empty() {
            "no_new_sources"
        } else {
            "source_reports_available"
        });
        added.push(node);
    }
    // Append only after every reference and report has validated.
    snapshot["nodes"]
        .as_array_mut()
        .ok_or("Missing nodes")?
        .extend(added);
    Ok(())
}

fn attach_probabilities(answer: &mut Value, program: &Value) -> Result<(), String> {
    if answer["schema"] != "foresight-outlook-v2" {
        return Err("New runs require open outlook v2".into());
    }
    for outcome in answer["outcomes"]
        .as_array_mut()
        .ok_or("Missing outcomes")?
    {
        let id = outcome["hypothesis_id"]
            .as_str()
            .ok_or("Missing evaluated hypothesis")?;
        let probability = program["results"][id]["estimate_likelihood"]
            .as_str()
            .ok_or("Hypothesis has no Jev event estimate")?
            .parse::<f64>()
            .map_err(|_| "Invalid event estimate")?;
        if !probability.is_finite() || !(0.0..=1.0).contains(&probability) {
            return Err("Invalid event probability".into());
        }
        outcome["probability"] = json!(probability);
    }
    Ok(())
}

fn replan(snapshot: &Value, old: &Value, generated: &Value, added: usize) -> Result<Value, String> {
    let mut program = core::plan(snapshot["nodes"].as_array().ok_or("Missing nodes")?)?;
    for key in ["results", "evaluations", "rounds"] {
        if !old[key].is_null() {
            program[key] = old[key].clone();
        }
    }
    program["round"] = json!(old["round"].as_u64().unwrap_or(0) + 1);
    program["continue_exploring"] =
        json!(generated["continue_exploring"].as_bool().unwrap_or(false));
    program["exploration_note"] = generated["exploration_note"].clone();
    let receipt = json!({"round":program["round"],"added_nodes":added,"note":generated["exploration_note"],"continue_exploring":program["continue_exploring"]});
    program["rounds"]
        .as_array_mut()
        .ok_or("Invalid round history")?
        .push(receipt);
    let results = program["results"].clone();
    program["tasks"]
        .as_array_mut()
        .ok_or("Missing tasks")?
        .retain(|t| results[core::field(t, "nodeId")][core::field(t, "function")].is_null());
    Ok(program)
}
fn run_inner(ctx: &Context) -> Result<(), String> {
    let raw = core::field(&ctx.entity_state, "reasoning_result").trim();
    let phase = core::field(&ctx.entity_state, "phase");
    let raw = if raw.starts_with("```") {
        raw.split_once('\n')
            .ok_or("Invalid fenced JSON")?
            .1
            .rsplit_once("```")
            .ok_or("Invalid fenced JSON")?
            .0
    } else {
        raw
    };
    let mut snapshot = core::parse(core::field(&ctx.entity_state, "snapshot_json"))?;
    if phase == "synthesize" {
        let mut answer = core::parse(raw)?;
        references::References::new(&snapshot)?.resolve_generated(&mut answer);
        attach_probabilities(
            &mut answer,
            &core::parse(core::field(&ctx.entity_state, "program_json"))?,
        )?;
        outlook::validate(&answer, &snapshot)?;
        set_success_result(
            "Complete",
            &json!({"answer":answer.to_string(),"finished_at_ms":Context::get_time_millis().to_string()}),
        );
        return Ok(());
    }
    let old = core::parse(core::field(&ctx.entity_state, "program_json"))?;
    let generated = core::parse(raw)?;
    let before = snapshot["nodes"].as_array().ok_or("Missing nodes")?.len();
    expand(&mut snapshot, &generated, phase, &old)?;
    let nodes = snapshot["nodes"].as_array_mut().ok_or("Missing nodes")?;
    for node in nodes.iter_mut().skip(before) {
        node["source_session_id"] = json!(core::field(&ctx.entity_state, "reasoning_session_id"));
    }
    let added = nodes.len() - before;
    let program = replan(&snapshot, &old, &generated, added)?;
    set_success_result(
        "Expanded",
        &json!({"snapshot_json":snapshot.to_string(),"program_json":program.to_string(),"started_at_ms":core::field(&ctx.entity_state,"started_at_ms")}),
    );
    Ok(())
}
#[unsafe(no_mangle)]
pub extern "C" fn run(_: i32, _: i32) -> i32 {
    match Context::from_host().and_then(|ctx| run_inner(&ctx)) {
        Ok(()) => (),
        Err(e) => set_success_result("Fail", &json!({"error_message":e})),
    };
    0
}
#[cfg(test)]
mod tests {
    use super::*;
    fn batch(id: &str) -> Value {
        json!({"hypotheses":[{"id":id,"statement":"A distinct hypothetical event","requires":["e"]}],"research_evidence":[],"continue_exploring":true,"exploration_note":"Explore another mechanism"})
    }
    #[test]
    fn invalid_parent_lineage_rejects_the_whole_batch() {
        let original = json!({"world":{"hindcast_mode":"false"},"nodes":[{"Id":"e","kind":"evidence","edges":"[]"}]});
        for parent in ["unknown", "ref_0001", "h"] {
            let mut snapshot = original.clone();
            let mut generated = batch("h");
            generated["hypotheses"][0]["parent"] = json!(parent);
            assert!(
                expand(&mut snapshot, &generated, "explore", &json!({})).is_err(),
                "parent {parent}"
            );
            assert_eq!(snapshot, original);
        }
        let mut snapshot = original.clone();
        let mut generated = batch("h");
        generated["hypotheses"][0]["parent"] = json!("other");
        generated["hypotheses"]
            .as_array_mut()
            .unwrap()
            .push(json!({"id":"other","statement":"Another future","requires":[],"parent":"h"}));
        assert!(
            expand(&mut snapshot, &generated, "explore", &json!({}))
                .unwrap_err()
                .contains("Cyclic")
        );
        assert_eq!(snapshot, original);
    }

    #[test]
    fn same_batch_parent_resolves_exactly_even_when_child_appears_first() {
        let mut snapshot = json!({"world":{"hindcast_mode":"false"},"nodes":[{"Id":"existing-hypothesis","kind":"scenario","statement":"Original","edges":"[]"}]});
        let generated = json!({"hypotheses":[
            {"id":"hyp_ai_feature_geofencing_0148","statement":"AI feature providers restrict regions","requires":["hyp_eu_ai_scope_0147"],"parent":"hyp_eu_ai_scope_0147"},
            {"id":"hyp_eu_ai_scope_0147","statement":"AI feature providers face product regulation","requires":["ref_0001"],"parent":"ref_0001"}
        ],"research_evidence":[],"continue_exploring":true,"exploration_note":"A new implication extends a new mechanism"});
        expand(&mut snapshot, &generated, "explore", &json!({"round":14})).unwrap();
        assert_eq!(snapshot["nodes"][1]["parent"], "r15-hyp_eu_ai_scope_0147");
        assert_eq!(snapshot["nodes"][1]["kind"], "revision");
        assert_eq!(snapshot["nodes"][2]["parent"], "existing-hypothesis");
        let plan = core::plan(snapshot["nodes"].as_array().unwrap()).unwrap();
        assert_eq!(plan["issues"], json!([]));
        assert_eq!(plan["tasks"][5]["nodeId"], "r15-hyp_eu_ai_scope_0147");
        assert_eq!(
            plan["tasks"][10]["nodeId"],
            "r15-hyp_ai_feature_geofencing_0148"
        );
    }

    #[test]
    fn short_references_resolve_exactly_and_mixed_uuid_still_fails() {
        let a = "en-01a0ba3d-11c5-79f1-b578-741b76950dee";
        let b = "en-01a0ba3d-13e3-7bf1-b2ad-8751edb87e9c";
        let original = json!({"world":{"hindcast_mode":"false"},"nodes":[{"Id":a,"edges":"[]"},{"Id":b,"edges":"[]"}]});
        let mut s = original.clone();
        let mut g = batch("h");
        g["hypotheses"][0]["requires"] = json!(["ref_0001", "ref_0002"]);
        expand(&mut s, &g, "seed", &json!({})).unwrap();
        let edges: Value = serde_json::from_str(s["nodes"][2]["edges"].as_str().unwrap()).unwrap();
        assert_eq!(edges[0]["to_id"], a);
        assert_eq!(edges[1]["to_id"], b);
        let mut invalid = original.clone();
        g["hypotheses"][0]["requires"] = json!(["en-01a0ba3d-13e3-7bf1-b578-741b76950dee"]);
        assert!(expand(&mut invalid, &g, "seed", &json!({})).is_err());
        assert_eq!(invalid, original);
        g["hypotheses"][0]["requires"] = json!(["ref_9999"]);
        assert!(expand(&mut invalid, &g, "seed", &json!({})).is_err());
        g["hypotheses"][0]["requires"] = json!(["ref_0001"]);
        g["hypotheses"][0]["id"] = json!("ref_0002");
        assert!(expand(&mut invalid, &g, "seed", &json!({})).is_err());
    }
    #[test]
    fn parent_alias_preserves_exact_lineage_and_parent_assessment() {
        let mut snapshot = json!({"world":{"hindcast_mode":"false"},"nodes":[{"Id":"e","edges":"[]"},{"Id":"existing-hypothesis","kind":"scenario","edges":"[]"}]});
        let mut generated = batch("new-path");
        generated["hypotheses"][0]["requires"] = json!(["ref_0001"]);
        generated["hypotheses"][0]["parent"] = json!("ref_0002");
        expand(&mut snapshot,&generated,"explore",&json!({"round":1,"results":{"existing-hypothesis":{"classify_gap":"evidence","choose_next_operation":"challenge"}}})).unwrap();
        let revised = &snapshot["nodes"][2];
        assert_eq!(revised["parent"], "existing-hypothesis");
        assert_eq!(revised["kind"], "revision");
        assert_eq!(revised["before_gap"], "evidence");
        assert_eq!(revised["operation"], "challenge");
    }

    #[test]
    fn synthesis_alias_resolves_before_probability_attachment() {
        let snapshot = json!({"nodes":[{"Id":"actual-hypothesis"}]});
        let mut answer = json!({"schema":"foresight-outlook-v2","outcomes":[{"hypothesis_id":"ref_0001","scenario_ids":["ref_0001"]}]});
        references::References::new(&snapshot)
            .unwrap()
            .resolve_generated(&mut answer);
        attach_probabilities(
            &mut answer,
            &json!({"results":{"actual-hypothesis":{"estimate_likelihood":"0.37"}}}),
        )
        .unwrap();
        assert_eq!(answer["outcomes"][0]["hypothesis_id"], "actual-hypothesis");
        assert_eq!(
            answer["outcomes"][0]["scenario_ids"][0],
            "actual-hypothesis"
        );
        assert_eq!(answer["outcomes"][0]["probability"], 0.37);
    }

    #[test]
    fn repeated_rounds_preserve_history_and_evaluate_new_hypotheses() {
        let mut s = json!({"world":{"hindcast_mode":"false"},"nodes":[{"Id":"e","edges":"[]"}]});
        let original = s["nodes"][0].clone();
        let g = batch("h");
        expand(&mut s, &g, "seed", &json!({})).unwrap();
        let mut p = replan(&s, &json!({}), &g, 1).unwrap();
        p["results"] = json!({"e":{"classify_gap":"none","choose_next_operation":"monitor"},"r1-h":{"classify_gap":"evidence","estimate_likelihood":"0.37","evaluate_novelty":"0.8","decision_value":"0.7","choose_next_operation":"connect"}});
        expand(&mut s, &g, "explore", &p).unwrap();
        let next = replan(&s, &p, &g, 1).unwrap();
        assert_eq!(next["round"], 2);
        assert_eq!(next["tasks"].as_array().unwrap().len(), 5);
        assert_eq!(s["nodes"][0], original);
        assert_eq!(next["results"], p["results"]);
    }
    #[test]
    fn empty_research_round_can_continue_without_false_convergence() {
        let s = json!({"nodes":[{"Id":"e","edges":"[]"}]});
        let p = replan(&s, &json!({}), &batch("h"), 0).unwrap();
        assert_eq!(p["continue_exploring"], true);
    }
    #[test]
    fn invented_reference_rejected_atomically() {
        let mut s = json!({"nodes":[{"Id":"e"}]});
        let original = s.clone();
        let mut g = batch("h");
        g["hypotheses"][0]["requires"] = json!(["invented"]);
        assert!(expand(&mut s, &g, "seed", &json!({})).is_err());
        assert_eq!(s, original);
    }
    #[test]
    fn probability_is_actual_jev_event_estimate() {
        let mut a = json!({"schema":"foresight-outlook-v2","outcomes":[{"hypothesis_id":"h","probability":0.99}]});
        attach_probabilities(
            &mut a,
            &json!({"results":{"h":{"estimate_likelihood":"0.37"}}}),
        )
        .unwrap();
        assert_eq!(a["outcomes"][0]["probability"], 0.37);
        assert!(attach_probabilities(&mut a, &json!({})).is_err());
    }
}
