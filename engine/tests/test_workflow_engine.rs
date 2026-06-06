use engine::orchestration::*;
use engine::runtime::FixtureRuntime;
use engine::task_analyzer::RuleBasedTaskAnalyzer;

fn analyze(request: &str) -> engine::task_analyzer::TaskAnalysis {
    let analyzer = RuleBasedTaskAnalyzer::new();
    analyzer.analyze(request, "test")
}

#[test]
fn test_task_decomposer_simple() {
    let mut decomposer = TaskDecomposer::new(None);
    let analysis = analyze("summarize this text");
    let mut runtime = FixtureRuntime::new();
    let graph = decomposer.decompose(&analysis, "disp-001", &mut runtime);
    assert_eq!(graph.nodes.len(), 1);
    assert!(graph.edges.is_empty());
    assert_eq!(graph.status, "decomposed");
}

#[test]
fn test_task_decomposer_medium() {
    let mut decomposer = TaskDecomposer::new(None);
    let analysis = analyze("review this code for bugs and security issues");
    let mut runtime = FixtureRuntime::new();
    let graph = decomposer.decompose(&analysis, "disp-001", &mut runtime);
    // complexity determines graph shape: <0.3 simple, >=0.6 complex, else medium
    assert!(!graph.nodes.is_empty());
    assert_eq!(graph.nodes.len() - 1, graph.edges.len());
}

#[test]
fn test_task_decomposer_complex() {
    let mut decomposer = TaskDecomposer::new(None);
    let analysis =
        analyze("refactor this architecture with complex dependencies and multiple risk factors");
    let mut runtime = FixtureRuntime::new();
    let graph = decomposer.decompose(&analysis, "disp-001", &mut runtime);
    assert!(!graph.nodes.is_empty());
    assert_eq!(graph.nodes.len() - 1, graph.edges.len());
}

#[test]
fn test_dependency_resolver_validates_decomposed_graph() {
    let mut decomposer = TaskDecomposer::new(None);
    let analysis = analyze("review this code");
    let mut runtime = FixtureRuntime::new();
    let graph = decomposer.decompose(&analysis, "disp-001", &mut runtime);
    let resolver = DependencyResolver::new();
    let (valid, errors) = resolver.validate(&graph);
    assert!(valid, "Expected valid graph, got errors: {errors:?}");
}

#[test]
fn test_conflict_resolver_detects_failed_nodes() {
    let resolver = ConflictResolver::new();
    let graph = WorkflowGraph {
        schema_version: WORKFLOW_SCHEMA_VERSION.to_string(),
        workflow_id: "wf-001".to_string(),
        dispatch_id: "disp-001".to_string(),
        nodes: vec![WorkflowNode {
            schema_version: WORKFLOW_NODE_SCHEMA_VERSION.to_string(),
            node_id: "node-1".to_string(),
            workflow_id: "wf-001".to_string(),
            task_type: "test".to_string(),
            assigned_agent_id: None,
            status: "failed".to_string(),
            input_refs: vec![],
            output_ref: None,
            budget: 0.0,
            cost_incurred: 0.0,
            error: Some("timeout".to_string()),
            created_at: String::new(),
            started_at: None,
            completed_at: None,
        }],
        edges: vec![],
        status: "running".to_string(),
        created_at: String::new(),
        updated_at: String::new(),
        started_at: None,
        completed_at: None,
        result: None,
    };
    let conflicts = resolver.detect_conflicts(&graph);
    assert!(!conflicts.is_empty());
    assert_eq!(conflicts[0].conflict_type, "dependency_violation");
}

#[test]
fn test_conflict_resolver_resolves() {
    let resolver = ConflictResolver::new();
    let conflict = engine::orchestration::schemas::ConflictRecord {
        schema_version: CONFLICT_RECORD_SCHEMA_VERSION.to_string(),
        conflict_id: "c-001".to_string(),
        workflow_id: "wf-001".to_string(),
        conflict_type: "output_conflict".to_string(),
        involved_nodes: vec!["n1".to_string(), "n2".to_string()],
        resolution_strategy: None,
        resolution_result: None,
        resolved_at: None,
    };
    let resolved = resolver.resolve(&conflict);
    assert_eq!(resolved.resolution_strategy.as_deref(), Some("latest_wins"));
    assert_eq!(
        resolved.resolution_result.as_deref(),
        Some("latest_output_wins")
    );
}

#[test]
fn test_result_aggregator() {
    let aggregator = ResultAggregator::new();
    let graph = WorkflowGraph {
        schema_version: WORKFLOW_SCHEMA_VERSION.to_string(),
        workflow_id: "wf-001".to_string(),
        dispatch_id: "disp-001".to_string(),
        nodes: vec![WorkflowNode {
            schema_version: WORKFLOW_NODE_SCHEMA_VERSION.to_string(),
            node_id: "n1".to_string(),
            workflow_id: "wf-001".to_string(),
            task_type: "test".to_string(),
            assigned_agent_id: None,
            status: "completed".to_string(),
            input_refs: vec![],
            output_ref: Some("out-1".to_string()),
            budget: 10.0,
            cost_incurred: 5.0,
            error: None,
            created_at: String::new(),
            started_at: None,
            completed_at: None,
        }],
        edges: vec![],
        status: "running".to_string(),
        created_at: String::new(),
        updated_at: String::new(),
        started_at: None,
        completed_at: None,
        result: None,
    };
    assert!(aggregator.is_complete(&graph));
    let result = aggregator.aggregate(&graph);
    assert_eq!(result["total_nodes"], 1);
    assert_eq!(result["completed_nodes"], 1);
    assert_eq!(result["failed_nodes"], 0);
}

#[test]
fn test_human_approval_gate() {
    let mut gate = HumanApprovalGate::new(0.7);
    let node = WorkflowNode {
        schema_version: WORKFLOW_NODE_SCHEMA_VERSION.to_string(),
        node_id: "n1".to_string(),
        workflow_id: "wf-001".to_string(),
        task_type: "test".to_string(),
        assigned_agent_id: None,
        status: "failed".to_string(),
        input_refs: vec![],
        output_ref: None,
        budget: 10.0,
        cost_incurred: 5.0,
        error: None,
        created_at: String::new(),
        started_at: None,
        completed_at: None,
    };
    let graph = WorkflowGraph {
        schema_version: WORKFLOW_SCHEMA_VERSION.to_string(),
        workflow_id: "wf-001".to_string(),
        dispatch_id: "disp-001".to_string(),
        nodes: vec![node.clone()],
        edges: vec![],
        status: "running".to_string(),
        created_at: String::new(),
        updated_at: String::new(),
        started_at: None,
        completed_at: None,
        result: None,
    };
    assert!(gate.requires_approval(&graph, &node));
    gate.approve("n1");
    assert!(!gate.requires_approval(&graph, &node));
    assert!(gate.is_approved("n1"));
}

#[test]
fn test_multi_agent_budget_manager() {
    let mut mgr = MultiAgentBudgetManager::new("cancel");
    mgr.create_workflow_budget("wf-001", 100.0);
    assert!(mgr.reserve_node_budget("wf-001", "n1", "agent-1", 30.0));
    assert!(mgr.reserve_node_budget("wf-001", "n2", "agent-1", 30.0));
    mgr.record_cost("wf-001", "n1", "agent-1", 60.0);
    mgr.record_cost("wf-001", "n2", "agent-1", 50.0);
    // Over budget after recording costs
    let (ok, _) = mgr.check_workflow_budget("wf-001");
    assert!(!ok);
    assert!((mgr.get_workflow_cost("wf-001") - 110.0).abs() < 0.001);
}

#[test]
fn test_agent_role_registry() {
    let mut registry = AgentRoleRegistry::new();
    let role = AgentRole {
        schema_version: AGENT_ROLE_SCHEMA_VERSION.to_string(),
        role_id: "role-1".to_string(),
        role_name: "analyzer".to_string(),
        capabilities: vec!["code_analyze".to_string()],
        max_concurrent_nodes: 2,
        budget_limit: 100.0,
    };
    registry.register_role(role);
    let assigned = registry.assign_agent("wf-001", "n1", "code_analyze");
    assert!(assigned.is_some());
    let assigned2 = registry.assign_agent("wf-001", "n2", "code_analyze");
    assert!(assigned2.is_some());
    // At capacity
    let assigned3 = registry.assign_agent("wf-001", "n3", "code_analyze");
    assert!(assigned3.is_none());
    registry.release_node("wf-001", "n1");
    let assigned4 = registry.assign_agent("wf-001", "n3", "code_analyze");
    assert!(assigned4.is_some());
}

#[test]
fn test_work_queue() {
    let queue = WorkQueue::new();
    let node = WorkflowNode {
        schema_version: WORKFLOW_NODE_SCHEMA_VERSION.to_string(),
        node_id: "n1".to_string(),
        workflow_id: "wf-001".to_string(),
        task_type: "test".to_string(),
        assigned_agent_id: None,
        status: "pending".to_string(),
        input_refs: vec![],
        output_ref: None,
        budget: 0.0,
        cost_incurred: 0.0,
        error: None,
        created_at: String::new(),
        started_at: None,
        completed_at: None,
    };
    let graph = WorkflowGraph {
        schema_version: WORKFLOW_SCHEMA_VERSION.to_string(),
        workflow_id: "wf-001".to_string(),
        dispatch_id: "disp-001".to_string(),
        nodes: vec![node],
        edges: vec![],
        status: "created".to_string(),
        created_at: String::new(),
        updated_at: String::new(),
        started_at: None,
        completed_at: None,
        result: None,
    };

    let graph = queue.start(&graph, "n1");
    assert_eq!(queue.status_of(&graph, "n1"), "running");

    let graph = queue.complete(&graph, "n1", "output-1");
    assert_eq!(queue.status_of(&graph, "n1"), "completed");
}

#[test]
fn test_workflow_engine_create_and_tick() {
    let decomposer = TaskDecomposer::new(None);
    let analysis = analyze("summarize this text");
    let mut runtime = FixtureRuntime::new();
    let mut engine = WorkflowEngine::new(decomposer);
    let graph = engine
        .create_workflow(&analysis, "disp-001", 100.0, "decided", &mut runtime)
        .unwrap();
    assert_eq!(graph.status, "decomposed");

    let graph = engine.tick(&graph);
    assert_eq!(graph.status, "running");
}

#[test]
fn test_workflow_engine_rejects_non_decided() {
    let decomposer = TaskDecomposer::new(None);
    let analysis = analyze("summarize this text");
    let mut runtime = FixtureRuntime::new();
    let mut engine = WorkflowEngine::new(decomposer);
    let result = engine.create_workflow(&analysis, "disp-001", 100.0, "rejected", &mut runtime);
    assert!(result.is_err());
}
