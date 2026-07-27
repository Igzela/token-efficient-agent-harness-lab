//! Versioned local pricing-table **estimates** for usage evidence.
//!
//! Cost math adapted from MIT-licensed CC Switch
//! (`farion1231/cc-switch@878c26f31e012ba32b9772bd080bd4fa9e7d495e`)
//! `src-tauri/src/proxy/usage/calculator.rs` (cache-inclusive vs exclusive
//! input semantics). Rewritten without Decimal dependency and without treating
//! estimates as provider billing receipts.
//!
//! Copyright (c) 2025 Jason Young — see `THIRD_PARTY_NOTICES.md`.
//!
//! **Authority boundary:**
//! - Estimates never set `CostSource::ProviderOrExecutorReported`.
//! - Missing table entry → estimate unavailable (not zero).
//! - Does not own ProductTask budget or gateway enforcement.

use super::model_normalize::normalize_for_pricing_lookup;
use super::protocol_usage::{InputTokenSemantics, ProtocolTokenUsage};
use super::CostSource;

/// Version id for the in-tree estimate table. Bump when rates change.
pub const LOCAL_PRICING_TABLE_VERSION: &str = "local_pricing_table.v1.2026-07-25";

#[derive(Debug, Clone, PartialEq)]
pub struct ModelRateRow {
    pub model_key: &'static str,
    pub provider: &'static str,
    /// USD per 1_000_000 tokens.
    pub input_per_million_usd: f64,
    pub output_per_million_usd: f64,
    pub cache_read_per_million_usd: f64,
    pub cache_creation_per_million_usd: f64,
    pub input_semantics: InputTokenSemantics,
}

/// Small explicit table for tests and offline estimates. Not a live catalog scrape.
pub const LOCAL_PRICING_ROWS: &[ModelRateRow] = &[
    ModelRateRow {
        model_key: "gpt-test-model",
        provider: "openai",
        input_per_million_usd: 1.0,
        output_per_million_usd: 2.0,
        cache_read_per_million_usd: 0.1,
        cache_creation_per_million_usd: 1.25,
        input_semantics: InputTokenSemantics::InputIncludesCache,
    },
    ModelRateRow {
        model_key: "claude-test-model",
        provider: "anthropic",
        input_per_million_usd: 3.0,
        output_per_million_usd: 15.0,
        cache_read_per_million_usd: 0.3,
        cache_creation_per_million_usd: 3.75,
        input_semantics: InputTokenSemantics::InputExcludesCache,
    },
];

#[derive(Debug, Clone, PartialEq)]
pub struct CostEstimateBreakdown {
    pub pricing_table_version: String,
    pub model_key: String,
    pub provider: String,
    pub input_cost_usd: f64,
    pub output_cost_usd: f64,
    pub cache_read_cost_usd: f64,
    pub cache_creation_cost_usd: f64,
    pub total_cost_usd: f64,
    pub cost_source: CostSource,
    pub estimate_semantics: String,
}

pub fn lookup_local_pricing(model: &str) -> Option<&'static ModelRateRow> {
    let key = normalize_for_pricing_lookup(model);
    LOCAL_PRICING_ROWS.iter().find(|r| r.model_key == key)
}

/// Estimate cost from usage + versioned local table. Never claims provider-reported money.
pub fn estimate_cost_usd(usage: &ProtocolTokenUsage, model: &str) -> Option<CostEstimateBreakdown> {
    let row = lookup_local_pricing(model)?;
    let million = 1_000_000.0_f64;
    let billable_input = match row.input_semantics {
        InputTokenSemantics::InputIncludesCache => usage
            .input_tokens
            .saturating_sub(usage.cache_read_tokens)
            .saturating_sub(usage.cache_creation_tokens),
        InputTokenSemantics::InputExcludesCache => usage.input_tokens,
    };
    let input_cost = (billable_input as f64) * row.input_per_million_usd / million;
    let output_cost = (usage.output_tokens as f64) * row.output_per_million_usd / million;
    let cache_read_cost =
        (usage.cache_read_tokens as f64) * row.cache_read_per_million_usd / million;
    let cache_creation_cost =
        (usage.cache_creation_tokens as f64) * row.cache_creation_per_million_usd / million;
    let total = input_cost + output_cost + cache_read_cost + cache_creation_cost;
    Some(CostEstimateBreakdown {
        pricing_table_version: LOCAL_PRICING_TABLE_VERSION.into(),
        model_key: row.model_key.into(),
        provider: row.provider.into(),
        input_cost_usd: input_cost,
        output_cost_usd: output_cost,
        cache_read_cost_usd: cache_read_cost,
        cache_creation_cost_usd: cache_creation_cost,
        total_cost_usd: total,
        cost_source: CostSource::Estimated,
        estimate_semantics: format!(
            "local_table_{}_{:?}",
            LOCAL_PRICING_TABLE_VERSION, row.input_semantics
        ),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unknown_model_is_unavailable_not_zero() {
        let usage = ProtocolTokenUsage {
            input_tokens: 1000,
            output_tokens: 100,
            ..Default::default()
        };
        assert!(estimate_cost_usd(&usage, "totally-unknown-model").is_none());
    }

    #[test]
    fn anthropic_excludes_cache_from_input_bucket() {
        let usage = ProtocolTokenUsage {
            input_tokens: 1000,
            output_tokens: 500,
            cache_read_tokens: 200,
            cache_creation_tokens: 100,
            ..Default::default()
        };
        let est = estimate_cost_usd(&usage, "claude-test-model").unwrap();
        assert_eq!(est.cost_source, CostSource::Estimated);
        assert!((est.input_cost_usd - 0.003).abs() < 1e-12);
        assert!((est.output_cost_usd - 0.0075).abs() < 1e-12);
        assert!((est.total_cost_usd - 0.010935).abs() < 1e-12);
        assert_ne!(est.cost_source, CostSource::ProviderOrExecutorReported);
    }

    #[test]
    fn openai_includes_cache_in_input_bucket() {
        let usage = ProtocolTokenUsage {
            input_tokens: 1000,
            output_tokens: 500,
            cache_read_tokens: 200,
            cache_creation_tokens: 100,
            ..Default::default()
        };
        let est = estimate_cost_usd(&usage, "openai/gpt-test-model").unwrap();
        // billable input = 1000 - 200 - 100 = 700 → 700 * 1.0 / 1e6
        assert!((est.input_cost_usd - 0.0007).abs() < 1e-12);
        assert_eq!(est.pricing_table_version, LOCAL_PRICING_TABLE_VERSION);
    }
}
