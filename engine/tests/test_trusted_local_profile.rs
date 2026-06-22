use engine::trusted_local::{
    TrustedLocalProfileInput, TrustedLocalProfileStatus, TRUSTED_LOCAL_PROFILE_SCHEMA_VERSION,
};

fn ready_input() -> TrustedLocalProfileInput {
    TrustedLocalProfileInput {
        requested: true,
        auth_configured: true,
        endpoint_configured: true,
        credentials_available: true,
        pricing_configured: true,
        per_dispatch_cost_cap_configured: true,
        daily_cost_cap_configured: true,
        fusion_kill_switch: false,
        experiments_paused: false,
        experiments_kill_switch: false,
        auto_promotion_kill_switch: false,
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
        fusion_kill_switch: false,
        experiments_paused: false,
        experiments_kill_switch: false,
        auto_promotion_kill_switch: false,
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

#[test]
fn kill_and_pause_controls_override_ready_profile() {
    let mut input = ready_input();
    input.fusion_kill_switch = true;
    input.experiments_paused = true;
    input.experiments_kill_switch = true;
    input.auto_promotion_kill_switch = true;

    let status = TrustedLocalProfileStatus::resolve(input);

    assert!(status.ready);
    assert!(status.capabilities.provider_execution);
    assert!(!status.capabilities.adaptive_execution);
    assert!(!status.capabilities.default_routing);
    assert!(!status.capabilities.experiments);
    assert!(!status.capabilities.auto_promotion);
}
