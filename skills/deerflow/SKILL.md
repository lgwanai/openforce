---
name: deerflow-skill
description: DeerFlow agent orchestration for complex tasks requiring multi-step reasoning, web search, tool orchestration, or parallel subagent delegation. Use when user needs to research topics, search web, or delegate complex tasks to subagents. 使用此 Skill 进行复杂任务的编排和执行。
---

# DeerFlow Agent

Invoke the DeerFlow agent system directly within Claude Code. No server required - runs embedded in the current process.

## Command System

> **双模式支持**: 本 Skill 同时支持**显性指令**和**模糊匹配**。当用户输入以 `/` 开头的指令时，直接执行对应操作；否则进行意图推断。

### 显性指令列表

| 指令 | 别名 | 功能 | 示例 |
|------|------|------|------|
| `/deer` | `/d`, `/启动` | 启动 DeerFlow agent | `/deer 研究量子计算最新进展` |
| `/deer --flash` | `/d -f`, `/快问` | 快速模式（无思考、无规划） | `/deer --flash 法国首都` |
| `/deer --pro` | `/d -p`, `/专业` | 专业模式（思考+规划） | `/deer --pro 创建REST API项目计划` |
| `/deer --ultra` | `/d -u`, `/超级` | 超级模式（思考+规划+并行子agent） | `/deer --ultra 分析所有模块性能瓶颈` |

### 指令详细说明

#### `/deer` - 启动 Agent
```
/deer <任务描述>
/d <任务描述>  # 别名
/启动 <任务描述>  # 中文别名

示例:
  /deer 研究量子计算的最新进展
  /deer 分析宝马中国市场策略
  /d 对比 Tesla 和 BYD 的技术路线
```

#### `/deer --flash` - 快速模式
```
/deer --flash <简单问题>
/d -f <简单问题>  # 别名
/快问 <简单问题>  # 中文别名

特点: 无思考、无规划、无子agent，快速响应

示例:
  /deer --flash 法国的首都是哪里
  /d -f 今天天气如何
  /快问 1+1等于几
```

#### `/deer --pro` - 专业模式
```
/deer --pro <复杂任务>
/d -p <复杂任务>  # 别名
/专业 <复杂任务>  # 中文别名

特点: 有思考、有规划、无子agent，结构化处理

示例:
  /deer --pro 创建一个REST API的详细项目计划
  /d -p 设计微服务架构方案
  /专业 制定代码重构策略
```

#### `/deer --ultra` - 超级模式
```
/deer --ultra <大型任务>
/d -u <大型任务>  # 别名
/超级 <大型任务>  # 中文别名

特点: 有思考、有规划、有并行子agent，处理复杂任务

示例:
  /deer --ultra 分析所有模块性能并找出瓶颈
  /d -u 对比三家竞品公司的技术栈
  /超级 研究行业发展趋势并生成报告
```

### 模糊匹配关键词

当用户输入不是显性指令时，通过关键词推断意图：

| 意图 | 触发关键词 | 执行模式 |
|------|-----------|----------|
| 简单查询 | 是什么、什么是、多少、哪里 | --flash |
| 网络搜索 | 搜索、查一下、调研、research、search | 标准模式 |
| 复杂任务 | 分析、对比、规划、设计、创建 | --pro |
| 大型任务 | 全面、所有、多个、并行、批量 | --ultra |

## Mode Presets

| Mode | Thinking | Planning | Subagents | Use Case |
|------|----------|----------|-----------|----------|
| `--flash` | No | No | No | Quick responses, simple queries |
| `--standard` | Yes | No | No | Default, balanced speed and quality |
| `--pro` | Yes | Yes | No | Complex tasks requiring structured planning |
| `--ultra` | Yes | Yes | Yes | Parallel subagent delegation for heavy workloads |

## Features

- **Web Search**: Search the web for current information via Tavily
- **Web Fetch**: Fetch and extract content from web pages via Jina AI
- **Multi-step Reasoning**: Extended thinking for complex problems
- **Planning Mode**: Structured task decomposition with TodoList
- **Subagent Delegation**: Parallel task execution with specialized agents

## Configuration

Requires config.yaml with model credentials. Copy config.example.yaml to config.yaml and configure:

- DEEPSEEK_API_KEY - DeepSeek API key (recommended, cost-effective)
- TAVILY_API_KEY - Tavily API key for web search
- JINA_API_KEY - Jina AI API key for web fetch

Alternative models: OPENAI_API_KEY, ANTHROPIC_API_KEY

## Installation

The skill uses its own embedded deerflow modules. Ensure dependencies are installed:

```bash
pip install langchain langchain-anthropic langchain-openai tavily-python httpx
```

## Notes

- First run may be slower as the agent initializes
- Web search and fetch require API keys in config.yaml
- For local models via Ollama, ensure Ollama is running on localhost:11434
