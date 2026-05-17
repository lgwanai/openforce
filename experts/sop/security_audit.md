# SOP: 安全审计

## 适用场景
- 代码安全审查
- 漏洞扫描
- 合规检查

## 拆解模板
1. **[security_auditor] 认证与授权检查**: 检查 AuthN/AuthZ 实现、Token 管理、权限校验
2. **[security_auditor] 注入与输入检查**: SQL注入、命令注入、XSS、路径遍历
3. **[security_auditor] 数据安全**: 敏感数据加密、密钥管理、日志脱敏
4. **[security_auditor] 依赖与配置**: 依赖漏洞扫描、不安全默认值、错误信息泄露

## 依赖关系
```
Task1 → Task2 → Task3 → Task4 (全部安全审计，可并行但建议串行以避免遗漏)
```
