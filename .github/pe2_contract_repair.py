from pathlib import Path

path = Path("engine/src/budget_manager.rs")
text = path.read_text()

replacements = [
    (
        """    pub reason_codes: Vec<String>,
    pub evidence_references: Vec<BudgetEvidenceReference>,
    pub anomaly_kind: Option<BudgetAnomalyKind>,
""",
        """    pub reason_codes: Vec<String>,
    pub evidence_references: Vec<BudgetEvidenceReference>,
    pub detected: bool,
    pub anomaly_kind: Option<BudgetAnomalyKind>,
""",
    ),
    (
        """        match self.outcome {
            BudgetEvidenceOutcome::Supported => {
                let kind = self
                    .anomaly_kind
                    .as_ref()
                    .ok_or_else(|| "supported anomaly requires a kind".to_string())?;
                if self.severity.is_none() || self.measurement.is_none() {
                    return Err("supported anomaly requires severity and measurement".to_string());
                }
                if matches!(kind, BudgetAnomalyKind::CostSpike) && !self.coverage.pricing_complete {
                    return Err("cost anomaly requires complete pricing".to_string());
                }
                validate_measurement(self.measurement.as_ref().expect("checked above"))?;
            }
            BudgetEvidenceOutcome::InsufficientEvidence
            | BudgetEvidenceOutcome::InvalidEvidence => {
                if self.anomaly_kind.is_some()
                    || self.severity.is_some()
                    || self.measurement.is_some()
                {
                    return Err(
                        "unsupported anomaly outcome must not contain a finding".to_string()
                    );
                }
            }
        }
""",
        """        match self.outcome {
            BudgetEvidenceOutcome::Supported if self.detected => {
                let kind = self
                    .anomaly_kind
                    .as_ref()
                    .ok_or_else(|| "detected anomaly requires a kind".to_string())?;
                if self.severity.is_none() || self.measurement.is_none() {
                    return Err("detected anomaly requires severity and measurement".to_string());
                }
                if matches!(kind, BudgetAnomalyKind::CostSpike) && !self.coverage.pricing_complete {
                    return Err("cost anomaly requires complete pricing".to_string());
                }
                validate_measurement(self.measurement.as_ref().expect("checked above"))?;
            }
            BudgetEvidenceOutcome::Supported => {
                if self.anomaly_kind.is_some()
                    || self.severity.is_some()
                    || self.measurement.is_some()
                {
                    return Err(
                        "normal supported evidence must not contain an anomaly finding".to_string(),
                    );
                }
            }
            BudgetEvidenceOutcome::InsufficientEvidence
            | BudgetEvidenceOutcome::InvalidEvidence => {
                if self.detected
                    || self.anomaly_kind.is_some()
                    || self.severity.is_some()
                    || self.measurement.is_some()
                {
                    return Err(
                        "unsupported anomaly outcome must not contain a finding".to_string(),
                    );
                }
            }
        }
""",
    ),
    (
        """            reason_codes: vec!["anomaly.token_spike".to_string()],
            evidence_references: vec![reference()],
            anomaly_kind: Some(BudgetAnomalyKind::TokenSpike),
""",
        """            reason_codes: vec!["anomaly.token_spike".to_string()],
            evidence_references: vec![reference()],
            detected: true,
            anomaly_kind: Some(BudgetAnomalyKind::TokenSpike),
""",
    ),
]

for before, after in replacements:
    if before not in text:
        raise SystemExit(f"missing expected repair anchor: {before.splitlines()[0]}")
    text = text.replace(before, after, 1)

marker = """    #[test]
    fn malformed_and_noncanonical_fields_are_rejected() {
"""
test = """    #[test]
    fn normal_supported_evidence_is_explicit_and_contains_no_finding() {
        let mut finding = BudgetAnomalyFinding {
            schema_version: BUDGET_ANOMALY_FINDING_SCHEMA_VERSION.to_string(),
            finding_id: "finding-normal-1".to_string(),
            scope: scope(),
            outcome: BudgetEvidenceOutcome::Supported,
            window: window(8),
            coverage: coverage(true),
            confidence: confidence(),
            reason_codes: vec!["anomaly.none".to_string()],
            evidence_references: vec![reference()],
            detected: false,
            anomaly_kind: None,
            severity: None,
            measurement: None,
            evidence_sha256: String::new(),
        };
        finding.seal().unwrap();
        finding.validate().unwrap();

        finding.anomaly_kind = Some(BudgetAnomalyKind::TokenSpike);
        finding.seal().unwrap();
        assert_eq!(
            finding.validate().unwrap_err(),
            "normal supported evidence must not contain an anomaly finding"
        );
    }

"""
if marker not in text:
    raise SystemExit("missing normal-state test anchor")
path.write_text(text.replace(marker, test + marker, 1))
