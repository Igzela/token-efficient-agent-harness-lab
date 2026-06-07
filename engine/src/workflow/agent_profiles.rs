use std::collections::HashMap;

// ---------------------------------------------------------------------------
// AgentProfileId — String newtype
// ---------------------------------------------------------------------------

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct AgentProfileId(pub String);

impl AgentProfileId {
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl From<&str> for AgentProfileId {
    fn from(s: &str) -> Self {
        Self(s.to_string())
    }
}

impl std::fmt::Display for AgentProfileId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

// ---------------------------------------------------------------------------
// AgentProfileRole
// ---------------------------------------------------------------------------

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum AgentProfileRole {
    Planner,
    Implementer,
    Reviewer,
    Tester,
    Researcher,
}

impl AgentProfileRole {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Planner => "planner",
            Self::Implementer => "implementer",
            Self::Reviewer => "reviewer",
            Self::Tester => "tester",
            Self::Researcher => "researcher",
        }
    }

    pub fn parse_str(s: &str) -> Option<Self> {
        match s {
            "planner" => Some(Self::Planner),
            "implementer" => Some(Self::Implementer),
            "reviewer" => Some(Self::Reviewer),
            "tester" => Some(Self::Tester),
            "researcher" => Some(Self::Researcher),
            _ => None,
        }
    }
}

impl std::fmt::Display for AgentProfileRole {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.as_str())
    }
}

// ---------------------------------------------------------------------------
// WorkspaceScope
// ---------------------------------------------------------------------------

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum WorkspaceScope {
    Full,
    Task,
    Isolated,
}

impl WorkspaceScope {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Full => "full",
            Self::Task => "task",
            Self::Isolated => "isolated",
        }
    }

    pub fn parse_str(s: &str) -> Option<Self> {
        match s {
            "full" => Some(Self::Full),
            "task" => Some(Self::Task),
            "isolated" => Some(Self::Isolated),
            _ => None,
        }
    }
}

impl Default for WorkspaceScope {
    fn default() -> Self {
        Self::Task
    }
}

impl std::fmt::Display for WorkspaceScope {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.as_str())
    }
}

// ---------------------------------------------------------------------------
// AgentProfile
// ---------------------------------------------------------------------------

#[derive(Clone, Debug)]
pub struct AgentProfile {
    pub profile_id: AgentProfileId,
    pub role: AgentProfileRole,
    pub tools: Vec<String>,
    pub model_hint: Option<String>,
    pub context_budget_tokens: Option<u64>,
    pub workspace_scope: WorkspaceScope,
    pub executor_preference: Option<String>,
    pub max_retries: u32,
}

// ---------------------------------------------------------------------------
// AgentProfileRegistry — in-memory registry with default profiles
// ---------------------------------------------------------------------------

pub struct AgentProfileRegistry {
    profiles: HashMap<AgentProfileId, AgentProfile>,
}

impl AgentProfileRegistry {
    pub fn new() -> Self {
        let mut registry = Self {
            profiles: HashMap::new(),
        };
        registry.register_defaults();
        registry
    }

    pub fn register(&mut self, profile: AgentProfile) {
        self.profiles.insert(profile.profile_id.clone(), profile);
    }

    pub fn get(&self, profile_id: &AgentProfileId) -> Option<&AgentProfile> {
        self.profiles.get(profile_id)
    }

    pub fn list_all(&self) -> Vec<&AgentProfile> {
        self.profiles.values().collect()
    }

    pub fn remove(&mut self, profile_id: &AgentProfileId) -> Option<AgentProfile> {
        self.profiles.remove(profile_id)
    }

    pub fn get_for_role(&self, role: &AgentProfileRole) -> Option<&AgentProfile> {
        self.profiles.values().find(|p| p.role == *role)
    }

    fn register_defaults(&mut self) {
        self.register(AgentProfile {
            profile_id: AgentProfileId("planner".to_string()),
            role: AgentProfileRole::Planner,
            tools: vec!["read".to_string(), "analyze".to_string()],
            model_hint: None,
            context_budget_tokens: Some(20_000),
            workspace_scope: WorkspaceScope::Full,
            executor_preference: None,
            max_retries: 3,
        });

        self.register(AgentProfile {
            profile_id: AgentProfileId("implementer".to_string()),
            role: AgentProfileRole::Implementer,
            tools: vec![
                "read".to_string(),
                "write".to_string(),
                "edit".to_string(),
                "bash".to_string(),
            ],
            model_hint: None,
            context_budget_tokens: Some(40_000),
            workspace_scope: WorkspaceScope::Task,
            executor_preference: None,
            max_retries: 3,
        });

        self.register(AgentProfile {
            profile_id: AgentProfileId("reviewer".to_string()),
            role: AgentProfileRole::Reviewer,
            tools: vec!["read".to_string(), "comment".to_string()],
            model_hint: None,
            context_budget_tokens: Some(20_000),
            workspace_scope: WorkspaceScope::Full,
            executor_preference: None,
            max_retries: 3,
        });

        self.register(AgentProfile {
            profile_id: AgentProfileId("tester".to_string()),
            role: AgentProfileRole::Tester,
            tools: vec!["read".to_string(), "bash".to_string(), "write".to_string()],
            model_hint: None,
            context_budget_tokens: Some(30_000),
            workspace_scope: WorkspaceScope::Task,
            executor_preference: None,
            max_retries: 3,
        });

        self.register(AgentProfile {
            profile_id: AgentProfileId("researcher".to_string()),
            role: AgentProfileRole::Researcher,
            tools: vec!["read".to_string(), "search".to_string()],
            model_hint: None,
            context_budget_tokens: Some(15_000),
            workspace_scope: WorkspaceScope::Full,
            executor_preference: None,
            max_retries: 3,
        });
    }
}

impl Default for AgentProfileRegistry {
    fn default() -> Self {
        Self::new()
    }
}

// ---------------------------------------------------------------------------
// Profile resolution helpers
// ---------------------------------------------------------------------------

/// Map a task_type string to the most appropriate profile role.
pub fn task_type_to_profile_role(task_type: &str) -> AgentProfileRole {
    match task_type {
        "plan" => AgentProfileRole::Planner,
        "analyze" => AgentProfileRole::Researcher,
        "execute" | "fix" | "implement" => AgentProfileRole::Implementer,
        "review" => AgentProfileRole::Reviewer,
        "test" | "verify" => AgentProfileRole::Tester,
        _ => AgentProfileRole::Implementer,
    }
}

/// Map a task_type string to the default profile_id.
pub fn task_type_to_profile_id(task_type: &str) -> AgentProfileId {
    let role = task_type_to_profile_role(task_type);
    AgentProfileId(role.as_str().to_string())
}
