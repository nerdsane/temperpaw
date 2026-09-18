use temper_wasm_sdk::prelude::*;
mod core {
    include!("../../semantic_core.rs");
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
            let selected = core::repair_candidates(&snapshot, &program);
            (
                r#"Repair and deepen the selected hypothetical futures, using the supplied frozen evidence and Jev's specific gap assessments. Return JSON only: {"revisions":[{"id":"r01","parent":"exact scenario ID from selected","statement":"concrete revised future with a causal mechanism, actors, date and observable threshold","requires":["existing option or evidence node IDs"],"scene":"two vivid hypothetical sentences showing ordinary life/work","signal":"dated measurable early indicator","falsifier":"observable disconfirmation","evidence_note":"what is supported and what remains conjecture"}]}. At most 8 revisions, one per selected parent. Preserve uncertainty. Do not invent sources or assert research happened. Choose genuinely distinct causal repairs, not mere rephrasing."#,
                json!({"world":snapshot["world"],"selected":selected,"evidence":nodes.iter().filter(|n|core::field(n,"kind")!="scenario").collect::<Vec<_>>()}),
            )
        }
        "synthesize" => (
            r#"Answer the user's future-world question using this actual exploration and its recorded semantic assessments. Write a clear, vivid answer with 3–5 distinct plausible worlds, concrete actors, causal mechanisms, dates, early signals and falsifiers. Cite the exact scenario IDs and supplied source references. Distinguish observed evidence, hypotheses, and unknowns. Explain meaningful disagreements and the impact on the user's decisions. Jev's choice distributions are NOT forecast probabilities. A no-gap judgement is not validation. Do not claim measured accuracy improvements or successful research beyond the supplied evidence. State coverage/budget limits. Use readable Markdown."#,
            json!({"world":snapshot["world"],"nodes":snapshot["nodes"],"assessments":program["results"],"issues":program["issues"],"stop_reason":program["stop_reason"],"remaining_calls":program["remaining_calls"]}),
        ),
        _ => return Err("Unknown reasoning phase".into()),
    };
    set_success_result(
        "LaunchReasoning",
        &json!({"system_prompt":prompt,"user_message":input.to_string()}),
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
