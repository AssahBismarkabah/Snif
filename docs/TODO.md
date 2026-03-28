# TODO

Outstanding work for Snif beyond v2.0.0.





# Evaluation and Tuning

The eval harness passes with 25 fixtures but the fixture set should grow over
time with real-world examples.

- Expand fixtures from 25 toward 50 using real diffs from production repos
- Track eval results over time to detect regressions
- Tune prompts based on production feedback data
- Activate the feedback learning system once enough signals accumulate


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
