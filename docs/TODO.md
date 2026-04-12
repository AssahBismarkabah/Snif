# TODO

Outstanding work for Snif beyond v2.0.0.



### from gitlab setup what went wrong and needs to be addressed


- Large diffs with many real file changes still fail when the diff alone
  exceeds the token budget. Phase 2 chunked parallel review needed — split
  the diff by file boundaries, run parallel LLM calls, merge findings.
# Evaluation and Tuning

The eval harness passes quality gates (Precision >= 70%, Recall >= 60%,
Noise <= 20%) with 50 fixtures across Rust, TypeScript, Python, and Java.

- ~~Expand fixtures from 25 toward 50 using real diffs from production repos~~ — DONE, 50 fixtures
- ~~Track eval results over time to detect regressions~~ — DONE, history.rs appends JSONL records with per-fixture TP/FP/FN breakdowns and aggregate regression warnings
- ~~Activate the feedback learning system once enough signals accumulate~~ — DONE, Phase 1 implemented (crates/snif-eval/src/adapter.rs). Analyzes last 5 eval records for precision/recall/noise trends and persistent fixture FP/FN patterns, generates guidance text appended to the system prompt before each run
- [ ] Phase 2: Wire snif-feedback crate (embeddings + SQLite + KNN) into eval pipeline — store TP findings as "accepted" signals and FP findings as "dismissed" signals, run apply_feedback_filter() on raw findings to suppress findings similar to historical dismissals
- [ ] Stabilise intermittent LLM non-determinism — integer-overflow and ts-type-assertion-crash fixtures occasionally lose detection; consider running each fixture 2-3x and aggregating, or tightening fixture code to reduce ambiguity


# Production Hardening

- Add unit and integration tests to the main codebase
- Handle edge cases: very large diffs, binary files in PRs, empty PRs
- Rate limit handling: detect provider rate limits and back off gracefully
  (beyond the current retry logic)
- Support configurable summarization concurrency in .snif.json (currently
  hardcoded to 3)
- Support multiple languages in the same repository (currently parses all
  supported languages but fixtures are Rust-only)


# Documentation

- Update docs/ci.md with GitHub App setup instructions once the App exists
