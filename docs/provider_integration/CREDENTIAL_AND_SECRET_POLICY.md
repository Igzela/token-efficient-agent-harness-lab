# Credential and Secret Policy

**Status**: Design document — policy for future implementation.

## Core Principles

1. **Never committed**: Credentials must never be committed to the repository
2. **Environment-only**: Credentials are loaded exclusively from environment variables
3. **Fail closed**: If credential is unavailable, no provider call is made
4. **No leakage**: Credentials must not appear in fixtures, logs, context packs, or usage ledgers

## Credential Storage Rules

### Environment Variables

- Provider credentials MUST be stored as environment variables
- Variable naming convention: `PROVIDER_<NAME>_API_KEY`
- No default or fallback values in code
- No credential files (`.env`, `credentials.json`, etc.) in the repository

### What Must Never Contain Credentials

| Location | Status |
|----------|--------|
| Git repository | FORBIDDEN |
| Test fixtures | FORBIDDEN |
| Log files | FORBIDDEN |
| Context packs | FORBIDDEN |
| Usage ledger | FORBIDDEN |
| Error messages | FORBIDDEN |
| Audit trails | FORBIDDEN |
| Diagnostic evidence | FORBIDDEN |

## Redaction Rules

When credentials or secrets appear in any output:

1. Replace with `[REDACTED]` marker
2. Log the redaction event (without the secret value)
3. Emit diagnostic evidence noting the redaction

### Redaction Examples

```
# Before redaction
Authorization: Bearer sk-abc123def456...

# After redaction
Authorization: Bearer [REDACTED]
```

## Fail-Closed Behavior

```
if credential_unavailable:
    log("Provider credential not found in environment")
    emit_diagnostic_evidence("credential_unavailable")
    return  # No call made
```

- No retry with alternative credential sources
- No fallback to other credentials
- No prompt for credentials at runtime

## Rotation Expectations

- Credentials should be rotatable without code changes
- Rotation is handled outside the harness (infrastructure concern)
- Harness should gracefully handle credential changes between runs
- No credential caching beyond request lifecycle

## Audit Requirements

- Log credential availability (present/absent), never the value
- Log rotation events if detected
- Never log credential strings, partial or full
