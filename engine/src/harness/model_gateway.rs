use std::collections::HashMap;
use std::fmt;

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

// ---------------------------------------------------------------------------
// Data structs
// ---------------------------------------------------------------------------

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct ModelTier {
    pub name: String,
    pub provider: String,
    pub model_id: String,
    pub max_tokens: i64,
    pub cost_per_1k_tokens: f64,
}

impl Default for ModelTier {
    fn default() -> Self {
        Self {
            name: String::new(),
            provider: "stub".to_string(),
            model_id: String::new(),
            max_tokens: 2048,
            cost_per_1k_tokens: 0.001,
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct ModelResponse {
    pub tier: String,
    pub model_id: String,
    pub content: String,
    pub token_usage: i64,
    pub provider: String,
    pub latency_ms: i64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub raw_response: Option<serde_json::Value>,
}

impl Default for ModelResponse {
    fn default() -> Self {
        Self {
            tier: String::new(),
            model_id: String::new(),
            content: String::new(),
            token_usage: 0,
            provider: "stub".to_string(),
            latency_ms: 0,
            raw_response: None,
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct ModelCapability {
    pub tier: String,
    pub supports_tools: bool,
    pub supports_thinking: bool,
    pub supports_caching: bool,
    pub max_context_tokens: i64,
    pub cost_per_1k_tokens: f64,
}

impl Default for ModelCapability {
    fn default() -> Self {
        Self {
            tier: String::new(),
            supports_tools: false,
            supports_thinking: false,
            supports_caching: false,
            max_context_tokens: 4096,
            cost_per_1k_tokens: 0.001,
        }
    }
}

// ---------------------------------------------------------------------------
// Trait
// ---------------------------------------------------------------------------

pub trait ModelProvider: Send + Sync {
    fn invoke(&self, tier: &ModelTier, prompt: &str, max_tokens: i64) -> ModelResponse;
}

// ---------------------------------------------------------------------------
// Error
// ---------------------------------------------------------------------------

#[derive(Debug, Clone)]
pub struct ModelGatewayUnknownTier {
    pub tier: String,
    pub available: Vec<String>,
}

impl fmt::Display for ModelGatewayUnknownTier {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "unknown tier: {:?}; available: {:?}",
            self.tier, self.available
        )
    }
}

impl std::error::Error for ModelGatewayUnknownTier {}

// ---------------------------------------------------------------------------
// StubModelProvider
// ---------------------------------------------------------------------------

pub struct StubModelProvider;

impl StubModelProvider {
    pub fn new() -> Self {
        Self
    }
}

impl Default for StubModelProvider {
    fn default() -> Self {
        Self::new()
    }
}

impl ModelProvider for StubModelProvider {
    fn invoke(&self, tier: &ModelTier, prompt: &str, max_tokens: i64) -> ModelResponse {
        let prompt_hash = {
            let mut hasher = Sha256::new();
            hasher.update(prompt.as_bytes());
            let result = hasher.finalize();
            hex::encode(result)
        };
        let short_hash = &prompt_hash[..8];
        let seed = u64::from_str_radix(short_hash, 16).unwrap_or(0);

        let content = match tier.name.as_str() {
            "strong_planner" => format!(
                "[plan:{}] Detailed plan for task with {} chars of context.",
                short_hash,
                prompt.len()
            ),
            "cheap_executor" => format!("[exec:{}] Simple execution output.", short_hash),
            "verifier" => format!("[verify:{}] Verification result: pass.", short_hash),
            "advisor" => format!("[advise:{}] Advisory guidance for task.", short_hash),
            _ => format!("[{}:{}] Generic output.", tier.name, short_hash),
        };

        let usage_ratio = 0.1 + (seed % 50) as f64 / 100.0;
        let token_usage = ((max_tokens as f64 * usage_ratio) as i64)
            .min(max_tokens)
            .max(1);

        let latency_ms = 10 + (seed % 90) as i64;

        ModelResponse {
            tier: tier.name.clone(),
            model_id: tier.model_id.clone(),
            content,
            token_usage,
            provider: "stub".to_string(),
            latency_ms,
            raw_response: None,
        }
    }
}

// ---------------------------------------------------------------------------
// ModelCapabilityRegistry
// ---------------------------------------------------------------------------

pub struct ModelCapabilityRegistry {
    tiers: HashMap<String, ModelTier>,
    capabilities: HashMap<String, ModelCapability>,
}

impl ModelCapabilityRegistry {
    pub fn new() -> Self {
        Self {
            tiers: HashMap::new(),
            capabilities: HashMap::new(),
        }
    }

    pub fn register(&mut self, tier: ModelTier, capability: ModelCapability) {
        self.tiers.insert(tier.name.clone(), tier);
        self.capabilities
            .insert(capability.tier.clone(), capability);
    }

    pub fn get_tier(&self, name: &str) -> Result<&ModelTier, ModelGatewayUnknownTier> {
        self.tiers.get(name).ok_or_else(|| ModelGatewayUnknownTier {
            tier: name.to_string(),
            available: self.tiers.keys().cloned().collect(),
        })
    }

    pub fn get_capability(&self, name: &str) -> Result<&ModelCapability, ModelGatewayUnknownTier> {
        self.capabilities
            .get(name)
            .ok_or_else(|| ModelGatewayUnknownTier {
                tier: name.to_string(),
                available: self.capabilities.keys().cloned().collect(),
            })
    }

    pub fn list_tiers(&self) -> Vec<String> {
        let mut names: Vec<String> = self.tiers.keys().cloned().collect();
        names.sort();
        names
    }
}

impl Default for ModelCapabilityRegistry {
    fn default() -> Self {
        Self::new()
    }
}

// ---------------------------------------------------------------------------
// ModelGateway
// ---------------------------------------------------------------------------

pub struct ModelGateway {
    registry: ModelCapabilityRegistry,
    provider: Box<dyn ModelProvider>,
}

impl ModelGateway {
    pub fn new(registry: ModelCapabilityRegistry, provider: Box<dyn ModelProvider>) -> Self {
        Self { registry, provider }
    }

    pub fn registry(&self) -> &ModelCapabilityRegistry {
        &self.registry
    }

    pub fn invoke(&self, tier: &str, prompt: &str, max_tokens: i64) -> ModelResponse {
        let model_tier = self
            .registry
            .get_tier(tier)
            .unwrap_or_else(|e| panic!("ModelGateway::invoke called with unknown tier: {}", e));
        self.provider.invoke(model_tier, prompt, max_tokens)
    }
}

// ---------------------------------------------------------------------------
// Default factory
// ---------------------------------------------------------------------------

fn init_default_tiers() -> Vec<(ModelTier, ModelCapability)> {
    vec![
        (
            ModelTier {
                name: "strong_planner".to_string(),
                provider: "stub".to_string(),
                model_id: "stub-planner".to_string(),
                max_tokens: 4096,
                cost_per_1k_tokens: 0.015,
            },
            ModelCapability {
                tier: "strong_planner".to_string(),
                supports_tools: true,
                supports_thinking: true,
                supports_caching: true,
                max_context_tokens: 200000,
                cost_per_1k_tokens: 0.015,
            },
        ),
        (
            ModelTier {
                name: "cheap_executor".to_string(),
                provider: "stub".to_string(),
                model_id: "stub-executor".to_string(),
                max_tokens: 2048,
                cost_per_1k_tokens: 0.001,
            },
            ModelCapability {
                tier: "cheap_executor".to_string(),
                supports_tools: true,
                supports_thinking: false,
                supports_caching: true,
                max_context_tokens: 100000,
                cost_per_1k_tokens: 0.001,
            },
        ),
        (
            ModelTier {
                name: "verifier".to_string(),
                provider: "stub".to_string(),
                model_id: "stub-verifier".to_string(),
                max_tokens: 1024,
                cost_per_1k_tokens: 0.003,
            },
            ModelCapability {
                tier: "verifier".to_string(),
                supports_tools: false,
                supports_thinking: false,
                supports_caching: true,
                max_context_tokens: 50000,
                cost_per_1k_tokens: 0.003,
            },
        ),
        (
            ModelTier {
                name: "advisor".to_string(),
                provider: "stub".to_string(),
                model_id: "stub-advisor".to_string(),
                max_tokens: 2048,
                cost_per_1k_tokens: 0.01,
            },
            ModelCapability {
                tier: "advisor".to_string(),
                supports_tools: false,
                supports_thinking: true,
                supports_caching: false,
                max_context_tokens: 100000,
                cost_per_1k_tokens: 0.01,
            },
        ),
    ]
}

pub fn create_default_registry() -> ModelCapabilityRegistry {
    let mut registry = ModelCapabilityRegistry::new();
    for (tier, capability) in init_default_tiers() {
        registry.register(tier, capability);
    }
    registry
}

pub fn create_default_gateway() -> ModelGateway {
    ModelGateway::new(
        create_default_registry(),
        Box::new(StubModelProvider::new()),
    )
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_registry_has_four_tiers() {
        let registry = create_default_registry();
        let tiers = registry.list_tiers();
        assert_eq!(tiers.len(), 4);
        assert!(tiers.contains(&"strong_planner".to_string()));
        assert!(tiers.contains(&"cheap_executor".to_string()));
        assert!(tiers.contains(&"verifier".to_string()));
        assert!(tiers.contains(&"advisor".to_string()));
    }

    #[test]
    fn get_tier_returns_correct_tier() {
        let registry = create_default_registry();
        let tier = registry.get_tier("strong_planner").unwrap();
        assert_eq!(tier.model_id, "stub-planner");
        assert_eq!(tier.max_tokens, 4096);
    }

    #[test]
    fn get_tier_unknown_returns_error() {
        let registry = create_default_registry();
        let err = registry.get_tier("nonexistent").unwrap_err();
        assert_eq!(err.tier, "nonexistent");
        assert!(err.available.len() > 0);
    }

    #[test]
    fn stub_provider_deterministic() {
        let provider = StubModelProvider::new();
        let tier = ModelTier {
            name: "strong_planner".to_string(),
            provider: "stub".to_string(),
            model_id: "stub-planner".to_string(),
            max_tokens: 4096,
            cost_per_1k_tokens: 0.015,
        };
        let r1 = provider.invoke(&tier, "hello world", 1000);
        let r2 = provider.invoke(&tier, "hello world", 1000);
        assert_eq!(r1.content, r2.content);
        assert_eq!(r1.token_usage, r2.token_usage);
        assert_eq!(r1.latency_ms, r2.latency_ms);
        assert_eq!(r1.provider, "stub");
    }

    #[test]
    fn stub_provider_different_prompts_different_hashes() {
        let provider = StubModelProvider::new();
        let tier = ModelTier {
            name: "cheap_executor".to_string(),
            provider: "stub".to_string(),
            model_id: "stub-executor".to_string(),
            max_tokens: 2048,
            cost_per_1k_tokens: 0.001,
        };
        let r1 = provider.invoke(&tier, "prompt alpha", 1000);
        let r2 = provider.invoke(&tier, "prompt beta", 1000);
        assert_ne!(r1.content, r2.content);
    }

    #[test]
    fn stub_provider_unknown_tier_generic_output() {
        let provider = StubModelProvider::new();
        let tier = ModelTier {
            name: "custom_tier".to_string(),
            provider: "stub".to_string(),
            model_id: "custom-model".to_string(),
            max_tokens: 1024,
            cost_per_1k_tokens: 0.005,
        };
        let r = provider.invoke(&tier, "test", 512);
        assert!(r.content.contains("Generic output"));
        assert_eq!(r.tier, "custom_tier");
    }

    #[test]
    fn gateway_invoke_uses_registry_and_provider() {
        let gw = create_default_gateway();
        let r = gw.invoke("verifier", "check this code", 512);
        assert_eq!(r.tier, "verifier");
        assert_eq!(r.provider, "stub");
        assert!(r.token_usage >= 1);
        assert!(r.token_usage <= 512);
    }

    #[test]
    fn gateway_capability_registry_get_capability() {
        let registry = create_default_registry();
        let cap = registry.get_capability("advisor").unwrap();
        assert!(cap.supports_thinking);
        assert!(!cap.supports_tools);
        assert_eq!(cap.max_context_tokens, 100000);
    }

    #[test]
    fn model_response_default() {
        let r = ModelResponse::default();
        assert_eq!(r.provider, "stub");
        assert_eq!(r.token_usage, 0);
        assert!(r.raw_response.is_none());
    }

    #[test]
    fn model_gateway_unknown_tier_display() {
        let err = ModelGatewayUnknownTier {
            tier: "bad".to_string(),
            available: vec!["a".to_string(), "b".to_string()],
        };
        let msg = format!("{}", err);
        assert!(msg.contains("bad"));
        assert!(msg.contains("a"));
    }

    #[test]
    fn register_custom_tier_and_invoke() {
        let mut registry = ModelCapabilityRegistry::new();
        registry.register(
            ModelTier {
                name: "custom".to_string(),
                provider: "stub".to_string(),
                model_id: "custom-model".to_string(),
                max_tokens: 1024,
                cost_per_1k_tokens: 0.005,
            },
            ModelCapability {
                tier: "custom".to_string(),
                supports_tools: true,
                supports_thinking: false,
                supports_caching: false,
                max_context_tokens: 32000,
                cost_per_1k_tokens: 0.005,
            },
        );
        let gw = ModelGateway::new(registry, Box::new(StubModelProvider::new()));
        let r = gw.invoke("custom", "test prompt", 256);
        assert_eq!(r.tier, "custom");
        assert_eq!(r.model_id, "custom-model");
    }
}
