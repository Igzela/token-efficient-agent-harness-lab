//! Model-name normalization for pricing lookup and identity hygiene.
//!
//! Adapted from MIT-licensed CC Switch
//! (`farion1231/cc-switch@878c26f31e012ba32b9772bd080bd4fa9e7d495e`):
//! - `src-tauri/src/services/session_usage_codex.rs` (`normalize_codex_model`)
//! - `src-tauri/src/proxy/thinking_optimizer.rs` (`normalize_model_name`)
//! - `src-tauri/src/proxy/gemini_url.rs` (`normalize_gemini_model_id`)
//! - `src-tauri/src/model_capabilities.rs` (`normalize_model_id`)
//!
//! Copyright (c) 2025 Jason Young — see `THIRD_PARTY_NOTICES.md`.
//!
//! **Authority boundary:** normalization never authorizes spend, selects a
//! provider credential, or mutates ProductTask budgets.

/// Normalize a Codex/OpenAI-style model id for pricing table lookup.
pub fn normalize_codex_model(raw: &str) -> String {
    let mut name = raw.trim().to_ascii_lowercase();
    if let Some(pos) = name.rfind('/') {
        name = name[pos + 1..].to_string();
    }
    // Strip ISO date suffix -YYYY-MM-DD (11 chars).
    if name.len() > 11 {
        let suffix = &name[name.len() - 11..];
        if suffix.as_bytes().first() == Some(&b'-')
            && suffix.is_ascii()
            && suffix[1..5].chars().all(|c| c.is_ascii_digit())
            && suffix.as_bytes().get(5) == Some(&b'-')
            && suffix[6..8].chars().all(|c| c.is_ascii_digit())
            && suffix.as_bytes().get(8) == Some(&b'-')
            && suffix[9..11].chars().all(|c| c.is_ascii_digit())
        {
            name.truncate(name.len() - 11);
        }
    }
    // Strip compact date suffix -YYYYMMDD.
    if name.len() > 9 {
        let parts: Vec<&str> = name.rsplitn(2, '-').collect();
        if parts.len() == 2 {
            let suffix = parts[0];
            if suffix.len() == 8 && suffix.chars().all(|c| c.is_ascii_digit()) {
                name = parts[1].to_string();
            }
        }
    }
    name
}

/// Lowercase and map `.` / `_` to `-` (capability matching hygiene).
pub fn normalize_model_slug(raw: &str) -> String {
    raw.trim().to_ascii_lowercase().replace(['.', '_'], "-")
}

/// Strip Gemini `models/` prefix if present.
pub fn normalize_gemini_model_id(raw: &str) -> String {
    let trimmed = raw.trim().trim_start_matches('/').to_ascii_lowercase();
    trimmed
        .strip_prefix("models/")
        .unwrap_or(&trimmed)
        .to_string()
}

/// Unified normalize for estimate-table keys.
pub fn normalize_for_pricing_lookup(raw: &str) -> String {
    let gemini = normalize_gemini_model_id(raw);
    normalize_codex_model(&gemini)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn strips_provider_prefix_and_dates() {
        assert_eq!(normalize_codex_model("OPENAI/GPT-5.4"), "gpt-5.4");
        assert_eq!(normalize_codex_model("gpt-5.4-2026-03-05"), "gpt-5.4");
        assert_eq!(normalize_codex_model("gpt-5.4-20260305"), "gpt-5.4");
    }

    #[test]
    fn gemini_prefix_and_slug() {
        assert_eq!(
            normalize_gemini_model_id("models/gemini-2.0-flash"),
            "gemini-2.0-flash"
        );
        assert_eq!(normalize_model_slug("GPT_5.4"), "gpt-5-4");
    }

    #[test]
    fn pricing_lookup_is_not_authorization() {
        // Documented non-claim: normalize returns a string only.
        let n = normalize_for_pricing_lookup("openai/gpt-test-model");
        assert_eq!(n, "gpt-test-model");
        assert!(!n.contains("spend") && !n.contains("auth"));
    }
}
