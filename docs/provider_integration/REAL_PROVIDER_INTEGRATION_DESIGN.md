# Real Provider Integration Design

**Status**: Design document — not implemented. Does NOT activate CA-8.

## Goals

1. Enable the agent harness to invoke real LLM providers for diagnostic and advisory purposes
2. Establish clear abstraction boundaries between harness and provider
3. Define failure modes, error handling, and audit requirements
4. Support advisor-only critique as the first use case

## Non-Goals

- Full agent autonomy (no file writes, shell execution, sandbox operations)
- Pull request creation, merge, or policy activation
- Real-time routing or provider selection optimization
- Cost optimization beyond budget enforcement
- Multi-provider failover (future consideration)

## Allowed First Use Case

**Advisor-only critique**: The provider receives context and returns diagnostic feedback. The harness:

- Reads provider responses as diagnostic evidence
- Does NOT write files based on provider output
- Does NOT execute shell commands suggested by provider
- Does NOT activate policies based on provider output
- Does NOT create PRs or modify repository state

## Provider Abstraction Boundaries

```
┌─────────────────────────────────────┐
│           Harness Core              │
│  (CA-0 through CA-7)               │
├─────────────────────────────────────┤
│       Provider Abstraction Layer    │  ← Design boundary
│  - Request assembly                │
│  - Response validation             │
│  - Budget enforcement              │
│  - Audit logging                   │
├─────────────────────────────────────┤
│       Provider Interface            │  ← Future implementation
│  - Credential management           │
│  - HTTP/SDK transport              │
│  - Rate limiting                   │
└─────────────────────────────────────┘
```

## Request Lifecycle

### 1. Context Assembly
- Collect relevant project context (files, logs, metrics)
- Apply token budget constraints to context size
- Serialize context in provider-compatible format

### 2. Budget Check
- Verify per-run budget has remaining allocation
- Verify per-request budget within limits
- If no budget available → hard stop, no call made

### 3. Credential Check
- Look for provider credentials in environment variables only
- If credential unavailable → fail closed, no call made
- Never log or expose raw credentials

### 4. Request Construction
- Build provider request per schema in `PROVIDER_REQUEST_RESPONSE_CONTRACT.md`
- Set allowed_tools to empty for advisor-only mode
- Set output_ref to diagnostic only

### 5. Provider Call
- Invoke provider via abstraction layer
- Enforce request timeout
- Capture full response for audit

### 6. Response Validation
- Validate response structure against contract
- Check for injection attempts or malformed output
- Extract diagnostic findings

### 7. Usage Ledger
- Record request_id, model_profile_id, token counts
- Record cost estimate (if available)
- Never record raw prompts or responses in ledger

### 8. Error Classification
- Classify errors per failure handling section below
- Emit diagnostic evidence for each error class

### 9. Diagnostic Evidence Emission
- Provider responses become diagnostic evidence
- Evidence cannot directly activate policies
- Evidence informs human review decisions

## Failure Handling

| Error Class | Description | Harness Behavior |
|------------|-------------|-----------------|
| `provider_error` | Provider returned error status | Log error, emit diagnostic evidence, no retry |
| `timeout` | Request exceeded time limit | Log timeout, emit diagnostic evidence, no retry |
| `invalid_response` | Response doesn't match contract | Log validation failure, emit diagnostic evidence |
| `budget_exceeded` | Per-run or per-request budget hit | Hard stop, no call made |
| `credential_unavailable` | No credential in environment | Fail closed, no call made |
| `policy_denied` | Request violates policy constraints | Log denial, emit diagnostic evidence |

## Audit Requirements

Every provider invocation must record:

- `request_id`: Unique identifier for the request
- `model_profile_id`: Which model was invoked
- `timestamp`: When the request was made
- `token_input`: Input token count
- `token_output`: Output token count
- `cost_estimate`: Estimated cost (if available)
- `error_class`: If request failed (from table above)
- `diagnostic_evidence_summary`: High-level summary of findings

### What MUST NOT Be Logged

- Raw API keys or credentials
- Full prompt content
- Full provider response content
- Any secret or sensitive material

## Provider Response as Diagnostic Evidence

Provider responses are treated as diagnostic evidence by default:

- They inform human review decisions
- They cannot directly activate policies
- They cannot trigger file writes or shell execution
- They are subject to human approval before any action

This ensures the provider remains advisory, not autonomous.
