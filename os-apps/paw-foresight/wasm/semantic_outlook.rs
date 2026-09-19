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
