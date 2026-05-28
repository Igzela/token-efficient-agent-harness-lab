use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::HashMap;

pub const TOOL_ADAPTER_SCHEMA_VERSION: &str = "tool_adapter.v1";

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ToolDefinition {
    pub schema_version: String,
    pub tool_id: String,
    pub name: String,
    pub description: String,
    pub input_schema: Value,
    pub output_schema: Value,
    pub timeout_seconds: i64,
    pub requires_network: bool,
}

impl ToolDefinition {
    pub fn to_dict(&self) -> Value {
        serde_json::to_value(self).unwrap_or(Value::Null)
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ToolExecutionRequest {
    pub tool_id: String,
    pub arguments: Value,
    pub request_id: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ToolExecutionResult {
    pub request_id: String,
    pub tool_id: String,
    pub success: bool,
    pub output: Option<Value>,
    pub error: Option<String>,
    pub duration_ms: f64,
}

pub struct ToolAdapterManager {
    registered: HashMap<String, ToolDefinition>,
}

impl Default for ToolAdapterManager {
    fn default() -> Self {
        Self::new()
    }
}

impl ToolAdapterManager {
    pub fn new() -> Self {
        Self {
            registered: HashMap::new(),
        }
    }

    pub fn register_tool(&mut self, tool: &ToolDefinition) -> bool {
        let errors = self.validate_tool(tool);
        if !errors.is_empty() {
            return false;
        }
        if self.registered.contains_key(&tool.tool_id) {
            return false;
        }
        self.registered.insert(tool.tool_id.clone(), tool.clone());
        true
    }

    pub fn unregister_tool(&mut self, tool_id: &str) -> bool {
        self.registered.remove(tool_id).is_some()
    }

    pub fn get_tool(&self, tool_id: &str) -> Option<&ToolDefinition> {
        self.registered.get(tool_id)
    }

    pub fn list_tools(&self) -> Vec<&ToolDefinition> {
        self.registered.values().collect()
    }

    pub fn validate_tool(&self, tool: &ToolDefinition) -> Vec<String> {
        let mut errors = Vec::new();
        if tool.tool_id.is_empty() {
            errors.push("tool_id is required".to_string());
        }
        if tool.name.is_empty() {
            errors.push("name is required".to_string());
        }
        if tool.description.is_empty() {
            errors.push("description is required".to_string());
        }
        if tool.timeout_seconds <= 0 {
            errors.push("timeout_seconds must be positive".to_string());
        }
        if tool.schema_version != TOOL_ADAPTER_SCHEMA_VERSION {
            errors.push(format!("invalid schema_version: '{}'", tool.schema_version));
        }
        errors
    }

    pub fn execute_tool(&self, request: &ToolExecutionRequest, now: f64) -> ToolExecutionResult {
        let start = now;
        let tool = self.registered.get(&request.tool_id);

        let duration_ms = (now - start) * 1000.0;

        match tool {
            None => ToolExecutionResult {
                request_id: request.request_id.clone(),
                tool_id: request.tool_id.clone(),
                success: false,
                output: None,
                error: Some(format!("tool not found: {}", request.tool_id)),
                duration_ms,
            },
            Some(_) => ToolExecutionResult {
                request_id: request.request_id.clone(),
                tool_id: request.tool_id.clone(),
                success: true,
                output: Some(serde_json::json!({})),
                error: None,
                duration_ms,
            },
        }
    }
}

pub fn make_tool(overrides: HashMap<String, Value>) -> ToolDefinition {
    let default = ToolDefinition {
        schema_version: TOOL_ADAPTER_SCHEMA_VERSION.to_string(),
        tool_id: "test-tool".to_string(),
        name: "Test Tool".to_string(),
        description: "A test tool".to_string(),
        input_schema: serde_json::json!({"type": "object", "properties": {}}),
        output_schema: serde_json::json!({"type": "object", "properties": {}}),
        timeout_seconds: 30,
        requires_network: false,
    };

    ToolDefinition {
        tool_id: overrides
            .get("tool_id")
            .and_then(|v| v.as_str())
            .unwrap_or(&default.tool_id)
            .to_string(),
        name: overrides
            .get("name")
            .and_then(|v| v.as_str())
            .unwrap_or(&default.name)
            .to_string(),
        description: overrides
            .get("description")
            .and_then(|v| v.as_str())
            .unwrap_or(&default.description)
            .to_string(),
        input_schema: overrides
            .get("input_schema")
            .cloned()
            .unwrap_or(default.input_schema),
        output_schema: overrides
            .get("output_schema")
            .cloned()
            .unwrap_or(default.output_schema),
        timeout_seconds: overrides
            .get("timeout_seconds")
            .and_then(|v| v.as_i64())
            .unwrap_or(default.timeout_seconds),
        requires_network: overrides
            .get("requires_network")
            .and_then(|v| v.as_bool())
            .unwrap_or(default.requires_network),
        ..default
    }
}
