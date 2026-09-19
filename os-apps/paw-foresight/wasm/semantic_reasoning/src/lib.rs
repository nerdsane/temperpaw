use temper_wasm_sdk::prelude::*;
// Each phase includes the shared evaluator contract but uses only its own subset.
#[allow(dead_code, unused_imports)]
mod core {
    include!("../../semantic_core.rs");
}

// The producer projects aliases; the consumer resolves them. Both share one mapping.
#[allow(dead_code)]
mod references {
    include!("../../semantic_references.rs");
}

const MAX_REASONING_INPUT_BYTES: usize = 3 * 1024 * 1024;
mod definitions {
    include!("../../semantic_definitions.rs");
}

const EXPLORATION_PROMPT: &str = r#"Investigate the user's question in state.world.description through open causal exploration. Existing nodes use short ref_ identifiers consistently across the catalog, dependencies and evaluations. Copy those references exactly; never invent or reconstruct a UUID. New hypothesis and evidence IDs must not start with ref_. The supplied catalog records what has already been considered; it is not the boundary of what can be considered. Discover new mechanisms and surprising hypotheses, pursue counterevidence, and revise the framing when it obscures something consequential. Follow interactions, second-order effects and alternatives emerging from evidence. Do not fill a predetermined taxonomy, Cartesian grid, fixed set of axes, or adoption/delay/failure template. A novel hypothesis must say what would happen and why, not merely rename a familiar outcome.

Question what today's world takes for granted. A tool, job, habit, price or institution may disappear, become cheap enough to be everywhere, or matter for a completely different reason. Follow those possibilities when they arise from the question; do not assume the future is today's workflow with more supervision. Trace what people would do differently next, including unexpected effects. Investigate strong counterarguments and futures in which the apparent trend reverses. These are ways to open the search, not a checklist or a demand for dramatic conclusions. An unlikely but consequential possibility can be worth exploring; ambitious does not mean likely. Never inflate an estimate to make a story exciting.

Write for a curious person, not a conference or a corporate report. Use familiar words, people doing things, ordinary objects and specific changes you can picture. Titles should make a clear claim rather than name a theme. Explain technical terms only when needed to understand the question. Avoid business jargon, news-roundup language and abstract labels. Each hypothesis needs a short scene: an explicitly imagined moment in someone's life if this event happens. The scene illustrates the exact event being evaluated; decorative details are not additional forecasts. Keep the underlying statement precise about who, what, when and any conditions. Explain what must change for it to happen and what could prevent it. Bold claims belong in the possibilities; doubts belong in their evaluation.

Use available read-only temper.web_search and temper.web_fetch tools to investigate useful missing premises and evidence beyond the hypotheses already explored. Prefer direct temper.web_fetch(url). If direct fetch fails, temper.web_search(query) returns bounded source-extracted text in each result's text field. A focused title or site query may retrieve a useful excerpt. Use only a claim and quotation actually contained in that returned text; never infer source contents from the title, URL or a search summary. Explicitly label indexed-excerpt evidence, direct-fetch failure and date or context limitations in the statement/evidence_note; use weak_signal when context remains unverified. A truncated excerpt does not establish that the whole source was inspected. Smaller article or text-version URLs may be fetched only when actually discovered, never invented. web_fetch accepts only a URL; do not invent size, range or encoding parameters. Tool absence, failure, or conflicting evidence must remain explicit. For frozen hindcasts, return research_evidence=[] and reference only existing catalog evidence within the stated vantage: later remembered knowledge is inadmissible. Neither a citation nor a Jev label proves a future true. Hypotheses and observations remain distinct.

Return JSON only after the research: {"hypotheses":[{"id":"unique-ascii-id","title":"concise distinct hypothesis","statement":"self-contained observable future event with actors and horizon","mechanism":"how and why it could happen, including the causal assumptions","requires":["existing node ID or new hypothesis/evidence ID whose truth this mechanism actually requires"],"parent":"optional existing or same-batch hypothesis ID when meaningfully extending or revising it","scene":"short imagined moment showing how a person lives or works if this exact event happens; not an observation","signal":"optional observable early signal","falsifier":"optional disconfirming observation","evidence_note":"what supports or challenges the mechanism and what is still conjecture","research_question":"optional consequential unanswered question"}],"research_evidence":[{"id":"unique-ascii-id","statement":"finding with date, scope, uncertainty and conflicting interpretation where relevant","url":"exact retrieved HTTPS URL","quote":"short supporting excerpt, maximum 25 words and 200 characters per source","observed_at":"YYYY-MM-DD","provenance":"observed|contested|weak_signal"}],"continue_exploring":true,"exploration_note":"what this exploration learned, which framing changed, and why another round would or would not be useful"}.

Choose the number and shape of hypotheses from the question and findings. At most128 TOTAL entries across hypotheses and research_evidence fit one batch; those are storage limits, never targets. IDs must be unique across the supplied catalog and new batch. Dependencies must reference actual supplied or newly returned nodes; use an empty requires array rather than invent supporting evidence. An optional parent records lineage, not proof. Do not assign probabilities: the engine evaluates every accepted hypothesis through Jev. Keep distinct futures even if they overlap or share a mechanism; avoid duplicates that only change wording. Research evidence must contain only content actually retrieved or supplied, with its epistemic status intact.

The engine can use up to5000 Jev calls,2048 nodes,64 exploration rounds and one hour; these are operational ceilings, not demands to pad the graph. Use the current assessments to challenge assumptions, explore neglected possibilities and direct research. Operation labels are advisory possibilities, not instructions or a required sequence; choose freely what investigation would add value. A gap is a reason to investigate or reconsider a mechanism, not a command to make every hypothesis conform to the same future. Set continue_exploring=false only when further exploration has low expected value relative to what is already covered, and explain the remaining blind spots and the concrete reason to stop. A budget stop is incomplete exploration, not convergence. Preserve unresolved questions honestly."#;

const SYNTHESIS_PROMPT: &str = r#"Use the exact short ref_ identifiers in this input for hypothesis_id and scenario_ids; the engine resolves them to persisted identities. Answer the user's question with rich, contrasting futures supported by this actual exploration. Preserve different causal mechanisms and discoveries; do not collapse them onto one convenient axis or a compliance/not-compliance partition. Explain what could happen, why, what would make it happen, and what observations would change the assessment. A few futures may be most useful, but there is no prescribed number; select what materially improves the answer from evaluated hypotheses.

Make the answer easy to consume, relate to and envision. Lead with what would change in someone's life, not a label for an industry trend. Use plain words and short, direct sentences. No corporate jargon, management abstractions, press-release voice or news digest. Be sharp about the claim and honest about uncertainty. Do not make a modest finding sound revolutionary. Do not choose only the highest odds: a well-explained, consequential alternative may teach more, but it still needs an actual evaluation. Preserve genuinely different futures instead of repeating one lesson in different words.

For each outcome, write a brief scene someone can picture, clearly hypothetical. Explain why it could happen and who gains or struggles in the narrative, using the exploration's actual evidence and assumptions. State useful things the reader could do or watch now in what_you_can_do, tied to this particular future; leave it empty if none are supported. The scene must not broaden the referenced event, invent observed people or incidents, or imply its illustrative details carry the displayed probability. Keep the exact claim and horizon in definition. Signals and falsifiers must be things a person could actually notice or check, written in ordinary language. Avoid repeating the same text across headline, summary, scene and narrative.

Return JSON ONLY: {"schema":"foresight-outlook-v2","headline":"<=160 characters","horizon":"exact world.target_date","probability_basis":"model_implied_event_estimate","probability_model":"overlapping_events","calibrated":false,"summary":"<=400 characters","evidence_limits":["1–32 honest limitations, each <=240 characters"],"research_questions":["0–64 unresolved questions, each <=240 characters"],"outcomes":[{"id":"stable-short-id","hypothesis_id":"exact evaluated hypothesis node ID","title":"<=100 characters","definition":"<=1000 characters; faithful to the referenced hypothesis event and horizon","scenario_ids":["related actual hypothesis IDs, possibly shared across outcomes"],"scene":"<=600 characters; a short hypothetical moment in everyday life showing the referenced event","narrative":"<=1200 characters; plain explanation of why it could happen, what must change, who gains or struggles, and what could prevent it","what_you_can_do":["0–4 specific things the reader could do or watch now, each <=240 characters"],"signals":["1–8 observable early signals, each <=240 characters"],"falsifiers":["1–8 observable disconfirmations, each <=240 characters"]}]}.

Return1–64 outcomes as useful within this output capacity, not a quota. Every outcome must reference a hypothesis with an actual estimate_likelihood evaluation. Keep its event definition faithful; do not broaden or replace it while retaining its probability. Do not supply or invent a probability field: the engine attaches the referenced hypothesis's exact Jev event estimate. These are model-implied estimates of individual overlapping events, not mutually exclusive buckets, calibrated forecasts, empirical frequencies, or Jev gap-label confidence. They need not sum to one. Related scenario_ids are optional context, not an exhaustive partition. No residual other outcome is required. Distinguish retrieved observations from hypotheses. A causal gap does not imply probability zero; no detected gap does not imply truth. Explain important disagreements, limitations, unresolved research and why the exploration stopped without claiming it exhausted the future."#;

fn node_catalog(snapshot: &Value) -> Vec<Value> {
    snapshot["nodes"]
        .as_array()
        .into_iter()
        .flatten()
        .map(|node| {
            let mut compact = json!({});
            for key in [
                "Id",
                "kind",
                "title",
                "statement",
                "mechanism",
                "edges",
                "parent",
                "provenance",
                "signal",
                "falsifier",
                "evidence_note",
                "research_question",
                "scene",
            ] {
                if let Some(value) = node.get(key) {
                    compact[key] = value.clone();
                }
            }
            compact
        })
        .collect()
}

fn compact_evaluations(program: &Value) -> Value {
    let mut compact = json!({});
    if let Some(nodes) = program["evaluations"].as_object() {
        for (id, evaluations) in nodes {
            if let Some(evaluations) = evaluations.as_object() {
                for (function, evaluation) in evaluations {
                    for key in ["probability", "score", "selected"] {
                        if let Some(value) = evaluation.get(key) {
                            compact[id][function][key] = value.clone();
                        }
                    }
                }
            }
        }
    }
    compact
}

fn reasoning_input(snapshot: &Value, program: &Value) -> Result<Value, String> {
    let nodes = snapshot["nodes"].as_array().ok_or("Missing nodes")?;
    let evidence: Vec<_> = nodes
        .iter()
        .filter(|node| {
            !matches!(
                core::field(node, "kind"),
                "hypothesis" | "scenario" | "revision" | "option"
            )
        })
        .collect();
    let input = json!({
        "world":snapshot["world"], "catalog":node_catalog(snapshot),
        "source_evidence":evidence,
        "assessments":program["results"], "evaluations": compact_evaluations(program), "assessment_semantics":core::gap_criteria(),
        "evaluation_semantics":{
            "score_scale":"Expected category index on a 0–4 scale, not a probability or a percentage.",
            "evaluate_novelty":definitions::evaluate_novelty(),
            "decision_value":definitions::decision_value(),
            "choose_next_operation":definitions::choose_next_operation(),
            "operation_role":"Advisory possibilities, not instructions, completion claims or a prescribed sequence."
        },
        "issues":program["issues"], "stop_reason":program["stop_reason"],
        "remaining_calls":program["remaining_calls"], "round":program["round"],
        "exploration_note":program["exploration_note"]
    });
    let input = references::References::new(snapshot)?.project(&input);
    if input.to_string().len() > MAX_REASONING_INPUT_BYTES {
        return Err(
            "Reasoning context exceeds 3 MiB; refusing to silently omit explored hypotheses".into(),
        );
    }
    Ok(input)
}

fn research_enabled(phase: &str, snapshot: &Value) -> bool {
    matches!(phase, "seed" | "explore")
        && core::field(&snapshot["world"], "hindcast_mode") == "false"
}

fn setup(ctx: &Context) -> Result<(), String> {
    let phase = core::field(&ctx.entity_state, "phase");
    let snapshot = core::parse(core::field(&ctx.entity_state, "snapshot_json"))?;
    let program = core::parse(core::field(&ctx.entity_state, "program_json"))?;
    let prompt = match phase {
        "seed" | "explore" => EXPLORATION_PROMPT,
        "synthesize" => SYNTHESIS_PROMPT,
        _ => return Err("Unknown reasoning phase".into()),
    };
    let input = reasoning_input(&snapshot, &program)?;
    let web_research = research_enabled(phase, &snapshot);
    set_success_result(
        "LaunchReasoning",
        &json!({"system_prompt":prompt,"user_message":input.to_string(),"tools_enabled":if web_research {"temper_web_search,temper_web_fetch"} else {""},"tool_choice":if web_research {"auto"} else {"none"},"max_turns":if web_research {"32"} else {"1"}}),
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

    mod outlook_contract {
        include!("../../semantic_outlook.rs");
    }

    #[test]
    fn reasoning_score_legends_are_the_same_definitions_sent_to_jev() {
        let snapshot = json!({"world":{},"nodes":[{"Id":"h","kind":"scenario","statement":"Future","edges":"[]","signal":"Signal","falsifier":"Falsifier","evidence_note":"Limited evidence","research_question":"Unanswered","scene":"Hypothetical scene"}]});
        let mut program = core::plan(snapshot["nodes"].as_array().unwrap()).unwrap();
        let input = reasoning_input(&snapshot, &program).unwrap();
        for function in [
            "evaluate_novelty",
            "decision_value",
            "choose_next_operation",
        ] {
            let index = program["tasks"]
                .as_array()
                .unwrap()
                .iter()
                .position(|task| task["function"] == function)
                .unwrap();
            program["cursor"] = json!(index);
            let request = core::request(&snapshot, &program).unwrap();
            assert_eq!(
                input["evaluation_semantics"][function],
                request["questions"]["result"]["criteria"]
            );
        }
        for field in [
            "signal",
            "falsifier",
            "evidence_note",
            "research_question",
            "scene",
        ] {
            assert_eq!(input["catalog"][0][field], snapshot["nodes"][0][field]);
        }
        assert!(
            input["evaluation_semantics"]["operation_role"]
                .as_str()
                .unwrap()
                .contains("Advisory")
        );
    }

    #[test]
    fn exploration_can_use_bounded_indexed_text_with_explicit_limits() {
        for required in [
            "result's text field",
            "actually contained in that returned text",
            "indexed-excerpt evidence",
            "direct-fetch failure",
            "weak_signal",
            "only when actually discovered",
            "web_fetch accepts only a URL",
        ] {
            assert!(EXPLORATION_PROMPT.contains(required), "missing {required}");
        }
        assert!(EXPLORATION_PROMPT.contains("return research_evidence=[]"));
    }

    #[test]
    fn advertised_synthesis_text_limits_pass_the_real_outlook_validator() {
        let raw = SYNTHESIS_PROMPT
            .split("Return JSON ONLY: ")
            .nth(1)
            .unwrap()
            .split("}.\n")
            .next()
            .unwrap();
        let mut answer: Value = serde_json::from_str(&format!("{raw}}}")).unwrap();
        fn expand(value: &Value) -> String {
            let limit = value
                .as_str()
                .unwrap()
                .split("<=")
                .nth(1)
                .unwrap()
                .split_whitespace()
                .next()
                .unwrap()
                .parse::<usize>()
                .unwrap();
            "x".repeat(limit)
        }
        for key in ["headline", "summary"] {
            answer[key] = json!(expand(&answer[key]));
        }
        answer["horizon"] = json!("2027");
        for key in ["evidence_limits", "research_questions"] {
            answer[key] = json!([expand(&answer[key][0])]);
        }
        let outcome = &mut answer["outcomes"][0];
        for key in ["title", "definition", "narrative"] {
            outcome[key] = json!(expand(&outcome[key]));
        }
        for key in ["signals", "falsifiers"] {
            outcome[key] = json!(vec![expand(&outcome[key][0]); 8]);
        }
        outcome["hypothesis_id"] = json!("h1");
        outcome["scenario_ids"] = json!(["h1"]);
        outcome["probability"] = json!(0.37);
        let snapshot =
            json!({"world":{"target_date":"2027"},"nodes":[{"Id":"h1","kind":"scenario"}]});
        outlook_contract::validate(&answer, &snapshot).unwrap();
        answer["summary"] = json!("x".repeat(401));
        assert!(outlook_contract::validate(&answer, &snapshot).is_err());
    }

    #[test]
    fn synthesis_sees_actual_numeric_estimates_without_duplicate_answer_payloads() {
        let snapshot = json!({"nodes":[{"Id":"h","kind":"scenario","statement":"Future"}]});
        let program = json!({"evaluations":{"h":{"estimate_likelihood":{"probability":0.37,"answer":{"noul":0.37}}}}});
        let input = reasoning_input(&snapshot, &program).unwrap();
        assert_eq!(
            input["evaluations"]["ref_0001"]["estimate_likelihood"]["probability"],
            0.37
        );
        assert!(
            input["evaluations"]["ref_0001"]["estimate_likelihood"]
                .get("answer")
                .is_none()
        );
    }

    #[test]
    fn catalog_preserves_actual_future_statement_and_mechanism() {
        let node = json!({"Id":"future-a","kind":"scenario","statement":"A novel future, not repeated option text","mechanism":"unexpected interaction","edges":"[]"});
        let catalog = node_catalog(&json!({"nodes":[node.clone()]}));
        assert_eq!(catalog[0], node);
    }

    #[test]
    fn complete_catalog_retains_every_hypothesis_without_ranked_duplication() {
        let nodes: Vec<_> = (0..150).map(|i| json!({"Id":format!("h-{i}"),"kind":"hypothesis","statement":format!("Unique future {i}")})).collect();
        let input = reasoning_input(&json!({"nodes":nodes}), &json!({})).unwrap();
        assert!(input.get("frontier").is_none());
        assert_eq!(input["catalog"].as_array().unwrap().len(), 150);
        assert_eq!(input["catalog"][149]["statement"], "Unique future 149");
        assert_eq!(input["assessment_semantics"], core::gap_criteria());
    }

    #[test]
    fn oversized_context_is_explicit_failure_not_silent_truncation() {
        let snapshot =
            json!({"nodes":[{"Id":"huge","statement":"x".repeat(MAX_REASONING_INPUT_BYTES)}]});
        assert!(
            reasoning_input(&snapshot, &json!({}))
                .unwrap_err()
                .contains("refusing")
        );
    }

    #[test]
    fn live_seed_and_exploration_can_research_but_hindcast_and_synthesis_cannot() {
        let live = json!({"world":{"hindcast_mode":"false"}});
        let frozen = json!({"world":{"hindcast_mode":"true"}});
        for phase in ["seed", "explore"] {
            assert!(research_enabled(phase, &live));
            assert!(!research_enabled(phase, &frozen));
        }
        assert!(!research_enabled("synthesize", &live));
        assert!(!research_enabled("explore", &json!({})));
    }

    #[test]
    fn prompts_require_open_hypotheses_and_engine_owned_event_probabilities() {
        assert!(EXPLORATION_PROMPT.contains("continue_exploring"));
        assert!(EXPLORATION_PROMPT.contains("novel hypothesis"));
        assert!(!EXPLORATION_PROMPT.contains("EXACTLY FOUR"));
        assert!(!EXPLORATION_PROMPT.contains("81 alternative"));
        assert!(SYNTHESIS_PROMPT.contains("overlapping_events"));
        assert!(SYNTHESIS_PROMPT.contains("exact Jev event estimate"));
        assert!(!SYNTHESIS_PROMPT.contains("summing EXACTLY1"));
    }
}
