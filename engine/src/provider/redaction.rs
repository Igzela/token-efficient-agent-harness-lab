use serde_json::Value;

pub fn redact_secrets(text: &str, secrets: &[&str]) -> String {
    let mut result = text.to_string();
    for secret in secrets {
        if !secret.is_empty() && result.contains(secret) {
            result = result.replace(secret, "***");
        }
    }
    result
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
}
