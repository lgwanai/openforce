# OpenForce - 三省六部多智能体系统

## What This Is

一个基于 LangGraph 的多智能体系统，采用中国古代三省六部制设计模式。系统中书省负责决策与规划，尚书省负责任务编排与调度，六部负责具体执行。

当前状态：架构骨架完成约 40-50%，存在 3 个 CRITICAL 安全漏洞，核心功能（六部 Agent、预算系统、Human-in-the-loop）尚未实现。

## Core Value

**构建安全、可控、可扩展的多智能体协作系统。**

如果其他所有功能都失败，至少要保证：
1. Agent 间任务委托能正确执行
2. 工具调用有安全边界，防止系统被攻破

## Requirements

### Validated

(None yet — ship to validate)

### Active

- [ ] 修复 3 个 CRITICAL 安全漏洞（Shell 注入、弱令牌、硬编码路径）
- [ ] 实现预算系统（token/time/cost budget + 熔断）
- [ ] 实现六部 Agent（兵部优先）
- [ ] 完善 Human-in-the-loop 审批流程
- [ ] 实现 Barrier 并发屏障
- [ ] 完善状态机流转
- [ ] 改进错误处理和日志

### Out of Scope

- 多租户支持 — 单机单用户模式
- 分布式部署 — 当前仅支持本地运行
- 移动端应用 — 暂不考虑

## Context

### 技术栈
- **语言**: Python 3.13
- **框架**: LangGraph (Agent 编排), FastAPI (API 层)
- **数据库**: SQLite (本地持久化)
- **LLM**: 支持多供应商 (OpenAI, Anthropic, DeepSeek, Kimi, Minimax)

### 现有架构
```
src/
├── agents/
│   ├── zhongshu.py    # 中书省 - 决策与规划
│   ├── shangshu.py    # 尚书省 - 编排与调度
│   └── hubu.py        # 户部 - 信息研究
├── tools/
│   ├── base.py        # 基础工具集 (文件、搜索、浏览器)
│   └── orchestration.py # 调度工具
├── core/
│   ├── db.py          # SQLite 持久化
│   ├── config.py      # 配置管理
│   └── utils.py       # LLM 调用工具
├── security/
│   └── taint_engine.py # 污点追踪 (骨架)
└── channels/
    └── cli.py         # CLI 消息通道
```

### 已完成功能
- 中书省/尚书省/户部基础架构
- 基础工具集（文件操作、搜索、浏览器）
- 沙箱路径校验
- 多模型配置
- Prompt 模板化
- CLI 通道
- 户部 ReAct 循环

### 已知问题
详见 idea2.md 审查报告

## Constraints

- **Tech Stack**: Python 3.13 + LangGraph + SQLite，不可更换核心框架
- **Timeline**: 无硬性截止日期，按优先级迭代
- **Compatibility**: 需兼容 macOS/Linux 开发环境
- **Security**: 必须修复 CRITICAL 漏洞才能进入生产

## Key Decisions

| Decision | Rationale | Outcome |
|----------|-----------|---------|
| LangGraph 作为编排框架 | 成熟的 Agent 编排库，支持状态管理和循环 | ✓ Good |
| SQLite 单文件数据库 | 简化部署，单机场景足够 | ✓ Good |
| 单机单用户模式 | 简化权限模型，避免过度设计 | — Pending |
| Prompt 模板文件化 | 避免硬编码，支持动态替换 | ✓ Good |

---
*Last updated: 2026-04-03 after code review*
