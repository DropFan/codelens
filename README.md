# Codelens

High performance code analysis tool written in Rust — stats, health scores, hotspots, change coupling, trends, cost estimation, and CI/AI quality gates.

## Features

- **Fast**: Parallel file traversal, 30-50x faster than Python alternatives
- **65+ Languages**: Built-in support for popular programming languages
- **Smart Filtering**: Respects `.gitignore`, `.gitattributes` linguist attributes, auto-excludes build directories
- **Multiple Outputs**: Console, JSON, CSV, Markdown, HTML with charts, OpenMetrics, badge JSON, SARIF
- **Complexity Analysis**: Function count, cyclomatic + cognitive complexity, nesting depth
- **Health Score**: Project/directory/file-level health grading (A-F) with pluggable scoring models
- **CI Quality Gates**: `--fail-under` absolute gate and `--baseline` regression gate ("clean as you code"), plus an official GitHub Action and pre-commit hooks
- **Hotspot Detection**: Risky files via churn × complexity, with code age and function-level breakdown
- **Change Coupling**: Files that keep changing together — hidden dependencies the module structure doesn't show
- **Trend Tracking**: Save snapshots, compare evolution, chart the full history
- **Cost Estimation**: Multi-model development cost estimation (COCOMO Basic/II, Putnam, LOCOMO)
- **LLM Token Estimation**: How many tokens a repo is, and whether it fits a model's context window
- **AI Agent Integration**: Built-in MCP server (`codelens mcp`) for Claude Code, Cursor, and friends
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
codelens health . --fail-under B   # CI gate: exit 1 if health is below B
codelens health . --baseline main --fail-on-regression   # regression gate
```

`--fail-under` accepts a grade (`A`/`B`/`C`/`D`) or a numeric score
(`75`), turning the health report into a CI quality gate — fail a PR
when project health drops below your threshold.

`--baseline` compares against a trend snapshot (`latest`, `latest~1`, a
date) or any git ref (`main`, `HEAD~1`, a tag — analyzed via a temporary
worktree). With `--fail-on-regression`, the gate fails only when the
project letter grade drops or a file present in both trees drops a
grade: legacy debt never blocks a PR, only the changes do ("clean as
you code"). The delta ("B 87.9 → C 77.2") renders in console, markdown
(great for PR comments), and JSON.

### Hotspot Detection

Find the riskiest files by combining git change frequency (churn) with code complexity — files that change often AND are complex are the most likely sources of bugs.

```bash
codelens hotspot .              # Last 90 days (default)
codelens hotspot . --since 30d  # Last 30 days
codelens hotspot . --since 6m --top 5  # Last 6 months, top 5
codelens hotspot . --functions  # Which functions inside absorb the churn
```

Each hotspot shows its **age** (days since first commit, rename-aware):
an old file that is still a hotspot signals chronic instability.
`--functions` intersects diff hunks with function spans to show which
functions inside the top files actually change (approximate, no AST).

### Change Coupling

Find file pairs that keep changing in the same commits — hidden dependencies the module structure does not express, and prime refactoring targets.

```bash
codelens coupling .                   # Last 90 days, noise-filtered
codelens coupling . --for src/api.rs  # What changes together with this file
codelens coupling . --min-shared 3 --min-coupling 20  # Lower thresholds
```

Bulk commits (more than `--max-changeset` files, default 30) are
excluded from pairing and reported, so formatting sweeps don't fake
coupling.

### Trend Tracking

Save snapshots and compare codebase evolution over time. Snapshots are stored in `.codelens/snapshots/`. Use `latest`, `latest~N`, or a date prefix like `2025-01-01` as references.

```bash
codelens trend --save --label v1.0     # Save a labeled snapshot
codelens trend                         # Compare latest two snapshots
codelens trend --list                  # List all snapshots
codelens trend --compare latest~2 latest  # Compare specific snapshots
```

### Cost Estimation

Estimate development cost, schedule, and team size using four pluggable models. Default mode runs all models and shows a comparison table.

```bash
codelens estimate .                       # All models comparison (default)
codelens estimate . --model cocomo-basic   # Single model with per-language breakdown
codelens estimate . --model cocomo2        # COCOMO II Post-Architecture
codelens estimate . --model putnam --ck 11000  # Putnam with custom productivity
codelens estimate . --model locomo         # LLM generation cost
codelens estimate . --avg-wage 120000      # Custom salary across all models
```

| Model | Description | Reference | Typical Use |
|-------|-------------|-----------|-------------|
| [COCOMO Basic](https://en.wikipedia.org/wiki/COCOMO) | Classic Boehm 1981 regression | *Software Engineering Economics*, Boehm 1981 (ISBN 0-13-822122-7) | Quick estimates, scc comparison |
| [COCOMO II](https://en.wikipedia.org/wiki/COCOMO) | Modern 2000 calibration with scale factors | *Software Cost Estimation with COCOMO II*, Boehm et al. 2000 (ISBN 0-13-026692-2) | Organization-level planning |
| [Putnam/SLIM](https://en.wikipedia.org/wiki/Putnam_model) | Rayleigh-curve conservative model | *A General Empirical Solution to the Macro Software Sizing and Estimating Problem*, IEEE TSE 1978 | Risk assessment, worst case |
| [LOCOMO](https://github.com/boyter/scc?tab=readme-ov-file#locomo) | LLM token cost model | [scc LOCOMO model](https://github.com/boyter/scc?tab=readme-ov-file#locomo), Boyter 2026 | AI-assisted development cost |

## Output Formats

| Format | Flag | Description |
|--------|------|-------------|
| Console | `-f console` | Colored terminal output (default) |
| JSON | `-f json` | Structured data for processing |
| CSV | `-f csv` | Spreadsheet compatible |
| Markdown | `-f markdown` | Documentation friendly |
| HTML | `-f html` | Interactive report with charts |
| OpenMetrics | `-f openmetrics` | Prometheus text format for scraping |
| Badge | `-f badge` | shields.io endpoint JSON (`codelens health -f badge` → live code-health badge) |
| SARIF | `-f sarif` | SARIF 2.1.0 for GitHub code scanning (`upload-sarif`) or `reviewdog -f=sarif` |

## CI Integration

**GitHub Action** — health gate + sticky PR comment + step summary in one step (see [docs/github-action.md](docs/github-action.md)):

```yaml
- uses: DropFan/codelens@rust
  with:
    fail-under: 'C'
    baseline: 'origin/${{ github.base_ref }}'
    fail-on-regression: 'true'
```

**pre-commit** — gate commits locally with the bundled [.pre-commit-hooks.yaml](.pre-commit-hooks.yaml). Install codelens first (the hooks run the binary on your PATH), and pin `rev` to v0.1.6-rust or newer (earlier tags do not ship the hook manifest):

```yaml
repos:
  - repo: https://github.com/DropFan/codelens
    rev: v0.1.6-rust
    hooks:
      - id: codelens-health
        args: ['--fail-under', 'C']
```

## AI Agent Integration

`codelens mcp` runs a built-in MCP server so coding agents can query repository stats, health, hotspots, and coupling before editing code (see [docs/ai-integration.md](docs/ai-integration.md)):

```bash
claude mcp add codelens -- codelens mcp
```

`codelens . --tokens` estimates the repository's LLM token count and whether it fits common context windows (byte-based estimate).

## Filtering & Detection

```bash
codelens --by-file --top 20         # Per-file statistics (respects --sort/--top)
codelens --by-dir --dir-depth 2     # Directory tree rollups (files/code/complexity)
codelens --count-as jsp:html        # Count .jsp files as HTML
codelens --no-duplicates            # Skip files with identical content
codelens --no-min-gen               # Skip minified/generated files
```

- Extensionless scripts are detected via shebang (`#!/usr/bin/env python`).
- Drop a `.codelensignore` file (gitignore syntax) anywhere in the tree to
  exclude paths, like scc's `.sccignore` / tokei's `.tokeignore`.
- `.gitattributes` linguist attributes are honored by default so numbers
  match GitHub: `linguist-language=X` overrides detection,
  `linguist-vendored` / `linguist-generated` exclude files
  (`--no-linguist` opts out).
- Files matching test conventions (`tests/`, `*_test.go`, `*.spec.ts`,
  `FooTest.java`, ...) are reported separately with a test/code ratio.

## Configuration

Create `.codelens.toml` in your project root. CLI flags override config file
values, which override built-in defaults:

```toml
# Exclude patterns
excludes = "*test*,*mock*"

# Target languages
lang = "rust,go,python"

# Extension remapping
count_as = "jsp:html,tpl:php"

# Output format
output = "json"

# Per-file statistics
by_file = true

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
