# SOP: 前端组件交付

## 适用场景
- 页面开发
- 组件库开发
- UI 重构

## 拆解模板
1. **[frontend_expert] 组件设计**: 组件树、Props/Events 定义、状态管理
2. **[frontend_expert] 组件实现**: 模板、样式、逻辑
3. **[frontend_expert] 响应式适配**: mobile/tablet/desktop
4. **[test_engineer] 组件测试**: 单元测试 + 快照测试
5. **[doc_writer] 组件文档**: Storybook 或使用文档

## 依赖关系
```
Task1 → Task2 → Task3 (并行: Task4 从 Task2 开始可并行)
Task2,3 → Task5
```
