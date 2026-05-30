use serde_json::Value;

/// Check if a recommendation is shadow-only (diagnostic, not active).
pub fn is_shadow_only(recommendation: &Value) -> bool {
    recommendation
        .get("admission_scope")
        .and_then(|v| v.as_str())
        == Some("diagnostic")
        && recommendation
            .get("active_routing_allowed")
            .and_then(|v| v.as_bool())
            == Some(false)
}

/// Check if a shadow recommendation can be compared with a usage_ledger group.
pub fn can_compare_with_usage_ledger(
    recommendation: &Value,
    usage_ledger_group: &str,
) -> (bool, String) {
    let task = recommendation
        .get("task_family")
        .and_then(|v| v.as_str())
        .unwrap_or("");
    let variant = recommendation
        .get("variant_family")
        .and_then(|v| v.as_str())
        .unwrap_or("");
    let criterion = recommendation
        .get("success_criterion")
        .and_then(|v| v.as_str())
        .unwrap_or("");
    let rec_group = format!("{}/{}/{}", task, variant, criterion);

    let ledger_tail = if let Some(idx) = usage_ledger_group.find('/') {
        &usage_ledger_group[idx + 1..]
    } else {
        usage_ledger_group
    };

    if rec_group == ledger_tail {
        (
            true,
            "recommendation matches usage_ledger group tail".to_string(),
        )
    } else {
        (
            false,
            format!(
                "recommendation {:?} does not match usage_ledger group {:?}",
                rec_group, usage_ledger_group
            ),
        )
    }
}
