use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};

pub const PLUGIN_SYSTEM_SCHEMA_VERSION: &str = "plugin_system.v1";

pub const TRUST_LEVEL_COMMUNITY: &str = "community";
pub const TRUST_LEVEL_VERIFIED: &str = "verified";
pub const TRUST_LEVEL_OFFICIAL: &str = "official";

pub const VALID_TRUST_LEVELS: &[&str] = &[
    TRUST_LEVEL_COMMUNITY,
    TRUST_LEVEL_VERIFIED,
    TRUST_LEVEL_OFFICIAL,
];

pub const ALL_KNOWN_PERMISSIONS: &[&str] = &[
    "dispatch:read",
    "dispatch:write",
    "provider:execute",
    "config:read",
    "config:write",
    "ledger:read",
    "ledger:write",
];

pub fn trust_permissions(trust_level: &str) -> HashSet<String> {
    match trust_level {
        TRUST_LEVEL_COMMUNITY => {
            let mut s = HashSet::new();
            s.insert("dispatch:read".to_string());
            s
        }
        TRUST_LEVEL_VERIFIED => {
            let mut s = HashSet::new();
            s.insert("dispatch:read".to_string());
            s.insert("dispatch:write".to_string());
            s
        }
        TRUST_LEVEL_OFFICIAL => HashSet::new(), // empty = unrestricted
        _ => HashSet::new(),
    }
}

pub const REQUIRED_MANIFEST_FIELDS: &[&str] = &[
    "schema_version",
    "plugin_id",
    "name",
    "version",
    "author",
    "permissions",
    "entrypoints",
    "compatible_dispatcher_versions",
    "required_env",
    "network_access",
    "filesystem_access",
    "trust_level",
];

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PluginManifest {
    pub schema_version: String,
    pub plugin_id: String,
    pub name: String,
    pub version: String,
    pub author: String,
    pub permissions: Vec<String>,
    pub entrypoints: Vec<String>,
    pub compatible_dispatcher_versions: Vec<String>,
    pub required_env: Vec<String>,
    pub network_access: bool,
    pub filesystem_access: bool,
    pub signature: Option<String>,
    pub trust_level: String,
}

impl PluginManifest {
    pub fn to_dict(&self) -> serde_json::Value {
        serde_json::to_value(self).unwrap_or(serde_json::Value::Null)
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct LoadedPlugin {
    pub manifest: PluginManifest,
    pub module_name: String,
    pub loaded_at: f64,
    pub status: String,
}

pub struct PluginSystem {
    plugins: HashMap<String, LoadedPlugin>,
}

impl Default for PluginSystem {
    fn default() -> Self {
        Self::new()
    }
}

impl PluginSystem {
    pub fn new() -> Self {
        Self {
            plugins: HashMap::new(),
        }
    }

    pub fn load_plugin_from_manifest(
        &mut self,
        manifest: PluginManifest,
        now: f64,
    ) -> Result<LoadedPlugin, String> {
        let errors = validate_manifest_fields(&manifest);
        if !errors.is_empty() {
            return Err(format!("Invalid manifest: {}", errors.join("; ")));
        }

        if !trust_permission_allowed(&manifest.trust_level, &manifest.permissions) {
            let denied: Vec<String> = manifest
                .permissions
                .iter()
                .filter(|p| {
                    let allowed = trust_permissions(&manifest.trust_level);
                    !allowed.contains(p.as_str())
                })
                .cloned()
                .collect();
            return Err(format!(
                "Trust level '{}' not allowed for permissions: {:?}",
                manifest.trust_level, denied
            ));
        }

        let module_name = format!("harness_plugin.{}", manifest.plugin_id);
        let loaded = LoadedPlugin {
            manifest: manifest.clone(),
            module_name,
            loaded_at: now,
            status: "loaded".to_string(),
        };
        self.plugins
            .insert(manifest.plugin_id.clone(), loaded.clone());
        Ok(loaded)
    }

    pub fn unload_plugin(&mut self, plugin_id: &str) -> bool {
        self.plugins.remove(plugin_id).is_some()
    }

    pub fn check_permission(&self, plugin_id: &str, permission: &str) -> bool {
        let loaded = match self.plugins.get(plugin_id) {
            Some(l) => l,
            None => return false,
        };
        if !ALL_KNOWN_PERMISSIONS.contains(&permission) {
            return false;
        }
        loaded
            .manifest
            .permissions
            .contains(&permission.to_string())
    }

    pub fn list_plugins(&self) -> Vec<&LoadedPlugin> {
        self.plugins.values().collect()
    }

    pub fn get_plugin(&self, plugin_id: &str) -> Option<&LoadedPlugin> {
        self.plugins.get(plugin_id)
    }
}

pub fn parse_manifest(raw: &serde_json::Value) -> Result<PluginManifest, String> {
    let get_str = |key: &str| -> String {
        raw.get(key)
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string()
    };
    let get_str_vec = |key: &str| -> Vec<String> {
        raw.get(key)
            .and_then(|v| v.as_array())
            .map(|arr| {
                arr.iter()
                    .filter_map(|v| v.as_str().map(String::from))
                    .collect()
            })
            .unwrap_or_default()
    };

    Ok(PluginManifest {
        schema_version: get_str("schema_version"),
        plugin_id: get_str("plugin_id"),
        name: get_str("name"),
        version: get_str("version"),
        author: get_str("author"),
        permissions: get_str_vec("permissions"),
        entrypoints: get_str_vec("entrypoints"),
        compatible_dispatcher_versions: get_str_vec("compatible_dispatcher_versions"),
        required_env: get_str_vec("required_env"),
        network_access: raw
            .get("network_access")
            .and_then(|v| v.as_bool())
            .unwrap_or(false),
        filesystem_access: raw
            .get("filesystem_access")
            .and_then(|v| v.as_bool())
            .unwrap_or(false),
        signature: raw
            .get("signature")
            .and_then(|v| v.as_str())
            .map(String::from),
        trust_level: get_str("trust_level"),
    })
}

pub fn validate_manifest_fields(manifest: &PluginManifest) -> Vec<String> {
    let mut errors = Vec::new();
    if manifest.plugin_id.is_empty() {
        errors.push("plugin_id is required".to_string());
    }
    if manifest.name.is_empty() {
        errors.push("name is required".to_string());
    }
    if manifest.version.is_empty() {
        errors.push("version is required".to_string());
    }
    if manifest.author.is_empty() {
        errors.push("author is required".to_string());
    }
    if !VALID_TRUST_LEVELS.contains(&manifest.trust_level.as_str()) {
        errors.push(format!(
            "trust_level must be one of {:?}, got '{}'",
            VALID_TRUST_LEVELS, manifest.trust_level
        ));
    }
    for perm in &manifest.permissions {
        if !ALL_KNOWN_PERMISSIONS.contains(&perm.as_str()) {
            errors.push(format!("unknown permission '{}'", perm));
        }
    }
    errors
}

pub fn trust_permission_allowed(trust_level: &str, permissions: &[String]) -> bool {
    if trust_level == TRUST_LEVEL_OFFICIAL {
        return true;
    }
    let allowed = trust_permissions(trust_level);
    permissions.iter().all(|p| allowed.contains(p.as_str()))
}
