use super::rules::{NEGATED_RISK_PHRASES, NEGATION_PREFIXES, PHRASE_FLAGS, RISK_KEYWORDS};
use super::RuleBasedTaskAnalyzer;
use crate::dispatch_decision::Evidence;

impl RuleBasedTaskAnalyzer {
    pub(super) fn detect_risk_flags(
        &self,
        text: &str,
        positive_text: &str,
    ) -> (Vec<&str>, Vec<Evidence>, Vec<Evidence>) {
        let mut flags = Vec::new();
        let mut positive_evidence = Vec::new();
        let mut negative_evidence = Vec::new();

        let mut negated_flags_set: Vec<&str> = Vec::new();
        for phrase in NEGATED_RISK_PHRASES {
            if text.contains(phrase) {
                if let Some(flag_list) = PHRASE_FLAGS.get(phrase) {
                    for flag in flag_list {
                        if !negated_flags_set.contains(flag) {
                            negated_flags_set.push(flag);
                        }
                    }
                }
            }
        }

        for (flag, keywords) in RISK_KEYWORDS {
            let mut detected = false;
            let mut matched_keyword = "";
            let mut matched_index = 0usize;
            for keyword in *keywords {
                if let Some(idx) = positive_text.find(keyword) {
                    let original_idx = text.find(keyword).unwrap_or(idx);
                    if !is_negated_occurrence(text, keyword, original_idx) {
                        detected = true;
                        matched_keyword = keyword;
                        matched_index = idx;
                        break;
                    }
                }
            }

            if detected {
                flags.push(*flag);
                positive_evidence.push(Evidence {
                    feature: flag.to_string(),
                    text: matched_keyword.to_string(),
                    span: [
                        matched_index as i64,
                        (matched_index + matched_keyword.len()) as i64,
                    ],
                    polarity: "positive".to_string(),
                    source: "raw_request".to_string(),
                    rule_id: Some(format!("risk_{}", flag)),
                    confidence: 0.9,
                    negation_scope: None,
                });
            } else if negated_flags_set.contains(flag) {
                negative_evidence.push(Evidence {
                    feature: flag.to_string(),
                    text: "[negated]".to_string(),
                    span: [0, 0],
                    polarity: "negative".to_string(),
                    source: "raw_request".to_string(),
                    rule_id: Some(format!("negation_{}", flag)),
                    confidence: 0.95,
                    negation_scope: Some(format!("negation phrase suppressed {}", flag)),
                });
            }
        }

        (flags, positive_evidence, negative_evidence)
    }
}

pub(super) fn positive_risk_text(text: &str) -> String {
    let mut result = text.to_string();
    for phrase in NEGATED_RISK_PHRASES {
        result = result.replace(phrase, " ");
    }
    result
}

fn is_negated_occurrence(text: &str, _keyword: &str, start: usize) -> bool {
    let clause_start = start.saturating_sub(40);
    let clause = &text[clause_start..start];
    NEGATION_PREFIXES
        .iter()
        .any(|prefix| clause.contains(prefix))
}
