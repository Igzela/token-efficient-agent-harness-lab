use regex::Regex;
use serde::{Deserialize, Serialize};
use std::collections::HashSet;
use std::path::PathBuf;
use std::sync::LazyLock;

pub const APP_REGISTRY_SCHEMA_VERSION: &str = "app_registry.v1";

static VALID_REPO_ID: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"^[A-Za-z0-9][A-Za-z0-9_.-]{0,63}$").unwrap());

const VALID_KINDS: &[&str] = &["local", "remote"];

#[derive(Debug, Clone, PartialEq)]
pub enum AppRegistryError {
    InvalidId,
    MissingName,
    InvalidKind,
    LocalRequiresPath,
    LocalMustNotHaveUrl,
    RemoteRequiresUrl,
    RemoteMustNotHavePath,
    PathNotDirectory,
    PathNotReadable,
    DuplicateId(String),
    SchemaVersionMismatch,
    InvalidReposField,
    UnreadableFile(String),
}

impl std::fmt::Display for AppRegistryError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::InvalidId => write!(
                f,
                "repo id must be 1-64 chars: letters, numbers, dot, underscore, hyphen"
            ),
            Self::MissingName => write!(f, "repo name is required"),
            Self::InvalidKind => write!(f, "repo kind must be local or remote"),
            Self::LocalRequiresPath => write!(f, "local repo requires path"),
            Self::LocalMustNotHaveUrl => write!(f, "local repo must not include url"),
            Self::RemoteRequiresUrl => write!(f, "remote repo requires url"),
            Self::RemoteMustNotHavePath => write!(f, "remote repo must not include path"),
            Self::PathNotDirectory => write!(f, "local repo path must exist and be a directory"),
            Self::PathNotReadable => write!(f, "local repo path is not readable"),
            Self::DuplicateId(id) => write!(f, "duplicate repo id: {id}"),
            Self::SchemaVersionMismatch => write!(f, "unsupported registry schema version"),
            Self::InvalidReposField => write!(f, "registry repos must be a list"),
            Self::UnreadableFile(msg) => write!(f, "{msg}"),
        }
    }
}

impl std::error::Error for AppRegistryError {}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RepoRef {
    pub id: String,
    pub name: String,
    pub kind: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub path: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub url: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub branch: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
}

#[allow(clippy::derivable_impls)]
impl Default for RepoRef {
    fn default() -> Self {
        Self {
            id: String::new(),
            name: String::new(),
            kind: String::new(),
            path: None,
            url: None,
            branch: None,
            description: None,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AppRegistry {
    pub repos: Vec<RepoRef>,
    pub schema_version: String,
}

impl Default for AppRegistry {
    fn default() -> Self {
        Self {
            repos: Vec::new(),
            schema_version: APP_REGISTRY_SCHEMA_VERSION.to_string(),
        }
    }
}

impl AppRegistry {
    pub fn empty() -> Self {
        Self::default()
    }

    pub fn load(path: &str) -> Result<Self, AppRegistryError> {
        let p = PathBuf::from(path);
        if !p.exists() {
            return Ok(Self::empty());
        }
        let data = std::fs::read_to_string(&p).map_err(|e| {
            AppRegistryError::UnreadableFile(format!("registry file is unreadable: {e}"))
        })?;
        let value: serde_json::Value = serde_json::from_str(&data).map_err(|e| {
            AppRegistryError::UnreadableFile(format!("registry file is invalid JSON: {e}"))
        })?;
        let schema_version = value
            .get("schema_version")
            .and_then(|v| v.as_str())
            .unwrap_or("");
        if schema_version != APP_REGISTRY_SCHEMA_VERSION {
            return Err(AppRegistryError::SchemaVersionMismatch);
        }
        let raw_repos = value
            .get("repos")
            .and_then(|v| v.as_array())
            .ok_or(AppRegistryError::InvalidReposField)?;
        let mut repos = Vec::with_capacity(raw_repos.len());
        for item in raw_repos {
            let repo: RepoRef = serde_json::from_value(item.clone())
                .map_err(|_| AppRegistryError::UnreadableFile("invalid repo entry".to_string()))?;
            repos.push(validate_repo_ref(repo)?);
        }
        reject_duplicate_ids(&repos)?;
        Ok(Self {
            repos,
            schema_version: APP_REGISTRY_SCHEMA_VERSION.to_string(),
        })
    }

    pub fn save(&self, path: &str) -> Result<(), AppRegistryError> {
        let p = PathBuf::from(path);
        if let Some(parent) = p.parent() {
            std::fs::create_dir_all(parent).map_err(|e| {
                AppRegistryError::UnreadableFile(format!("cannot create directory: {e}"))
            })?;
        }
        let data = serde_json::json!({
            "schema_version": self.schema_version,
            "repos": self.repos.iter().map(repo_ref_to_json).collect::<Vec<_>>(),
        });
        let json = serde_json::to_string_pretty(&data).unwrap();
        std::fs::write(&p, json + "\n")
            .map_err(|e| AppRegistryError::UnreadableFile(format!("cannot write registry: {e}")))
    }

    pub fn list_repos(&self) -> &[RepoRef] {
        &self.repos
    }

    pub fn get_repo(&self, repo_id: &str) -> Option<&RepoRef> {
        self.repos.iter().find(|r| r.id == repo_id)
    }

    pub fn add_repo(&self, repo: RepoRef) -> Result<Self, AppRegistryError> {
        let normalized = validate_repo_ref(repo)?;
        if self.get_repo(&normalized.id).is_some() {
            return Err(AppRegistryError::DuplicateId(normalized.id));
        }
        let mut new_repos = self.repos.clone();
        new_repos.push(normalized);
        Ok(Self {
            repos: new_repos,
            schema_version: self.schema_version.clone(),
        })
    }
}

fn repo_ref_to_json(repo: &RepoRef) -> serde_json::Value {
    let mut map = serde_json::Map::new();
    map.insert("id".into(), serde_json::Value::String(repo.id.clone()));
    map.insert("name".into(), serde_json::Value::String(repo.name.clone()));
    map.insert("kind".into(), serde_json::Value::String(repo.kind.clone()));
    if let Some(ref path) = repo.path {
        map.insert("path".into(), serde_json::Value::String(path.clone()));
    }
    if let Some(ref url) = repo.url {
        map.insert("url".into(), serde_json::Value::String(url.clone()));
    }
    if let Some(ref branch) = repo.branch {
        map.insert("branch".into(), serde_json::Value::String(branch.clone()));
    }
    if let Some(ref description) = repo.description {
        map.insert(
            "description".into(),
            serde_json::Value::String(description.clone()),
        );
    }
    serde_json::Value::Object(map)
}

pub fn validate_repo_ref(repo: RepoRef) -> Result<RepoRef, AppRegistryError> {
    if !VALID_REPO_ID.is_match(&repo.id) {
        return Err(AppRegistryError::InvalidId);
    }
    if repo.name.trim().is_empty() {
        return Err(AppRegistryError::MissingName);
    }
    if !VALID_KINDS.contains(&repo.kind.as_str()) {
        return Err(AppRegistryError::InvalidKind);
    }
    if repo.kind == "local" {
        if repo.path.is_none() || repo.path.as_deref().unwrap_or("").is_empty() {
            return Err(AppRegistryError::LocalRequiresPath);
        }
        if repo.url.is_some() {
            return Err(AppRegistryError::LocalMustNotHaveUrl);
        }
        let raw_path = PathBuf::from(repo.path.as_ref().unwrap());
        let resolved = std::fs::canonicalize(&raw_path).unwrap_or(raw_path);
        validate_local_repo_path(&resolved)?;
        return Ok(RepoRef {
            id: repo.id,
            name: repo.name.trim().to_string(),
            kind: "local".to_string(),
            path: Some(resolved.to_string_lossy().to_string()),
            url: None,
            branch: clean_optional(repo.branch),
            description: clean_optional(repo.description),
        });
    }
    if repo.url.is_none() || repo.url.as_deref().unwrap_or("").is_empty() {
        return Err(AppRegistryError::RemoteRequiresUrl);
    }
    if repo.path.is_some() {
        return Err(AppRegistryError::RemoteMustNotHavePath);
    }
    Ok(RepoRef {
        id: repo.id,
        name: repo.name.trim().to_string(),
        kind: "remote".to_string(),
        path: None,
        url: Some(repo.url.unwrap().trim().to_string()),
        branch: clean_optional(repo.branch),
        description: clean_optional(repo.description),
    })
}

pub fn validate_local_repo_path(path: &std::path::Path) -> Result<(), AppRegistryError> {
    if !path.exists() || !path.is_dir() {
        return Err(AppRegistryError::PathNotDirectory);
    }
    match std::fs::read_dir(path) {
        Ok(_) => Ok(()),
        Err(_) => Err(AppRegistryError::PathNotReadable),
    }
}

pub fn registry_to_dict(registry: &AppRegistry) -> serde_json::Value {
    serde_json::json!({
        "schema_version": registry.schema_version,
        "repos": registry.repos.iter().map(repo_ref_to_json).collect::<Vec<_>>(),
    })
}

fn reject_duplicate_ids(repos: &[RepoRef]) -> Result<(), AppRegistryError> {
    let mut seen = HashSet::new();
    for repo in repos {
        if !seen.insert(&repo.id) {
            return Err(AppRegistryError::DuplicateId(repo.id.clone()));
        }
    }
    Ok(())
}

fn clean_optional(value: Option<String>) -> Option<String> {
    value.and_then(|v| {
        let trimmed = v.trim().to_string();
        if trimmed.is_empty() {
            None
        } else {
            Some(trimmed)
        }
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn valid_local_repo(id: &str, path: &str) -> RepoRef {
        RepoRef {
            id: id.to_string(),
            name: format!("{id} repo"),
            kind: "local".to_string(),
            path: Some(path.to_string()),
            ..Default::default()
        }
    }

    fn valid_remote_repo(id: &str, url: &str) -> RepoRef {
        RepoRef {
            id: id.to_string(),
            name: format!("{id} repo"),
            kind: "remote".to_string(),
            url: Some(url.to_string()),
            ..Default::default()
        }
    }

    #[test]
    fn test_validate_repo_ref_remote_ok() {
        let repo = valid_remote_repo("myrepo", "https://github.com/example/repo");
        let result = validate_repo_ref(repo).unwrap();
        assert_eq!(result.kind, "remote");
        assert_eq!(
            result.url.as_deref(),
            Some("https://github.com/example/repo")
        );
        assert!(result.path.is_none());
    }

    #[test]
    fn test_validate_repo_ref_local_ok() {
        let tmp = tempfile::tempdir().unwrap();
        let repo = valid_local_repo("local1", tmp.path().to_str().unwrap());
        let result = validate_repo_ref(repo).unwrap();
        assert_eq!(result.kind, "local");
        assert!(result.path.is_some());
    }

    #[test]
    fn test_validate_repo_ref_invalid_id() {
        let repo = RepoRef {
            id: "".to_string(),
            name: "test".to_string(),
            kind: "remote".to_string(),
            url: Some("https://example.com".to_string()),
            ..Default::default()
        };
        assert_eq!(validate_repo_ref(repo), Err(AppRegistryError::InvalidId));
    }

    #[test]
    fn test_validate_repo_ref_invalid_kind() {
        let repo = RepoRef {
            id: "goodid".to_string(),
            name: "test".to_string(),
            kind: "unknown".to_string(),
            ..Default::default()
        };
        assert_eq!(validate_repo_ref(repo), Err(AppRegistryError::InvalidKind));
    }

    #[test]
    fn test_validate_repo_ref_local_requires_path() {
        let repo = RepoRef {
            id: "goodid".to_string(),
            name: "test".to_string(),
            kind: "local".to_string(),
            ..Default::default()
        };
        assert_eq!(
            validate_repo_ref(repo),
            Err(AppRegistryError::LocalRequiresPath)
        );
    }

    #[test]
    fn test_validate_repo_ref_remote_requires_url() {
        let repo = RepoRef {
            id: "goodid".to_string(),
            name: "test".to_string(),
            kind: "remote".to_string(),
            ..Default::default()
        };
        assert_eq!(
            validate_repo_ref(repo),
            Err(AppRegistryError::RemoteRequiresUrl)
        );
    }

    #[test]
    fn test_validate_repo_ref_local_no_url() {
        let tmp = tempfile::tempdir().unwrap();
        let repo = RepoRef {
            id: "goodid".to_string(),
            name: "test".to_string(),
            kind: "local".to_string(),
            path: Some(tmp.path().to_str().unwrap().to_string()),
            url: Some("https://example.com".to_string()),
            ..Default::default()
        };
        assert_eq!(
            validate_repo_ref(repo),
            Err(AppRegistryError::LocalMustNotHaveUrl)
        );
    }

    #[test]
    fn test_validate_repo_ref_remote_no_path() {
        let repo = RepoRef {
            id: "goodid".to_string(),
            name: "test".to_string(),
            kind: "remote".to_string(),
            url: Some("https://example.com".to_string()),
            path: Some("/tmp".to_string()),
            ..Default::default()
        };
        assert_eq!(
            validate_repo_ref(repo),
            Err(AppRegistryError::RemoteMustNotHavePath)
        );
    }

    #[test]
    fn test_app_registry_empty() {
        let registry = AppRegistry::empty();
        assert!(registry.repos.is_empty());
        assert_eq!(registry.schema_version, APP_REGISTRY_SCHEMA_VERSION);
    }

    #[test]
    fn test_app_registry_add_repo() {
        let registry = AppRegistry::empty();
        let repo = valid_remote_repo("r1", "https://example.com");
        let updated = registry.add_repo(repo).unwrap();
        assert_eq!(updated.repos.len(), 1);
        assert_eq!(updated.repos[0].id, "r1");
    }

    #[test]
    fn test_app_registry_add_duplicate() {
        let registry = AppRegistry::empty();
        let repo = valid_remote_repo("r1", "https://example.com");
        let updated = registry.add_repo(repo).unwrap();
        let dup = valid_remote_repo("r1", "https://other.com");
        assert_eq!(
            updated.add_repo(dup),
            Err(AppRegistryError::DuplicateId("r1".to_string()))
        );
    }

    #[test]
    fn test_app_registry_get_repo() {
        let registry = AppRegistry::empty();
        let updated = registry
            .add_repo(valid_remote_repo("r1", "https://example.com"))
            .unwrap();
        assert!(updated.get_repo("r1").is_some());
        assert!(updated.get_repo("nonexistent").is_none());
    }

    #[test]
    fn test_registry_to_dict_roundtrip() {
        let registry = AppRegistry::empty();
        let updated = registry
            .add_repo(valid_remote_repo("r1", "https://example.com"))
            .unwrap();
        let dict = registry_to_dict(&updated);
        assert_eq!(
            dict["schema_version"].as_str().unwrap(),
            APP_REGISTRY_SCHEMA_VERSION
        );
        assert_eq!(dict["repos"].as_array().unwrap().len(), 1);
    }

    #[test]
    fn test_registry_save_load_roundtrip() {
        let tmp = tempfile::tempdir().unwrap();
        let path = tmp.path().join("registry.json");
        let updated = AppRegistry::empty()
            .add_repo(valid_remote_repo("r1", "https://example.com"))
            .unwrap();
        updated.save(path.to_str().unwrap()).unwrap();
        let loaded = AppRegistry::load(path.to_str().unwrap()).unwrap();
        assert_eq!(loaded.repos.len(), 1);
        assert_eq!(loaded.repos[0].id, "r1");
    }

    #[test]
    fn test_registry_load_missing_file() {
        let result = AppRegistry::load("/tmp/nonexistent_registry_path_12345.json");
        assert!(result.is_ok());
        assert!(result.unwrap().repos.is_empty());
    }

    #[test]
    fn test_validate_local_repo_path_not_exists() {
        assert_eq!(
            validate_local_repo_path(std::path::Path::new("/nonexistent/path/xyz")),
            Err(AppRegistryError::PathNotDirectory)
        );
    }

    #[test]
    fn test_clean_optional() {
        assert_eq!(clean_optional(None), None);
        assert_eq!(clean_optional(Some("  ".to_string())), None);
        assert_eq!(
            clean_optional(Some(" hello ".to_string())),
            Some("hello".to_string())
        );
    }

    #[test]
    fn test_repo_ref_serialization_skip_none() {
        let repo = valid_remote_repo("r1", "https://example.com");
        let json = serde_json::to_value(&repo).unwrap();
        assert!(!json.as_object().unwrap().contains_key("path"));
        assert!(json.as_object().unwrap().contains_key("url"));
    }
}
