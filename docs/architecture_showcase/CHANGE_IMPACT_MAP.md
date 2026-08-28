# Change Impact Map（核心模块变更影响与防御矩阵）

> **目标**：当人类开发者或自主 Agent 准备对仓库的核心模块进行代码或配置变更时，必须对照本矩阵检查影响半径、不可妥协的不变量（Invariants）、绝对禁止的反模式（Anti-Patterns）以及必跑测试基线。

---

## 1. 系统核心设计灵魂

1. **权限分阶段授予（Capability Staged Granting）**：
   - 任何一个阶段的凭据（Admission、Intake、Graph-Ready、Leased Execution）绝不自动蕴含后续阶段的执行与写入权限。
2. **单一持久化权威与无第二状态库（Single Persistence Authority）**：
   - Rust `engine/` 下的 `LocalProductStore` 是唯一的业务持久化状态所有者。禁止任何 Adapter、Handler、CLI 或 Dashboard 自行维护第二状态缓存。
3. **副作用绑定唯一 Owner 与不可逆操作停机（Fail-Closed on Unbound Effects）**：
   - 外部调用、真实 Provider 扣费、目标分支写入必须由受控 Envelope 派生一次性授权。遇到 `OUTCOME_UNKNOWN` 必须立即停机，严禁盲目重试。

---

## 2. 核心模块变更防御矩阵

### 1. `LocalProductStore`（持久化存储 / 事务边界）
* **涉及路径**：[`engine/src/storage/local_product_store/`](../../engine/src/storage/local_product_store/)
* **影响半径 (Blast Radius)**：🔥 **全系统 (P0)**。所有 Workflow Runs、Nodes、Events、Audit Logs 状态流转与事务一致性均受其约束。
* **必须遵守的不变量 (Invariants)**：
  1. **审计与业务同事务**：状态变更（Mutation）与审计日志（Audit Log）必须在同一个事务（`with_sqlite_transaction` / `with_transaction`）内原子提交。
  2. **禁止泄露外部连接**：严禁将数据库长连接或未提交事务句柄传递给耗时的外部网络或 IO 操作。
  3. **SQLite / PostgreSQL 双引擎对齐**：所有新增 SQL 查询、DDL 变更与锁逻辑必须同时提供 SQLite 与 PostgreSQL 实现并通过集成测试。
* **绝对禁止的反模式 (Anti-Patterns)**：
  * ❌ 在 Adapter 或 Handler 层自行缓存任务状态或使用裸连接写入。
  * ❌ 使用补救式写入（Compensating Writes）替代强事务原子提交。
* **必跑测试基线**：
  ```bash
  cargo test -p engine test_local_product_store
  cargo test -p engine test_pg_integration
  ```

---

### 2. `ToolPolicyNodeExecutor` & 安全门禁（沙箱隔离 / 权限约束）
* **涉及路径**：[`engine/src/tool_policy_executor.rs`](../../engine/src/tool_policy_executor.rs)
* **影响半径 (Blast Radius)**：🛡️ **执行安全 (P0)**。任何工具调用、文件读写、子进程派发与工作区路径限制。
* **必须遵守的不变量 (Invariants)**：
  1. **工作区严格隔离 (Workspace Path Confinement)**：所有文件读写和命令执行必须限制在任务分配的 Worktree / Workspace 根目录下，禁止跨目录逃逸。
  2. **默认关闭 (Fail-Closed on Unknown)**：未在白名单中显式声明的命令或路径，必须拒绝执行并记录审计日志。
  3. **凭据脱敏 (Redaction Guarantee)**：子进程环境变量中绝对不得包含 GitHub 写凭据或未授权的 Provider API Keys。
* **绝对禁止的反模式 (Anti-Patterns)**：
  * ❌ 允许任意字符串拼接并传入底层 `sh -c`。
  * ❌ 在执行前跳过工作区合法性校验。
* **必跑测试基线**：
  ```bash
  cargo test -p engine test_tool_policy_executor
  python3 tools/check_security_baseline.py
  ```

---

### 3. `Scheduler` & `ExecutorPool`（调度器 / 租约派发）
* **涉及路径**：[`engine/src/scheduler.rs`](../../engine/src/scheduler.rs), [`engine/src/executor_pool.rs`](../../engine/src/executor_pool.rs)
* **影响半径 (Blast Radius)**：⚙️ **调度与并发 (P1)**。节点就绪判定、并发工作线程池、租约超时与恢复。
* **必须遵守的不变量 (Invariants)**：
  1. **有租约执行 (Leased Execution Only)**：Worker 执行任何节点前必须通过 `LocalProductStore` 获取有效且未过期的 Lease。
  2. **崩溃幂等恢复**：若 Worker 意外崩溃或超时，调度器必须能安全释放或转移 Lease，且已完成步骤不得产生重复副作用。
* **绝对禁止的反模式 (Anti-Patterns)**：
  * ❌ 在无 Lease 状态下直接调用 Node Executor。
  * ❌ 忽略节点依赖 DAG 强行提前调度后续节点。
* **必跑测试基线**：
  ```bash
  cargo test -p engine test_scheduler
  cargo test -p engine test_executor_pool
  ```

---

### 4. `HTTP API Server` & Handlers（外部入口 / 接入网关）
* **涉及路径**：[`engine/src/http_server/`](../../engine/src/http_server/)
* **影响半径 (Blast Radius)**：🌐 **API 控制面 (P1)**。SDK、Dashboard 及外部 Client 接口。
* **必须遵守的不变量 (Invariants)**：
  1. **认证与授权前置**：所有写操作必须通过网关鉴权与请求完整性校验。
  2. **仅充当适配器**：Handler 必须将外部请求委托给 Store/Scheduler，自身不保存任何状态逻辑。
* **绝对禁止的反模式 (Anti-Patterns)**：
  * ❌ 在 Handler 内部执行重计算或耗时阻塞调用。
  * ❌ 绕过 Store 事务直接暴露原始未校验数据给 Client。
* **必跑测试基线**：
  ```bash
  cargo test -p engine test_http_server
  ```

---

### 5. `AdaptiveFusionEngine`（自适应优化与多目标评估）
* **涉及路径**：[`engine/src/adaptive/`](../../engine/src/adaptive/)
* **影响半径 (Blast Radius)**：📊 **策略与评估 (P2)**。在线实验候选生成、Token 效率优化与打分。
* **必须遵守的不变量 (Invariants)**：
  1. **确定性重放 (Deterministic Replay)**：实验评分与策略选择在固定种子与输入下必须完全确定。
  2. **基线防退化**：候选生成与自适应调整不得突破安全基线与成本上限。
* **绝对禁止的反模式 (Anti-Patterns)**：
  * ❌ 在缺少完整执行轨迹证据的情况下生成评分结论。
* **必跑测试基线**：
  ```bash
  cargo test -p engine test_adaptive
  ```

---

### 6. 自主治理与 Agent 控制面（Steward / WorkCard / CI）
* **涉及路径**：[`scripts/agent-control/`](../../scripts/agent-control/), [`docs/`](../../docs/)
* **影响半径 (Blast Radius)**：🤖 **仓库自主治理 (P1)**。Mission、Stage、WorkCard 调度与 CI / Review 门禁。
* **必须遵守的不变量 (Invariants)**：
  1. **单写入者原则 (Single Writer Invariant)**：同时只能有一个主控制器写入治理状态。
  2. **Exact-Head CI & Review 绑定**：CI、审查证据与 PR 必须强绑定到同一个 Git Commit SHA，任何新 commit 必须重新触发全套验证。
  3. **零动态状态污染**：静态文档不得硬编码动态 SHA 或 PR 运行时回执。
* **绝对禁止的反模式 (Anti-Patterns)**：
  * ❌ 并行修改 `docs/NEXT_DECISION.md` 或 `docs/CURRENT_STATUS.md`。
  * ❌ 在未跑通 `check_agent_handoff.py` 的情况下强行合并。
* **必跑测试基线**：
  ```bash
  python3 scripts/check_agent_handoff.py
  python3 tools/check_security_baseline.py
  python3 -m unittest discover -s tests -p 'test_agent_*.py'
  git diff --check
  ```
