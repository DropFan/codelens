# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

### Added
- Initial Rust implementation of codelens
- High-performance parallel file traversal using `ignore` crate
- Support for 70+ programming languages
- Multiple output formats: Console, JSON, CSV, Markdown, HTML
- Smart directory exclusion based on project type detection
- Cyclomatic complexity analysis
- Respects `.gitignore` rules automatically
- Cross-platform support (Linux, macOS, Windows)

### Changed
- Complete rewrite from Python to Rust for 30-50x performance improvement

## [0.1.0] - YYYY-MM-DD

### Added
- First release of the Rust version
