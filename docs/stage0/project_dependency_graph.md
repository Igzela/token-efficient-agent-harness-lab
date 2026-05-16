# Project Dependency Graph

Source: harness_architecture_book_v0.7.4.1-canonical §6.4

Consumed by: Project Board, Cross-task Dependency Manager, Project-to-Queue Handoff.

```yaml
project_dependency_graph:
  graph_id: graph_2026_stage0_schema_validation
  project_id: proj_2026_stage0_schema_validation
  graph_version: 1
  created_at: ""
  updated_at: "2026-05-15T22:05:00+08:00"
  nodes:
    - node_id: node_001
      item_id: item_001
      node_type: module
      status: done
      artifact_refs: []
      risk_level: low

    - node_id: node_002
      item_id: item_002
      node_type: bug
      status: done
      artifact_refs: []
      risk_level: low

    - node_id: node_003
      item_id: item_003
      node_type: doc
      status: done
      artifact_refs: []
      risk_level: low

    - node_id: node_004
      item_id: item_004
      node_type: test_case
      status: done
      artifact_refs: []
      risk_level: low

    - node_id: node_005
      item_id: item_005
      node_type: module
      status: done
      artifact_refs: []
      risk_level: medium

  edges:
    - edge_id: edge_001_002
      from_node: node_001
      to_node: node_002
      dependency_type: hard_dependency
      required_artifacts: []
      downstream_policy:
        on_upstream_success: start
        on_upstream_fail: block
        on_upstream_partial: wait

    - edge_id: edge_001_003
      from_node: node_001
      to_node: node_003
      dependency_type: hard_dependency
      required_artifacts: []
      downstream_policy:
        on_upstream_success: start
        on_upstream_fail: block
        on_upstream_partial: wait

    - edge_id: edge_002_005
      from_node: node_002
      to_node: node_005
      dependency_type: soft_dependency
      required_artifacts: []
      downstream_policy:
        on_upstream_success: start
        on_upstream_fail: run_readonly_only
        on_upstream_partial: allow_prefetch
```

## Dependency Rules (§6.4)

| Type | Rule |
|------|------|
| `hard_dependency` | Upstream must complete before downstream enters write phase |
| `artifact_dependency` | Unlocked by Artifact Gate |
| `soft_dependency` | Allows read-only prefetch; must be satisfied before final integration |
| `approval_dependency` | Must wait for Approval Broker decision |

## Execution Order

```
item_001 (no deps)
  ├─→ item_002 (hard dep on 001)
  │     └─→ item_005 (soft dep on 002)
  └─→ item_003 (hard dep on 001)

item_004 (no deps, can run in parallel with item_001)
```

## Readiness Rules

- `item_001` and `item_004` are immediately `ready` (no unmet dependencies)
- `item_002` becomes `ready` after `item_001` is `done`
- `item_003` becomes `ready` after `item_001` is `done`
- `item_005` can start read-only prefetch after `item_002` is `running`; enters write phase after `item_002` is `done`
