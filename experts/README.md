# OpenForce 专家库 & 经验库

## 两库区别

| | 专家库 (Expert Library) | 经验库 (Experience Library) |
|---|---|---|
| **存什么** | 角色模板 + SOP 流程 | 历史成功案例 + 拆解模式 |
| **谁来用** | Planner 拆解任务时查询 | Evolver 分析时自动提炼 |
| **如何增长** | 人工定义 + Evolver 自动派生 | 每次成功执行后自动沉淀 |

## 专家库：Planner 如何匹配角色

```
用户输入: "帮我做一个图书管理系统"

Planner 分词 → ["图书", "管理", "系统"]
   │
   ├── 精确匹配 index.json → 无命中
   │
   ├── LLM 语义扩展: "图书管理系统" ≈ "CRUD + 前端界面"
   │     "CRUD" → category: backend
   │     "前端" → category: frontend
   │
   ├── 检索 profiles → Worker 角色分配
   │     backend:  [backend_expert, rust_developer]
   │     frontend: [frontend_expert, vue_developer]
   │
   └── 读取 role profile JSON → 组装 Worker Spec
```

## 经验库：SOP 如何指导拆解

```
Planner 确定任务类别为 "backend"
   │
   ├── 检索 experts/sop/backend_crud_delivery.md
   │     Task1: [architect] 契约设计
   │     Task2: [backend_expert] 模型实现
   │     Task3: [backend_expert] 服务实现
   │     Task4: [test_engineer] 测试
   │
   └── LLM 根据实际任务裁剪模板 → 生成最终 DAG
```

## 四种扩充方式

### 1. 手动添加角色
```bash
# 新增角色 JSON → 更新 index.json categories[].profiles
```

### 2. 手动添加 SOP
```bash
# 新增 SOP markdown → 更新 index.json categories[].sop
```

### 3. Evolver 自动派生（自举）
```
成功执行 10 次 "Rust 后端 CRUD" 后:
  Evaluator 发现每次都用了相同的额外 Rust 指令
  Evolver 自动生成: rust_backend_specialist.json (更精确角色)
  下次 Planner 直接匹配到新角色
```

### 4. 跨 Session 经验提炼
```
Session A,B,C 的成功分解模式
  → Evolver 统计: backend_expert 使用率 80%, claude-sonnet 成功率 94%
  → 决策: 推荐 backend_expert 作为默认角色, 推荐 claude-sonnet 作为默认模型
  → 生成: backend_crud_delivery_v2 SOP
```

## 文件结构

```
experts/
├── index.json             ← 知识索引 (关键词→类别→角色)
├── profiles/              ← 角色模板 (6 个)
├── sop/                   ← 标准操作流程 (5 个)
└── experience/            ← 经验沉淀 (自动生成)
```
