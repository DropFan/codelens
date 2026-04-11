# Codelens

High performance code analysis tool written in Rust — stats, health scores, hotspots, and trends.

## Features

- **Fast**: Parallel file traversal, 30-50x faster than Python alternatives
- **65+ Languages**: Built-in support for popular programming languages
- **Smart Filtering**: Respects `.gitignore`, auto-excludes build directories
- **Multiple Outputs**: Console, JSON, CSV, Markdown, HTML with charts
- **Complexity Analysis**: Function count, cyclomatic complexity, nesting depth
- **Health Score**: Project/directory/file-level health grading (A-F) with pluggable scoring models
- **Hotspot Detection**: Identify risky files via churn × complexity analysis
- **Trend Tracking**: Save snapshots and compare codebase evolution over time
- **Extensible**: Add custom languages via TOML configuration

## Installation

### Homebrew (macOS/Linux)

```bash
brew install DropFan/tap/codelens
```

### Cargo

```bash
cargo install codelens
```

### Build from source

```bash
git clone https://github.com/DropFan/codelens
cd codelens
cargo build --release
```

## Usage

```bash
# Analyze current directory
codelens

# Analyze specific directories
codelens src tests

# Only count specific languages
codelens -l rust,go,python

# Output JSON
codelens -f json -O stats.json

# Output HTML report
codelens -f html -O report.html

# Show top 20 languages by code lines
codelens --top 20 --sort code

# Exclude directories
codelens --exclude vendor,dist,node_modules

# List supported languages
codelens --list-languages
```

### Health Score

Score code health across five dimensions (complexity, function size, comment ratio, file size, nesting depth) with grades from A to F.

```bash
codelens health .               # Project, directory, and file-level report
codelens health . --top 20      # Show top 20 worst files
codelens health . -f json       # Output as JSON
```

### Hotspot Detection

Find the riskiest files by combining git change frequency (churn) with code complexity — files that change often AND are complex are the most likely sources of bugs.

```bash
codelens hotspot .              # Last 90 days (default)
codelens hotspot . --since 30d  # Last 30 days
codelens hotspot . --since 6m --top 5  # Last 6 months, top 5
```

### Trend Tracking

Save snapshots and compare codebase evolution over time. Snapshots are stored in `.codelens/snapshots/`. Use `latest`, `latest~N`, or a date prefix like `2025-01-01` as references.

```bash
codelens trend --save --label v1.0     # Save a labeled snapshot
codelens trend                         # Compare latest two snapshots
codelens trend --list                  # List all snapshots
codelens trend --compare latest~2 latest  # Compare specific snapshots
```

## Output Formats

| Format | Flag | Description |
|--------|------|-------------|
| Console | `-f console` | Colored terminal output (default) |
| JSON | `-f json` | Structured data for processing |
| CSV | `-f csv` | Spreadsheet compatible |
| Markdown | `-f markdown` | Documentation friendly |
| HTML | `-f html` | Interactive report with charts |

## Configuration

Create `.codelens.toml` in your project root:

```toml
# Exclude patterns
excludes = "*test*,*mock*"

# Target languages
lang = "rust,go,python"

# Output format
output = "json"

# Threading
threads = 8

# Depth limit
depth = 10

# Show git info
git_info = true
```

## Custom Languages (Planned)

> **Note**: This feature is planned but not yet implemented.

Custom language definitions will be supported in `~/.config/codelens/languages.toml`:

```toml
[mylang]
name = "MyLang"
extensions = [".ml", ".mli"]
line_comments = ["#"]
block_comments = [["/*", "*/"]]
function_pattern = "^\\s*def\\s+\\w+"
complexity_keywords = ["if", "for", "while"]
```

## License

MIT
