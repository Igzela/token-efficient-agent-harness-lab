# CC Switch adaptation inventory (observation only)

This document records the pre-port inventory required before reusing
`farion1231/cc-switch` for **protocol compatibility and usage observation**.
It does **not** authorize live provider spend, merge, or Architecture Convergence.

## 1. Exact upstream commit

| Field | Value |
|-------|--------|
| Repository | `https://github.com/farion1231/cc-switch` |
| Commit SHA | `878c26f31e012ba32b9772bd080bd4fa9e7d495e` |
| Commit date | 2026-07-24 12:12:22 +0800 |
| License | MIT (Copyright (c) 2025 Jason Young) |

## 2. Source files / functions used (adapted)

| CC Switch path (at SHA above) | Symbols | Use in this repo |
|------------------------------|---------|------------------|
| `src-tauri/src/proxy/usage/parser.rs` | `TokenUsage`, `openai_cache_*`, `from_claude_response`, `from_claude_stream_events`, `from_codex_response*`, `from_openai_response`, `from_openai_stream_events`, `from_gemini_response`, `from_gemini_stream_chunks`, `from_codex_response_auto`, stream auto-detect | Adapted into `execution_usage/protocol_usage.rs` as evidence extractors |
| `src-tauri/src/proxy/usage/calculator.rs` | `CostCalculator`, `ModelPricing`, cache-inclusive vs exclusive input semantics | Adapted into `execution_usage/pricing_estimate.rs` as **estimate-only** math |
| `src-tauri/src/services/session_usage_codex.rs` | `normalize_codex_model` | Adapted into `execution_usage/model_normalize.rs` |
| `src-tauri/src/proxy/thinking_optimizer.rs` | `normalize_model_name` (lowercase, `.`/`_` → `-`) | Combined into model normalize helpers |
| `src-tauri/src/proxy/gemini_url.rs` | `normalize_gemini_model_id` (strip `models/` prefix) | Combined into model normalize helpers |
| `src-tauri/src/services/provider/endpoints.rs` + proxy path handling | endpoint/path recognition patterns for `/v1/responses`, `/v1/chat/completions`, Anthropic messages, Gemini generateContent | Adapted into `execution_usage/endpoint_identity.rs` for **admission binding checks only** |

### Explicitly **not** imported

| CC Switch surface | Reason |
|-------------------|--------|
| OAuth / account switching / `codex_oauth*` / `xai_oauth*` | Forbidden credential/account authority |
| Credential persistence / scraping | Secrets boundary |
| Codex OAuth reverse proxy | Parallel proxy authority |
| Automatic failover / circuit breaker as spend gate | Implicit retries / authority bypass |
| Caller-configured authorization / CLI config as product authority | Conflicts with store-owned principal + spend |
| `proxy_request_logs` / session logs as pre-call authorization | Logs are corroboration only |
| Desktop UI state as execution approval | Not product authority |
| CC Switch proxy tables as ProductTask budget owner | ProductTask remains sole budget owner |
| Full proxy server (`proxy/server.rs`, failover, live switch) | Second runtime |

## 3. MIT attribution requirements

- The MIT license requires that the copyright notice and permission notice be
  included in all copies or substantial portions of the Software.
- This repository satisfies that by:
  1. `THIRD_PARTY_NOTICES.md` (full MIT text + commit SHA);
  2. SPDX / copyright headers on adapted source files under `engine/src/execution_usage/`.
- Adapted code is not a verbatim copy of entire modules; it is rewritten to our
  types (`ExecutionUsageEventV1`, `CostAuthority`) and authority contracts.

## 4. Semantic differences from this repository

| Topic | CC Switch | This repository |
|-------|-----------|-----------------|
| Runtime owner | Desktop Tauri app + embedded proxy | Rust `engine/` sole runtime |
| Persistence | CC Switch SQLite (`proxy_request_logs`, model pricing rows) | `LocalProductStore` sole app store |
| Budget | Proxy logs + optional limits | **ProductTask** sole budget; gateway journal for mediated Codex |
| Usage primary source | Proxy measurement when proxy mode | **Gateway measurement primary**; session JSONL corroborating only |
| Cost | Local pricing + multipliers drive UI totals | Typed `CostAuthority`: `provider_reported` \| `local_estimate` \| `cost_unavailable`; estimates never invent provider billing |
| Auth | OAuth, multi-account, CLI config | Canonical API-key metadata principal; risk ack ≠ one-use spend |
| Retries | Failover / implicit retry paths exist | `max_retries=0` while Codex retry identity unproved |
| Child credentials | Proxy may hold upstream keys | No reusable credential in child (session token only) |
| Evidence content | May retain request metadata for UI | No raw prompt/output/transcript in durable evidence |

## 5. Tests proving authority boundaries

See unit tests in:

- `execution_usage/protocol_usage.rs` — multi-format parse without storing content fields; canonical disjoint buckets (no cache/reasoning double-count);
- `execution_usage/pricing_estimate.rs` — estimate is tagged estimate-only; missing table → `cost_unavailable`;
- `execution_usage/model_normalize.rs` — normalization is identity hygiene only;
- `execution_usage/endpoint_identity.rs` — path classification does not authorize spend;
- `execution_usage/provider_adapter.rs` — no invented `bound_provider`; no request-id collapse onto `managed_execution_id`; estimate never provider-reported;
- boundary tests assert adapted helpers never set `CostSource::ProviderOrExecutorReported` from local tables, never claim budget authority, and never read OAuth/CLI config.

## 6. Non-claims

- Does not replace #299/#300 managed-acceptance or RWE authority repair.
- Does not open Architecture Convergence, Level-2, Meta, OpenCode, Vader, or Issue #208.
- Does not perform live provider requests.
- Does not merge PRs.
