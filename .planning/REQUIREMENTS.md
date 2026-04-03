# Requirements: OpenForce - 三省六部多智能体系统

**Defined:** 2026-04-03
**Core Value:** 构建安全、可控、可扩展的多智能体协作系统

## v1 Requirements

Requirements for initial release. Each maps to roadmap phases.

### Security (CRITICAL)

- [x] **SEC-01**: Shell 注入漏洞修复 - 移除 shell=True，使用参数列表和白名单校验
- [ ] **SEC-02**: 审批令牌安全 - 使用 HMAC + 密钥生成签名令牌
- [ ] **SEC-03**: 移除硬编码路径 - 外部工具路径使用配置文件或环境变量
- [ ] **SEC-04**: SSRF 防护 - fetch_webpage 添加 URL 协议和主机名校验
- [ ] **SEC-05**: 污点追踪实现 - TaintEngine 实现真正的污点传播和校验逻辑

### Budget System

- [ ] **BUD-01**: Token 预算限制 - 实现 token_budget 消耗追踪
- [ ] **BUD-02**: Time 预算限制 - 实现 time_budget 超时熔断
- [ ] **BUD-03**: Cost 预算限制 - 实现 cost_budget 成本控制
- [ ] **BUD-04**: 全局熔断机制 - 预算耗尽时强制终止任务
- [ ] **BUD-05**: 并发 Agent 预算隔离 - 子 Agent 独立预算，防止饿死

### Agent System

- [ ] **AGT-01**: 兵部 Agent 实现 - 通用执行、代码编写能力
- [ ] **AGT-02**: 吏部 Agent 实现 - 技能管理能力
- [ ] **AGT-03**: 工部 Agent 实现 - 环境维护能力
- [ ] **AGT-04**: 刑部 Agent 实现 - 审查测试能力
- [ ] **AGT-05**: 礼部 Agent 实现 - 文档生成能力
- [ ] **AGT-06**: 都察院 Agent 实现 - 安全审计能力

### Human-in-the-loop

- [ ] **HIL-01**: 审批流程集成 - 高风险工具触发审批阻断
- [ ] **HIL-02**: 审批快照生成 - TOCTOU 防护
- [ ] **HIL-03**: 审批令牌消费 - 一次性原子消费
- [ ] **HIL-04**: 审批回调续跑 - 审批通过后恢复执行

### Orchestration

- [ ] **ORC-01**: Barrier 并发屏障 - 并发 Agent 结果统一收集
- [ ] **ORC-02**: 超时屏障释放 - 防止死锁
- [ ] **ORC-03**: 状态机完善 - 补充 WaitingApproval/Paused 等状态
- [ ] **ORC-04**: ReAct 死循环熔断 - 检测连续相同 Thought/Action
- [ ] **ORC-05**: 指数退避重试 - 实现 Backoff 策略

### Code Quality

- [ ] **QTY-01**: 错误处理规范化 - 捕获特定异常，添加日志
- [ ] **QTY-02**: 类型注解完善 - 关键函数添加类型提示
- [ ] **QTY-03**: 数据库连接安全 - 使用上下文管理器
- [ ] **QTY-04**: 竞态条件修复 - active_task 操作原子化
- [ ] **QTY-05**: 移除 Debug print - 替换为 logging

## v2 Requirements

Deferred to future release.

### Channels

- **CHN-01**: Feishu/Lark 通道实现
- **CHN-02**: HTTP API 通道实现
- **CHN-03**: 通道鉴权校验

### Persistence

- **PRS-01**: 图快照恢复
- **PRS-02**: 副作用一致性 - Outbox 模式

## Out of Scope

| Feature | Reason |
|---------|--------|
| 多租户支持 | 单机单用户模式，简化权限模型 |
| 分布式部署 | 当前仅支持本地运行 |
| 移动端应用 | 暂不考虑 |
| RBAC/ABAC | 单用户模式不需要复杂权限系统 |

## Traceability

Which phases cover which requirements. Updated during roadmap creation.

| Requirement | Phase | Status |
|-------------|-------|--------|
| SEC-01 | Phase 1 | Complete |
| SEC-02 | Phase 1 | Pending |
| SEC-03 | Phase 1 | Pending |
| SEC-04 | Phase 1 | Pending |
| SEC-05 | Phase 1 | Pending |
| BUD-01 | Phase 2 | Pending |
| BUD-02 | Phase 2 | Pending |
| BUD-03 | Phase 2 | Pending |
| BUD-04 | Phase 2 | Pending |
| BUD-05 | Phase 2 | Pending |
| AGT-01 | Phase 3 | Pending |
| AGT-02 | Phase 4 | Pending |
| AGT-03 | Phase 4 | Pending |
| AGT-04 | Phase 4 | Pending |
| AGT-05 | Phase 4 | Pending |
| AGT-06 | Phase 4 | Pending |
| HIL-01 | Phase 3 | Pending |
| HIL-02 | Phase 3 | Pending |
| HIL-03 | Phase 3 | Pending |
| HIL-04 | Phase 3 | Pending |
| ORC-01 | Phase 5 | Pending |
| ORC-02 | Phase 5 | Pending |
| ORC-03 | Phase 5 | Pending |
| ORC-04 | Phase 5 | Pending |
| ORC-05 | Phase 5 | Pending |
| QTY-01 | Phase 6 | Pending |
| QTY-02 | Phase 6 | Pending |
| QTY-03 | Phase 6 | Pending |
| QTY-04 | Phase 6 | Pending |
| QTY-05 | Phase 6 | Pending |

**Coverage:**
- v1 requirements: 30 total
- Mapped to phases: 30
- Unmapped: 0 ✓

---
*Requirements defined: 2026-04-03*
*Last updated: 2026-04-03 after code review*
