use serde_json::json;

pub struct ManualUsageBridge;

impl Default for ManualUsageBridge {
    fn default() -> Self {
        Self
    }
}

impl ManualUsageBridge {
    pub fn new() -> Self {
        Self
    }

    pub fn build_usage_row(
        &self,
        dispatch_id: &str,
        submission_id: &str,
        input_tokens: i64,
        output_tokens: i64,
        estimated_cost: f64,
        model: &str,
        provider: &str,
        passed: bool,
    ) -> serde_json::Value {
        json!({
            "schema_version": "usage_ledger_row.v1",
            "dispatch_id": dispatch_id,
            "submission_id": submission_id,
            "input_tokens": input_tokens,
            "output_tokens": output_tokens,
            "estimated_cost": estimated_cost,
            "model": model,
            "provider": provider,
            "pass": passed,
            "source": "manual_pasteback",
            "created_at": chrono::Utc::now().to_rfc3339(),
        })
    }

    pub fn build_from_pasteback(
        &self,
        pasteback: &serde_json::Value,
        eval_result: &serde_json::Value,
    ) -> serde_json::Value {
        let dispatch_id = pasteback
            .get("dispatch_id")
            .and_then(|v| v.as_str())
            .unwrap_or("");
        let submission_id = pasteback
            .get("submission_id")
            .and_then(|v| v.as_str())
            .unwrap_or("");
        let input_tokens = pasteback
            .get("claimed_input_tokens")
            .and_then(|v| v.as_i64())
            .unwrap_or(0);
        let output_tokens = pasteback
            .get("claimed_output_tokens")
            .and_then(|v| v.as_i64())
            .unwrap_or(0);
        let cost = pasteback
            .get("claimed_cost")
            .and_then(|v| v.as_f64())
            .unwrap_or(0.0);
        let model = pasteback
            .get("model_used")
            .and_then(|v| v.as_str())
            .unwrap_or("unknown");
        let provider = pasteback
            .get("provider_used")
            .and_then(|v| v.as_str())
            .unwrap_or("unknown");
        let passed = eval_result
            .get("overall_passed")
            .and_then(|v| v.as_bool())
            .unwrap_or(false);
        self.build_usage_row(
            dispatch_id,
            submission_id,
            input_tokens,
            output_tokens,
            cost,
            model,
            provider,
            passed,
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn build_usage_row() {
        let bridge = ManualUsageBridge::new();
        let row = bridge.build_usage_row("d1", "s1", 100, 50, 0.01, "gpt-4", "openai", true);
        assert_eq!(row["dispatch_id"], "d1");
        assert_eq!(row["pass"], true);
        assert_eq!(row["source"], "manual_pasteback");
    }

    #[test]
    fn build_from_pasteback() {
        let pb = json!({"dispatch_id": "d1", "submission_id": "s1", "claimed_input_tokens": 10, "claimed_output_tokens": 5, "claimed_cost": 0.001, "model_used": "m", "provider_used": "p"});
        let eval = json!({"overall_passed": true});
        let row = ManualUsageBridge::new().build_from_pasteback(&pb, &eval);
        assert_eq!(row["pass"], true);
        assert_eq!(row["input_tokens"], 10);
    }

    #[test]
    fn build_from_pasteback_defaults() {
        let pb = json!({});
        let eval = json!({});
        let row = ManualUsageBridge::new().build_from_pasteback(&pb, &eval);
        assert_eq!(row["pass"], false);
        assert_eq!(row["input_tokens"], 0);
    }

    #[test]
    fn row_has_schema_version() {
        let row = ManualUsageBridge::new().build_usage_row("d", "s", 1, 1, 0.0, "m", "p", false);
        assert_eq!(row["schema_version"], "usage_ledger_row.v1");
    }

    #[test]
    fn row_has_created_at() {
        let row = ManualUsageBridge::new().build_usage_row("d", "s", 1, 1, 0.0, "m", "p", false);
        assert!(row.get("created_at").is_some());
    }
}
