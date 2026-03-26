# Snif

Snif is a code review agent that runs inside your CI pipeline. It reads a pull request, assembles repository context, runs a single structured LLM review, and posts only findings that are specific, evidenced, and actionable.

Most AI reviewers treat every codebase the same. They read a narrow slice of context, apply a generic prompt, and produce noisy output that developers learn to ignore. Snif solves this by treating context assembly, output filtering, and annotation lifecycle as engineering problems — not prompt problems.

## How It Works

Snif ships as a single Rust binary. It runs in three environments:

- **CI pipelines** — triggered by GitLab CI, GitHub Actions, or any CI system on PR/MR events
- **Platform webhooks** — triggered directly by GitHub or GitLab on change events
- **Local CLI** — `snif review` from a developer's terminal

A review run follows a fixed pipeline:

1. Load repo config and credentials from environment
2. Fetch change metadata and diff from the platform adapter
3. Assemble deterministic context: conventions, changed files, imports, tests, shared types
4. Execute a single structured review through OpenCode
5. Filter findings aggressively — reject anything speculative, style-only, or weakly evidenced
6. Post surviving findings as inline comments; resolve stale ones from prior runs

## Key Properties

- **Quiet on clean changes.** No output is a success, not a miss.
- **Idempotent reruns.** Stable finding fingerprints prevent comment churn across pushes.
- **Platform-agnostic core.** GitLab, GitHub, and future platforms are integration adapters, not product boundaries.
- **Benchmark-gated.** Prompt, model, and retrieval changes must pass a fixed evaluation harness before shipping.

## Quality Targets

| Metric | Target |
|---|---|
| Precision | >= 80% |
| Recall | >= 60% |
| Noise rate | <= 10% |
| Review time | < 120s |

## Status

Documentation phase. Implementation has not started. See [Product](./docs/product.md) and [Architecture](./docs/architecture.md) for the full design.
