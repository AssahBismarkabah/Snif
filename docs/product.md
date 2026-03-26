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

| User | Interaction |
|---|---|
| Developer | Sees inline review comments on their PR/MR |
| Tech lead | Configures review behavior per repository via `.dev-agent.json` |
| Platform admin | Adds Snif to CI pipelines and manages credentials |

## Deployment Model

Snif ships as a single Rust binary. It runs anywhere a CI job runs.

| Environment | Trigger | Example |
|---|---|---|
| CI pipeline | PR/MR event | GitHub Actions job, GitLab CI stage |
| Platform webhook | Repository event | GitHub App, GitLab webhook |
| Local | Manual | `snif review` from terminal |

Configuration lives in the repository (`.dev-agent.json`). Credentials come from environment variables. No external database or service is required.

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

| Metric | Target | Rationale |
|---|---|---|
| Precision | >= 80% | Most comments must be correct |
| Recall | >= 60% | Must still catch meaningful issues |
| Noise rate | <= 10% | Clean changes stay quiet |
| False positive rate | < 10% | Trust erodes fast on wrong comments |
| Actionable rate | > 60% | Findings must lead to developer action |
| Review time | < 120s | Must fit CI time budgets |
| Token cost | Tracked per review | Cost regressions must be visible |

Quality gates block shipping if precision drops below 70% or noise rate exceeds 20%.

## Evaluation Strategy

Changes to prompts, models, or retrieval are validated against a fixed benchmark set before shipping.

Benchmark composition:

- 10 changes with known real bugs
- 10 clean changes with no issues
- 5 changes with style noise that must not be flagged

Each fixture includes: change metadata, unified diff, file contents, conventions, and expected findings or expected silence.

## Delivery Plan

| Phase | Scope |
|---|---|
| **1** | Single-agent deterministic review, strict filtering, platform adapters, evaluation harness |
| **1.5** | Structural retrieval upgrade (local import graph, symbol map) if baseline retrieval proves insufficient |
| **2** | Specialized review dimensions (security, logic, conventions) — only if measured gaps justify it |
| **3** | Additional commands (`suggest`, `migrate`, `document`), semantic retrieval as secondary path |

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
