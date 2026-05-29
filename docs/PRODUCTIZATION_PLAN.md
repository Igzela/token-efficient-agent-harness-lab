# 产品化计划

Date: 2026-05-29

## 现有文档参考

| 文档 | 内容 |
|------|------|
| `docs/dispatch/DISPATCHER_KERNEL_V0_ARCHITECTURE.md` §6 | Phase 3 完整架构：ProviderAdapter、凭证边界、审计、重试、错误分类（P3-T1~T13） |
| `docs/AGENT_CONTROL_PLANE_MIGRATION_CLOSEOUT.md` | 迁移完成状态，provider trait boundary 已实现但 disabled-by-default |
| `docs/CURRENT_STATUS.md` | 当前 1031 Rust test cases enumerated，local small-team productization、provider stack Stage 1、provider audit/usage bridge 已实现 |
| `docs/CI_VERIFICATION.md` | CI 验证范围（不含真实 provider 调用） |

## 现有代码参考

| 模块 | 状态 | 位置 |
|------|------|------|
| `Provider` trait + `DisabledProvider` | Rust 已实现 | `engine/src/provider.rs` |
| `NoopExecutor` | Rust 已实现 | `engine/src/executor_adapter.rs` |
| `AnthropicProvider` | **Python 完整实现**（272 行） | `src/harness_core/dispatch/provider/anthropic_provider.py` |
| `OpenAIProvider` | **Python 完整实现**（261 行） | `src/harness_core/dispatch/provider/openai_provider.py` |
| `RetryFallbackManager` | **Python 完整实现**（125 行） | `src/harness_core/dispatch/provider/retry_manager.py` |
| `CredentialBoundary` | **Python 完整实现**（51 行） | `src/harness_core/dispatch/provider/credential_boundary.py` |
| `ProviderAuditRecorder` | **Python 完整实现**（96 行） | `src/harness_core/dispatch/provider/audit_recorder.py` |
| `ProviderConfig` / `CredentialRef` / `RetryPolicy` | **Python 完整实现**（112 行） | `src/harness_core/dispatch/provider/provider_config.py` |
| `LocalProductStore` | Rust 已实现（569 行） | `engine/src/storage/local_product_store.rs` |
| `/api/v1/audit` 端点 | Rust 已实现 | `engine/src/http_server.rs` |

## 当前阻塞点

`DispatchEngine` 使用 `NoopExecutor`，dispatch 请求分析+路由后返回 `executor_type: "noop"` + `status: "not_executed"`。

---

## 阶段 1：Rust Provider 执行

**目标**：将 Python 的 provider 实现移植到 Rust，让 dispatch 真正调用 LLM

**对应架构书**：`DISPATCHER_KERNEL_V0_ARCHITECTURE.md` §6 Phase 3（P3-T1~T10）

### 步骤

| # | 架构书任务 | 任务 | 参考 Python | 达标要求 |
|---|-----------|------|------------|---------|
| 1.1 | P3-T5 | `engine/src/provider/credential.rs` — 凭证边界 | `credential_boundary.py`（51 行） | env 读取、validate、redact `sk-***abc` |
| 1.2 | — | `engine/src/provider/config.rs` — ProviderConfig | `provider_config.py`（112 行） | ProviderConfig + CredentialRef + RetryPolicy 结构体 |
| 1.3 | P3-T7 | `engine/src/provider/audit.rs` — 审计记录器 | `audit_recorder.py`（96 行） | 记录事件到 SQLite，`list_events(dispatch_id)` |
| 1.4 | P3-T10 | `engine/src/provider/error.rs` — 错误分类 | `anthropic_provider.py` L21-29 | HTTP status → error_domain 映射 |
| 1.5 | P3-T9 | `engine/src/provider/retry.rs` — 重试管理器 | `retry_manager.py`（125 行） | 指数退避、retryable domains、budget check |
| 1.6 | P3-T1 | `engine/src/provider/mod.rs` — Provider trait 扩展 | 已有 `provider.rs` | 添加 `health_check()`、`provider_id()` |
| 1.7 | P3-T2 | `engine/src/provider/openai.rs` — OpenAI provider | `openai_provider.py`（261 行） | `POST /v1/chat/completions`，解析 choices + usage |
| 1.8 | P3-T3 | `engine/src/provider/anthropic.rs` — Anthropic provider | `anthropic_provider.py`（272 行） | `POST /v1/messages`，解析 content blocks + usage |
| 1.9 | — | `engine/src/provider/executor.rs` — Provider→Executor 适配器 | `provider_executor.py`（58 行） | Provider trait → Executor trait，构建 prompt pack |
| 1.10 | — | `engine/src/dispatch_engine.rs` — 替换 NoopExecutor | — | `DispatchEngine` 接受 `Option<Arc<dyn Provider>>` |
| 1.11 | P3-T13 | `engine/tests/test_provider.rs` — 安全测试 | — | 无 secret 泄漏、mock HTTP 返回验证 |
| 1.12 | — | 端到端验证 | — | `cargo run` + curl → 真实 LLM 返回 |

### 达标标准（对应架构书 §6.11 Promotion Gate）

- [ ] `POST /api/v1/dispatch` 返回 `executor_type: "provider"` + `status: "provider_completed"`
- [ ] `output` 包含 LLM 生成文本，`input_tokens` / `output_tokens` / `latency_ms` 有值
- [ ] API key 未设置 → `error_domain: "provider_auth"`
- [ ] HTTP 超时 → `error_domain: "provider_timeout"`
- [ ] 429 → `error_domain: "provider_rate_limit"` + 自动重试
- [ ] 凭证在日志/响应中显示为 `sk-***abc`（P3-T13）
- [ ] 所有现有 Rust 测试 + 新测试全部通过；当前枚举值为 1031 Rust test cases
- [ ] CI 全绿

### 新增依赖

- `reqwest` crate（HTTP 客户端）

---

## 阶段 2：生产化加固

**目标**：审计持久化、Provider 健康检查、API 完善

**对应架构书**：`DISPATCHER_KERNEL_V0_ARCHITECTURE.md` §6（P3-T6~T8, P3-T12）

### 步骤

| # | 架构书任务 | 任务 | 参考 | 达标要求 |
|---|-----------|------|------|---------|
| 2.1 | P3-T6 | 审计事件写入 SQLite | `LocalProductStore` 已有 audit 表 | 每次 provider 调用记录 event |
| 2.2 | P3-T8 | 真实成本计算 | `anthropic_provider.py` L128-129 | `input_tokens * input_cost_per_1k + output_tokens * output_cost_per_1k` |
| 2.3 | — | `/api/v1/provider/health` 端点 | `http_server.rs` 已有 health 端点模式 | 返回各 provider 连通性 |
| 2.4 | P3-T12 | UsageLedger 真实数据 | `dispatch_ledger.rs` | provider 返回的 usage 写入 ledger |
| 2.5 | — | SDK 更新 | `sdk/typescript/src/wire-types.ts` | TypeScript/Python SDK 类型更新 |

### 达标标准

- [ ] 审计事件持久化到 SQLite（重启不丢失）
- [ ] 成本计算准确（与 provider 报告一致）
- [ ] Provider 健康检查 API 可用
- [ ] 现有测试 + 新测试全部通过

---

## 阶段 3：交付包装

**目标**：用户可一键安装、配置、使用

### 步骤

| # | 任务 | 参考 | 达标要求 |
|---|------|------|---------|
| 3.1 | `.env` 配置文件支持 | `main.rs` 已有 `HOST`/`PORT` env | `ANTHROPIC_API_KEY`、`OPENAI_API_KEY`、`PROVIDER_BASE_URL`、`PROVIDER_MODEL` |
| 3.2 | CLI 参数 `--config` | `main.rs` | `--config .env` 加载配置 |
| 3.3 | `scripts/install.sh` | `scripts/engine-start.sh` 已有模式 | 下载二进制 + 创建默认 `.env` |
| 3.4 | `docs/QUICKSTART.md` | `README.md` 已有安装步骤 | 5 分钟完成：安装 → 配置 → 启动 → dispatch |
| 3.5 | `docs/API.md` | `openapi_document()` 已有 OpenAPI JSON | 所有端点文档 |
| 3.6 | `CHANGELOG.md` | — | v0.1.0 changelog |
| 3.7 | `.github/workflows/release.yml` | `tests.yml` 已有 CI | tag 触发构建 + GitHub Release |

### 达标标准

- [ ] 新用户 5 分钟内完成首次 dispatch
- [ ] `.env` 配置 API key 即可使用
- [ ] API 文档覆盖所有端点
- [ ] `git tag v0.1.0` 触发自动发布

---

## 时间估算

| 阶段 | 工作量 | 依赖 |
|------|--------|------|
| 阶段 1：Rust Provider 执行 | 1-2 周 | 无 |
| 阶段 2：生产化加固 | 1 周 | 阶段 1 |
| 阶段 3：交付包装 | 1 周 | 阶段 2 |
| **总计** | **3-4 周** | |

## 验证清单

每个阶段完成后：

1. `cargo test -p engine` — 全部通过
2. `cargo fmt --check && cargo clippy -p engine -- -D warnings`
3. `python3 scripts/check_agent_handoff.py`
4. CI 全绿（`gh run list` 查看）
5. 端到端 dispatch 返回真实 LLM 结果（阶段 1+）
