use temper_wasm_sdk::prelude::*;
mod core {
    include!("../../semantic_core.rs");
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
        json!({"Id":id,"statement":statement,"kind":kind,"Status":"Hypothesis","provenance":"generated_hypothesis","edges":serde_json::to_string(&edges).unwrap(),"source_refs":"[]","evidence_note":v["evidence_note"],"signal":v["signal"],"falsifier":v["falsifier"],"scene":v["scene"],"parent":v["parent"]}),
    )
}
fn expand(snapshot: &mut Value, generated: &Value, phase: &str) -> Result<(), String> {
    let nodes = snapshot["nodes"].as_array_mut().ok_or("Missing snapshot")?;
    let mut known: std::collections::BTreeSet<String> = nodes
        .iter()
        .map(|n| core::field(n, "Id").to_owned())
        .collect();
    if phase == "seed" {
        let axes = generated["axes"]
            .as_array()
            .filter(|a| a.len() == 4)
            .ok_or("Expected four causal axes")?;
        let evidence = known.clone();
        let mut options = vec![];
        for axis in axes {
            let items = axis["options"]
                .as_array()
                .filter(|a| a.len() == 3)
                .ok_or("Expected three options per axis")?;
            let mut group = vec![];
            for v in items {
                let mut n = generated_node(v, "option", &evidence)?;
                let id = core::field(&n, "Id").to_owned();
                if !known.insert(id.clone()) {
                    return Err("Duplicate generated identity".into());
                }
                n["axis"] = axis["title"].clone();
                group.push(n.clone());
                nodes.push(n);
            }
            options.push(group);
        }
        for a in 0..3 {
            for b in 0..3 {
                for c in 0..3 {
                    for d in 0..3 {
                        let combination = [
                            &options[0][a],
                            &options[1][b],
                            &options[2][c],
                            &options[3][d],
                        ];
                        let id = format!("scenario-{a}{b}{c}{d}");
                        if !known.insert(id.clone()) {
                            return Err("Scenario identity collision".into());
                        }
                        nodes.push(json!({"Id":id,"kind":"scenario","Status":"Hypothesis","provenance":"composed_hypothesis","statement":combination.iter().map(|n|core::field(n,"statement")).collect::<Vec<_>>().join(" Together: "),"edges":serde_json::to_string(&combination.iter().map(|n|json!({"kind":"requires","to_id":core::field(n,"Id")})).collect::<Vec<_>>()).unwrap(),"source_refs":"[]"}));
                    }
                }
            }
        }
    } else {
        let revisions = generated["revisions"]
            .as_array()
            .filter(|v| !v.is_empty() && v.len() <= 8)
            .ok_or("Expected 1–8 deepened futures")?;
        for v in revisions {
            let parent = identifier(&v["parent"])?;
            if !nodes
                .iter()
                .any(|n| core::field(n, "Id") == parent && core::field(n, "kind") == "scenario")
            {
                return Err("Revision parent is not a scenario".into());
            }
            let n = generated_node(v, "revision", &known)?;
            if !known.insert(core::field(&n, "Id").to_owned()) {
                return Err("Duplicate revision identity".into());
            }
            nodes.push(n);
        }
    }
    Ok(())
}
fn run_inner(ctx: &Context) -> Result<(), String> {
    let raw = core::field(&ctx.entity_state, "reasoning_result").trim();
    let phase = core::field(&ctx.entity_state, "phase");
    if phase == "synthesize" {
        if raw.is_empty() {
            return Err("Synthesis returned no answer".into());
        }
        set_success_result(
            "Complete",
            &json!({"answer":raw,"finished_at_ms":Context::get_time_millis().to_string()}),
        );
        return Ok(());
    }
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
    expand(&mut snapshot, &core::parse(raw)?, phase)?;
    let mut program = core::plan(snapshot["nodes"].as_array().ok_or("Missing nodes")?)?;
    // Recursion reuses persisted assessments for unchanged nodes when deepening.
    if phase == "deepen" {
        let old = core::parse(core::field(&ctx.entity_state, "program_json"))?;
        program["results"] = old["results"].clone();
        program["prior_traversal"] =
            json!({"stop_reason":old["stop_reason"],"remaining_calls":old["remaining_calls"]});
        let results = program["results"].clone();
        program["tasks"].as_array_mut().unwrap().retain(|t| {
            results[t["nodeId"].as_str().unwrap()][t["function"].as_str().unwrap()].is_null()
        });
    }
    set_success_result(
        "Expanded",
        &json!({"snapshot_json":snapshot.to_string(),"program_json":program.to_string(),"started_at_ms":if phase=="seed"{Context::get_time_millis().to_string()}else{core::field(&ctx.entity_state,"started_at_ms").to_string()}}),
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
    #[test]
    fn cross_product_builds_81_worlds_without_invented_evidence() {
        let mut s = json!({"nodes":[{"Id":"e","edges":"[]"}]});
        let axes:Vec<_>=(0..4).map(|a|json!({"title":format!("axis {a}"),"options":(0..3).map(|b|json!({"id":format!("a{a}o{b}"),"statement":"hypothesis","requires":["e"]})).collect::<Vec<_>>()})).collect();
        expand(&mut s, &json!({"axes":axes}), "seed").unwrap();
        assert_eq!(s["nodes"].as_array().unwrap().len(), 94);
        assert_eq!(
            s["nodes"]
                .as_array()
                .unwrap()
                .iter()
                .filter(|n| n["kind"] == "scenario")
                .count(),
            81
        );
    }
    #[test]
    fn invented_evidence_reference_is_rejected() {
        assert!(
            generated_node(
                &json!({"id":"x","statement":"hypothesis","requires":["imaginary"]}),
                "option",
                &Default::default()
            )
            .is_err()
        );
    }
}
