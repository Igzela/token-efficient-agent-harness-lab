# Test Case Pack — module_001

Source: harness_architecture_book_v0.7.4.1-canonical §6.6

```yaml
test_case_pack:
  pack_id: pack_module_001
  module_id: module_001
  pass_threshold: all_required_pass
  required_tests:
    - test_id: test_001_01
      description: "验证 5 个 task_spec.json 的关键字段已填写且 _template 标记保留"
      input: "5 个 task_spec.json 文件"
      expected_output: "每个文件的 project_id、source_project_item、objective、risk_level 不为空字符串；_template 字段为 true"
      verification_method: manual

    - test_id: test_001_02
      description: "验证 project_board.md 中的 schema inconsistency 被修复"
      input: "修复后的 project_board.md"
      expected_output: "item_004 type 为 test_case（非 requirement）；依赖边数量为 3；所有 item 的 status 枚举值合法"
      verification_method: manual

    - test_id: test_001_03
      description: "验证 task-002-bugfix/run_log.md 记录了 bug 复现路径"
      input: "task-002-bugfix/run_log.md"
      expected_output: "Bug Reproduction 表格至少有 1 行填写；failure_code 字段不为空"
      verification_method: manual

    - test_id: test_001_04
      description: "验证 Project Board 状态回写"
      input: "project_board.md 的 item_001 和 item_002 status 字段"
      expected_output: "item_001 status 为 done；item_002 status 为 done 或 review"
      verification_method: manual

  optional_tests:
    - test_id: test_001_opt_01
      description: "验证 handoff_pack.json 的 structured_fields 与 completion.json 一致"
      input: "task-001 和 task-002 的 handoff_pack.json + completion.json"
      expected_output: "task_id 一致，status 一致"
      verification_method: manual

  regression_tests: []
```
