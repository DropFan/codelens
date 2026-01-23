# Codelens

High performance code statistics tool written in Rust.

## Features

- **Fast**: Parallel file traversal, 30-50x faster than Python alternatives
- **65+ Languages**: Built-in support for popular programming languages
- **Smart Filtering**: Respects `.gitignore`, auto-excludes build directories
- **Multiple Outputs**: Console, JSON, CSV, Markdown, HTML with charts
- **Complexity Analysis**: Function count, cyclomatic complexity, nesting depth
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

## Performance

Benchmarks on a large codebase (~100k files):

| Tool | Time |
|------|------|
| cloc (Perl) | ~60s |
| code-stats (Python) | ~45s |
| **codelens (Rust)** | **~1.5s** |

## License

MIT
