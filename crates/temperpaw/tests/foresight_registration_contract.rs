//! Runtime contract for immutable Forecast initialization and its declared queue.
use serde_json::json;
use std::collections::BTreeMap;
use temper_jit::TransitionTable;

#[test]
fn strict_forecast_initializes_only_through_declared_register() {
    let source = include_str!("../../../os-apps/paw-foresight/specs/forecast.ioa.toml");
    let table = TransitionTable::from_ioa_source(source);
    assert!(table.strict_action_params);
    assert_eq!(table.initial_state, "Created");
    assert!(
        table
            .validate_initial_fields(&json!({"Id":"forecast-test"}))
            .is_ok()
    );
    assert!(
        table
            .validate_initial_fields(&json!({"probability":"0.55"}))
            .is_err()
    );
    let registration = json!({"registration_key":"key","world_id":"world","event_node_id":"event","question":"Future question?","probability":"0.55","base_probability":"0.55","model_version":"identity","learning_run_id":"","previous_forecast_id":"","evidence_kind":"simulated","resolve_by":"2025-06-01T00:00:00Z","market_ref":"","engine_version":"test","registered_at":"2025-03-01T00:00:00Z"});
    assert!(
        table
            .validate_action_params(
                "Register",
                &registration,
                &json!({}),
                &BTreeMap::new(),
                &BTreeMap::new()
            )
            .is_ok()
    );
    for forbidden in ["outcome", "brier", "resolved_at", "outcome_evidence_kind"] {
        let mut injected = registration.clone();
        injected[forbidden] = json!("forged");
        assert!(
            table
                .validate_action_params(
                    "Register",
                    &injected,
                    &json!({}),
                    &BTreeMap::new(),
                    &BTreeMap::new()
                )
                .is_err()
        );
    }
    let spec: toml::Value = toml::from_str(source).unwrap();
    for action in spec["action"].as_array().unwrap() {
        let accepts_prediction = action["params"]
            .as_array()
            .is_some_and(|ps| ps.iter().any(|p| p.as_str() == Some("probability")));
        if accepts_prediction {
            assert_eq!(
                action["from"].as_array().unwrap(),
                &vec![toml::Value::String("Created".into())]
            );
            assert_eq!(action["to"].as_str(), Some("Preregistered"));
        }
    }
}
