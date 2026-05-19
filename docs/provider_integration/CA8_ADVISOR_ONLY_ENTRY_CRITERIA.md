# CA-8 Advisor-Only Entry Criteria

**Status**: Design document — entry criteria for CA-8 Provider Integration milestone.

## Preconditions

All of the following must be satisfied before CA-8 work begins:

| Gate | Status | Description |
|------|--------|-------------|
| CA-0 | Sealed | Foundation baseline established |
| CA-1 | Sealed | Core infrastructure complete |
| CA-2 | Sealed | Agent harness functional |
| CA-3 | Sealed | Security baseline established |
| CA-4 | Sealed | Testing infrastructure complete |
| CA-5 | Sealed | CI/CD pipeline operational |
| CA-6 | Sealed | Packaging and distribution ready |
| CA-7 | Sealed | Baseline verification complete |
| Security Review | Passed | No unresolved security findings |
| CI Status | Green | All tests passing, no warnings |
| Design Approval | Approved | This design document reviewed and approved |
| Budget Allocation | Approved | Budget policy accepted and funded |
| Credential Strategy | Approved | Credential policy accepted and implemented |

## Allowed Actions (Advisor-Only)

CA-8 is restricted to advisory and diagnostic capabilities:

### 1. Advisor Preflight

- Before any code change, consult provider for risk assessment
- Provider reviews change plan and identifies potential issues
- Output is advisory only — human decides whether to proceed

### 2. Correction Suggestions

- Provider identifies potential bugs or issues in code
- Suggestions are diagnostic evidence, not direct fixes
- Human reviews and applies corrections manually

### 3. Risk Scanning

- Provider scans for security vulnerabilities, performance issues
- Results are advisory — no automatic remediation
- Human reviews and prioritizes findings

### 4. Offline Critique

- Provider reviews completed work for quality
- Critique informs future decisions
- No direct impact on current codebase

### 5. Candidate Ranking

- Provider evaluates multiple implementation approaches
- Ranking informs human decision-making
- Final choice is human-authored

## Forbidden Actions

CA-8 must NOT perform any of the following:

| Category | Forbidden Action |
|----------|-----------------|
| File Operations | Write, create, delete, or rename files |
| Shell Execution | Run any shell commands |
| Sandbox | Execute code in sandboxed environments |
| Active Routing | Route requests to providers without human approval |
| Policy Activation | Activate policies based on provider output |
| Pull Requests | Create, modify, or merge PRs |
| Merge Operations | Merge branches or resolve conflicts |
| Prompt Mutation | Modify system prompts based on provider feedback |
| Autonomous Actions | Take any action without human review |

## Required Audit Outputs

Every CA-8 invocation must produce:

### 1. Request Audit Trail

```
request_id: <uuid>
model_profile_id: <model>
timestamp: <iso8601>
context_summary: <brief description of what was sent>
allowed_tools: []
output_ref: diagnostic
```

### 2. Response Audit Trail

```
response_id: <uuid>
request_id: <uuid>
status: success|error|timeout
findings_summary: <brief summary>
confidence: <0-1>
recommendations_count: <integer>
risks_count: <integer>
```

### 3. Usage Audit Trail

```
request_id: <uuid>
token_input: <count>
token_output: <count>
cost_estimate: <usd>
budget_remaining: <usd>
```

### 4. Decision Audit Trail

```
request_id: <uuid>
human_decision: accepted|rejected|modified
decision_rationale: <brief explanation>
decision_timestamp: <iso8601>
```

## Exit Criteria

CA-8 is complete when:

1. **All preconditions met**: CA-0 through CA-7 sealed, security reviewed, CI green
2. **Design approved**: All design documents reviewed and accepted
3. **Budget allocated**: Budget policy accepted and funded
4. **Credentials configured**: Credential policy implemented and tested
5. **Advisor-only verified**: Provider can only advise, not act
6. **Audit complete**: Full audit trail for every provider interaction
7. **No policy activation**: Provider output never directly activates policies
8. **Human approval gate**: All provider recommendations require human review

## Success Metrics

- 100% of provider calls have complete audit trail
- 0% of provider outputs directly activate policies
- 0% of credentials leaked in logs or output
- 100% of budget checks pass before provider calls
- 100% of forbidden actions blocked by harness
