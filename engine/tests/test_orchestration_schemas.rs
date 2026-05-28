use engine::orchestration::schemas::*;

#[test]
fn test_workflow_node_to_dict() {
    let node = WorkflowNode {
        schema_version: WORKFLOW_NODE_SCHEMA_VERSION.to_string(),
        node_id: "node-0001".to_string(),
        workflow_id: "wf-0001".to_string(),
        task_type: "code_analyze".to_string(),
        assigned_agent_id: Some("role-analyzer".to_string()),
        status: "pending".to_string(),
        input_refs: vec![],
        output_ref: None,
        budget: 10.0,
        cost_incurred: 0.0,
        error: None,
        created_at: "2025-01-01T00:00:00Z".to_string(),
        started_at: None,
        completed_at: None,
    };
    let d = node.to_dict();
    assert_eq!(d["node_id"], "node-0001");
    assert_eq!(d["status"], "pending");
    assert_eq!(d["budget"], 10.0);
}

#[test]
fn test_workflow_edge_to_dict() {
    let edge = WorkflowEdge {
        schema_version: WORKFLOW_EDGE_SCHEMA_VERSION.to_string(),
        edge_id: "edge-0001".to_string(),
        from_node_id: "node-0001".to_string(),
        to_node_id: "node-0002".to_string(),
        edge_type: "dependency".to_string(),
    };
    let d = edge.to_dict();
    assert_eq!(d["from_node_id"], "node-0001");
    assert_eq!(d["edge_type"], "dependency");
}

#[test]
fn test_workflow_graph_to_dict() {
    let graph = WorkflowGraph {
        schema_version: WORKFLOW_SCHEMA_VERSION.to_string(),
        workflow_id: "wf-0001".to_string(),
        dispatch_id: "disp-0001".to_string(),
        nodes: vec![],
        edges: vec![],
        status: "created".to_string(),
        created_at: "2025-01-01T00:00:00Z".to_string(),
        updated_at: "2025-01-01T00:00:00Z".to_string(),
        started_at: None,
        completed_at: None,
        result: None,
    };
    let d = graph.to_dict();
    assert_eq!(d["workflow_id"], "wf-0001");
    assert_eq!(d["status"], "created");
}

#[test]
fn test_agent_role_to_dict() {
    let role = AgentRole {
        schema_version: AGENT_ROLE_SCHEMA_VERSION.to_string(),
        role_id: "role-001".to_string(),
        role_name: "analyzer".to_string(),
        capabilities: vec!["code_analyze".to_string(), "docs_analyze".to_string()],
        max_concurrent_nodes: 3,
        budget_limit: 100.0,
    };
    let d = role.to_dict();
    assert_eq!(d["role_name"], "analyzer");
    assert_eq!(d["max_concurrent_nodes"], 3);
}

#[test]
fn test_conflict_record_to_dict() {
    let conflict = ConflictRecord {
        schema_version: CONFLICT_RECORD_SCHEMA_VERSION.to_string(),
        conflict_id: "conflict-001".to_string(),
        workflow_id: "wf-0001".to_string(),
        conflict_type: "output_conflict".to_string(),
        involved_nodes: vec!["node-0001".to_string(), "node-0002".to_string()],
        resolution_strategy: Some("latest_wins".to_string()),
        resolution_result: Some("latest_output_wins".to_string()),
        resolved_at: Some("2025-01-01T00:00:00Z".to_string()),
    };
    let d = conflict.to_dict();
    assert_eq!(d["conflict_type"], "output_conflict");
    assert_eq!(d["resolution_strategy"], "latest_wins");
}

#[test]
fn test_orchestration_constants() {
    assert!(WORKFLOW_STATUSES.contains(&"created"));
    assert!(WORKFLOW_STATUSES.contains(&"completed"));
    assert!(NODE_STATUSES.contains(&"pending"));
    assert!(NODE_STATUSES.contains(&"running"));
    assert!(EDGE_TYPES.contains(&"dependency"));
    assert!(CONFLICT_TYPES.contains(&"output_conflict"));
    assert!(RESOLUTION_STRATEGIES.contains(&"latest_wins"));
}

#[test]
fn test_workflow_graph_serde_roundtrip() {
    let graph = WorkflowGraph {
        schema_version: WORKFLOW_SCHEMA_VERSION.to_string(),
        workflow_id: "wf-0001".to_string(),
        dispatch_id: "disp-0001".to_string(),
        nodes: vec![WorkflowNode {
            schema_version: WORKFLOW_NODE_SCHEMA_VERSION.to_string(),
            node_id: "node-0001".to_string(),
            workflow_id: "wf-0001".to_string(),
            task_type: "code_analyze".to_string(),
            assigned_agent_id: None,
            status: "pending".to_string(),
            input_refs: vec![],
            output_ref: None,
            budget: 10.0,
            cost_incurred: 0.0,
            error: None,
            created_at: "2025-01-01T00:00:00Z".to_string(),
            started_at: None,
            completed_at: None,
        }],
        edges: vec![],
        status: "created".to_string(),
        created_at: "2025-01-01T00:00:00Z".to_string(),
        updated_at: "2025-01-01T00:00:00Z".to_string(),
        started_at: None,
        completed_at: None,
        result: None,
    };
    let json = serde_json::to_string(&graph).unwrap();
    let deserialized: WorkflowGraph = serde_json::from_str(&json).unwrap();
    assert_eq!(deserialized.workflow_id, "wf-0001");
    assert_eq!(deserialized.nodes.len(), 1);
}
