# Roadmap: OpenForce - 三省六部多智能体系统

**Created:** 2026-04-03
**Milestone:** v1.0 - 安全加固与核心功能完善
**Total Phases:** 7
**Granularity:** Standard

---

## Milestone Overview

将现有骨架代码完善为安全、可用的多智能体系统。优先修复安全漏洞，然后逐步补充核心功能。

**Success Criteria:**
- 所有 CRITICAL 安全漏洞修复
- 预算系统正常工作，能防止资源失控
- 六部 Agent 可正常执行任务
- Human-in-the-loop 流程完整可用

---

## Phase 1: 安全漏洞修复 (CRITICAL)

**Goal:** 修复所有 CRITICAL 安全漏洞，确保系统基础安全

**Depends on:** None

**Requirements:** SEC-01, SEC-02, SEC-03, SEC-04, SEC-05

**Success Criteria:**
1. Shell 注入漏洞已修复，run_agent_browser 使用白名单校验
2. 审批令牌使用 HMAC + 密钥生成，不可伪造
3. 所有硬编码路径移至配置文件
4. fetch_webpage 阻止访问内网地址
5. TaintEngine 实现基础污点传播逻辑

**Plans:**
- [x] Plan 1.1: 修复 Shell 注入漏洞
- [x] Plan 1.2: 实现安全令牌生成
- [x] Plan 1.3: 配置外部化
- [x] Plan 1.4: SSRF 防护
- [x] Plan 1.5: TaintEngine 基础实现

---

## Phase 2: 预算系统实现

**Goal:** 实现完整的预算控制系统，防止资源失控

**Depends on:** Phase 1

**Requirements:** BUD-01, BUD-02, BUD-03, BUD-04, BUD-05

**Success Criteria:**
1. Token 消耗实时追踪，超限自动熔断
2. Time 超时强制终止任务
3. Cost 预算可控
4. 全局熔断机制工作正常
5. 并发 Agent 预算隔离，无饿死问题

**Plans:** 6/6 plans complete
- [x] 02-00-PLAN.md — Wave 0: Test Infrastructure
- [x] 02-01-PLAN.md — Wave 1: Token Budget Tracking (BUD-01)
- [x] 02-02-PLAN.md — Wave 1: Time Budget Enforcement (BUD-02)
- [x] 02-03-PLAN.md — Wave 2: Cost Budget Tracking (BUD-03)
- [x] 02-04-PLAN.md — Wave 2: Global Circuit Breaker (BUD-04)
- [x] 02-05-PLAN.md — Wave 3: Concurrent Agent Budget Isolation (BUD-05)

---

## Phase 3: Human-in-the-loop 实现

**Goal:** 实现完整的高风险操作审批流程

**Depends on:** Phase 1, Phase 2

**Requirements:** HIL-01, HIL-02, HIL-03, HIL-04, AGT-01

**Success Criteria:**
1. 高风险工具调用自动触发审批
2. 审批快照正确生成和校验
3. 审批令牌一次性消费，防重放
4. 审批通过后任务正确恢复
5. 兵部 Agent 可执行代码任务

**Plans:** 6 plans
- [ ] 03-00-PLAN.md — Wave 0: Test Infrastructure
- [ ] 03-01-PLAN.md — Wave 1: Approval Flow Integration (HIL-01)
- [ ] 03-02-PLAN.md — Wave 1: Snapshot Generation (HIL-02)
- [ ] 03-03-PLAN.md — Wave 2: Token Consumption (HIL-03)
- [ ] 03-04-PLAN.md — Wave 2: Approval Resumption (HIL-04)
- [ ] 03-05-PLAN.md — Wave 3: Bingbu Agent (AGT-01)

---

## Phase 4: 六部 Agent 完善

**Goal:** 实现剩余五部 Agent 的核心能力

**Depends on:** Phase 3

**Requirements:** AGT-02, AGT-03, AGT-04, AGT-05, AGT-06

**Success Criteria:**
1. 吏部可管理技能安装和更新
2. 工部可维护沙箱环境
3. 刑部可执行代码审查和测试
4. 礼部可生成文档
5. 都察院可执行安全审计

**Plans:**
- [ ] Plan 4.1: 吏部 Agent
- [ ] Plan 4.2: 工部 Agent
- [ ] Plan 4.3: 刑部 Agent
- [ ] Plan 4.4: 礼部 Agent
- [ ] Plan 4.5: 都察院 Agent

---

## Phase 5: 编排机制完善

**Goal:** 完善并发控制和状态管理

**Depends on:** Phase 4

**Requirements:** ORC-01, ORC-02, ORC-03, ORC-04, ORC-05

**Success Criteria:**
1. Barrier 正确收集并发 Agent 结果
2. 超时屏障自动释放，无死锁
3. 状态机支持所有设计状态
4. ReAct 死循环自动熔断
5. 失败重试使用指数退避

**Plans:**
- [ ] Plan 5.1: Barrier 并发屏障
- [ ] Plan 5.2: 状态机完善
- [ ] Plan 5.3: ReAct 熔断
- [ ] Plan 5.4: 指数退避

---

## Phase 6: 代码质量提升

**Goal:** 提升代码健壮性和可维护性

**Depends on:** Phase 5

**Requirements:** QTY-01, QTY-02, QTY-03, QTY-04, QTY-05

**Success Criteria:**
1. 所有异常正确处理和日志记录
2. 关键函数有完整类型注解
3. 数据库连接使用上下文管理器
4. active_task 操作原子化
5. 无 Debug print 残留

**Plans:**
- [ ] Plan 6.1: 错误处理规范化
- [ ] Plan 6.2: 类型注解
- [ ] Plan 6.3: 数据库安全
- [ ] Plan 6.4: 代码清理

---

## Phase 7: 三层记忆系统设计

**Goal:** 设计并实现三层记忆系统，包括用户信息收集工具、短期记忆管理、长期知识图谱存储与检索

**Depends on:** Phase 6

**Requirements:** MEM-01, MEM-02, MEM-03, MEM-04, MEM-05

**Success Criteria:**
1. 用户信息收集作为独立 tool，户部直接收集不再绕行中书省
2. 短期记忆管理 session 上下文，支持压缩和检索
3. 长期记忆使用图数据库存储知识点，支持语义检索
4. /dream 指令触发原文整理和知识提取
5. 信息检索支持权重排序和时效标记

**Plans:**
- [ ] Plan 7.1: 用户信息收集 Tool
- [ ] Plan 7.2: 短期记忆管理
- [ ] Plan 7.3: 长期知识图谱
- [ ] Plan 7.4: /dream 整理机制
- [ ] Plan 7.5: 检索与权重系统

---

## Progress Summary

| Phase | Status | Plans | Progress |
|-------|--------|-------|----------|
| 1 | ✓ | 5/5 | 100% |
| 2 | ✓ | 6/6 | 100% |
| 3 | ○ | 0/6 | 0% |
| 4 | ○ | 0/5 | 0% |
| 5 | ○ | 0/4 | 0% |
| 6 | ○ | 0/4 | 0% |
| 7 | ○ | 0/5 | 0% |

**Legend:** ✓ Complete | ◆ In Progress | ○ Pending | ✗ Blocked

---

*Roadmap created: 2026-04-03*
*Last updated: 2026-04-04*
