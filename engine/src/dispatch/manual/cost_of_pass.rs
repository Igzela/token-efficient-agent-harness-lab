use std::collections::HashMap;

pub struct CostOfPassAccumulator {
    rows: Vec<serde_json::Value>,
}

impl Default for CostOfPassAccumulator {
    fn default() -> Self {
        Self::new()
    }
}

impl CostOfPassAccumulator {
    pub fn new() -> Self {
        Self { rows: Vec::new() }
    }

    pub fn add(&mut self, row: serde_json::Value) {
        self.rows.push(row);
    }

    pub fn total_cost(&self) -> f64 {
        self.rows
            .iter()
            .filter_map(|r| r.get("estimated_cost").and_then(|v| v.as_f64()))
            .sum()
    }

    pub fn total_rows(&self) -> usize {
        self.rows.len()
    }

    pub fn success_rate(&self) -> f64 {
        if self.rows.is_empty() {
            return 0.0;
        }
        let successes = self
            .rows
            .iter()
            .filter(|r| r.get("pass").and_then(|v| v.as_bool()).unwrap_or(false))
            .count();
        successes as f64 / self.rows.len() as f64
    }

    pub fn group_by(&self, key: &str) -> HashMap<String, Vec<&serde_json::Value>> {
        let mut groups: HashMap<String, Vec<&serde_json::Value>> = HashMap::new();
        for row in &self.rows {
            let group = row
                .get(key)
                .and_then(|v| v.as_str())
                .unwrap_or("unknown")
                .to_string();
            groups.entry(group).or_default().push(row);
        }
        groups
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_accumulator() {
        let acc = CostOfPassAccumulator::new();
        assert_eq!(acc.total_rows(), 0);
        assert_eq!(acc.total_cost(), 0.0);
        assert_eq!(acc.success_rate(), 0.0);
    }

    #[test]
    fn add_and_count() {
        let mut acc = CostOfPassAccumulator::new();
        acc.add(serde_json::json!({"estimated_cost": 0.5, "pass": true}));
        acc.add(serde_json::json!({"estimated_cost": 0.3, "pass": false}));
        assert_eq!(acc.total_rows(), 2);
    }

    #[test]
    fn total_cost() {
        let mut acc = CostOfPassAccumulator::new();
        acc.add(serde_json::json!({"estimated_cost": 1.5}));
        acc.add(serde_json::json!({"estimated_cost": 2.5}));
        assert!((acc.total_cost() - 4.0).abs() < 0.001);
    }

    #[test]
    fn success_rate() {
        let mut acc = CostOfPassAccumulator::new();
        acc.add(serde_json::json!({"pass": true}));
        acc.add(serde_json::json!({"pass": true}));
        acc.add(serde_json::json!({"pass": false}));
        assert!((acc.success_rate() - 2.0 / 3.0).abs() < 0.01);
    }

    #[test]
    fn group_by_key() {
        let mut acc = CostOfPassAccumulator::new();
        acc.add(serde_json::json!({"group": "a", "v": 1}));
        acc.add(serde_json::json!({"group": "b", "v": 2}));
        acc.add(serde_json::json!({"group": "a", "v": 3}));
        let groups = acc.group_by("group");
        assert_eq!(groups["a"].len(), 2);
        assert_eq!(groups["b"].len(), 1);
    }
}
