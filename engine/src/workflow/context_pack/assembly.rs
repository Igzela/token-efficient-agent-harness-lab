use serde_json::{json, Value};

use crate::context_working_set::{
    compose_runtime_prompt, cws_benchmark_preflight, cws_benchmark_run, partition_working_set,
    project as project_working_set, project_repository_session, reduce_tool_result, CachePartition,
    CacheTelemetryObservation, CwsBenchmarkPreflight, CwsBenchmarkRunReport, ProjectedWorkingSet,
    ProjectorBounds, ProjectorError, ReducedToolResult, RepositorySessionMode, SourceItem,
    ToolResultAdmission,
};

use super::budget::allocate_context_budget;

/// Existing context-pack owner delegates derived projection; it does not move
/// source authority into the working-set module.
pub fn project_authorized_working_set(
    items: &[SourceItem],
    bounds: ProjectorBounds,
) -> Result<ProjectedWorkingSet, ProjectorError> {
    project_working_set(items, bounds)
}

pub fn reduce_authorized_tool_result(
    admission: &ToolResultAdmission,
) -> Result<ReducedToolResult, ProjectorError> {
    reduce_tool_result(admission)
}

pub fn project_authorized_repository_session(
    accepted_main_sha: &str,
    head_sha: &str,
    packet_id: &str,
    mode: RepositorySessionMode,
    docs: &[SourceItem],
    bounds: ProjectorBounds,
) -> Result<ProjectedWorkingSet, ProjectorError> {
    project_repository_session(accepted_main_sha, head_sha, packet_id, mode, docs, bounds)
}

pub fn compose_authorized_runtime_prompt(
    task_binding: &str,
    projected: &ProjectedWorkingSet,
    user_prompt: &str,
) -> Result<String, ProjectorError> {
    compose_runtime_prompt(task_binding, projected, user_prompt)
}

pub fn partition_authorized_working_set(
    projected: &ProjectedWorkingSet,
    telemetry: Option<CacheTelemetryObservation>,
) -> Result<CachePartition, ProjectorError> {
    partition_working_set(projected, telemetry)
}

pub fn authorized_cws_benchmark_preflight(
    head_sha: &str,
    provider_capability_known: bool,
    evidence_paths_bound: bool,
) -> Result<CwsBenchmarkPreflight, ProjectorError> {
    cws_benchmark_preflight(head_sha, provider_capability_known, evidence_paths_bound)
}

pub fn authorized_cws_benchmark_run(
    head_sha: &str,
    provider_capability_known: bool,
    evidence_paths_bound: bool,
    authorization_issued: bool,
    provider_credential_present: bool,
) -> Result<CwsBenchmarkRunReport, ProjectorError> {
    cws_benchmark_run(
        head_sha,
        provider_capability_known,
        evidence_paths_bound,
        authorization_issued,
        provider_credential_present,
    )
}

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

pub fn assemble_context_injection_with_bridge(
    target_node_id: &str,
    sources: &[ContextSource],
    field_mappings: &[Option<Value>],
    config: &ContextAssemblyConfig,
) -> Option<Value> {
    if !config.enabled || sources.is_empty() {
        return None;
    }

    let allocations = allocate_context_budget(sources, config.max_context_tokens);
    let mut assembled = Vec::new();
    let mut total_estimated_tokens = 0_usize;
    let mut truncated = false;

    for (i, source) in sources.iter().enumerate() {
        let mapping = field_mappings.get(i).and_then(|m| m.as_ref());
        let (bridged_output, mapping_decisions) = bridge_context_fields(&source.output, mapping);
        let output_text = output_to_text(&bridged_output);
        let estimated_tokens = estimate_tokens(&output_text);
        let (_, allocated_tokens, was_truncated) = &allocations[i];
        let include_tokens = *allocated_tokens;
        let included_output = if estimated_tokens > include_tokens {
            truncated = true;
            Value::String(truncate_to_tokens(&output_text, include_tokens))
        } else {
            bridged_output
        };
        if *was_truncated {
            truncated = true;
        }
        total_estimated_tokens += estimated_tokens;

        assembled.push(json!({
            "edge_id": source.edge_id,
            "from_node_id": source.from_node_id,
            "estimated_tokens": estimated_tokens,
            "included_tokens": include_tokens,
            "truncated": *was_truncated,
            "mapping_decisions": mapping_decisions,
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

pub fn bridge_context_fields(
    output: &Value,
    field_mapping: Option<&Value>,
) -> (Value, Vec<String>) {
    let mapping = match field_mapping {
        Some(m) if m.is_object() => m,
        _ => return (output.clone(), vec!["default_passthrough".to_string()]),
    };

    let source = if output.is_string() {
        let mut m = serde_json::Map::new();
        m.insert("value".to_string(), output.clone());
        Value::Object(m)
    } else if output.is_object() {
        output.clone()
    } else {
        return (output.clone(), vec!["default_passthrough".to_string()]);
    };

    let source_obj = source.as_object().unwrap();
    let mapping_obj = mapping.as_object().unwrap();
    let mut result = serde_json::Map::new();
    let mut decisions = Vec::new();

    for (src_key, dest_key) in mapping_obj {
        let dest_key = match dest_key.as_str() {
            Some(s) => s.to_string(),
            None => continue,
        };
        if let Some(val) = source_obj.get(src_key) {
            result.insert(dest_key.clone(), val.clone());
            decisions.push(format!("mapped {} -> {}", src_key, dest_key));
        }
    }

    (Value::Object(result), decisions)
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

    #[test]
    fn bridge_maps_output_fields() {
        let output = json!({"a": 1, "b": 2});
        let mapping = json!({"a": "x"});
        let (result, decisions) = bridge_context_fields(&output, Some(&mapping));
        assert_eq!(result.get("x").unwrap(), &json!(1));
        assert!(result.get("b").is_none());
        assert_eq!(decisions, vec!["mapped a -> x"]);
    }

    #[test]
    fn bridge_no_mapping_passthrough() {
        let output = json!({"a": 1});
        let (result, decisions) = bridge_context_fields(&output, None);
        assert_eq!(result, output);
        assert_eq!(decisions, vec!["default_passthrough"]);
    }

    #[test]
    fn bridge_string_output_with_mapping() {
        let output = json!("hello");
        let mapping = json!({"value": "text"});
        let (result, decisions) = bridge_context_fields(&output, Some(&mapping));
        assert_eq!(result.get("text").unwrap(), &json!("hello"));
        assert_eq!(decisions, vec!["mapped value -> text"]);
    }

    #[test]
    fn bridge_invalid_mapping_passthrough() {
        let output = json!({"a": 1});
        let mapping = Value::Bool(true);
        let (result, decisions) = bridge_context_fields(&output, Some(&mapping));
        assert_eq!(result, output);
        assert_eq!(decisions, vec!["default_passthrough"]);
    }

    #[test]
    fn bridge_variant_applies_mapping() {
        let injection = assemble_context_injection_with_bridge(
            "node-b",
            &[ContextSource {
                edge_id: "edge-a-b".to_string(),
                from_node_id: "node-a".to_string(),
                output: json!({"x": 10, "y": 20}),
            }],
            &[Some(json!({"x": "alpha"}))],
            &ContextAssemblyConfig {
                enabled: true,
                max_context_tokens: 200,
            },
        )
        .unwrap();

        let src = &injection["sources"][0];
        assert_eq!(src["output"]["alpha"], json!(10));
        assert!(src["output"]["y"].is_null());
        assert_eq!(src["mapping_decisions"][0], "mapped x -> alpha");
    }

    #[test]
    fn bridge_variant_passthrough_when_no_mapping() {
        let injection = assemble_context_injection_with_bridge(
            "node-b",
            &[ContextSource {
                edge_id: "edge-a-b".to_string(),
                from_node_id: "node-a".to_string(),
                output: json!({"x": 10}),
            }],
            &[None],
            &ContextAssemblyConfig {
                enabled: true,
                max_context_tokens: 200,
            },
        )
        .unwrap();

        let src = &injection["sources"][0];
        assert_eq!(src["output"], json!({"x": 10}));
        assert_eq!(src["mapping_decisions"][0], "default_passthrough");
    }

    #[test]
    fn bridge_variant_disabled_returns_none() {
        let injection = assemble_context_injection_with_bridge(
            "node-b",
            &[ContextSource {
                edge_id: "edge-a-b".to_string(),
                from_node_id: "node-a".to_string(),
                output: json!("done"),
            }],
            &[None],
            &ContextAssemblyConfig {
                enabled: false,
                max_context_tokens: 200,
            },
        );
        assert!(injection.is_none());
    }

    #[test]
    fn bridge_variant_empty_sources_returns_none() {
        let injection = assemble_context_injection_with_bridge(
            "node-b",
            &[],
            &[],
            &ContextAssemblyConfig {
                enabled: true,
                max_context_tokens: 200,
            },
        );
        assert!(injection.is_none());
    }
}
