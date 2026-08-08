# Codelens GitHub Action

在 CI 里跑代码健康门禁：健康分低于阈值、或相对基线分支出现回归时让 PR 变红，并把报告贴成 PR 置顶评论和 step summary。

## 快速开始

```yaml
name: Code Health
on:
  pull_request:

permissions:
  contents: read
  pull-requests: write   # 贴 PR 评论需要

jobs:
  health:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v5
        with:
          fetch-depth: 0        # baseline 需要能解析到基线分支
      - uses: DropFan/codelens@rust
        with:
          fail-under: 'C'                                # 绝对阈值门禁
          baseline: 'origin/${{ github.base_ref }}'      # 回归门禁基线
          fail-on-regression: 'true'
```

效果：本次 PR 让任何文件的健康等级下降（例如 B → C），或项目等级下降时，检查失败并在 PR 评论里列出退步的文件；存量债务不会拦截 PR——只有这次改动引入的劣化才会。

## 输入

| 输入 | 默认 | 说明 |
|------|------|------|
| `path` | `.` | 要分析的路径（空格分隔多个） |
| `version` | `latest` | codelens 版本（release tag，如 `v0.1.5-rust`） |
| `fail-under` | 空 | 绝对门禁：健康分低于该等级（A/B/C/D）或分数（0-100）时失败 |
| `baseline` | 空 | 回归基线 git ref（推荐 `origin/${{ github.base_ref }}`） |
| `fail-on-regression` | `false` | 相对基线出现回归时失败 |
| `summary` | `true` | 报告写入 step summary |
| `comment` | `true` | 贴/更新 PR 置顶评论（仅 pull_request 事件） |

## 输出

| 输出 | 说明 |
|------|------|
| `score` | 项目健康分（0-100） |
| `grade` | 项目健康等级（A-F） |
| `gate` | `passed` / `failed` |

## 健康分徽章

Action 每次运行都会生成 `codelens-badge.json`（shields.io endpoint 格式）。把它发布到 gist 或 `gh-pages`，README 就能挂实时健康分徽章：

```yaml
      - name: Update badge gist
        if: github.ref == 'refs/heads/main'
        env:
          GH_TOKEN: ${{ secrets.BADGE_GIST_TOKEN }}   # 需要 gist 权限
        run: gh gist edit <GIST_ID> codelens-badge.json
```

README 中引用：

```markdown
![code health](https://img.shields.io/endpoint?url=https://gist.githubusercontent.com/<USER>/<GIST_ID>/raw/codelens-badge.json)
```

## 注意事项

- **浅克隆**：`actions/checkout` 默认 `fetch-depth: 1`，用 `baseline` 时请设 `fetch-depth: 0`（Action 也会尽力补拉基线 ref，但完整历史最可靠）。
- **私有仓库**：无需额外配置，二进制从公开 release 下载。
- **PR 评论权限**：fork 发起的 PR 上 `GITHUB_TOKEN` 是只读的，评论会失败但不影响门禁本身；可以关掉 `comment` 只用 step summary。
