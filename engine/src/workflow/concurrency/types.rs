use serde::{Deserialize, Serialize};
use serde_json::Value;

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct FileOverlap {
    pub item_a_id: String,
    pub item_b_id: String,
    pub files: Vec<String>,
}

impl Default for FileOverlap {
    fn default() -> Self {
        Self {
            item_a_id: String::new(),
            item_b_id: String::new(),
            files: Vec::new(),
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct ScheduleBatch {
    pub scheduled_items: Vec<Value>,
    pub blocked_items: Vec<Value>,
    pub file_overlaps: Vec<FileOverlap>,
    pub warnings: Vec<String>,
}

impl Default for ScheduleBatch {
    fn default() -> Self {
        Self {
            scheduled_items: Vec::new(),
            blocked_items: Vec::new(),
            file_overlaps: Vec::new(),
            warnings: Vec::new(),
        }
    }
}

impl ScheduleBatch {
    pub fn item_ids(&self) -> Vec<String> {
        self.scheduled_items
            .iter()
            .map(super::helpers::item_id)
            .collect()
    }
}
