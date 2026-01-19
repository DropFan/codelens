# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## 项目概述

代码统计工具 - 统计目录下所有仓库的代码行数，支持 70+ 种语言识别、5 种输出格式和智能过滤。

## 常用命令

```bash
# 开发环境
pip install -e ".[dev]"          # 安装开发依赖

# 运行
python code_statistics.py        # 直接运行
code-stats                       # 安装后命令

# 测试
pytest                           # 运行所有测试
pytest tests/test_analyzers/     # 运行特定模块
pytest -v tests/test_filters/test_gitignore.py::test_function_name  # 单个测试

# 代码检查 (配置: line-length=100, target-version=py38)
ruff check .
```

## 依赖

- **必需**: `jinja2>=3.0`
- **可选**: `pathspec` (gitignore), `pyyaml` (配置), `argcomplete` (补全)
- **Python**: >=3.8

## 代码架构

模块化 Python 包 (`src/code_stats/`)，入口点 `code_statistics.py` 保持向后兼容。

```
src/code_stats/
├── cli.py              # 命令行解析 (argparse)
├── core.py             # CodeStatistics 主类，协调各模块
├── config.py           # 配置文件加载 (yaml/json)
├── completion.py       # Shell 补全 (argcomplete)
├── analyzers/          # 代码分析
│   ├── file.py         # FileContentAnalyzer: 单文件分析
│   ├── repository.py   # RepositoryAnalyzer: 仓库扫描
│   └── patterns.py     # 语言注释模式、函数正则、复杂度关键字
├── filters/            # 文件过滤
│   ├── gitignore.py    # GitIgnoreMatcher: pathspec 优先，回退 git 命令
│   ├── file_filter.py  # 代码/文档/二进制文件识别
│   └── dir_filter.py   # 智能排除 (检测项目类型动态构建排除列表)
├── outputs/            # 输出格式 (均继承 BaseOutput)
│   ├── console.py, html.py, markdown.py, json_output.py, csv_output.py
│   └── templates/      # HTML Jinja2 模板
├── constants/          # 常量定义
│   ├── extensions.py   # CODE_EXTENSIONS, BINARY_EXTENSIONS, DOC_EXTENSIONS
│   ├── excludes.py     # BASE_EXCLUDE_DIRS, LANGUAGE_EXCLUDE_DIRS
│   └── languages.py    # LANGUAGE_MAP, PROJECT_MARKERS
└── utils/formatters.py # 格式化工具
```

### 核心流程

1. `cli.py`: 解析参数 → 加载配置 → 创建 `CodeStatistics`
2. `core.py`: 收集仓库 → 并行/串行分析 → 生成摘要 → 输出
3. `repository.py`: 遍历目录 → `GitIgnoreMatcher` 过滤 → `FileContentAnalyzer` 分析
4. `file.py`: 根据 `patterns.py` 识别代码/注释/空行 → 计算复杂度

### 关键设计

- **智能排除**: `dir_filter.py` 根据项目标记文件 (package.json→node_modules, go.mod→vendor) 动态排除
- **Gitignore**: `GitIgnoreMatcher` 优先用 `pathspec` 库，回退到 `git check-ignore`
- **输出抽象**: 所有输出类继承 `BaseOutput`，实现 `output(repos, summary)`

## 扩展指南

### 添加新语言支持

1. `constants/extensions.py`: 添加文件扩展名到 `CODE_EXTENSIONS`
2. `constants/languages.py`: 添加扩展名→语言名映射到 `LANGUAGE_MAP`
3. `analyzers/patterns.py`: 添加注释模式 (`SINGLE_LINE_COMMENTS`, `MULTI_LINE_COMMENTS`)、函数正则 (`FUNCTION_PATTERNS`)、复杂度关键字 (`COMPLEXITY_KEYWORDS`)

### 添加新输出格式

1. 在 `outputs/` 创建新类，继承 `BaseOutput`
2. 实现 `output(self, repos: List[dict], summary: dict)` 方法
3. 在 `outputs/__init__.py` 导出
4. 在 `cli.py` 的 `--output` 选项中添加
