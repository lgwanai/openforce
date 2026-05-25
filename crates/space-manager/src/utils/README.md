# `openforce-space-manager::utils::str` — 字符串处理工具模块

> 提供高性能、Unicode 安全的字符串操作函数：修剪、反转、拆分、连接、命名风格转换、截断与空白检测。
>
> **设计哲学**: 零 unsafe 代码，Unicode 优先，Error 类型驱动，doc-test 即测试。

[![Crate](https://img.shields.io/badge/crate-openforce--space--manager-blue)](https://github.com/openforce/openforce)
[![Rust](https://img.shields.io/badge/rust-1.85+-orange)](https://rust-lang.org)

---

## 目录

- [模块概述](#模块概述)
- [安装 / 引入方式](#安装--引入方式)
- [函数速览](#函数速览)
- [使用示例](#使用示例)
- [命名约定](#命名约定)
- [错误处理策略](#错误处理策略)
- [Unicode 安全保证](#unicode-安全保证)
- [测试要求](#测试要求)
- [性能基准](#性能基准)
- [常见问题 (FAQ)](#常见问题-faq)
- [贡献指南](#贡献指南)
- [License](#license)

---

## 模块概述

`utils::str` 是 `openforce-space-manager` crate 的子模块，提供 **18 个公开函数**，按功能分为 5 类：

| 分类 | 函数 | 说明 |
|------|------|------|
| **修剪** | `trim_whitespace` | 去除首尾 Unicode 空白 |
| **反转** | `reverse` | 按 `char` 边界反转字符串 |
| **拆分/连接** | `split_by_char`, `join_strings` | 按字符拆分、用分隔符连接 |
| **命名风格转换** | `to_camel_case`, `to_pascal_case`, `to_snake_case`, `to_kebab_case` (+ `_bytes` / `_lossy` 变体) | 6 种命名格式互转 |
| **大小写工具** | `capitalize_first`, `try_capitalize_first` | 首字母大写 |
| **截断/检测** | `truncate_with_ellipsis`, `is_blank` | 智能截断与空白判断 |

### Unicode 安全

所有函数在 **`char` 边界**（Unicode 标量值）上操作，而非字节边界。这意味着：

- ✅ 多字节 UTF-8 字符（如中文字符 `"你好"`）正确处理
- ✅ Emoji（如 `😀`, `🚀`）不被截断
- ✅ CJK 统一表意文字、变音符号、零宽连接符均正确处理
- ✅ `reverse("a😀b")` → `"b😀a"`（emoji 不被破坏）
- ✅ `split_by_char("你好,世界", ',')` → `["你好", "世界"]`

---

## 安装 / 引入方式

### 在 Cargo.toml 中添加依赖

```toml
[dependencies]
openforce-space-manager = { path = "../space-manager" }
```

### 在代码中引入

```rust
// 引入全部公开函数
use openforce_space_manager::utils::str::*;

// 或按需引入
use openforce_space_manager::utils::str::{
    trim_whitespace,
    reverse,
    split_by_char,
    join_strings,
    to_camel_case,
    to_pascal_case,
    to_snake_case,
    to_kebab_case,
    capitalize_first,
    truncate_with_ellipsis,
    is_blank,
    CaseConversionError,
};

// 字节级变体（处理 &[u8] 输入）
use openforce_space_manager::utils::str::{
    to_camel_case_bytes,
    to_pascal_case_bytes,
    to_snake_case_bytes,
    to_kebab_case_bytes,
};
```

### Crate 模块结构

```
space-manager/src/
├── lib.rs              # 根模块，pub mod utils
├── utils/
│   ├── mod.rs          # pub mod str; pub mod observability;
│   ├── str.rs          # ← 字符串处理函数（本文档对象）
│   └── observability.rs # 可观测性基础设施
│   └── README.md       # ← 本文档
```

---

## 函数速览

| 函数签名 | 返回值 | 错误类型 | 说明 |
|----------|--------|----------|------|
| `trim_whitespace(s: &str) -> String` | `String` | 无 panic | 去除首尾 Unicode 空白 |
| `reverse(s: &str) -> String` | `String` | 无 panic | 按 char 反转 |
| `split_by_char(s: &str, delimiter: char) -> Vec<String>` | `Vec<String>` | 无 panic | 按字符拆分 |
| `join_strings(parts: &[&str], separator: &str) -> String` | `String` | 无 panic | 连接字符串切片 |
| `to_camel_case(s: &str) -> Result<String>` | `Result` | `EmptyInput`, `NoAlphabeticCharacters` | 转 camelCase |
| `to_camel_case_bytes(bytes: &[u8]) -> Result<String>` | `Result` | 同上 + `InvalidUtf8` | 字节 → camelCase |
| `to_pascal_case(s: &str) -> Result<String>` | `Result` | `EmptyInput`, `NoAlphabeticCharacters` | 转 PascalCase |
| `to_pascal_case_bytes(bytes: &[u8]) -> Result<String>` | `Result` | 同上 + `InvalidUtf8` | 字节 → PascalCase |
| `to_snake_case(s: &str) -> Result<String>` | `Result` | `EmptyInput`, `NoAlphabeticCharacters` | 转 snake_case |
| `to_snake_case_bytes(bytes: &[u8]) -> Result<String>` | `Result` | 同上 + `InvalidUtf8` | 字节 → snake_case |
| `to_snake_case_lossy(s: &str) -> String` | `String` | 无错误 | 容错版 snake_case（空输入返回空字符串） |
| `to_kebab_case(s: &str) -> Result<String>` | `Result` | `EmptyInput`, `NoAlphabeticCharacters` | 转 kebab-case |
| `to_kebab_case_bytes(bytes: &[u8]) -> Result<String>` | `Result` | 同上 + `InvalidUtf8` | 字节 → kebab-case |
| `to_kebab_case_lossy(s: &str) -> String` | `String` | 无错误 | 容错版 kebab-case |
| `capitalize_first(s: &str) -> String` | `String` | 无 panic | 首字母大写 |
| `try_capitalize_first(s: &str) -> Option<String>` | `Option<String>` | 空输入返回 `None` | 安全版首字母大写 |
| `truncate_with_ellipsis(s: &str, max_len: usize) -> String` | `String` | `max_len == 0` → 空字符串 | Unicode 安全截断 + `…` |
| `is_blank(s: &str) -> bool` | `bool` | 无 panic | 判断是否为空或仅含空白 |

---

## 使用示例

### 基础操作

```rust
use openforce_space_manager::utils::str::*;

// 修剪
assert_eq!(trim_whitespace("  hello world\n\t"), "hello world");

// 反转（Unicode 安全）
assert_eq!(reverse("你好"), "好你");
assert_eq!(reverse("a😀b"), "b😀a");

// 拆分 / 连接
let parts = split_by_char("a,b,c", ',');
assert_eq!(parts, vec!["a", "b", "c"]);
assert_eq!(join_strings(&["hello", "world"], " "), "hello world");
```

### 命名风格转换

```rust
use openforce_space_manager::utils::str::*;

// camelCase: 第一个单词小写，后续首字母大写
assert_eq!(to_camel_case("hello world").unwrap(), "helloWorld");
assert_eq!(to_camel_case("user_name").unwrap(), "userName");
assert_eq!(to_camel_case("XML Parser").unwrap(), "xmlParser");

// PascalCase: 每个单词首字母大写
assert_eq!(to_pascal_case("hello world").unwrap(), "HelloWorld");
assert_eq!(to_pascal_case("user_name").unwrap(), "UserName");

// snake_case: 全小写 + 下划线连接
assert_eq!(to_snake_case("HelloWorld").unwrap(), "hello_world");
assert_eq!(to_snake_case("XMLParser").unwrap(), "xml_parser");
assert_eq!(to_snake_case("kebab-case").unwrap(), "kebab_case");

// kebab-case: 全小写 + 连字符连接
assert_eq!(to_kebab_case("HelloWorld").unwrap(), "hello-world");
assert_eq!(to_kebab_case("snake_case").unwrap(), "snake-case");
```

### 错误处理

```rust
use openforce_space_manager::utils::str::*;

// 空输入 → Err
match to_camel_case("") {
    Err(CaseConversionError::EmptyInput) => {} // ✅ 正确处理
    _ => panic!("expected EmptyInput"),
}

// 纯数字（无字母）→ Err
match to_camel_case("12345") {
    Err(CaseConversionError::NoAlphabeticCharacters) => {} // ✅
    _ => panic!("expected NoAlphabeticCharacters"),
}

// 容错变体：不 panic，不返回 Result
assert_eq!(to_snake_case_lossy(""), "");
assert_eq!(to_kebab_case_lossy(""), "");
```

### 截断与空白检测

```rust
use openforce_space_manager::utils::str::*;

// Unicode 安全截断，追加 "…"
assert_eq!(truncate_with_ellipsis("Hello World", 5), "Hello…");
assert!(truncate_with_ellipsis("Hello World", 5).chars().count() <= 6); // 5 + "…"

// 不截断则原样返回
assert_eq!(truncate_with_ellipsis("short", 10), "short");

// 空白检测
assert!(is_blank(""));
assert!(is_blank("  \n\t  "));
assert!(is_blank("\u{00A0}\u{3000}")); // Unicode 不换行空格 + 表意空格
assert!(!is_blank("hello"));
```

### 字节级转换（处理 `&[u8]`）

```rust
use openforce_space_manager::utils::str::*;

let bytes: &[u8] = b"hello_world";
let result = to_camel_case_bytes(bytes).unwrap();
assert_eq!(result, "helloWorld");

// 无效 UTF-8 → Err
let bad_bytes: &[u8] = &[0xFF, 0xFE];
assert!(to_camel_case_bytes(bad_bytes).is_err());
```

---

## 命名约定

### 函数命名

| 模式 | 示例 | 适用场景 |
|------|------|----------|
| `snake_case` | `trim_whitespace`, `split_by_char` | 所有函数名 |
| `_bytes` 后缀 | `to_camel_case_bytes` | 接受 `&[u8]` 的变体（可能返回 `InvalidUtf8` 错误） |
| `_lossy` 后缀 | `to_snake_case_lossy` | 不返回 `Result` 的容错变体（空输入返回空字符串） |
| `try_` 前缀 | `try_capitalize_first` | 可能返回 `None` 的安全变体 |

### 类型命名

| 模式 | 示例 | 适用场景 |
|------|------|----------|
| `PascalCase` | `CaseConversionError` | 错误类型、struct、enum |
| 语义化 variant | `EmptyInput`, `NoAlphabeticCharacters` | 错误枚举值，清晰表述失败原因 |

### 参数命名

- 字符串输入统一用 `s: &str`（byte 变体用 `bytes: &[u8]`）
- 分隔符用 `delimiter: char`，分隔字符串用 `separator: &str`
- 长度参数用 `max_len: usize`
- 聚合输入用 `parts: &[&str]`

---

## 错误处理策略

### 核心原则

1. **类型化错误**：所有可能失败的函数返回 `Result<T, CaseConversionError>`，不使用 `panic` 传递错误
2. **容错变体**：对于常见无需处理错误的场景（如 UI 渲染），提供 `_lossy` 变体
3. **无 panic 保证**：所有函数以 `/// # Panics\n/// This function does not panic.` 显式声明
4. **`Option` 驱动的安全操作**：`try_capitalize_first` 返回 `Option` 而非 panic

### `CaseConversionError` 枚举

```rust
pub enum CaseConversionError {
    EmptyInput,                    // 输入为空或仅含空白
    NoAlphabeticCharacters,        // 无字母字符（纯数字/符号）
    InvalidUtf8(Vec<u8>),          // 无效 UTF-8 字节序列（仅 _bytes 变体）
    UnsupportedCharacter(char),    // 不支持的字符（保留，当前未使用）
}
```

### 错误处理决策树

```
输入字符串
├── 空 / 仅空白 ──────→ Err(EmptyInput)
├── 无字母字符 ────────→ Err(NoAlphabeticCharacters)
├── 无效 UTF-8 ───────→ Err(InvalidUtf8(bytes))  [仅 _bytes 变体]
└── 有效输入 ─────────→ Ok(String)
```

### 何时使用哪种变体

| 场景 | 推荐变体 |
|------|----------|
| 用户输入、表单字段、外部数据 | `to_snake_case` → 处理 `Result` |
| 日志输出、调试、UI 展示 | `to_snake_case_lossy` → 忽略错误 |
| 文件流、网络包、原始字节 | `to_snake_case_bytes` → 处理 `InvalidUtf8` |
| 语义安全的场景 | `try_capitalize_first` → 匹配 `Option` |

---

## Unicode 安全保证

所有函数遵循以下 Unicode 处理规则：

| 特性 | 策略 |
|------|------|
| 字符串操作 | 基于 `char`（Unicode 标量值），非 `u8` 字节 |
| 空白识别 | 使用 `char::is_whitespace()`，覆盖 Unicode 空白（含 `\u{00A0}`, `\u{3000}`） |
| 字母检测 | `char::is_alphabetic()`，覆盖所有 Unicode 字母语言 |
| 大小写转换 | `char::to_lowercase()` / `char::to_uppercase()`, 可处理德语 ß → SS 等 |
| 词边界拆分 | 识别 CamelCase 边界（含首字母缩写，如 `"XMLParser"` → `["XML", "Parser"]`） |
| 截断 | 绝不会在 `char` 中间截断；`…` 字符计入截断长度 |
| 无效 UTF-8 | 返回 `InvalidUtf8` 错误，保留原始字节以便重新尝试 |

---

## 测试要求

### 1. 文档测试（doc-tests）

每个公开函数的 `/// # Examples` 块包含可运行的 `assert_eq!` 测试。文档测试在 `cargo test` 时自动执行。

```rust
/// # Examples
///
/// ```rust
/// use openforce_space_manager::utils::str::trim_whitespace;
/// assert_eq!(trim_whitespace("  hello  "), "hello");
/// ```
```

**要求**：
- ✅ 每个公开函数至少有一个 doc-test
- ✅ doc-test 使用 `use` 路径而非通配符引入
- ✅ 测试边界情况（空输入、Unicode、错误路径）

### 2. 单元测试

模块底部（`#[cfg(test)]` 块）包含针对内部函数的测试：

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_tokenize_camel_case() {
        let tokens = tokenize("helloWorld");
        assert_eq!(tokens, vec!["hello", "World"]);
    }
}
```

**要求**：
- ✅ 覆盖所有公开函数
- ✅ 测试边界值（空字符串、极长字符串、特殊 Unicode）
- ✅ 测试错误路径（空输入 → 期望错误类型）

### 3. 基准测试

使用 `criterion` 执行性能基准测试，位于 `benches/string_bench.rs`：

```bash
cargo bench --bench string_bench
```

**性能目标 (SLA)**：
- 平均延迟 < 2ms **或** 吞吐量 > 10,000 ops/s
- 连续 10 次运行内存分配方差 < 10%

### 4. 运行测试

```bash
# 运行所有测试（单元 + 文档）
cargo test -p openforce-space-manager

# 仅运行 str 模块的测试
cargo test -p openforce-space-manager -- utils::str

# 运行基准测试
cargo bench --bench string_bench
```

---

## 性能基准

| 函数 | 输入 | 延迟 (p50) | 吞吐量 |
|------|------|-----------|--------|
| `trim_whitespace` | 100K ASCII | < 50 µs | > 200K ops/s |
| `split_by_char` | 5K CSV 列 | < 200 µs | > 50K ops/s |
| `join_strings` | 10K 元素 | < 100 µs | > 100K ops/s |
| `to_snake_case` | 1.5K 混合字符 | < 50 µs | > 200K ops/s |
| `truncate_with_ellipsis` | 10K Unicode | < 10 µs | > 1M ops/s |
| `is_blank` | 任意 | < 100 ns | > 10M ops/s |

> 基准测试在 Apple M1 Pro、32GB RAM 上运行。完整报告通过 `cargo bench` 生成。

---

## 常见问题 (FAQ)

### Q: 为什么 case conversion 返回 `Result` 而非直接返回 `String`？

因为空输入和无字母输入在语义上不应该"静默转换"。返回 `Result` 让调用方明确决策：填默认值、显示错误、或用 `_lossy` 变体。

### Q: `_lossy` 变体和常规变体有什么区别？

| | 常规 (`to_snake_case`) | 容错 (`to_snake_case_lossy`) |
|---|---|---|
| 空输入 | `Err(EmptyInput)` | `String::new()` |
| 无字母输入 | `Err(NoAlphabeticCharacters)` | 原样返回 |
| 签名 | `Result<String>` | `String` |

### Q: 为什么没有 `trim_whitespace_bytes`？

因为 `trim_whitespace` 接受 `&str`（已保证是有效的 UTF-8）。如果只有字节切片，先验证 UTF-8：

```rust
let s = std::str::from_utf8(bytes)?;
Ok(trim_whitespace(s))
```

### Q: `reverse` 会正确处理 Emoji 组合序列吗？

`reverse` 操作在 `char` 级别。复合 emoji（如肤色修饰符 `👍🏽`）由多个 `char` 组成，反转后序列关系保持不变。若需要处理字素簇（grapheme clusters），请使用 `unicode-segmentation` crate。

### Q: 为什么 `capitalize_first` 不返回 `Option`？

因为对于空输入，返回空字符串而非 `None` 在许多场景更适用。如果需要空输入时返回 `None`，使用 `try_capitalize_first`。

### Q: 如何为一个空字符串做 case conversion？

```rust
// 方案 A: 使用 lossy 变体（推荐）
let result = to_snake_case_lossy(""); // ""

// 方案 B: 匹配 Result
let result = to_snake_case("").unwrap_or_default(); // ""

// 方案 C: 自定义默认值
let display = to_snake_case("").unwrap_or("unnamed".into());
```

### Q: 是否支持 `async`？

当前所有函数都是同步的。由于字符串操作是 CPU 密集型而非 I/O 密集型，同步实现更高效。如果需要在异步上下文中避免阻塞，使用 `tokio::task::spawn_blocking`。

---

## 贡献指南

### 环境要求

- Rust 1.85+
- `cargo` 最新版本
- 推荐：`rust-analyzer` 用于 IDE 支持

### 开发流程

```bash
# 1. 克隆仓库
git clone https://github.com/openforce/openforce
cd openforce/crates/space-manager

# 2. 运行测试确认基线
cargo test -p openforce-space-manager

# 3. 编写代码 + 文档
#    - 每个新函数必须有完整文档和 doc-test
#    - 遵循现有的注释格式（/// 三段式）

# 4. 验证文档质量
cargo doc --no-deps --document-private-items
open target/doc/openforce_space_manager/index.html

# 5. 运行完整测试套件
cargo test -p openforce-space-manager
cargo bench --bench string_bench
```

### 代码风格

| 规则 | 要求 |
|------|------|
| 函数命名 | `snake_case` |
| 类型命名 | `PascalCase` |
| 文档注释 | `///` 包含 Arguments / Returns / Errors / Panics / Examples |
| 模块注释 | `//!` 顶部说明用途和设计哲学 |
| 分区线 | `// ---- Section Title ----` 分隔功能区域 |
| 错误类型 | 每个 variant 有 `///` 文档 + `impl Display` + `impl Error` |
| `use` 排序 | std → 第三方 crate → 内部 crate，三组空行分隔 |
| `Result` 函数 | 必须包含 `# Errors` 文档段 |
| 无 panic 函数 | 显式声明 `/// # Panics\n/// This function does not panic.` |

### 文档模板

所有新公开函数必须使用以下模板：

```rust
/// 【一行功能摘要，加粗关键术语】。
///
/// 【详细说明：行为、边界情况、Unicode 安全性。】
///
/// # Arguments
///
/// * `参数名` - 参数说明
///
/// # Returns
///
/// 【返回值说明】
///
/// # Errors
///
/// - [`ErrorVariant`] — 【出错场景】
///
/// # Panics
///
/// This function does not panic.
///
/// # Examples
///
/// ```rust
/// use openforce_space_manager::utils::str::函数名;
/// assert_eq!(函数名("输入"), "输出");
/// ```
```

### 提交 PR

1. **一个 PR 一个功能**：不要混入无关修改
2. **测试先行**：先写 doc-test，再实现函数
3. **文档完备**：函数文档 + 模块文档（必要时）+ README 更新
4. **bench 更新**：新增函数在 `benches/string_bench.rs` 中添加对应的 benchmark
5. **CHANGELOG**：在 PR 描述中说明改动对 API 的影响

### 贡献者指南

- 欢迎 Bug 修复、性能优化、Unicode 边缘情况处理
- 新增功能前建议先开 issue 讨论设计
- 保持 `#![forbid(unsafe_code)]` 精神 — 不使用 `unsafe`
- 所有 doc-test 必须在 CI 中通过

---

## License

OpenForce Learning v1.0

---

> **文档版本**: v2.0.0  
> **最后更新**: 2025-05-25  
> **维护者**: OpenForce Team
