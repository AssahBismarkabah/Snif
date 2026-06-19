# TODO

Outstanding work for Snif beyond v2.0.0.

### Completed: Scalable Indexing (Phase 1 & 2)

- [x] Content-hash-based summary invalidation — summaries are keyed by a SHA-256 hash of the source code. Re-indexing skips unchanged symbols automatically. Stale summaries (hash mismatch) are deleted and regenerated. File-level summaries use a hash of their child summaries.
- [x] `snif index` now defaults to structural-only (no LLM calls). `snif index --full-index` pre-warms all summaries and embeddings. `snif index --rebuild` resets the database and rebuilds from scratch.
- [x] On-demand summarization in `snif review` — when the review command encounters files without summaries, it generates them before building context. Only pays for the ~5-15 files a review actually needs.
- [x] `summarize_files()` public API for targeted summarization — focuses on specific file paths rather than the entire repo.
- [x] `has_summaries_missing_embeddings()` moved to `snif-embeddings` crate as a public function, shared by both index and review commands.
- [x] Schema version bumped to 4 to accommodate `content_hash` column in `summaries` table.

### from gitlab setup what went wrong and needs to be addressed

- [ ] [Chunked parallel review for large diffs](https://github.com/AssahBismarkabah/Snif/issues/12) - when a PR diff plus file content exceeds the token budget, the LLM only sees diffs without full file context for many files. Current behavior degrades gracefully (files get marked DiffOnly), but review quality suffers for PRs with 20+ changed files. Solution: split diff by file boundaries, run parallel LLM calls, merge findings.

# Evaluation and Tuning

The eval harness passes quality gates (Precision >= 70%, Recall >= 60%,
Noise <= 20%) with 50 fixtures across Rust, TypeScript, Python, and Java.

- ~~Expand fixtures from 25 toward 50 using real diffs from production repos~~ - DONE, 50 fixtures
- ~~Track eval results over time to detect regressions~~ - DONE, history.rs appends JSONL records with per-fixture TP/FP/FN breakdowns and aggregate regression warnings
- ~~Activate the feedback learning system once enough signals accumulate~~ - DONE, Phase 1 implemented (crates/snif-eval/src/adapter.rs). Analyzes last 5 eval records for precision/recall/noise trends and persistent fixture FP/FN patterns, generates guidance text appended to the system prompt before each run
- [ ] [Wire snif-feedback crate (embeddings + SQLite + KNN) into eval pipeline](https://github.com/AssahBismarkabah/Snif/issues/9) - store TP findings as "accepted" signals and FP findings as "dismissed" signals, run apply_feedback_filter() on raw findings to suppress findings similar to historical dismissals
- [ ] [Stabilise intermittent LLM non-determinism](https://github.com/AssahBismarkabah/Snif/issues/10) - integer-overflow and ts-type-assertion-crash fixtures occasionally lose detection; consider running each fixture 2-3x and aggregating, or tightening fixture code to reduce ambiguity


# Production Hardening

- [ ] [Add unit and integration tests for core crates](https://github.com/AssahBismarkabah/Snif/issues/13) - snif-store, snif-context, snif-platform, snif-summarizer, snif-embeddings, snif-retrieval, snif-cli all have zero tests
- [x] Handle edge cases: very large diffs, binary files in PRs, empty PRs
  - Large diffs: graceful degradation implemented (DiffOnly tiering, budget trimming). [Chunked parallel review](https://github.com/AssahBismarkabah/Snif/issues/12) tracks the enhancement.
  - Binary files: files >50KB excluded, non-reviewable types (lockfiles, bundles) excluded. Remaining edge cases handled by graceful degradation.
  - Empty PRs: reviewed normally, produces zero findings when diff is clean.
- [x] Rate limit handling: exponential backoff with 5 max retries for 429, 408, and 5xx errors
- [x] [Make summarization concurrency configurable in .snif.json](https://github.com/AssahBismarkabah/Snif/issues/14) - added `context.summarizer_concurrency` for provider-specific indexing rate limits
- [x] [Handle Hugging Face rate limits when loading embedding model](https://github.com/AssahBismarkabah/Snif/issues/27) - cache FastEmbed model files, add `snif warm-embeddings`, and degrade only semantic indexing/retrieval on model-download 429s
- [x] Treat provider pressure (`429`, `408`, `502`, `503`, `504`, and upstream timeouts) as reducible failures - review retries with smaller prompt and completion budgets only after provider failure, truncating diff context only as a final fallback, and index summarization stops after sustained pressure while preserving completed summaries
- [x] Mark malformed or contradictory review output as inconclusive instead of reporting a false clean review
- [ ] Add eval fixtures for TypeScript, Python, and Java - multi-language parsing is supported but only Rust fixtures exist in the eval harness


# Monitoring and Dashboards

- [ ] [Connect eval harness to Braintrust monitoring dashboard](https://github.com/AssahBismarkabah/Snif/issues/15) - push eval results (precision, recall, noise_rate, per-fixture TP/FP/FN) to Braintrust or equivalent platform for trend tracking and visualization

# Documentation

- Update docs/ci.md with GitHub App setup instructions once the App exists
