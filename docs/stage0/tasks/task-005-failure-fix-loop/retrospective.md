# Retrospective — task-005-failure-fix-loop

Source: harness_architecture_book_v0.7.4.1-canonical §7 / Memory Plane

## Task Summary

| Field | Value |
|-------|-------|
| task_id | stage0_task_005 |
| type | failure_then_fix_loop |
| final_status | _not yet run_ |

## What Went Well



## What Went Wrong



## Schema Issues Found



## Process Issues Found



## Suggested Schema Changes

```yaml
# proposed changes
```

## Suggested Process Changes



## Advisor Protocol Validation

- [ ] Advisor was invoked at least once
- [ ] diagnosis field was populated
- [ ] recommended_action field was populated
- [ ] do_not_do field was populated
- [ ] Advisor response was recorded in events.jsonl

## Failure Loop Validation

- [ ] At least one failed_retryable was produced
- [ ] failure_code was recorded in completion.json
- [ ] Fix Loop either succeeded or gave合理的 failure explanation
- [ ] retry_count reflects actual attempts

## Project Board Writeback Check

- [ ] Project Board item status updated after task completion
- [ ] Updated status matches completion.json status
- [ ] Event recorded in events.jsonl for state change
