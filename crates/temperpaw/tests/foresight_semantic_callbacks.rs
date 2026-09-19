#[test]
fn semantic_callbacks_remain_machine_owned() {
    use temper_authz::{AuthzEngine, PrincipalKind, SecurityContext};
    let e = AuthzEngine::new(
        &std::fs::read_to_string(
            std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
                .join("../../os-apps/paw-foresight/policies/foresight.cedar"),
        )
        .unwrap(),
    )
    .unwrap();
    let admin =
        SecurityContext::from_verified_jwt("owner", PrincipalKind::Admin, None, None, None, None);
    let system = SecurityContext::from_resolved_identity("scheduler", "system", None);
    let wasm = SecurityContext::from_resolved_identity("service:wasm-runtime", "service", None);
    let attrs = std::collections::HashMap::new();
    for a in ["Fail", "CheckReasoning", "SpawnReasoning", "Complete"] {
        assert!(
            !e.authorize(&admin, a, "SemanticRun", &attrs).is_allowed(),
            "admin {a}"
        );
    }
    for a in ["create", "Start", "Cancel"] {
        assert!(
            e.authorize(&admin, a, "SemanticRun", &attrs).is_allowed(),
            "admin {a}"
        );
    }
    for a in ["Fail", "CheckReasoning", "SpawnReasoning"] {
        assert!(
            e.authorize(&system, a, "SemanticRun", &attrs).is_allowed(),
            "system {a}"
        );
    }
    assert!(
        e.authorize(&wasm, "Fail", "SemanticRun", &attrs)
            .is_allowed()
    );
}
