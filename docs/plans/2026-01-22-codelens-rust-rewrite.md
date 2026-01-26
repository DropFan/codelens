# Codelens - Rust 重写设计方案

> 高性能代码统计工具，用 Rust 重写，作为 Python 版 `code-stats` 的替代。

## 1. 项目概览

### 1.1 目标

| 目标 | 说明 |
|------|------|
| CLI 兼容 | 保持与 Python 版相同的命令行参数 |
| 性能 | 比 Python 版快 30-50x |
| 可扩展 | 库与 CLI 分离，便于集成和二次开发 |
| 跨平台 | 支持 Linux / macOS / Windows |

### 1.2 设计决策

| 决策点 | 选择 | 理由 |
|--------|------|------|
| 架构 | 库 + CLI 分离 | 便于测试、复用、未来可发布 crates.io |
| 语言定义 | 内置 + 配置覆盖 | 开箱即用，允许用户扩展 |
| 并行策略 | ignore crate 目录级并行 | ripgrep 核心依赖，性能极佳 |
| 模板引擎 | askama | 编译时检查，类型安全 |
| 项目名 | codelens | 简洁有辨识度 |

### 1.3 技术栈

```
codelens
├── 并行遍历: ignore (ripgrep 作者开发)
├── 命令行解析: clap v4 (derive)
├── 序列化: serde + serde_json
├── 配置文件: toml
├── 模板引擎: askama
├── 并行计算: rayon
├── 错误处理: thiserror (库) + anyhow (CLI)
├── 日志: tracing
└── 测试: criterion (基准测试)
```

---

## 2. 项目结构

采用 Cargo workspace 组织多 crate：

```
codelens/
├── Cargo.toml                    # workspace 配置
├── README.md
├── LICENSE
├── CHANGELOG.md
├── .github/
│   └── workflows/
│       ├── ci.yml                # 测试 + lint
│       └── release.yml           # 跨平台构建发布
│
├── crates/
│   ├── codelens/                 # CLI binary
│   │   ├── Cargo.toml
│   │   └── src/
│   │       ├── main.rs
│   │       ├── cli.rs            # clap 定义
│   │       └── config.rs         # CLI 配置合并
│   │
│   └── codelens-core/            # 核心库
│       ├── Cargo.toml
│       ├── languages.toml        # 内置语言定义
│       └── src/
│           ├── lib.rs
│           ├── error.rs
│           ├── analyzer/
│           │   ├── mod.rs
│           │   ├── file.rs       # 单文件分析器
│           │   ├── repo.rs       # 仓库分析器
│           │   ├── stats.rs      # 统计数据结构
│           │   └── complexity.rs # 复杂度计算
│           ├── language/
│           │   ├── mod.rs
│           │   ├── registry.rs   # 语言注册表
│           │   ├── definition.rs # 语言定义结构
│           │   └── builtin.rs    # 内置语言
│           ├── filter/
│           │   ├── mod.rs
│           │   ├── gitignore.rs  # gitignore 支持
│           │   ├── pattern.rs    # glob/regex 匹配
│           │   └── smart.rs      # 智能排除
│           ├── output/
│           │   ├── mod.rs
│           │   ├── format.rs     # 输出 trait
│           │   ├── console.rs    # 终端输出
│           │   ├── json.rs
│           │   ├── csv.rs
│           │   ├── markdown.rs
│           │   └── html.rs       # HTML 报告
│           ├── walker/
│           │   ├── mod.rs
│           │   └── parallel.rs   # 并行遍历
│           └── config/
│               ├── mod.rs
│               ├── file.rs       # 配置文件加载
│               └── merge.rs      # 配置合并
│
├── templates/
│   └── report.html               # HTML 模板
│
├── benches/
│   └── benchmark.rs
│
├── tests/
│   ├── integration.rs
│   └── fixtures/                 # 测试用例文件
│
└── docs/
    └── plans/
```

### 2.1 Cargo.toml (workspace)

```toml
[workspace]
resolver = "2"
members = ["crates/*"]

[workspace.package]
version = "0.1.0"
edition = "2021"
rust-version = "1.75"
authors = ["..."]
license = "MIT"
repository = "https://github.com/..."

[workspace.dependencies]
clap = { version = "4", features = ["derive"] }
serde = { version = "1", features = ["derive"] }
serde_json = "1"
toml = "0.8"
ignore = "0.4"
rayon = "1.10"
askama = "0.12"
thiserror = "1"
anyhow = "1"
tracing = "0.1"
tracing-subscriber = "0.3"
colored = "2"
chrono = "0.4"
indexmap = { version = "2", features = ["serde"] }
regex = "1"
num_cpus = "1"
crossbeam-channel = "0.5"
```

---

## 3. 核心数据结构

### 3.1 统计数据

```rust
// analyzer/stats.rs

/// 单文件统计
#[derive(Debug, Clone, Default, Serialize)]
pub struct FileStats {
    pub path: PathBuf,
    pub language: String,
    pub lines: LineStats,
    pub size: u64,
    pub complexity: Complexity,
}

/// 行数统计
#[derive(Debug, Clone, Default, Serialize)]
pub struct LineStats {
    pub total: usize,
    pub code: usize,
    pub comment: usize,
    pub blank: usize,
}

/// 复杂度指标
#[derive(Debug, Clone, Default, Serialize)]
pub struct Complexity {
    pub cyclomatic: usize,
    pub functions: usize,
    pub max_depth: usize,
}

/// 仓库统计
#[derive(Debug, Clone, Serialize)]
pub struct RepoStats {
    pub name: String,
    pub path: PathBuf,
    pub primary_language: String,
    pub files: Vec<FileStats>,
    pub summary: RepoSummary,
    pub by_language: HashMap<String, LanguageSummary>,
    pub git_info: Option<GitInfo>,
}

/// 汇总统计
#[derive(Debug, Clone, Serialize)]
pub struct RepoSummary {
    pub total_files: usize,
    pub lines: LineStats,
    pub total_size: u64,
    pub complexity: Complexity,
    pub size_distribution: SizeDistribution,
}
```

### 3.2 核心 Trait

```rust
/// 文件分析器 trait
pub trait FileAnalyzer: Send + Sync {
    fn analyze(&self, path: &Path, content: &str) -> Result<FileStats>;
    fn supports(&self, path: &Path) -> bool;
}

/// 输出格式 trait
pub trait OutputFormat: Send + Sync {
    fn name(&self) -> &'static str;
    fn extension(&self) -> &'static str;
    fn write(&self, stats: &AnalysisResult, options: &OutputOptions, w: &mut dyn Write) -> Result<()>;
}

/// 过滤器 trait
pub trait Filter: Send + Sync {
    fn should_include(&self, path: &Path, is_dir: bool) -> bool;
}
```

---

## 4. 语言定义

### 4.1 语言结构

```rust
// language/definition.rs

#[derive(Debug, Clone, Deserialize)]
pub struct Language {
    pub name: String,
    pub extensions: Vec<String>,
    #[serde(default)]
    pub filenames: Vec<String>,
    #[serde(default)]
    pub line_comments: Vec<String>,
    #[serde(default)]
    pub block_comments: Vec<(String, String)>,
    #[serde(default)]
    pub string_delimiters: Vec<StringDelimiter>,
    #[serde(default)]
    pub function_pattern: Option<String>,
    #[serde(default)]
    pub complexity_keywords: Vec<String>,
    #[serde(default)]
    pub nested_comments: bool,
}
```

### 4.2 内置语言示例 (languages.toml)

```toml
[rust]
name = "Rust"
extensions = [".rs"]
line_comments = ["//"]
block_comments = [["/*", "*/"]]
function_pattern = "^\\s*(pub\\s+)?(async\\s+)?fn\\s+\\w+"
complexity_keywords = ["if", "else", "for", "while", "loop", "match", "?"]
nested_comments = true

[python]
name = "Python"
extensions = [".py", ".pyw", ".pyi"]
filenames = ["SConstruct", "SConscript"]
line_comments = ["#"]
block_comments = [['"""', '"""'], ["'''", "'''"]]
function_pattern = "^\\s*(async\\s+)?def\\s+\\w+"
complexity_keywords = ["if", "elif", "for", "while", "except", "with", "and", "or"]

[go]
name = "Go"
extensions = [".go"]
line_comments = ["//"]
block_comments = [["/*", "*/"]]
function_pattern = "^\\s*func\\s+"
complexity_keywords = ["if", "else", "for", "switch", "select", "case", "&&", "||"]
```

### 4.3 语言注册表

```rust
// language/registry.rs

const BUILTIN_LANGUAGES: &str = include_str!("../../languages.toml");

pub struct LanguageRegistry {
    by_extension: HashMap<String, Arc<Language>>,
    by_filename: HashMap<String, Arc<Language>>,
    by_name: HashMap<String, Arc<Language>>,
}

impl LanguageRegistry {
    pub fn with_builtin() -> Result<Self> {
        let mut registry = Self::new();
        registry.load_toml(BUILTIN_LANGUAGES)?;
        Ok(registry)
    }

    pub fn load_user_config(&mut self, path: &Path) -> Result<()>;

    pub fn detect(&self, path: &Path) -> Option<Arc<Language>> {
        // 1. 先匹配文件名
        // 2. 再匹配扩展名
    }
}
```

---

## 5. 并行遍历

### 5.1 配置

```rust
#[derive(Debug, Clone)]
pub struct WalkerConfig {
    pub threads: usize,
    pub follow_symlinks: bool,
    pub use_gitignore: bool,
    pub smart_exclude: bool,
    pub max_depth: Option<usize>,
    pub custom_ignores: Vec<String>,
}

impl Default for WalkerConfig {
    fn default() -> Self {
        Self {
            threads: num_cpus::get(),
            follow_symlinks: false,
            use_gitignore: true,
            smart_exclude: true,
            max_depth: None,
            custom_ignores: vec![],
        }
    }
}
```

### 5.2 并行遍历实现

```rust
impl ParallelWalker {
    pub fn walk_and_analyze<F>(
        &self,
        root: &Path,
        analyzer: Arc<FileAnalyzer>,
        filter: Arc<dyn Filter>,
        mut on_result: F,
    ) -> Result<()>
    where
        F: FnMut(FileStats) + Send,
    {
        let (tx, rx) = bounded(1000);

        let mut builder = WalkBuilder::new(root);
        builder
            .hidden(false)
            .git_ignore(self.config.use_gitignore)
            .git_global(self.config.use_gitignore)
            .threads(self.config.threads);

        if self.config.smart_exclude {
            self.apply_smart_excludes(&mut builder, root);
        }

        builder.build_parallel().run(|| {
            // 并行处理每个文件
        });

        for stats in rx {
            on_result(stats);
        }

        Ok(())
    }

    fn apply_smart_excludes(&self, builder: &mut WalkBuilder, root: &Path) {
        let markers = [
            ("package.json", &["node_modules", "dist", ".next"][..]),
            ("go.mod", &["vendor"]),
            ("Cargo.toml", &["target"]),
            ("pom.xml", &["target", ".mvn"]),
            ("requirements.txt", &["venv", ".venv", "__pycache__"]),
        ];
        // ...
    }
}
```

---

## 6. CLI 设计

### 6.1 参数定义

```rust
#[derive(Parser, Debug)]
#[command(name = "codelens", about = "高性能代码统计工具")]
pub struct Cli {
    /// 要统计的目录
    #[arg(default_value = ".")]
    pub paths: Vec<PathBuf>,

    #[command(flatten)]
    pub filter: FilterArgs,

    #[command(flatten)]
    pub output: OutputArgs,

    #[command(flatten)]
    pub advanced: AdvancedArgs,
}

#[derive(Args, Debug)]
pub struct FilterArgs {
    #[arg(short, long, value_delimiter = ',')]
    pub lang: Option<Vec<String>>,

    #[arg(long, value_delimiter = ',')]
    pub exclude: Option<Vec<String>>,

    #[arg(long)]
    pub exclude_files: Option<String>,

    #[arg(long)]
    pub include_files: Option<String>,

    #[arg(long)]
    pub min_lines: Option<usize>,

    #[arg(long)]
    pub max_lines: Option<usize>,

    #[arg(short, long)]
    pub depth: Option<usize>,

    #[arg(short, long)]
    pub all: bool,

    #[arg(long)]
    pub no_gitignore: bool,

    #[arg(long)]
    pub no_smart_exclude: bool,
}

#[derive(Args, Debug)]
pub struct OutputArgs {
    #[arg(short, long, value_enum, default_value = "console")]
    pub output: OutputFormat,

    #[arg(short = 'O', long)]
    pub output_file: Option<PathBuf>,

    #[arg(short, long)]
    pub summary: bool,

    #[arg(short, long)]
    pub quiet: bool,

    #[arg(short, long)]
    pub verbose: bool,

    #[arg(long, value_enum, default_value = "lines")]
    pub sort: SortBy,

    #[arg(long)]
    pub top: Option<usize>,
}

#[derive(Args, Debug)]
pub struct AdvancedArgs {
    #[arg(short = 'j', long)]
    pub threads: Option<usize>,

    #[arg(short, long)]
    pub config: Option<PathBuf>,

    #[arg(long)]
    pub no_config: bool,

    #[arg(long)]
    pub git_info: bool,

    #[arg(long)]
    pub list_languages: bool,
}
```

### 6.2 主流程

```rust
fn main() -> Result<()> {
    let cli = Cli::parse();
    init_tracing(cli.output.verbose);

    if cli.advanced.list_languages {
        return list_languages();
    }

    let config = load_config(&cli)?;
    let registry = Arc::new(LanguageRegistry::with_builtin()?);
    let analyzer = Arc::new(FileAnalyzer::new(Arc::clone(&registry)));
    let filter = build_filter(&config)?;
    let walker = ParallelWalker::new(config.walker.clone());

    let start = Instant::now();
    let stats = run_analysis(&cli.paths, &walker, &analyzer, &filter)?;
    let elapsed = start.elapsed();

    output_results(&stats, &config, elapsed)?;
    Ok(())
}
```

---

## 7. 输出格式

### 7.1 输出 Trait

```rust
pub trait OutputFormat: Send + Sync {
    fn name(&self) -> &'static str;
    fn extension(&self) -> &'static str;
    fn write(&self, stats: &AnalysisResult, options: &OutputOptions, w: &mut dyn Write) -> Result<()>;
}

#[derive(Debug, Clone)]
pub struct OutputOptions {
    pub summary_only: bool,
    pub sort_by: SortBy,
    pub top_n: Option<usize>,
    pub colorize: bool,
    pub show_git_info: bool,
}
```

### 7.2 支持的格式

| 格式 | 文件 | 说明 |
|------|------|------|
| Console | console.rs | 带颜色的终端输出 |
| JSON | json.rs | 结构化数据 |
| CSV | csv.rs | 表格数据 |
| Markdown | markdown.rs | 文档格式 |
| HTML | html.rs | 交互式报告 |

### 7.3 HTML 报告特性

- 暗色主题，现代 UI
- Chart.js 图表（语言分布、代码/注释比例）
- 响应式设计
- 排序、搜索功能

---

## 8. 错误处理

```rust
// error.rs

#[derive(Error, Debug)]
pub enum Error {
    #[error("无法读取文件: {path}")]
    FileRead { path: PathBuf, #[source] source: std::io::Error },

    #[error("无法解析配置文件: {path}")]
    ConfigParse { path: PathBuf, #[source] source: toml::de::Error },

    #[error("无效的语言定义: {name} - {reason}")]
    InvalidLanguage { name: String, reason: String },

    #[error("无效的正则表达式: {pattern}")]
    InvalidRegex { pattern: String, #[source] source: regex::Error },

    #[error("目录不存在: {path}")]
    DirectoryNotFound { path: PathBuf },

    #[error("输出写入失败")]
    OutputWrite(#[from] std::io::Error),

    #[error("模板渲染失败")]
    TemplateRender(#[from] askama::Error),

    #[error("JSON 序列化失败")]
    JsonSerialize(#[from] serde_json::Error),

    #[error("遍历目录时出错: {0}")]
    Walk(#[from] ignore::Error),
}

pub type Result<T> = std::result::Result<T, Error>;
```

---

## 9. 测试策略

### 9.1 单元测试

- 行数统计（各语言注释模式）
- 语言检测（扩展名、文件名）
- 配置加载与合并
- 过滤器匹配

### 9.2 集成测试

- 完整目录分析
- gitignore 规则
- CLI 参数解析
- 各输出格式

### 9.3 基准测试

- 单文件分析性能
- 目录遍历（不同线程数）
- 与 Python 版对比

---

## 10. 实现路线图

### Phase 1: 核心功能 (2 周)

**Week 1**
- [ ] 项目脚手架 (workspace, CI/CD)
- [ ] 语言定义模块 + 内置 50 种语言
- [ ] 文件分析器 (行数统计)
- [ ] 单元测试覆盖

**Week 2**
- [ ] 并行目录遍历 (ignore crate)
- [ ] gitignore 支持
- [ ] Console + JSON 输出
- [ ] 基本 CLI 实现

### Phase 2: 功能完善 (2 周)

**Week 3**
- [ ] 智能排除 (项目类型检测)
- [ ] 配置文件支持 (TOML)
- [ ] CLI 参数完全兼容 Python 版
- [ ] CSV + Markdown 输出

**Week 4**
- [ ] HTML 报告 + 图表
- [ ] 复杂度分析
- [ ] Git 信息集成
- [ ] 集成测试

### Phase 3: 优化与增强 (1 周)

**Week 5**
- [ ] 性能基准测试 + 优化
- [ ] 用户自定义语言支持
- [ ] Shell 补全脚本
- [ ] 跨平台构建 (CI)
- [ ] 文档 + README

### Phase 4: 发布准备 (可选)

- [ ] crates.io 发布
- [ ] Homebrew formula
- [ ] GitHub Releases (预编译二进制)
- [ ] 迁移指南 (Python -> Rust)

---

## 11. 参考资料

- [tokei](https://github.com/XAMPPRocky/tokei) - Rust 代码统计工具
- [scc](https://github.com/boyter/scc) - Go 代码统计工具
- [ignore](https://docs.rs/ignore) - ripgrep 的目录遍历库
- [clap](https://docs.rs/clap) - Rust CLI 框架
- [askama](https://docs.rs/askama) - 编译时模板引擎
