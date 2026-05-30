use std::collections::HashMap;

use serde::{Deserialize, Serialize};
use serde_json::Value;

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct ContextBudget {
    pub max_context_tokens: i64,
    pub preferred_context_tokens: i64,
    pub max_response_tokens: Option<i64>,
    pub reserved_response_tokens: Option<i64>,
}

impl Default for ContextBudget {
    fn default() -> Self {
        Self {
            max_context_tokens: 4000,
            preferred_context_tokens: 2000,
            max_response_tokens: None,
            reserved_response_tokens: None,
        }
    }
}

impl ContextBudget {
    pub fn to_value(&self) -> Value {
        serde_json::to_value(self).unwrap()
    }
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct RetrievalPolicy {
    pub allow_retrieval: bool,
    pub allowed_ref_types: Option<Vec<String>>,
    pub forbidden_paths: Option<Vec<String>>,
    pub max_retrieval_calls: Option<i64>,
}

impl Default for RetrievalPolicy {
    fn default() -> Self {
        Self {
            allow_retrieval: true,
            allowed_ref_types: None,
            forbidden_paths: None,
            max_retrieval_calls: None,
        }
    }
}

impl RetrievalPolicy {
    pub fn to_value(&self) -> Value {
        serde_json::to_value(self).unwrap()
    }
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct MemoryDigest {
    pub source_refs: Vec<String>,
    pub expiry_policy: String,
    pub conflict_resolution: String,
    pub summary: Option<String>,
}

impl Default for MemoryDigest {
    fn default() -> Self {
        Self {
            source_refs: Vec::new(),
            expiry_policy: String::new(),
            conflict_resolution: String::new(),
            summary: None,
        }
    }
}

impl MemoryDigest {
    pub fn to_value(&self) -> Value {
        serde_json::to_value(self).unwrap()
    }
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct ContextLayers {
    pub invariants: HashMap<String, Value>,
    pub task_pack: HashMap<String, Value>,
    pub dynamic_refs: Vec<HashMap<String, Value>>,
    pub memory_digest: MemoryDigest,
    pub recent_evidence: Vec<HashMap<String, Value>>,
    pub freshness: String,
    pub cache_policy: String,
    pub pack_prune_policy: String,
}

impl Default for ContextLayers {
    fn default() -> Self {
        Self {
            invariants: HashMap::new(),
            task_pack: HashMap::new(),
            dynamic_refs: Vec::new(),
            memory_digest: MemoryDigest::default(),
            recent_evidence: Vec::new(),
            freshness: "current".to_string(),
            cache_policy: "no_cache".to_string(),
            pack_prune_policy: "preserve_invariants".to_string(),
        }
    }
}

impl ContextLayers {
    pub fn to_value(&self) -> Value {
        serde_json::to_value(self).unwrap()
    }
}
