# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

### Fixed
- **`-l` language filtering now works** — the flag was parsed but never
  applied (dead `target_languages` field); filtering now happens at
  language detection, case-insensitively
- **Config files now take effect** — values were unconditionally
  overwritten by CLI defaults; loading now merges three layers
  (defaults → config file → explicit CLI args). A broken TOML found via
  the default search path is a hard error instead of silently ignored;
  the never-parseable `.code_stats.yaml` candidates were dropped
- **`--sort` now reorders the language table** (was hardcoded to
  code-lines-descending)
- Subcommands accept global args (`-j`, `--config`, `--git-info`), no
  longer drop `--exclude-files`/`--include-files`/`--min-lines`/`--max-lines`,
  and read config files; `trend --save` uses the same filter semantics
  as the main command
- **Machine formats are valid again**: the default command's three
  reports (stats + health + estimation) are emitted as ONE document —
  single JSON object, single combined HTML page, analysis-only CSV
- `-q` no longer suppresses writing an explicitly requested `-O` file
- `-O` files and piped output no longer contain ANSI color codes
- CSV text fields are quoted per RFC 4180
- **Line counting accuracy** (now matches scc exactly on this repo):
  string delimiters are per-language config instead of hardcoded `"`+`'`
  (Rust lifetimes/YAML apostrophes no longer misclassify whole sections),
  code after an inline `/* */` close is counted as code, string escapes
  no longer swallow newlines, nested block comments are tracked by depth,
  Ruby `=begin` requires line start and its fake docstring was removed,
  UTF-8 BOM is stripped
- **hotspot works from subdirectories and with absolute paths** (analysis
  paths are rewritten repo-root-relative to match git); renamed files
  keep their full churn history (`{old => new}` parsing with chains);
  an empty `--since` window warns instead of silently printing nothing
- Read/traversal failures are counted separately from filtered files and
  shown in the console footer as `(N skipped, M errors)`

### Added
- `--by-file` per-file statistics table (console/markdown append it,
  CSV replaces the language table), respecting `--sort` and `--top`
- `.codelensignore` project ignore files (gitignore syntax)
- `--count-as ext:lang,...` extension remapping (also `count_as` in
  config files)
- Shebang detection for extensionless scripts (`#!/usr/bin/env python`)
- `--no-duplicates` content-hash dedup of identical files
- `--no-min-gen` minified/generated file detection (long average line
  length or generated-code markers)
- `--locomo-preset large|medium|small|local` pricing tiers matching scc
- `-f openmetrics` (Prometheus text exposition) and `-f badge`
  (shields.io endpoint JSON) output formats

## [0.1.3] - 2026-04-12

### Added
- **`codelens estimate`** — Multi-model cost estimation subcommand
  - Four pluggable models via `EstimationModel` trait:
    - **COCOMO Basic** (Boehm 1981) — classic `E = a × KLOC^b × EAF`
    - **COCOMO II** (Boehm 2000) — modern calibration with 5 scale factors
    - **Putnam/SLIM** (1978) — Rayleigh-curve conservative estimate
    - **LOCOMO** — LLM token cost model (AI-era code generation)
  - `--model all` (default) shows comparison table across all four models
  - Per-language cost breakdown for single model mode
  - All model parameters configurable via CLI flags (`--eaf`, `--ck`, `--d0`, `--llm-input-price`, etc.)
  - Full output format support (console, JSON, CSV, Markdown, HTML with Chart.js)
- Default `codelens` output now includes health score and estimation comparison alongside statistics
- HTML estimation templates with consistent header/footer matching existing report pages

### Changed
- `Report` enum extended with `Estimation` and `EstimationComparison` variants
- CLI help and examples updated to reflect new `estimate` subcommand

### Removed
- `--show-estimate` flag (superseded by default output including estimation)

## [0.1.2] - 2026-04-12

### Fixed
- Deadlock in parallel walker when scanning repositories with more than 1000 files, caused by bounded channel consumer starting only after blocking `run()` call returns; fixed by running consumer concurrently via `std::thread::scope`

## [0.1.1] - 2026-04-11

### Fixed
- Hotspot analysis returning empty results due to path prefix mismatch (`./src/main.rs` vs `src/main.rs`)

## [0.1.0] - 2026-04-11

### Added
- **`codelens health`** — Code health scoring with pluggable scoring models
  - Five dimensions: complexity, function size, comment ratio, file size, nesting depth
  - Three-level reporting: project, directory, and file
  - Grades from A (best) to F (worst)
  - `ScoringModel` trait for custom scoring strategies
- **`codelens hotspot`** — Change hotspot detection via churn × complexity
  - Integrates git change frequency with code complexity
  - Risk levels: HIGH / MED / LOW
  - Configurable time window (`--since 30d`, `6m`, `1y`, or `YYYY-MM-DD`)
- **`codelens trend`** — Codebase trend tracking with snapshots
  - Save snapshots with `--save` and optional `--label`
  - Compare snapshots with `--compare` (supports `latest`, `latest~N`, date prefix)
  - Per-language change tracking (Added / Removed / Changed)
  - Snapshots stored in `.codelens/snapshots/` as JSON
- Git module (`git/`) for repository integration via system git CLI
- Full output format support (console, JSON, CSV, Markdown, HTML) for all new commands
- Interactive HTML reports with Chart.js (radar chart for health, bar charts for hotspot/trend)
- Colored CLI help with clap Styles and grouped examples

### Changed
- `OutputFormat` trait generalized with `Report` enum to support multiple report types
- Project description updated to "High performance code analysis tool"
- P90 percentile used for nesting depth aggregation (instead of max) to reduce outlier impact

### Fixed
- `--top` argument name conflict between subcommands and `OutputArgs`

## [0.0.3] - 2026-04-10

### Added
- Byte-level state machine counter for fast line classification
- TokenTrie and ProcessMask (bloom filter) for unified token matching
- Precompiled regex patterns with OnceLock caching
- Criterion benchmarks for byte-level counter

### Changed
- FileAnalyzer switched to byte-level state machine
- Per-thread buffer reuse, read bytes instead of String for zero-copy processing

## [0.0.2] - 2026-01-26

### Added
- Multiline string support for Python, Rust, and other languages
- Improved filter rules for file exclusion

### Fixed
- Homebrew formula update workflow

## [0.0.1] - 2026-01-23

### Added
- Initial Rust implementation of codelens
- High-performance parallel file traversal using `ignore` crate
- Support for 70+ programming languages via TOML definitions
- Multiple output formats: Console, JSON, CSV, Markdown, HTML (with Chart.js)
- Smart directory exclusion based on project type detection
- Cyclomatic complexity analysis (function count, nesting depth)
- Respects `.gitignore` rules automatically
- Cross-platform support (Linux, macOS, Windows)
- Configuration file support (`.codelens.toml`)
- CI/CD pipeline with GitHub Actions
- Homebrew tap for macOS/Linux installation
