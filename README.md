# Snif

Snif is a repository-aware code review agent. It reads a code change, assembles
context from the repository, runs a single structured review through an LLM, and
posts only findings that are specific, evidenced, and actionable.

Most AI review tools treat every codebase the same way. They read a narrow slice
of context, apply a generic prompt, and return output that is noisy, obvious, or
weakly justified. Developers learn to ignore that output quickly. Snif treats
this as a systems problem rather than a prompt problem, and solves it by owning
the full pipeline: context assembly, output filtering, annotation lifecycle, and
evaluation.

Snif ships as a single Rust binary. It is designed to run inside CI pipelines
triggered by pull request or merge request events, but can also be invoked
locally from the terminal with `snif review`. The core is platform-agnostic.
GitLab, GitHub, and future platforms are integration adapters, not product
boundaries.

A review run loads repository configuration from `.snif.json` and credentials
from environment variables, fetches the change metadata and diff from the
platform adapter, assembles deterministic context from conventions, changed
files, imports, tests, and shared types, executes a single structured review
through OpenCode, filters findings aggressively, and posts surviving findings as
inline comments while resolving stale ones from prior runs.

Snif is quiet on clean changes. No output is a success, not a miss. Reruns are
idempotent — stable finding fingerprints prevent comment churn across pushes.
Prompt, model, and retrieval changes must pass a fixed evaluation harness before
shipping.

The quality targets for Phase 1 are at least 80% precision, at least 60% recall,
a noise rate under 10%, and review completion within 120 seconds.


# Status

This repository is in the documentation phase. Implementation has not started.
See the [Product](./docs/product.md) document for scope, delivery plan, and
success criteria. See the [Architecture](./docs/architecture.md) document for
the technical design.
