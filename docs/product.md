# Product

## Problem

AI code review tools fail for predictable reasons:

- They lack repository-specific context, so findings are generic.
- They have no output discipline, so developers get noise.
- They have no rerun stability, so every push creates comment churn.
- They have no evaluation harness, so quality regressions ship silently.

The result is the same every time: developers disable the tool within weeks.

## Solution

Snif owns the full review pipeline end-to-end. The model is a replaceable execution layer. Snif controls what context goes in, what findings come out, how they're filtered, and how they're published.

This means Snif owns:

- Context assembly (what the model sees)
- Prompt construction (what the model is asked)
- Output parsing and filtering (what survives)
- Annotation lifecycle (what gets posted, updated, or resolved)
- Evaluation (whether quality is improving or regressing)

## Users

Snif targets engineering teams running CI-driven code review. The primary integration surface is a CI pipeline step that triggers on pull request or merge request events.

Developers see inline review comments on their PRs. Tech leads configure review behavior per repository via `.snif.json`. Platform admins add Snif to CI pipelines and manage credentials.

## Deployment Model

Snif ships as a single Rust binary. It runs anywhere a CI job runs — GitHub Actions, GitLab CI, Jenkins, or any system that can execute a binary and pass environment variables.

The primary trigger is a PR/MR event in CI. Snif can also be invoked by platform webhooks (GitHub App, GitLab webhook) or manually from a developer's terminal with `snif review`.

Configuration lives in the repository (`.snif.json`). Credentials come from environment variables. No external database or service is required.

## Phase 1 Scope

Phase 1 delivers one workflow: **deterministic change review**.

**In scope:**

- `snif review` — review a code change
- `snif eval` — run the benchmark evaluation harness
- Deterministic, diff-first context assembly
- Structured findings with confidence, evidence, and impact
- Platform adapters for annotation publishing and lifecycle
- Benchmark fixtures and quality gates

**Out of scope:**

- Semantic search or vector databases
- Multi-agent orchestration
- Chat or ask-the-codebase workflows
- Automated code modification

## Success Metrics

Snif is measured on trust, not volume. Precision must be at least 80% — most comments must be correct. Recall must be at least 60% — the reviewer must still catch meaningful issues. Noise rate must stay under 10% — clean changes stay quiet. The false positive rate in production must remain below 10%, and over 60% of findings must be directly actionable by the developer.

Review time must stay under 120 seconds to fit CI time budgets. Token cost is tracked per review so cost regressions are visible immediately.

Quality gates block shipping if precision drops below 70% or noise rate exceeds 20%.

## Evaluation Strategy

Changes to prompts, models, or retrieval are validated against a fixed benchmark set before shipping.

Benchmark composition:

- 10 changes with known real bugs
- 10 clean changes with no issues
- 5 changes with style noise that must not be flagged

Each fixture includes: change metadata, unified diff, file contents, conventions, and expected findings or expected silence.

## Delivery Plan

**Phase 1** delivers the single-agent deterministic reviewer with strict filtering, platform adapters, and the evaluation harness. This is the foundation.

**Phase 1.5** adds a structural retrieval upgrade — a local import graph and symbol map — but only if baseline retrieval proves insufficient against the benchmark.

**Phase 2** introduces specialized review dimensions (security, logic, conventions) as separate review passes. This only happens if measured gaps in depth or latency justify the added complexity.

**Phase 3** expands the command surface (`suggest`, `migrate`, `document`) and adds semantic retrieval as a secondary path alongside the structural baseline.

## Implementation Order

1. Scaffold Rust CLI
2. Configuration loading and validation
3. Platform adapter abstraction + first adapter
4. Deterministic related-file retrieval
5. Context package assembler
6. OpenCode execution integration
7. Prompt templates and output schema
8. Response parsing and filter policies
9. Finding fingerprinting and lifecycle logic
10. Evaluation harness and benchmark fixtures
11. Tune until quality gates pass
12. CI integration and first production deployment
