from pathlib import Path

path = Path("engine/src/budget_forecast.rs")
text = path.read_text()

text = text.replace("BudgetObservedUsage::default()", "empty_observed()")

anchor = '''fn aggregate_observed(observations: &[BudgetUsageObservation]) -> Result<BudgetObservedUsage, String> {
'''
helper = '''fn empty_observed() -> BudgetObservedUsage {
    BudgetObservedUsage {
        input_tokens: None,
        output_tokens: None,
        total_tokens: None,
        cost_usd: None,
        latency_ms: None,
        retry_count: None,
        context_bytes: None,
    }
}

'''
if anchor not in text:
    raise SystemExit("missing aggregate_observed anchor")
text = text.replace(anchor, helper + anchor, 1)

old_call = '''    let mixed_dimensions = mixed_dimensions(&request.scope, &selected);
'''
new_call = '''    let mixed_dimensions = mixed_dimensions(
        &request.scope,
        &request.required_dimensions,
        &selected,
    );
'''
if old_call not in text:
    raise SystemExit("missing mixed_dimensions call anchor")
text = text.replace(old_call, new_call, 1)

old_sig = '''fn mixed_dimensions(
    scope: &BudgetEvidenceScope,
    observations: &[BudgetUsageObservation],
) -> Vec<String> {
'''
new_sig = '''fn mixed_dimensions(
    scope: &BudgetEvidenceScope,
    required_dimensions: &[String],
    observations: &[BudgetUsageObservation],
) -> Vec<String> {
'''
if old_sig not in text:
    raise SystemExit("missing mixed_dimensions signature anchor")
text = text.replace(old_sig, new_sig, 1)

replacements = {
    '''    if scope.run_id.is_none() && distinct_count(observations.iter().filter_map(|item| item.run_id.as_deref())) > 1 {
''': '''    if required_dimensions.iter().any(|dimension| dimension == "run_id")
        && scope.run_id.is_none()
        && distinct_count(observations.iter().filter_map(|item| item.run_id.as_deref())) > 1
    {
''',
    '''    if scope.workspace_id.is_none()
        && distinct_count(observations.iter().filter_map(|item| item.workspace_id.as_deref())) > 1
''': '''    if required_dimensions
        .iter()
        .any(|dimension| dimension == "workspace_id")
        && scope.workspace_id.is_none()
        && distinct_count(observations.iter().filter_map(|item| item.workspace_id.as_deref())) > 1
''',
    '''    if scope.provider_id.is_none()
        && distinct_count(observations.iter().filter_map(|item| item.provider_id.as_deref())) > 1
''': '''    if required_dimensions
        .iter()
        .any(|dimension| dimension == "provider_id")
        && scope.provider_id.is_none()
        && distinct_count(observations.iter().filter_map(|item| item.provider_id.as_deref())) > 1
''',
    '''    if scope.model_id.is_none()
        && distinct_count(observations.iter().filter_map(|item| item.model_id.as_deref())) > 1
''': '''    if required_dimensions.iter().any(|dimension| dimension == "model_id")
        && scope.model_id.is_none()
        && distinct_count(observations.iter().filter_map(|item| item.model_id.as_deref())) > 1
''',
}
for before, after in replacements.items():
    if before not in text:
        raise SystemExit(f"missing mixed-dimension anchor: {before.splitlines()[0]}")
    text = text.replace(before, after, 1)

path.write_text(text)
