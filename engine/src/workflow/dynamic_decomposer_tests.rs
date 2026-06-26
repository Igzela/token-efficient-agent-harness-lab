use serde_json::Value;

use crate::workflow::dynamic_decomposer::*;

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[test]
fn test_initial_plan_simple_decomposition() {
    let decomposer = RuleBasedDecomposer {
        complexity_threshold: 0.1,
    };
    let context = DecompositionContext {
        run_id: "run-test".to_string(),
        existing_nodes: Vec::new(),
        existing_edges: Vec::new(),
        feedback_stats: None,
        max_nodes: 100,
    };

    let result = decomposer.decompose(DecompositionTrigger::InitialPlan, &context);

    assert_eq!(result.strategy, "simple");
    assert_eq!(result.proposals.len(), 1);
    assert_eq!(result.proposals[0].node_id, "execute-1");
    assert_eq!(result.proposals[0].node_type, "task");
    assert_eq!(result.proposals[0].task_type, "execute");
    assert!(result.proposals[0].depends_on.is_empty());
}

#[test]
fn test_initial_plan_complex_decomposition() {
    let decomposer = RuleBasedDecomposer {
        complexity_threshold: 0.9,
    };
    let context = DecompositionContext {
        run_id: "run-test".to_string(),
        existing_nodes: Vec::new(),
        existing_edges: Vec::new(),
        feedback_stats: None,
        max_nodes: 100,
    };

    let result = decomposer.decompose(DecompositionTrigger::InitialPlan, &context);

    assert_eq!(result.strategy, "complex");
    assert_eq!(result.proposals.len(), 5);
    let types: Vec<&str> = result
        .proposals
        .iter()
        .map(|p| p.task_type.as_str())
        .collect();
    assert_eq!(
        types,
        vec!["plan", "analyze", "execute", "review", "verify"]
    );
}

#[test]
fn test_test_failure_triggers_fix_proposals() {
    let decomposer = RuleBasedDecomposer::new();
    let context = DecompositionContext {
        run_id: "run-test".to_string(),
        existing_nodes: vec!["n1".to_string()],
        existing_edges: Vec::new(),
        feedback_stats: None,
        max_nodes: 100,
    };

    let result = decomposer.decompose(
        DecompositionTrigger::TestFailure {
            node_id: "n1".to_string(),
            error: "assertion failed at line 42".to_string(),
        },
        &context,
    );

    assert_eq!(result.strategy, "test_failure_recovery");
    assert_eq!(result.proposals.len(), 2);

    assert_eq!(result.proposals[0].node_id, "fix-n1");
    assert_eq!(result.proposals[0].task_type, "fix");
    assert_eq!(result.proposals[0].depends_on, vec!["n1"]);
    assert!(result.proposals[0].reason.contains("assertion failed"));

    assert_eq!(result.proposals[1].node_id, "test-fix-n1");
    assert_eq!(result.proposals[1].task_type, "test");
    assert_eq!(result.proposals[1].depends_on, vec!["fix-n1"]);
}

#[test]
fn test_quality_failure_triggers_review_proposals() {
    let decomposer = RuleBasedDecomposer::new();
    let context = DecompositionContext {
        run_id: "run-test".to_string(),
        existing_nodes: vec!["n1".to_string()],
        existing_edges: Vec::new(),
        feedback_stats: None,
        max_nodes: 100,
    };

    let result = decomposer.decompose(
        DecompositionTrigger::QualityFailure {
            node_id: "n1".to_string(),
            reason: "quality score below threshold".to_string(),
        },
        &context,
    );

    assert_eq!(result.strategy, "quality_review");
    assert_eq!(result.proposals.len(), 1);
    assert_eq!(result.proposals[0].node_id, "review-n1");
    assert_eq!(result.proposals[0].task_type, "review");
    assert_eq!(result.proposals[0].depends_on, vec!["n1"]);
    assert!(result.proposals[0].reason.contains("quality score"));
}

#[test]
fn test_observation_triggers_alternative_executor() {
    let decomposer = RuleBasedDecomposer::new();
    let context = DecompositionContext {
        run_id: "run-test".to_string(),
        existing_nodes: vec!["n1".to_string()],
        existing_edges: Vec::new(),
        feedback_stats: None,
        max_nodes: 100,
    };

    let result = decomposer.decompose(
        DecompositionTrigger::Observation("high failure rate for executor_type=cli".to_string()),
        &context,
    );

    assert_eq!(result.strategy, "alternative_executor");
    assert_eq!(result.proposals.len(), 1);
    assert_eq!(result.proposals[0].task_type, "execute");
    assert_eq!(result.proposals[0].node_type, "task");
    assert!(result.proposals[0]
        .reason
        .contains("alternative executor proposed"));
}

#[test]
fn test_user_goal_triggers_analyze_execute_verify() {
    let decomposer = RuleBasedDecomposer::new();
    let context = DecompositionContext {
        run_id: "run-test".to_string(),
        existing_nodes: Vec::new(),
        existing_edges: Vec::new(),
        feedback_stats: None,
        max_nodes: 100,
    };

    let result = decomposer.decompose(
        DecompositionTrigger::UserGoal("improve test coverage to 90%".to_string()),
        &context,
    );

    assert_eq!(result.strategy, "user_goal");
    assert_eq!(result.proposals.len(), 3);

    assert_eq!(result.proposals[0].task_type, "analyze");
    assert!(result.proposals[0].depends_on.is_empty());

    assert_eq!(result.proposals[1].task_type, "execute");
    assert_eq!(
        result.proposals[1].depends_on,
        vec![result.proposals[0].node_id.clone()]
    );

    assert_eq!(result.proposals[2].task_type, "verify");
    assert_eq!(
        result.proposals[2].depends_on,
        vec![result.proposals[1].node_id.clone()]
    );

    // Verify goal text appears in reasons
    assert!(result.proposals[0].reason.contains("improve test coverage"));
}

#[test]
fn test_max_nodes_limits_proposals() {
    let decomposer = RuleBasedDecomposer::new();

    // Test failure: already have 1 node, max_nodes=2, fix adds 1 (ok), test adds 1 (exceeds)
    let context = DecompositionContext {
        run_id: "run-test".to_string(),
        existing_nodes: vec!["n1".to_string()],
        existing_edges: Vec::new(),
        feedback_stats: None,
        max_nodes: 2,
    };

    let result = decomposer.decompose(
        DecompositionTrigger::TestFailure {
            node_id: "n1".to_string(),
            error: "failed".to_string(),
        },
        &context,
    );

    assert_eq!(result.proposals.len(), 1);
    assert_eq!(result.proposals[0].node_id, "fix-n1");

    // User goal: existing=2, max_nodes=3, needs 3 new -> skip
    let context2 = DecompositionContext {
        run_id: "run-test".to_string(),
        existing_nodes: vec!["n1".to_string(), "n2".to_string()],
        existing_edges: Vec::new(),
        feedback_stats: None,
        max_nodes: 3,
    };

    let result2 = decomposer.decompose(
        DecompositionTrigger::UserGoal("do something".to_string()),
        &context2,
    );

    assert_eq!(result2.strategy, "user_goal_skip");
    assert!(result2.proposals.is_empty());
}

#[test]
fn test_existing_nodes_not_duplicated() {
    let decomposer = RuleBasedDecomposer::new();
    let context = DecompositionContext {
        run_id: "run-test".to_string(),
        existing_nodes: vec![
            "n1".to_string(),
            "fix-n1".to_string(),
            "test-fix-n1".to_string(),
        ],
        existing_edges: Vec::new(),
        feedback_stats: None,
        max_nodes: 100,
    };

    let result = decomposer.decompose(
        DecompositionTrigger::TestFailure {
            node_id: "n1".to_string(),
            error: "failed again".to_string(),
        },
        &context,
    );

    assert_eq!(result.strategy, "test_failure_skip");
    assert!(result.proposals.is_empty());

    // Quality failure: review-n1 already exists
    let context2 = DecompositionContext {
        run_id: "run-test".to_string(),
        existing_nodes: vec!["n1".to_string(), "review-n1".to_string()],
        existing_edges: Vec::new(),
        feedback_stats: None,
        max_nodes: 100,
    };

    let result2 = decomposer.decompose(
        DecompositionTrigger::QualityFailure {
            node_id: "n1".to_string(),
            reason: "quality bad".to_string(),
        },
        &context2,
    );

    assert_eq!(result2.strategy, "quality_failure_skip");
    assert!(result2.proposals.is_empty());
}

#[test]
fn test_decomposer_integrates_with_controller() {
    let decomposer = RuleBasedDecomposer::new();

    // Simulate: controller detects test failure, calls decomposer, converts to mutations
    let proposals = decomposer.decompose(
        DecompositionTrigger::TestFailure {
            node_id: "n1".to_string(),
            error: "assertion error".to_string(),
        },
        &DecompositionContext {
            run_id: "run-integ".to_string(),
            existing_nodes: vec!["n1".to_string()],
            existing_edges: Vec::new(),
            feedback_stats: None,
            max_nodes: 100,
        },
    );

    let mutations = node_proposals_to_dag_mutations("run-integ", &proposals.proposals);

    // 2 node proposals + 2 edge proposals = 4 mutations
    assert_eq!(mutations.len(), 4);

    let node_mutations: Vec<_> = mutations
        .iter()
        .filter(|m| m.mutation_type == "add_node")
        .collect();
    let edge_mutations: Vec<_> = mutations
        .iter()
        .filter(|m| m.mutation_type == "add_edge")
        .collect();

    assert_eq!(node_mutations.len(), 2);
    assert_eq!(edge_mutations.len(), 2);

    // All mutations target the correct dag_id
    for m in &mutations {
        assert_eq!(m.dag_id, "run-integ");
    }

    // Edge from n1 -> fix-n1
    let edge1 = &edge_mutations[0];
    assert_eq!(
        edge1.payload.get("from_node").and_then(Value::as_str),
        Some("n1")
    );
    assert_eq!(
        edge1.payload.get("to_node").and_then(Value::as_str),
        Some("fix-n1")
    );

    // Edge from fix-n1 -> test-fix-n1
    let edge2 = &edge_mutations[1];
    assert_eq!(
        edge2.payload.get("from_node").and_then(Value::as_str),
        Some("fix-n1")
    );
    assert_eq!(
        edge2.payload.get("to_node").and_then(Value::as_str),
        Some("test-fix-n1")
    );
}

#[test]
fn test_empty_context_returns_empty_proposals() {
    let decomposer = RuleBasedDecomposer::new();

    // TestFailure with max_nodes=0
    let context = DecompositionContext {
        run_id: "run-test".to_string(),
        existing_nodes: Vec::new(),
        existing_edges: Vec::new(),
        feedback_stats: None,
        max_nodes: 0,
    };

    let result = decomposer.decompose(
        DecompositionTrigger::TestFailure {
            node_id: "n1".to_string(),
            error: "failed".to_string(),
        },
        &context,
    );

    assert!(result.proposals.is_empty());

    // QualityFailure with max_nodes=0
    let result2 = decomposer.decompose(
        DecompositionTrigger::QualityFailure {
            node_id: "n1".to_string(),
            reason: "bad quality".to_string(),
        },
        &context,
    );

    assert!(result2.proposals.is_empty());

    // UserGoal with max_nodes=0
    let result3 = decomposer.decompose(
        DecompositionTrigger::UserGoal("do something".to_string()),
        &context,
    );

    assert!(result3.proposals.is_empty());
}
