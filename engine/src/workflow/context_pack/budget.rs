use std::collections::HashMap;

use serde_json::Value;

pub fn check_budget_compliance(
    pack_data: &HashMap<String, Value>,
    total_tokens_used: i64,
) -> (bool, String) {
    let budget = pack_data
        .get("context_budget")
        .or_else(|| pack_data.get("budget"));
    let max_tokens = budget
        .and_then(|b| b.get("max_context_tokens"))
        .and_then(Value::as_i64)
        .unwrap_or(0);
    if max_tokens <= 0 {
        return (true, "no budget defined".to_string());
    }
    if total_tokens_used <= max_tokens {
        (
            true,
            format!("within budget ({total_tokens_used}/{max_tokens})"),
        )
    } else {
        (
            false,
            format!("over budget ({total_tokens_used}/{max_tokens})"),
        )
    }
}

pub fn apply_prune_policy(
    pack_data: &HashMap<String, Value>,
    current_tokens: i64,
    max_tokens: i64,
) -> Result<(HashMap<String, Value>, String), String> {
    let policy = pack_data
        .get("pack_prune_policy")
        .and_then(Value::as_str)
        .unwrap_or("deny_if_over_budget");

    if current_tokens <= max_tokens {
        return Ok((pack_data.clone(), "no_pruning_needed".to_string()));
    }

    if policy == "deny_if_over_budget" {
        return Err(format!(
            "pack over budget ({current_tokens}/{max_tokens}) and policy is deny_if_over_budget"
        ));
    }

    let mut pruned = pack_data.clone();
    let layers = pruned
        .get("context_layers")
        .and_then(Value::as_object)
        .cloned()
        .unwrap_or_default();

    if policy == "drop_recent_evidence_first" {
        if let Some(re) = layers.get("recent_evidence") {
            if re.as_array().map_or(false, |a| !a.is_empty()) {
                let mut new_layers = layers.clone();
                new_layers.insert("recent_evidence".to_string(), Value::Array(vec![]));
                pruned.insert("context_layers".to_string(), Value::Object(new_layers));
                return Ok((pruned, "dropped_recent_evidence".to_string()));
            }
        }
    } else if policy == "drop_memory_digest_first" {
        if let Some(md) = layers.get("memory_digest") {
            if md.is_object() {
                let mut new_layers = layers.clone();
                let mut empty_digest = serde_json::Map::new();
                empty_digest.insert("source_refs".to_string(), Value::Array(vec![]));
                empty_digest.insert(
                    "expiry_policy".to_string(),
                    Value::String("on_prune".to_string()),
                );
                empty_digest.insert(
                    "conflict_resolution".to_string(),
                    Value::String("drop".to_string()),
                );
                new_layers.insert("memory_digest".to_string(), Value::Object(empty_digest));
                pruned.insert("context_layers".to_string(), Value::Object(new_layers));
                return Ok((pruned, "dropped_memory_digest".to_string()));
            }
        }
    } else if policy == "preserve_invariants" {
        if let Some(re) = layers.get("recent_evidence") {
            if re.as_array().map_or(false, |a| !a.is_empty()) {
                let mut new_layers = layers.clone();
                new_layers.insert("recent_evidence".to_string(), Value::Array(vec![]));
                pruned.insert("context_layers".to_string(), Value::Object(new_layers));
                return Ok((pruned, "dropped_recent_evidence".to_string()));
            }
        }
        if let Some(md) = layers.get("memory_digest") {
            if md.is_object() {
                let mut new_layers = layers.clone();
                let mut empty_digest = serde_json::Map::new();
                empty_digest.insert("source_refs".to_string(), Value::Array(vec![]));
                empty_digest.insert(
                    "expiry_policy".to_string(),
                    Value::String("on_prune".to_string()),
                );
                empty_digest.insert(
                    "conflict_resolution".to_string(),
                    Value::String("drop".to_string()),
                );
                new_layers.insert("memory_digest".to_string(), Value::Object(empty_digest));
                pruned.insert("context_layers".to_string(), Value::Object(new_layers));
                return Ok((pruned, "dropped_memory_digest".to_string()));
            }
        }
    }

    Err(format!(
        "cannot prune pack under budget ({current_tokens}/{max_tokens}) with policy '{policy}'"
    ))
}
