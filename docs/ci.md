# CI Integration

This document covers how to configure Snif in CI pipelines across different
platforms.


# GitHub Actions

GitHub Actions is the primary supported platform. Snif posts findings as
inline PR review comments and uploads SARIF results to GitHub's security tab.

## Using a pre-built binary (recommended)

Download the binary from the latest release and run it in your workflow. This
is the recommended approach for repos adopting Snif.

```yaml
name: Snif Review

on:
  pull_request:
    types: [opened, synchronize]

permissions:
  contents: read
  pull-requests: write
  security-events: write

jobs:
  review:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4
        with:
          fetch-depth: 0

      - name: Install Snif
        run: |
          curl --proto '=https' --tlsv1.2 -LsSf \
            https://github.com/AssahBismarkabah/Snif/releases/latest/download/snif-installer.sh | sh

      - name: Index repository
        run: snif index --path .
        env:
          SNIF_API_KEY: ${{ secrets.SNIF_API_KEY }}

      - name: Review pull request
        run: |
          snif review \
            --path . \
            --repo "$GITHUB_REPO" \
            --pr "$PR_NUMBER" \
            --format sarif > findings.sarif
        env:
          GITHUB_TOKEN: ${{ secrets.GITHUB_TOKEN }}
          SNIF_API_KEY: ${{ secrets.SNIF_API_KEY }}
          GITHUB_REPO: ${{ github.repository }}
          PR_NUMBER: ${{ github.event.pull_request.number }}
          SNIF_PR_NUMBER: ${{ github.event.pull_request.number }}

      - name: Upload SARIF
        if: always()
        uses: github/codeql-action/upload-sarif@v3
        with:
          sarif_file: findings.sarif
          category: snif
```

## Building from source

For the Snif repo itself or for forks, build from source instead of downloading
a binary.

```yaml
      - uses: dtolnay/rust-toolchain@stable
      - uses: Swatinem/rust-cache@v2
      - name: Build
        run: cargo build --release
      - name: Index
        run: ./target/release/snif index --path .
```

## Required secrets

`SNIF_API_KEY` — your LLM provider API key. Configure in the repository
settings under Settings > Secrets and variables > Actions.

`GITHUB_TOKEN` — provided automatically by GitHub Actions. No configuration
needed. Grants permission to post PR comments and fetch PR data.

## SARIF integration

When the review outputs SARIF (`--format sarif`), upload it with the
`github/codeql-action/upload-sarif` action. Findings appear in the
repository's Security tab under Code scanning alerts. This provides a
persistent record of all findings across PRs.


# GitLab CI

The GitLab adapter is planned but not yet implemented. In the meantime, use
the diff-file approach to run Snif in GitLab merge request pipelines.

```yaml
snif-review:
  stage: review
  image: ghcr.io/assahbismarkabah/snif:latest
  script:
    - git diff origin/$CI_MERGE_REQUEST_TARGET_BRANCH_NAME..HEAD > change.patch
    - snif index --path .
    - snif review --path . --diff-file change.patch --format json
  rules:
    - if: $CI_PIPELINE_SOURCE == "merge_request_event"
  variables:
    SNIF_API_KEY: $SNIF_API_KEY
```

Findings are printed to the pipeline log as JSON. When the GitLab adapter
ships, it will post findings as merge request discussion threads directly.


# Generic CI (Jenkins, CircleCI, etc.)

For any CI system that can run a binary and set environment variables, use the
diff-file approach.

1. Download the Snif binary for your platform from GitHub releases
2. Set `SNIF_API_KEY` as an environment variable
3. Run `snif index --path .` to build the repository index
4. Generate a diff: `git diff origin/main..HEAD > change.patch`
5. Run `snif review --path . --diff-file change.patch`
6. Parse the JSON output for your reporting system

The `--format sarif` flag produces SARIF 2.1.0 output that can be consumed
by any SARIF-compatible tool or dashboard.


# Docker

The Snif container image includes the binary and all dependencies. Use it in
any CI system that supports Docker.

```
docker run --rm \
  -v "$(pwd):/workspace" \
  -e SNIF_API_KEY \
  ghcr.io/assahbismarkabah/snif:latest \
  review --path /workspace --diff-file /workspace/change.patch
```

For indexing:

```
docker run --rm \
  -v "$(pwd):/workspace" \
  -e SNIF_API_KEY \
  ghcr.io/assahbismarkabah/snif:latest \
  index --path /workspace
```


# Configuration

Every repository using Snif needs a `.snif.json` at the root. This file is
committed to the repository and contains no secrets.

At minimum, configure the LLM provider:

```json
{
  "model": {
    "review_model": "gpt-4o",
    "summary_model": "gpt-4o-mini",
    "endpoint": "https://api.openai.com/v1"
  }
}
```

The endpoint can also be set via `SNIF_ENDPOINT` environment variable, which
overrides the config file value. This is useful when different CI environments
use different providers.

See [Configuration](./configuration.md) for all available fields.
