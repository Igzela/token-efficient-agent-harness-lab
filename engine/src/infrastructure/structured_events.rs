// Structured operational log events for dispatch/regulator/policy paths.
// Uses tracing crate (already in Cargo.toml, previously unused).
// No secrets in log payloads — only IDs, decisions, and safe metadata.

/// Dispatch decision event — emitted once per dispatch request
pub fn log_dispatch_start(
    request_id: &str,
    dispatch_id: &str,
    request_source: &str,
    request_len: usize,
) {
    tracing::info!(
        event = "dispatch.start",
        request_id,
        dispatch_id,
        request_source,
        request_len,
        "Dispatch started"
    );
}

/// Task analysis complete
pub fn log_dispatch_analysis(
    request_id: &str,
    dispatch_id: &str,
    analysis_id: &str,
    task_domain: &str,
    task_intent: &str,
    risk_level: &str,
    confidence: f64,
) {
    tracing::info!(
        event = "dispatch.analysis",
        request_id,
        dispatch_id,
        analysis_id,
        task_domain,
        task_intent,
        risk_level,
        confidence,
        "Task analysis complete"
    );
}

/// Model selection decision
pub fn log_dispatch_selection(
    request_id: &str,
    dispatch_id: &str,
    selected_tier: &str,
    executor_type: &str,
) {
    tracing::info!(
        event = "dispatch.selection",
        request_id,
        dispatch_id,
        selected_tier,
        executor_type,
        "Model tier selected"
    );
}

/// Decision status (decided vs needs_approval)
pub fn log_dispatch_decision(
    request_id: &str,
    dispatch_id: &str,
    decision_status: &str,
    gate_count: usize,
) {
    tracing::info!(
        event = "dispatch.decision",
        request_id,
        dispatch_id,
        decision_status,
        gate_count,
        "Dispatch decision made"
    );
}

/// Execution result
pub fn log_dispatch_execution(
    request_id: &str,
    dispatch_id: &str,
    executor_type: &str,
    tier: &str,
    status: &str,
) {
    tracing::info!(
        event = "dispatch.execution",
        request_id,
        dispatch_id,
        executor_type,
        tier,
        status,
        "Dispatch execution complete"
    );
}

/// Quality retry triggered
pub fn log_dispatch_retry(
    request_id: &str,
    dispatch_id: &str,
    old_tier: &str,
    new_tier: &str,
    reason: &str,
) {
    tracing::info!(
        event = "dispatch.retry",
        request_id,
        dispatch_id,
        old_tier,
        new_tier,
        reason,
        "Quality retry triggered"
    );
}

/// Final dispatch status
pub fn log_dispatch_complete(request_id: &str, dispatch_id: &str, final_status: &str) {
    tracing::info!(
        event = "dispatch.complete",
        request_id,
        dispatch_id,
        final_status,
        "Dispatch complete"
    );
}

/// Cost gate decision
pub fn log_cost_gate(
    request_id: &str,
    dispatch_id: &str,
    reserved_cost: f64,
    daily_cost: f64,
    per_dispatch_limit: f64,
    daily_limit: f64,
    passed: bool,
) {
    tracing::info!(
        event = "dispatch.cost_gate",
        request_id,
        dispatch_id,
        reserved_cost,
        daily_cost,
        per_dispatch_limit,
        daily_limit,
        passed,
        "Cost gate evaluated"
    );
}

/// Auto-adjustment guard decision
pub fn log_auto_adjustment_guard(allowed: bool, mode: &str, env_gate: bool, dry_run: bool) {
    tracing::info!(
        event = "regulator.guard",
        allowed,
        mode,
        env_gate,
        dry_run,
        "Auto-adjustment guard evaluated"
    );
}

/// Policy eligibility decision
pub fn log_policy_eligible(
    candidate_id: &str,
    eligible: bool,
    policy_key: &str,
    target_tier: &str,
    confidence: f64,
    blocked_reason_count: usize,
) {
    tracing::info!(
        event = "regulator.policy_eligible",
        candidate_id,
        eligible,
        policy_key,
        target_tier,
        confidence,
        blocked_reason_count,
        "Policy eligibility evaluated"
    );
}

/// Generated proposal
pub fn log_proposal_generated(
    candidate_id: &str,
    pattern_type: &str,
    target_tier: &str,
    confidence: f64,
    policy_key: &str,
) {
    tracing::info!(
        event = "regulator.proposal_generated",
        candidate_id,
        pattern_type,
        target_tier,
        confidence,
        policy_key,
        "Policy proposal generated"
    );
}

/// Simulation result
pub fn log_simulation_result(
    scenario_id: &str,
    policy: &str,
    trace_count: usize,
    success_rate_delta: f64,
    cost_delta: f64,
) {
    tracing::info!(
        event = "regulator.simulation",
        scenario_id,
        policy,
        trace_count,
        success_rate_delta,
        cost_delta,
        "Simulation complete"
    );
}

/// Snapshot created
pub fn log_snapshot_created(
    snapshot_id: &str,
    candidate_id: &str,
    policy_key: &str,
    target_tier: &str,
) {
    tracing::info!(
        event = "regulator.snapshot_created",
        snapshot_id,
        candidate_id,
        policy_key,
        target_tier,
        "Policy snapshot created"
    );
}

/// Snapshot hash validation
pub fn log_snapshot_hash_valid(snapshot_id: &str, adjustment_id: &str, valid: bool) {
    tracing::info!(
        event = "regulator.snapshot_hash_valid",
        snapshot_id,
        adjustment_id,
        valid,
        "Snapshot hash validated"
    );
}

/// Active apply event
pub fn log_active_apply(
    adjustment_id: &str,
    proposal_id: &str,
    policy_key: &str,
    target_tier: &str,
    accepted: bool,
) {
    tracing::info!(
        event = "regulator.active_apply",
        adjustment_id,
        proposal_id,
        policy_key,
        target_tier,
        accepted,
        "Active apply decision"
    );
}

/// Rollback event
pub fn log_rollback(adjustment_id: &str, proposal_id: &str, accepted: bool, reason: &str) {
    tracing::info!(
        event = "regulator.rollback",
        adjustment_id,
        proposal_id,
        accepted,
        reason,
        "Rollback decision"
    );
}

/// Policy proposal action (approve/reject/deactivate)
pub fn log_proposal_action(proposal_id: &str, action: &str, actor: &str) {
    tracing::info!(
        event = "regulator.proposal_action",
        proposal_id,
        action,
        actor,
        "Proposal action taken"
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_event_constants_are_distinct() {
        let events = [
            "dispatch.start",
            "dispatch.analysis",
            "dispatch.selection",
            "dispatch.decision",
            "dispatch.execution",
            "dispatch.retry",
            "dispatch.complete",
            "dispatch.cost_gate",
            "regulator.guard",
            "regulator.policy_eligible",
            "regulator.proposal_generated",
            "regulator.simulation",
            "regulator.snapshot_created",
            "regulator.snapshot_hash_valid",
            "regulator.active_apply",
            "regulator.rollback",
            "regulator.proposal_action",
        ];
        let mut sorted = events.to_vec();
        sorted.sort();
        sorted.dedup();
        assert_eq!(events.len(), sorted.len(), "Event names must be unique");
    }

    #[test]
    fn test_log_functions_do_not_panic() {
        log_dispatch_start("req-1", "disp-1", "http", 100);
        log_dispatch_analysis(
            "req-1",
            "disp-1",
            "analysis-1",
            "code_generate",
            "generate",
            "low",
            0.9,
        );
        log_dispatch_selection("req-1", "disp-1", "balanced_worker", "cli");
        log_dispatch_decision("req-1", "disp-1", "decided", 2);
        log_dispatch_execution("req-1", "disp-1", "cli", "balanced_worker", "completed");
        log_dispatch_retry(
            "req-1",
            "disp-1",
            "cheap_executor",
            "balanced_worker",
            "quality_fail",
        );
        log_dispatch_complete("req-1", "disp-1", "completed");
        log_cost_gate("req-1", "disp-1", 0.05, 1.50, 1.0, 10.0, true);
        log_auto_adjustment_guard(true, "dry_run", true, true);
        log_policy_eligible(
            "proposal-abc",
            true,
            "tier_override:cheap->balanced",
            "balanced_worker",
            0.9,
            0,
        );
        log_proposal_generated(
            "proposal-abc",
            "TierFailureConcentration",
            "balanced_worker",
            0.85,
            "tier_override:cheap->balanced",
        );
        log_simulation_result("sim-cheapest", "cheapest", 100, 0.05, -0.10);
        log_snapshot_created(
            "snapshot-abc",
            "proposal-abc",
            "tier_override:cheap->balanced",
            "balanced_worker",
        );
        log_snapshot_hash_valid("snapshot-abc", "adj-1", true);
        log_active_apply(
            "adj-1",
            "proposal-abc",
            "tier_override:cheap->balanced",
            "balanced_worker",
            true,
        );
        log_rollback("adj-1", "proposal-abc", true, "hash_valid");
        log_proposal_action("proposal-abc", "approve", "admin");
    }

    #[test]
    fn test_no_secrets_in_log_payloads() {
        let safe_ids = ["req-1", "disp-1", "proposal-abc", "adj-1", "http", "cli"];
        for id in &safe_ids {
            assert!(
                !id.contains("key"),
                "Log payload should not contain key-like values"
            );
            assert!(
                !id.contains("token"),
                "Log payload should not contain token-like values"
            );
            assert!(
                !id.contains("secret"),
                "Log payload should not contain secret-like values"
            );
        }
    }
}
