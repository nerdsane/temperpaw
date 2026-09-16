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

#[test]
fn recovery_and_adoption_preserve_operational_state() {
    let table = TransitionTable::from_ioa_source(include_str!(
        "../../../os-apps/paw-foresight/specs/world.ioa.toml"
    ));
    let recovered = table
        .evaluate("RegisteringForecasts", 0, "ForecastRegistrationFailed")
        .unwrap();
    assert!(recovered.success);
    assert_eq!(recovered.new_state, "Active");
    for state in ["Active", "Updating", "RegisteringForecasts"] {
        let adopted = table.evaluate(state, 0, "AdoptModel").unwrap();
        assert!(adopted.success, "{state}");
        assert_eq!(adopted.new_state, state);
    }
    for state in ["Created", "Archived", "Failed"] {
        assert!(
            !table
                .evaluate(state, 0, "AdoptModel")
                .is_some_and(|r| r.success)
        );
    }
}

#[test]
fn hundred_batch_scores_have_one_declared_learning_trigger() {
    let source = include_str!("../../../os-apps/paw-foresight/specs/forecast.ioa.toml");
    let table = TransitionTable::from_ioa_source(source);
    let forecast: toml::Value = toml::from_str(source).unwrap();
    let action = forecast["action"]
        .as_array()
        .unwrap()
        .iter()
        .find(|a| a["name"].as_str() == Some("ScoreBatch"))
        .unwrap();
    let mut spawned = 0;
    for _ in 0..100 {
        let scored = table.evaluate("Resolved", 0, "ScoreBatch").unwrap();
        assert!(scored.success);
        assert_eq!(scored.new_state, "Scored");
        spawned += action
            .get("triggers")
            .and_then(toml::Value::as_array)
            .map_or(0, Vec::len);
    }
    assert_eq!(spawned, 0);
    let hindcast: toml::Value = toml::from_str(include_str!(
        "../../../os-apps/paw-foresight/specs/hindcast.ioa.toml"
    ))
    .unwrap();
    let done = hindcast["action"]
        .as_array()
        .unwrap()
        .iter()
        .find(|a| a["name"].as_str() == Some("ScoreComplete"))
        .unwrap();
    let triggers = done["triggers"].as_array().unwrap();
    assert_eq!(triggers.len(), 1);
    assert_eq!(triggers[0]["target_entity"].as_str(), Some("LearningRun"));
    assert_eq!(triggers[0]["target_action"].as_str(), Some("Start"));
    assert!(triggers[0].get("guard").is_some());
}

#[test]
fn changed_action_metadata_matches_ioa_and_detects_missing_parameters() {
    use std::collections::BTreeSet;
    use temper_spec::{
        automaton::parse_automaton,
        csdl::{CsdlDocument, parse_csdl},
    };
    let xml = include_str!("../../../os-apps/paw-foresight/specs/model.csdl.xml");
    let targets = [
        (
            "World",
            include_str!("../../../os-apps/paw-foresight/specs/world.ioa.toml"),
            &[
                "Configure",
                "ResearchSessionStarted",
                "ForecastPrepared",
                "ForecastRegistrationComplete",
                "ForecastRegistrationFailed",
            ][..],
        ),
        (
            "Forecast",
            include_str!("../../../os-apps/paw-foresight/specs/forecast.ioa.toml"),
            &["ScoreBatch"][..],
        ),
        (
            "Hindcast",
            include_str!("../../../os-apps/paw-foresight/specs/hindcast.ioa.toml"),
            &["ScoreComplete"][..],
        ),
    ];
    let mut expected = BTreeMap::new();
    for (entity, source, names) in targets {
        let ioa = parse_automaton(source).unwrap();
        for name in names {
            let action = ioa.actions.iter().find(|a| a.name == *name).unwrap();
            let params: BTreeSet<String> =
                action.params.iter().map(|p| p.name().to_string()).collect();
            expected.insert(
                (format!("TemperPaw.Foresight.{entity}"), name.to_string()),
                params,
            );
        }
    }
    let matches = |document: &CsdlDocument| {
        expected.iter().all(|((binding, name), params)| {
            let actions: Vec<_> = document
                .schemas
                .iter()
                .flat_map(|s| &s.actions)
                .filter(|a| a.name == *name && a.binding_type() == Some(binding.as_str()))
                .collect();
            actions.len() == 1
                && actions[0]
                    .parameters
                    .iter()
                    .skip(1)
                    .map(|p| p.name.clone())
                    .collect::<BTreeSet<_>>()
                    == *params
        })
    };
    assert!(matches(&parse_csdl(xml).unwrap()));
    // Exercise each changed contract's missing-parameter failure with the real parser.
    for (entity, name, parameter) in [
        ("World", "Configure", "last_ingest_date"),
        ("World", "ResearchSessionStarted", "research_session_id"),
        (
            "World",
            "ResearchSessionStarted",
            "expected_research_attempt",
        ),
        ("World", "ForecastPrepared", "registration_model_json"),
        ("World", "ForecastPrepared", "learning_mode"),
        ("World", "ForecastRegistrationComplete", "learning_mode"),
        ("World", "ForecastRegistrationFailed", "error_message"),
        ("Forecast", "ScoreBatch", "brier"),
        ("Hindcast", "ScoreComplete", "learning_as_of"),
    ] {
        let mut mutant = parse_csdl(xml).unwrap();
        let binding = format!("TemperPaw.Foresight.{entity}");
        let action = mutant
            .schemas
            .iter_mut()
            .flat_map(|s| &mut s.actions)
            .find(|a| a.name == name && a.binding_type() == Some(binding.as_str()))
            .unwrap();
        let before = action.parameters.len();
        action.parameters.retain(|p| p.name != parameter);
        assert_eq!(action.parameters.len() + 1, before);
        assert!(
            !matches(&mutant),
            "{entity}.{name} missing {parameter} escaped the check"
        );
    }
}

#[test]
fn research_session_callback_preserves_world_progress_and_is_declared_on_each_seed() {
    let source = include_str!("../../../os-apps/paw-foresight/specs/world.ioa.toml");
    let table = TransitionTable::from_ioa_source(source);
    for state in [
        "Seeding",
        "Active",
        "Updating",
        "RegisteringForecasts",
        "Archived",
        "Failed",
    ] {
        let result = table.evaluate(state, 0, "ResearchSessionStarted").unwrap();
        assert!(result.success, "{state}");
        assert_eq!(result.new_state, state);
    }
    for state in ["Created", "OpeningReplay"] {
        assert!(
            !table
                .evaluate(state, 0, "ResearchSessionStarted")
                .is_some_and(|r| r.success)
        );
    }
    assert!(
        table
            .validate_initial_fields(&json!({"research_session_id":"forged"}))
            .is_err()
    );
    let spec: toml::Value = toml::from_str(source).unwrap();
    for name in ["Seed", "ResumeSeed"] {
        let action = spec["action"]
            .as_array()
            .unwrap()
            .iter()
            .find(|action| action["name"].as_str() == Some(name))
            .unwrap();
        let trigger = action["triggers"]
            .as_array()
            .unwrap()
            .iter()
            .find(|trigger| trigger["module"].as_str() == Some("seed_world"))
            .unwrap();
        assert_eq!(
            trigger.get("on_success").and_then(toml::Value::as_str),
            Some("ResearchSessionStarted")
        );
    }
}

#[test]
fn older_research_attempt_cannot_replace_the_current_session() {
    use std::sync::Arc;
    use temper_runtime::scheduler::SimActorHandler;
    use temper_server::entity_actor::sim_handler::EntityActorHandler;

    fn reverse_callbacks(
        table: TransitionTable,
        completed: bool,
    ) -> Result<serde_json::Value, String> {
        let mut world = EntityActorHandler::new("World", "world-1", Arc::new(table));
        world.init().unwrap();
        let first = world.handle_message("Seed", "{}").unwrap();
        assert_eq!(first["counters"]["research_attempt"], 1);
        let second = world.handle_message("ResumeSeed", "{}").unwrap();
        assert_eq!(second["counters"]["research_attempt"], 2);
        let current = world
            .handle_message(
                "ResearchSessionStarted",
                &json!({"research_session_id":"session-current","expected_research_attempt":2})
                    .to_string(),
            )
            .unwrap();
        assert_eq!(current["fields"]["research_session_id"], "session-current");
        if completed {
            let current = world.handle_message("SeedComplete",
                &json!({"skeleton_node_count":"1","graph_snapshot_file_id":"","uncertainty_axes":"[]"}).to_string()).unwrap();
            assert_eq!(current["status"], "Active");
        }
        let delayed = world.handle_message(
            "ResearchSessionStarted",
            &json!({"research_session_id":"session-old","expected_research_attempt":1}).to_string(),
        );
        assert_eq!(
            world.current_status(),
            if completed { "Active" } else { "Seeding" }
        );
        delayed
    }

    let source = include_str!("../../../os-apps/paw-foresight/specs/world.ioa.toml");
    for completed in [false, true] {
        assert!(reverse_callbacks(TransitionTable::from_ioa_source(source), completed).is_err());
        // Removing the guard must reproduce the overwrite with the same actor driver.
        let mut mutant = TransitionTable::from_ioa_source(source);
        let constraints = &mut mutant
            .action_contracts
            .get_mut("ResearchSessionStarted")
            .unwrap()
            .constraints;
        let before = constraints.len();
        constraints.retain(|constraint| constraint.param() != "expected_research_attempt");
        assert_eq!(constraints.len() + 1, before);
        let overwritten = reverse_callbacks(mutant, completed).unwrap();
        assert_eq!(overwritten["fields"]["research_session_id"], "session-old");
    }
}
