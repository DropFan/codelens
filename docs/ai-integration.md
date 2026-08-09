# 在 AI 编程工具里使用 codelens

让 AI 编程代理（Claude Code、Cursor 等）在动手改代码之前，先了解代码库的结构、健康度和风险。三种接入方式：

## 方式一：MCP 服务器（推荐）

codelens 内置 MCP（Model Context Protocol）服务器，代理可以直接调用分析工具：

```bash
# Claude Code
claude mcp add codelens -- codelens mcp

# Cursor / 其他支持 MCP 的工具（mcp.json）
{
  "mcpServers": {
    "codelens": { "command": "codelens", "args": ["mcp"] }
  }
}
```

暴露的工具：

| 工具 | 用途 |
|------|------|
| `repo_overview` | 仓库概览：文件/行数/语言分布/复杂度/测试占比/token 估算 |
| `code_health` | 健康报告：项目评分（A-F）、六维度得分（含重复度）、最差目录与文件 |
| `hotspots` | 变更热点：又复杂又频繁改动的文件（最可能出 bug），含代码年龄与作者集中度（知识孤岛标记） |
| `change_coupling` | 变更耦合：总是一起改的文件对（改 A 通常还要改 B）。注意：MCP 路径目前包含测试文件，与 CLI 默认（排除测试）口径不同 |
| `file_metrics` | 单文件指标：行数、圈/认知复杂度、嵌套深度、健康分 |

典型用法：代理在改一个文件前先查 `hotspots` 和 `change_coupling`，知道"这个文件风险多高、动它会牵连谁"；改完后用 `code_health` 确认没有劣化。

MCP 属于默认开启的 cargo feature；如需更小的二进制，可用 `cargo build --no-default-features` 去掉。

## 方式二：Agent Skill（教会代理工作流）

MCP 和裸 CLI 只告诉代理"有哪些工具"，不教"什么时候用、怎么组合、结果怎么读"。
仓库自带一个 Agent Skill（[`skills/codelens/SKILL.md`](../skills/codelens/SKILL.md)），
固化了四个工作流：陌生仓库摸底、改文件前的风险评估（定向耦合 + 函数级热点）、
重构前后留证据对比、CI 质量门禁，外加解读指南（等级含义、常见误导统计、噪音识别）。

安装（Claude Code）：

```bash
mkdir -p ~/.claude/skills/codelens
curl -fsSL https://raw.githubusercontent.com/DropFan/codelens/rust/skills/codelens/SKILL.md \
  -o ~/.claude/skills/codelens/SKILL.md
```

也可以放进单个项目的 `.claude/skills/codelens/` 只对该项目生效。skill 与 MCP
互补：装了 MCP 时 skill 会引导代理优先用 MCP 工具做只读查询。

## 方式三：直接跑 CLI（零配置）

有 Bash 能力的代理不需要 MCP，直接执行命令并读 JSON：

```bash
codelens . -f json                 # 仓库统计（含 token 估算、测试占比、ULOC）
codelens health . -f json          # 健康报告（默认六维度；--no-dup-scan 时省略重复度维度）
codelens health . --baseline main -f json   # 相对 main 分支的健康回归
codelens diff main -f json                   # 改动前后健康分对比（改完代码后验证）
codelens hotspot . -f json --top 10          # 热点文件（含知识孤岛）
codelens hotspot . --functions -f json       # 函数级热点
codelens coupling . -f json                  # 变更耦合（默认排除测试文件，--include-tests 找回）
```

可以把这段加进项目的 `CLAUDE.md` / `AGENTS.md`，代理会自己学会用：

```markdown
## 代码库分析工具
改动前先了解风险：
- `codelens health <path> -f json` — 文件健康分（A-F），低分文件谨慎重构
- `codelens hotspot . -f json` — 高频修改且复杂的文件，改动需额外小心
- `codelens coupling . --for <file> -f json` — 改这个文件通常还要同步改什么（默认不含测试文件配对，需要含测试时加 --include-tests）
```

## 上下文窗口预估

`codelens . --tokens` 会估算整个仓库折合多少 LLM token、能否放进主流模型的上下文窗口，帮助决定"全量喂给模型"还是"分模块处理"。
