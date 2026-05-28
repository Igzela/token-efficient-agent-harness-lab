use std::collections::BTreeMap;

pub const FIXTURE_TIMESTAMP: &str = "2000-01-01T00:00:00+00:00";

#[derive(Debug, Default)]
pub struct FixtureRuntime {
    counters: BTreeMap<&'static str, usize>,
}

impl FixtureRuntime {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn now(&self) -> String {
        FIXTURE_TIMESTAMP.to_string()
    }

    pub fn id(&mut self, prefix: &'static str) -> String {
        let counter = self.counters.entry(prefix).or_insert(0);
        *counter += 1;
        format!("{prefix}{counter:04}")
    }
}
