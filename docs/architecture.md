# Code Stats 架构文档

## 目录结构

```
code-stats/
├── code_statistics.py      # 入口脚本（向后兼容）
├── pyproject.toml          # 项目配置
├── src/
│   └── code_stats/         # 核心包
│       ├── __init__.py
│       ├── __main__.py     # python -m 入口
│       ├── cli.py          # 命令行解析
│       ├── core.py         # 核心协调类
│       ├── config.py       # 配置加载
│       ├── analyzers/      # 分析器模块
│       ├── filters/        # 过滤器模块
│       ├── outputs/        # 输出模块
│       ├── constants/      # 常量定义
│       ├── git/            # Git 集成
│       └── utils/          # 工具函数
└── tests/                  # 测试目录
```

## 系统架构图

```mermaid
graph TB
    subgraph Entry["入口层"]
        CLI[code_statistics.py]
        MOD[python -m code_stats]
        CMD[code-stats 命令]
    end

    subgraph Core["核心层"]
        CLIP[cli.py<br/>命令行解析]
        CFG[config.py<br/>配置加载]
        CORE[core.py<br/>CodeStatistics]
    end

    subgraph Analyzers["分析层"]
        RA[RepositoryAnalyzer<br/>仓库分析]
        FA[FileContentAnalyzer<br/>文件分析]
        PAT[patterns.py<br/>语言模式]
    end

    subgraph Filters["过滤层"]
        GI[GitIgnoreMatcher<br/>.gitignore 规则]
        FF[FileFilter<br/>文件过滤]
        DF[DirFilter<br/>目录过滤]
    end

    subgraph Outputs["输出层"]
        CO[ConsoleOutput<br/>控制台]
        HO[HtmlOutput<br/>HTML 报告]
        MO[MarkdownOutput<br/>Markdown]
        JO[JsonOutput<br/>JSON]
        CSO[CsvOutput<br/>CSV]
    end

    subgraph Constants["常量层"]
        EXT[extensions.py<br/>文件扩展名]
        LANG[languages.py<br/>语言映射]
        EXC[excludes.py<br/>排除规则]
    end

    CLI --> CLIP
    MOD --> CLIP
    CMD --> CLIP
    CLIP --> CFG
    CLIP --> CORE

    CORE --> RA
    RA --> FA
    FA --> PAT

    RA --> GI
    RA --> FF
    RA --> DF

    CORE --> CO
    CORE --> HO
    CORE --> MO
    CORE --> JO
    CORE --> CSO

    PAT --> EXT
    PAT --> LANG
    DF --> EXC
    FF --> EXT
```

## 核心流程

```mermaid
sequenceDiagram
    participant User as 用户
    participant CLI as cli.py
    participant Core as CodeStatistics
    participant RA as RepositoryAnalyzer
    participant FA as FileContentAnalyzer
    participant Output as OutputHandler

    User->>CLI: 执行命令
    CLI->>CLI: 解析参数
    CLI->>CLI: 加载配置文件
    CLI->>Core: 创建 CodeStatistics

    Core->>Core: 收集仓库路径

    loop 每个仓库
        Core->>RA: analyze(repo_path)
        RA->>RA: detect_project_types()
        RA->>RA: 初始化 GitIgnoreMatcher

        loop 每个文件
            RA->>RA: 过滤检查
            RA->>FA: analyze_file()
            FA->>FA: 计算代码/注释/空行
            FA->>FA: 计算复杂度
            FA-->>RA: FileStats
        end

        RA-->>Core: 仓库统计结果
    end

    Core->>Core: 生成全局摘要
    Core->>Output: output(repos, summary)
    Output-->>User: 显示结果
```

## 模块详解

### 1. 分析器模块 (analyzers/)

```mermaid
classDiagram
    class FileStats {
        +int total
        +int code
        +int comment
        +int blank
        +int size
        +int functions
        +int complexity
        +int max_depth
    }

    class ExtensionStats {
        +int files
        +int lines
        +int code
        +int comment
        +int blank
    }

    class FileContentAnalyzer {
        +analyze_file(path) FileStats
        -_count_lines(content, lang)
        -_calculate_complexity(content, lang)
        -_detect_language(ext)
    }

    class RepositoryAnalyzer {
        +analyze(repo_path) dict
        -_should_exclude(path)
        -_detect_project_types()
        +gitignore_matcher: GitIgnoreMatcher
    }

    FileContentAnalyzer ..> FileStats : creates
    RepositoryAnalyzer ..> ExtensionStats : creates
    RepositoryAnalyzer --> FileContentAnalyzer : uses
```

### 2. 过滤器模块 (filters/)

```mermaid
classDiagram
    class GitIgnoreMatcher {
        +bool use_pathspec
        +match(path) bool
        +batch_check(paths) dict
        -_load_gitignore_files()
        -_check_with_git_command(path)
    }

    class FileFilter {
        +is_code_file(path) bool
        +is_doc_file(path) bool
        +is_binary_file(path) bool
        -CODE_EXTENSIONS: set
        -DOC_EXTENSIONS: set
    }

    class DirFilter {
        +should_exclude(path) bool
        +detect_project_types(repo) set
        -BASE_EXCLUDE_DIRS: set
        -LANGUAGE_EXCLUDE_DIRS: dict
    }

    GitIgnoreMatcher --> FileFilter : collaborates
    DirFilter --> FileFilter : collaborates
```

### 3. 输出模块 (outputs/)

```mermaid
classDiagram
    class BaseOutput {
        <<abstract>>
        +output(repos, summary)*
        #format_number(n)
        #format_size(bytes)
    }

    class ConsoleOutput {
        +output(repos, summary)
        -_print_table()
        -_print_summary()
    }

    class HtmlOutput {
        +output(repos, summary)
        -_render_template()
        -template_env: Environment
    }

    class MarkdownOutput {
        +output(repos, summary)
        -_generate_table()
        -_generate_progress_bar()
    }

    class JsonOutput {
        +output(repos, summary)
        -_serialize()
    }

    class CsvOutput {
        +output(repos, summary)
        -_write_rows()
    }

    BaseOutput <|-- ConsoleOutput
    BaseOutput <|-- HtmlOutput
    BaseOutput <|-- MarkdownOutput
    BaseOutput <|-- JsonOutput
    BaseOutput <|-- CsvOutput
```

## 数据流图

```mermaid
flowchart LR
    subgraph Input["输入"]
        A[命令行参数]
        B[配置文件]
        C[源代码目录]
    end

    subgraph Process["处理"]
        D[参数解析]
        E[仓库发现]
        F[文件过滤]
        G[代码分析]
        H[统计汇总]
    end

    subgraph Output["输出"]
        I[控制台]
        J[HTML]
        K[Markdown]
        L[JSON]
        M[CSV]
    end

    A --> D
    B --> D
    D --> E
    C --> E
    E --> F
    F --> G
    G --> H
    H --> I
    H --> J
    H --> K
    H --> L
    H --> M
```

## 语言分析流程

```mermaid
flowchart TB
    A[读取文件内容] --> B{检测语言}
    B --> C[获取注释模式]
    C --> D[逐行分析]

    D --> E{是否空行?}
    E -->|是| F[blank++]
    E -->|否| G{是否在多行注释中?}

    G -->|是| H{是否结束标记?}
    H -->|是| I[退出多行注释]
    H -->|否| J[comment++]

    G -->|否| K{是否多行注释开始?}
    K -->|是| L[进入多行注释]
    K -->|否| M{是否单行注释?}

    M -->|是| N[comment++]
    M -->|否| O[code++]

    F --> P{更多行?}
    I --> P
    J --> P
    L --> P
    N --> P
    O --> P

    P -->|是| D
    P -->|否| Q[返回统计结果]
```

## 智能排除机制

```mermaid
flowchart TB
    A[扫描仓库根目录] --> B{检测项目标记}

    B --> C{package.json?}
    C -->|是| D[添加 node_modules 到排除列表]

    B --> E{go.mod?}
    E -->|是| F[添加 vendor 到排除列表]

    B --> G{requirements.txt?}
    G -->|是| H[添加 __pycache__, .venv 到排除列表]

    B --> I{Cargo.toml?}
    I -->|是| J[添加 target 到排除列表]

    B --> K{pom.xml?}
    K -->|是| L[添加 target, build 到排除列表]

    D --> M[合并排除规则]
    F --> M
    H --> M
    J --> M
    L --> M

    M --> N[应用于目录遍历]
```

## 复杂度计算

```mermaid
flowchart LR
    subgraph Input["输入"]
        A[源代码]
    end

    subgraph Metrics["指标计算"]
        B[函数计数<br/>正则匹配函数定义]
        C[圈复杂度<br/>统计分支关键字]
        D[嵌套深度<br/>追踪缩进/大括号]
    end

    subgraph Output["输出"]
        E[functions: 函数数量]
        F[complexity: 复杂度值]
        G[max_depth: 最大深度]
        H[avg_func_lines: 平均函数行数]
    end

    A --> B --> E
    A --> C --> F
    A --> D --> G
    E --> H
```

## 配置优先级

```mermaid
flowchart TB
    A[命令行参数] --> D{合并配置}
    B[配置文件] --> D
    C[默认值] --> D

    D --> E[最终配置]

    style A fill:#f96,stroke:#333
    style B fill:#9cf,stroke:#333
    style C fill:#ccc,stroke:#333

    subgraph Priority["优先级：高 → 低"]
        A
        B
        C
    end
```

## 并行处理架构

```mermaid
flowchart TB
    A[CodeStatistics.run] --> B{--parallel?}

    B -->|否| C[串行处理]
    C --> D[逐个分析仓库]

    B -->|是| E[并行处理]
    E --> F[ThreadPoolExecutor]
    F --> G[Worker 1<br/>分析 repo1]
    F --> H[Worker 2<br/>分析 repo2]
    F --> I[Worker N<br/>分析 repoN]

    G --> J[结果收集]
    H --> J
    I --> J
    D --> J

    J --> K[生成摘要]
    K --> L[输出结果]
```

## 扩展点

### 添加新语言支持

```mermaid
flowchart LR
    A[constants/extensions.py<br/>添加文件扩展名] --> B[constants/languages.py<br/>添加语言映射]
    B --> C[analyzers/patterns.py<br/>添加注释模式]
    C --> D[完成]
```

### 添加新输出格式

```mermaid
flowchart LR
    A[继承 BaseOutput] --> B[实现 output 方法]
    B --> C[在 outputs/__init__.py 注册]
    C --> D[在 cli.py 添加选项]
    D --> E[完成]
```

## 依赖关系

```mermaid
graph BT
    subgraph External["外部依赖"]
        J2[jinja2]
        PS[pathspec<br/>可选]
        YML[pyyaml<br/>可选]
    end

    subgraph Internal["内部模块"]
        CLI[cli]
        CORE[core]
        ANA[analyzers]
        FIL[filters]
        OUT[outputs]
        CONST[constants]
        UTIL[utils]
    end

    CLI --> CORE
    CORE --> ANA
    CORE --> FIL
    CORE --> OUT
    ANA --> CONST
    ANA --> FIL
    FIL --> CONST
    OUT --> UTIL
    OUT --> J2
    FIL --> PS
    CLI --> YML
```
