use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::HashMap;

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct SkillRecord {
    pub skill_id: String,
    pub name: String,
    pub content: String,
    pub content_hash: String,
    pub tags: Vec<String>,
    pub created_at: String,
}

pub struct SkillStore {
    skills: HashMap<String, SkillRecord>,
}

impl Default for SkillStore {
    fn default() -> Self {
        Self::new()
    }
}

impl SkillStore {
    pub fn new() -> Self {
        Self {
            skills: HashMap::new(),
        }
    }

    pub fn add(&mut self, name: &str, content: &str, tags: Vec<String>) -> SkillRecord {
        let hash = {
            let mut h = Sha256::new();
            h.update(content.as_bytes());
            hex::encode(&h.finalize()[..8])
        };
        let skill_id = format!("skill-{}", hash);
        let record = SkillRecord {
            skill_id: skill_id.clone(),
            name: name.into(),
            content: content.into(),
            content_hash: hash,
            tags,
            created_at: chrono::Utc::now().to_rfc3339(),
        };
        self.skills.insert(skill_id, record.clone());
        record
    }

    pub fn get(&self, id: &str) -> Option<&SkillRecord> {
        self.skills.get(id)
    }
    pub fn list(&self) -> Vec<&SkillRecord> {
        self.skills.values().collect()
    }
    pub fn len(&self) -> usize {
        self.skills.len()
    }
    pub fn is_empty(&self) -> bool {
        self.skills.is_empty()
    }

    pub fn search(&self, query: &str) -> Vec<&SkillRecord> {
        let q = query.to_lowercase();
        self.skills
            .values()
            .filter(|s| {
                s.name.to_lowercase().contains(&q)
                    || s.tags.iter().any(|t| t.to_lowercase().contains(&q))
            })
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn add_and_get() {
        let mut s = SkillStore::new();
        let sk = s.add("test", "content", vec!["t1".into()]);
        assert_eq!(s.get(&sk.skill_id).unwrap().name, "test");
    }

    #[test]
    fn deterministic_id() {
        assert_eq!(
            SkillStore::new().add("a", "c", vec![]).skill_id,
            SkillStore::new().add("a", "c", vec![]).skill_id
        );
    }

    #[test]
    fn search_by_name() {
        let mut s = SkillStore::new();
        s.add("code_review", "x", vec![]);
        s.add("doc_gen", "y", vec![]);
        assert_eq!(s.search("review").len(), 1);
    }

    #[test]
    fn search_by_tag() {
        let mut s = SkillStore::new();
        s.add("s1", "c", vec!["rust".into()]);
        assert_eq!(s.search("rust").len(), 1);
    }

    #[test]
    fn list_all() {
        let mut s = SkillStore::new();
        s.add("a", "content_a", vec![]);
        s.add("b", "content_b", vec![]);
        assert_eq!(s.len(), 2);
    }
}
