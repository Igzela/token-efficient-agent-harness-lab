from pathlib import Path

path = Path("engine/src/budget_anomaly.rs")
text = path.read_text()

before = '''fn evidence_references<'a>(
    observations: impl Iterator<Item = &'a BudgetAnomalyObservation>,
) -> Vec<BudgetEvidenceReference> {
    let mut references = observations
        .map(|observation| BudgetEvidenceReference {
            evidence_type: observation.evidence_type.clone(),
            evidence_id: observation.evidence_id.clone(),
            content_sha256: observation.content_sha256.clone(),
        })
        .collect::<Vec<_>>();
    references.sort_by(|left, right| {
        (left.evidence_type.as_str(), left.evidence_id.as_str())
            .cmp(&(right.evidence_type.as_str(), right.evidence_id.as_str()))
    });
    references
}
'''
after = '''fn evidence_references<'a>(
    observations: impl Iterator<Item = &'a BudgetAnomalyObservation>,
) -> Vec<BudgetEvidenceReference> {
    let mut references = BTreeMap::new();
    for observation in observations {
        let key = (
            observation.evidence_type.clone(),
            observation.evidence_id.clone(),
        );
        match references.get_mut(&key) {
            None => {
                references.insert(key, observation.content_sha256.clone());
            }
            Some(existing) if existing == &observation.content_sha256 => {}
            Some(existing) => {
                *existing = None;
            }
        }
    }
    references
        .into_iter()
        .map(
            |((evidence_type, evidence_id), content_sha256)| BudgetEvidenceReference {
                evidence_type,
                evidence_id,
                content_sha256,
            },
        )
        .collect()
}
'''
if before not in text:
    raise SystemExit("missing evidence reference anchor")
text = text.replace(before, after, 1)

before = '''        let mut conflicting = observations[1].clone();
        conflicting.total_tokens = Some(999);
        observations.push(conflicting);
'''
after = '''        let mut conflicting = observations[1].clone();
        conflicting.total_tokens = Some(999);
        conflicting.content_sha256 = Some("f".repeat(64));
        observations.push(conflicting);
'''
if before not in text:
    raise SystemExit("missing conflicting duplicate fixture anchor")
text = text.replace(before, after, 1)

before = '''        assert!(finding.coverage.missing_fields.is_empty());
    }
}
'''
after = '''        assert!(finding.coverage.missing_fields.is_empty());
        assert_eq!(finding.evidence_references.len(), 6);
        let conflicted = finding
            .evidence_references
            .iter()
            .find(|reference| reference.evidence_id == "baseline-1")
            .expect("conflicting evidence identity must remain referenced");
        assert!(conflicted.content_sha256.is_none());
    }
}
'''
if before not in text:
    raise SystemExit("missing conflicting duplicate assertion anchor")
text = text.replace(before, after, 1)

path.write_text(text)
