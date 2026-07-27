//! Provider endpoint / path recognition for admission binding checks.
//!
//! Inspired by path handling in MIT-licensed CC Switch
//! (`farion1231/cc-switch@878c26f31e012ba32b9772bd080bd4fa9e7d495e`) proxy/provider
//! routing, rewritten to classify paths only.
//!
//! Copyright (c) 2025 Jason Young — see `THIRD_PARTY_NOTICES.md`.
//!
//! **Authority boundary:** classifying a path never authorizes a live call,
//! expands admitted paths, or weakens gateway/ProductTask budgets.

/// Recognized protocol surface for usage extraction / admission matching.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProviderEndpointKind {
    OpenAiResponses,
    OpenAiChatCompletions,
    AnthropicMessages,
    GeminiGenerateContent,
    Unknown,
}

impl ProviderEndpointKind {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::OpenAiResponses => "openai_responses",
            Self::OpenAiChatCompletions => "openai_chat_completions",
            Self::AnthropicMessages => "anthropic_messages",
            Self::GeminiGenerateContent => "gemini_generate_content",
            Self::Unknown => "unknown",
        }
    }
}

/// Classify a request path (may include query string).
pub fn classify_provider_path(path: &str) -> ProviderEndpointKind {
    let path = path.split('?').next().unwrap_or(path).to_ascii_lowercase();
    let path = path.trim_end_matches('/');
    if path.ends_with("/v1/responses") || path == "/responses" {
        return ProviderEndpointKind::OpenAiResponses;
    }
    if path.ends_with("/v1/chat/completions") || path.ends_with("/chat/completions") {
        return ProviderEndpointKind::OpenAiChatCompletions;
    }
    if path.ends_with("/v1/messages") || path.ends_with("/messages") {
        return ProviderEndpointKind::AnthropicMessages;
    }
    if path.contains(":generatecontent") || path.contains("/generatecontent") {
        return ProviderEndpointKind::GeminiGenerateContent;
    }
    ProviderEndpointKind::Unknown
}

/// True when `path` is exactly one of the predeclared admitted paths
/// (literal match after stripping query). Does not invent new admissions.
pub fn path_is_admitted(path: &str, admitted_paths: &[String]) -> bool {
    let normalized = path
        .split('?')
        .next()
        .unwrap_or(path)
        .trim_end_matches('/')
        .to_string();
    admitted_paths.iter().any(|a| {
        let a = a.split('?').next().unwrap_or(a).trim_end_matches('/');
        a == normalized
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn classifies_common_paths() {
        assert_eq!(
            classify_provider_path("/v1/responses"),
            ProviderEndpointKind::OpenAiResponses
        );
        assert_eq!(
            classify_provider_path("/v1/chat/completions?stream=true"),
            ProviderEndpointKind::OpenAiChatCompletions
        );
        assert_eq!(
            classify_provider_path("/v1/messages"),
            ProviderEndpointKind::AnthropicMessages
        );
        assert_eq!(
            classify_provider_path("/v1beta/models/gemini-2.0-flash:generateContent"),
            ProviderEndpointKind::GeminiGenerateContent
        );
        assert_eq!(
            classify_provider_path("/admin/reset"),
            ProviderEndpointKind::Unknown
        );
    }

    #[test]
    fn admission_is_literal_not_prefix_expansion() {
        let admitted = vec!["/v1/responses".into()];
        assert!(path_is_admitted("/v1/responses", &admitted));
        assert!(!path_is_admitted("/v1/responses/admin", &admitted));
        assert!(!path_is_admitted("/v1/chat/completions", &admitted));
    }
}
