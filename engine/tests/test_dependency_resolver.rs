use engine::orchestration::dependency_resolver::DependencyResolver;
use engine::orchestration::schemas::*;

fn make_node(id: &str, status: &str) -> WorkflowNode {
    WorkflowNode {
        schema_version: WORKFLOW_NODE_SCHEMA_VERSION.to_string(),
        node_id: id.to_string(),
        workflow_id: "wf-001".to_string(),
        task_type: "test".to_string(),
        assigned_agent_id: None,
        status: status.to_string(),
        input_refs: vec![],
        output_ref: None,
        budget: 0.0,
        cost_incurred: 0.0,
        error: None,
        created_at: String::new(),
        started_at: None,
        completed_at: None,
    }
}

fn make_edge(from: &str, to: &str) -> WorkflowEdge {
    WorkflowEdge {
        schema_version: WORKFLOW_EDGE_SCHEMA_VERSION.to_string(),
        edge_id: format!("edge-{from}-{to}"),
        from_node_id: from.to_string(),
        to_node_id: to.to_string(),
        edge_type: "dependency".to_string(),
    }
}

#[test]
fn test_validate_empty_graph() {
    let resolver = DependencyResolver::new();
    let graph = WorkflowGraph {
        schema_version: WORKFLOW_SCHEMA_VERSION.to_string(),
        workflow_id: "wf-001".to_string(),
        dispatch_id: "disp-001".to_string(),
        nodes: vec![],
        edges: vec![],
        status: "created".to_string(),
        created_at: String::new(),
        updated_at: String::new(),
        started_at: None,
        completed_at: None,
        result: None,
    };
    let (valid, errors) = resolver.validate(&graph);
    assert!(valid);
    assert!(errors.is_empty());
}

#[test]
fn test_validate_valid_dag() {
    let resolver = DependencyResolver::new();
    let graph = WorkflowGraph {
        schema_version: WORKFLOW_SCHEMA_VERSION.to_string(),
        workflow_id: "wf-001".to_string(),
        dispatch_id: "disp-001".to_string(),
        nodes: vec![make_node("a", "pending"), make_node("b", "pending")],
        edges: vec![make_edge("a", "b")],
        status: "created".to_string(),
        created_at: String::new(),
        updated_at: String::new(),
        started_at: None,
        completed_at: None,
        result: None,
    };
    let (valid, errors) = resolver.validate(&graph);
    assert!(valid);
    assert!(errors.is_empty());
}

#[test]
fn test_validate_cycle_detected() {
    let resolver = DependencyResolver::new();
    let graph = WorkflowGraph {
        schema_version: WORKFLOW_SCHEMA_VERSION.to_string(),
        workflow_id: "wf-001".to_string(),
        dispatch_id: "disp-001".to_string(),
        nodes: vec![make_node("a", "pending"), make_node("b", "pending")],
        edges: vec![make_edge("a", "b"), make_edge("b", "a")],
        status: "created".to_string(),
        created_at: String::new(),
        updated_at: String::new(),
        started_at: None,
        completed_at: None,
        result: None,
    };
    let (valid, errors) = resolver.validate(&graph);
    assert!(!valid);
    assert!(errors.contains(&"cycle_detected".to_string()));
}

#[test]
fn test_validate_missing_source() {
    let resolver = DependencyResolver::new();
    let graph = WorkflowGraph {
        schema_version: WORKFLOW_SCHEMA_VERSION.to_string(),
        workflow_id: "wf-001".to_string(),
        dispatch_id: "disp-001".to_string(),
        nodes: vec![make_node("b", "pending")],
        edges: vec![make_edge("a", "b")],
        status: "created".to_string(),
        created_at: String::new(),
        updated_at: String::new(),
        started_at: None,
        completed_at: None,
        result: None,
    };
    let (valid, errors) = resolver.validate(&graph);
    assert!(!valid);
    assert!(errors.iter().any(|e| e.contains("missing_source")));
}

#[test]
fn test_execution_order_linear() {
    let resolver = DependencyResolver::new();
    let graph = WorkflowGraph {
        schema_version: WORKFLOW_SCHEMA_VERSION.to_string(),
        workflow_id: "wf-001".to_string(),
        dispatch_id: "disp-001".to_string(),
        nodes: vec![
            make_node("a", "pending"),
            make_node("b", "pending"),
            make_node("c", "pending"),
        ],
        edges: vec![make_edge("a", "b"), make_edge("b", "c")],
        status: "created".to_string(),
        created_at: String::new(),
        updated_at: String::new(),
        started_at: None,
        completed_at: None,
        result: None,
    };
    let waves = resolver.execution_order(&graph);
    assert_eq!(waves.len(), 3);
    assert_eq!(waves[0], vec!["a"]);
    assert_eq!(waves[1], vec!["b"]);
    assert_eq!(waves[2], vec!["c"]);
}

#[test]
fn test_execution_order_parallel() {
    let resolver = DependencyResolver::new();
    let graph = WorkflowGraph {
        schema_version: WORKFLOW_SCHEMA_VERSION.to_string(),
        workflow_id: "wf-001".to_string(),
        dispatch_id: "disp-001".to_string(),
        nodes: vec![
            make_node("a", "pending"),
            make_node("b", "pending"),
            make_node("c", "pending"),
        ],
        edges: vec![make_edge("a", "c"), make_edge("b", "c")],
        status: "created".to_string(),
        created_at: String::new(),
        updated_at: String::new(),
        started_at: None,
        completed_at: None,
        result: None,
    };
    let waves = resolver.execution_order(&graph);
    assert_eq!(waves.len(), 2);
    assert_eq!(waves[0], vec!["a", "b"]);
    assert_eq!(waves[1], vec!["c"]);
}

#[test]
fn test_execution_order_invalid_graph() {
    let resolver = DependencyResolver::new();
    let graph = WorkflowGraph {
        schema_version: WORKFLOW_SCHEMA_VERSION.to_string(),
        workflow_id: "wf-001".to_string(),
        dispatch_id: "disp-001".to_string(),
        nodes: vec![make_node("a", "pending"), make_node("b", "pending")],
        edges: vec![make_edge("a", "b"), make_edge("b", "a")],
        status: "created".to_string(),
        created_at: String::new(),
        updated_at: String::new(),
        started_at: None,
        completed_at: None,
        result: None,
    };
    let waves = resolver.execution_order(&graph);
    assert!(waves.is_empty());
}

#[test]
fn test_ready_nodes() {
    let resolver = DependencyResolver::new();
    let graph = WorkflowGraph {
        schema_version: WORKFLOW_SCHEMA_VERSION.to_string(),
        workflow_id: "wf-001".to_string(),
        dispatch_id: "disp-001".to_string(),
        nodes: vec![
            make_node("a", "completed"),
            make_node("b", "pending"),
            make_node("c", "pending"),
        ],
        edges: vec![make_edge("a", "b"), make_edge("a", "c")],
        status: "running".to_string(),
        created_at: String::new(),
        updated_at: String::new(),
        started_at: None,
        completed_at: None,
        result: None,
    };
    let ready = resolver.ready_nodes(&graph);
    assert_eq!(ready, vec!["b", "c"]);
}

#[test]
fn test_ready_nodes_with_unmet_deps() {
    let resolver = DependencyResolver::new();
    let graph = WorkflowGraph {
        schema_version: WORKFLOW_SCHEMA_VERSION.to_string(),
        workflow_id: "wf-001".to_string(),
        dispatch_id: "disp-001".to_string(),
        nodes: vec![make_node("a", "running"), make_node("b", "pending")],
        edges: vec![make_edge("a", "b")],
        status: "running".to_string(),
        created_at: String::new(),
        updated_at: String::new(),
        started_at: None,
        completed_at: None,
        result: None,
    };
    let ready = resolver.ready_nodes(&graph);
    assert!(ready.is_empty());
}
