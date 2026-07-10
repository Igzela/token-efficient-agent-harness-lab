use serde_json::Value;

const MAX_REDACTED_TEXT_BYTES: usize = 64 * 1024;

pub fn redact_secrets(text: &str, secrets: &[&str]) -> String {
    let mut result = text.to_string();
    for secret in secrets {
        if !secret.is_empty() && result.contains(secret) {
            result = result.replace(secret, "***");
        }
    }
    result
}

pub fn redact_sensitive_patterns(text: &str) -> String {
    let mut result = text.to_string();
    for pattern in sensitive_patterns() {
        let regex = regex::Regex::new(pattern).expect("valid redaction regex");
        result = regex.replace_all(&result, "***").to_string();
    }
    truncate_redacted_text(result)
}

pub fn contains_sensitive_patterns(text: &str) -> bool {
    sensitive_patterns().iter().any(|pattern| {
        regex::Regex::new(pattern)
            .expect("valid redaction regex")
            .is_match(text)
    })
}

fn sensitive_patterns() -> [&'static str; 5] {
    [
        r"(?i)\bsk-[A-Za-z0-9_\-]{12,}\b",
        r"(?i)\bharness_[a-f0-9]{64}\b",
        r#"(?i)\b(api[_-]?key|secret|token|password|credential)\s*[:=]\s*['\"]?[^'\"\s,}]+"#,
        r"-----BEGIN [A-Z ]*PRIVATE KEY-----[\s\S]*?-----END [A-Z ]*PRIVATE KEY-----",
        r"(?i)\bBearer\s+[A-Za-z0-9._\-]{12,}",
    ]
}

pub fn truncate_redacted_text(mut text: String) -> String {
    if text.len() <= MAX_REDACTED_TEXT_BYTES {
        return text;
    }
    let original_len = text.len();
    let mut split = MAX_REDACTED_TEXT_BYTES;
    while split > 0 && !text.is_char_boundary(split) {
        split -= 1;
    }
    text.truncate(split);
    text.push_str(&format!(
        "\n[truncated {} bytes]\n",
        original_len.saturating_sub(split)
    ));
    text
}

pub fn redact_audit_fields(data: &Value) -> Value {
    match data {
        Value::Object(map) => {
            let mut out = serde_json::Map::new();
            for (key, value) in map {
                if is_sensitive_key(key) {
                    out.insert(key.clone(), Value::String("***".to_string()));
                } else {
                    out.insert(key.clone(), redact_audit_fields(value));
                }
            }
            Value::Object(out)
        }
        Value::Array(arr) => Value::Array(arr.iter().map(redact_audit_fields).collect()),
        other => other.clone(),
    }
}

fn is_sensitive_key(key: &str) -> bool {
    matches!(
        key.to_lowercase().as_str(),
        "api_key"
            | "secret"
            | "token"
            | "password"
            | "credential"
            | "private_key"
            | "access_key"
            | "auth_token"
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn redact_secrets_basic() {
        let result = redact_secrets("my key is sk-abc123", &["sk-abc123"]);
        assert_eq!(result, "my key is ***");
    }

    #[test]
    fn redact_secrets_multiple_occurrences() {
        let result = redact_secrets("sk-abc sk-abc sk-abc", &["sk-abc"]);
        assert_eq!(result, "*** *** ***");
    }

    #[test]
    fn redact_secrets_multiple_secrets() {
        let result = redact_secrets("key1 and key2", &["key1", "key2"]);
        assert_eq!(result, "*** and ***");
    }

    #[test]
    fn redact_secrets_skips_empty() {
        let result = redact_secrets("hello world", &["", "world"]);
        assert_eq!(result, "hello ***");
    }

    #[test]
    fn redact_secrets_no_match() {
        let result = redact_secrets("nothing here", &["sk-missing"]);
        assert_eq!(result, "nothing here");
    }

    #[test]
    fn redact_secrets_empty_text() {
        let result = redact_secrets("", &["sk-abc"]);
        assert_eq!(result, "");
    }

    #[test]
    fn redacts_harness_api_key_shape() {
        let key = format!("harness_{}", "a".repeat(64));
        let text = format!("authorization={key}");
        assert_eq!(redact_sensitive_patterns(&text), "authorization=***");
        assert!(contains_sensitive_patterns(&key));
    }

    #[test]
    fn redact_audit_fields_simple() {
        let data = json!({"api_key": "secret123", "name": "test"});
        let result = redact_audit_fields(&data);
        assert_eq!(result["api_key"], "***");
        assert_eq!(result["name"], "test");
    }

    #[test]
    fn redact_audit_fields_case_insensitive() {
        let data = json!({"API_KEY": "x", "Api_Key": "y", "api_key": "z"});
        let result = redact_audit_fields(&data);
        assert_eq!(result["API_KEY"], "***");
        assert_eq!(result["Api_Key"], "***");
        assert_eq!(result["api_key"], "***");
    }

    #[test]
    fn redact_audit_fields_all_sensitive_keys() {
        let data = json!({
            "api_key": "a",
            "secret": "b",
            "token": "c",
            "password": "d",
            "credential": "e",
            "private_key": "f",
            "access_key": "g",
            "auth_token": "h",
            "safe_field": "kept"
        });
        let result = redact_audit_fields(&data);
        for key in &[
            "api_key",
            "secret",
            "token",
            "password",
            "credential",
            "private_key",
            "access_key",
            "auth_token",
        ] {
            assert_eq!(result[key], "***", "key={key}");
        }
        assert_eq!(result["safe_field"], "kept");
    }

    #[test]
    fn redact_audit_fields_nested_object() {
        let data = json!({"outer": {"api_key": "secret", "inner": {"password": "pw"}}});
        let result = redact_audit_fields(&data);
        assert_eq!(result["outer"]["api_key"], "***");
        assert_eq!(result["outer"]["inner"]["password"], "***");
    }

    #[test]
    fn redact_audit_fields_array_of_objects() {
        let data = json!({"items": [{"token": "t1"}, {"token": "t2", "name": "x"}]});
        let result = redact_audit_fields(&data);
        assert_eq!(result["items"][0]["token"], "***");
        assert_eq!(result["items"][1]["token"], "***");
        assert_eq!(result["items"][1]["name"], "x");
    }

    #[test]
    fn redact_audit_fields_array_of_primitives() {
        let data = json!({"tags": ["a", "b", "c"]});
        let result = redact_audit_fields(&data);
        assert_eq!(result["tags"], json!(["a", "b", "c"]));
    }

    #[test]
    fn redact_audit_fields_non_object_passthrough() {
        let data = json!("just a string");
        let result = redact_audit_fields(&data);
        assert_eq!(result, data);
    }

    #[test]
    fn redact_audit_fields_preserves_types() {
        let data =
            json!({"count": 42, "flag": true, "ratio": std::f64::consts::PI, "nothing": null});
        let result = redact_audit_fields(&data);
        assert_eq!(result["count"], 42);
        assert!(result["flag"].as_bool().unwrap());
        assert_eq!(result["ratio"], std::f64::consts::PI);
        assert_eq!(result["nothing"], serde_json::Value::Null);
    }

    #[test]
    fn redact_audit_fields_empty_object() {
        let data = json!({});
        let result = redact_audit_fields(&data);
        assert_eq!(result, data);
    }

    #[test]
    fn redact_sensitive_patterns_catches_common_secret_shapes() {
        let text = concat!(
            "api_",
            "key=sk-abcdefghijklmnopqrstuvwxyz token: bearer-secret Bear",
            "er abcdefghijklmnopqrstuvwxyz"
        );
        let result = redact_sensitive_patterns(text);
        assert!(!result.contains("sk-abcdefghijklmnopqrstuvwxyz"));
        assert!(!result.contains("bearer-secret"));
        assert!(!result.contains("abcdefghijklmnopqrstuvwxyz"));
        assert!(result.contains("***"));
    }

    #[test]
    fn contains_sensitive_patterns_does_not_treat_large_safe_text_as_secret() {
        assert!(!contains_sensitive_patterns(
            &"x".repeat(MAX_REDACTED_TEXT_BYTES + 10)
        ));
        assert!(contains_sensitive_patterns(concat!(
            "api_",
            "key=sk-abcdefghijklmnopqrstuvwxyz"
        )));
    }

    #[test]
    fn redact_sensitive_patterns_truncates_large_text() {
        let result = redact_sensitive_patterns(&"x".repeat(MAX_REDACTED_TEXT_BYTES + 10));
        assert!(result.contains("[truncated 10 bytes]"));
    }
}
