---
name: codelens
description: >-
  Analyze codebase health with the codelens CLI: code stats, health scores
  (A-F), change hotspots, change coupling, ref-to-ref health diffs, and CI
  quality gates. Use this skill whenever the user asks about code quality,
  code health, technical debt, refactoring risk, "which files are risky or
  need refactoring", "what usually changes together", knowledge silos / bus
  factor, sizing up an unfamiliar repository, proving a refactor actually
  helped, or wiring a quality gate into CI or pre-commit — even when the user
  never mentions codelens by name. Also use it before large refactors to
  capture a baseline, and after them to measure the delta.
---

# Codelens: code health analysis workflows

Codelens is a fast code analysis CLI (Rust). It reads a source tree (and its
git history) and answers: how big is this codebase, how healthy is it, where
are the risky files, what changes together, and did this change make things
better or worse.

## Ground rules

- **`--help` is the source of truth.** Run `codelens --help` and
  `codelens <subcommand> --help` for flags, defaults, and output formats.
  This skill teaches workflows and interpretation; do not assume flag
  details from memory.
- **Use `-f json` when you consume the output yourself.** The console
  format is for humans. Explore the JSON shape with `jq` keys rather than
  assuming a schema.
- **If a codelens MCP server is connected** (tools like `code_health`,
  `hotspots`, `change_coupling`), prefer those tools for quick read-only
  queries; fall back to the CLI for everything they don't cover (diffs,
  trends, gates, custom languages).
- If `codelens` is not installed: `brew install DropFan/tap/codelens` or
  `cargo install codelens` (source: https://github.com/DropFan/codelens).

## Workflow 1 — Size up an unfamiliar repository

Use when onboarding onto a codebase or before estimating work in it.

```bash
codelens .                      # stats + health + cost estimate in one report
codelens health . --top 10      # worst files and directories, A-F grades
codelens hotspot .              # risky files: frequently changed AND complex
```

Read it in this order: overall size and language mix → project grade →
which directories drag the grade down → which files are hotspots. A file
that is both a hotspot and low-graded is where bugs most likely live; that
is the first place to be careful in.

The hotspot table also flags **knowledge islands**: risky files owned
almost entirely by one author. Treat those as review/bus-factor risks, not
just code-quality risks.

## Workflow 2 — Risk assessment before touching a file

Use before editing or refactoring a specific file the user cares about.

```bash
codelens hotspot . --functions          # which functions absorb the churn
codelens coupling . --for <FILE>        # what usually changes together with it
codelens health <dir-of-file> --top 5   # local health context
```

Interpretation:

- Coupling percentages are `shared commits / average commits of the pair`.
  High coupling with a file *outside* its module is a hidden dependency —
  plan to touch both, or you will ship half a change.
- Test files are excluded from coupling by default (a test changing with
  its implementation is expected, not a hidden dependency). Add
  `--include-tests` when the user explicitly wants the test blast radius;
  pointing `--for` at a test file includes tests automatically.
- `--functions` shows which functions inside a hotspot actually absorb the
  churn, so a refactor can target the hot function instead of the whole file.
- An empty coupling result usually means the default noise thresholds are
  too strict for a low-churn file, not that nothing is coupled — lower
  `--min-shared` and `--min-coupling` and retry before concluding the file
  is isolated.

## Workflow 3 — Prove a refactor helped (or catch a regression)

Capture evidence, don't guess. Two interchangeable approaches:

**Git refs** (no preparation needed):

```bash
codelens diff <BASE-REF>                # base ref vs working tree
codelens diff <REF-A>..<REF-B>          # two refs
```

**Snapshots** (works without clean refs, persists across sessions):

```bash
codelens trend --save --label before-refactor    # before starting
codelens health . --baseline latest              # after: delta vs snapshot
```

Report the movement, not just the endpoint: "health 79.7 (C) → 84.2 (B),
file-size dimension F → A" is the sentence the user wants. Per-file grade
drops listed by `diff` are the regressions to fix before shipping.

## Workflow 4 — Quality gates in CI

Use when the user wants declining code health to block PRs.

```bash
codelens health . --fail-under B                        # absolute gate
codelens health . --baseline main --fail-on-regression  # only NEW debt fails
codelens diff main --fail-on-regression                 # ref-to-ref gate
```

Choosing a gate: `--fail-under` enforces a floor (legacy debt counts);
`--fail-on-regression` implements "clean as you code" (only changes fail
the gate, pre-existing debt never does) — usually the right default for
older codebases. There is an official GitHub Action and a pre-commit hook;
see the repository README for current wiring.

## Interpretation guide (stable concepts)

- **Grades**: A is healthy, C is "needs attention", F drags the project
  down. The project score aggregates per-dimension scores (complexity,
  function size, comments, file size, nesting, duplication — run
  `codelens health --help` for the current set).
- **Hotspot = churn × complexity.** Frequently changed simple files are
  fine; complex stable files are fine; the intersection is where defects
  cluster. Age and author concentration qualify the risk.
- **A dimension can be legitimately absent.** On very large repositories,
  `--no-dup-scan` skips duplication collection to save memory; the health
  score then omits that dimension and renormalizes — it is *not* scored as
  perfect. Machine-readable output names the scoring model so you can tell.
- **Expected noise**: generated/vendored code is auto-excluded by default
  (gitignore, linguist attributes, smart excludes). If numbers look
  inflated, check what was scanned before distrusting the tool — `--by-file`
  or `-v` shows what got counted.
- **Test stats count separate test FILES only.** Languages that inline
  tests into source files (Rust `#[cfg(test)]`, D `unittest`) will show a
  near-zero test ratio even with excellent coverage. Verify with a quick
  grep before reporting "this project lacks tests".

## Situational flags worth knowing

| Situation | Flag |
|---|---|
| Huge repo, memory pressure | `--no-dup-scan` |
| Agent-consumed output | `-f json` (also: csv, markdown, html, sarif, openmetrics) |
| Unrecognized in-house language / DSL | `--languages-file <toml>` (schema in README) |
| Extension counted as wrong language | `--count-as ext:lang` |
| Monorepo subtree | pass the subdirectory as the path argument |

Anything else: `codelens <subcommand> --help`.
