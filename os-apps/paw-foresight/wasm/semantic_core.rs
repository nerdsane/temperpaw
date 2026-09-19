// Pure bounded graph planner. Missing references, cycles and uncertainty remain explicit.
use serde_json::{json, Value};
use std::collections::{BTreeMap, BTreeSet};
pub const MODEL: &str = "jev-1.13.0";
pub const MAX_CALLS: usize = 320;
pub const MAX_DEPTH: usize = 8;
pub const MAX_MS: u64 = 600_000;
pub fn field<'a>(v: &'a Value, key: &str) -> &'a str {
    v.get("fields").unwrap_or(v).get(key).and_then(Value::as_str).unwrap_or("")
}
pub fn parse(raw: &str) -> Result<Value, String> {
    serde_json::from_str(raw).map_err(|_| "Invalid persisted semantic state".into())
}
pub fn plan(nodes: &[Value]) -> Result<Value, String> {
    if nodes.is_empty() || nodes.len() > 256 { return Err("A semantic run needs 1–256 world events".into()); }
    let mut by_id = BTreeMap::new();
    for n in nodes {
        let id = field(n, "Id");
        if id.is_empty() || by_id.insert(id.to_owned(), n).is_some() { return Err("Missing or duplicate event identity".into()); }
    }
    let mut active = BTreeSet::new(); let mut depths = BTreeMap::new();
    let mut tasks = vec![]; let mut issues = vec![];
    // Memoize intrinsic depth (longest prerequisite path), not first-visit DFS depth.
    // The node-count bound caps this traversal, independently of identifier ordering.
    fn walk(id: &str, by_id: &BTreeMap<String, &Value>, active: &mut BTreeSet<String>, depths: &mut BTreeMap<String, usize>, tasks: &mut Vec<Value>, issues: &mut Vec<Value>) -> usize {
        if active.contains(id) { issues.push(json!({"nodeId":id,"result":"cycle_detected"})); return MAX_DEPTH + 1; }
        if let Some(depth) = depths.get(id) { return *depth; }
        let Some(node) = by_id.get(id) else { issues.push(json!({"nodeId":id,"result":"missing_prerequisite"})); return 0; };
        active.insert(id.to_owned());
        let mut depth = 0;
        match parse(field(node,"edges")).ok().and_then(|v|v.as_array().cloned()) {
            Some(edges) => for edge in edges {
                if edge["kind"] == "requires" {
                    if let Some(target) = edge["to_id"].as_str().filter(|s|!s.is_empty()) { depth = depth.max(1 + walk(target,by_id,active,depths,tasks,issues)); }
                    else { issues.push(json!({"nodeId":id,"result":"invalid_reference"})); }
                }
            },
            None => issues.push(json!({"nodeId":id,"result":"invalid_edges"})),
        }
        active.remove(id); depths.insert(id.to_owned(), depth);
        if depth > MAX_DEPTH { issues.push(json!({"nodeId":id,"result":"depth_limit","depth":depth})); }
        else {
            tasks.push(json!({"nodeId":id,"function":"classify_gap","depth":depth}));
            tasks.push(json!({"nodeId":id,"function":"choose_next_operation","depth":depth}));
        }
        depth
    }
    for id in by_id.keys() { walk(id,&by_id,&mut active,&mut depths,&mut tasks,&mut issues); }
    Ok(json!({"schema":"foresight-native-semantic-v1","cursor":0,"tasks":tasks,"issues":issues,"results":{},"max_calls":MAX_CALLS,"max_depth":MAX_DEPTH,"time_budget_ms":MAX_MS}))
}
pub fn gap_criteria() -> Value {
    json!({"none":"No specific causal gap identified; this does not establish truth.","timing":"The supplied interval is materially too short.","prerequisite":"A necessary causal prerequisite is missing.","evidence":"The key premise lacks supporting evidence in the supplied input.","uncertain":"Cannot distinguish reliably."})
}
pub fn request(snapshot: &Value, program: &Value) -> Result<Value, String> {
    let i = program["cursor"].as_u64().ok_or("Missing cursor")? as usize;
    let task = &program["tasks"][i]; let id = task["nodeId"].as_str().ok_or("Missing task")?;
    let nodes = snapshot["nodes"].as_array().ok_or("Missing snapshot nodes")?;
    let node = nodes.iter().find(|n|field(n,"Id")==id).ok_or("Task references absent node")?;
    let edges = parse(field(node,"edges")).unwrap_or(json!([]));
    let prerequisites: Vec<Value> = edges.as_array().into_iter().flatten().filter(|e|e["kind"]=="requires").map(|edge| {
        let target=edge["to_id"].as_str().unwrap_or("");
        json!({"node":nodes.iter().find(|n|field(n,"Id")==target),"id":target,"assessment":program["results"][target]})
    }).collect();
    let (instructions,criteria) = if task["function"]=="classify_gap" {
        ("Classify the most consequential causal gap in state.node using its actual prerequisites and the supplied world evidence. A future event is a hypothesis, not a false observation. Source references alone do not establish source contents. Do not confuse probability with causal coherence. Return uncertain when the provided evidence cannot distinguish options.",gap_criteria())
    } else {
        ("Choose the highest-value next operation for this event using state.assessment and prerequisite results. This is a recommendation, never a claim that research or repair has executed.",json!({"research":"Acquire evidence for an unsupported premise.","repair":"Revise an inadequate mechanism or timing.","monitor":"Keep as a hypothesis and monitor dated signals.","uncertain":"The next operation is unclear."}))
    };
    let req=json!({"model":MODEL,"state":{"world":snapshot["world"],"node":node,"prerequisites":prerequisites,"assessment":program["results"][id]},"questions":{"result":{"type":"choice","instructions":instructions,"criteria":criteria}}});
    if req.to_string().len()>128*1024 { return Err("Semantic request exceeds 128 KB".into()); }
    Ok(req)
}
/// Select actual repair recommendations, spreading work across causal options.
/// Monitoring and research recommendations remain visible, never claimed executed.
pub fn repair_candidates(snapshot: &Value, program: &Value) -> Vec<Value> {
    let Some(nodes)=snapshot["nodes"].as_array() else { return vec![]; };
    let mut candidates:Vec<_>=nodes.iter().filter(|n|field(n,"kind")=="scenario")
        .filter(|n|program["results"][field(n,"Id")]["choose_next_operation"]=="repair").collect();
    let mut selected=vec![]; let mut covered=BTreeSet::new();
    while selected.len()<8 && !candidates.is_empty() {
        let novel=|n:&Value| -> Vec<String> {parse(field(n,"edges")).ok().and_then(|e|e.as_array().cloned()).unwrap_or_default().iter()
            .filter_map(|e|e["to_id"].as_str().map(str::to_owned)).filter(|id|!covered.contains(id)).collect()};
        let index=(0..candidates.len()).max_by_key(|i|novel(candidates[*i]).len()).unwrap();
        let node=candidates.remove(index);let new_ids=novel(node);covered.extend(new_ids);
        selected.push(json!({"node":node,"assessment":program["results"][field(node,"Id")]}));
    }
    selected
}
pub fn validate(request: &Value, response: &Value) -> Result<String, String> {
    if response["model"]!=MODEL {return Err("Provider model mismatch".into());}
    let a=&response["answers"]["result"];
    if a["type"]!="choice" {return Err("Provider answer is not a choice".into());}
    let opts=request["questions"]["result"]["criteria"].as_object().ok_or("Missing criteria")?;
    let p=a["probabilities"].as_object().ok_or("Missing distribution")?;
    if p.len()!=opts.len() {return Err("Wrong option count".into());}
    let mut sum=0.0;let mut max:f64=0.0;
    for k in opts.keys() {
        let n=p.get(k).and_then(Value::as_f64).ok_or("Missing probability")?;
        if !n.is_finite() || !(0.0..=1.0).contains(&n) {return Err("Invalid probability".into());}
        sum+=n;max=max.max(n);
    }
    if (sum-1.0).abs()>0.005*opts.len() as f64+f64::EPSILON {return Err("Invalid probability mass".into());}
    let selected=a["choice"].as_str().ok_or("Missing choice")?;
    if p.get(selected).and_then(Value::as_f64)!=Some(max) {return Err("Choice is not argmax".into());}
    Ok(if max<0.65 {"uncertain"} else {selected}.into())
}
#[cfg(test)]
mod tests {
    use super::*;
    fn node(id:&str,requires:&[&str])->Value {json!({"Id":id,"statement":id,"edges":serde_json::to_string(&requires.iter().map(|s|json!({"kind":"requires","to_id":s})).collect::<Vec<_>>()).unwrap()})}
    #[test] fn shared_prerequisites_are_evaluated_once_before_dependents() {
        let p=plan(&[node("a",&["shared"]),node("b",&["shared"]),node("shared",&[])]).unwrap();
        assert_eq!(p["tasks"].as_array().unwrap().len(),6);assert_eq!(p["tasks"][0]["nodeId"],"shared");
    }
    #[test] fn cycles_missing_edges_and_depth_are_not_success() {
        let p=plan(&[node("a",&["b","absent"]),node("b",&["a"])]).unwrap();
        let issues=p["issues"].to_string();assert!(issues.contains("cycle_detected"));assert!(issues.contains("missing_prerequisite"));
        let nodes:Vec<_>=(0..12).map(|i|node(&format!("n{i:02}"),&[&format!("n{:02}",i+1)])).collect();
        assert!(plan(&nodes).unwrap()["issues"].to_string().contains("depth_limit"));
    }
    #[test] fn chained_operation_uses_persisted_gap_assessment() {
        let nodes=vec![node("a",&[])];let mut p=plan(&nodes).unwrap();p["cursor"]=json!(1);p["results"]["a"]=json!({"classify_gap":"timing"});
        let r=request(&json!({"world":{},"nodes":nodes}),&p).unwrap();assert_eq!(r["state"]["assessment"]["classify_gap"],"timing");
        assert!(r["questions"]["result"]["criteria"].get("repair").is_some());
    }
    #[test] fn deepening_uses_recorded_repair_choices_and_diverse_dependencies() {
        let mut a=node("a", &["x"]);a["kind"]=json!("scenario");
        let mut b=node("b", &["y"]);b["kind"]=json!("scenario");
        let snapshot=json!({"nodes":[a,b]});
        let p=json!({"results":{"a":{"choose_next_operation":"monitor"},"b":{"choose_next_operation":"repair"}}});
        let selected=repair_candidates(&snapshot,&p);assert_eq!(selected.len(),1);assert_eq!(selected[0]["node"]["Id"],"b");
        assert!(repair_candidates(&snapshot,&json!({"results":{}})).is_empty());
    }
    #[test] fn provider_uncertainty_and_malformed_results_fail_closed() {
        let req=json!({"questions":{"result":{"criteria":{"none":"","uncertain":""}}}});
        let mut r=json!({"model":MODEL,"answers":{"result":{"type":"choice","choice":"none","probabilities":{"none":0.6,"uncertain":0.4}}}});
        assert_eq!(validate(&req,&r).unwrap(),"uncertain");
        r["answers"]["result"]["choice"]=json!("uncertain");assert!(validate(&req,&r).is_err());
        r["model"]=json!("other");assert!(validate(&req,&r).is_err());
    }
}

#[cfg(test)]
mod depth_regressions {
use serde_json::{json,Value};
fn node(id:&str,requires:Vec<String>)->Value{json!({"Id":id,"edges":requires.iter().map(|s|json!({"kind":"requires","to_id":s})).collect::<Vec<_>>().pipe_json()})}
trait JsonString {fn pipe_json(self)->String;} impl JsonString for Vec<Value>{fn pipe_json(self)->String{serde_json::to_string(&self).unwrap()}}
fn chain(size:usize,reverse:bool)->Vec<Value>{let id=|i|format!("n{:03}",if reverse{size-1-i}else{i});(0..size).map(|i|node(&id(i),if i==0{vec![]}else{vec![id(i-1)]})).collect()}
#[test]fn chain_depth_bound_is_id_order_independent(){for reverse in [false,true]{for size in [9,10,12,256]{let p=super::plan(&chain(size,reverse)).unwrap();let tasks=p["tasks"].as_array().unwrap();assert_eq!(tasks.len(),18);assert_eq!(tasks.iter().map(|t|t["depth"].as_u64().unwrap()).max(),Some(8));assert_eq!(p["issues"].as_array().unwrap().iter().filter(|i|i["result"]=="depth_limit").count(),size-9);}}}
#[test]fn dag_sharing_has_intrinsic_depth_and_order(){let p=super::plan(&[node("a",vec!["c".into(),"b".into()]),node("b",vec!["c".into()]),node("c",vec![])]).unwrap();let t=p["tasks"].as_array().unwrap();assert_eq!(t.len(),6);assert_eq!(t[0]["nodeId"],"c");assert_eq!(t[0]["depth"],0);assert_eq!(t[2]["nodeId"],"b");assert_eq!(t[2]["depth"],1);assert_eq!(t[4]["nodeId"],"a");assert_eq!(t[4]["depth"],2);assert_eq!(p["issues"],json!([]));}
#[test]fn cycle_excluded_missing_explicit_independent_survives(){let p=super::plan(&[node("a",vec!["b".into()]),node("b",vec!["a".into()]),node("c",vec!["missing".into()]),node("ok",vec![])]).unwrap();let s=p["issues"].to_string();assert!(s.contains("cycle_detected"));assert!(s.contains("missing_prerequisite"));let t=p["tasks"].as_array().unwrap();assert!(t.iter().all(|x|x["nodeId"]!="a"&&x["nodeId"]!="b"));assert!(t.iter().any(|x|x["nodeId"]=="ok"));}
#[test]fn input_bounds(){assert!(super::plan(&[]).is_err());assert!(super::plan(&chain(257,false)).is_err());let n=node("a",vec![]);assert!(super::plan(&[n.clone(),n]).is_err());}

}
