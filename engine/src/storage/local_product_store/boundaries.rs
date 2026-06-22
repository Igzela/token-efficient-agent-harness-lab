use serde_json::json;
use serde_json::Value;

pub fn local_boundaries(executor_type: &str, provider_enabled: bool) -> Value {
    let provider_transport = match (executor_type, provider_enabled) {
        (_, true) => "provider/enabled",
        ("provider", false) => "provider/disabled",
        ("stub", false) => "stub",
        _ => "noop",
    };
    json!({
        "provider_transport": provider_transport,
        "target_repository_writes": "disabled",
        "sandbox_process_execution": "disabled",
        "runtime_workers": "env_gated_supervised",
        "deployment": "local-only",
        "docker_required": false,
    })
}

#[cfg(test)]
mod tests {
    use super::local_boundaries;

    #[test]
    fn adaptive_provider_availability_is_visible_for_multi_executor() {
        assert_eq!(
            local_boundaries("multi", true)["provider_transport"],
            "provider/enabled"
        );
    }
}
