use std::collections::HashMap;
use std::sync::Arc;

use crate::dispatch_decision::DispatchDecision;
use crate::executor_adapter::{ExecutionResult, Executor};
use crate::provider::executor::ProviderExecutor;
use crate::provider::Provider;
use crate::runtime::FixtureRuntime;

pub struct HybridExecutor {
    provider_executor: Option<ProviderExecutor>,
    cli_executors: HashMap<String, Box<dyn Executor>>,
    default_executor: Box<dyn Executor>,
    complexity_threshold: f64,
}

impl HybridExecutor {
    pub fn new(
        provider: Option<Arc<dyn Provider>>,
        cli_executors: HashMap<String, Box<dyn Executor>>,
        default_executor: Box<dyn Executor>,
        complexity_threshold: f64,
    ) -> Self {
        let provider_executor = provider.map(ProviderExecutor::new);
        Self {
            provider_executor,
            cli_executors,
            default_executor,
            complexity_threshold,
        }
    }

    fn has_constraint(decision: &DispatchDecision, constraint: &str) -> bool {
        decision.hard_constraints.iter().any(|c| c == constraint)
    }

    fn is_cli_tier(tier: &str) -> bool {
        tier == "claude_code_cli" || tier == "codex_cli"
    }

    fn select_executor<'a>(&'a self, decision: &DispatchDecision) -> (&'a dyn Executor, String) {
        let tier = &decision.selected_tier;
        let complexity = decision
            .analysis_snapshot
            .get("complexity_score")
            .and_then(|v| v.as_f64())
            .unwrap_or(0.0);

        if Self::is_cli_tier(tier) {
            if let Some(exec) = self.cli_executors.get(tier) {
                return (exec.as_ref(), tier.clone());
            }
        }

        if complexity < self.complexity_threshold
            && !Self::has_constraint(decision, "no_provider_call")
        {
            if let Some(ref pe) = self.provider_executor {
                return (pe, "provider".to_string());
            }
        }

        if complexity >= self.complexity_threshold {
            if let Some(exec) = self.cli_executors.get("claude_code_cli") {
                return (exec.as_ref(), "claude_code_cli".to_string());
            }
            if let Some(exec) = self.cli_executors.get("codex_cli") {
                return (exec.as_ref(), "codex_cli".to_string());
            }
        }

        if !Self::has_constraint(decision, "no_provider_call") {
            if let Some(ref pe) = self.provider_executor {
                return (pe, "provider".to_string());
            }
        }

        (self.default_executor.as_ref(), "noop".to_string())
    }
}

impl Executor for HybridExecutor {
    fn execute(
        &self,
        decision: &DispatchDecision,
        raw_request: &str,
        dispatch_id: &str,
        runtime: &mut FixtureRuntime,
    ) -> ExecutionResult {
        let (executor, _selected_type) = self.select_executor(decision);
        executor.execute(decision, raw_request, dispatch_id, runtime)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::executor_adapter::NoopExecutor;
    use crate::runtime::FixtureRuntime;
    use serde_json::json;

    fn make_decision(tier: &str, complexity: Option<f64>) -> DispatchDecision {
        make_decision_with_constraints(tier, complexity, Vec::new())
    }

    fn make_decision_with_constraints(
        tier: &str,
        complexity: Option<f64>,
        hard_constraints: Vec<String>,
    ) -> DispatchDecision {
        let mut snap = serde_json::Map::new();
        if let Some(c) = complexity {
            snap.insert("complexity_score".to_string(), json!(c));
        }
        DispatchDecision {
            selected_tier: tier.to_string(),
            analysis_snapshot: serde_json::Value::Object(snap),
            hard_constraints,
            ..DispatchDecision::default()
        }
    }

    #[test]
    fn cli_tier_routes_to_cli_executor() {
        let mut cli: HashMap<String, Box<dyn Executor>> = HashMap::new();
        cli.insert("claude_code_cli".to_string(), Box::new(NoopExecutor));
        let exec = HybridExecutor::new(None, cli, Box::new(NoopExecutor), 0.5);

        let decision = make_decision("claude_code_cli", None);
        let (e, label) = exec.select_executor(&decision);
        assert_eq!(label, "claude_code_cli");
        // Verify it's the CLI executor (not default) by running it
        let mut rt = FixtureRuntime::new();
        let result = e.execute(&decision, "test", "d1", &mut rt);
        assert_eq!(result.executor_type, "noop");
    }

    #[test]
    fn low_complexity_prefers_provider() {
        let cli: HashMap<String, Box<dyn Executor>> = HashMap::new();
        use crate::provider::DisabledProvider;
        let provider: Arc<dyn Provider> = Arc::new(DisabledProvider::new("test"));
        let exec = HybridExecutor::new(Some(provider), cli, Box::new(NoopExecutor), 0.5);

        let decision = make_decision("openai", Some(0.2));
        let (_, label) = exec.select_executor(&decision);
        assert_eq!(label, "provider");
    }

    #[test]
    fn high_complexity_prefers_cli() {
        let mut cli: HashMap<String, Box<dyn Executor>> = HashMap::new();
        cli.insert("claude_code_cli".to_string(), Box::new(NoopExecutor));
        use crate::provider::DisabledProvider;
        let provider: Arc<dyn Provider> = Arc::new(DisabledProvider::new("test"));
        let exec = HybridExecutor::new(Some(provider), cli, Box::new(NoopExecutor), 0.5);

        let decision = make_decision("openai", Some(0.8));
        let (_, label) = exec.select_executor(&decision);
        assert_eq!(label, "claude_code_cli");
    }

    #[test]
    fn no_provider_no_cli_falls_back_to_default() {
        let cli: HashMap<String, Box<dyn Executor>> = HashMap::new();
        let exec = HybridExecutor::new(None, cli, Box::new(NoopExecutor), 0.5);

        let decision = make_decision("openai", Some(0.8));
        let (_, label) = exec.select_executor(&decision);
        assert_eq!(label, "noop");
    }

    #[test]
    fn cli_requested_but_missing_falls_through() {
        let cli: HashMap<String, Box<dyn Executor>> = HashMap::new();
        use crate::provider::DisabledProvider;
        let provider: Arc<dyn Provider> = Arc::new(DisabledProvider::new("test"));
        let exec = HybridExecutor::new(Some(provider), cli, Box::new(NoopExecutor), 0.5);

        let decision = make_decision("claude_code_cli", None);
        let (_, label) = exec.select_executor(&decision);
        assert_eq!(label, "provider");
    }

    #[test]
    fn no_provider_call_constraint_skips_provider_low_complexity() {
        let cli: HashMap<String, Box<dyn Executor>> = HashMap::new();
        use crate::provider::DisabledProvider;
        let provider: Arc<dyn Provider> = Arc::new(DisabledProvider::new("test"));
        let exec = HybridExecutor::new(Some(provider), cli, Box::new(NoopExecutor), 0.5);

        let decision = make_decision_with_constraints(
            "openai",
            Some(0.2),
            vec!["no_provider_call".to_string()],
        );
        let (_, label) = exec.select_executor(&decision);
        assert_eq!(label, "noop");
    }

    #[test]
    fn no_provider_call_constraint_skips_provider_fallback() {
        let cli: HashMap<String, Box<dyn Executor>> = HashMap::new();
        use crate::provider::DisabledProvider;
        let provider: Arc<dyn Provider> = Arc::new(DisabledProvider::new("test"));
        let exec = HybridExecutor::new(Some(provider), cli, Box::new(NoopExecutor), 0.5);

        // High complexity with no CLI executors → falls through to provider fallback
        let decision = make_decision_with_constraints(
            "openai",
            Some(0.8),
            vec!["no_provider_call".to_string()],
        );
        let (_, label) = exec.select_executor(&decision);
        assert_eq!(label, "noop");
    }

    #[test]
    fn no_provider_call_prevents_provider_routing() {
        let cli: HashMap<String, Box<dyn Executor>> = HashMap::new();
        use crate::provider::DisabledProvider;
        let provider: Arc<dyn Provider> = Arc::new(DisabledProvider::new("test"));
        let exec = HybridExecutor::new(Some(provider), cli, Box::new(NoopExecutor), 0.5);

        let mut decision = make_decision("openai", Some(0.2)); // low complexity -> would prefer provider
        decision.hard_constraints = vec!["no_provider_call".to_string()];
        let (_, label) = exec.select_executor(&decision);
        assert_ne!(
            label, "provider",
            "should not route to provider when no_provider_call constraint exists"
        );
        assert_eq!(label, "noop"); // falls through to default
    }

    #[test]
    fn auto_mode_missing_cli_does_not_claim_cli() {
        let cli: HashMap<String, Box<dyn Executor>> = HashMap::new(); // empty — no CLI executors
        use crate::provider::DisabledProvider;
        let provider: Arc<dyn Provider> = Arc::new(DisabledProvider::new("test"));
        let exec = HybridExecutor::new(Some(provider), cli, Box::new(NoopExecutor), 0.5);

        let decision = make_decision("claude_code_cli", None); // CLI requested but not available
        let (_, label) = exec.select_executor(&decision);
        // Should NOT claim CLI execution — should fall through to provider or noop
        assert_ne!(
            label, "claude_code_cli",
            "should not claim CLI when no CLI executor is registered"
        );
    }
}
