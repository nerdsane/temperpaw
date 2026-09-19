use temper_wasm_sdk::prelude::*;
mod core {
    include!("../../semantic_core.rs");
}

// Scenario statements repeat the full text of their four option dependencies.
// Keep those references and the complete option text once in the model input.
fn synthesis_nodes(snapshot: &Value) -> Vec<Value> {
    snapshot["nodes"]
        .as_array()
        .into_iter()
        .flatten()
        .map(|node| {
            let mut node = node.clone();
            if core::field(&node, "kind") == "scenario" {
                node.as_object_mut().unwrap().remove("statement");
            }
            node
        })
        .collect()
}

fn synthesis_input(snapshot: &Value, program: &Value) -> Value {
    json!({"world":snapshot["world"],"nodes":synthesis_nodes(snapshot),"assessments":program["results"],"assessment_semantics":core::gap_criteria(),"issues":program["issues"],"stop_reason":program["stop_reason"],"remaining_calls":program["remaining_calls"]})
}

fn research_enabled(phase: &str, snapshot: &Value) -> bool {
    phase == "deepen" && core::field(&snapshot["world"], "hindcast_mode") == "false"
}

fn setup(ctx: &Context) -> Result<(), String> {
    let phase = core::field(&ctx.entity_state, "phase");
    let snapshot = core::parse(core::field(&ctx.entity_state, "snapshot_json"))?;
    let program = core::parse(core::field(&ctx.entity_state, "program_json"))?;
    let (prompt, input) = match phase {
        "seed" => (
            r#"You design a branching Foresight search space for the user's question in state.world.description. Use ONLY the supplied dated evidence; mark unsupported assumptions. Return JSON only, no markdown: {"axes":[{"id":"a1","title":"...","options":[{"id":"a1o1","statement":"concrete hypothetical dated outcome with actors and a measurable threshold","requires":["supplied evidence node ID"],"evidence_note":"what the evidence does and does not support","signal":"dated measurable early signal","falsifier":"observable disconfirmation"}]}]}. EXACTLY FOUR independent causal axes, EXACTLY THREE mutually distinguishable options each. Include adoption, delay and failure mechanisms, not cosmetic rewordings. The engine will cross these options into 81 alternative worlds and inspect their prerequisites recursively. IDs must be unique ASCII. Requires must name supplied evidence IDs, never invented evidence. Hypothetical details must never be described as observations. Do not assign probabilities."#,
            json!({"world":snapshot["world"],"evidence":snapshot["nodes"]}),
        ),
        "deepen" => {
            let nodes = snapshot["nodes"].as_array().ok_or("Missing nodes")?;
            let selected = core::deepening_candidates(&snapshot, &program);
            (
                r#"Deepen each selected hypothetical future. The operation label is a recommendation, not work already done. For research/uncertain selections, use only the available read-only temper.web_search and temper.web_fetch tools to investigate the most decision-relevant missing premises. At most 2 search queries and 4 fetched sources in this single round; prefer primary sources. If tools are unavailable, denied, or unhelpful, return no new evidence and retain the unanswered questions. Never invent a fetched source, infer source contents from a URL alone, or present a hypothetical future as observed. For repair selections, revise the causal mechanism. Return JSON only: {"research_evidence":[{"id":"research-01","statement":"bounded factual finding from retrieved content, with uncertainty","url":"exact fetched public URL","quote":"short relevant excerpt at most 25 words and 200 characters","observed_at":"YYYY-MM-DD"}],"revisions":[{"id":"revision-01","parent":"exact selected scenario ID","statement":"concrete revised hypothetical future, actors, date and observable threshold","requires":["existing option/evidence ID or research evidence ID"],"scene":"two vivid hypothetical sentences","signal":"dated measurable early indicator","falsifier":"observable disconfirmation","evidence_note":"what supports this and which premise remains unsupported","research_question":"the most consequential unanswered question, or explicitly none identified"}]}. Return at most 4 research_evidence entries and 1–8 revisions, at most one per selected parent. Keep original evidence and gaps unchanged. A new source is a research report, not proof of the future. Distinguish alternatives with changed causal assumptions, not cosmetic rewording. No probabilities at this stage."#,
                json!({"world":snapshot["world"],"selected":selected,"evidence":nodes.iter().filter(|n|core::field(n,"kind")!="scenario").collect::<Vec<_>>()}),
            )
        }
        "synthesize" => (
            r#"Synthesize this actual exploration into a compact decision outlook. Return JSON ONLY, no markdown: {"schema":"foresight-outlook-v1","headline":"<=160 characters","horizon":"exact world.target_date","probability_basis":"subjective_model_estimate","calibrated":false,"summary":"<=400 characters","evidence_limits":["1–6 limits, each <=240 characters"],"research_questions":["0–8 remaining questions, each <=240 characters"],"outcomes":[{"id":"stable-short-id","title":"<=70 characters","definition":"<=240 characters: observable rule distinguishing this bucket from every other","probability":0.25,"scenario_ids":["exact scenario IDs assigned to this bucket"],"narrative":"<=360 characters, concrete actors and causal mechanism","signals":["1–3 dated signals, each <=160 characters"],"falsifiers":["1–3 disconfirmations, each <=160 characters"]}]}. Exactly 3–5 outcomes TOTAL, including one id='other' with scenario_ids=[] for futures outside this incomplete modeled space. Group ALL supplied kind=scenario IDs into the other 2–4 mutually exclusive buckets: every modeled scenario must appear exactly once, no invented IDs or omitted combinations. Use a clear observable partition rule (for example one axis with disjoint thresholds), not overlapping labels. Revisions can inform judgments but their IDs are not scenario membership. Probabilities are your explicitly subjective, uncalibrated estimates for the user's question and horizon, using evidence and judgment; finite numbers between0 and1 summing EXACTLY1. These numbers are NOT Jev classification probabilities, measured frequencies, calibrated forecasts, or accuracy claims. Include honest probability mass for residual other. Use assessment_semantics: evidence means a key premise LACKS supporting evidence, never support; uncertain means evidence cannot distinguish options; none does not validate truth. Do not force gaps to disappear or claim requested research succeeded. Keep unresolved questions and distinguish retrieved reports, frozen evidence, and hypothetical revisions. Mention incomplete coverage and source limitations. Make the summary directly answer what the user should expect, in ordinary language."#,
            synthesis_input(&snapshot, &program),
        ),
        _ => return Err("Unknown reasoning phase".into()),
    };
    let web_research = research_enabled(phase, &snapshot);
    set_success_result(
        "LaunchReasoning",
        &json!({"system_prompt":prompt,"user_message":input.to_string(),"tools_enabled":if web_research {"temper_web_search,temper_web_fetch"} else {""},"tool_choice":if web_research {"auto"} else {"none"},"max_turns":if web_research {"8"} else {"1"}}),
    );
    Ok(())
}
#[unsafe(no_mangle)]
pub extern "C" fn run(_: i32, _: i32) -> i32 {
    match Context::from_host().and_then(|ctx| setup(&ctx)) {
        Ok(()) => (),
        Err(e) => set_success_result("Fail", &json!({"error_message":e})),
    };
    0
}

#[cfg(test)]
mod reasoning_tests {
    use super::*;
    #[test]
    fn synthesis_receives_the_exact_gap_definitions_used_by_jev() {
        let snapshot = json!({"world":{},"nodes":[{"Id":"a","edges":"[]"}]});
        let program = core::plan(snapshot["nodes"].as_array().unwrap()).unwrap();
        let request = core::request(&snapshot, &program).unwrap();
        let input = synthesis_input(&snapshot, &program);
        assert_eq!(
            input["assessment_semantics"],
            request["questions"]["result"]["criteria"]
        );
        assert_eq!(
            input["assessment_semantics"]["evidence"],
            "The key premise lacks supporting evidence in the supplied input."
        );
    }

    #[test]
    fn synthesis_preserves_option_text_and_scenario_dependencies_without_repeating_them() {
        let option = json!({"Id":"a1o1","kind":"option","statement":"dated mechanism"});
        let scenario = json!({"Id":"scenario-0000","kind":"scenario","statement":"dated mechanism repeated","edges":"[{\"kind\":\"requires\",\"to_id\":\"a1o1\"}]"});
        let nodes = synthesis_nodes(&json!({"nodes":[option.clone(),scenario.clone()]}));
        assert_eq!(nodes[0], option);
        assert_eq!(nodes[1]["edges"], scenario["edges"]);
        assert_eq!(nodes[1]["Id"], scenario["Id"]);
        assert!(nodes[1].get("statement").is_none());
        assert!(scenario.get("statement").is_some());
    }
}

#[cfg(test)]
mod research_tests {
    use super::*;
    #[test]
    fn only_live_deepening_enables_read_only_research() {
        let live = json!({"world":{"hindcast_mode":"false"}});
        assert!(research_enabled("deepen", &live));
        assert!(!research_enabled("seed", &live));
        assert!(!research_enabled("synthesize", &live));
        assert!(!research_enabled(
            "deepen",
            &json!({"world":{"hindcast_mode":"true"}})
        ));
    }
}
