from pathlib import Path

path = Path("engine/src/budget_anomaly.rs")
text = path.read_text()

before = '''    let mut missing_fields =
        missing_required_dimensions(&request.required_dimensions, &observed_dimensions);
    let mixed = mixed_required_dimensions(&request.scope, &request.required_dimensions, &combined);
    missing_fields.extend(mixed.iter().map(|dimension| format!("{dimension}.mixed")));
'''
after = '''    let mut missing_fields =
        missing_required_dimensions(&request.required_dimensions, &observed_dimensions);
    let mixed = mixed_required_dimensions(&request.scope, &request.required_dimensions, &combined);
'''
if before not in text:
    raise SystemExit("missing mixed-dimension coverage anchor")
text = text.replace(before, after, 1)

before = '''        assert!(finding
            .reason_codes
            .contains(&"insufficient_evidence.mixed_dimensions".to_string()));
    }
'''
after = '''        assert!(finding
            .reason_codes
            .contains(&"insufficient_evidence.mixed_dimensions".to_string()));
        assert!(finding.coverage.missing_fields.is_empty());
    }
'''
if before not in text:
    raise SystemExit("missing mixed-dimension test anchor")
text = text.replace(before, after, 1)

path.write_text(text)
