use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct AdvisorContextPack {
    pub task_description: String,
    pub context: String,
    pub constraints: Vec<String>,
    pub budget_tokens: i64,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct AdvisorResponse {
    pub recommendation: String,
    pub confidence: f64,
    pub reasoning: String,
    pub alternatives: Vec<String>,
}

pub trait AdvisorProvider: Send + Sync {
    fn advise(&self, context: &AdvisorContextPack) -> AdvisorResponse;
}

pub struct StubAdvisorProvider;
impl Default for StubAdvisorProvider {
    fn default() -> Self {
        Self
    }
}
impl StubAdvisorProvider {
    pub fn new() -> Self {
        Self
    }
}

impl AdvisorProvider for StubAdvisorProvider {
    fn advise(&self, ctx: &AdvisorContextPack) -> AdvisorResponse {
        AdvisorResponse {
            recommendation: format!("process: {}", ctx.task_description),
            confidence: 0.8,
            reasoning: "stub".into(),
            alternatives: vec!["alt_a".into(), "alt_b".into()],
        }
    }
}

pub struct AdvisorBroker {
    provider: Box<dyn AdvisorProvider>,
}

impl AdvisorBroker {
    pub fn new(provider: Box<dyn AdvisorProvider>) -> Self {
        Self { provider }
    }
    pub fn request_advice(&self, ctx: &AdvisorContextPack) -> AdvisorResponse {
        self.provider.advise(ctx)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ctx() -> AdvisorContextPack {
        AdvisorContextPack {
            task_description: "do thing".into(),
            context: "ctx".into(),
            constraints: vec![],
            budget_tokens: 4000,
        }
    }

    #[test]
    fn stub_responds() {
        let r = StubAdvisorProvider::new().advise(&ctx());
        assert!(r.recommendation.contains("do thing"));
        assert_eq!(r.confidence, 0.8);
    }

    #[test]
    fn stub_alternatives() {
        assert_eq!(
            StubAdvisorProvider::new().advise(&ctx()).alternatives.len(),
            2
        );
    }

    #[test]
    fn broker_delegates() {
        let r = AdvisorBroker::new(Box::new(StubAdvisorProvider::new())).request_advice(&ctx());
        assert!(!r.reasoning.is_empty());
    }

    #[test]
    fn context_serializes() {
        let v = serde_json::to_value(&ctx()).unwrap();
        assert_eq!(v["budget_tokens"], 4000);
    }

    #[test]
    fn response_serializes() {
        let v = serde_json::to_value(&StubAdvisorProvider::new().advise(&ctx())).unwrap();
        assert_eq!(v["confidence"], 0.8);
    }
}
