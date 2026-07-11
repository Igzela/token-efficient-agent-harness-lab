from pathlib import Path

path = Path("engine/src/budget_anomaly.rs")
text = path.read_text()

replacements = []

replacements.append((
'''    for observation in observations {
        let occurred = match parse_timestamp("observation.occurred_at", &observation.occurred_at) {
            Ok(value) => value,
            Err(_) => {
                return make_finding(
                    request,
                    BudgetEvidenceOutcome::InvalidEvidence,
                    false,
                    0,
                    freshness_seconds,
                    0,
                    vec!["invalid_evidence.timestamp".to_string()],
                    vec![],
                    vec![],
                    false,
                    None,
                );
            }
        };
        if !scope_matches(&request.scope, observation) {
            continue;
        }
''',
'''    for observation in observations {
        if !scope_matches(&request.scope, observation) {
            continue;
        }
        let occurred = match parse_timestamp("observation.occurred_at", &observation.occurred_at) {
            Ok(value) => value,
            Err(_) => {
                return make_finding(
                    request,
                    BudgetEvidenceOutcome::InvalidEvidence,
                    false,
                    1,
                    freshness_seconds,
                    0,
                    vec!["invalid_evidence.timestamp".to_string()],
                    evidence_references(std::iter::once(observation)),
                    observed_dimensions(std::slice::from_ref(observation)),
                    observation.cost_usd.is_some(),
                    None,
                );
            }
        };
'''))

replacements.append((
'''    let (baseline, baseline_duplicates) = match deduplicate(baseline) {
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
''',
'''    let baseline_sample_count = baseline.len() as u32;
    let baseline_references = evidence_references(baseline.iter());
    let baseline_observed_dimensions = observed_dimensions(&baseline);
    let baseline_pricing_complete =
        !baseline.is_empty() && baseline.iter().all(|observation| observation.cost_usd.is_some());
    let (baseline, baseline_duplicates) = match deduplicate(baseline) {
        Ok(value) => value,
        Err(()) => {
            return make_finding(
                request,
                BudgetEvidenceOutcome::InvalidEvidence,
                false,
                baseline_sample_count,
                freshness_seconds,
                0,
                vec!["invalid_evidence.conflicting_duplicate".to_string()],
                baseline_references,
                baseline_observed_dimensions,
                baseline_pricing_complete,
                None,
            );
        }
    };

    let mut pre_dedup_combined = baseline
        .iter()
        .chain(current.iter())
        .cloned()
        .collect::<Vec<_>>();
    pre_dedup_combined
        .sort_by(|left, right| observation_key(left).cmp(&observation_key(right)));
    let current_sample_count = pre_dedup_combined.len() as u32;
    let current_references = evidence_references(pre_dedup_combined.iter());
    let current_observed_dimensions = observed_dimensions(&pre_dedup_combined);
    let current_pricing_complete = !pre_dedup_combined.is_empty()
        && pre_dedup_combined
            .iter()
            .all(|observation| observation.cost_usd.is_some());
    let (current, current_duplicates) = match deduplicate(current) {
        Ok(value) => value,
        Err(()) => {
            return make_finding(
                request,
                BudgetEvidenceOutcome::InvalidEvidence,
                false,
                current_sample_count,
                freshness_seconds,
                baseline_duplicates,
                vec!["invalid_evidence.conflicting_duplicate".to_string()],
                current_references,
                current_observed_dimensions,
                current_pricing_complete,
                None,
            );
        }
    };
'''))

replacements.append((
'''    if let Some(reason) = invalid_observation_reason(&combined) {
        return make_finding(
            request,
            BudgetEvidenceOutcome::InvalidEvidence,
            false,
            combined.len() as u32,
            freshness_seconds,
            duplicate_events,
            vec![reason],
            evidence_references(combined.iter()),
            vec![],
            false,
            None,
        );
    }

    let observed_dimensions = observed_dimensions(&combined);
    let mut missing_fields = request
        .required_dimensions
        .iter()
        .filter(|dimension| !observed_dimensions.contains(*dimension))
        .cloned()
        .collect::<Vec<_>>();
''',
'''    let observed_dimensions = observed_dimensions(&combined);
    let pricing_complete = !combined.is_empty()
        && combined
            .iter()
            .all(|observation| observation.cost_usd.is_some());
    if let Some(reason) = invalid_observation_reason(&combined) {
        return make_finding(
            request,
            BudgetEvidenceOutcome::InvalidEvidence,
            false,
            combined.len() as u32,
            freshness_seconds,
            duplicate_events,
            vec![reason],
            evidence_references(combined.iter()),
            observed_dimensions,
            pricing_complete,
            None,
        );
    }

    let mut missing_fields =
        missing_required_dimensions(&request.required_dimensions, &observed_dimensions);
'''))

replacements.append((
'''    let pricing_complete = !combined.is_empty()
        && combined
            .iter()
            .all(|observation| observation.cost_usd.is_some());
    let metric_complete = metric_complete(&request.anomaly_kind, &baseline)
''',
'''    let metric_complete = metric_complete(&request.anomaly_kind, &baseline)
'''))

replacements.append((
'''fn mixed_required_dimensions(
''',
'''fn missing_required_dimensions(
    required_dimensions: &[String],
    observed_dimensions: &[String],
) -> Vec<String> {
    let mut missing = required_dimensions
        .iter()
        .filter(|dimension| !observed_dimensions.contains(*dimension))
        .cloned()
        .collect::<Vec<_>>();
    missing.sort();
    missing.dedup();
    missing
}

fn mixed_required_dimensions(
'''))

replacements.append((
'''    make_finding_with_details(
        request,
        outcome,
        detected,
        sample_count,
        freshness_seconds,
        duplicate_events,
        reason_codes,
        references,
        observed_dimensions,
        pricing_complete,
        request.required_dimensions.clone(),
        None,
        measurement,
    )
''',
'''    let missing_fields =
        missing_required_dimensions(&request.required_dimensions, &observed_dimensions);
    make_finding_with_details(
        request,
        outcome,
        detected,
        sample_count,
        freshness_seconds,
        duplicate_events,
        reason_codes,
        references,
        observed_dimensions,
        pricing_complete,
        missing_fields,
        None,
        measurement,
    )
'''))

replacements.append((
'''        assert_eq!(sparse.outcome, BudgetEvidenceOutcome::InsufficientEvidence);
        assert!(!sparse.detected);

        let mut observations = paired([100, 100, 100], [300, 300, 300]);
''',
'''        assert_eq!(sparse.outcome, BudgetEvidenceOutcome::InsufficientEvidence);
        assert!(!sparse.detected);
        assert!(sparse
            .coverage
            .observed_dimensions
            .contains(&"provider_id".to_string()));
        assert!(sparse.coverage.missing_fields.is_empty());
        assert_eq!(
            sparse.reason_codes,
            vec!["insufficient_evidence.sparse"]
        );

        let mut observations = paired([100, 100, 100], [300, 300, 300]);
'''))

replacements.append((
'''    #[test]
    fn conflicting_duplicates_fail_closed() {
''',
'''    #[test]
    fn missing_fields_only_reports_unobserved_required_dimensions() {
        let observations = paired([100, 100, 100], [300, 300, 300]);
        let mut request = request(BudgetAnomalyKind::TokenSpike);
        request.required_dimensions = vec!["provider_id".to_string(), "workspace_id".to_string()];
        let finding = detect_budget_anomaly(&request, &observations).unwrap();
        assert_eq!(finding.outcome, BudgetEvidenceOutcome::InsufficientEvidence);
        assert!(finding
            .coverage
            .observed_dimensions
            .contains(&"provider_id".to_string()));
        assert_eq!(finding.coverage.missing_fields, vec!["workspace_id"]);
    }

    #[test]
    fn invalid_metric_preserves_filtered_dimension_coverage() {
        let mut observations = paired([100, 100, 100], [300, 300, 300]);
        observations[0].total_tokens = Some(-1);
        let finding = detect_budget_anomaly(
            &request(BudgetAnomalyKind::TokenSpike),
            &observations,
        )
        .unwrap();
        assert_eq!(finding.outcome, BudgetEvidenceOutcome::InvalidEvidence);
        assert_eq!(finding.reason_codes, vec!["invalid_evidence.negative_metric"]);
        assert!(finding
            .coverage
            .observed_dimensions
            .contains(&"provider_id".to_string()));
        assert!(finding.coverage.missing_fields.is_empty());
        assert_eq!(finding.evidence_references.len(), observations.len());
    }

    #[test]
    fn conflicting_duplicates_fail_closed() {
'''))

replacements.append((
'''        assert!(!finding.detected);
    }
}
''',
'''        assert!(!finding.detected);
        assert!(finding
            .coverage
            .observed_dimensions
            .contains(&"provider_id".to_string()));
        assert!(finding.coverage.missing_fields.is_empty());
    }
}
'''))

for before, after in replacements:
    if before not in text:
        raise SystemExit(f"missing repair anchor:\n{before[:160]}")
    text = text.replace(before, after, 1)

path.write_text(text)
