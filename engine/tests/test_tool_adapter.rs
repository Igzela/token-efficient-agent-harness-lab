use engine::ecosystem::tool_adapter::*;
use std::collections::HashMap;

fn make_test_tool() -> ToolDefinition {
    ToolDefinition {
        schema_version: TOOL_ADAPTER_SCHEMA_VERSION.to_string(),
        tool_id: "t1".to_string(),
        name: "Calculator".to_string(),
        description: "A simple calculator".to_string(),
        input_schema: serde_json::json!({"type": "object"}),
        output_schema: serde_json::json!({"type": "object"}),
        timeout_seconds: 30,
        requires_network: false,
    }
}

#[test]
fn test_register_and_list() {
    let mut mgr = ToolAdapterManager::new();
    assert!(mgr.register_tool(&make_test_tool()));
    assert_eq!(mgr.list_tools().len(), 1);
}

#[test]
fn test_register_duplicate() {
    let mut mgr = ToolAdapterManager::new();
    assert!(mgr.register_tool(&make_test_tool()));
    assert!(!mgr.register_tool(&make_test_tool()));
}

#[test]
fn test_unregister() {
    let mut mgr = ToolAdapterManager::new();
    mgr.register_tool(&make_test_tool());
    assert!(mgr.unregister_tool("t1"));
    assert!(mgr.get_tool("t1").is_none());
    assert!(!mgr.unregister_tool("nonexistent"));
}

#[test]
fn test_validate_valid() {
    let mgr = ToolAdapterManager::new();
    let errors = mgr.validate_tool(&make_test_tool());
    assert!(errors.is_empty());
}

#[test]
fn test_validate_missing_id() {
    let mgr = ToolAdapterManager::new();
    let mut tool = make_test_tool();
    tool.tool_id = String::new();
    let errors = mgr.validate_tool(&tool);
    assert!(errors.iter().any(|e| e.contains("tool_id")));
}

#[test]
fn test_validate_negative_timeout() {
    let mgr = ToolAdapterManager::new();
    let mut tool = make_test_tool();
    tool.timeout_seconds = -1;
    let errors = mgr.validate_tool(&tool);
    assert!(errors.iter().any(|e| e.contains("timeout")));
}

#[test]
fn test_execute_existing_tool() {
    let mut mgr = ToolAdapterManager::new();
    mgr.register_tool(&make_test_tool());
    let request = ToolExecutionRequest {
        tool_id: "t1".to_string(),
        arguments: serde_json::json!({}),
        request_id: "req-1".to_string(),
    };
    let result = mgr.execute_tool(&request, 1000.0);
    assert!(result.success);
    assert!(result.error.is_none());
    assert_eq!(result.tool_id, "t1");
}

#[test]
fn test_execute_missing_tool() {
    let mgr = ToolAdapterManager::new();
    let request = ToolExecutionRequest {
        tool_id: "nonexistent".to_string(),
        arguments: serde_json::json!({}),
        request_id: "req-1".to_string(),
    };
    let result = mgr.execute_tool(&request, 1000.0);
    assert!(!result.success);
    assert!(result.error.is_some());
}

#[test]
fn test_to_dict() {
    let tool = make_test_tool();
    let d = tool.to_dict();
    assert_eq!(d["tool_id"], "t1");
    assert_eq!(d["name"], "Calculator");
}

#[test]
fn test_make_tool_defaults() {
    let tool = make_tool(HashMap::new());
    assert_eq!(tool.tool_id, "test-tool");
    assert_eq!(tool.timeout_seconds, 30);
}

#[test]
fn test_make_tool_overrides() {
    let mut overrides = HashMap::new();
    overrides.insert("tool_id".to_string(), serde_json::json!("custom"));
    overrides.insert("timeout_seconds".to_string(), serde_json::json!(60));
    let tool = make_tool(overrides);
    assert_eq!(tool.tool_id, "custom");
    assert_eq!(tool.timeout_seconds, 60);
}
