// Validate presentation probabilities separately from Jev's causal classifiers.
use serde_json::Value;
use std::collections::BTreeSet;

fn text(value: &Value, limit: usize) -> Result<&str, String> {
    value
        .as_str()
        .filter(|s| !s.trim().is_empty() && s.chars().count() <= limit)
        .ok_or_else(|| format!("Outlook text must contain 1–{limit} characters"))
}
fn list(value: &Value, min: usize, max: usize, limit: usize) -> Result<(), String> {
    let values = value
        .as_array()
        .filter(|v| (min..=max).contains(&v.len()))
        .ok_or("Invalid outlook list length")?;
    for v in values {
        text(v, limit)?;
    }
    Ok(())
}
/// The modeled scenarios are an explicit finite partition; `other` retains
/// probability mass for real futures outside that deliberately incomplete model.
pub fn validate(answer: &Value, snapshot: &Value) -> Result<(), String> {
    if answer["schema"] == "foresight-outlook-v2" {
        return validate_v2(answer, snapshot);
    }
    validate_v1(answer, snapshot)
}
fn validate_v1(answer: &Value, snapshot: &Value) -> Result<(), String> {
    if answer["schema"] != "foresight-outlook-v1"
        || answer["probability_basis"] != "subjective_model_estimate"
        || answer["calibrated"] != false
    {
        return Err(
            "Outlook must label probabilities as uncalibrated subjective model estimates".into(),
        );
    }
    text(&answer["headline"], 160)?;
    text(&answer["summary"], 400)?;
    if answer["horizon"] != snapshot["world"]["target_date"] {
        return Err("Outlook horizon differs from the question".into());
    }
    list(&answer["evidence_limits"], 1, 6, 240)?;
    list(&answer["research_questions"], 0, 8, 240)?;
    let scenarios: BTreeSet<_> = snapshot["nodes"]
        .as_array()
        .ok_or("Missing snapshot nodes")?
        .iter()
        .filter(|n| n["kind"] == "scenario")
        .filter_map(|n| n["Id"].as_str())
        .collect();
    if scenarios.is_empty() {
        return Err("Outlook requires modeled scenarios".into());
    }
    let outcomes = answer["outcomes"]
        .as_array()
        .filter(|v| (3..=5).contains(&v.len()))
        .ok_or("Expected 3–5 outcome buckets")?;
    let mut used = BTreeSet::new();
    let mut ids = BTreeSet::new();
    let mut total = 0.0;
    let mut other = false;
    for outcome in outcomes {
        let id = text(&outcome["id"], 50)?;
        if !ids.insert(id) {
            return Err("Repeated outcome identity".into());
        }
        text(&outcome["title"], 70)?;
        text(&outcome["definition"], 240)?;
        text(&outcome["narrative"], 360)?;
        list(&outcome["signals"], 1, 3, 160)?;
        list(&outcome["falsifiers"], 1, 3, 160)?;
        let p = outcome["probability"]
            .as_f64()
            .filter(|p| p.is_finite() && (0.0..=1.0).contains(p))
            .ok_or("Invalid subjective outcome probability")?;
        total += p;
        let refs = outcome["scenario_ids"]
            .as_array()
            .ok_or("Missing scenario membership")?;
        if id == "other" {
            if !refs.is_empty() {
                return Err("Residual other cannot overlap modeled scenarios".into());
            }
            other = true;
        } else {
            if refs.is_empty() {
                return Err("Outcome has no modeled scenarios".into());
            }
            for reference in refs {
                let reference = reference.as_str().ok_or("Invalid scenario reference")?;
                if !scenarios.contains(reference) || !used.insert(reference) {
                    return Err("Scenario membership is invented or overlapping".into());
                }
            }
        }
    }
    if !other || used != scenarios {
        return Err(
            "Outcome buckets must partition modeled scenarios and include residual other".into(),
        );
    }
    if (total - 1.0).abs() > 0.000001 {
        return Err("Subjective outcome probabilities must sum to one".into());
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    fn fixture() -> (Value, Value) {
        let outcome = |id: &str, refs: Vec<&str>, probability: f64| json!({"id":id,"title":"A future","definition":"Observable non-overlapping outcome rule","probability":probability,"scenario_ids":refs,"narrative":"Hypothetical outcome, not an observation.","signals":["A dated observable signal"],"falsifiers":["A measurable disconfirmation"]});
        (
            json!({"schema":"foresight-outlook-v1","headline":"Three alternatives","horizon":"2027-09-19","probability_basis":"subjective_model_estimate","calibrated":false,"summary":"A subjective distribution over mutually exclusive buckets, not measured accuracy.","evidence_limits":["Sparse evidence"],"research_questions":["What could change adoption?"],"outcomes":[outcome("a",vec!["s1"],0.5),outcome("b",vec!["s2"],0.35),outcome("other",vec![],0.15)]}),
            json!({"world":{"target_date":"2027-09-19"},"nodes":[{"Id":"s1","kind":"scenario"},{"Id":"s2","kind":"scenario"}]}),
        )
    }
    #[test]
    fn valid_subjective_partition_is_accepted() {
        let (a, s) = fixture();
        assert!(validate(&a, &s).is_ok());
    }
    #[test]
    fn malformed_probability_claims_fail_closed() {
        let (a, s) = fixture();
        for (key, value) in [
            ("calibrated", json!(true)),
            ("probability_basis", json!("jev_distribution")),
            ("horizon", json!("2028-01-01")),
        ] {
            let mut bad = a.clone();
            bad[key] = value;
            assert!(validate(&bad, &s).is_err());
        }
        let mut bad = a;
        bad["outcomes"][0]["probability"] = json!(0.7);
        assert!(validate(&bad, &s).is_err());
    }
    #[test]
    fn overlapping_missing_and_invented_memberships_fail() {
        let (a, s) = fixture();
        for refs in [json!(["s1"]), json!(["imaginary"]), json!([])] {
            let mut bad = a.clone();
            bad["outcomes"][1]["scenario_ids"] = refs;
            assert!(validate(&bad, &s).is_err());
        }
        let mut bad = a;
        bad["outcomes"][2]["id"] = json!("not-other");
        assert!(validate(&bad, &s).is_err());
    }
    #[test]
    fn excessive_or_empty_display_text_is_rejected() {
        let (a, s) = fixture();
        let mut bad = a.clone();
        bad["headline"] = json!("x".repeat(161));
        assert!(validate(&bad, &s).is_err());
        let mut bad = a;
        bad["outcomes"][0]["narrative"] = json!("");
        assert!(validate(&bad, &s).is_err());
    }
}

/// Independent event estimates may overlap and must never be normalized into a partition.
fn validate_v2(answer: &Value, snapshot: &Value) -> Result<(), String> {
    if answer["probability_model"] != "overlapping_events"
        || answer["probability_basis"] != "model_implied_event_estimate"
        || answer["calibrated"] != false
    {
        return Err("Outlook must identify overlapping uncalibrated event estimates".into());
    }
    text(&answer["headline"], 160)?;
    text(&answer["summary"], 400)?;
    if answer["horizon"] != snapshot["world"]["target_date"] {
        return Err("Outlook horizon differs from the question".into());
    }
    list(&answer["evidence_limits"], 1, 32, 240)?;
    list(&answer["research_questions"], 0, 64, 240)?;
    let hypotheses: BTreeSet<_> = snapshot["nodes"]
        .as_array()
        .ok_or("Missing snapshot nodes")?
        .iter()
        .filter(|n| matches!(n["kind"].as_str(), Some("scenario" | "revision")))
        .filter_map(|n| n["Id"].as_str())
        .collect();
    let outcomes = answer["outcomes"]
        .as_array()
        .filter(|v| (1..=64).contains(&v.len()))
        .ok_or("Expected 1–64 hypothesis outcomes")?;
    let mut ids = BTreeSet::new();
    for outcome in outcomes {
        if !ids.insert(text(&outcome["id"], 50)?) {
            return Err("Repeated outcome identity".into());
        }
        let hypothesis = outcome["hypothesis_id"]
            .as_str()
            .ok_or("Missing hypothesis identity")?;
        if !hypotheses.contains(hypothesis) {
            return Err("Outcome references an absent hypothesis".into());
        }
        text(&outcome["title"], 100)?;
        text(&outcome["definition"], 1000)?;
        text(&outcome["narrative"], 1200)?;
        list(&outcome["signals"], 1, 8, 240)?;
        list(&outcome["falsifiers"], 1, 8, 240)?;
        outcome["probability"]
            .as_f64()
            .filter(|p| p.is_finite() && (0.0..=1.0).contains(p))
            .ok_or("Invalid event probability")?;
        for reference in outcome["scenario_ids"]
            .as_array()
            .ok_or("Missing hypothesis references")?
        {
            if !reference.as_str().is_some_and(|id| hypotheses.contains(id)) {
                return Err("Invented hypothesis reference".into());
            }
        }
    }
    Ok(())
}
#[cfg(test)]
mod v2_tests {
    use super::*;
    use serde_json::json;
    fn fixture() -> (Value, Value) {
        let outcome = |id: &str, h: &str| json!({"id":id,"hypothesis_id":h,"title":"Future","definition":"Event by the horizon","narrative":"A mechanism and its implications","probability":0.8,"scenario_ids":["h1"],"signals":["Signal"],"falsifiers":["Disconfirmation"]});
        (
            json!({"schema":"foresight-outlook-v2","probability_model":"overlapping_events","probability_basis":"model_implied_event_estimate","calibrated":false,"headline":"Independent events","summary":"Both events may happen","horizon":"2027","evidence_limits":["Limited observations"],"research_questions":[],"outcomes":[outcome("a","h1"),outcome("b","h2")]}),
            json!({"world":{"target_date":"2027"},"nodes":[{"Id":"h1","kind":"scenario"},{"Id":"h2","kind":"revision"},{"Id":"e1","kind":"evidence"}]}),
        )
    }
    #[test]
    fn overlapping_probabilities_are_not_a_partition() {
        let (a, s) = fixture();
        assert!(validate(&a, &s).is_ok());
        let mut a = a;
        a["outcomes"].as_array_mut().unwrap().truncate(1);
        a["outcomes"][0]["scenario_ids"] = json!([]);
        assert!(validate(&a, &s).is_ok());
    }
    #[test]
    fn invented_or_evidence_hypotheses_fail() {
        let (a, s) = fixture();
        for id in ["missing", "e1"] {
            let mut b = a.clone();
            b["outcomes"][0]["hypothesis_id"] = json!(id);
            assert!(validate(&b, &s).is_err());
            let mut b = a.clone();
            b["outcomes"][0]["scenario_ids"] = json!([id]);
            assert!(validate(&b, &s).is_err());
        }
    }
    #[test]
    fn event_probability_and_resource_bounds() {
        let (a, s) = fixture();
        for p in [-0.1, 1.1] {
            let mut b = a.clone();
            b["outcomes"][0]["probability"] = json!(p);
            assert!(validate(&b, &s).is_err());
        }
        for (key, n) in [("title", 101), ("definition", 1001), ("narrative", 1201)] {
            let mut b = a.clone();
            b["outcomes"][0][key] = json!("x".repeat(n));
            assert!(validate(&b, &s).is_err());
        }
        let mut b = a.clone();
        b["outcomes"] = json!([]);
        assert!(validate(&b, &s).is_err());
        let mut b = a;
        b["calibrated"] = json!(true);
        assert!(validate(&b, &s).is_err());
    }
}
