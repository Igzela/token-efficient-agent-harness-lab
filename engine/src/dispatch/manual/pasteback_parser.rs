use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use uuid::Uuid;

pub const PASTEBACK_SUBMISSION_SCHEMA_VERSION: &str = "pasteback_submission.v1";
pub const MAX_OUTPUT_LENGTH: usize = 100_000;
pub const ESTIMATED_CHARS_PER_TOKEN: usize = 4;
pub const DEFAULT_COST_PER_1K_TOKENS: f64 = 0.002;

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct PastebackSubmission {
    pub schema_version: String,
    pub submission_id: String,
    pub dispatch_id: String,
    pub submitted_by: String,
    pub model_used: Option<String>,
    pub provider_used: Option<String>,
    pub raw_output: String,
    pub output_hash: String,
    pub claimed_input_tokens: Option<i64>,
    pub claimed_output_tokens: Option<i64>,
    pub claimed_cost: Option<f64>,
    pub submitted_at: String,
}

pub struct PastebackParser;

impl Default for PastebackParser {
    fn default() -> Self {
        Self
    }
}

impl PastebackParser {
    pub fn new() -> Self {
        Self
    }

    pub fn parse(
        &self,
        dispatch_id: &str,
        raw_output: &str,
        submitted_by: &str,
        model_used: Option<&str>,
        provider_used: Option<&str>,
        claimed_input_tokens: Option<i64>,
        claimed_output_tokens: Option<i64>,
        claimed_cost: Option<f64>,
    ) -> Result<PastebackSubmission, String> {
        let trimmed = raw_output.trim();
        if trimmed.is_empty() {
            return Err("Pasteback output cannot be empty".to_string());
        }
        if trimmed.len() > MAX_OUTPUT_LENGTH {
            return Err(format!(
                "Output exceeds max length ({} > {})",
                trimmed.len(),
                MAX_OUTPUT_LENGTH
            ));
        }
        let output_hash = {
            let mut h = Sha256::new();
            h.update(trimmed.as_bytes());
            hex::encode(&h.finalize()[..8])
        };
        Ok(PastebackSubmission {
            schema_version: PASTEBACK_SUBMISSION_SCHEMA_VERSION.to_string(),
            submission_id: format!("pb-{}", &Uuid::new_v4().to_string().replace('-', "")[..12]),
            dispatch_id: dispatch_id.to_string(),
            submitted_by: submitted_by.to_string(),
            model_used: model_used.map(|s| s.to_string()),
            provider_used: provider_used.map(|s| s.to_string()),
            raw_output: trimmed.to_string(),
            output_hash,
            claimed_input_tokens,
            claimed_output_tokens,
            claimed_cost,
            submitted_at: chrono::Utc::now().to_rfc3339(),
        })
    }

    pub fn estimate_tokens(&self, text: &str) -> i64 {
        std::cmp::max(1, (text.len() / ESTIMATED_CHARS_PER_TOKEN) as i64)
    }

    pub fn estimate_cost(&self, input_tokens: i64, output_tokens: i64, cost_per_1k: f64) -> f64 {
        ((input_tokens + output_tokens) as f64 / 1000.0 * cost_per_1k * 1_000_000.0).round()
            / 1_000_000.0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_valid() {
        let p = PastebackParser::new();
        let s = p
            .parse("d1", "Hello world", "human", None, None, None, None, None)
            .unwrap();
        assert_eq!(s.dispatch_id, "d1");
        assert!(s.submission_id.starts_with("pb-"));
    }

    #[test]
    fn parse_empty_rejected() {
        assert!(PastebackParser::new()
            .parse("d1", "", "human", None, None, None, None, None)
            .is_err());
    }

    #[test]
    fn estimate_tokens() {
        assert_eq!(PastebackParser::new().estimate_tokens("hello"), 1);
    }

    #[test]
    fn estimate_cost() {
        let cost = PastebackParser::new().estimate_cost(1000, 500, 0.002);
        assert!((cost - 0.003).abs() < 0.0001);
    }

    #[test]
    fn to_value_roundtrip() {
        let p = PastebackParser::new();
        let s = p
            .parse(
                "d1",
                "out",
                "h",
                Some("g"),
                Some("o"),
                Some(1),
                Some(2),
                Some(0.1),
            )
            .unwrap();
        let v = serde_json::to_value(&s).unwrap();
        let d: PastebackSubmission = serde_json::from_value(v).unwrap();
        assert_eq!(d, s);
    }
}
