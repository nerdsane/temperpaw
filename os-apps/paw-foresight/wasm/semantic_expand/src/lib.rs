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
    let allowed: std::collections::BTreeSet<String> = core::deepening_candidates(snapshot, program)
        .iter()
        .map(|candidate| core::field(&candidate["node"], "Id").to_owned())
        .collect();
    let mut revised = std::collections::BTreeSet::new();
    let allow_research = core::field(&snapshot["world"], "hindcast_mode") == "false";
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
        let reports = match generated.get("research_evidence") {
            None => vec![],
            Some(value) => value
                .as_array()
                .ok_or("Research evidence must be an array")?
                .clone(),
        };
        if !allow_research && !reports.is_empty() {
            return Err("Frozen or unknown world mode cannot add fresh research reports".into());
        }
        if reports.len() > 4 {
            return Err("Research round exceeds four source reports".into());
        }
        for report in &reports {
            let id = identifier(&report["id"])?;
            if !known.insert(id.to_owned()) {
                return Err("Duplicate research identity".into());
            }
            let statement = report["statement"]
                .as_str()
                .filter(|s| !s.trim().is_empty() && s.len() < 2000)
                .ok_or("Invalid research report")?;
            let url = report["url"]
                .as_str()
                .filter(|s| {
                    s.starts_with("https://")
                        && s.len() <= 2000
                        && !s.chars().any(char::is_whitespace)
                })
                .ok_or("Research report needs an exact public HTTPS source URL")?;
            let quote = report["quote"]
                .as_str()
                .filter(|s| {
                    !s.trim().is_empty()
                        && s.chars().count() <= 200
                        && s.split_whitespace().count() <= 25
                })
                .ok_or("Research excerpt must contain at most 25 words and 200 characters")?;
            nodes.push(json!({"Id":id,"statement":statement,"kind":"research_evidence","Status":"Reported","provenance":"session_research_report","edges":"[]","source_refs":serde_json::to_string(&vec![url]).unwrap(),"quote":quote,"observed_at":report["observed_at"],"evidence_note":"Reported from read-only session research; not a validation of a future outcome."}));
        }
        let revisions = generated["revisions"]
            .as_array()
            .filter(|v| !v.is_empty() && v.len() <= 8)
            .ok_or("Expected 1–8 deepened futures")?;
        for v in revisions {
            let parent = identifier(&v["parent"])?;
            if !allowed.contains(parent) || !revised.insert(parent.to_owned()) {
                return Err(
                    "Revision parent was not selected for deepening or was repeated".into(),
                );
            }
            if !nodes
                .iter()
                .any(|n| core::field(n, "Id") == parent && core::field(n, "kind") == "scenario")
            {
                return Err("Revision parent is not a scenario".into());
            }
            let question=v["research_question"].as_str().filter(|s|!s.trim().is_empty()&&s.chars().count()<=240).ok_or("Revision must retain its unanswered research question (at most 240 characters)")?;
            let mut n = generated_node(v, "revision", &known)?;
            n["before_gap"] = program["results"][parent]["classify_gap"].clone();
            n["operation"] = program["results"][parent]["choose_next_operation"].clone();
            n["research_question"] = json!(question);
            n["research_status"] = json!(if reports.is_empty() {
                "unresolved"
            } else {
                "source_reports_available"
            });
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
        let answer = core::parse(raw)?;
        outlook::validate(&answer, &snapshot)?;
        set_success_result(
            "Complete",
            &json!({"answer":answer.to_string(),"finished_at_ms":Context::get_time_millis().to_string()}),
        );
        return Ok(());
    }
    expand(
        &mut snapshot,
        &core::parse(raw)?,
        phase,
        &core::parse(core::field(&ctx.entity_state, "program_json"))?,
    )?;
    if phase == "deepen" {
        for node in snapshot["nodes"].as_array_mut().unwrap() {
            if matches!(core::field(node, "kind"), "research_evidence" | "revision") {
                node["source_session_id"] =
                    json!(core::field(&ctx.entity_state, "reasoning_session_id"));
            }
        }
    }
    let mut program = core::plan(snapshot["nodes"].as_array().ok_or("Missing nodes")?)?;
    program["traversal_started_at_ms"] = json!(Context::get_time_millis());
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
    #[test]
    fn deepen_only_accepts_unique_jev_selected_parents() {
        let snapshot = json!({"nodes":[{"Id":"e","edges":"[]"},{"Id":"a","kind":"scenario","edges":"[]"},{"Id":"b","kind":"scenario","edges":"[]"}]});
        let program = json!({"results":{"a":{"choose_next_operation":"repair"},"b":{"choose_next_operation":"monitor"}}});
        let revision = |id: &str, parent: &str| json!({"id":id,"parent":parent,"statement":"Revised mechanism","requires":["e"],"research_question":"Which measurable evidence would confirm the revised premise?"});
        assert!(
            expand(
                &mut snapshot.clone(),
                &json!({"revisions":[revision("r1","a")]}),
                "deepen",
                &program
            )
            .is_ok()
        );
        assert!(
            expand(
                &mut snapshot.clone(),
                &json!({"revisions":[revision("r1","b")]}),
                "deepen",
                &program
            )
            .is_err()
        );
        assert!(
            expand(
                &mut snapshot.clone(),
                &json!({"revisions":[revision("r1","a"),revision("r2","a")]}),
                "deepen",
                &program
            )
            .is_err()
        );
    }
    #[test]
    fn cross_product_builds_81_worlds_without_invented_evidence() {
        let mut s = json!({"nodes":[{"Id":"e","edges":"[]"}]});
        let axes:Vec<_>=(0..4).map(|a|json!({"title":format!("axis {a}"),"options":(0..3).map(|b|json!({"id":format!("a{a}o{b}"),"statement":"hypothesis","requires":["e"]})).collect::<Vec<_>>()})).collect();
        expand(&mut s, &json!({"axes":axes}), "seed", &json!({})).unwrap();
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

#[cfg(test)]
mod research_revision_tests {
    use super::*;
    fn fixture() -> (Value, Value, Value) {
        (
            json!({"world":{"hindcast_mode":"false"},"nodes":[{"Id":"e","edges":"[]"},{"Id":"s","kind":"scenario","edges":"[]"}]}),
            json!({"results":{"s":{"classify_gap":"evidence","choose_next_operation":"research"}}}),
            json!({"research_evidence":[{"id":"re1","statement":"A reported baseline count, not a future fact","url":"https://example.org/report","quote":"Reported baseline count","observed_at":"2026-09-19"}],"revisions":[{"id":"r1","parent":"s","statement":"A conditional future with revised mechanism","requires":["e","re1"],"research_question":"Will adoption exceed the observed baseline?"}]}),
        )
    }
    #[test]
    fn source_reports_and_revisions_preserve_original_gaps() {
        let (mut snapshot, program, generated) = fixture();
        let original = snapshot["nodes"][1].clone();
        expand(&mut snapshot, &generated, "deepen", &program).unwrap();
        assert_eq!(snapshot["nodes"][1], original);
        let report = &snapshot["nodes"][2];
        assert_eq!(report["Status"], "Reported");
        assert_eq!(report["provenance"], "session_research_report");
        let revision = &snapshot["nodes"][3];
        assert_eq!(revision["before_gap"], "evidence");
        assert_eq!(revision["operation"], "research");
        assert_eq!(revision["research_status"], "source_reports_available");
        assert_eq!(program["results"]["s"]["classify_gap"], "evidence");
        let planned = core::plan(snapshot["nodes"].as_array().unwrap()).unwrap();
        let tasks = planned["tasks"].as_array().unwrap();
        assert!(
            tasks.iter().position(|t| t["nodeId"] == "re1").unwrap()
                < tasks.iter().position(|t| t["nodeId"] == "r1").unwrap()
        );
    }
    #[test]
    fn unavailable_research_stays_explicit() {
        let (mut snapshot, program, mut generated) = fixture();
        generated["research_evidence"] = json!([]);
        generated["revisions"][0]["requires"] = json!(["e"]);
        expand(&mut snapshot, &generated, "deepen", &program).unwrap();
        assert_eq!(snapshot["nodes"][2]["research_status"], "unresolved");
    }
    #[test]
    fn invented_or_overlong_reports_fail_closed() {
        let (snapshot, program, generated) = fixture();
        for (key, value) in [
            ("url", json!("javascript:invented")),
            ("quote", json!("word ".repeat(26))),
        ] {
            let mut bad = generated.clone();
            bad["research_evidence"][0][key] = value;
            assert!(expand(&mut snapshot.clone(), &bad, "deepen", &program).is_err());
        }
        let mut bad = generated;
        bad["revisions"][0]["parent"] = json!("unselected");
        assert!(expand(&mut snapshot.clone(), &bad, "deepen", &program).is_err());
    }
}
