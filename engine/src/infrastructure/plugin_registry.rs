use super::plugin_system::{
    validate_manifest_fields, PluginManifest, ALL_KNOWN_PERMISSIONS, VALID_TRUST_LEVELS,
};
use std::collections::HashMap;

pub struct PluginRegistry {
    registered: HashMap<String, PluginManifest>,
}

impl Default for PluginRegistry {
    fn default() -> Self {
        Self::new()
    }
}

impl PluginRegistry {
    pub fn new() -> Self {
        Self {
            registered: HashMap::new(),
        }
    }

    pub fn register_plugin(&mut self, manifest: &PluginManifest) -> bool {
        let errors = validate_manifest_fields(manifest);
        if !errors.is_empty() {
            return false;
        }
        if self.registered.contains_key(&manifest.plugin_id) {
            return false;
        }
        self.registered
            .insert(manifest.plugin_id.clone(), manifest.clone());
        true
    }

    pub fn unregister_plugin(&mut self, plugin_id: &str) -> bool {
        self.registered.remove(plugin_id).is_some()
    }

    pub fn validate_manifest(&self, manifest: &PluginManifest) -> Vec<String> {
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
            errors.push(format!("invalid trust_level: '{}'", manifest.trust_level));
        }
        for perm in &manifest.permissions {
            if !ALL_KNOWN_PERMISSIONS.contains(&perm.as_str()) {
                errors.push(format!("unknown permission '{}'", perm));
            }
        }
        errors
    }

    pub fn list_registered(&self) -> Vec<&PluginManifest> {
        self.registered.values().collect()
    }

    pub fn get_plugin(&self, plugin_id: &str) -> Option<&PluginManifest> {
        self.registered.get(plugin_id)
    }

    pub fn search_plugins(&self, query: &str) -> Vec<&PluginManifest> {
        let query_lower = query.to_lowercase();
        self.registered
            .values()
            .filter(|m| {
                m.name.to_lowercase().contains(&query_lower)
                    || m.author.to_lowercase().contains(&query_lower)
            })
            .collect()
    }
}
