# TODO

Outstanding work for Snif beyond v2.0.0.



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
- [ ] [Make summarization concurrency configurable in .snif.json](https://github.com/AssahBismarkabah/Snif/issues/14) - currently hardcoded to `Semaphore::new(3)` in snif-summarizer
- [ ] Add eval fixtures for TypeScript, Python, and Java - multi-language parsing is supported but only Rust fixtures exist in the eval harness


# Monitoring and Dashboards

- [ ] [Connect eval harness to Braintrust monitoring dashboard](https://github.com/AssahBismarkabah/Snif/issues/15) - push eval results (precision, recall, noise_rate, per-fixture TP/FP/FN) to Braintrust or equivalent platform for trend tracking and visualization

# Documentation

- Update docs/ci.md with GitHub App setup instructions once the App exists
