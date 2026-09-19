use temper_wasm_sdk::prelude::*;
mod core {
    include!("../../semantic_core.rs");
}
mod outlook {
    include!("../../semantic_outlook.rs");
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
        let id = format!("r{round}-{local}");
        if known.contains(local)
            || mapped.insert(local.to_owned(), id.clone()).is_some()
            || !known.insert(id)
        {
            return Err("Duplicate generated identity".into());
        }
    }
    let resolve = |id: &str| mapped.get(id).cloned().unwrap_or_else(|| id.to_owned());
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
        let parent = hypothesis["parent"].as_str().filter(|s| !s.is_empty());
        if let Some(parent) = parent {
            if !nodes.iter().any(|n| {
                core::field(n, "Id") == parent
                    && matches!(core::field(n, "kind"), "scenario" | "revision")
            }) {
                return Err("Unknown hypothesis parent".into());
            }
        }
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
            node["before_gap"] = program["results"][parent]["classify_gap"].clone();
            node["operation"] = program["results"][parent]["choose_next_operation"].clone();
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
