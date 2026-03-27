# Changelog

## 1.0.0

First production release.

### Commands

- `snif index` — builds the repository index with structural graph, LLM
  summaries, and vector embeddings. Supports incremental updates.
- `snif review` — reviews code changes with full codebase context. Posts
  inline comments on GitHub PRs with a walkthrough summary. Supports JSON
  and SARIF output formats.
- `snif eval` — runs benchmark fixtures through the review pipeline and
  enforces quality gates on precision and noise rate.
- `snif clean` — removes local runtime data.

### Indexing

- Tree-sitter parsing for Rust, TypeScript, and Python.
- Structural graph: imports, symbols, references, co-change correlations.
- LLM-generated summaries for every function and file.
- Vector embeddings via fastembed (AllMiniLML6V2, 384 dimensions).
- Incremental indexing: skips unchanged files and existing summaries.

### Review

- Three-method context retrieval: structural graph traversal, semantic
  vector similarity, and keyword matching.
- Token-budgeted context assembly with ranked file selection.
- Provider-neutral LLM execution via any OpenAI-compatible endpoint.
- Static output filtering: confidence, evidence, impact, style suppression,
  deduplication.
- Stable finding fingerprints for annotation lifecycle management.
- PR summary comments with change walkthrough, context details, and outcome.
- SARIF 2.1.0 output for GitHub code scanning integration.

### Evaluation

- Directory-based benchmark fixtures with real source files.
- 25 fixtures: 10 bug, 10 clean, 5 style noise.
- Precision, recall, and noise rate metrics with quality gate enforcement.

### Platform

- GitHub adapter: PR diff/metadata fetching, inline review comments,
  stale finding resolution, PR description and labels in context.
- Feedback learning system: signal collection, embedding storage,
  similarity-based filter (built, activates after threshold signals).

### CI

- GitHub Actions workflows: CI (check, test, clippy, fmt), PR review,
  eval quality gate, cross-platform release via cargo-dist.
- Installers: shell, PowerShell, Homebrew.
- Build targets: Linux x86_64, macOS x86_64 + aarch64, Windows x86_64.
