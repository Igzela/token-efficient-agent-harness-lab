use serde::{Deserialize, Serialize};
#[cfg(test)]
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

// ---------------------------------------------------------------------------
// Trait
// ---------------------------------------------------------------------------

pub trait ModelProvider: Send + Sync {
    fn invoke(&self, tier: &ModelTier, prompt: &str, max_tokens: i64) -> ModelResponse;
}

// ---------------------------------------------------------------------------
// StubModelProvider (test fixture)
// ---------------------------------------------------------------------------

#[cfg(test)]
pub struct StubModelProvider;

#[cfg(test)]
impl StubModelProvider {
    pub fn new() -> Self {
        Self
    }
}

#[cfg(test)]
impl Default for StubModelProvider {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
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
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

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
    fn model_response_default() {
        let r = ModelResponse::default();
        assert_eq!(r.provider, "stub");
        assert_eq!(r.token_usage, 0);
        assert!(r.raw_response.is_none());
    }
}
