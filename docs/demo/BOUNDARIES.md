# Boundaries

What the Harness App may and may not do.

## Allowed

| Capability | Description |
|---|---|
| Local app server | Runs on `127.0.0.1` or `localhost` only. Binds to a user-specified port. |
| App-owned `/tmp` registry | `/tmp/harness-demo-registry.json` stores registered repo metadata. |
| App-owned `/tmp` plans | `/tmp/harness-demo-plans.json` stores non-executable resource plans. |
| Read-only target repo audit | The auditor reads harness control files from the target repo. It does not write, modify, or delete anything in the target repo. |
| Non-executable plans | The planner generates resource estimates and approval gates. Plans are never executed by the app. |
| Preview-only review guidance | Guidance is derived from stored plans. It is advisory only and not persisted. |
| Derived triage | Triage ranks stored plans for human review. It is advisory only and not persisted. |
| Diagnostics | Operations diagnostics describe app-owned state (component health, data flow, storage). They are read-only. |

## Forbidden

| Prohibition | Description |
|---|---|
| Provider/model calls | The app does not call OpenAI, Anthropic, Google, or any other model provider. |
| Sandbox/process/container/VM execution | The app does not start sandboxes, processes, containers, or virtual machines. |
| Autonomous workers | The app does not spawn, manage, or dispatch autonomous workers. |
| Target repo writes | The app never writes to, modifies, or deletes files in any registered target repository. |
| Plan execution | Plans are non-executable. The app does not run, schedule, or dispatch plan steps. |
| Approval/run/execute/assign/deploy/merge controls | The app provides no controls for approving, running, executing, assigning, deploying, or merging work. |
| Stage 5 | No Stage 5 implementation exists. This demo does not start one. |
| MVP9 | No MVP9 implementation exists. This demo does not start one. |
| Production deployment | The app is a local development tool. It is not packaged, deployed, or distributed. |
| Credential handling | The app does not store, transmit, or manage API keys, tokens, or secrets. |

## State Ownership

| State | Owner | Writable | Location |
|---|---|---|---|
| Target repositories | User | No (read-only by app) | Wherever the user keeps their repos |
| App registry | App | Yes | `/tmp/harness-demo-registry.json` |
| Plan store | App | Yes | `/tmp/harness-demo-plans.json` |
| Diagnostics | Derived | No (computed on each request) | In-memory |
| Recent errors | Derived | No (computed from component status) | In-memory |
| Triage | Derived | No (computed from plan store) | In-memory |
| Review guidance | Derived | No (computed from plan store) | In-memory |

## Human Authority

The human operator remains the final authority. The app provides information; the human decides what to do with it. No app output is execution authorization.
