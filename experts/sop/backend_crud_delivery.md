# SOP: 后端 CRUD 交付

## 适用场景
- API 端点开发
- 数据库 CRUD 操作
- 微服务模块开发

## 拆解模板
Planner 在遇到匹配 "backend" 类别的任务时，应将其拆解为以下子任务序列：

1. **[architect] 契约设计**: 定义 API 接口（路径/方法/请求体/响应体）和数据库 Schema
2. **[backend_expert] 模型实现**: 实现数据模型、Repository 层、参数校验
3. **[backend_expert] 服务实现**: 实现业务逻辑层、错误处理、边界条件
4. **[test_engineer] 测试编写**: 单元测试 + 集成测试
5. **[rust_reviewer] 代码审查**: 检查安全性、性能、惯用写法 (Rust 项目)

## 依赖关系
```
Task1 (契约设计)
  ├──→ Task2 (模型实现)
  │     └──→ Task4 (测试)
  └──→ Task3 (服务实现)
        └──→ Task4 (测试)
              └──→ Task5 (审查)
```
