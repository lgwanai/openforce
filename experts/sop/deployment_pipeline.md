# SOP: 部署流水线

## 适用场景
- CI/CD 配置
- Docker 化
- Kubernetes 部署

## 拆解模板
1. **[devops_engineer] Dockerfile 编写**: 多阶段构建、镜像优化
2. **[devops_engineer] CI 配置**: GitHub Actions / GitLab CI
3. **[k8s_operator] K8s 资源**: Deployment/Service/ConfigMap/Secret
4. **[security_auditor] 安全扫描**: 镜像漏洞、配置审计
5. **[test_engineer] 部署验证**: 冒烟测试、健康检查

## 依赖关系
```
Task1 → Task2 → Task3 (可部分并行)
Task1 → Task4 (安全扫描)
Task3 → Task5 (部署验证)
```
