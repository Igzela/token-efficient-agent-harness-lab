use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct SamplingCandidate {
    pub candidate_id: String,
    pub output: String,
    pub tier: String,
    pub score: f64,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct SamplingReport {
    pub task_id: String,
    pub candidates: Vec<SamplingCandidate>,
    pub best_candidate_id: String,
    pub best_score: f64,
    pub selection_method: String,
}

pub struct SamplingRunner;

impl Default for SamplingRunner {
    fn default() -> Self {
        Self
    }
}

impl SamplingRunner {
    pub fn new() -> Self {
        Self
    }

    pub fn select_best<'a>(
        &self,
        candidates: &'a [SamplingCandidate],
    ) -> Option<&'a SamplingCandidate> {
        candidates.iter().max_by(|a, b| {
            a.score
                .partial_cmp(&b.score)
                .unwrap_or(std::cmp::Ordering::Equal)
        })
    }

    pub fn build_report(
        &self,
        task_id: &str,
        candidates: Vec<SamplingCandidate>,
    ) -> SamplingReport {
        let (best_id, best_score) = self
            .select_best(&candidates)
            .map(|c| (c.candidate_id.clone(), c.score))
            .unwrap_or_default();
        SamplingReport {
            task_id: task_id.to_string(),
            candidates,
            best_candidate_id: best_id,
            best_score,
            selection_method: "highest_score".to_string(),
        }
    }
}

impl Default for SamplingCandidate {
    fn default() -> Self {
        Self {
            candidate_id: String::new(),
            output: String::new(),
            tier: String::new(),
            score: 0.0,
        }
    }
}
impl Default for SamplingReport {
    fn default() -> Self {
        Self {
            task_id: String::new(),
            candidates: Vec::new(),
            best_candidate_id: String::new(),
            best_score: 0.0,
            selection_method: "highest_score".to_string(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cand(id: &str, score: f64) -> SamplingCandidate {
        SamplingCandidate {
            candidate_id: id.into(),
            output: format!("out {}", id),
            tier: "t".into(),
            score,
        }
    }

    #[test]
    fn select_best() {
        let candidates = vec![cand("a", 0.5), cand("b", 0.9)];
        let c = SamplingRunner::new().select_best(&candidates).unwrap();
        assert_eq!(c.candidate_id, "b");
    }

    #[test]
    fn select_empty() {
        let candidates: Vec<SamplingCandidate> = vec![];
        assert!(SamplingRunner::new().select_best(&candidates).is_none());
    }

    #[test]
    fn report_best() {
        let r = SamplingRunner::new().build_report("t1", vec![cand("a", 0.5), cand("b", 0.9)]);
        assert_eq!(r.best_candidate_id, "b");
    }

    #[test]
    fn report_serializes() {
        let v = serde_json::to_value(SamplingRunner::new().build_report("t1", vec![])).unwrap();
        assert_eq!(v["selection_method"], "highest_score");
    }

    #[test]
    fn candidate_serializes() {
        let v = serde_json::to_value(cand("x", 0.8)).unwrap();
        assert_eq!(v["candidate_id"], "x");
    }
}
