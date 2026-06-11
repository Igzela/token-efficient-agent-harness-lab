use std::collections::HashMap;

use serde_json::Value;

use super::assembly::ContextSource;

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

fn estimate_tokens_from_value(value: &Value) -> usize {
    let text = match value {
        Value::String(s) => s.clone(),
        _ => serde_json::to_string(value).unwrap_or_default(),
    };
    (text.chars().count() / 4).max(1)
}

pub fn allocate_context_budget(
    sources: &[ContextSource],
    max_context_tokens: usize,
) -> Vec<(String, usize, bool)> {
    if sources.is_empty() {
        return Vec::new();
    }

    let mut indexed: Vec<(usize, &ContextSource)> = sources.iter().enumerate().collect();
    indexed.sort_by_key(|(_, s)| s.from_node_id.clone());

    let estimates: Vec<(usize, usize, &ContextSource)> = indexed
        .iter()
        .map(|&(i, s)| (i, estimate_tokens_from_value(&s.output), s))
        .collect();

    let total_estimate: usize = estimates.iter().map(|&(_, e, _)| e).sum();

    if total_estimate <= max_context_tokens {
        let mut result = vec![(String::new(), 0, false); sources.len()];
        for &(orig_idx, est, src) in &estimates {
            result[orig_idx] = (src.from_node_id.clone(), est, false);
        }
        return result;
    }

    let mut allocations: Vec<(usize, usize, usize, &ContextSource)> = Vec::new();
    let mut allocated_sum = 0_usize;
    for &(orig_idx, est, src) in &estimates {
        let alloc = ((est as f64 / total_estimate as f64) * max_context_tokens as f64) as usize;
        let alloc = alloc.max(1);
        allocated_sum += alloc;
        allocations.push((orig_idx, est, alloc, src));
    }

    let remainder = max_context_tokens.saturating_sub(allocated_sum);
    allocations[0].2 += remainder;

    let mut result = vec![(String::new(), 0, false); sources.len()];
    for (orig_idx, est, alloc, src) in allocations {
        result[orig_idx] = (src.from_node_id.clone(), alloc, alloc < est);
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn make_source(edge_id: &str, from_node_id: &str, text: &str) -> ContextSource {
        ContextSource {
            edge_id: edge_id.to_string(),
            from_node_id: from_node_id.to_string(),
            output: json!(text),
        }
    }

    #[test]
    fn budget_allocator_single_source_within() {
        let s = make_source("e1", "n1", &"a".repeat(400));
        let result = allocate_context_budget(&[s], 200);
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].0, "n1");
        assert_eq!(result[0].1, 100);
        assert!(!result[0].2);
    }

    #[test]
    fn budget_allocator_single_source_over() {
        let s = make_source("e1", "n1", &"a".repeat(2000));
        let result = allocate_context_budget(&[s], 100);
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].0, "n1");
        assert_eq!(result[0].1, 100);
        assert!(result[0].2);
    }

    #[test]
    fn budget_allocator_multiple_within() {
        let s1 = make_source("e1", "n1", &"a".repeat(200));
        let s2 = make_source("e2", "n2", &"b".repeat(200));
        let result = allocate_context_budget(&[s1, s2], 200);
        assert_eq!(result.len(), 2);
        assert!(!result[0].2);
        assert!(!result[1].2);
        assert_eq!(result[0].1, 50);
        assert_eq!(result[1].1, 50);
    }

    #[test]
    fn budget_allocator_multiple_over() {
        let s1 = make_source("e1", "n1", &"a".repeat(2000));
        let s2 = make_source("e2", "n2", &"b".repeat(1200));
        let result = allocate_context_budget(&[s1, s2], 400);
        assert_eq!(result.len(), 2);
        assert!(result[0].2);
        assert!(result[1].2);
        let total: usize = result.iter().map(|r| r.1).sum();
        assert_eq!(total, 400);
    }

    #[test]
    fn budget_allocator_deterministic() {
        let first_run: Vec<_> = (0..10)
            .map(|_| {
                let s1 = make_source("e1", "n2", &"a".repeat(500));
                let s2 = make_source("e2", "n1", &"b".repeat(300));
                let s3 = make_source("e3", "n3", &"c".repeat(200));
                allocate_context_budget(&[s1, s2, s3], 250)
            })
            .collect();
        for run in &first_run {
            assert_eq!(*run, first_run[0]);
        }
    }

    #[test]
    fn budget_allocator_empty_sources() {
        let result = allocate_context_budget(&[], 1000);
        assert!(result.is_empty());
    }
}
