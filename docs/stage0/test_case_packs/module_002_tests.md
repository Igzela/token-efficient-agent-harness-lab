# Test Case Pack — module_002

Source: harness_architecture_book_v0.7.4.1-canonical §6.6

```yaml
test_case_pack:
  pack_id: pack_module_002
  module_id: module_002
  pass_threshold: all_required_pass
  required_tests:
    - test_id: test_002_01
      description: "验证 README.md Five Task Templates 表格已更新"
      input: "更新后的 README.md"
      expected_output: "Five Task Templates 表格包含 item_id 列，5 行对应 item_001~005"
      verification_method: manual

    - test_id: test_002_02
      description: "验证 task-004 approval_request 模板已填写"
      input: "task-004-config-rule/run_log.md"
      expected_output: "Approval Request Reference 区的 approval_id、risk_level、requested_action、summary 不为空"
      verification_method: manual

    - test_id: test_002_03
      description: "验证 task-005 产生了至少一次 failed_retryable"
      input: "task-005-failure-fix-loop/run_log.md"
      expected_output: "Failure Loop Trace 至少有 Attempt 1 的 failure_code 和 result 字段填写"
      verification_method: manual

    - test_id: test_002_04
      description: "验证 task-005 Advisor Protocol 调用记录完整"
      input: "task-005-failure-fix-loop/run_log.md"
      expected_output: "Advisor Calls 表格至少有 1 行；diagnosis、recommended_action、do_not_do 均不为空"
      verification_method: manual

    - test_id: test_002_05
      description: "验证 Project Board 状态回写（item_003, 004, 005）"
      input: "project_board.md 的 item_003、004、005 status 字段"
      expected_output: "三个 item 的 status 均已从 todo 变为其他合法值（done/review/failed）"
      verification_method: manual

  optional_tests:
    - test_id: test_002_opt_01
      description: "验证 batch_digest.md 能从运行结果派生"
      input: "batch_digest.md + 5 个 task 的 completion.json"
      expected_output: "batch_digest 的 completed_tasks 和 failed_tasks 列表与 completion.json 一致"
      verification_method: manual

  regression_tests: []
```
