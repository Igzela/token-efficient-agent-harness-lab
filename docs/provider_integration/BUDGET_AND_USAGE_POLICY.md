# Budget and Usage Policy

**Status**: Design document — policy for future implementation.

## Budget Controls

### Per-Run Budget

- Each harness run has a maximum total budget
- Budget is measured in estimated cost (USD) or token count
- Budget is checked before each provider invocation
- Budget is decremented after each successful call

### Per-Request Budget

- Each individual provider request has a maximum token limit
- Per-request limit is a subset of per-run budget
- Exceeding per-request limit fails the request, not the run

### Hard Stop

- When per-run budget is exhausted, NO provider calls are made
- No retry, no fallback, no partial budget usage
- The harness continues operating without provider assistance

## Token Accounting

### Input Tokens

- Counted from the assembled context
- Includes system prompt, project context, diagnostic evidence
- Does not include credentials or secrets (never included in request)

### Output Tokens

- Counted from the provider response
- Includes diagnostic findings, recommendations, risks
- Recorded for audit and cost estimation

### Accounting Precision

- Token counts are exact (provider-reported)
- Cost estimates are approximate (may vary by provider/pricing tier)
- Both are recorded in the usage ledger

## Cost Accounting

### Cost Calculation

```
cost_estimate = token_input × input_price + token_output × output_price
```

- Prices are per-model and may change
- Harness stores price tables externally (not in code)
- Cost estimates are advisory, actual cost determined by provider

### Cost Tracking

- Cumulative cost tracked per run
- Per-request cost tracked for audit
- Total cost reported in usage ledger

## Retry Policy

### Default: No Retry

- Provider errors are logged and emitted as diagnostic evidence
- No automatic retry for failed requests
- Human review determines if retry is appropriate

### Exception: Transient Failures

- Only `timeout` errors may be retried (future consideration)
- Maximum 1 retry with exponential backoff
- Retry budget is separate from primary budget

### Retry Budget

- Retry attempts consume additional budget
- If retry budget exhausted, no further retries
- Both primary and retry costs tracked separately

## Budget Enforcement Flow

```
1. Assemble context
2. Check per-run budget remaining
   → If insufficient: HARD STOP, no call
3. Estimate request cost from context size
   → If exceeds per-request limit: adjust context or fail
4. Make provider call
5. Record actual usage in ledger
6. Decrement per-run budget by actual cost
7. If budget exhausted: mark run as budget-complete
```

## Usage Ledger

### Recorded Fields

| Field | Description |
|-------|-------------|
| `request_id` | Unique request identifier |
| `model_profile_id` | Model used |
| `token_input` | Input tokens consumed |
| `token_output` | Output tokens generated |
| `cost_estimate` | Estimated cost (USD) |
| `timestamp` | When the request was made |
| `budget_remaining` | Per-run budget after this request |

### What MUST NOT Be in Ledger

- Raw prompts or responses
- Credentials or secrets
- File contents beyond summaries
- Any sensitive material

## Emergency Budget Controls

- Global budget cap across all runs (infrastructure limit)
- Alert threshold at 80% of global cap
- Hard stop at 100% of global cap
- Override requires manual intervention with audit trail
