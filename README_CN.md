# gitmind

> 自动生成 AI 上下文文档，一条命令让 AI 秒懂你的项目。

`gitmind sync` 扫描你的代码仓库，用 tree-sitter 提取代码结构，自动生成供 AI Agent（Claude Code、Cursor、Copilot）使用的项目文档。

## 安装

```bash
cargo install gitmind
```

## 使用

```bash
# 扫描当前仓库并生成文档
gitmind sync

# 指定输出目录
gitmind sync --output docs/ai

# 只扫描特定语言
gitmind sync --lang rust,python

# 查看项目统计（不生成文件）
gitmind stats
```

## 生成的文件

```
.gitmind/
├── AGENTS.md         # AI Agent 上下文（放入项目根目录即可）
├── architecture.md   # 模块结构 + 语言分布
└── knowledge.md      # 完整项目参考
```

### AGENTS.md

项目结构化摘要，AI Agent 可直接使用：

```markdown
# my-project

## Project Overview
Languages: rust (12400 lines), typescript (8200 lines)

## Dependencies
- `tokio` (1.35)
- `serde` (1.0)
- `axum` (0.7)

## Directory Structure
src/api/             REST API handlers
src/core/            Business logic
src/db/              Database layer

## Public Interfaces
### api
- `function` create_user (line 42)
- `function` get_user (line 58)
```

## 支持的语言

| 语言       | 扩展名               |
|------------|---------------------|
| TypeScript | `.ts`, `.tsx`       |
| JavaScript | `.js`, `.jsx`       |
| Python     | `.py`               |
| Rust       | `.rs`               |
| Go         | `.go`               |

## 为什么需要 gitmind？

每次开启新的 AI 编程会话，Agent 都需要理解你的项目。没有上下文时，它会做出错误假设、建议不合适的模式、浪费 token。

`gitmind` 解决这个问题：运行一次，提交输出，每次 AI 会话都从完整的项目认知开始。

## 工作原理

1. **扫描** — 遍历仓库，遵守 `.gitignore`，找到源代码文件
2. **解析** — 用 tree-sitter 提取 AST 级别的符号（函数、类、结构体等）
3. **分析** — 将符号归类为模块，从 manifest 文件检测依赖
4. **生成** — 输出结构化 Markdown 文档

## 对比

| 工具 | 功能 | 局限性 |
|------|------|--------|
| 手写 AGENTS.md | 手动编写项目文档 | 容易过时 |
| repomix | 拼接所有源代码 | 无分析，只是原始代码 |
| **gitmind** | 自动生成结构化文档 | 你现在正在看它 |

## 路线图

- [ ] 增量扫描（只分析变更文件）
- [ ] Git 历史分析（热点文件、变更频率）
- [ ] VS Code 扩展
- [ ] CI/CD 集成（push 时自动更新文档）
- [ ] 更多语言（Java、C++、Ruby、PHP）

## 许可证

MIT
