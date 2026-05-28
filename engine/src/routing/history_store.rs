use std::collections::HashMap;

use super::schemas::{
    aggregate_cost_of_pass, make_task_group, parse_cost_of_pass_group, CostOfPassAggregate,
    UsageLedgerRow,
};

#[derive(Debug, Default)]
pub struct RoutingHistoryStore {
    rows: Vec<UsageLedgerRow>,
    tier_map: HashMap<String, String>,
    group_cache: HashMap<String, Vec<usize>>,
    tier_cache: HashMap<String, Vec<usize>>,
    dirty: bool,
}

impl RoutingHistoryStore {
    pub fn new(tier_profile_map: Option<HashMap<String, String>>) -> Self {
        Self {
            tier_map: tier_profile_map.unwrap_or_default(),
            dirty: true,
            ..Default::default()
        }
    }

    pub fn add_row(&mut self, row: UsageLedgerRow) {
        self.rows.push(row);
        self.dirty = true;
    }

    pub fn set_tier_map(&mut self, mapping: HashMap<String, String>) {
        self.tier_map = mapping;
        self.dirty = true;
    }

    pub fn tier_for_profile(&self, profile_id: &str) -> Option<&str> {
        self.tier_map.get(profile_id).map(|s| s.as_str())
    }

    fn rebuild_caches(&mut self) {
        if !self.dirty {
            return;
        }
        self.group_cache.clear();
        self.tier_cache.clear();
        for (idx, row) in self.rows.iter().enumerate() {
            if let Some(tg) = task_group_from_row(row) {
                self.group_cache.entry(tg).or_default().push(idx);
            }
            if let Some(tier) = self.tier_map.get(&row.model_profile_id) {
                self.tier_cache.entry(tier.clone()).or_default().push(idx);
            }
        }
        self.dirty = false;
    }

    pub fn rows_by_tier(&mut self, tier: &str) -> Vec<&UsageLedgerRow> {
        self.rebuild_caches();
        self.tier_cache
            .get(tier)
            .map(|indices| indices.iter().map(|&i| &self.rows[i]).collect())
            .unwrap_or_default()
    }

    pub fn rows_by_task_group(&mut self, task_group: &str) -> Vec<&UsageLedgerRow> {
        self.rebuild_caches();
        self.group_cache
            .get(task_group)
            .map(|indices| indices.iter().map(|&i| &self.rows[i]).collect())
            .unwrap_or_default()
    }

    pub fn rows_by_tier_and_task_group(
        &mut self,
        tier: &str,
        task_group: &str,
    ) -> Vec<&UsageLedgerRow> {
        self.rebuild_caches();
        let tier_indices: std::collections::HashSet<usize> = self
            .tier_cache
            .get(tier)
            .map(|v| v.iter().copied().collect())
            .unwrap_or_default();
        let group_indices: std::collections::HashSet<usize> = self
            .group_cache
            .get(task_group)
            .map(|v| v.iter().copied().collect())
            .unwrap_or_default();
        tier_indices
            .intersection(&group_indices)
            .map(|&i| &self.rows[i])
            .collect()
    }

    pub fn aggregate_by_tier(&mut self, tier: &str) -> Option<CostOfPassAggregate> {
        let rows: Vec<UsageLedgerRow> = self.rows_by_tier(tier).into_iter().cloned().collect();
        aggregate_cost_of_pass(&rows)
    }

    pub fn aggregate_by_tier_and_task_group(
        &mut self,
        tier: &str,
        task_group: &str,
    ) -> Option<CostOfPassAggregate> {
        let rows: Vec<UsageLedgerRow> = self
            .rows_by_tier_and_task_group(tier, task_group)
            .into_iter()
            .cloned()
            .collect();
        aggregate_cost_of_pass(&rows)
    }

    pub fn sample_count(&mut self, task_group: &str) -> usize {
        self.rows_by_task_group(task_group).len()
    }

    pub fn sample_count_for_tier(&mut self, task_group: &str, tier: &str) -> usize {
        self.rows_by_tier_and_task_group(tier, task_group).len()
    }

    pub fn tiers_observed(&mut self, task_group: &str) -> Vec<String> {
        self.rebuild_caches();
        let group_indices: std::collections::HashSet<usize> = self
            .group_cache
            .get(task_group)
            .map(|v| v.iter().copied().collect())
            .unwrap_or_default();
        let mut tiers: std::collections::BTreeSet<String> = std::collections::BTreeSet::new();
        for &idx in &group_indices {
            if let Some(tier) = self.tier_map.get(&self.rows[idx].model_profile_id) {
                tiers.insert(tier.clone());
            }
        }
        tiers.into_iter().collect()
    }

    pub fn total_rows(&self) -> usize {
        self.rows.len()
    }
}

fn task_group_from_row(row: &UsageLedgerRow) -> Option<String> {
    let (_, family, variant, _) = parse_cost_of_pass_group(&row.cost_of_pass_group);
    if family.is_empty() || variant.is_empty() {
        return None;
    }
    Some(make_task_group(&family, &variant))
}
