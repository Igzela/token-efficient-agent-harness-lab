# Read-Only Interaction Policy

## Read-Only First Principle

The dashboard is a read-only observability tool. It displays harness state. It does not create, modify, or delete state. This is the foundational constraint for the first version.

## Forbidden Actions

The following actions are explicitly forbidden in the read-only dashboard:

### 1. Activate Policy

No policy activation through the dashboard. Policy lifecycle management remains in governance tooling.

### 2. Approve Policy

No policy approval through the dashboard. Approvals require governance workflow, not UI clicks.

### 3. Reject Policy

No policy rejection through the dashboard. Rejections require governance workflow.

### 4. Edit Project Board

No modification to the project board through the dashboard.

### 5. Edit Task Queue

No modification to the task queue through the dashboard.

### 6. Write Event Log

No writes to `events.jsonl` through the dashboard. The dashboard reads events, never writes them.

### 7. Call Provider

No LLM API calls through the dashboard. No model inference, no streaming, no provider interaction.

### 8. Run Sandbox

No code execution, no container management, no sandbox operations through the dashboard.

### 9. Change Routing

No model routing changes through the dashboard. Routing remains in model profile configuration.

### 10. Mutate Prompt

No prompt modification through the dashboard. Prompts are defined in source, not mutated via UI.

### 11. Mutate Skill

No skill modification through the dashboard. Skills are defined in source, not mutated via UI.

### 12. Create or Merge PR

No git operations through the dashboard. No PR creation, no branch management, no merge operations.

## Allowed First Actions

The following actions are allowed in the read-only dashboard:

### Filter

Users may filter displayed data by gate status, eval result, policy status, date range, or other relevant dimensions. Filtering is a view operation, not a state mutation.

### Sort

Users may sort displayed data by any visible column or field. Sorting is a view operation.

### Copy References

Users may copy references to data sources (file paths, gate IDs, eval names, policy IDs) for use in other tools. Copy is a clipboard operation, not a state mutation.

### Open Links

Users may open links to source files, governance docs, eval fixtures, or other referenced documents. Opening links is a navigation operation.

## Human Approval Display Rules

When the dashboard displays human approval status:

1. **Show approval state.** Display whether an approval is pending, approved, or rejected.
2. **Show approver identity.** Display who approved (with permission).
3. **Show approval timestamp.** Display when the approval occurred.
4. **Do not enable approval.** The dashboard shows approval status but does not allow granting or revoking approvals.
5. **Link to governance workflow.** Provide links to the governance tool where approvals are managed.

## Future Write Requirements

If write capability is added to the dashboard in the future, it requires:

1. **Separate design document.** A new design specifying what writes are allowed and under what conditions.
2. **Entry criteria.** Defined prerequisites including security review, human approval, and audit trail.
3. **Audit trail.** Every write action must be logged with timestamp, actor, action, and target.
4. **Rollback capability.** Every write action must be reversible.
5. **Human approval for sensitive actions.** Policy activation, PR creation, and provider calls require explicit human sign-off.
6. **Rate limiting.** Write actions must be rate-limited to prevent accidental or malicious bulk operations.
7. **Confirmation dialogs.** Every write action must require user confirmation before execution.

## Scope

This policy applies to all dashboard views defined in `UI_DASHBOARD_DESIGN.md`. Every panel, every data source, every interaction follows these rules. Exceptions require a formal policy change with stakeholder approval.
