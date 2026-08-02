use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::{HashMap, HashSet};
use uuid::Uuid;

pub const AUTH_SCHEMA_VERSION: &str = "auth.v1";
/// The environment-injected bootstrap key is the sole local authority allowed
/// to delegate managed-acceptance scopes. API-created keys remain ordinary
/// principals even when they carry `team:admin`.
pub const LOCAL_BOOTSTRAP_API_KEY_ID: &str = "local-admin-env";
pub const LOCAL_BOOTSTRAP_TENANT_ID: &str = "local";
const API_KEY_PREFIX: &str = "harness_";
const API_KEY_SUFFIX_LEN: usize = 64;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RequestContext {
    pub tenant_id: String,
    pub api_key_id: String,
    pub scopes: HashSet<String>,
    pub request_id: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Tenant {
    pub tenant_id: String,
    pub name: String,
    pub scopes: HashSet<String>,
    pub rate_limit: Option<i64>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct APIKey {
    pub key_id: String,
    pub tenant_id: String,
    pub key_hash: String,
    pub key_salt: String,
    pub scopes: HashSet<String>,
    pub created_at: f64,
    pub expires_at: Option<f64>,
    pub revoked_at: Option<f64>,
    pub last_used_at: Option<f64>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AuthDecision {
    pub allowed: bool,
    pub tenant_id: Option<String>,
    pub api_key_id: Option<String>,
    pub scopes: HashSet<String>,
    pub reason: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AuthorizationDecision {
    pub allowed: bool,
    pub reason: String,
    pub required_scopes: HashSet<String>,
    pub granted_scopes: HashSet<String>,
}

pub fn hash_api_key(raw_key: &str, salt: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(salt.as_bytes());
    hasher.update(raw_key.as_bytes());
    hex::encode(hasher.finalize())
}

pub fn validate_token_shape(token: &str) -> bool {
    if !token.starts_with(API_KEY_PREFIX) {
        return false;
    }
    let suffix = &token[API_KEY_PREFIX.len()..];
    if suffix.len() != API_KEY_SUFFIX_LEN {
        return false;
    }
    suffix.chars().all(|c| c.is_ascii_hexdigit())
}

pub struct TenantResolver {
    api_keys: HashMap<String, APIKey>,
    tenants: HashMap<String, Tenant>,
}

impl Default for TenantResolver {
    fn default() -> Self {
        Self::new()
    }
}

impl TenantResolver {
    pub fn new() -> Self {
        Self {
            api_keys: HashMap::new(),
            tenants: HashMap::new(),
        }
    }

    pub fn add_tenant(&mut self, tenant: Tenant) {
        self.tenants.insert(tenant.tenant_id.clone(), tenant);
    }

    pub fn add_api_key(&mut self, key: APIKey) {
        self.api_keys.insert(key.key_id.clone(), key);
    }

    pub fn remove_api_key(&mut self, key_id: &str) -> bool {
        self.api_keys.remove(key_id).is_some()
    }

    pub fn api_key(&self, key_id: &str) -> Option<APIKey> {
        self.api_keys.get(key_id).cloned()
    }

    pub fn mark_key_used(&mut self, key_id: &str, now: f64) {
        if let Some(key) = self.api_keys.get_mut(key_id) {
            key.last_used_at = Some(now);
        }
    }

    pub fn tenant_rate_limit(&self, tenant_id: &str) -> Option<i64> {
        self.tenants
            .get(tenant_id)
            .and_then(|tenant| tenant.rate_limit)
    }

    pub fn validate_api_key_scopes(
        &self,
        key_id: &str,
        scopes: &HashSet<String>,
    ) -> Result<(), String> {
        let key = self
            .api_keys
            .get(key_id)
            .ok_or_else(|| format!("unknown api key: {key_id}"))?;
        let tenant = self
            .tenants
            .get(&key.tenant_id)
            .ok_or_else(|| format!("unknown tenant: {}", key.tenant_id))?;
        if !tenant.scopes.is_empty() && !scopes.is_subset(&tenant.scopes) {
            return Err("key scopes exceed tenant scopes".to_string());
        }
        Ok(())
    }

    pub fn update_api_key_scopes(
        &mut self,
        key_id: &str,
        scopes: HashSet<String>,
    ) -> Result<(), String> {
        self.validate_api_key_scopes(key_id, &scopes)?;
        self.api_keys
            .get_mut(key_id)
            .expect("validated api key must still exist")
            .scopes = scopes;
        Ok(())
    }

    pub fn create_api_key(
        &mut self,
        tenant_id: &str,
        scopes: Option<HashSet<String>>,
        expires_at: Option<f64>,
        now: f64,
    ) -> Result<(APIKey, String), String> {
        let (key, raw_key) = self.prepare_api_key(tenant_id, scopes, expires_at, now)?;
        self.api_keys.insert(key.key_id.clone(), key.clone());
        Ok((key, raw_key))
    }

    /// Prepare a key without registering it in the in-memory resolver. The
    /// caller can persist the key through its canonical store owner first and
    /// register it only after durable metadata succeeds.
    pub fn prepare_api_key(
        &self,
        tenant_id: &str,
        scopes: Option<HashSet<String>>,
        expires_at: Option<f64>,
        now: f64,
    ) -> Result<(APIKey, String), String> {
        let tenant = self
            .tenants
            .get(tenant_id)
            .ok_or_else(|| format!("unknown tenant: {tenant_id}"))?;

        let raw_key = format!(
            "{API_KEY_PREFIX}{}{}",
            Uuid::new_v4().simple(),
            Uuid::new_v4().simple()
        );
        let salt = format!("{}{}", Uuid::new_v4().simple(), Uuid::new_v4().simple());
        let key_hash = hash_api_key(&raw_key, &salt);

        let key_scopes = scopes.unwrap_or_else(|| tenant.scopes.clone());
        if !tenant.scopes.is_empty() && !key_scopes.is_subset(&tenant.scopes) {
            return Err(format!(
                "key scopes {:?} exceed tenant scopes {:?}",
                key_scopes, tenant.scopes
            ));
        }

        let key = APIKey {
            key_id: format!("key_{}", Uuid::new_v4().simple()),
            tenant_id: tenant_id.to_string(),
            key_hash,
            key_salt: salt,
            scopes: key_scopes,
            created_at: now,
            expires_at,
            revoked_at: None,
            last_used_at: None,
        };
        Ok((key, raw_key))
    }

    pub fn resolve_mut(&mut self, auth_header: Option<&str>, now: f64) -> AuthDecision {
        let header = match auth_header {
            Some(h) => h,
            None => {
                return AuthDecision {
                    allowed: false,
                    tenant_id: None,
                    api_key_id: None,
                    scopes: HashSet::new(),
                    reason: "missing authorization header".to_string(),
                }
            }
        };

        let parts: Vec<&str> = header.splitn(2, ' ').collect();
        if parts.len() != 2 || parts[0].to_lowercase() != "bearer" {
            return AuthDecision {
                allowed: false,
                tenant_id: None,
                api_key_id: None,
                scopes: HashSet::new(),
                reason: "invalid authorization format".to_string(),
            };
        }

        let raw_token = parts[1];
        if !validate_token_shape(raw_token) {
            return AuthDecision {
                allowed: false,
                tenant_id: None,
                api_key_id: None,
                scopes: HashSet::new(),
                reason: "invalid api key".to_string(),
            };
        }

        let raw_hash = {
            let mut matched = None;
            for key in self.api_keys.values() {
                let computed = hash_api_key(raw_token, &key.key_salt);
                if constant_time_eq(&computed, &key.key_hash) {
                    matched = Some(key.clone());
                    break;
                }
            }
            matched
        };

        let matched_key = match raw_hash {
            Some(k) => k,
            None => {
                return AuthDecision {
                    allowed: false,
                    tenant_id: None,
                    api_key_id: None,
                    scopes: HashSet::new(),
                    reason: "invalid api key".to_string(),
                }
            }
        };

        if let Some(expires) = matched_key.expires_at {
            if now > expires {
                return AuthDecision {
                    allowed: false,
                    tenant_id: None,
                    api_key_id: None,
                    scopes: HashSet::new(),
                    reason: "api key expired".to_string(),
                };
            }
        }

        if let Some(revoked) = matched_key.revoked_at {
            if now >= revoked {
                return AuthDecision {
                    allowed: false,
                    tenant_id: None,
                    api_key_id: None,
                    scopes: HashSet::new(),
                    reason: "api key revoked".to_string(),
                };
            }
        }

        let tenant = self.tenants.get(&matched_key.tenant_id);
        if tenant.is_none() {
            return AuthDecision {
                allowed: false,
                tenant_id: None,
                api_key_id: None,
                scopes: HashSet::new(),
                reason: "unknown tenant".to_string(),
            };
        }

        self.mark_key_used(&matched_key.key_id, now);

        AuthDecision {
            allowed: true,
            tenant_id: Some(matched_key.tenant_id.clone()),
            api_key_id: Some(matched_key.key_id.clone()),
            scopes: matched_key.scopes.clone(),
            reason: String::new(),
        }
    }
}

fn constant_time_eq(a: &str, b: &str) -> bool {
    if a.len() != b.len() {
        return false;
    }
    let mut result = 0u8;
    for (x, y) in a.bytes().zip(b.bytes()) {
        result |= x ^ y;
    }
    result == 0
}
