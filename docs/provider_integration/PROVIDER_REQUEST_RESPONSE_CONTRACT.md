# Provider Request/Response Contract

**Status**: Design document — future schema definition.

## Overview

This document defines the schema for provider requests and responses. These schemas are designed for the advisor-only first use case (CA-8) and will evolve as the provider integration matures.

## Provider Request Schema

```json
{
  "request_id": "string (UUID)",
  "model_profile_id": "string",
  "timestamp": "ISO 8601",
  "context": {
    "project_summary": "string",
    "relevant_files": ["string"],
    "diagnostic_evidence": ["string"],
    "metrics": {}
  },
  "allowed_tools": [],
  "output_ref": "diagnostic",
  "constraints": {
    "max_tokens": "integer",
    "temperature": "number"
  }
}
```

### Field Definitions

| Field | Type | Description |
|-------|------|-------------|
| `request_id` | string (UUID) | Unique identifier for this request |
| `model_profile_id` | string | Which model profile to invoke |
| `timestamp` | string (ISO 8601) | When the request was constructed |
| `context.project_summary` | string | Brief project description |
| `context.relevant_files` | string[] | Files relevant to the diagnostic query |
| `context.diagnostic_evidence` | string[] | Prior diagnostic findings |
| `context.metrics` | object | Performance/quality metrics |
| `allowed_tools` | array | **Empty for CA-8 advisor-only mode** |
| `output_ref` | string | **"diagnostic" only — output is diagnostic evidence** |
| `constraints.max_tokens` | integer | Maximum response tokens |
| `constraints.temperature` | number | Sampling temperature |

### Advisor-Only Constraints

- `allowed_tools` MUST be empty `[]` — provider cannot invoke tools
- `output_ref` MUST be `"diagnostic"` — output is evidence only
- Provider response CANNOT directly update policy or activate actions

## Provider Response Schema

```json
{
  "request_id": "string (UUID)",
  "response_id": "string (UUID)",
  "model_profile_id": "string",
  "timestamp": "ISO 8601",
  "status": "success | error | timeout",
  "diagnostic": {
    "findings": ["string"],
    "confidence": "number (0-1)",
    "recommendations": ["string"],
    "risks": ["string"]
  },
  "usage": {
    "token_input": "integer",
    "token_output": "integer",
    "cost_estimate": "number | null"
  },
  "error": {
    "class": "string",
    "message": "string",
    "retryable": "boolean"
  }
}
```

### Field Definitions

| Field | Type | Description |
|-------|------|-------------|
| `request_id` | string (UUID) | Matches the request that triggered this response |
| `response_id` | string (UUID) | Unique identifier for this response |
| `model_profile_id` | string | Which model responded |
| `timestamp` | string (ISO 8601) | When the response was received |
| `status` | string | Outcome of the request |
| `diagnostic.findings` | string[] | Key findings from analysis |
| `diagnostic.confidence` | number | Confidence score (0-1) |
| `diagnostic.recommendations` | string[] | Suggested actions (for human review) |
| `diagnostic.risks` | string[] | Identified risks |
| `usage.token_input` | integer | Tokens consumed from input |
| `usage.token_output` | integer | Tokens generated in output |
| `usage.cost_estimate` | number \| null | Estimated cost in USD |
| `error.class` | string | Error classification (see `REAL_PROVIDER_INTEGRATION_DESIGN.md`) |
| `error.message` | string | Human-readable error description |
| `error.retryable` | boolean | Whether the error is retryable |

### Output Constraints

- Response `diagnostic` section is advisory only
- `recommendations` require human review before any action
- `risks` inform policy decisions but don't activate them
- No file writes, shell execution, or policy updates from response

## Validation Rules

### Request Validation

1. `request_id` must be a valid UUID
2. `allowed_tools` must be empty `[]`
3. `output_ref` must be `"diagnostic"`
4. `context` must not contain raw credentials or secrets

### Response Validation

1. `response_id` must be a valid UUID
2. `request_id` must match a known outgoing request
3. `status` must be one of: `success`, `error`, `timeout`
4. If `status` is `success`, `diagnostic` must be present
5. If `status` is `error`, `error` must be present
6. No raw credentials or secrets in any field

## Future Evolution

This schema will evolve as the provider integration matures:

- CA-8: Advisor-only (current design)
- Future: Tool invocation, policy activation, autonomous actions
- Future: Multi-provider support, failover, routing

Each evolution requires a new design document and approval gate.
