# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

<!-- next-header -->

## [Unreleased]

### Added
- `codelens diff <FROM> [<TO>]` (or `FROM..TO`) — compare two git refs
  (TO defaults to the working tree) with health-score movement,
  per-file grade regressions, and complexity deltas as the primary
  output; `--fail-on-regression` gates CI
- Duplication as the sixth health dimension: ULOC + DRYness% in the
  stats summary, per-file duplicated-line counts, copy-paste directly
  moves the health score (lines under 8 trimmed bytes never count,
  keeping structural brace/import noise out of the signal)
- Knowledge risk in hotspots: per-file author count and ownership
  share; a MED/HIGH-risk file owned ≥75% by one author is flagged as a
  knowledge island (frequently changed + complex + one person knows it)
- `--no-dup-scan` — skip line-duplication collection (saves hundreds
  of MB on very large repos); the Duplication health dimension is
  omitted and the remaining weights renormalized, never faked as a
  perfect score
- `--languages-file <PATH>` — load custom language definitions from a
  TOML file (also settable in `.codelens.toml`, resolved relative to
  the config file); definitions with the same id replace the built-in
  entirely, and invalid regexes are rejected at load time

### Changed
- `codelens coupling` now excludes test files by default (previously
  they participated in pairing, so expected test↔impl pairs like
  `foo.go` / `foo_test.go` drowned the hidden-dependency signal);
  `--include-tests` restores the old behavior, and `--for` pointed at
  a test file implies it. Bulk-commit detection (`--max-changeset`)
  now uses the commit's own file count, taken before tree/test
  filtering, so a bulk commit can no longer slip under the threshold
  when part of it is filtered away. Test detection now shares one
  conservative rule set with the summary's test-code stats, which
  therefore flag slightly fewer files (e.g. `test_helper.rb` and
  `NewsTest.js` no longer count as tests)
- Old trend snapshots lack duplication data and default to a perfect
  duplication score; regenerate baselines for meaningful comparisons
- `.m` files are now always counted as Objective-C (matching scc's
  dominant real-world output); previously the MATLAB/Objective-C
  winner was random per run. MATLAB repos: use `--count-as m:matlab`
- `health --fail-under` gates on the current analysis's own scoring
  model; a baseline snapshot's capture settings no longer change the
  absolute score

### Fixed
- `--list-languages` printed every language twice and doubled the total
- `codelens diff A..B C` now errors instead of silently ignoring `C`
- Release pipeline: draft releases are published automatically after
  all builds complete, and Homebrew checksum downloads fail loudly

## [0.1.6] - 2026-08-08

### Added
- `codelens coupling` — change coupling analysis: file pairs that keep
  changing in the same commits (hidden dependencies), with
  `--min-shared` / `--min-coupling` / `--max-changeset` noise controls
  and `--for FILE` blast-radius view
- `codelens health --baseline <REF> --fail-on-regression` — regression
  gate against a trend snapshot or any git ref (materialized via a
  temporary worktree): only grade drops fail CI, never pre-existing
  debt ("clean as you code")
- `codelens mcp` — MCP server over stdio for AI coding agents
  (repo_overview / code_health / hotspots / change_coupling /
  file_metrics), behind the default-on `mcp` cargo feature
- Official GitHub Action (`action.yml`): health gate + sticky PR
  comment + step summary + badge JSON, plus `.pre-commit-hooks.yaml`
- `hotspot --functions` — function-level hotspot breakdown (diff hunks
  intersected with heuristic function spans, no AST)
- Hotspot `Age` column: days since each file's first commit,
  rename-aware full-history scan
- `--by-dir` / `--dir-depth` — cumulative directory tree statistics
- `--tokens` — estimated LLM token counts per language plus
  context-window fit (byte-based estimate, always shown with ≈)
- Test code separation: Test Files / Test Code Lines / Test-to-Code
  ratio in the summary, detected by path conventions
- Cognitive complexity metric (nesting-weighted control flow),
  alongside cyclomatic in stats and per-file data
- `-f sarif` — SARIF 2.1.0 output for GitHub code scanning / reviewdog
- `.gitattributes` linguist support: `linguist-language` overrides,
  `linguist-vendored`/`linguist-generated` exclusion (`--no-linguist`
  opts out)
- Trend HTML report now charts the full snapshot history (code lines +
  complexity), and trend JSON carries the history series

### Changed
- Git collection now parses per-commit records (author, timestamp,
  per-file changes) as the shared foundation for coupling, code age,
  and future author analyses; `FileChurn` gained `last_commit_ts`

### Fixed
- Non-ASCII filenames were C-quoted by git and silently dropped from
  churn/hotspot/coupling/age (`core.quotepath` now disabled)
- Rename tracking was time-blind: a new file created under a
  renamed-away name had its history folded into the rename target
- `health --baseline` compared mismatched scopes (and fabricated a
  regression) when the analyzed path did not exist in the baseline;
  now a hard error
- `--by-dir` produced meaningless "/", "/Users" rows for absolute
  path arguments; the tree now anchors at the files' common root

## [0.1.5] - 2026-08-08

### Added
- `codelens health --fail-under <GRADE|SCORE>` — CI quality gate: exits
  non-zero when project health is below a grade (A/B/C/D) or numeric
  score, so a PR can be blocked on declining code health

### Fixed
- Health metrics no longer distorted by false signals:
  - nesting depth ignored brackets inside string literals, char
    literals, and line comments (an ANSI-colored help string alone
    produced a fake depth of 126)
  - average function length divided ALL code lines (including
    Markdown/HTML documents that have no functions) by the function
    count; only files containing functions now contribute
  - document and data formats (Markdown/HTML/JSON/TOML/YAML/CSS) no
    longer report complexity metrics at all — bracket depth in a JSON
    file is not a code-nesting signal

### Changed
- Flattened deeply nested code in the parallel walker (extracted
  `classify_entry` dispatch), git rename parsing, and trie construction

## [0.1.4] - 2026-08-08

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
