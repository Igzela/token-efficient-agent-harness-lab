use serde::{Deserialize, Serialize};
use serde_json::Value;

// ---------------------------------------------------------------------------
// ArtifactCheck
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ArtifactCheck {
    pub name: String,
    pub passed: bool,
    pub message: String,
}

impl Default for ArtifactCheck {
    fn default() -> Self {
        Self {
            name: String::new(),
            passed: false,
            message: String::new(),
        }
    }
}

// ---------------------------------------------------------------------------
// ArtifactGateResult
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ArtifactGateResult {
    pub ok: bool,
    pub checks: Vec<ArtifactCheck>,
    pub missing_artifacts: Vec<String>,
    pub schema_violations: Vec<String>,
    pub forbidden_violations: Vec<String>,
}

impl Default for ArtifactGateResult {
    fn default() -> Self {
        Self {
            ok: false,
            checks: Vec::new(),
            missing_artifacts: Vec::new(),
            schema_violations: Vec::new(),
            forbidden_violations: Vec::new(),
        }
    }
}

// ---------------------------------------------------------------------------
// ArtifactGate
// ---------------------------------------------------------------------------

pub struct ArtifactGate;

impl Default for ArtifactGate {
    fn default() -> Self {
        Self
    }
}

impl ArtifactGate {
    pub fn new() -> Self {
        Self
    }

    pub fn evaluate(
        &self,
        completion: &Value,
        handoff_pack: &Value,
        allowed_files: Option<&[String]>,
        forbidden_files: Option<&[String]>,
    ) -> ArtifactGateResult {
        let mut checks = Vec::new();
        let mut missing = Vec::new();
        let mut schema_violations = Vec::new();
        let mut forbidden = Vec::new();

        self.check_completion_schema(completion, &mut checks, &mut schema_violations);
        self.check_handoff_schema(handoff_pack, &mut checks, &mut schema_violations);
        self.check_artifact_existence(completion, &mut checks, &mut missing);
        self.check_evidence_refs(handoff_pack, &mut checks, &mut missing);
        self.check_handoff_pack_ref(completion, &mut checks, &mut missing);
        self.check_allowed_files(completion, allowed_files, &mut checks, &mut missing);
        self.check_forbidden_files(completion, forbidden_files, &mut checks, &mut forbidden);

        let ok = checks.iter().all(|c| c.passed);

        ArtifactGateResult {
            ok,
            checks,
            missing_artifacts: missing,
            schema_violations,
            forbidden_violations: forbidden,
        }
    }

    fn check_completion_schema(
        &self,
        completion: &Value,
        checks: &mut Vec<ArtifactCheck>,
        violations: &mut Vec<String>,
    ) {
        if !completion.is_object() {
            let msg = "completion must be a JSON object".to_string();
            violations.push(msg.clone());
            checks.push(ArtifactCheck {
                name: "completion_schema".to_string(),
                passed: false,
                message: msg,
            });
            return;
        }

        let obj = completion.as_object().unwrap();
        if obj.get("status").and_then(Value::as_str).is_none() {
            let msg = "completion missing 'status' field".to_string();
            violations.push(msg.clone());
            checks.push(ArtifactCheck {
                name: "completion_schema".to_string(),
                passed: false,
                message: msg,
            });
            return;
        }

        checks.push(ArtifactCheck {
            name: "completion_schema".to_string(),
            passed: true,
            message: "completion valid".to_string(),
        });
    }

    fn check_handoff_schema(
        &self,
        handoff_pack: &Value,
        checks: &mut Vec<ArtifactCheck>,
        violations: &mut Vec<String>,
    ) {
        if !handoff_pack.is_object() {
            let msg = "handoff_pack must be a JSON object".to_string();
            violations.push(msg.clone());
            checks.push(ArtifactCheck {
                name: "handoff_schema".to_string(),
                passed: false,
                message: msg,
            });
            return;
        }

        checks.push(ArtifactCheck {
            name: "handoff_schema".to_string(),
            passed: true,
            message: "handoff_pack valid".to_string(),
        });
    }

    fn check_artifact_existence(
        &self,
        completion: &Value,
        checks: &mut Vec<ArtifactCheck>,
        missing: &mut Vec<String>,
    ) {
        let refs = match completion.get("artifact_refs").and_then(Value::as_array) {
            Some(r) if !r.is_empty() => r,
            _ => {
                checks.push(ArtifactCheck {
                    name: "artifact_existence".to_string(),
                    passed: true,
                    message: "no artifact_refs to check".to_string(),
                });
                return;
            }
        };

        let mut any_failed = false;
        for ref_val in refs {
            let path = ref_val
                .as_object()
                .and_then(|o| o.get("path"))
                .and_then(Value::as_str)
                .unwrap_or("");

            if path.is_empty() {
                missing.push("<empty path>".to_string());
                checks.push(ArtifactCheck {
                    name: "artifact_existence".to_string(),
                    passed: false,
                    message: "artifact_ref has no path".to_string(),
                });
                any_failed = true;
                continue;
            }

            if !std::path::Path::new(path).exists() {
                missing.push(path.to_string());
                checks.push(ArtifactCheck {
                    name: "artifact_existence".to_string(),
                    passed: false,
                    message: format!("not found: {}", path),
                });
                any_failed = true;
            }
        }

        if !any_failed {
            checks.push(ArtifactCheck {
                name: "artifact_existence".to_string(),
                passed: true,
                message: "all artifacts exist".to_string(),
            });
        }
    }

    fn check_evidence_refs(
        &self,
        handoff_pack: &Value,
        checks: &mut Vec<ArtifactCheck>,
        missing: &mut Vec<String>,
    ) {
        let refs = match handoff_pack.get("evidence_refs").and_then(Value::as_array) {
            Some(r) if !r.is_empty() => r,
            _ => {
                checks.push(ArtifactCheck {
                    name: "evidence_refs".to_string(),
                    passed: false,
                    message: "evidence_refs empty or missing".to_string(),
                });
                missing.push("evidence_refs".to_string());
                return;
            }
        };

        let all_ok = refs.iter().all(|ref_val| {
            ref_val
                .as_object()
                .and_then(|o| o.get("path"))
                .and_then(Value::as_str)
                .map(|s| !s.is_empty())
                .unwrap_or(false)
        });

        if all_ok {
            checks.push(ArtifactCheck {
                name: "evidence_refs".to_string(),
                passed: true,
                message: "evidence_refs valid".to_string(),
            });
        } else {
            missing.push("<invalid evidence_ref>".to_string());
            checks.push(ArtifactCheck {
                name: "evidence_refs".to_string(),
                passed: false,
                message: "some evidence_refs invalid".to_string(),
            });
        }
    }

    fn check_handoff_pack_ref(
        &self,
        completion: &Value,
        checks: &mut Vec<ArtifactCheck>,
        missing: &mut Vec<String>,
    ) {
        let ref_path = match completion.get("handoff_pack_ref").and_then(Value::as_str) {
            Some(r) if !r.is_empty() => r,
            _ => {
                checks.push(ArtifactCheck {
                    name: "handoff_pack_ref".to_string(),
                    passed: true,
                    message: "no handoff_pack_ref to check".to_string(),
                });
                return;
            }
        };

        if std::path::Path::new(ref_path).exists() {
            checks.push(ArtifactCheck {
                name: "handoff_pack_ref".to_string(),
                passed: true,
                message: format!("handoff_pack_ref exists: {}", ref_path),
            });
        } else {
            missing.push(ref_path.to_string());
            checks.push(ArtifactCheck {
                name: "handoff_pack_ref".to_string(),
                passed: false,
                message: format!("handoff_pack_ref not found: {}", ref_path),
            });
        }
    }

    fn check_allowed_files(
        &self,
        completion: &Value,
        allowed_files: Option<&[String]>,
        checks: &mut Vec<ArtifactCheck>,
        missing: &mut Vec<String>,
    ) {
        let allowed = match allowed_files {
            Some(a) => a,
            None => {
                checks.push(ArtifactCheck {
                    name: "allowed_files".to_string(),
                    passed: true,
                    message: "no allowed_files constraint".to_string(),
                });
                return;
            }
        };

        let artifact_paths: Vec<String> = completion
            .get("artifact_refs")
            .and_then(Value::as_array)
            .map(|refs| {
                refs.iter()
                    .filter_map(|r| {
                        r.as_object()
                            .and_then(|o| o.get("path"))
                            .and_then(Value::as_str)
                            .map(String::from)
                    })
                    .collect()
            })
            .unwrap_or_default();

        let uncovered: Vec<&String> = artifact_paths
            .iter()
            .filter(|p| {
                !allowed
                    .iter()
                    .any(|a| p.starts_with(a) || a.starts_with(p.as_str()))
            })
            .collect();

        if uncovered.is_empty() {
            checks.push(ArtifactCheck {
                name: "allowed_files".to_string(),
                passed: true,
                message: "all artifacts covered by allowed_files".to_string(),
            });
        } else {
            for p in &uncovered {
                missing.push(format!("not in allowed_files: {}", p));
            }
            checks.push(ArtifactCheck {
                name: "allowed_files".to_string(),
                passed: false,
                message: format!("{} artifact(s) not in allowed_files", uncovered.len()),
            });
        }
    }

    fn check_forbidden_files(
        &self,
        completion: &Value,
        forbidden_files: Option<&[String]>,
        checks: &mut Vec<ArtifactCheck>,
        forbidden: &mut Vec<String>,
    ) {
        let forbidden_list = match forbidden_files {
            Some(f) => f,
            None => {
                checks.push(ArtifactCheck {
                    name: "forbidden_files".to_string(),
                    passed: true,
                    message: "no forbidden_files constraint".to_string(),
                });
                return;
            }
        };

        let mut violations = Vec::new();
        if let Some(refs) = completion.get("artifact_refs").and_then(Value::as_array) {
            for ref_val in refs {
                if let Some(path) = ref_val
                    .as_object()
                    .and_then(|o| o.get("path"))
                    .and_then(Value::as_str)
                {
                    if forbidden_list
                        .iter()
                        .any(|f| path.starts_with(f) || f.starts_with(path))
                    {
                        violations.push(path.to_string());
                    }
                }
            }
        }

        if violations.is_empty() {
            checks.push(ArtifactCheck {
                name: "forbidden_files".to_string(),
                passed: true,
                message: "no forbidden_files violations".to_string(),
            });
        } else {
            let count = violations.len();
            forbidden.extend(violations);
            checks.push(ArtifactCheck {
                name: "forbidden_files".to_string(),
                passed: false,
                message: format!("{} forbidden violation(s)", count),
            });
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn test_valid_completion_and_handoff() {
        let gate = ArtifactGate::new();
        let completion = json!({"status": "completed", "exit_code": 0});
        let handoff = json!({"evidence_refs": [{"path": "/tmp/test"}]});

        let result = gate.evaluate(&completion, &handoff, None, None);
        let completion_check = result
            .checks
            .iter()
            .find(|c| c.name == "completion_schema")
            .unwrap();
        assert!(completion_check.passed);
        let handoff_check = result
            .checks
            .iter()
            .find(|c| c.name == "handoff_schema")
            .unwrap();
        assert!(handoff_check.passed);
    }

    #[test]
    fn test_invalid_completion_missing_status() {
        let gate = ArtifactGate::new();
        let completion = json!({"exit_code": 0});
        let handoff = json!({});

        let result = gate.evaluate(&completion, &handoff, None, None);
        assert!(!result.ok);
        assert!(!result.schema_violations.is_empty());
    }

    #[test]
    fn test_missing_evidence_refs() {
        let gate = ArtifactGate::new();
        let completion = json!({"status": "completed"});
        let handoff = json!({});

        let result = gate.evaluate(&completion, &handoff, None, None);
        assert!(result
            .checks
            .iter()
            .any(|c| c.name == "evidence_refs" && !c.passed));
        assert!(result
            .missing_artifacts
            .contains(&"evidence_refs".to_string()));
    }

    #[test]
    fn test_forbidden_file_violation() {
        let gate = ArtifactGate::new();
        let completion = json!({
            "status": "completed",
            "artifact_refs": [{"path": "/etc/passwd"}]
        });
        let handoff = json!({"evidence_refs": [{"path": "/tmp/e"}]});

        let forbidden = vec!["/etc".to_string()];
        let result = gate.evaluate(&completion, &handoff, None, Some(&forbidden));
        assert!(!result.ok);
        assert!(!result.forbidden_violations.is_empty());
    }

    #[test]
    fn test_no_constraints_all_pass() {
        let gate = ArtifactGate::new();
        let completion = json!({"status": "completed"});
        let handoff = json!({"evidence_refs": [{"path": "/tmp/x"}]});

        let result = gate.evaluate(&completion, &handoff, None, None);
        let evidence = result
            .checks
            .iter()
            .find(|c| c.name == "evidence_refs")
            .unwrap();
        assert!(evidence.passed);
    }

    #[test]
    fn test_non_object_handoff_fails() {
        let gate = ArtifactGate::new();
        let completion = json!({"status": "completed"});
        let handoff = json!("not an object");

        let result = gate.evaluate(&completion, &handoff, None, None);
        assert!(!result.ok);
        assert!(result
            .schema_violations
            .iter()
            .any(|v| v.contains("handoff_pack")));
    }

    #[test]
    fn test_result_serializes_roundtrip() {
        let result = ArtifactGateResult {
            ok: true,
            checks: vec![ArtifactCheck {
                name: "test".to_string(),
                passed: true,
                message: "ok".to_string(),
            }],
            missing_artifacts: vec![],
            schema_violations: vec![],
            forbidden_violations: vec![],
        };
        let json_str = serde_json::to_string(&result).unwrap();
        let back: ArtifactGateResult = serde_json::from_str(&json_str).unwrap();
        assert_eq!(result, back);
    }
}
