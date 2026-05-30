use serde_json::json;
use serde_json::Value;

pub fn local_boundaries(executor_type: &str, provider_enabled: bool) -> Value {
    let provider_transport = match executor_type {
        "provider" if provider_enabled => "provider/enabled",
        "provider" => "provider/disabled",
        "stub" => "stub",
        _ => "noop",
    };
    json!({
        "provider_transport": provider_transport,
        "target_repository_writes": "disabled",
        "sandbox_process_execution": "disabled",
        "runtime_workers": "disabled",
        "deployment": "local-only",
        "docker_required": false,
    })
}
