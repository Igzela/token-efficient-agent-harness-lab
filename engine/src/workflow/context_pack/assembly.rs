use serde_json::{json, Value};

#[derive(Debug, Clone, PartialEq)]
pub struct ContextAssemblyConfig {
    pub enabled: bool,
    pub max_context_tokens: usize,
}

impl ContextAssemblyConfig {
    pub fn from_env() -> Self {
        let enabled = std::env::var("ACP_CONTEXT_ASSEMBLY_ENABLED")
            .ok()
            .map(|value| !matches!(value.as_str(), "0" | "false" | "FALSE" | "off" | "OFF"))
            .unwrap_or(true);
        let max_context_tokens = std::env::var("ACP_CONTEXT_ASSEMBLY_MAX_TOKENS")
            .ok()
            .and_then(|value| value.parse::<usize>().ok())
            .unwrap_or(1200)
            .clamp(1, 20_000);
        Self {
            enabled,
            max_context_tokens,
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct ContextSource {
    pub edge_id: String,
    pub from_node_id: String,
    pub output: Value,
}

pub fn assemble_context_injection(
    target_node_id: &str,
    sources: &[ContextSource],
    config: &ContextAssemblyConfig,
) -> Option<Value> {
    if !config.enabled || sources.is_empty() {
        return None;
    }

    let mut remaining = config.max_context_tokens;
    let mut assembled = Vec::new();
    let mut total_estimated_tokens = 0_usize;
    let mut truncated = false;

    for source in sources {
        if remaining == 0 {
            truncated = true;
            break;
        }
        let output_text = output_to_text(&source.output);
        let estimated_tokens = estimate_tokens(&output_text);
        let include_tokens = estimated_tokens.min(remaining);
        let included_output = if estimated_tokens > include_tokens {
            truncated = true;
            Value::String(truncate_to_tokens(&output_text, include_tokens))
        } else {
            source.output.clone()
        };
        remaining = remaining.saturating_sub(include_tokens);
        total_estimated_tokens += estimated_tokens;

        assembled.push(json!({
            "edge_id": source.edge_id,
            "from_node_id": source.from_node_id,
            "estimated_tokens": estimated_tokens,
            "included_tokens": include_tokens,
            "truncated": estimated_tokens > include_tokens,
            "output": included_output,
        }));
    }

    Some(json!({
        "schema_version": "context_injection.v1",
        "target_node_id": target_node_id,
        "source": "completed_predecessor_node_results",
        "injection_surface": "node_metadata_only",
        "max_context_tokens": config.max_context_tokens,
        "total_estimated_tokens": total_estimated_tokens,
        "included_source_count": assembled.len(),
        "truncated": truncated,
        "sources": assembled,
    }))
}

fn output_to_text(value: &Value) -> String {
    match value {
        Value::String(text) => text.clone(),
        _ => serde_json::to_string(value).unwrap_or_default(),
    }
}

fn estimate_tokens(text: &str) -> usize {
    (text.chars().count() / 4).max(1)
}

fn truncate_to_tokens(text: &str, max_tokens: usize) -> String {
    let max_chars = max_tokens.saturating_mul(4);
    text.chars().take(max_chars).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn assembles_sources_with_budget_metadata() {
        let injection = assemble_context_injection(
            "node-b",
            &[ContextSource {
                edge_id: "edge-a-b".to_string(),
                from_node_id: "node-a".to_string(),
                output: json!("done"),
            }],
            &ContextAssemblyConfig {
                enabled: true,
                max_context_tokens: 10,
            },
        )
        .unwrap();

        assert_eq!(injection["schema_version"], "context_injection.v1");
        assert_eq!(injection["target_node_id"], "node-b");
        assert_eq!(injection["sources"][0]["from_node_id"], "node-a");
        assert_eq!(injection["injection_surface"], "node_metadata_only");
    }

    #[test]
    fn disabled_config_returns_none() {
        let injection = assemble_context_injection(
            "node-b",
            &[ContextSource {
                edge_id: "edge-a-b".to_string(),
                from_node_id: "node-a".to_string(),
                output: json!("done"),
            }],
            &ContextAssemblyConfig {
                enabled: false,
                max_context_tokens: 10,
            },
        );

        assert!(injection.is_none());
    }
}
