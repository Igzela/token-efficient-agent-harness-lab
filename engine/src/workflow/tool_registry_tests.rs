use serde_json::json;

use super::tool_registry::*;
use crate::storage::local_product_store::LocalProductStore;

fn new_store() -> LocalProductStore {
    LocalProductStore::new(":memory:").expect("in-memory store")
}

// ---------------------------------------------------------------------------
// Test 1: test_register_and_get_capability
// ---------------------------------------------------------------------------

#[test]
fn test_register_and_get_capability() {
    let store = new_store();

    store
        .register_tool_capability(
            "bash",
            "Execute shell commands",
            Some(&json!({"type": "object", "properties": {"command": {"type": "string"}}})),
            Some(&json!({"type": "object", "properties": {"output": {"type": "string"}}})),
            false,
            "medium",
        )
        .expect("register");

    let cap = store
        .get_tool_capability("bash")
        .expect("get")
        .expect("exists");
    assert_eq!(cap.tool_name, "bash");
    assert_eq!(cap.description, "Execute shell commands");
    assert!(cap.input_schema.is_some());
    assert!(cap.output_schema.is_some());
    assert!(!cap.requires_approval);
    assert_eq!(cap.risk_level, RiskLevel::Medium);

    // Non-existent returns None
    assert!(store
        .get_tool_capability("nonexistent")
        .expect("get")
        .is_none());

    // Upsert updates existing
    store
        .register_tool_capability(
            "bash",
            "Execute shell commands (updated)",
            None,
            None,
            true,
            "high",
        )
        .expect("update");

    let updated = store
        .get_tool_capability("bash")
        .expect("get")
        .expect("exists");
    assert_eq!(updated.description, "Execute shell commands (updated)");
    assert!(updated.requires_approval);
    assert_eq!(updated.risk_level, RiskLevel::High);
}

// ---------------------------------------------------------------------------
// Test 2: test_list_capabilities
// ---------------------------------------------------------------------------

#[test]
fn test_list_capabilities() {
    let store = new_store();

    assert!(store.list_tool_capabilities().expect("list").is_empty());

    store
        .register_tool_capability("read", "Read files", None, None, false, "low")
        .expect("register read");
    store
        .register_tool_capability("write", "Write files", None, None, true, "high")
        .expect("register write");
    store
        .register_tool_capability("bash", "Run commands", None, None, true, "high")
        .expect("register bash");

    let caps = store.list_tool_capabilities().expect("list");
    assert_eq!(caps.len(), 3);
    let names: Vec<&str> = caps.iter().map(|c| c.tool_name.as_str()).collect();
    assert!(names.contains(&"read"));
    assert!(names.contains(&"write"));
    assert!(names.contains(&"bash"));
}

// ---------------------------------------------------------------------------
// Test 3: test_allowlist_blocks_unknown_tool
// ---------------------------------------------------------------------------

#[test]
fn test_allowlist_blocks_unknown_tool() {
    let store = new_store();

    store
        .set_tool_allowlist("implementer", &["read".to_string(), "write".to_string()])
        .expect("set allowlist");

    assert!(store
        .check_tool_allowed("implementer", "read")
        .expect("check"));
    assert!(store
        .check_tool_allowed("implementer", "write")
        .expect("check"));
    assert!(!store
        .check_tool_allowed("implementer", "bash")
        .expect("check"));
    assert!(!store
        .check_tool_allowed("implementer", "delete")
        .expect("check"));
}

// ---------------------------------------------------------------------------
// Test 4: test_allowlist_permits_listed_tool
// ---------------------------------------------------------------------------

#[test]
fn test_allowlist_permits_listed_tool() {
    let store = new_store();

    store
        .set_tool_allowlist("reviewer", &["read".to_string()])
        .expect("set allowlist");

    assert!(store.check_tool_allowed("reviewer", "read").expect("check"));
    assert!(!store
        .check_tool_allowed("reviewer", "write")
        .expect("check"));
}

// ---------------------------------------------------------------------------
// Test 5: test_no_allowlist_permits_all
// ---------------------------------------------------------------------------

#[test]
fn test_no_allowlist_permits_all() {
    let store = new_store();

    // No allowlist set for "planner" -> everything allowed
    assert!(store.check_tool_allowed("planner", "read").expect("check"));
    assert!(store.check_tool_allowed("planner", "write").expect("check"));
    assert!(store.check_tool_allowed("planner", "bash").expect("check"));
    assert!(store
        .check_tool_allowed("planner", "anything")
        .expect("check"));
}

#[test]
fn test_explicit_empty_allowlist_blocks_every_tool() {
    let store = new_store();

    store
        .set_tool_allowlist("locked-down", &[])
        .expect("set explicit empty allowlist");

    assert!(!store
        .check_tool_allowed("locked-down", "read")
        .expect("check"));
    assert!(!store
        .check_tool_allowed("locked-down", "bash")
        .expect("check"));
}

// ---------------------------------------------------------------------------
// Test 6: test_hook_blocks_execution
// ---------------------------------------------------------------------------

#[test]
fn test_hook_blocks_execution() {
    let store = new_store();

    store
        .add_tool_hook(
            "block-bash",
            "pre_execution",
            Some("bash"),
            None,
            "block",
            Some(&json!({"reason": "bash not allowed in review phase"})),
        )
        .expect("add hook");

    let result = store
        .evaluate_hooks(
            &HookType::PreExecution,
            "bash",
            &json!({"command": "echo hi"}),
        )
        .expect("evaluate");

    match result {
        HookResult::Block(reason) => assert_eq!(reason, "bash not allowed in review phase"),
        other => panic!("expected Block, got {:?}", other),
    }

    // Other tools not affected
    let result2 = store
        .evaluate_hooks(&HookType::PreExecution, "read", &json!({"path": "foo.rs"}))
        .expect("evaluate");
    assert!(matches!(result2, HookResult::Allow));
}

// ---------------------------------------------------------------------------
// Test 7: test_hook_enriches_input
// ---------------------------------------------------------------------------

#[test]
fn test_hook_enriches_input() {
    let store = new_store();

    store
        .add_tool_hook(
            "enrich-bash",
            "pre_execution",
            Some("bash"),
            None,
            "enrich",
            Some(&json!({"enrichment": {"cwd": "/workspace", "timeout": 30000}})),
        )
        .expect("add hook");

    let result = store
        .evaluate_hooks(
            &HookType::PreExecution,
            "bash",
            &json!({"command": "echo hi"}),
        )
        .expect("evaluate");

    match result {
        HookResult::Enrich(value) => {
            assert_eq!(
                value.get("command").and_then(|v| v.as_str()),
                Some("echo hi")
            );
            assert_eq!(
                value.get("cwd").and_then(|v| v.as_str()),
                Some("/workspace")
            );
            assert_eq!(value.get("timeout").and_then(|v| v.as_i64()), Some(30000));
        }
        other => panic!("expected Enrich, got {:?}", other),
    }
}

// ---------------------------------------------------------------------------
// Test 8: test_hook_requests_approval
// ---------------------------------------------------------------------------

#[test]
fn test_hook_requests_approval() {
    let store = new_store();

    store
        .add_tool_hook(
            "approve-bash",
            "pre_execution",
            Some("bash"),
            None,
            "request_approval",
            Some(&json!({"reason": "high-risk tool requires approval"})),
        )
        .expect("add hook");

    let result = store
        .evaluate_hooks(&HookType::PreExecution, "bash", &json!({}))
        .expect("evaluate");

    match result {
        HookResult::RequestApproval(reason) => {
            assert_eq!(reason, "high-risk tool requires approval");
        }
        other => panic!("expected RequestApproval, got {:?}", other),
    }
}

// ---------------------------------------------------------------------------
// Test 9: test_hooks_evaluated_in_order
// ---------------------------------------------------------------------------

#[test]
fn test_hooks_evaluated_in_order() {
    let store = new_store();

    // First hook enriches
    store
        .add_tool_hook(
            "enrich-first",
            "pre_execution",
            Some("bash"),
            None,
            "enrich",
            Some(&json!({"enrichment": {"extra": "data"}})),
        )
        .expect("add enrich hook");

    // Second hook blocks — block should win since it's evaluated after enrich
    // But per spec: first Block wins, enrich accumulates
    // With enrich first and block second, block wins
    store
        .add_tool_hook(
            "block-second",
            "pre_execution",
            Some("bash"),
            None,
            "block",
            Some(&json!({"reason": "blocked"})),
        )
        .expect("add block hook");

    let result = store
        .evaluate_hooks(&HookType::PreExecution, "bash", &json!({}))
        .expect("evaluate");

    // Block evaluated after Enrich in row order -> Block wins
    match result {
        HookResult::Block(reason) => assert_eq!(reason, "blocked"),
        other => panic!("expected Block, got {:?}", other),
    }

    // Now reverse order: block first, enrich second
    // Delete and re-add
    store.delete_all_hooks().expect("clear hooks");

    store
        .add_tool_hook(
            "block-first",
            "pre_execution",
            Some("bash"),
            None,
            "block",
            Some(&json!({"reason": "blocked first"})),
        )
        .expect("add block hook");

    store
        .add_tool_hook(
            "enrich-second",
            "pre_execution",
            Some("bash"),
            None,
            "enrich",
            Some(&json!({"enrichment": {"extra": "data"}})),
        )
        .expect("add enrich hook");

    let result2 = store
        .evaluate_hooks(&HookType::PreExecution, "bash", &json!({}))
        .expect("evaluate");

    // Block is first -> returns immediately
    match result2 {
        HookResult::Block(reason) => assert_eq!(reason, "blocked first"),
        other => panic!("expected Block, got {:?}", other),
    }
}

#[test]
fn test_block_hook_precedes_approval_and_records_all_matching_provenance() {
    let store = new_store();
    store
        .add_tool_hook(
            "a-approval",
            "pre_execution",
            Some("bash"),
            None,
            "request_approval",
            Some(&json!({"reason": "approval alone is insufficient"})),
        )
        .unwrap();
    store
        .add_tool_hook(
            "z-block",
            "pre_execution",
            Some("bash"),
            None,
            "block",
            Some(&json!({"reason": "authoritative block"})),
        )
        .unwrap();

    let evaluation = store
        .evaluate_hooks_with_provenance(&HookType::PreExecution, "bash", &json!({}))
        .unwrap();

    match evaluation.result {
        HookResult::Block(reason) => assert_eq!(reason, "authoritative block"),
        other => panic!("expected Block, got {other:?}"),
    }
    assert_eq!(evaluation.matched_hook_ids, ["a-approval", "z-block"]);
}

#[test]
fn test_hook_contract_rejects_noop_enrichment_and_unknown_condition() {
    assert!(validate_tool_hook_contract(
        "pre_execution",
        None,
        "enrich",
        Some(&json!({"enrichment": {}})),
    )
    .is_err());
    assert!(validate_tool_hook_contract(
        "pre_execution",
        Some(&json!({"unsupported": true})),
        "block",
        None,
    )
    .is_err());
}

// ---------------------------------------------------------------------------
// Test 10: test_mcp_descriptors_format
// ---------------------------------------------------------------------------

#[test]
fn test_mcp_descriptors_format() {
    let store = new_store();

    store
        .register_tool_capability(
            "read",
            "Read files from disk",
            Some(&json!({"type": "object", "properties": {"path": {"type": "string"}}})),
            None,
            false,
            "low",
        )
        .expect("register read");

    store
        .register_tool_capability(
            "bash",
            "Execute shell commands",
            Some(&json!({"type": "object", "properties": {"command": {"type": "string"}}})),
            None,
            true,
            "high",
        )
        .expect("register bash");

    let descriptors = store.get_mcp_descriptors().expect("descriptors");
    assert_eq!(descriptors.len(), 2);

    // Read: no annotations (low risk, no approval)
    let read_desc = descriptors.iter().find(|d| d.name == "read").unwrap();
    assert_eq!(read_desc.description, "Read files from disk");
    assert!(read_desc.input_schema.is_some());
    assert!(read_desc.annotations.is_none());

    // Bash: has annotations (high risk + requires approval)
    let bash_desc = descriptors.iter().find(|d| d.name == "bash").unwrap();
    assert_eq!(bash_desc.description, "Execute shell commands");
    assert!(bash_desc.annotations.is_some());
    let ann = bash_desc.annotations.as_ref().unwrap();
    assert_eq!(
        ann.get("requires_approval").and_then(|v| v.as_bool()),
        Some(true)
    );
    assert_eq!(ann.get("risk_level").and_then(|v| v.as_str()), Some("high"));

    // Verify to_value() produces valid MCP-like JSON
    let read_val = read_desc.to_value();
    assert_eq!(read_val.get("name").and_then(|v| v.as_str()), Some("read"));
    assert_eq!(
        read_val.get("description").and_then(|v| v.as_str()),
        Some("Read files from disk")
    );
    assert!(read_val.get("inputSchema").is_some());
}

// ---------------------------------------------------------------------------
// Test 11: test_disabled_hook_not_evaluated
// ---------------------------------------------------------------------------

#[test]
fn test_disabled_hook_not_evaluated() {
    let store = new_store();

    // Add a blocking hook
    store
        .add_tool_hook(
            "block-hook",
            "pre_execution",
            Some("bash"),
            None,
            "block",
            Some(&json!({"reason": "should not fire"})),
        )
        .expect("add hook");

    // Verify it blocks
    let result = store
        .evaluate_hooks(&HookType::PreExecution, "bash", &json!({}))
        .expect("evaluate");
    assert!(matches!(result, HookResult::Block(_)));

    // Disable it
    store
        .set_hook_enabled("block-hook", false)
        .expect("disable hook");

    // Now should allow
    let result2 = store
        .evaluate_hooks(&HookType::PreExecution, "bash", &json!({}))
        .expect("evaluate");
    assert!(matches!(result2, HookResult::Allow));
}

// ---------------------------------------------------------------------------
// Test 12: test_export_import_round_trip
// ---------------------------------------------------------------------------

#[test]
fn test_export_import_round_trip() {
    let store = new_store();

    store
        .register_tool_capability(
            "read",
            "Read files",
            Some(&json!({"type": "object"})),
            None,
            false,
            "low",
        )
        .expect("register read");

    store
        .register_tool_capability(
            "bash",
            "Run commands",
            None,
            Some(&json!({"type": "object"})),
            true,
            "high",
        )
        .expect("register bash");

    let exported = store.export_tool_capabilities().expect("export");
    assert_eq!(exported.len(), 2);

    // Import into a fresh store
    let store2 = new_store();
    for entry in &exported {
        store2.import_tool_capability_entry(entry).expect("import");
    }

    let imported = store2.list_tool_capabilities().expect("list");
    assert_eq!(imported.len(), 2);

    // Verify round-trip fidelity
    let read_cap = store2
        .get_tool_capability("read")
        .expect("get")
        .expect("exists");
    assert_eq!(read_cap.description, "Read files");
    assert!(!read_cap.requires_approval);
    assert_eq!(read_cap.risk_level, RiskLevel::Low);
    assert!(read_cap.input_schema.is_some());

    let bash_cap = store2
        .get_tool_capability("bash")
        .expect("get")
        .expect("exists");
    assert_eq!(bash_cap.description, "Run commands");
    assert!(bash_cap.requires_approval);
    assert_eq!(bash_cap.risk_level, RiskLevel::High);
    assert!(bash_cap.output_schema.is_some());
}
