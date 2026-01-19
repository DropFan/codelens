# Code Stats

一个功能强大的代码统计工具，用于分析目录下所有仓库的代码行数、语言分布和复杂度指标。

## 功能特性

- **多语言支持** - 支持 70+ 种编程语言的识别和分析
- **智能过滤** - 自动识别项目类型，智能排除依赖目录（node_modules、vendor 等）
- **复杂度分析** - 计算圈复杂度、函数数量、嵌套深度等代码质量指标
- **多种输出格式** - 支持控制台、HTML、Markdown、JSON、CSV 五种输出格式
- **Gitignore 支持** - 自动遵循 .gitignore 规则过滤文件
- **并行处理** - 支持多线程并行分析，提升大型项目的处理速度
- **配置文件** - 支持 YAML/JSON 配置文件，便于团队共享设置

## 安装

### 使用 pip 安装

```bash
# 基础安装
pip install -e .

# 安装开发依赖
pip install -e ".[dev]"

# 安装所有可选依赖
pip install -e ".[all]"
```

### 依赖说明

- **必需**: `jinja2>=3.0` (HTML 模板渲染)
- **可选**:
  - `pathspec`: 高效的 .gitignore 规则解析（推荐）
  - `pyyaml`: YAML 配置文件支持

## 快速开始

```bash
# 统计当前目录下所有仓库
python code_statistics.py

# 使用安装后的命令
code-stats

# 或以模块方式运行
python -m code_stats
```

## 使用示例

### 基本用法

```bash
# 统计当前目录
python code_statistics.py

# 统计指定目录
python code_statistics.py --dirs api admin gateway

# 包含所有文件（含依赖和文档）
python code_statistics.py --all

# 只统计特定语言
python code_statistics.py --lang python,go,javascript
```

### 输出格式

```bash
# 生成 HTML 报告
python code_statistics.py --output html

# 生成 Markdown 报告
python code_statistics.py --output markdown

# 生成 JSON 数据
python code_statistics.py --output json

# 生成 CSV 文件
python code_statistics.py --output csv

# 指定输出文件名
python code_statistics.py --output html -O report.html
```

### 过滤选项

```bash
# 排除测试文件
python code_statistics.py --exclude-files ".*_test\.py$"

# 排除特定目录
python code_statistics.py --exclude-dirs ".*/vendor/.*"

# 只包含特定文件
python code_statistics.py --include-files ".*\.py$"

# 限制扫描深度
python code_statistics.py --depth 3

# 禁用 .gitignore 过滤
python code_statistics.py --no-gitignore
```

### 高级选项

```bash
# 多线程并行处理
python code_statistics.py --parallel

# 显示 Git 信息
python code_statistics.py --git-info

# 详细输出模式
python code_statistics.py --verbose

# 查看支持的语言列表
python code_statistics.py --help-lang

# 使用配置文件
python code_statistics.py --config .code_stats.yaml
```

## 配置文件

支持 `.code_stats.yaml` 或 `.code_stats.json` 配置文件：

```yaml
# .code_stats.yaml
excludes: "*test*,*mock*"
lang: python,go,javascript
output: html
parallel: true
depth: 5
git_info: true
no_gitignore: false
```

## 输出示例

### 控制台输出

```
┌──────────────────────────────────────────────────────────────────────────────┐
│                              Code Statistics                                  │
├──────────────┬──────────┬──────────┬──────────┬──────────┬──────────────────┤
│ Repository   │ Language │    Files │    Lines │     Code │ Comment │  Blank │
├──────────────┼──────────┼──────────┼──────────┼──────────┼──────────────────┤
│ api-gateway  │ Go       │       45 │    3,250 │    2,800 │     200 │    250 │
│ web-admin    │ TypeScript│      120 │   15,000 │   12,000 │   1,500 │  1,500 │
├──────────────┴──────────┴──────────┴──────────┴──────────┴──────────────────┤
│ Total: 165 files, 18,250 lines (14,800 code, 1,700 comment, 1,750 blank)    │
└──────────────────────────────────────────────────────────────────────────────┘
```

### HTML 报告

HTML 报告包含：
- 项目总览和语言分布图表
- 按仓库和语言的详细统计
- 文件大小分布
- 复杂度指标和趋势
- 现代化响应式设计

## 统计指标说明

| 指标 | 说明 |
|------|------|
| Files | 代码文件数量 |
| Lines | 总行数 |
| Code | 代码行数（不含注释和空行） |
| Comment | 注释行数 |
| Blank | 空行数 |
| Functions | 函数/方法数量 |
| Complexity | 圈复杂度 |
| Max Depth | 最大嵌套深度 |

## 支持的语言

支持 70+ 种编程语言，包括但不限于：

| 类别 | 语言 |
|------|------|
| 通用语言 | Python, JavaScript, TypeScript, Go, Rust, Java, C/C++, C#, PHP, Ruby |
| 函数式语言 | Haskell, Lisp, Clojure, Erlang, Elixir, Scala, F# |
| 脚本语言 | Shell, Perl, Lua, AWK, TCL |
| JVM 生态 | Kotlin, Groovy, Scala |
| 前端技术 | HTML, CSS, Sass/SCSS, Less, Vue, Svelte |
| 数据/配置 | SQL, YAML, JSON, TOML, XML |
| 其他 | Markdown, LaTeX, Solidity, R, Julia, MATLAB |

使用 `--help-lang` 查看完整列表。

## 智能排除

工具会根据检测到的项目类型自动排除相关依赖目录：

| 项目类型 | 标记文件 | 自动排除 |
|----------|----------|----------|
| Python | requirements.txt, pyproject.toml | `__pycache__`, `.venv`, `venv`, `.pytest_cache` |
| JavaScript | package.json | `node_modules`, `dist`, `.next`, `coverage` |
| Go | go.mod | `vendor`, `bin` |
| Java | pom.xml, build.gradle | `target`, `build`, `.gradle` |
| Rust | Cargo.toml | `target`, `.cargo` |

## 项目架构

```
src/code_stats/
├── cli.py              # 命令行解析
├── core.py             # 核心协调类
├── config.py           # 配置加载
├── analyzers/          # 代码分析模块
│   ├── file.py         # 单文件分析
│   ├── repository.py   # 仓库扫描
│   └── patterns.py     # 语言模式定义
├── filters/            # 文件过滤模块
│   ├── gitignore.py    # .gitignore 规则
│   ├── file_filter.py  # 文件过滤
│   └── dir_filter.py   # 目录过滤
├── outputs/            # 输出格式模块
│   ├── console.py      # 控制台输出
│   ├── html.py         # HTML 报告
│   ├── markdown.py     # Markdown 输出
│   ├── json_output.py  # JSON 输出
│   └── csv_output.py   # CSV 输出
├── constants/          # 常量定义
└── utils/              # 工具函数
```

## 命令行选项

| 选项 | 说明 |
|------|------|
| `--all` | 统计所有文件（含依赖） |
| `--lang LANGS` | 只统计指定语言（逗号分隔） |
| `--dirs DIRS` | 只统计指定目录（逗号分隔） |
| `--repo REPOS` | 只统计指定仓库（逗号分隔） |
| `--output FORMAT` | 输出格式：html/json/markdown/csv |
| `-O, --output-file FILE` | 指定输出文件名 |
| `--exclude-files PATTERN` | 排除文件（正则表达式） |
| `--exclude-dirs PATTERN` | 排除目录（正则表达式） |
| `--include-files PATTERN` | 只包含文件（正则表达式） |
| `--depth N` | 限制扫描深度 |
| `--parallel` | 启用多线程并行处理 |
| `--git-info` | 显示 Git 仓库信息 |
| `--no-gitignore` | 禁用 .gitignore 过滤 |
| `--config FILE` | 使用配置文件 |
| `--verbose, -v` | 详细输出模式 |
| `--help-lang` | 显示支持的语言列表 |

## 开发

```bash
# 安装开发依赖
pip install -e ".[dev]"

# 运行测试
pytest

# 运行特定测试
pytest tests/test_analyzers/ -v
```

## 许可证

MIT License
