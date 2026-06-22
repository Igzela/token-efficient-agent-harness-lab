use engine::trusted_local::{
    EffectiveExecutionGates, TrustedLocalProfileInput, TrustedLocalProfileStatus,
    TrustedLocalTaskAdvancementStatus, TRUSTED_LOCAL_PROFILE_SCHEMA_VERSION,
    TRUSTED_LOCAL_TASK_ADVANCEMENT_SCHEMA_VERSION,
};
use std::collections::BTreeMap;

fn ready_input() -> TrustedLocalProfileInput {
    TrustedLocalProfileInput {
        requested: true,
        auth_configured: true,
        endpoint_configured: true,
        credentials_available: true,
        pricing_configured: true,
        per_dispatch_cost_cap_configured: true,
        daily_cost_cap_configured: true,
    }
}

#[test]
fn profile_is_inert_when_not_requested() {
    let mut input = ready_input();
    input.requested = false;

    let status = TrustedLocalProfileStatus::resolve(input);

    assert_eq!(status.schema_version, TRUSTED_LOCAL_PROFILE_SCHEMA_VERSION);
    assert!(!status.requested);
    assert!(!status.ready);
    assert!(status.blockers.is_empty());
    assert!(!status.capabilities.provider_execution);
    assert!(!status.capabilities.adaptive_execution);
    assert!(!status.capabilities.default_routing);
    assert!(!status.capabilities.experiments);
    assert!(!status.capabilities.auto_promotion);
}

#[test]
fn profile_fails_closed_with_stable_readiness_blockers() {
    let status = TrustedLocalProfileStatus::resolve(TrustedLocalProfileInput {
        requested: true,
        auth_configured: false,
        endpoint_configured: false,
        credentials_available: false,
        pricing_configured: false,
        per_dispatch_cost_cap_configured: false,
        daily_cost_cap_configured: false,
    });

    assert!(!status.ready);
    assert_eq!(
        status.blockers,
        vec![
            "auth_not_configured",
            "daily_cost_cap_not_configured",
            "endpoint_not_configured",
            "endpoint_pricing_not_configured",
            "per_dispatch_cost_cap_not_configured",
            "provider_credential_not_available",
        ]
    );
    assert!(!status.capabilities.provider_execution);
    assert!(!status.capabilities.adaptive_execution);
    assert!(!status.capabilities.default_routing);
}

#[test]
fn ready_profile_enables_bounded_live_capabilities() {
    let status = TrustedLocalProfileStatus::resolve(ready_input());

    assert!(status.ready);
    assert!(status.blockers.is_empty());
    assert!(status.capabilities.provider_execution);
    assert!(status.capabilities.adaptive_execution);
    assert!(status.capabilities.default_routing);
    assert!(status.capabilities.experiments);
    assert!(status.capabilities.auto_promotion);
}

fn ready_environment() -> BTreeMap<String, String> {
    BTreeMap::from([
        ("ACP_TRUSTED_LOCAL_PROFILE".to_string(), "1".to_string()),
        ("ACP_REQUIRE_AUTH".to_string(), "1".to_string()),
        (
            "ACP_ADMIN_API_KEY".to_string(),
            format!("harness_{}", "a".repeat(64)),
        ),
        (
            "ACP_ADAPTIVE_PROVIDER_ENDPOINTS_JSON".to_string(),
            r#"[{"endpoint_id":"local-stub","provider_type":"stub","model":"stub-model","timeout_ms":30000,"input_cost_per_1k_usd":0.001,"output_cost_per_1k_usd":0.002}]"#
                .to_string(),
        ),
        (
            "ACP_COST_PER_DISPATCH_USD".to_string(),
            "1.0".to_string(),
        ),
        ("ACP_COST_DAILY_USD".to_string(), "10.0".to_string()),
    ])
}

#[test]
fn environment_lookup_resolves_ready_stub_profile_without_process_env_mutation() {
    let environment = ready_environment();

    let status = TrustedLocalProfileStatus::from_lookup(|key| environment.get(key).cloned());

    assert!(status.requested);
    assert!(status.ready);
    assert!(status.capabilities.provider_execution);
    assert!(status.capabilities.default_routing);
}

#[test]
fn runtime_kill_controls_do_not_deconfigure_ready_profile() {
    let mut environment = ready_environment();
    environment.insert(
        "ACP_ADAPTIVE_FUSION_KILL_SWITCH".to_string(),
        "1".to_string(),
    );
    environment.insert(
        "ACP_ADAPTIVE_EXPERIMENTS_PAUSED".to_string(),
        "1".to_string(),
    );
    environment.insert(
        "ACP_ADAPTIVE_EXPERIMENTS_KILL_SWITCH".to_string(),
        "1".to_string(),
    );
    environment.insert(
        "ACP_ADAPTIVE_AUTO_PROMOTION_KILL_SWITCH".to_string(),
        "1".to_string(),
    );

    let gates = EffectiveExecutionGates::from_lookup(|key| environment.get(key).cloned());

    assert!(gates.profile.ready);
    assert!(gates.adaptive_execution);
    assert!(gates.default_routing);
    assert!(gates.experiments_enabled);
    assert!(gates.auto_promotion_enabled);
}

#[test]
fn environment_lookup_requires_real_provider_credential_value() {
    let mut environment = ready_environment();
    environment.insert(
        "ACP_ADAPTIVE_PROVIDER_ENDPOINTS_JSON".to_string(),
        r#"[{"endpoint_id":"quality","provider_type":"anthropic","base_url":"https://api.anthropic.com","model":"quality-model","credential_env":"QUALITY_PROVIDER_KEY","timeout_ms":30000,"input_cost_per_1k_usd":0.003,"output_cost_per_1k_usd":0.015}]"#.to_string(),
    );

    let blocked = TrustedLocalProfileStatus::from_lookup(|key| environment.get(key).cloned());
    assert!(!blocked.ready);
    assert_eq!(blocked.blockers, vec!["provider_credential_not_available"]);

    environment.insert(
        "QUALITY_PROVIDER_KEY".to_string(),
        "test-provider-value".to_string(),
    );
    let ready = TrustedLocalProfileStatus::from_lookup(|key| environment.get(key).cloned());
    assert!(ready.ready);
}

#[test]
fn environment_lookup_fails_closed_for_invalid_endpoint_config() {
    let mut environment = ready_environment();
    environment.insert(
        "ACP_ADAPTIVE_PROVIDER_ENDPOINTS_JSON".to_string(),
        r#"[{"endpoint_id":"bad","provider_type":"anthropic","base_url":"http://metadata.internal","model":"bad","credential_env":"TEST_PROVIDER_KEY","timeout_ms":30000,"input_cost_per_1k_usd":0.003,"output_cost_per_1k_usd":0.015}]"#.to_string(),
    );
    environment.insert(
        "TEST_PROVIDER_KEY".to_string(),
        "provider-fixture".to_string(),
    );

    let status = TrustedLocalProfileStatus::from_lookup(|key| environment.get(key).cloned());

    assert!(!status.ready);
    assert!(status
        .blockers
        .iter()
        .any(|blocker| blocker == "endpoint_not_configured"));
}

#[test]
fn environment_lookup_rejects_empty_endpoints_and_non_positive_pricing() {
    let mut environment = ready_environment();
    environment.insert(
        "ACP_ADAPTIVE_PROVIDER_ENDPOINTS_JSON".to_string(),
        "[]".to_string(),
    );

    let empty = TrustedLocalProfileStatus::from_lookup(|key| environment.get(key).cloned());
    assert!(!empty.ready);
    assert_eq!(
        empty.blockers,
        vec![
            "endpoint_not_configured",
            "endpoint_pricing_not_configured",
            "provider_credential_not_available",
        ]
    );

    environment.insert(
        "ACP_ADAPTIVE_PROVIDER_ENDPOINTS_JSON".to_string(),
        r#"[{"endpoint_id":"free-input","provider_type":"stub","model":"stub-model","timeout_ms":30000,"input_cost_per_1k_usd":0.0,"output_cost_per_1k_usd":0.002}]"#
            .to_string(),
    );
    let non_positive = TrustedLocalProfileStatus::from_lookup(|key| environment.get(key).cloned());
    assert!(!non_positive.ready);
    assert_eq!(
        non_positive.blockers,
        vec!["endpoint_pricing_not_configured"]
    );
}

#[test]
fn effective_gates_preserve_explicit_legacy_flags_without_profile() {
    let environment = BTreeMap::from([
        ("ACP_ENABLE_PROVIDER_EXECUTION".to_string(), "1".to_string()),
        (
            "ACP_ENABLE_ADAPTIVE_FUSION_EXECUTION".to_string(),
            "true".to_string(),
        ),
        (
            "ACP_ADAPTIVE_DEFAULT_LIVE_ROUTING".to_string(),
            "1".to_string(),
        ),
        (
            "ACP_ENABLE_ADAPTIVE_EXPERIMENTS".to_string(),
            "1".to_string(),
        ),
        (
            "ACP_ADAPTIVE_EXPERIMENTS_ACTIVE".to_string(),
            "1".to_string(),
        ),
        (
            "ACP_ENABLE_ADAPTIVE_AUTO_PROMOTION".to_string(),
            "1".to_string(),
        ),
        (
            "ACP_ADAPTIVE_AUTO_PROMOTION_ACTIVE".to_string(),
            "1".to_string(),
        ),
    ]);

    let gates = EffectiveExecutionGates::from_lookup(|key| environment.get(key).cloned());

    assert!(!gates.profile.requested);
    assert!(gates.provider_execution);
    assert!(gates.adaptive_execution);
    assert!(gates.default_routing);
    assert!(gates.experiments_enabled);
    assert!(gates.experiments_active);
    assert!(gates.auto_promotion_enabled);
    assert!(gates.auto_promotion_active);
}

#[test]
fn effective_gates_promote_ready_profile_capabilities() {
    let environment = ready_environment();

    let gates = EffectiveExecutionGates::from_lookup(|key| environment.get(key).cloned());

    assert!(gates.profile.ready);
    assert!(gates.provider_execution);
    assert!(gates.adaptive_execution);
    assert!(gates.default_routing);
    assert!(gates.experiments_enabled);
    assert!(gates.experiments_active);
    assert!(gates.auto_promotion_enabled);
    assert!(gates.auto_promotion_active);
}

#[test]
fn effective_gates_do_not_elevate_blocked_profile() {
    let mut environment = ready_environment();
    environment.remove("ACP_COST_DAILY_USD");

    let gates = EffectiveExecutionGates::from_lookup(|key| environment.get(key).cloned());

    assert!(gates.profile.requested);
    assert!(!gates.profile.ready);
    assert!(!gates.provider_execution);
    assert!(!gates.adaptive_execution);
    assert!(!gates.default_routing);
    assert!(!gates.experiments_enabled);
    assert!(!gates.auto_promotion_enabled);
}

#[test]
fn task_advancement_is_inert_until_explicitly_requested() {
    let environment = ready_environment();

    let status =
        TrustedLocalTaskAdvancementStatus::from_lookup(|key| environment.get(key).cloned());

    assert_eq!(
        status.schema_version,
        TRUSTED_LOCAL_TASK_ADVANCEMENT_SCHEMA_VERSION
    );
    assert!(!status.requested);
    assert!(!status.ready);
    assert!(status.blockers.is_empty());
    assert_eq!(status.executor_type, "adaptive_provider");
    assert_eq!(status.worker_count, 1);
    assert_eq!(status.max_concurrent, 4);
}

#[test]
fn task_advancement_requires_ready_profile_and_adaptive_executor() {
    let mut environment = ready_environment();
    environment.insert(
        "ACP_TRUSTED_LOCAL_TASK_ADVANCEMENT".to_string(),
        "1".to_string(),
    );
    environment.remove("ACP_COST_DAILY_USD");
    environment.insert(
        "ACP_SCHEDULER_EXECUTOR".to_string(),
        "codex_cli".to_string(),
    );

    let status =
        TrustedLocalTaskAdvancementStatus::from_lookup(|key| environment.get(key).cloned());

    assert!(!status.ready);
    assert_eq!(
        status.blockers,
        vec![
            "trusted_local_profile_not_ready",
            "scheduler_executor_not_adaptive_provider",
        ]
    );
}

#[test]
fn task_advancement_accepts_bounded_default_scheduler_configuration() {
    let mut environment = ready_environment();
    environment.insert(
        "ACP_TRUSTED_LOCAL_TASK_ADVANCEMENT".to_string(),
        "1".to_string(),
    );

    let status =
        TrustedLocalTaskAdvancementStatus::from_lookup(|key| environment.get(key).cloned());
    let gates = EffectiveExecutionGates::from_lookup(|key| environment.get(key).cloned());

    assert!(status.ready);
    assert!(status.blockers.is_empty());
    assert_eq!(status.executor_type, "adaptive_provider");
    assert_eq!(status.worker_count, 1);
    assert_eq!(status.max_concurrent, 4);
    assert!(gates.scheduler_enabled);
    assert!(gates.supervised_workers_enabled);
}

#[test]
fn task_advancement_rejects_unbounded_scheduler_configuration() {
    let mut environment = ready_environment();
    environment.extend([
        (
            "ACP_TRUSTED_LOCAL_TASK_ADVANCEMENT".to_string(),
            "1".to_string(),
        ),
        ("ACP_SUPERVISED_WORKER_COUNT".to_string(), "5".to_string()),
        ("ACP_SCHEDULER_MAX_CONCURRENT".to_string(), "4".to_string()),
        ("ACP_SCHEDULER_INTERVAL_MS".to_string(), "10".to_string()),
        (
            "ACP_SCHEDULER_LEASE_TIMEOUT_MS".to_string(),
            "999".to_string(),
        ),
    ]);

    let status =
        TrustedLocalTaskAdvancementStatus::from_lookup(|key| environment.get(key).cloned());

    assert!(!status.ready);
    assert_eq!(
        status.blockers,
        vec![
            "worker_count_exceeds_max_concurrent",
            "scheduler_interval_out_of_bounds",
            "scheduler_lease_timeout_out_of_bounds",
        ]
    );
}

#[test]
fn task_advancement_rejects_explicitly_invalid_numeric_configuration() {
    let mut environment = ready_environment();
    environment.extend([
        (
            "ACP_TRUSTED_LOCAL_TASK_ADVANCEMENT".to_string(),
            "1".to_string(),
        ),
        ("ACP_SUPERVISED_WORKER_COUNT".to_string(), "0".to_string()),
        (
            "ACP_SCHEDULER_MAX_CONCURRENT".to_string(),
            "invalid".to_string(),
        ),
        (
            "ACP_SCHEDULER_INTERVAL_MS".to_string(),
            "invalid".to_string(),
        ),
        (
            "ACP_SCHEDULER_LEASE_TIMEOUT_MS".to_string(),
            "0".to_string(),
        ),
    ]);

    let status =
        TrustedLocalTaskAdvancementStatus::from_lookup(|key| environment.get(key).cloned());

    assert!(!status.ready);
    assert_eq!(status.worker_count, 0);
    assert_eq!(status.max_concurrent, 0);
    assert_eq!(
        status.blockers,
        vec![
            "worker_count_out_of_bounds",
            "scheduler_max_concurrent_out_of_bounds",
            "scheduler_interval_out_of_bounds",
            "scheduler_lease_timeout_out_of_bounds",
        ]
    );
}

#[test]
fn effective_gates_preserve_legacy_scheduler_flags_without_task_advancement() {
    let environment = BTreeMap::from([
        ("ACP_ENABLE_SCHEDULER".to_string(), "1".to_string()),
        ("ACP_ENABLE_SUPERVISED_WORKERS".to_string(), "1".to_string()),
    ]);

    let gates = EffectiveExecutionGates::from_lookup(|key| environment.get(key).cloned());

    assert!(!gates.task_advancement.requested);
    assert!(gates.scheduler_enabled);
    assert!(gates.supervised_workers_enabled);
}
