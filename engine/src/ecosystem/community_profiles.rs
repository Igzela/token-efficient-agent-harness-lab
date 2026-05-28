use serde::{Deserialize, Serialize};
use std::collections::HashMap;

pub const COMMUNITY_PROFILE_SCHEMA_VERSION: &str = "community_profile.v1";

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ModelProfile {
    pub schema_version: String,
    pub profile_id: String,
    pub name: String,
    pub provider: String,
    pub model_name: String,
    pub capabilities: Vec<String>,
    pub cost_per_1k_tokens: f64,
    pub max_context: i64,
    pub created_at: f64,
    pub author: String,
    pub tags: Vec<String>,
}

impl ModelProfile {
    pub fn to_dict(&self) -> serde_json::Value {
        serde_json::to_value(self).unwrap_or(serde_json::Value::Null)
    }
}

pub struct CommunityProfileRegistry {
    registered: HashMap<String, ModelProfile>,
}

impl Default for CommunityProfileRegistry {
    fn default() -> Self {
        Self::new()
    }
}

impl CommunityProfileRegistry {
    pub fn new() -> Self {
        Self {
            registered: HashMap::new(),
        }
    }

    pub fn register_profile(&mut self, profile: &ModelProfile) -> bool {
        let errors = self.validate_profile(profile);
        if !errors.is_empty() {
            return false;
        }
        if self.registered.contains_key(&profile.profile_id) {
            return false;
        }
        self.registered
            .insert(profile.profile_id.clone(), profile.clone());
        true
    }

    pub fn unregister_profile(&mut self, profile_id: &str) -> bool {
        self.registered.remove(profile_id).is_some()
    }

    pub fn get_profile(&self, profile_id: &str) -> Option<&ModelProfile> {
        self.registered.get(profile_id)
    }

    pub fn list_profiles(&self) -> Vec<&ModelProfile> {
        self.registered.values().collect()
    }

    pub fn search_by_provider(&self, provider: &str) -> Vec<&ModelProfile> {
        let provider_lower = provider.to_lowercase();
        self.registered
            .values()
            .filter(|p| p.provider.to_lowercase() == provider_lower)
            .collect()
    }

    pub fn search_by_tag(&self, tag: &str) -> Vec<&ModelProfile> {
        let tag_lower = tag.to_lowercase();
        self.registered
            .values()
            .filter(|p| p.tags.iter().any(|t| t.to_lowercase() == tag_lower))
            .collect()
    }

    pub fn validate_profile(&self, profile: &ModelProfile) -> Vec<String> {
        let mut errors = Vec::new();
        if profile.profile_id.is_empty() {
            errors.push("profile_id is required".to_string());
        }
        if profile.name.is_empty() {
            errors.push("name is required".to_string());
        }
        if profile.provider.is_empty() {
            errors.push("provider is required".to_string());
        }
        if profile.model_name.is_empty() {
            errors.push("model_name is required".to_string());
        }
        if profile.author.is_empty() {
            errors.push("author is required".to_string());
        }
        if profile.cost_per_1k_tokens < 0.0 {
            errors.push("cost_per_1k_tokens must be non-negative".to_string());
        }
        if profile.max_context <= 0 {
            errors.push("max_context must be positive".to_string());
        }
        if profile.schema_version != COMMUNITY_PROFILE_SCHEMA_VERSION {
            errors.push(format!(
                "invalid schema_version: '{}'",
                profile.schema_version
            ));
        }
        errors
    }
}

pub fn make_profile(overrides: HashMap<String, serde_json::Value>) -> ModelProfile {
    let default = ModelProfile {
        schema_version: COMMUNITY_PROFILE_SCHEMA_VERSION.to_string(),
        profile_id: "test-profile".to_string(),
        name: "Test Profile".to_string(),
        provider: "openai".to_string(),
        model_name: "gpt-4".to_string(),
        capabilities: vec!["chat".to_string(), "code".to_string()],
        cost_per_1k_tokens: 0.03,
        max_context: 8192,
        created_at: 1000.0,
        author: "test_author".to_string(),
        tags: vec!["general".to_string(), "coding".to_string()],
    };

    ModelProfile {
        profile_id: overrides
            .get("profile_id")
            .and_then(|v| v.as_str())
            .unwrap_or(&default.profile_id)
            .to_string(),
        name: overrides
            .get("name")
            .and_then(|v| v.as_str())
            .unwrap_or(&default.name)
            .to_string(),
        provider: overrides
            .get("provider")
            .and_then(|v| v.as_str())
            .unwrap_or(&default.provider)
            .to_string(),
        model_name: overrides
            .get("model_name")
            .and_then(|v| v.as_str())
            .unwrap_or(&default.model_name)
            .to_string(),
        capabilities: overrides
            .get("capabilities")
            .and_then(|v| v.as_array())
            .map(|arr| {
                arr.iter()
                    .filter_map(|v| v.as_str().map(String::from))
                    .collect()
            })
            .unwrap_or(default.capabilities),
        cost_per_1k_tokens: overrides
            .get("cost_per_1k_tokens")
            .and_then(|v| v.as_f64())
            .unwrap_or(default.cost_per_1k_tokens),
        max_context: overrides
            .get("max_context")
            .and_then(|v| v.as_i64())
            .unwrap_or(default.max_context),
        created_at: overrides
            .get("created_at")
            .and_then(|v| v.as_f64())
            .unwrap_or(default.created_at),
        author: overrides
            .get("author")
            .and_then(|v| v.as_str())
            .unwrap_or(&default.author)
            .to_string(),
        tags: overrides
            .get("tags")
            .and_then(|v| v.as_array())
            .map(|arr| {
                arr.iter()
                    .filter_map(|v| v.as_str().map(String::from))
                    .collect()
            })
            .unwrap_or(default.tags),
        ..default
    }
}
