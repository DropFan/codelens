# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

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
