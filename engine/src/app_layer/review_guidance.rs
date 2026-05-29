use serde_json::json;

pub fn build_review_guidance(plan: &serde_json::Value) -> serde_json::Value {
    let plan_id = plan.get("plan_id").and_then(|v| v.as_str()).unwrap_or("unknown");
    let steps = plan.get("steps").and_then(|v| v.as_array());
    let gates = plan.get("approval_gates").and_then(|v| v.as_array());
    let blockers = plan.get("blockers").and_then(|v| v.as_array());

    let has_budget_pressure = plan.get("budget_pressure").and_then(|v| v.as_bool()).unwrap_or(false);
    let has_boundary_gate = gates.map(|g| !g.is_empty()).unwrap_or(false);

    let mut options = Vec::new();
    if let Some(bl) = blockers {
        if !bl.is_empty() {
            options.push(json!({"action": "resolve_blockers", "count": bl.len()}));
        }
    }
    if has_boundary_gate {
        options.push(json!({"action": "review_approval_gates"}));
    }
    if has_budget_pressure {
        options.push(json!({"action": "review_budget"}));
    }
    if options.is_empty() {
        options.push(json!({"action": "proceed"}));
    }

    json!({
        "plan_id": plan_id,
        "step_count": steps.map(|s| s.len()).unwrap_or(0),
        "options": options,
        "has_budget_pressure": has_budget_pressure,
        "has_boundary_gate": has_boundary_gate,
    })
}

pub fn derive_review_options(plan: &serde_json::Value) -> Vec<String> {
    let mut opts = Vec::new();
    if let Some(blockers) = plan.get("blockers").and_then(|v| v.as_array()) {
        if !blockers.is_empty() {
            opts.push("resolve_blockers".to_string());
        }
    }
    if let Some(gates) = plan.get("approval_gates").and_then(|v| v.as_array()) {
        if !gates.is_empty() {
            opts.push("review_approval_gates".to_string());
        }
    }
    if plan.get("budget_pressure").and_then(|v| v.as_bool()).unwrap_or(false) {
        opts.push("review_budget".to_string());
    }
    if opts.is_empty() {
        opts.push("proceed".to_string());
    }
    opts
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn guidance_no_issues() {
        let plan = json!({"plan_id": "p1", "steps": [1], "approval_gates": [], "blockers": []});
        let g = build_review_guidance(&plan);
        assert_eq!(g["options"].as_array().unwrap().len(), 1);
        assert_eq!(g["options"][0]["action"], "proceed");
    }

    #[test]
    fn guidance_with_blockers() {
        let plan = json!({"plan_id": "p2", "steps": [], "blockers": ["b1"], "approval_gates": []});
        let g = build_review_guidance(&plan);
        assert!(g["options"].as_array().unwrap().iter().any(|o| o["action"] == "resolve_blockers"));
    }

    #[test]
    fn guidance_with_budget_pressure() {
        let plan = json!({"plan_id": "p3", "steps": [], "blockers": [], "approval_gates": [], "budget_pressure": true});
        let g = build_review_guidance(&plan);
        assert!(g["has_budget_pressure"].as_bool().unwrap());
    }

    #[test]
    fn derive_options_empty() {
        let plan = json!({"blockers": [], "approval_gates": []});
        assert_eq!(derive_review_options(&plan), vec!["proceed"]);
    }

    #[test]
    fn derive_options_with_gates() {
        let plan = json!({"blockers": [], "approval_gates": ["g1"]});
        assert!(derive_review_options(&plan).contains(&"review_approval_gates".to_string()));
    }
}
