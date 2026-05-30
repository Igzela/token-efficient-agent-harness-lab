use serde_json::Value;

use super::dag_types::DagState;
use super::helpers::{blocking_reason, conflicting_files, item_id};
use super::types::{FileOverlap, ScheduleBatch};

pub struct ConcurrencyController {
    pub max_concurrent: usize,
}

impl Default for ConcurrencyController {
    fn default() -> Self {
        Self { max_concurrent: 4 }
    }
}

impl ConcurrencyController {
    pub fn new(max_concurrent: usize) -> Self {
        if max_concurrent < 1 {
            panic!("max_concurrent must be at least 1");
        }
        Self { max_concurrent }
    }

    pub fn schedule(
        &self,
        ready_items: &[Value],
        dag: &DagState,
        active_claims: &[Value],
    ) -> ScheduleBatch {
        if ready_items.is_empty() {
            return ScheduleBatch::default();
        }

        let overlaps = self.detect_file_overlaps(ready_items);
        let mut blocked: Vec<Value> = Vec::new();
        let mut eligible: Vec<Value> = Vec::new();
        let mut warnings: Vec<String> = Vec::new();

        let mut sorted_items = ready_items.to_vec();
        sorted_items.sort_by_key(item_id);

        for item in sorted_items {
            if let Some(reason) = blocking_reason(&item, dag, active_claims) {
                blocked.push(item.clone());
                warnings.push(reason);
            } else {
                eligible.push(item);
            }
        }

        let mut scheduled: Vec<Value> = Vec::new();
        for item in eligible {
            if scheduled.len() >= self.max_concurrent {
                warnings.push(format!("{} exceeds max_concurrent", item_id(&item)));
                blocked.push(item);
                continue;
            }
            if scheduled
                .iter()
                .all(|existing| self.can_run_parallel(existing, &item, &overlaps))
            {
                scheduled.push(item);
            } else {
                warnings.push(format!(
                    "{} conflicts with scheduled file claims",
                    item_id(&item)
                ));
                blocked.push(item);
            }
        }

        ScheduleBatch {
            scheduled_items: scheduled,
            blocked_items: blocked,
            file_overlaps: overlaps,
            warnings,
        }
    }

    pub fn detect_file_overlaps(&self, items: &[Value]) -> Vec<FileOverlap> {
        let mut overlaps = Vec::new();
        let mut sorted_items = items.to_vec();
        sorted_items.sort_by_key(item_id);

        for (index, item_a) in sorted_items.iter().enumerate() {
            for item_b in sorted_items.iter().skip(index + 1) {
                let files = conflicting_files(item_a, item_b);
                if !files.is_empty() {
                    overlaps.push(FileOverlap {
                        item_a_id: item_id(item_a),
                        item_b_id: item_id(item_b),
                        files,
                    });
                }
            }
        }
        overlaps
    }

    pub fn can_run_parallel(
        &self,
        item_a: &Value,
        item_b: &Value,
        overlaps: &[FileOverlap],
    ) -> bool {
        let mut pair = vec![item_id(item_a), item_id(item_b)];
        pair.sort();
        for overlap in overlaps {
            let mut overlap_pair = vec![overlap.item_a_id.clone(), overlap.item_b_id.clone()];
            overlap_pair.sort();
            if pair == overlap_pair {
                return false;
            }
        }
        true
    }
}
