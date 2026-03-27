# TODO

Outstanding work for Snif beyond v1.0.0.


# GitHub App

The bot currently posts as "github-actions" with the default GitHub logo.
Creating a GitHub App gives Snif its own identity.

- Create a GitHub App at github.com/settings/apps with the name "Snif"
- Upload a logo/avatar for the app
- Configure permissions: read pull requests, write pull request comments, read
  contents
- Update the GitHub adapter to authenticate as the App (JWT to installation
  token) instead of using GITHUB_TOKEN
- Install the App on the Snif repo for testing
- Publish to GitHub Marketplace when ready


# GitLab Adapter

The platform adapter trait is defined but only GitHub is implemented.

- Implement `platform::gitlab` adapter using the GitLab merge request API
- Support: fetch diff, fetch metadata (title, description, labels), post
  discussion threads, resolve stale threads
- Test against a real GitLab merge request
- Document GitLab setup in docs/ci.md


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
