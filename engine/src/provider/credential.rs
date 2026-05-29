use std::env;

pub struct CredentialBoundary {
    #[allow(dead_code)]
    backend: String,
}

impl CredentialBoundary {
    pub fn new(backend: &str) -> Result<Self, String> {
        if backend != "env" {
            return Err(format!("Only 'env' backend supported, got '{backend}'"));
        }
        Ok(Self {
            backend: backend.to_string(),
        })
    }

    pub fn resolve(&self, ref_id: &str) -> Result<String, String> {
        env::var(ref_id)
            .map_err(|_| format!("Credential environment variable '{ref_id}' is not set"))
    }

    pub fn validate(&self, ref_id: &str) -> bool {
        self.resolve(ref_id).is_ok()
    }

    pub fn redact_display(secret: &str) -> String {
        if secret.len() <= 4 {
            "***".to_string()
        } else {
            format!("{}***{}", &secret[..3], &secret[secret.len() - 3..])
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn new_rejects_non_env_backend() {
        assert!(CredentialBoundary::new("file").is_err());
        assert!(CredentialBoundary::new("keyring").is_err());
        assert!(CredentialBoundary::new("env").is_ok());
    }

    #[test]
    fn resolve_returns_env_value() {
        env::set_var("_TEST_CRED_BOUNDARY_123", "secret-value");
        let b = CredentialBoundary::new("env").unwrap();
        assert_eq!(
            b.resolve("_TEST_CRED_BOUNDARY_123").unwrap(),
            "secret-value"
        );
        env::remove_var("_TEST_CRED_BOUNDARY_123");
    }

    #[test]
    fn resolve_err_for_missing_var() {
        let b = CredentialBoundary::new("env").unwrap();
        assert!(b.resolve("_NONEXISTENT_VAR_NEVER_SET_").is_err());
    }

    #[test]
    fn validate_true_false() {
        env::set_var("_TEST_CRED_VALIDATE_123", "x");
        let b = CredentialBoundary::new("env").unwrap();
        assert!(b.validate("_TEST_CRED_VALIDATE_123"));
        env::remove_var("_TEST_CRED_VALIDATE_123");
        assert!(!b.validate("_TEST_CRED_VALIDATE_123"));
    }

    #[test]
    fn redact_display_long_string() {
        assert_eq!(
            CredentialBoundary::redact_display("sk-abc123def"),
            "sk-***def"
        );
    }

    #[test]
    fn redact_display_short_string() {
        assert_eq!(CredentialBoundary::redact_display("abc"), "***");
        assert_eq!(CredentialBoundary::redact_display("ab"), "***");
    }
}
