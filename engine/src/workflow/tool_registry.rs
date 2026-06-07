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
