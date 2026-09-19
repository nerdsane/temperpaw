// Jev primitives evaluate explicit hypotheses; their distributions are not empirical calibration.
use super::{MODEL, field, parse};
use serde_json::{Value, json};
mod definitions {
    include!("semantic_definitions.rs");
}
fn digest(node: &Value) -> Value {
    let mut out = serde_json::Map::new();
    for key in [
        "Id",
        "kind",
        "title",
        "mechanism",
        "source_quote",
        "quote",
        "observed_at",
        "claim_type",
        "evidence_note",
        "signal",
        "statement",
        "date",
        "timestamp",
        "sources",
        "source_refs",
        "resolve_by",
        "provenance",
        "resolution",
        "probability",
        "evidence",
        "research_question",
        "component_ids", "counter_ids", "scene", "narrative", "signals", "falsifiers", "what_you_can_do",
    ] {
        if let Some(value) = node.get("fields").unwrap_or(node).get(key) {
            out.insert(key.into(), value.clone());
        }
    }
    Value::Object(out)
}
pub fn request(snapshot: &Value, program: &Value) -> Result<Value, String> {
    let cursor = program["cursor"].as_u64().ok_or("Missing cursor")? as usize;
    let task = &program["tasks"][cursor];
    let id = task["nodeId"].as_str().ok_or("Missing task identity")?;
    let nodes = snapshot["nodes"]
        .as_array()
        .ok_or("Missing snapshot nodes")?;
    let node = nodes
        .iter()
        .find(|n| field(n, "Id") == id)
        .ok_or("Task references absent node")?;
    let edges = parse(field(node, "edges"))?;
    let prerequisites: Vec<Value> = edges.as_array().ok_or("Invalid edges")?.iter().filter(|e| e["kind"] == "requires").map(|edge| {
        let target = edge["to_id"].as_str().unwrap_or("");
        json!({"id":target,"node":nodes.iter().find(|n|field(n,"Id")==target).map(digest),"assessment":program["results"][target],"evaluations":program["evaluations"][target]})
    }).collect();
    let candidates: Vec<_> = nodes
        .iter()
        .filter(|n| field(n, "Id") != id && matches!(field(n, "kind"), "scenario" | "revision"))
        .collect();
    let words: std::collections::BTreeSet<_> = field(node, "statement")
        .split_whitespace()
        .filter(|w| w.len() > 3)
        .map(str::to_lowercase)
        .collect();
    let mut nearest = candidates.clone();
    nearest.sort_by_key(|n| {
        std::cmp::Reverse(
            field(n, "statement")
                .split_whitespace()
                .filter(|w| words.contains(&w.to_lowercase()))
                .count(),
        )
    });
    let mut selected = std::collections::BTreeSet::new();
    let comparisons: Vec<_> = candidates
        .iter()
        .rev()
        .take(8)
        .chain(nearest.iter())
        .filter(|n| selected.insert(field(n, "Id")))
        .take(16)
        .map(|n| digest(n))
        .collect();
    let mut question = match task["function"].as_str().ok_or("Missing function")? {
        "classify_gap" => {
            json!({"type":"choice","instructions":"Identify the most consequential causal gap in this hypothesis using actual supplied evidence. Future events are hypotheses, not false observations. Source URLs alone do not prove contents. A coherent mechanism does not imply a likely outcome.","criteria":super::gap_criteria()})
        }
        "estimate_likelihood" => {
            json!({"type":"noul","instructions":"Estimate whether the explicit event described by state.node will occur within its stated date or horizon, conditioned on the supplied world question, evidence and prerequisite assessments. This is an event proposition, not a question about coherence, novelty, confidence, or whether the text asserts the event. Account for unsupported premises and contrary evidence. Preserve uncertainty; overlapping hypotheses need not sum to one.","criteria":{"true":"The described event occurs within its stated horizon.","false":"The described event does not occur within its stated horizon."}})
        }
        "evaluate_novelty" => {
            json!({"type":"score","instructions":"Assess how much distinct explanatory or decision-relevant information this hypothesis adds compared with the supplied comparison sample. Novelty is relative to this sample, not a claim of global originality. Reward a new causal mechanism, not surprising wording.","criteria":definitions::evaluate_novelty()})
        }
        "decision_value" => {
            json!({"type":"score","instructions":"Assess the value of investigating or monitoring this hypothesis for decisions implied by the world question. Consider consequences, tractable uncertainty and actionable discriminating signals; likelihood alone is not decision value.","criteria":definitions::decision_value()})
        }
        "choose_next_operation" => {
            json!({"type":"choice","instructions":"Recommend the next useful operation using the accumulated assessments and evaluations. This does not claim that an operation executed. Exploration should follow information value, unanswered causal questions and evidence, without a prescribed phase sequence.","criteria":definitions::choose_next_operation()})
        }
        _ => return Err("Unsupported semantic function".into()),
    };
    let is_world = field(node,"kind") == "world";
    let mut counter_hypotheses = vec![];
    let mut evidence = vec![];
    if is_world {
        for id in node["counter_ids"].as_array().ok_or("Missing world counters")? {
            let target=id.as_str().ok_or("Invalid counter identity")?;
            let counter=nodes.iter().find(|n|field(n,"Id")==target).ok_or("Missing counter hypothesis")?;
            counter_hypotheses.push(json!({"node":digest(counter),"assessment":program["results"][target]}));
        }
        // Supply actual source claims, including provenance and quoted excerpts,
        // rather than treating component likelihoods as evidence for a joint event.
        evidence=nodes.iter().filter(|n|field(n,"kind")=="evidence").map(digest).collect();
        if task["function"] == "estimate_likelihood" {
            question["instructions"]=json!("Estimate the probability of the WHOLE JOINT WORLD defined by state.node.statement AND ALL its defining component events in state.prerequisites, within the world horizon. Every defining component must occur for this joint world to occur. Evaluate causal interactions, correlations, shared assumptions, supplied source evidence, baseline unknowns, and counter hypotheses. Component estimates are context only: never average, multiply, inherit, or substitute them for a fresh assessment of the joint world. Counter hypotheses are contrary context, not required events. This is event likelihood, not narrative coherence or confidence. Worlds may overlap and need not sum to one.");
        }
    }
    let request = json!({"model":MODEL,"state":{"world":snapshot["world"],"node":digest(node),"prerequisites":prerequisites,"counter_hypotheses":counter_hypotheses,"source_evidence":evidence,"baseline":program["baseline"],"comparisons":comparisons,"assessment":program["results"][id],"evaluations":program["evaluations"][id]},"questions":{"result":question}});
    if request.to_string().len() > 128 * 1024 {
        return Err("Semantic request exceeds 128 KB".into());
    }
    Ok(request)
}
fn distribution(answer: &Value, keys: &[String]) -> Result<Vec<f64>, String> {
    let values = answer["probabilities"]
        .as_object()
        .ok_or("Missing distribution")?;
    if values.len() != keys.len() {
        return Err("Wrong option count".into());
    }
    let probabilities: Vec<_> = keys
        .iter()
        .map(|key| {
            values
                .get(key)
                .and_then(Value::as_f64)
                .filter(|p| p.is_finite() && (0.0..=1.0).contains(p))
                .ok_or("Invalid probability".to_owned())
        })
        .collect::<Result<_, _>>()?;
    if (probabilities.iter().sum::<f64>() - 1.0).abs() > 0.005 * keys.len() as f64 + f64::EPSILON {
        return Err("Invalid probability mass".into());
    }
    Ok(probabilities)
}
pub fn validate(request: &Value, response: &Value) -> Result<String, String> {
    if response["model"] != MODEL {
        return Err("Provider model mismatch".into());
    }
    let answer = &response["answers"]["result"];
    let question = &request["questions"]["result"];
    // Missing type is supported only for the historical choice test/request format.
    let kind = question["type"].as_str().unwrap_or("choice");
    if answer["type"] != kind {
        return Err("Provider answer type mismatch".into());
    }
    match kind {
        "noul" => answer["noul"]
            .as_f64()
            .filter(|p| p.is_finite() && (0.0..=1.0).contains(p))
            .map(|p| p.to_string())
            .ok_or("Invalid event probability".into()),
        "score" => {
            let levels = question["criteria"]
                .as_array()
                .ok_or("Missing score criteria")?;
            if !(2..=10).contains(&levels.len()) {
                return Err("Invalid score levels".into());
            }
            let keys: Vec<_> = (0..levels.len()).map(|i| i.to_string()).collect();
            let p = distribution(answer, &keys)?;
            let score = answer["score"]
                .as_f64()
                .filter(|s| s.is_finite() && *s >= 0.0 && *s <= (levels.len() - 1) as f64)
                .ok_or("Invalid score")?;
            let expected: f64 = p.iter().enumerate().map(|(i, p)| i as f64 * p).sum();
            let tolerance = 0.005 * (0..levels.len()).sum::<usize>() as f64 + 0.01;
            if (score - expected).abs() > tolerance {
                return Err("Score does not match distribution".into());
            }
            Ok(score.to_string())
        }
        "choice" => {
            let options = question["criteria"].as_object().ok_or("Missing criteria")?;
            let keys: Vec<_> = options.keys().cloned().collect();
            let probabilities = distribution(answer, &keys)?;
            let selected = answer["choice"].as_str().ok_or("Missing choice")?;
            let max = probabilities.iter().copied().fold(0.0, f64::max);
            if answer["probabilities"][selected].as_f64() != Some(max) {
                return Err("Choice is not argmax".into());
            }
            Ok(if max < 0.65 && options.contains_key("uncertain") {
                "uncertain"
            } else {
                selected
            }
            .into())
        }
        _ => Err("Unsupported answer type".into()),
    }
}
pub fn evaluation_value(request: &Value, response: &Value) -> Result<Value, String> {
    let selected = validate(request, response)?;
    let answer = &response["answers"]["result"];
    Ok(match answer["type"].as_str() {
        Some("noul") => {
            json!({"type":"noul","answer":answer,"probability":answer["noul"],"interpretation":"model_implied_event_probability_not_empirically_calibrated"})
        }
        Some("score") => json!({"type":"score","answer":answer,"score":answer["score"]}),
        _ => json!({"type":"choice","answer":answer,"selected":selected}),
    })
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn event_probability_is_not_thresholded_or_choice_confidence() {
        let q = json!({"questions":{"result":{"type":"noul"}}});
        for p in [0.0, 0.37, 1.0] {
            let r = json!({"model":MODEL,"answers":{"result":{"type":"noul","noul":p}}});
            assert_eq!(evaluation_value(&q, &r).unwrap()["probability"], p);
        }
        for p in [-0.1, 1.1] {
            assert!(
                validate(
                    &q,
                    &json!({"model":MODEL,"answers":{"result":{"type":"noul","noul":p}}})
                )
                .is_err()
            );
        }
    }
    #[test]
    fn score_requires_consistent_bounded_distribution() {
        let q = json!({"questions":{"result":{"type":"score","criteria":["low","medium","high"]}}});
        let mut r = json!({"model":MODEL,"answers":{"result":{"type":"score","score":1.5,"probabilities":{"0":0.1,"1":0.3,"2":0.6},"legend":["low","medium","high"]}}});
        assert_eq!(validate(&q, &r).unwrap(), "1.5");
        r["answers"]["result"]["score"] = json!(0.2);
        assert!(validate(&q, &r).is_err());
        r["answers"]["result"]["type"] = json!("choice");
        assert!(validate(&q, &r).is_err());
    }
    #[test]
    fn requests_chain_prior_numeric_evaluations() {
        let snapshot = json!({"world":{"question":"What happens by 2027?"},"nodes":[{"Id":"h","kind":"scenario","statement":"Event occurs by 2027","edges":"[]"}]});
        let p = json!({"cursor":0,"tasks":[{"nodeId":"h","function":"choose_next_operation"}],"results":{"h":{"classify_gap":"evidence"}},"evaluations":{"h":{"estimate_likelihood":{"probability":0.2}}}});
        let r = request(&snapshot, &p).unwrap();
        assert_eq!(
            r["state"]["evaluations"]["estimate_likelihood"]["probability"],
            0.2
        );
        assert!(
            r["questions"]["result"]["criteria"]
                .get("challenge")
                .is_some()
        );
    }
}
#[cfg(test)]
mod comparison_tests {
    use super::*;
    #[test]
    fn comparison_sample_includes_recent_and_relevant_mechanisms() {
        let mut nodes:Vec<_>=(0..40).map(|i|json!({"Id":format!("h{i}"),"kind":"scenario","statement":if i==15{"semiconductor supply bottleneck"}else{"different unrelated path"},"edges":"[]"})).collect();
        nodes.push(json!({"Id":"target","kind":"scenario","statement":"semiconductor supply bottleneck reverses","source_quote":"Direct quoted observation","quote":"Source text","observed_at":"2026-09-19","claim_type":"reported_observation","mechanism":"Supply constrains adoption","edges":"[]"}));
        let r = request(
            &json!({"world":{},"nodes":nodes}),
            &json!({"cursor":0,"tasks":[{"nodeId":"target","function":"evaluate_novelty"}]}),
        )
        .unwrap();
        let ids: Vec<_> = r["state"]["comparisons"]
            .as_array()
            .unwrap()
            .iter()
            .map(|n| n["Id"].as_str().unwrap())
            .collect();
        assert!(ids.contains(&"h39") && ids.contains(&"h15"));
        assert_eq!(ids.len(), 16);
        assert_eq!(
            r["state"]["node"]["source_quote"],
            "Direct quoted observation"
        );
        assert_eq!(
            r["state"]["node"]["mechanism"],
            "Supply constrains adoption"
        );
        assert_eq!(r["state"]["node"]["quote"], "Source text");
        assert_eq!(r["state"]["node"]["observed_at"], "2026-09-19");
        assert_eq!(r["state"]["node"]["claim_type"], "reported_observation");
    }
}
