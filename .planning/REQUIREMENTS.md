# Requirements: SwarmOS v5.1

**Defined:** 2026-05-16
**Core Value:** 让 AI Agent 在可控的安全边界内可靠地完成复杂工程任务 — 调度永不脑裂、写入永不冲突、副作用永不重复、审计链路永不缺失。

## v1 Requirements

### 控制面核心 (CONTROL)

- [ ] **CONTROL-01**: Event-Sourced Session 支持 append-only Event Log 持久化所有状态变更
- [ ] **CONTROL-02**: Session Store 实现 CAS（Compare-And-Set）语义并支持并发安全版本控制
- [ ] **CONTROL-03**: 支持 Plan/PlanEpoch 的编译、失效与继承语义
- [ ] **CONTROL-04**: Scheduler 主循环实现 DAG 任务图的确定性推进
- [ ] **CONTROL-05**: Projection 视图支持从 Event Log 派生与重建

### 任务租约与执行 (LEASE)

- [ ] **LEASE-01**: 任务状态机实现：Pending→Ready→Leased→Running→Succeeded/Failed/TimedOut/Cancelled
- [ ] **LEASE-02**: 支持 Lease 发放、续租、超时与过期处理
- [ ] **LEASE-03**: Fencing Token 单调递增，旧 Worker 迟到写入被拒绝
- [ ] **LEASE-04**: Worker Spec 冻结为不可变执行规格并可重放
- [ ] **LEASE-05**: 支持幂等提交（command_id 去重），防止重复执行

### Project Tools 与 HITL (TOOLS)

- [ ] **TOOLS-01**: 支持 ReadProjectFile / ReadProjectTree / SubmitProjectPatch / DeleteProjectFile 四种远程工具
- [ ] **TOOLS-02**: 路径能力边界鉴权（allowed_read_paths / allowed_write_paths / forbidden_paths）
- [ ] **TOOLS-03**: Patch Classifier 实现语义分级（safe / moderate / sensitive / reject）
- [ ] **TOOLS-04**: Approval Request 创建与 Approval Token 签发、校验、消费
- [ ] **TOOLS-05**: Approval Token 与 lease_id/fencing_token/snapshot_id/payload_hash 强绑定

### Effect Gateway (EFFECT)

- [ ] **EFFECT-01**: 所有真实副作用都通过 Effect Gateway 统一入口申请与执行
- [ ] **EFFECT-02**: Idempotency Key 驱动幂等校验，防止重复副作用
- [ ] **EFFECT-03**: 审批策略引擎支持分权、四眼原则、Break-Glass 紧急越权
- [ ] **EFFECT-04**: Manifest 不可变绑定（payload_sha256 + object_version_id）防审批后篡改
- [ ] **EFFECT-05**: 副作用 Outbox 异步投递 + Inbox 去重双写一致性协议

### 三层空间隔离 (SPACE)

- [ ] **SPACE-01**: Agent Space 支持轻量沙箱（WASM）运行 Worker ReAct 循环
- [ ] **SPACE-02**: Project Workspace 作为虚拟文件系统或对象存储提供全局项目视图
- [ ] **SPACE-03**: Execution/Target Space 支持完整业务依赖的集成测试沙箱（Firecracker）
- [ ] **SPACE-04**: Warm Pool 实现镜像分层管理（Agent Image 与 Target Image 分离）
- [ ] **SPACE-05**: 任务结束后沙箱回滚与临时凭证清除

### 多租户治理 (TENANT)

- [ ] **TENANT-01**: 租户级策略（model_egress_policy / training_opt_in / log_retention_policy）
- [ ] **TENANT-02**: 租户级数据隔离（存储、缓存、日志的 tenant_id 索引）
- [ ] **TENANT-03**: 租户级配额与公平调度（Deficit Round Robin）
- [ ] **TENANT-04**: Token 撤销（短期 Token + Epoch + 在线 introspection）
- [ ] **TENANT-05**: Kill Switch 与应急熔断（租户级/模型端点级/全局）
- [ ] **TENANT-06**: 数据留存/删除/Offboarding 流程与 Legal Hold
- [ ] **TENANT-07**: BYOK 密钥生命周期管理

### Gateway (GATEWAY)

- [ ] **GATEWAY-01**: REST ↔ gRPC 协议转换网关（Tonic + Axum）
- [ ] **GATEWAY-02**: 中间件链（RequestID / Recovery / Logging / Timeout / AuthN / AuthZ / Idempotency）
- [ ] **GATEWAY-03**: 错误码映射（ToolErrorCode → gRPC status → HTTP status）
- [ ] **GATEWAY-04**: 审批流 REST API（创建/查询/批准/拒绝审批 + Token 消费）
- [ ] **GATEWAY-05**: 观测性集成（tracing spans / Prometheus metrics / gRPC metadata 透传）

### 进化面 (EVOLVE)

- [ ] **EVOLVE-01**: Observer 旁路采集运行指标、执行日志、工具调用轨迹
- [ ] **EVOLVE-02**: Evaluator 对结果质量、资源效率进行评分
- [ ] **EVOLVE-03**: Evolver 提炼 Prompt/SOP/工具策略，生成版本化候选
- [ ] **EVOLVE-04**: 灰度发布与 A/B 实验框架（小流量 → 对照 → 晋升/回滚）
- [ ] **EVOLVE-05**: Red-Team 安全验证流程（作弊检测、越权检查、注入防御）

### 上线验证 (LAUNCH)

- [ ] **LAUNCH-01**: Release Gate Checklist 自动化验证
- [ ] **LAUNCH-02**: Red-Team Scenario 自动/半自动演练（24 个注册用例）
- [ ] **LAUNCH-03**: Go/No-Go 发布决策系统
- [ ] **LAUNCH-04**: 测试用例注册表持续维护与覆盖追踪

## v2 Requirements (Deferred)

### Planner 高阶功能

- **PLAN-01**: 多候选方案自动对比与策略推荐
- **PLAN-02**: 分域 Sub-Planner 自动路由
- **PLAN-03**: Consultant Mode 高阶模型诊断

### 进化面高级

- **EVLV-01**: 跨租户脱敏样本的联邦进化
- **EVLV-02**: 自动红队评测流水线
- **EVLV-03**: 提示注入的红队自动化测试

### 运维面板

- **OPS-01**: 租户管理 UI（配额/策略/审计）
- **OPS-02**: Worker 运行时仪表板
- **OPS-03**: Effect 审批与审计 UI

## Out of Scope

| Feature | Reason |
|---------|--------|
| 非 AI 编排工作负载 | SwarmOS 专为 AI Agent 工作负载的编排设计，不支持通用 CI/CD 调度 |
| 公有云 SaaS 租户面板 | 首发为平台基础设施层，终端用户 UI 后续补充 |
| 跨集群 Session 迁移 | 分布式拓扑定位于单 Region 多节点，跨 Region 容灾为 v2 范围 |
| Agent 自进化无审查发布 | 进化面所有更新必须经过版本化、灰度化、可回滚流程 |
| 大模型自托管 | 平台通过 Effect Gateway 调度模型调用，不直接管理 LLM 推理基础设施 |

## Traceability

| Requirement | Phase | Status |
|-------------|-------|--------|
| CONTROL-01 | Phase 1 | Pending |
| CONTROL-02 | Phase 1 | Pending |
| CONTROL-03 | Phase 1 | Pending |
| CONTROL-04 | Phase 1 | Pending |
| CONTROL-05 | Phase 1 | Pending |
| LEASE-01 | Phase 2 | Pending |
| LEASE-02 | Phase 2 | Pending |
| LEASE-03 | Phase 2 | Pending |
| LEASE-04 | Phase 2 | Pending |
| LEASE-05 | Phase 2 | Pending |
| TOOLS-01 | Phase 3 | Pending |
| TOOLS-02 | Phase 3 | Pending |
| TOOLS-03 | Phase 3 | Pending |
| TOOLS-04 | Phase 3 | Pending |
| TOOLS-05 | Phase 3 | Pending |
| EFFECT-01 | Phase 3 | Pending |
| EFFECT-02 | Phase 3 | Pending |
| EFFECT-03 | Phase 3 | Pending |
| EFFECT-04 | Phase 3 | Pending |
| EFFECT-05 | Phase 3 | Pending |
| SPACE-01 | Phase 2 | Pending |
| SPACE-02 | Phase 2 | Pending |
| SPACE-03 | Phase 4 | Pending |
| SPACE-04 | Phase 4 | Pending |
| SPACE-05 | Phase 4 | Pending |
| TENANT-01 | Phase 2 | Pending |
| TENANT-02 | Phase 2 | Pending |
| TENANT-03 | Phase 4 | Pending |
| TENANT-04 | Phase 4 | Pending |
| TENANT-05 | Phase 4 | Pending |
| TENANT-06 | Phase 4 | Pending |
| TENANT-07 | Phase 4 | Pending |
| GATEWAY-01 | Phase 2 | Pending |
| GATEWAY-02 | Phase 2 | Pending |
| GATEWAY-03 | Phase 2 | Pending |
| GATEWAY-04 | Phase 3 | Pending |
| GATEWAY-05 | Phase 2 | Pending |
| EVOLVE-01 | Phase 5 | Pending |
| EVOLVE-02 | Phase 5 | Pending |
| EVOLVE-03 | Phase 5 | Pending |
| EVOLVE-04 | Phase 5 | Pending |
| EVOLVE-05 | Phase 5 | Pending |
| LAUNCH-01 | Phase 5 | Pending |
| LAUNCH-02 | Phase 5 | Pending |
| LAUNCH-03 | Phase 5 | Pending |
| LAUNCH-04 | All | Pending |

**Coverage:**
- v1 requirements: 46 total
- Mapped to phases: 46
- Unmapped: 0 ✓

---
*Requirements defined: 2026-05-16*
*Last updated: 2026-05-16 after initial definition*
