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
    if nodes.len() + hypotheses.len() + reports.len() > core::MAX_NODES - 6 {
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

fn compose(snapshot: &mut Value, generated: &Value, old: &Value) -> Result<Value, String> {
    let mut generated = generated.clone();
    references::References::new(snapshot)?.resolve_generated(&mut generated);
    let baseline = &generated["baseline"];
    outlook::validate_baseline(baseline, snapshot)?;
    let nodes = snapshot["nodes"].as_array().ok_or("Missing nodes")?;
    let by_id: std::collections::BTreeMap<_, _> =
        nodes.iter().map(|n| (core::field(n, "Id"), n)).collect();
    let worlds = generated["worlds"]
        .as_array()
        .filter(|w| (2..=6).contains(&w.len()))
        .ok_or("Compose two to six worlds")?;
    if nodes.len() + worlds.len() > core::MAX_NODES {
        return Err("World composition exceeds node budget".into());
    }
    let mut added = vec![];
    let mut identities = std::collections::BTreeSet::new();
    for world in worlds {
        let local = identifier(&world["id"])?;
        if local.starts_with(references::PREFIX) {
            return Err("Reserved world identity".into());
        }
        let id = format!("world-{local}");
        if by_id.contains_key(id.as_str()) || !identities.insert(id.clone()) {
            return Err("Duplicate world identity".into());
        }
        for (key, max) in [
            ("title", 100),
            ("statement", 1000),
            ("mechanism", 1200),
            ("scene", 600),
            ("narrative", 1200),
        ] {
            bounded_text(&world[key], max)?;
        }
        for key in ["signals", "falsifiers"] {
            bounded_texts(&world[key], 1, 8, 240)?;
        }
        bounded_texts(&world["what_you_can_do"], 0, 4, 240)?;
        let mut components = std::collections::BTreeSet::new();
        for reference in world["component_ids"]
            .as_array()
            .ok_or("Missing world components")?
        {
            let reference = reference.as_str().ok_or("Invalid world component")?;
            if by_id
                .get(reference)
                .is_none_or(|n| !matches!(core::field(n, "kind"), "scenario" | "revision"))
                || !components.insert(reference)
            {
                return Err("World components must be distinct existing hypotheses".into());
            }
        }
        if components.len() < 2 || components.len() > 12 {
            return Err("World needs two to twelve defining components".into());
        }
        let counters = world["counter_ids"]
            .as_array()
            .ok_or("Missing world counter hypotheses")?;
        let mut counter_ids = std::collections::BTreeSet::new();
        if counters.len() > 12
            || counters.iter().any(|id| {
                let reference = id.as_str().unwrap_or("");
                components.contains(reference)
                    || !counter_ids.insert(reference)
                    || by_id
                        .get(id.as_str().unwrap_or(""))
                        .is_none_or(|n| !matches!(core::field(n, "kind"), "scenario" | "revision"))
            })
        {
            return Err("Unknown counter hypothesis".into());
        }
        let mut node = world.clone();
        node.as_object_mut().ok_or("Invalid world")?.remove("id");
        node["Id"] = json!(id);
        node["kind"] = json!("world");
        node["Status"] = json!("Hypothesis");
        node["provenance"] = json!("composed_world_hypothesis");
        node["edges"] = json!(
            components
                .iter()
                .map(|id| json!({"kind":"requires","to_id":id}))
                .collect::<Vec<_>>()
                .pipe_json()
        );
        added.push(node);
    }
    let mut updated = snapshot.clone();
    updated["nodes"].as_array_mut().unwrap().extend(added);
    let mut program = core::plan(updated["nodes"].as_array().unwrap())?;
    for key in ["results", "evaluations", "round", "rounds", "last_error"] {
        if !old[key].is_null() {
            program[key] = old[key].clone();
        }
    }
    program["tasks"]
        .as_array_mut()
        .unwrap()
        .retain(|t| identities.contains(core::field(t, "nodeId")));
    program["stage"] = json!("worlds");
    program["baseline"] = baseline.clone();
    program["continue_exploring"] = json!(false);
    program["exploration_stop_reason"] = old["stop_reason"].clone();
    // A provider/trace failure cannot be repaired by asking again within this run.
    if matches!(
        old["stop_reason"].as_str(),
        Some("provider_error" | "trace_budget")
    ) {
        program["stop_reason"] = old["stop_reason"].clone();
    }
    *snapshot = updated;
    Ok(program)
}
trait JsonEncode {
    fn pipe_json(self) -> String;
}
impl JsonEncode for Vec<Value> {
    fn pipe_json(self) -> String {
        serde_json::to_string(&self).unwrap()
    }
}
fn bounded_text(value: &Value, max: usize) -> Result<(), String> {
    value
        .as_str()
        .filter(|s| !s.trim().is_empty() && s.chars().count() <= max)
        .map(|_| ())
        .ok_or("Missing or oversized world text".into())
}
fn bounded_texts(value: &Value, min: usize, max: usize, chars: usize) -> Result<(), String> {
    let values = value
        .as_array()
        .filter(|v| (min..=max).contains(&v.len()))
        .ok_or("Invalid world text list")?;
    for value in values {
        bounded_text(value, chars)?;
    }
    Ok(())
}
fn attach_world_probabilities(
    answer: &mut Value,
    program: &Value,
    snapshot: &Value,
) -> Result<(), String> {
    if answer["schema"] != "foresight-worlds-v3" {
        return Err("World runs require world outlook v3".into());
    }
    let nodes = snapshot["nodes"].as_array().ok_or("Missing nodes")?;
    let mut evaluated = 0;
    let outcomes = answer["outcomes"].as_array_mut().ok_or("Missing worlds")?;
    let count = outcomes.len();
    for outcome in outcomes {
        let id = outcome["world_id"]
            .as_str()
            .ok_or("Missing world identity")?;
        let node = nodes
            .iter()
            .find(|n| core::field(n, "Id") == id && n["kind"] == "world")
            .ok_or("Outcome must reference a composed world")?;
        let raw = &program["results"][id]["estimate_likelihood"];
        let probability = if raw.is_null() {
            None
        } else {
            Some(
                raw.as_str()
                    .and_then(|s| s.parse::<f64>().ok())
                    .filter(|p| p.is_finite() && (0.0..=1.0).contains(p))
                    .ok_or("Invalid world likelihood")?,
            )
        };
        if probability.is_some() {
            evaluated += 1;
        }
        outcome["probability"] = json!(probability);
        for key in ["component_ids", "counter_ids"] {
            outcome[key] = node[key].clone();
        }
        outcome["definition"] = node["statement"].clone();
    }
    answer["baseline"] = program["baseline"].clone();
    answer["evaluation_status"] = json!(if evaluated == count && count > 0 {
        "evaluated"
    } else if evaluated > 0 {
        "partial"
    } else {
        "unavailable"
    });
    answer["probability_basis"] = json!("model_implied_world_estimate");
    answer["probability_model"] = json!("overlapping_worlds");
    answer["calibrated"] = json!(false);
    answer["evaluation_note"] = json!(if evaluated == count && count > 0 {
        format!("All {count} worlds were evaluated separately by Jev.")
    } else {
        let reason = match core::field(program, "stop_reason") {
            "provider_error" => format!(
                "Jev could not finish: {}",
                core::field(program, "last_error")
            ),
            "time_budget" => "The available evaluation time ended.".to_owned(),
            "call_budget" => "The available evaluation calls were used.".to_owned(),
            "trace_budget" => "The evaluation record reached its size limit.".to_owned(),
            _ => "The remaining worlds have no whole-world probability estimate.".to_owned(),
        };
        format!("{evaluated} of {count} worlds were evaluated separately by Jev. {reason}")
    });
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
        let program = core::parse(core::field(&ctx.entity_state, "program_json"))?;
        if program["stage"] == "worlds" || answer["schema"] == "foresight-worlds-v3" {
            attach_world_probabilities(&mut answer, &program, &snapshot)?;
        } else {
            if program["stage"] == "exploration" {
                return Err("Compose whole worlds before synthesis".into());
            }
            attach_probabilities(&mut answer, &program)?;
        }
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
    let composed = if phase == "compose" {
        Some(compose(&mut snapshot, &generated, &old)?)
    } else {
        expand(&mut snapshot, &generated, phase, &old)?;
        None
    };
    let nodes = snapshot["nodes"].as_array_mut().ok_or("Missing nodes")?;
    for node in nodes.iter_mut().skip(before) {
        node["source_session_id"] = json!(core::field(&ctx.entity_state, "reasoning_session_id"));
    }
    let added = nodes.len() - before;
    let program = if let Some(program) = composed {
        program
    } else {
        replan(&snapshot, &old, &generated, added)?
    };
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
    fn world_fixture() -> (Value, Value, Value) {
        let snapshot = json!({"world":{"last_ingest_date":"2026-09-19","target_date":"2027-09-19"},"nodes":[{"Id":"e","kind":"evidence","statement":"Observed baseline","edges":"[]"},{"Id":"a","kind":"scenario","statement":"Component A","edges":"[]"},{"Id":"b","kind":"revision","statement":"Component B","edges":"[]"},{"Id":"c","kind":"scenario","statement":"Counter C","edges":"[]"}]});
        let world = json!({"id":"one","title":"A whole world","statement":"A and B occur jointly","mechanism":"A enables B","component_ids":["ref_0002","ref_0003"],"counter_ids":["ref_0004"],"scene":"An imagined day","narrative":"A causes B but C may prevent it","what_you_can_do":[],"signals":["Observe A"],"falsifiers":["Observe C"]});
        let mut second = world.clone();
        second["id"] = json!("two");
        let generated = json!({"baseline":{"as_of":"2026-09-19","observed":[{"claim":"Observed baseline","evidence_ids":["ref_0001"]}],"assumptions":[],"unknowns":[]},"worlds":[world,second]});
        let program = json!({"results":{"a":{"estimate_likelihood":"0.9"},"b":{"estimate_likelihood":"0.8"}},"evaluations":{},"rounds":[],"round":6,"stop_reason":"exploration_converged"});
        (snapshot, generated, program)
    }
    #[test]
    fn worlds_are_evaluated_fresh_and_never_inherit_component_probabilities() {
        let (mut snapshot, generated, old) = world_fixture();
        snapshot["nodes"].as_array_mut().unwrap().push(json!({"Id":"recent","kind":"research_evidence","statement":"Recent extracted claim","quote":"Actual source excerpt","edges":"[]"}));
        let mut program = compose(&mut snapshot, &generated, &old).unwrap();
        assert_eq!(program["tasks"].as_array().unwrap().len(), 4);
        assert!(
            program["tasks"]
                .as_array()
                .unwrap()
                .iter()
                .all(|t| core::field(t, "nodeId").starts_with("world-"))
        );
        assert!(program["results"]["world-one"].is_null());
        let request = core::request(&snapshot, &program).unwrap();
        assert_eq!(request["state"]["counter_hypotheses"][0]["node"]["Id"], "c");
        assert_eq!(request["state"]["source_evidence"][0]["Id"], "e");
        assert_eq!(request["state"]["source_evidence"][1]["Id"], "recent");
        assert_eq!(
            request["state"]["source_evidence"][1]["quote"],
            "Actual source excerpt"
        );
        program["cursor"] = json!(1);
        let request = core::request(&snapshot, &program).unwrap();
        assert!(
            request["questions"]["result"]["instructions"]
                .as_str()
                .unwrap()
                .contains("never average, multiply, inherit")
        );
        let mut answer = json!({"schema":"foresight-worlds-v3","outcomes":[{"world_id":"world-one","probability":0.99},{"world_id":"world-two","probability":0.88}]});
        attach_world_probabilities(&mut answer, &program, &snapshot).unwrap();
        assert!(answer["outcomes"][0]["probability"].is_null());
        assert_eq!(answer["evaluation_status"], "unavailable");
        program["results"]["world-one"] = json!({"estimate_likelihood":"0.23"});
        attach_world_probabilities(&mut answer, &program, &snapshot).unwrap();
        assert_eq!(answer["outcomes"][0]["probability"], 0.23);
        assert_eq!(answer["evaluation_status"], "partial");
        assert_eq!(answer["outcomes"][0]["definition"], "A and B occur jointly");
        assert_eq!(answer["outcomes"][0]["component_ids"], json!(["a", "b"]));
        assert_eq!(
            answer["baseline"]["observed"][0]["evidence_ids"],
            json!(["e"])
        );
        answer["outcomes"][0]["world_id"] = json!("a");
        assert!(attach_world_probabilities(&mut answer, &program, &snapshot).is_err());
    }
    #[test]
    fn composition_rejects_hypothetical_observations_and_unknown_components_atomically() {
        let (snapshot, generated, old) = world_fixture();
        for (key, value) in [
            ("component_ids", json!(["ref_0002", "invented"])),
            ("component_ids", json!(["ref_0002", "ref_0002"])),
            ("counter_ids", json!(["ref_0001"])),
            ("counter_ids", json!(["ref_0004", "ref_0004"])),
            ("counter_ids", json!(["ref_0002"])),
        ] {
            let mut bad = generated.clone();
            bad["worlds"][0][key] = value;
            let mut candidate = snapshot.clone();
            assert!(compose(&mut candidate, &bad, &old).is_err());
            assert_eq!(candidate, snapshot);
        }
        let mut bad = generated.clone();
        bad["baseline"]["observed"][0]["evidence_ids"] = json!(["ref_0002"]);
        assert!(compose(&mut snapshot.clone(), &bad, &old).is_err());
        let mut stopped = old.clone();
        stopped["stop_reason"] = json!("provider_error");
        assert_eq!(
            compose(&mut snapshot.clone(), &generated, &stopped).unwrap()["stop_reason"],
            "provider_error"
        );
    }
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
