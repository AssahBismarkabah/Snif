# CI Integration

This document covers how to configure Snif in CI pipelines across different
platforms.


# GitHub Actions

GitHub Actions is the primary supported platform. Snif posts findings as
inline PR review comments.

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

jobs:
  review:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4
        with:
          fetch-depth: 0

      - uses: sigstore/cosign-installer@v3

      - name: Install Snif
        env:
          SNIF_VERSION: "3.1.4"
        run: |
          curl -sSLf "https://github.com/AssahBismarkabah/Snif/releases/download/v${SNIF_VERSION}/snif-x86_64-unknown-linux-gnu.tar.xz" -O
          curl -sSLf "https://github.com/AssahBismarkabah/Snif/releases/download/v${SNIF_VERSION}/snif-x86_64-unknown-linux-gnu.tar.xz.sha256" -O
          curl -sSLf "https://github.com/AssahBismarkabah/Snif/releases/download/v${SNIF_VERSION}/snif-x86_64-unknown-linux-gnu.tar.xz.sha256.sig" -O
          curl -sSLf "https://github.com/AssahBismarkabah/Snif/releases/download/v${SNIF_VERSION}/snif-x86_64-unknown-linux-gnu.tar.xz.sha256.pem" -O
          cosign verify-blob snif-x86_64-unknown-linux-gnu.tar.xz.sha256 \
            --signature snif-x86_64-unknown-linux-gnu.tar.xz.sha256.sig \
            --certificate snif-x86_64-unknown-linux-gnu.tar.xz.sha256.pem \
            --certificate-identity "https://github.com/AssahBismarkabah/Snif/.github/workflows/sign-release.yml@refs/heads/main" \
            --certificate-oidc-issuer "https://token.actions.githubusercontent.com"
          sha256sum -c snif-x86_64-unknown-linux-gnu.tar.xz.sha256
          tar xJf snif-x86_64-unknown-linux-gnu.tar.xz
          mv snif-x86_64-unknown-linux-gnu/snif /usr/local/bin/snif

      - name: Index repository
        run: snif index --path .
        env:
          SNIF_API_KEY: ${{ secrets.SNIF_API_KEY }}

      - name: Review pull request
        run: |
          snif review \
            --path . \
            --repo "$GITHUB_REPO" \
            --pr "$PR_NUMBER"
        env:
          GITHUB_TOKEN: ${{ secrets.GITHUB_TOKEN }}
          SNIF_API_KEY: ${{ secrets.SNIF_API_KEY }}
          GITHUB_REPO: ${{ github.repository }}
          PR_NUMBER: ${{ github.event.pull_request.number }}
          SNIF_PR_NUMBER: ${{ github.event.pull_request.number }}
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

# GitLab CI

Snif supports GitLab natively. It posts findings as inline merge request
discussions and summary comments. Works with gitlab.com, self-hosted GitLab,
and enterprise instances with LDAP/SSO authentication.

The pipeline must run as a merge request pipeline. Snif reads
`CI_PROJECT_PATH` and `CI_MERGE_REQUEST_IID` from the environment, which
GitLab only provides in merge request pipelines. Without the merge request
rule, these variables are not set and `snif review` will fail.

```yaml
snif-review:
  stage: review
  image: debian:bookworm-slim
  variables:
    SNIF_VERSION: "3.1.4"
    SNIF_API_KEY: $SNIF_API_KEY
    GITLAB_TOKEN: $GITLAB_TOKEN
  before_script:
    - apt-get update && apt-get install -y curl git xz-utils ca-certificates
    - curl -sSLf "https://github.com/sigstore/cosign/releases/latest/download/cosign-linux-amd64" -o /usr/local/bin/cosign && chmod +x /usr/local/bin/cosign
    - curl -sSLf "https://github.com/AssahBismarkabah/Snif/releases/download/v${SNIF_VERSION}/snif-x86_64-unknown-linux-gnu.tar.xz" -O
    - curl -sSLf "https://github.com/AssahBismarkabah/Snif/releases/download/v${SNIF_VERSION}/snif-x86_64-unknown-linux-gnu.tar.xz.sha256" -O
    - curl -sSLf "https://github.com/AssahBismarkabah/Snif/releases/download/v${SNIF_VERSION}/snif-x86_64-unknown-linux-gnu.tar.xz.sha256.sig" -O
    - curl -sSLf "https://github.com/AssahBismarkabah/Snif/releases/download/v${SNIF_VERSION}/snif-x86_64-unknown-linux-gnu.tar.xz.sha256.pem" -O
    - cosign verify-blob snif-x86_64-unknown-linux-gnu.tar.xz.sha256
        --signature snif-x86_64-unknown-linux-gnu.tar.xz.sha256.sig
        --certificate snif-x86_64-unknown-linux-gnu.tar.xz.sha256.pem
        --certificate-identity "https://github.com/AssahBismarkabah/Snif/.github/workflows/sign-release.yml@refs/heads/main"
        --certificate-oidc-issuer "https://token.actions.githubusercontent.com"
    - sha256sum -c snif-x86_64-unknown-linux-gnu.tar.xz.sha256
    - tar xJf snif-x86_64-unknown-linux-gnu.tar.xz
    - mv snif-x86_64-unknown-linux-gnu/snif /usr/local/bin/snif
  script:
    - snif index --path .
    - snif review --path .
  rules:
    - if: $CI_PIPELINE_SOURCE == "merge_request_event"  # Required
```

Snif auto-detects GitLab from the `CI_PROJECT_PATH` and
`CI_MERGE_REQUEST_IID` environment variables provided by GitLab CI. No
flags needed. If auto-detection does not work in your environment, pass the
values explicitly:

```yaml
  script:
    - snif index --path .
    - snif review --path . --repo "$CI_PROJECT_PATH" --pr "$CI_MERGE_REQUEST_IID"
```

## Required variables

Set these as CI/CD variables in the project settings (Settings > CI/CD >
Variables):

`SNIF_API_KEY` is the LLM provider API key.

`GITLAB_TOKEN` is a project or personal access token with `api` scope. This
is needed to post merge request comments. Alternatively, `CI_JOB_TOKEN` works
if the project allows it, but it has limited permissions on some GitLab
configurations.

## Self-hosted GitLab

For self-hosted instances (e.g., `https://git.example.com`), Snif reads the
`CI_API_V4_URL` variable that GitLab CI provides automatically. No additional
configuration is needed — the pipeline already knows the instance URL.

For projects that want to be explicit, add the instance URL to `.snif.json`:

```json
{
  "platform": {
    "provider": "gitlab",
    "api_base": "https://git.example.com/api/v4"
  }
}
```


# Generic CI (Jenkins, CircleCI, etc.)

For CI systems without a native Snif adapter, generate a diff from git and
pass it directly. Snif runs the full review pipeline and outputs findings as
JSON to stdout.

1. Download the Snif binary for your platform from GitHub releases
2. Set `SNIF_API_KEY` as an environment variable
3. Run `snif index --path .` to build the repository index
4. Generate a diff: `git diff origin/main..HEAD > change.patch`
5. Run `snif review --path . --diff-file change.patch`
6. Findings are printed to stdout as JSON




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
