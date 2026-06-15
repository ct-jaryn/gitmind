# gitmind

> Auto-generate AI context docs for any codebase. One command, instant project knowledge.

`gitmind sync` scans your repository with tree-sitter, extracts code structure, and generates documentation files that AI agents (Claude Code, Cursor, Copilot) can use to understand your project instantly.

## Install

```bash
cargo install gitmind
```

## Usage

```bash
# Scan current repo and generate docs
gitmind sync

# Custom output directory
gitmind sync --output docs/ai

# Scan specific languages only
gitmind sync --lang rust,python

# Show project stats without writing files
gitmind stats
```

## What It Generates

```
.gitmind/
├── AGENTS.md         # AI agent context (drop into your project root)
├── architecture.md   # Module map + language distribution
└── knowledge.md      # Full project reference
```

### AGENTS.md

A structured summary of your project for AI agents:

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

### architecture.md

Language distribution, module details, and symbol visibility.

## Supported Languages

| Language   | Extensions         |
|------------|--------------------|
| TypeScript | `.ts`, `.tsx`      |
| JavaScript | `.js`, `.jsx`      |
| Python     | `.py`              |
| Rust       | `.rs`              |
| Go         | `.go`              |

## Why?

Every time you start a new AI coding session, the agent needs to understand your project. Without context, it makes wrong assumptions, suggests incorrect patterns, and wastes tokens.

`gitmind` solves this: run it once, commit the output, and every AI session starts with full project awareness.

## How It Works

1. **Scan** — Walks your repo, respects `.gitignore`, finds source files
2. **Parse** — Uses tree-sitter to extract AST-level symbols (functions, classes, structs, etc.)
3. **Analyze** — Groups symbols into modules, detects dependencies from manifest files
4. **Generate** — Outputs structured Markdown docs

## Comparison

| Tool | What it does | Limitation |
|------|-------------|------------|
| Hand-written AGENTS.md | Manual project docs | Gets stale fast |
| repomix | Concatenates all source files | No analysis, just raw code |
| **gitmind** | Auto-generates structured docs | You're reading about it now |

## Roadmap

- [ ] Incremental scan (only re-analyze changed files)
- [ ] Git history analysis (hot files, change frequency)
- [ ] VS Code extension
- [ ] CI/CD integration (auto-update docs on push)
- [ ] More languages (Java, C++, Ruby, PHP)

## License

MIT
