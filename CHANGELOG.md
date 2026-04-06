# Changelog

## 3.2.1

### Budget Enforcement

- Fix context assembly ignoring `output_reserve_tokens` — budget now correctly
  subtracts the output reserve before assembling context. Previously the full
  `max_tokens` was used as the input budget, producing prompts that filled
  99% of the context window.
- Increase default `output_reserve_tokens` from 4,096 to 32,000 (25% of 128K).
  Effective input budget is now 96K instead of 124K. Reduces model latency
  and mitigates "lost in the middle" quality degradation on long prompts.

### Retry and Timeout

- Retry on HTTP 429 (Too Many Requests) and 408 (Request Timeout) in addition
  to 5xx server errors. Previously 429 was treated as a permanent failure.
- Increase request timeout from 120s to 300s for large prompts.

## 3.2.0

### Stale Finding Resolution

- Fix stale finding resolution marking unfixed bugs as resolved after rebases; switch from line-based fingerprints to content-based matching.

### Budget-Aware Content Degradation

- Three content tiers for changed files: Full, SummaryOnly, DiffOnly. When
  the token budget is tight, files are progressively degraded instead of
  sending an over-budget prompt that fails with a 400 error.
- Files prioritized by diff hunk count — files with more changes get full
  content first.
- Non-reviewable file detection uses explicit flag instead of inferring from
  empty content. Empty files (e.g. __init__.py) are no longer misclassified
  as generated files.
- Summary cleared on DiffOnly tier to prevent unbudgeted tokens leaking into
  the rendered prompt.
- Trim loop extended as safety net — degrades the largest full-content
  changed file when the rendered prompt still exceeds budget after removing
  all related files.
- Token estimation for Full tier includes summary cost, preventing systematic
  underestimation.
- Hunk counter handles deleted files (+++ /dev/null) correctly — no longer
  inflates the preceding file's hunk count.

### Review Quality

- System prompt explicitly blocks micro-optimizations (unnecessary allocations,
  format patterns, iterator vs collect) unless code is in a hot path or
  processes unbounded input.
- Filter suppresses convention findings when style suppression is enabled.
  Convention findings without an explicit conventions file are style opinions.
- Prompt rendering uses file.content for degraded tiers, preserving the
  distinction between non-reviewable files and budget-degraded files.
- Line number instructions no longer contradictory for degraded files.

### CI and Workflow

- Removed SARIF upload from review workflow — snif-dev inline comments are
  the single source of findings. Eliminates duplicate Code Scanning alerts.
- Switched review model to gemini-3.1-pro.
- Removed security-events permission from review workflow.
- Removed SARIF references from CI docs.

## 3.1.8

- Skip full content for lock files and generated files in changed file context
- Non-reviewable files (pnpm-lock.yaml, package-lock.json, yarn.lock, Cargo.lock, *.min.js, etc.) use diff only
- Enforce 50KB per-file size limit — files exceeding limit get placeholder instead of full content
- Prevents prompt overflow when PRs include large generated files

## 3.1.7

- Fix context budget enforcement — budget now checked on rendered prompt, not raw content
- Token estimation changed from /4 to /3 (conservative, prevents underestimation on formatted code)
- Post-render trim loop removes lowest-ranked related files until prompt fits within `max_tokens`
- New config field `output_reserve_tokens` (default 4096) reserves space for model response
- No more hardcoded values — output reserve is configurable via `.snif.json`
- Fix cosign certificate identity in CI docs — uses `refs/heads/main` not `refs/tags`

## 3.1.6

- Fix sign-release workflow trigger — use workflow_run instead of release event (GITHUB_TOKEN limitation)

## 3.1.5

- Add Sigstore cosign keyless signing for all release artifacts
- Release checksums are now signed with GitHub OIDC identity and recorded in Sigstore transparency log
- CI docs updated with cosign signature verification for both GitHub Actions and GitLab CI
- No more `curl | sh` — all installation examples use pinned versions with checksum and signature verification

## 3.1.4

- Update cargo-dist to 0.31.0 — fixes Node.js 20 deprecation warnings in release workflow
- Release workflow now uses actions/checkout@v6, upload-artifact@v6, download-artifact@v7
- Clean up releasing doc — single changelog step, remove duplicate section

## 3.1.3

- Remove musl target from release — sqlite-vec uses BSD types incompatible with musl libc
- Switch TLS from native-tls to rustls — eliminates OpenSSL system dependency
- Alpine users should use `debian:bookworm-slim` base image with the glibc binary

## 3.1.2

- improve GitLab CI support, update documentation, and switch to Alpine base image
- update CI docs to replace Docker image with manual Snif installation steps
-  allow `--repo` flag fallback for GitLab auto-detection, update CI docs for clarity

## 2.0.0

### GitHub App

- Snif now posts as its own GitHub App identity instead of "github-actions".
  Custom name and avatar appear on all PR comments.
- Dual authentication: supports GitHub App credentials (SNIF_APP_ID,
  SNIF_APP_PRIVATE_KEY, SNIF_APP_INSTALLATION_ID) with automatic JWT signing
  and installation token exchange. Falls back to GITHUB_TOKEN for backward
  compatibility.
- App published at https://github.com/apps/snif-dev. Marketplace listing
  submitted.

### Review Quality

- PR summary now includes an LLM-generated walkthrough describing what the
  change does and why, not just file counts.
- Summary uses collapsible details section for review metadata — clean and
  scannable.
- Removed emojis from review comments for professional presentation.
- PR description, labels, and commit messages are now extracted from the
  GitHub API and included in the review prompt. The model understands the
  developer's stated intent, not just the diff.
- SARIF output written directly to file by the binary instead of relying on
  shell redirect. Fixes invalid SARIF when tracing logs contaminated stdout.
- Tracing explicitly writes to stderr to prevent mixing with JSON/SARIF
  output on stdout.

### Incremental Indexing

- Summarizer skips symbols and files that already have summaries in the
  database. Subsequent index runs only call the LLM for new or changed code.
- Embedder skips summaries that already have embeddings. Only new summaries
  are embedded.
- Index cache step added to CI workflow for faster incremental runs.

### Reliability

- LLM retry logic upgraded to exponential backoff (2s, 4s, 8s) with 3 max
  retries instead of flat 2s delay with 2 retries.
- Summarization concurrency reduced from 5 to 3 to avoid provider rate
  limiting.
- Request timeout set to 120 seconds.
- Windows build fixed: added msvc-crt-static=false for ONNX Runtime dynamic
  CRT linking compatibility.

### Output

- New `snif-output/summary.rs` module owns PR summary formatting. Extracted
  from the review command to follow single-responsibility principle.
- Parser updated to handle new response format: JSON object with `summary`
  and `findings` fields. Backward compatible with plain array format.

### Fixtures

- Restructured from flat JSON files to directory-based fixtures with real
  source files, separate patch files, and metadata-only fixture.json.
- Fixed 7 fixtures that contained real bugs in supposedly "clean" code
  (UTF-8 slicing, division by zero, type confusion, email validation,
  unused imports, misleading context).
- All 25 fixtures passing at 81.8% precision, 90% recall, 18.2% noise.

### Documentation

- Added CONTRIBUTING.md with guidelines for code changes, changelog updates,
  language adapters, fixtures, and prompt changes.
- Added docs/releasing.md with release process documentation.
- Added docs/ci.md with setup guides for GitHub Actions, GitLab CI, generic
  CI, and Docker.
- Added fixtures/README.md documenting the benchmark fixture format.
- Added crates/README.md describing each crate's responsibility.
- Added docs/TODO.md tracking outstanding work.
- Updated README with getting started guide, CLI reference, and doc links.

### CI

- Added ci.yml for standard Rust quality checks (check, test, clippy, fmt).
- Release workflow generated by cargo-dist with shell, PowerShell, and
  Homebrew installers.
- GitHub Actions updated to actions/checkout@v6 for Node.js 24 support.

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
