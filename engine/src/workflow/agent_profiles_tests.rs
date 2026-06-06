use serde_json::{json, Value};

use super::agent_profiles::*;
use super::dynamic_decomposer::{
    node_proposals_to_dag_mutations, Decomposer, DecompositionContext, DecompositionTrigger,
    NodeProposal, RuleBasedDecomposer,
};
use crate::node_executor::NoopNodeExecutor;
use crate::storage::local_product_store::LocalProductStore;

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn new_store() -> LocalProductStore {
    LocalProductStore::new(":memory:").expect("in-memory store")
}

fn setup_two_node_run() -> (LocalProductStore, String) {
    let store = new_store();
    let plan = store
        .create_workflow_plan("test-req", "test", "actor", |ids, _| {
            Ok(json!({
                "schema_version": "read_only_plan.v1",
                "plan_id": ids.plan_id,
                "status": "planned_read_only",
                "workflow_id": ids.workflow_id,
                "dispatch_id": ids.dispatch_id,
                "analysis": {"analysis_id": "a-1", "task_domain": "test"},
                "graph": {
                    "schema_version": "workflow_graph.v1",
                    "workflow_id": ids.workflow_id,
                    "dispatch_id": ids.dispatch_id,
                    "status": "decomposed",
                    "created_at": "2026-06-06T00:00:00Z",
                    "updated_at": "2026-06-06T00:00:00Z",
                    "nodes": [
                        {"node_id": "n1", "task_type": "analyze", "status": "pending"},
                        {"node_id": "n2", "task_type": "execute", "status": "pending"}
                    ],
                    "edges": [
                        {"edge_id": "e1", "from_node_id": "n1", "to_node_id": "n2", "edge_type": "dependency"}
                    ]
                },
                "boundaries": {
                    "execution_authority": "disabled",
                    "target_repository_writes": "disabled",
                    "runtime_workers": "disabled",
                },
            }))
        })
        .expect("create plan");

    let plan_id = plan.get("plan_id").and_then(Value::as_str).unwrap();
    let run = store
        .create_workflow_run_from_plan(plan_id, "test")
        .expect("create run");
    let run_id = run
        .get("run_id")
        .and_then(Value::as_str)
        .unwrap()
        .to_string();
    (store, run_id)
}

// ---------------------------------------------------------------------------
// Test 1: test_default_profiles_exist
// ---------------------------------------------------------------------------

#[test]
fn test_default_profiles_exist() {
    let registry = AgentProfileRegistry::new();
    assert_eq!(registry.list_all().len(), 5);

    let planner = registry.get(&AgentProfileId("planner".to_string()));
    assert!(planner.is_some());
    let planner = planner.unwrap();
    assert_eq!(planner.role, AgentProfileRole::Planner);
    assert_eq!(planner.tools, vec!["read", "analyze"]);
    assert_eq!(planner.context_budget_tokens, Some(20_000));
    assert_eq!(planner.workspace_scope, WorkspaceScope::Full);

    let implementer = registry.get(&AgentProfileId("implementer".to_string()));
    assert!(implementer.is_some());
    let implementer = implementer.unwrap();
    assert_eq!(implementer.role, AgentProfileRole::Implementer);
    assert!(implementer.tools.contains(&"write".to_string()));
    assert_eq!(implementer.context_budget_tokens, Some(40_000));
    assert_eq!(implementer.workspace_scope, WorkspaceScope::Task);

    let reviewer = registry.get(&AgentProfileId("reviewer".to_string()));
    assert!(reviewer.is_some());
    assert_eq!(reviewer.unwrap().role, AgentProfileRole::Reviewer);

    let tester = registry.get(&AgentProfileId("tester".to_string()));
    assert!(tester.is_some());
    let tester = tester.unwrap();
    assert_eq!(tester.role, AgentProfileRole::Tester);
    assert_eq!(tester.context_budget_tokens, Some(30_000));

    let researcher = registry.get(&AgentProfileId("researcher".to_string()));
    assert!(researcher.is_some());
    let researcher = researcher.unwrap();
    assert_eq!(researcher.role, AgentProfileRole::Researcher);
    assert_eq!(researcher.context_budget_tokens, Some(15_000));
}

// ---------------------------------------------------------------------------
// Test 2: test_upsert_and_get_profile
// ---------------------------------------------------------------------------

#[test]
fn test_upsert_and_get_profile() {
    let store = new_store();

    store
        .upsert_agent_profile(
            "custom-1",
            "implementer",
            &["read".to_string(), "bash".to_string()],
            Some("opus"),
            Some(50_000),
            "isolated",
            Some("cli"),
            5,
        )
        .expect("upsert");

    let profile = store
        .get_agent_profile("custom-1")
        .expect("get")
        .expect("profile exists");
    assert_eq!(profile.profile_id.as_str(), "custom-1");
    assert_eq!(profile.role, AgentProfileRole::Implementer);
    assert_eq!(profile.tools, vec!["read", "bash"]);
    assert_eq!(profile.model_hint.as_deref(), Some("opus"));
    assert_eq!(profile.context_budget_tokens, Some(50_000));
    assert_eq!(profile.workspace_scope, WorkspaceScope::Isolated);
    assert_eq!(profile.executor_preference.as_deref(), Some("cli"));
    assert_eq!(profile.max_retries, 5);

    // Upsert with updated fields
    store
        .upsert_agent_profile(
            "custom-1",
            "reviewer",
            &["read".to_string()],
            None,
            None,
            "full",
            None,
            3,
        )
        .expect("upsert update");

    let updated = store
        .get_agent_profile("custom-1")
        .expect("get")
        .expect("profile exists");
    assert_eq!(updated.role, AgentProfileRole::Reviewer);
    assert_eq!(updated.tools, vec!["read"]);
    assert!(updated.model_hint.is_none());
    assert!(updated.context_budget_tokens.is_none());
}

// ---------------------------------------------------------------------------
// Test 3: test_list_profiles
// ---------------------------------------------------------------------------

#[test]
fn test_list_profiles() {
    let store = new_store();

    assert!(store.list_agent_profiles().expect("list").is_empty());

    store
        .upsert_agent_profile(
            "p1",
            "planner",
            &["read".to_string()],
            None,
            None,
            "full",
            None,
            3,
        )
        .expect("upsert p1");
    store
        .upsert_agent_profile(
            "p2",
            "tester",
            &["bash".to_string()],
            None,
            None,
            "task",
            None,
            3,
        )
        .expect("upsert p2");

    let profiles = store.list_agent_profiles().expect("list");
    assert_eq!(profiles.len(), 2);
    let ids: Vec<&str> = profiles.iter().map(|p| p.profile_id.as_str()).collect();
    assert!(ids.contains(&"p1"));
    assert!(ids.contains(&"p2"));
}

// ---------------------------------------------------------------------------
// Test 4: test_delete_profile
// ---------------------------------------------------------------------------

#[test]
fn test_delete_profile() {
    let store = new_store();

    store
        .upsert_agent_profile(
            "del-me",
            "planner",
            &["read".to_string()],
            None,
            None,
            "full",
            None,
            3,
        )
        .expect("upsert");

    assert!(store.get_agent_profile("del-me").expect("get").is_some());
    assert!(store.delete_agent_profile("del-me").expect("delete"));
    assert!(store.get_agent_profile("del-me").expect("get").is_none());

    // Deleting non-existent returns false
    assert!(!store.delete_agent_profile("del-me").expect("delete again"));
}

// ---------------------------------------------------------------------------
// Test 5: test_get_profile_for_role
// ---------------------------------------------------------------------------

#[test]
fn test_get_profile_for_role() {
    let store = new_store();

    store
        .upsert_agent_profile(
            "impl-1",
            "implementer",
            &["read".to_string(), "write".to_string()],
            None,
            Some(40_000),
            "task",
            None,
            3,
        )
        .expect("upsert impl-1");
    store
        .upsert_agent_profile(
            "impl-2",
            "implementer",
            &["read".to_string()],
            None,
            None,
            "task",
            None,
            3,
        )
        .expect("upsert impl-2");
    store
        .upsert_agent_profile(
            "rev-1",
            "reviewer",
            &["read".to_string(), "comment".to_string()],
            None,
            None,
            "full",
            None,
            3,
        )
        .expect("upsert rev-1");

    // Returns first match for role
    let impl_profile = store
        .get_profile_for_role("implementer")
        .expect("get")
        .expect("exists");
    assert_eq!(impl_profile.role, AgentProfileRole::Implementer);

    let rev_profile = store
        .get_profile_for_role("reviewer")
        .expect("get")
        .expect("exists");
    assert_eq!(rev_profile.role, AgentProfileRole::Reviewer);

    // Non-existent role returns None
    assert!(store
        .get_profile_for_role("nonexistent")
        .expect("get")
        .is_none());
}

// ---------------------------------------------------------------------------
// Test 6: test_node_records_profile_id
// ---------------------------------------------------------------------------

#[test]
fn test_node_records_profile_id() {
    let (store, run_id) = setup_two_node_run();

    // Seed a profile
    store
        .upsert_agent_profile(
            "implementer",
            "implementer",
            &["read".to_string(), "write".to_string()],
            None,
            Some(40_000),
            "task",
            None,
            3,
        )
        .expect("upsert");

    // The nodes from the plan don't have profile_id yet (they were created before
    // the profile system was wired into plan creation). But we can verify that
    // when we insert a node with profile_id, it's persisted.
    store
        .insert_workflow_node(
            &run_id,
            &json!({
                "node_id": "n3",
                "task_type": "execute",
                "status": "pending",
                "profile_id": "implementer",
            }),
            "test",
            "test insert",
        )
        .expect("insert node");

    let run = store
        .get_workflow_run(&run_id)
        .expect("get run")
        .expect("run exists");
    let nodes = run
        .get("nodes")
        .and_then(Value::as_array)
        .expect("nodes array");

    let n3 = nodes
        .iter()
        .find(|n| n.get("node_id").and_then(Value::as_str) == Some("n3"))
        .expect("n3 found");
    assert_eq!(
        n3.get("profile_id").and_then(Value::as_str),
        Some("implementer")
    );
}

// ---------------------------------------------------------------------------
// Test 7: test_controller_attaches_fix_profile
// ---------------------------------------------------------------------------

#[test]
fn test_controller_attaches_fix_profile() {
    // Verify that when the decomposer generates fix/test proposals from a test failure,
    // the proposals get the correct profile_id (implementer for fix, tester for test).
    let decomposer = RuleBasedDecomposer::new();
    let context = DecompositionContext {
        run_id: "run-1".to_string(),
        existing_nodes: vec!["n1".to_string()],
        existing_edges: Vec::new(),
        feedback_stats: None,
        max_nodes: 100,
    };

    let result = decomposer.decompose(
        DecompositionTrigger::TestFailure {
            node_id: "n1".to_string(),
            error: "assertion failed".to_string(),
        },
        &context,
    );

    assert_eq!(result.proposals.len(), 2);

    // Fix proposal should get implementer profile
    let fix_proposal = &result.proposals[0];
    assert_eq!(fix_proposal.task_type, "fix");
    assert!(fix_proposal.profile_id.is_none()); // raw decomposer doesn't set it

    // Convert to mutations — the converter resolves profile_id
    let mutations = node_proposals_to_dag_mutations("run-1", &result.proposals);
    let fix_mutation = mutations
        .iter()
        .find(|m| {
            m.mutation_type == "add_node"
                && m.payload.get("task_type").and_then(Value::as_str) == Some("fix")
        })
        .expect("fix mutation found");
    assert_eq!(
        fix_mutation
            .payload
            .get("profile_id")
            .and_then(Value::as_str),
        Some("implementer")
    );

    let test_mutation = mutations
        .iter()
        .find(|m| {
            m.mutation_type == "add_node"
                && m.payload.get("task_type").and_then(Value::as_str) == Some("test")
        })
        .expect("test mutation found");
    assert_eq!(
        test_mutation
            .payload
            .get("profile_id")
            .and_then(Value::as_str),
        Some("tester")
    );
}

// ---------------------------------------------------------------------------
// Test 8: test_controller_attaches_review_profile
// ---------------------------------------------------------------------------

#[test]
fn test_controller_attaches_review_profile() {
    let decomposer = RuleBasedDecomposer::new();
    let context = DecompositionContext {
        run_id: "run-1".to_string(),
        existing_nodes: vec!["n1".to_string()],
        existing_edges: Vec::new(),
        feedback_stats: None,
        max_nodes: 100,
    };

    let result = decomposer.decompose(
        DecompositionTrigger::QualityFailure {
            node_id: "n1".to_string(),
            reason: "low quality score".to_string(),
        },
        &context,
    );

    assert_eq!(result.proposals.len(), 1);
    assert_eq!(result.proposals[0].task_type, "review");

    let mutations = node_proposals_to_dag_mutations("run-1", &result.proposals);
    let review_mutation = mutations
        .iter()
        .find(|m| m.mutation_type == "add_node")
        .expect("review mutation found");
    assert_eq!(
        review_mutation
            .payload
            .get("profile_id")
            .and_then(Value::as_str),
        Some("reviewer")
    );
}

// ---------------------------------------------------------------------------
// Test 9: test_decomposer_attaches_profile_to_proposals
// ---------------------------------------------------------------------------

#[test]
fn test_decomposer_attaches_profile_to_proposals() {
    // Verify all decomposition triggers produce proposals that resolve to correct profiles
    let decomposer = RuleBasedDecomposer::new();

    // Initial plan (complex) — use high threshold to trigger complex decomposition
    let complex_decomposer = RuleBasedDecomposer {
        complexity_threshold: 0.9,
    };
    let context = DecompositionContext {
        run_id: "run-1".to_string(),
        existing_nodes: Vec::new(),
        existing_edges: Vec::new(),
        feedback_stats: None,
        max_nodes: 100,
    };
    let result = complex_decomposer.decompose(DecompositionTrigger::InitialPlan, &context);
    let mutations = node_proposals_to_dag_mutations("run-1", &result.proposals);

    let node_mutations: Vec<_> = mutations
        .iter()
        .filter(|m| m.mutation_type == "add_node")
        .collect();

    // Each node should have a profile_id resolved from its task_type
    for m in &node_mutations {
        assert!(
            m.payload
                .get("profile_id")
                .and_then(Value::as_str)
                .is_some(),
            "node {:?} missing profile_id",
            m.payload.get("node_id")
        );
    }

    // plan-1 -> planner, analyze-1 -> researcher, execute-1 -> implementer,
    // review-1 -> reviewer, verify-1 -> tester
    let plan_m = node_mutations
        .iter()
        .find(|m| m.payload.get("node_id").and_then(Value::as_str) == Some("plan-1"))
        .unwrap();
    assert_eq!(
        plan_m.payload.get("profile_id").and_then(Value::as_str),
        Some("planner")
    );

    let analyze_m = node_mutations
        .iter()
        .find(|m| m.payload.get("node_id").and_then(Value::as_str) == Some("analyze-1"))
        .unwrap();
    assert_eq!(
        analyze_m.payload.get("profile_id").and_then(Value::as_str),
        Some("researcher")
    );

    let execute_m = node_mutations
        .iter()
        .find(|m| m.payload.get("node_id").and_then(Value::as_str) == Some("execute-1"))
        .unwrap();
    assert_eq!(
        execute_m.payload.get("profile_id").and_then(Value::as_str),
        Some("implementer")
    );

    let review_m = node_mutations
        .iter()
        .find(|m| m.payload.get("node_id").and_then(Value::as_str) == Some("review-1"))
        .unwrap();
    assert_eq!(
        review_m.payload.get("profile_id").and_then(Value::as_str),
        Some("reviewer")
    );

    let verify_m = node_mutations
        .iter()
        .find(|m| m.payload.get("node_id").and_then(Value::as_str) == Some("verify-1"))
        .unwrap();
    assert_eq!(
        verify_m.payload.get("profile_id").and_then(Value::as_str),
        Some("tester")
    );
}

// ---------------------------------------------------------------------------
// Test 10: test_export_import_round_trip
// ---------------------------------------------------------------------------

#[test]
fn test_export_import_round_trip() {
    let store = new_store();

    // Seed default profiles
    store.seed_default_agent_profiles().expect("seed");

    let profiles = store.list_agent_profiles().expect("list");
    assert_eq!(profiles.len(), 5);

    // Verify each default profile exists and has correct data
    for profile in &profiles {
        let from_db = store
            .get_agent_profile(profile.profile_id.as_str())
            .expect("get")
            .expect("exists");
        assert_eq!(from_db.role, profile.role);
        assert_eq!(from_db.tools, profile.tools);
        assert_eq!(from_db.workspace_scope, profile.workspace_scope);
    }

    // Re-seeding should be idempotent
    store.seed_default_agent_profiles().expect("re-seed");
    let profiles2 = store.list_agent_profiles().expect("list");
    assert_eq!(profiles2.len(), 5);

    // Export profiles to JSON and verify structure
    let exported: Vec<Value> = profiles
        .iter()
        .map(|p| {
            json!({
                "profile_id": p.profile_id.as_str(),
                "role": p.role.as_str(),
                "tools": p.tools,
                "model_hint": p.model_hint,
                "context_budget_tokens": p.context_budget_tokens,
                "workspace_scope": p.workspace_scope.as_str(),
                "executor_preference": p.executor_preference,
                "max_retries": p.max_retries,
            })
        })
        .collect();

    assert_eq!(exported.len(), 5);

    // Import into a fresh store
    let store2 = new_store();
    for entry in &exported {
        let pid = entry.get("profile_id").and_then(Value::as_str).unwrap();
        let role = entry.get("role").and_then(Value::as_str).unwrap();
        let tools: Vec<String> = entry
            .get("tools")
            .and_then(Value::as_array)
            .map(|a| {
                a.iter()
                    .filter_map(|v| v.as_str().map(String::from))
                    .collect()
            })
            .unwrap_or_default();
        let model_hint = entry.get("model_hint").and_then(Value::as_str);
        let budget = entry.get("context_budget_tokens").and_then(Value::as_u64);
        let scope = entry
            .get("workspace_scope")
            .and_then(Value::as_str)
            .unwrap_or("task");
        let exec_pref = entry.get("executor_preference").and_then(Value::as_str);
        let retries = entry
            .get("max_retries")
            .and_then(Value::as_u64)
            .unwrap_or(3) as u32;

        store2
            .upsert_agent_profile(
                pid, role, &tools, model_hint, budget, scope, exec_pref, retries,
            )
            .expect("import upsert");
    }

    let imported = store2.list_agent_profiles().expect("list");
    assert_eq!(imported.len(), 5);

    // Verify round-trip fidelity
    for original in &profiles {
        let imported_profile = store2
            .get_agent_profile(original.profile_id.as_str())
            .expect("get")
            .expect("exists");
        assert_eq!(imported_profile.role, original.role);
        assert_eq!(imported_profile.tools, original.tools);
        assert_eq!(
            imported_profile.context_budget_tokens,
            original.context_budget_tokens
        );
        assert_eq!(imported_profile.workspace_scope, original.workspace_scope);
        assert_eq!(imported_profile.max_retries, original.max_retries);
    }
}
