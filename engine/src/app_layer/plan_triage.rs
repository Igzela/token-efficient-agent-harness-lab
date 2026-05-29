use serde_json::json;

pub fn triage_plan(plan: &serde_json::Value) -> serde_json::Value {
    let plan_id = plan.get("plan_id").and_then(|v| v.as_str()).unwrap_or("unknown");
    let steps = plan.get("steps").and_then(|v| v.as_array()).map(|a| a.len()).unwrap_or(0);
    let gates = plan.get("approval_gates").and_then(|v| v.as_array()).map(|a| a.len()).unwrap_or(0);
    let blockers = plan.get("blockers").and_then(|v| v.as_array()).map(|a| a.len()).unwrap_or(0);

    let risk_score = if blockers > 2 { "high" } else if blockers > 0 { "medium" } else { "low" };
    let priority = if blockers > 2 { 1 } else if steps > 10 { 2 } else { 3 };

    json!({
        "plan_id": plan_id,
        "step_count": steps,
        "approval_gate_count": gates,
        "blocker_count": blockers,
        "risk_score": risk_score,
        "priority": priority,
        "classification": if blockers > 0 { "blocked" } else if gates > 0 { "needs_approval" } else { "ready" },
    })
}

pub fn build_portfolio_triage(plans: &[serde_json::Value]) -> serde_json::Value {
    let triages: Vec<serde_json::Value> = plans.iter().map(triage_plan).collect();
    let blocked = triages.iter().filter(|t| t["classification"] == "blocked").count();
    let ready = triages.iter().filter(|t| t["classification"] == "ready").count();
    json!({
        "total_plans": plans.len(),
        "blocked": blocked,
        "ready": ready,
        "triages": triages,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn triage_ready_plan() {
        let plan = json!({"plan_id": "p1", "steps": [1,2,3], "approval_gates": [], "blockers": []});
        let t = triage_plan(&plan);
        assert_eq!(t["classification"], "ready");
        assert_eq!(t["risk_score"], "low");
    }

    #[test]
    fn triage_blocked_plan() {
        let plan = json!({"plan_id": "p2", "steps": [], "approval_gates": [], "blockers": ["b1", "b2", "b3"]});
        let t = triage_plan(&plan);
        assert_eq!(t["classification"], "blocked");
        assert_eq!(t["risk_score"], "high");
    }

    #[test]
    fn triage_needs_approval() {
        let plan = json!({"plan_id": "p3", "steps": [1], "approval_gates": ["g1"], "blockers": []});
        let t = triage_plan(&plan);
        assert_eq!(t["classification"], "needs_approval");
    }

    #[test]
    fn portfolio_triage_counts() {
        let plans = vec![
            json!({"plan_id": "p1", "steps": [], "blockers": ["b1"], "approval_gates": []}),
            json!({"plan_id": "p2", "steps": [], "blockers": [], "approval_gates": []}),
        ];
        let pt = build_portfolio_triage(&plans);
        assert_eq!(pt["total_plans"], 2);
        assert_eq!(pt["blocked"], 1);
        assert_eq!(pt["ready"], 1);
    }

    #[test]
    fn triage_empty_plan() {
        let plan = json!({});
        let t = triage_plan(&plan);
        assert_eq!(t["classification"], "ready");
    }
}
