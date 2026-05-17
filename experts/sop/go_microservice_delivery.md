# SOP: Go 微服务交付
## 拆解模板
1. [api_designer] Proto/gRPC 契约定义
2. [go_developer] 服务实现 (handler + service + repository)
3. [go_developer] 中间件 (auth/logging/tracing)
4. [test_engineer] 测试 + benchmark
## DAG
Task1 → Task2 → Task3 → Task4
