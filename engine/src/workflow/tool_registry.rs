use serde_json::Value;

// ---------------------------------------------------------------------------
// RiskLevel
// ---------------------------------------------------------------------------

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum RiskLevel {
    Low,
    Medium,
    High,
}

impl RiskLevel {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Low => "low",
            Self::Medium => "medium",
            Self::High => "high",
        }
    }

    pub fn parse_str(s: &str) -> Option<Self> {
        match s {
            "low" => Some(Self::Low),
            "medium" => Some(Self::Medium),
            "high" => Some(Self::High),
            _ => None,
        }
    }
}

impl std::fmt::Display for RiskLevel {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.as_str())
    }
}

// ---------------------------------------------------------------------------
// ToolCapability
// ---------------------------------------------------------------------------

#[derive(Clone, Debug)]
pub struct ToolCapability {
    pub tool_name: String,
    pub description: String,
    pub input_schema: Option<Value>,
    pub output_schema: Option<Value>,
    pub requires_approval: bool,
    pub risk_level: RiskLevel,
    pub created_at: String,
}

// ---------------------------------------------------------------------------
// HookType
// ---------------------------------------------------------------------------

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum HookType {
    PreExecution,
    PostExecution,
}

impl HookType {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::PreExecution => "pre_execution",
            Self::PostExecution => "post_execution",
        }
    }

    pub fn parse_str(s: &str) -> Option<Self> {
        match s {
            "pre_execution" => Some(Self::PreExecution),
            "post_execution" => Some(Self::PostExecution),
            _ => None,
        }
    }
}

// ---------------------------------------------------------------------------
// HookAction
// ---------------------------------------------------------------------------

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum HookAction {
    Log,
    Block,
    Enrich,
    RequestApproval,
}

impl HookAction {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Log => "log",
            Self::Block => "block",
            Self::Enrich => "enrich",
            Self::RequestApproval => "request_approval",
        }
    }

    pub fn parse_str(s: &str) -> Option<Self> {
        match s {
            "log" => Some(Self::Log),
            "block" => Some(Self::Block),
            "enrich" => Some(Self::Enrich),
            "request_approval" => Some(Self::RequestApproval),
            _ => None,
        }
    }
}

// ---------------------------------------------------------------------------
// HookResult — returned from evaluate_hooks
// ---------------------------------------------------------------------------

#[derive(Clone, Debug)]
pub enum HookResult {
    Allow,
    Block(String),
    Enrich(Value),
    RequestApproval(String),
}

#[derive(Clone, Debug)]
pub struct HookEvaluation {
    pub result: HookResult,
    pub matched_hook_ids: Vec<String>,
}

// ---------------------------------------------------------------------------
// ToolHook
// ---------------------------------------------------------------------------

#[derive(Clone, Debug)]
pub struct ToolHook {
    pub hook_id: String,
    pub hook_type: HookType,
    pub tool_name: Option<String>,
    pub condition: Option<Value>,
    pub action: HookAction,
    pub action_config: Option<Value>,
    pub enabled: bool,
    pub created_at: String,
}

pub(crate) fn validate_tool_hook_contract(
    hook_type: &str,
    condition: Option<&Value>,
    action: &str,
    action_config: Option<&Value>,
) -> Result<(), String> {
    if !matches!(hook_type, "pre_execution" | "post_execution")
        || !matches!(action, "log" | "block" | "enrich" | "request_approval")
        || (hook_type == "post_execution" && action == "request_approval")
    {
        return Err("invalid tool hook type or action".to_string());
    }
    if let Some(condition) = condition {
        let object = condition
            .as_object()
            .ok_or_else(|| "tool hook condition must be an object".to_string())?;
        let path = object
            .get("path")
            .and_then(Value::as_str)
            .ok_or_else(|| "tool hook condition requires a path string".to_string())?;
        if object.len() != 2
            || !object.contains_key("equals")
            || path.is_empty()
            || path.len() > 256
            || path.split('.').any(|part| {
                part.is_empty()
                    || !part
                        .bytes()
                        .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'_' | b'-'))
            })
        {
            return Err(
                "tool hook condition must contain only bounded path and equals fields".to_string(),
            );
        }
    }

    match action {
        "enrich" => {
            let object = action_config
                .and_then(Value::as_object)
                .ok_or_else(|| "enrich hook requires an action_config object".to_string())?;
            let enrichment = object
                .get("enrichment")
                .and_then(Value::as_object)
                .ok_or_else(|| "enrich hook requires an enrichment object".to_string())?;
            if object.len() != 1 || enrichment.is_empty() {
                return Err(
                    "enrich hook action_config must contain only a non-empty enrichment object"
                        .to_string(),
                );
            }
        }
        "block" | "request_approval" => {
            if let Some(config) = action_config {
                let object = config.as_object().ok_or_else(|| {
                    "block or approval hook action_config must be an object".to_string()
                })?;
                let reason = object
                    .get("reason")
                    .and_then(Value::as_str)
                    .ok_or_else(|| "hook action_config reason must be a string".to_string())?;
                if object.len() != 1 || reason.trim().is_empty() || reason.len() > 512 {
                    return Err(
                        "hook action_config must contain only a bounded non-empty reason"
                            .to_string(),
                    );
                }
            }
        }
        "log" => {
            if action_config
                .and_then(Value::as_object)
                .is_some_and(|object| !object.is_empty())
                || action_config.is_some_and(|config| !config.is_object())
            {
                return Err("log hook action_config must be absent or empty".to_string());
            }
        }
        _ => unreachable!("validated action"),
    }
    Ok(())
}

// ---------------------------------------------------------------------------
// ToolDescriptor — MCP-like metadata
// ---------------------------------------------------------------------------

#[derive(Clone, Debug)]
pub struct ToolDescriptor {
    pub name: String,
    pub description: String,
    pub input_schema: Option<Value>,
    pub annotations: Option<Value>,
}

impl ToolDescriptor {
    pub fn to_value(&self) -> Value {
        let mut map = serde_json::json!({
            "name": self.name,
            "description": self.description,
        });
        if let Some(ref schema) = self.input_schema {
            map["inputSchema"] = schema.clone();
        }
        if let Some(ref ann) = self.annotations {
            map["annotations"] = ann.clone();
        }
        map
    }
}
