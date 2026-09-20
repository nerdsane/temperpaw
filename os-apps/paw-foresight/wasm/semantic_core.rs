// Append-only open hypothesis graph. Budgets bound execution, not the shape of futures.
use serde_json::{Value, json};
use std::collections::{BTreeMap, BTreeSet};
pub const MODEL: &str = "jev-1.13.0";
pub const MAX_CALLS: usize = 5000;
pub const MAX_NODES: usize = 2048;
pub const MAX_MS: u64 = 3_600_000;
pub const MAX_TRACE_BYTES: usize = 24 * 1024 * 1024;
pub const MAX_ROUNDS: u64 = 64;
// Questions, not HTTP requests: independent structural questions can share a call.
pub const WORLD_CALL_RESERVE: usize = 3600;
pub const WORLD_TIME_RESERVE_MS: u64 = 600_000;
// The reserved tail contains pair search followed by world evaluation.
pub const COMBINATION_CALL_BUDGET: usize = 1000;
pub const COMBINATION_TIME_BUDGET_MS: u64 = 180_000;
pub fn call_limit(program: &Value) -> usize {
    match program["stage"].as_str() {
        Some("worlds") => MAX_CALLS,
        Some("combinations") => MAX_CALLS - WORLD_CALL_RESERVE + COMBINATION_CALL_BUDGET,
        _ => MAX_CALLS - WORLD_CALL_RESERVE,
    }
}
pub fn time_limit(program: &Value) -> u64 {
    match program["stage"].as_str() {
        Some("worlds") => MAX_MS,
        Some("combinations") => MAX_MS - WORLD_TIME_RESERVE_MS + COMBINATION_TIME_BUDGET_MS,
        _ => MAX_MS - WORLD_TIME_RESERVE_MS,
    }
}
pub mod evaluation {
    include!("semantic_evaluation.rs");
}
pub mod search {
    include!("semantic_search.rs");
}
pub mod batch {
    include!("semantic_batch.rs");
}
pub use evaluation::{evaluation_value, request, validate};
pub fn field<'a>(v: &'a Value, key: &str) -> &'a str {
    v.get("fields")
        .unwrap_or(v)
        .get(key)
        .and_then(Value::as_str)
        .unwrap_or("")
}
pub fn parse(raw: &str) -> Result<Value, String> {
    serde_json::from_str(raw).map_err(|_| "Invalid persisted semantic state".into())
}
/// Order prerequisites before dependent hypotheses without recursive stack growth.
pub fn plan(nodes: &[Value]) -> Result<Value, String> {
    if nodes.is_empty() || nodes.len() > MAX_NODES {
        return Err("Semantic graph exceeds its node budget or is empty".into());
    }
    let mut by_id = BTreeMap::new();
    for node in nodes {
        let id = field(node, "Id");
        if id.is_empty() || by_id.insert(id.to_owned(), node).is_some() {
            return Err("Missing or duplicate event identity".into());
        }
    }
    let mut prerequisites: BTreeMap<String, BTreeSet<String>> = BTreeMap::new();
    let mut dependents: BTreeMap<String, BTreeSet<String>> = BTreeMap::new();
    let mut issues = vec![];
    for (id, node) in &by_id {
        let mut refs = BTreeSet::new();
        let edges = parse(field(node, "edges"))
            .ok()
            .and_then(|v| v.as_array().cloned());
        if let Some(edges) = edges {
            for edge in edges.iter().filter(|e| e["kind"] == "requires") {
                if let Some(target) = edge["to_id"].as_str().filter(|s| !s.is_empty()) {
                    if by_id.contains_key(target) {
                        refs.insert(target.to_owned());
                        dependents
                            .entry(target.to_owned())
                            .or_default()
                            .insert(id.clone());
                    } else {
                        issues.push(
                            json!({"nodeId":id,"target":target,"result":"missing_prerequisite"}),
                        );
                    }
                } else {
                    issues.push(json!({"nodeId":id,"result":"invalid_reference"}));
                }
            }
        } else {
            issues.push(json!({"nodeId":id,"result":"invalid_edges"}));
        }
        prerequisites.insert(id.clone(), refs);
    }
    let mut remaining: BTreeMap<String, usize> = prerequisites
        .iter()
        .map(|(id, refs)| (id.clone(), refs.len()))
        .collect();
    let mut ready: BTreeSet<String> = remaining
        .iter()
        .filter(|(_, n)| **n == 0)
        .map(|(id, _)| id.clone())
        .collect();
    let mut depths: BTreeMap<String, usize> = BTreeMap::new();
    let mut tasks = vec![];
    while let Some(id) = ready.pop_first() {
        let depth = prerequisites[&id]
            .iter()
            .map(|p| depths[p] + 1)
            .max()
            .unwrap_or(0);
        depths.insert(id.clone(), depth);
        let hypothesis = matches!(field(by_id[&id], "kind"), "scenario" | "revision");
        let functions: &[&str] = if field(by_id[&id], "kind") == "world" {
            &["classify_gap", "estimate_likelihood"]
        } else if hypothesis {
            &[
                "classify_gap",
                "estimate_likelihood",
                "evaluate_novelty",
                "decision_value",
            ]
        } else {
            &["classify_gap"]
        };
        for function in functions {
            tasks.push(json!({"nodeId":id,"function":function,"depth":depth}));
        }
        for dependent in dependents.get(&id).into_iter().flatten() {
            let count = remaining
                .get_mut(dependent)
                .ok_or("Planner lost a dependency")?;
            *count -= 1;
            if *count == 0 {
                ready.insert(dependent.clone());
            }
        }
    }
    for id in by_id.keys().filter(|id| !depths.contains_key(*id)) {
        issues.push(json!({"nodeId":id,"result":"cycle_or_cyclic_prerequisite"}));
    }
    Ok(
        json!({"schema":"foresight-open-semantic-v2","stage":"exploration","cursor":0,"tasks":tasks,"issues":issues,"results":{},"evaluations":{},"round":0,"rounds":[],"continue_exploring":true,"max_calls":MAX_CALLS,"max_nodes":MAX_NODES,"time_budget_ms":MAX_MS}),
    )
}
pub fn gap_criteria() -> Value {
    json!({"none":"No specific causal gap identified; this does not establish truth.","timing":"The supplied interval is materially too short.","prerequisite":"A necessary causal prerequisite is missing.","evidence":"The key premise lacks supporting evidence in the supplied input.","uncertain":"Cannot distinguish reliably."})
}
#[cfg(test)]
mod tests {
    use super::*;
    fn node(id: &str, kind: &str, refs: &[&str]) -> Value {
        json!({"Id":id,"kind":kind,"statement":id,"edges":refs.iter().map(|id|json!({"kind":"requires","to_id":id})).collect::<Vec<_>>().pipe()})
    }
    trait Encoded {
        fn pipe(self) -> String;
    }
    impl Encoded for Vec<Value> {
        fn pipe(self) -> String {
            serde_json::to_string(&self).unwrap()
        }
    }
    #[test]
    fn prerequisites_are_once_before_dependents() {
        let p = plan(&[
            node("a", "scenario", &["z"]),
            node("b", "scenario", &["z"]),
            node("z", "evidence", &[]),
        ])
        .unwrap();
        assert_eq!(p["tasks"].as_array().unwrap().len(), 9);
        assert_eq!(p["tasks"][0]["nodeId"], "z");
        assert_eq!(p["tasks"][1]["function"], "classify_gap");
    }
    #[test]
    fn thousands_of_evaluations_are_planned_without_a_cartesian_product() {
        let nodes: Vec<_> = (0..1000)
            .map(|i| node(&format!("h{i}"), "scenario", &[]))
            .collect();
        let p = plan(&nodes).unwrap();
        assert_eq!(p["tasks"].as_array().unwrap().len(), 4000);
        assert_eq!(p["max_calls"], 5000);
    }
    #[test]
    fn cycles_and_missing_evidence_stay_explicit() {
        let p = plan(&[
            node("a", "scenario", &["b"]),
            node("b", "scenario", &["a"]),
            node("ok", "scenario", &["missing"]),
        ])
        .unwrap();
        assert_eq!(p["tasks"].as_array().unwrap().len(), 4);
        assert!(
            p["issues"]
                .to_string()
                .contains("cycle_or_cyclic_prerequisite")
        );
        assert!(p["issues"].to_string().contains("missing_prerequisite"));
    }
    #[test]
    fn deep_graph_does_not_get_cut_off_at_eight() {
        let nodes: Vec<_> = (0..100)
            .map(|i| node(&format!("h{i}"), "scenario", &[]))
            .collect();
        let mut nodes = nodes;
        for i in 1..100 {
            nodes[i]["edges"] = json!(format!(
                "[{{\"kind\":\"requires\",\"to_id\":\"h{}\"}}]",
                i - 1
            ));
        }
        let p = plan(&nodes).unwrap();
        assert_eq!(p["tasks"].as_array().unwrap().len(), 400);
        assert_eq!(p["tasks"][399]["depth"], 99);
    }
    #[test]
    fn identities_and_memory_budget_are_enforced() {
        let n = node("a", "scenario", &[]);
        assert!(plan(&[n.clone(), n]).is_err());
        assert!(plan(&[]).is_err());
        assert!(
            plan(
                &(0..=MAX_NODES)
                    .map(|i| node(&format!("h{i}"), "scenario", &[]))
                    .collect::<Vec<_>>()
            )
            .is_err()
        );
    }
}
