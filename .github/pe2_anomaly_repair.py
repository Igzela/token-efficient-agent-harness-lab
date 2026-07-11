from pathlib import Path

path = Path("engine/src/budget_anomaly.rs")
text = path.read_text()

before = '''    let (baseline, baseline_duplicates) = deduplicate(request, baseline)?;
    let (current, current_duplicates) = deduplicate(request, current)?;
'''
after = '''    let (baseline, baseline_duplicates) = match deduplicate(baseline) {
        Ok(value) => value,
        Err(()) => {
            return make_finding(
                request,
                BudgetEvidenceOutcome::InvalidEvidence,
                false,
                0,
                freshness_seconds,
                0,
                vec!["invalid_evidence.conflicting_duplicate".to_string()],
                vec![],
                vec![],
                false,
                None,
            );
        }
    };
    let (current, current_duplicates) = match deduplicate(current) {
        Ok(value) => value,
        Err(()) => {
            return make_finding(
                request,
                BudgetEvidenceOutcome::InvalidEvidence,
                false,
                baseline.len() as u32,
                freshness_seconds,
                baseline_duplicates,
                vec!["invalid_evidence.conflicting_duplicate".to_string()],
                evidence_references(baseline.iter()),
                observed_dimensions(&baseline),
                false,
                None,
            );
        }
    };
'''
if before not in text:
    raise SystemExit("missing deduplicate call anchor")
text = text.replace(before, after, 1)

before = '''fn deduplicate(
    request: &BudgetAnomalyRequest,
    mut observations: Vec<BudgetAnomalyObservation>,
) -> Result<(Vec<BudgetAnomalyObservation>, u32), String> {
'''
after = '''fn deduplicate(
    mut observations: Vec<BudgetAnomalyObservation>,
) -> Result<(Vec<BudgetAnomalyObservation>, u32), ()> {
'''
if before not in text:
    raise SystemExit("missing deduplicate signature anchor")
text = text.replace(before, after, 1)

before = '''            Some(_) => {
                return Err(format!(
                    "{}: conflicting duplicate evidence",
                    request.finding_id
                ));
            }
'''
after = '''            Some(_) => return Err(()),
'''
if before not in text:
    raise SystemExit("missing duplicate conflict anchor")
text = text.replace(before, after, 1)

before = '''        assert!(detect_budget_anomaly(&request, &observations)
            .unwrap_err()
            .contains("conflicting duplicate evidence"));
'''
after = '''        let finding = detect_budget_anomaly(&request, &observations).unwrap();
        assert_eq!(finding.outcome, BudgetEvidenceOutcome::InvalidEvidence);
        assert_eq!(
            finding.reason_codes,
            vec!["invalid_evidence.conflicting_duplicate"]
        );
        assert!(!finding.detected);
'''
if before not in text:
    raise SystemExit("missing conflicting duplicate test anchor")
text = text.replace(before, after, 1)

path.write_text(text)
