use serde::{Deserialize, Serialize};
use std::collections::HashMap;

pub const PLAN_REVIEW_ACTIONS: &[&str] = &[
    "review_blockers",
    "review_approval_gates",
    "review_token_budget",
    "review_steps",
    "review_remote_limit",
    "review_audit_failure",
    "ready_for_human_decision",
];

#[derive(Debug, Clone, PartialEq)]
pub enum PlanWorkbenchError {
    ExactlyTwoRequired,
    DuplicatePlanId,
    PlanNotFound(String),
}

impl std::fmt::Display for PlanWorkbenchError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::ExactlyTwoRequired => {
                write!(f, "exactly two plan_id query parameters are required")
            }
            Self::DuplicatePlanId => write!(f, "duplicate plan_id values cannot be compared"),
            Self::PlanNotFound(id) => write!(f, "plan not found: {id}"),
        }
    }
}

impl std::error::Error for PlanWorkbenchError {}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PlanFilters {
    pub repo_id: Option<String>,
    pub status: Option<String>,
    pub risk_level: Option<String>,
    pub task_type: Option<String>,
    pub limit: Option<usize>,
}

#[allow(clippy::derivable_impls)]
impl Default for PlanFilters {
    fn default() -> Self {
        Self {
            repo_id: None,
            status: None,
            risk_level: None,
            task_type: None,
            limit: None,
        }
    }
}

pub fn list_plan_summaries(
    plans: &[serde_json::Value],
    filters: Option<&PlanFilters>,
) -> Vec<serde_json::Value> {
    let plan_filters = filters.cloned().unwrap_or_default();
    let summaries: Vec<serde_json::Value> = plans
        .iter()
        .enumerate()
        .filter(|(_, plan)| plan.is_object())
        .map(|(index, plan)| plan_list_item(plan, index))
        .collect();
    let mut filtered: Vec<serde_json::Value> = summaries
        .into_iter()
        .filter(|item| matches_filters(item, &plan_filters))
        .collect();
    if let Some(limit) = plan_filters.limit {
        filtered.truncate(limit);
    }
    filtered
}

pub fn summarize_plans(plans: &[serde_json::Value], repo_id: Option<&str>) -> serde_json::Value {
    let summaries = list_plan_summaries(
        plans,
        Some(&PlanFilters {
            repo_id: repo_id.map(|s| s.to_string()),
            ..Default::default()
        }),
    );
    let mut by_status: HashMap<String, i64> = HashMap::new();
    by_status.insert("ready_for_review".to_string(), 0);
    by_status.insert("needs_approval".to_string(), 0);
    by_status.insert("blocked".to_string(), 0);

    let mut by_repo_kind: HashMap<String, i64> = HashMap::new();
    by_repo_kind.insert("local".to_string(), 0);
    by_repo_kind.insert("remote".to_string(), 0);

    let mut by_action: HashMap<String, i64> = HashMap::new();
    let mut total_budget: i64 = 0;
    let mut plans_with_blockers: i64 = 0;
    let mut plans_with_approval_gates: i64 = 0;

    for item in &summaries {
        let status = json_str(&item["status"]);
        *by_status.entry(status.to_string()).or_insert(0) += 1;
        let kind = json_str(&item["repo_kind"]);
        *by_repo_kind.entry(kind.to_string()).or_insert(0) += 1;
        total_budget += json_i64(&item["total_token_budget"]);
        if json_i64(&item["blocker_count"]) > 0 {
            plans_with_blockers += 1;
        }
        if json_i64(&item["approval_gate_count"]) > 0 {
            plans_with_approval_gates += 1;
        }
        let action = json_str(&item["next_review_action"]);
        *by_action.entry(action.to_string()).or_insert(0) += 1;
    }

    let total_plans = summaries.len() as i64;
    let average_budget = if total_plans > 0 {
        total_budget / total_plans
    } else {
        0
    };

    serde_json::json!({
        "total_plans": total_plans,
        "by_status": by_status,
        "by_repo_kind": by_repo_kind,
        "total_token_budget": total_budget,
        "average_token_budget": average_budget,
        "plans_with_blockers": plans_with_blockers,
        "plans_with_approval_gates": plans_with_approval_gates,
        "most_common_next_review_action": most_common_action(&by_action),
    })
}

pub fn compare_plans(
    plans: &[serde_json::Value],
    plan_ids: &[&str],
) -> Result<serde_json::Value, PlanWorkbenchError> {
    if plan_ids.len() != 2 {
        return Err(PlanWorkbenchError::ExactlyTwoRequired);
    }
    if plan_ids[0] == plan_ids[1] {
        return Err(PlanWorkbenchError::DuplicatePlanId);
    }
    let by_id = plans_by_id(plans);
    for pid in plan_ids {
        if !by_id.contains_key(*pid) {
            return Err(PlanWorkbenchError::PlanNotFound(pid.to_string()));
        }
    }
    let first = &by_id[plan_ids[0]];
    let second = &by_id[plan_ids[1]];
    let first_steps = steps(first);
    let second_steps = steps(second);
    let token_delta =
        json_i64(&second["total_token_budget"]) - json_i64(&first["total_token_budget"]);
    let context_delta = json_i64(&second["context_budget"]) - json_i64(&first["context_budget"]);
    let execution_delta =
        json_i64(&second["execution_budget"]) - json_i64(&first["execution_budget"]);
    let approval_delta = approval_gates(second).len() as i64 - approval_gates(first).len() as i64;
    let blocker_delta = blockers(second).len() as i64 - blockers(first).len() as i64;

    Ok(serde_json::json!({
        "plan_ids": plan_ids,
        "same_repo": repo_id(first) == repo_id(second),
        "status_delta": delta_label(json_str(&first["status"]), json_str(&second["status"])),
        "next_review_action_delta": delta_label(&recommend_next_review_action(first), &recommend_next_review_action(second)),
        "token_budget_delta": token_delta,
        "context_budget_delta": context_delta,
        "execution_budget_delta": execution_delta,
        "step_count_delta": second_steps.len() as i64 - first_steps.len() as i64,
        "approval_gate_delta": approval_delta,
        "blocker_delta": blocker_delta,
        "context_mode_changes": context_mode_changes(&first_steps, &second_steps),
        "efficiency_note": efficiency_note(token_delta, context_delta, execution_delta, approval_delta, blocker_delta),
    }))
}

pub fn recommend_next_review_action(plan: &serde_json::Value) -> String {
    let status = json_str(&plan["status"]);
    let bl = blockers(plan);
    let blocker_set: std::collections::HashSet<&str> = bl.iter().map(|s| s.as_str()).collect();

    if status == "blocked" && blocker_set.contains("remote_metadata_only") {
        return "review_remote_limit".to_string();
    }
    if status == "blocked"
        && (blocker_set.contains("audit_blocked") || audit_verdict(plan) == "BLOCKED")
    {
        return "review_audit_failure".to_string();
    }
    if status == "blocked" {
        return "review_blockers".to_string();
    }
    if status == "needs_approval" {
        return "review_approval_gates".to_string();
    }
    if status == "ready_for_review" && has_budget_pressure(plan) {
        return "review_token_budget".to_string();
    }
    if status == "ready_for_review" {
        return "review_steps".to_string();
    }
    "ready_for_human_decision".to_string()
}

fn plan_list_item(plan: &serde_json::Value, stored_index: usize) -> serde_json::Value {
    let task = plan.get("task").filter(|v| v.is_object());
    let repo = plan.get("repo_snapshot").filter(|v| v.is_object());
    let steps_list = steps(plan);
    let gates = approval_gates(plan);
    let bl = blockers(plan);
    let task_id = task
        .map(|t| json_str(&t["task_id"]).to_string())
        .unwrap_or_default();
    let task_type = task
        .map(|t| json_str(&t["task_type"]).to_string())
        .unwrap_or_default();
    let repo_kind = repo
        .map(|r| json_str(&r["kind"]).to_string())
        .unwrap_or_default();

    serde_json::json!({
        "stored_index": stored_index,
        "plan_id": json_str(&plan["plan_id"]),
        "repo_id": repo_id(plan),
        "repo_kind": repo_kind,
        "task_id": task_id,
        "task_type": task_type,
        "status": json_str(&plan["status"]),
        "effective_risk": json_str(&plan["effective_risk"]),
        "executable": plan.get("executable").and_then(|v| v.as_bool()).unwrap_or(false),
        "total_token_budget": json_i64(&plan["total_token_budget"]),
        "context_budget": json_i64(&plan["context_budget"]),
        "execution_budget": json_i64(&plan["execution_budget"]),
        "step_count": steps_list.len(),
        "approval_gate_count": gates.len(),
        "blocker_count": bl.len(),
        "next_review_action": recommend_next_review_action(plan),
    })
}

fn matches_filters(item: &serde_json::Value, filters: &PlanFilters) -> bool {
    if let Some(ref rid) = filters.repo_id {
        if json_str(&item["repo_id"]) != rid.as_str() {
            return false;
        }
    }
    if let Some(ref status) = filters.status {
        if json_str(&item["status"]) != status.as_str() {
            return false;
        }
    }
    if let Some(ref risk) = filters.risk_level {
        if json_str(&item["effective_risk"]) != risk.as_str() {
            return false;
        }
    }
    if let Some(ref tt) = filters.task_type {
        if json_str(&item["task_type"]) != tt.as_str() {
            return false;
        }
    }
    true
}

fn plans_by_id(plans: &[serde_json::Value]) -> HashMap<String, serde_json::Value> {
    let mut result = HashMap::new();
    for plan in plans {
        if !plan.is_object() {
            continue;
        }
        let pid = json_str(&plan["plan_id"]).to_string();
        if !pid.is_empty() && !result.contains_key(&pid) {
            result.insert(pid, plan.clone());
        }
    }
    result
}

fn context_mode_changes(
    first_steps: &[serde_json::Value],
    second_steps: &[serde_json::Value],
) -> Vec<serde_json::Value> {
    let max_len = first_steps.len().max(second_steps.len());
    let mut changes = Vec::new();
    for index in 0..max_len {
        let a = step_context_mode(first_steps, index);
        let b = step_context_mode(second_steps, index);
        if a != b {
            changes.push(serde_json::json!({"step_index": index, "a": a, "b": b}));
        }
    }
    changes
}

fn step_context_mode(steps: &[serde_json::Value], index: usize) -> Option<String> {
    steps
        .get(index)
        .and_then(|s| s.get("context_mode"))
        .map(|v| json_str(v).to_string())
}

fn efficiency_note(
    token_delta: i64,
    context_delta: i64,
    execution_delta: i64,
    approval_delta: i64,
    blocker_delta: i64,
) -> String {
    let direction = if token_delta < 0 {
        "Plan b uses a lower total token budget."
    } else if token_delta > 0 {
        "Plan b uses a higher total token budget."
    } else {
        "Both plans use the same total token budget."
    };
    format!(
        "{} Context delta {}; execution delta {}; approval gate delta {}; blocker delta {}.",
        direction, context_delta, execution_delta, approval_delta, blocker_delta
    )
}

fn delta_label(first: &str, second: &str) -> String {
    if first == second {
        "same".to_string()
    } else {
        format!("{}->{}", first, second)
    }
}

fn most_common_action(by_action: &HashMap<String, i64>) -> Option<String> {
    if by_action.is_empty() {
        return None;
    }
    by_action
        .iter()
        .max_by(|a, b| a.1.cmp(b.1).reverse().then_with(|| a.0.cmp(b.0)))
        .map(|(k, _)| k.clone())
}

fn repo_id(plan: &serde_json::Value) -> String {
    let from_task = plan.get("task").filter(|v| v.is_object()).and_then(|t| {
        let s = json_str(&t["repo_id"]);
        if s.is_empty() {
            None
        } else {
            Some(s.to_string())
        }
    });
    if let Some(id) = from_task {
        return id;
    }
    plan.get("repo_snapshot")
        .filter(|v| v.is_object())
        .and_then(|r| {
            let s = json_str(&r["id"]);
            if s.is_empty() {
                None
            } else {
                Some(s.to_string())
            }
        })
        .unwrap_or_default()
}

fn audit_verdict(plan: &serde_json::Value) -> String {
    plan.get("audit_summary")
        .filter(|v| v.is_object())
        .map(|a| json_str(&a["verdict"]).to_string())
        .unwrap_or_default()
}

fn steps(plan: &serde_json::Value) -> Vec<serde_json::Value> {
    plan.get("steps")
        .and_then(|v| v.as_array())
        .map(|arr| arr.iter().filter(|s| s.is_object()).cloned().collect())
        .unwrap_or_default()
}

fn approval_gates(plan: &serde_json::Value) -> Vec<String> {
    string_list(plan.get("approval_gates"))
}
fn blockers(plan: &serde_json::Value) -> Vec<String> {
    string_list(plan.get("blockers"))
}

fn has_budget_pressure(plan: &serde_json::Value) -> bool {
    let notes = string_list(plan.get("token_efficiency_notes"));
    if notes
        .iter()
        .any(|n| n.to_lowercase().contains("budget pressure"))
    {
        return true;
    }
    json_i64(&plan["total_token_budget"]) >= 6000 || json_i64(&plan["context_budget"]) >= 5000
}

fn string_list(value: Option<&serde_json::Value>) -> Vec<String> {
    value
        .and_then(|v| v.as_array())
        .map(|arr| {
            arr.iter()
                .filter_map(|v| v.as_str().map(|s| s.to_string()))
                .collect()
        })
        .unwrap_or_default()
}

fn json_str(value: &serde_json::Value) -> &str {
    value.as_str().unwrap_or("")
}
fn json_i64(value: &serde_json::Value) -> i64 {
    value.as_i64().unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_plan(
        pid: &str,
        status: &str,
        rid: &str,
        tt: &str,
        tb: i64,
        cb: i64,
        eb: i64,
        exec: bool,
        blockers: Vec<&str>,
        gates: Vec<&str>,
        steps: Vec<serde_json::Value>,
    ) -> serde_json::Value {
        serde_json::json!({
            "plan_id": pid, "status": status, "executable": exec,
            "total_token_budget": tb, "context_budget": cb, "execution_budget": eb,
            "effective_risk": "medium",
            "task": {"task_id": format!("{}-task", pid), "task_type": tt, "repo_id": rid},
            "repo_snapshot": {"id": rid, "kind": "local"},
            "blockers": blockers, "approval_gates": gates, "steps": steps,
        })
    }

    #[test]
    fn test_recommend_blocked_remote() {
        let p = make_plan(
            "p1",
            "blocked",
            "r1",
            "bugfix",
            1000,
            500,
            500,
            false,
            vec!["remote_metadata_only"],
            vec![],
            vec![],
        );
        assert_eq!(recommend_next_review_action(&p), "review_remote_limit");
    }

    #[test]
    fn test_recommend_blocked_audit() {
        let p = make_plan(
            "p1",
            "blocked",
            "r1",
            "bugfix",
            1000,
            500,
            500,
            false,
            vec!["audit_blocked"],
            vec![],
            vec![],
        );
        assert_eq!(recommend_next_review_action(&p), "review_audit_failure");
    }

    #[test]
    fn test_recommend_blocked_generic() {
        let p = make_plan(
            "p1",
            "blocked",
            "r1",
            "bugfix",
            1000,
            500,
            500,
            false,
            vec!["some_blocker"],
            vec![],
            vec![],
        );
        assert_eq!(recommend_next_review_action(&p), "review_blockers");
    }

    #[test]
    fn test_recommend_needs_approval() {
        let p = make_plan(
            "p1",
            "needs_approval",
            "r1",
            "bugfix",
            1000,
            500,
            500,
            false,
            vec![],
            vec!["g1"],
            vec![],
        );
        assert_eq!(recommend_next_review_action(&p), "review_approval_gates");
    }

    #[test]
    fn test_recommend_budget_pressure() {
        let p = make_plan(
            "p1",
            "ready_for_review",
            "r1",
            "bugfix",
            7000,
            6000,
            1000,
            true,
            vec![],
            vec![],
            vec![],
        );
        assert_eq!(recommend_next_review_action(&p), "review_token_budget");
    }

    #[test]
    fn test_recommend_ready_for_review() {
        let p = make_plan(
            "p1",
            "ready_for_review",
            "r1",
            "bugfix",
            1000,
            500,
            500,
            true,
            vec![],
            vec![],
            vec![],
        );
        assert_eq!(recommend_next_review_action(&p), "review_steps");
    }

    #[test]
    fn test_recommend_ready_for_human() {
        let p = make_plan(
            "p1",
            "done",
            "r1",
            "bugfix",
            1000,
            500,
            500,
            true,
            vec![],
            vec![],
            vec![],
        );
        assert_eq!(recommend_next_review_action(&p), "ready_for_human_decision");
    }

    #[test]
    fn test_list_plan_summaries_no_filter() {
        let plans = vec![
            make_plan(
                "p1",
                "ready_for_review",
                "r1",
                "bugfix",
                1000,
                500,
                500,
                true,
                vec![],
                vec![],
                vec![],
            ),
            make_plan(
                "p2",
                "blocked",
                "r2",
                "feature",
                2000,
                1000,
                1000,
                false,
                vec!["b1"],
                vec![],
                vec![],
            ),
        ];
        assert_eq!(list_plan_summaries(&plans, None).len(), 2);
    }

    #[test]
    fn test_list_plan_summaries_filter() {
        let plans = vec![
            make_plan(
                "p1",
                "ready_for_review",
                "r1",
                "bugfix",
                1000,
                500,
                500,
                true,
                vec![],
                vec![],
                vec![],
            ),
            make_plan(
                "p2",
                "blocked",
                "r2",
                "feature",
                2000,
                1000,
                1000,
                false,
                vec!["b1"],
                vec![],
                vec![],
            ),
        ];
        let result = list_plan_summaries(
            &plans,
            Some(&PlanFilters {
                status: Some("blocked".to_string()),
                ..Default::default()
            }),
        );
        assert_eq!(result.len(), 1);
        assert_eq!(json_str(&result[0]["plan_id"]), "p2");
    }

    #[test]
    fn test_list_plan_summaries_limit() {
        let plans = vec![
            make_plan(
                "p1",
                "ready_for_review",
                "r1",
                "bugfix",
                1000,
                500,
                500,
                true,
                vec![],
                vec![],
                vec![],
            ),
            make_plan(
                "p2",
                "ready_for_review",
                "r2",
                "feature",
                2000,
                1000,
                1000,
                true,
                vec![],
                vec![],
                vec![],
            ),
            make_plan(
                "p3",
                "ready_for_review",
                "r3",
                "refactor",
                3000,
                1500,
                1500,
                true,
                vec![],
                vec![],
                vec![],
            ),
        ];
        let result = list_plan_summaries(
            &plans,
            Some(&PlanFilters {
                limit: Some(2),
                ..Default::default()
            }),
        );
        assert_eq!(result.len(), 2);
    }

    #[test]
    fn test_summarize_plans() {
        let plans = vec![
            make_plan(
                "p1",
                "ready_for_review",
                "r1",
                "bugfix",
                1000,
                500,
                500,
                true,
                vec![],
                vec![],
                vec![],
            ),
            make_plan(
                "p2",
                "blocked",
                "r2",
                "feature",
                2000,
                1000,
                1000,
                false,
                vec!["b1"],
                vec![],
                vec![],
            ),
        ];
        let s = summarize_plans(&plans, None);
        assert_eq!(json_i64(&s["total_plans"]), 2);
        assert_eq!(json_i64(&s["total_token_budget"]), 3000);
        assert_eq!(json_i64(&s["plans_with_blockers"]), 1);
    }

    #[test]
    fn test_compare_plans_ok() {
        let plans = vec![
            make_plan(
                "p1",
                "ready_for_review",
                "r1",
                "bugfix",
                1000,
                500,
                500,
                true,
                vec![],
                vec![],
                vec![serde_json::json!({"context_mode": "compact"})],
            ),
            make_plan(
                "p2",
                "blocked",
                "r1",
                "feature",
                2000,
                1000,
                1000,
                false,
                vec!["b1"],
                vec!["g1"],
                vec![],
            ),
        ];
        let r = compare_plans(&plans, &["p1", "p2"]).unwrap();
        assert_eq!(json_i64(&r["token_budget_delta"]), 1000);
        assert_eq!(json_i64(&r["blocker_delta"]), 1);
        assert_eq!(json_i64(&r["approval_gate_delta"]), 1);
        assert_eq!(json_i64(&r["step_count_delta"]), -1);
    }

    #[test]
    fn test_compare_plans_wrong_count() {
        let plans: Vec<serde_json::Value> = vec![];
        assert!(matches!(
            compare_plans(&plans, &["p1"]),
            Err(PlanWorkbenchError::ExactlyTwoRequired)
        ));
    }

    #[test]
    fn test_compare_plans_duplicate() {
        let plans: Vec<serde_json::Value> = vec![];
        assert!(matches!(
            compare_plans(&plans, &["p1", "p1"]),
            Err(PlanWorkbenchError::DuplicatePlanId)
        ));
    }

    #[test]
    fn test_compare_plans_not_found() {
        let plans = vec![make_plan(
            "p1",
            "ready_for_review",
            "r1",
            "bugfix",
            1000,
            500,
            500,
            true,
            vec![],
            vec![],
            vec![],
        )];
        assert!(matches!(
            compare_plans(&plans, &["p1", "missing"]),
            Err(PlanWorkbenchError::PlanNotFound(_))
        ));
    }

    #[test]
    fn test_delta_label() {
        assert_eq!(delta_label("a", "a"), "same");
        assert_eq!(delta_label("a", "b"), "a->b");
    }

    #[test]
    fn test_efficiency_note() {
        assert!(efficiency_note(-500, -100, -50, 0, 0).contains("lower total token budget"));
        assert!(efficiency_note(500, 100, 50, 1, 1).contains("higher total token budget"));
        assert!(efficiency_note(0, 0, 0, 0, 0).contains("same total token budget"));
    }

    #[test]
    fn test_has_budget_pressure() {
        let p1 = serde_json::json!({"total_token_budget": 100, "context_budget": 100, "token_efficiency_notes": ["budget pressure detected"]});
        assert!(has_budget_pressure(&p1));
        let p2 = serde_json::json!({"total_token_budget": 6000, "context_budget": 100});
        assert!(has_budget_pressure(&p2));
    }

    #[test]
    fn test_repo_id_priority() {
        let p1 = serde_json::json!({"task": {"repo_id": "task_repo"}, "repo_snapshot": {"id": "snap_repo"}});
        assert_eq!(repo_id(&p1), "task_repo");
        let p2 = serde_json::json!({"repo_snapshot": {"id": "snap_repo"}});
        assert_eq!(repo_id(&p2), "snap_repo");
    }
}
