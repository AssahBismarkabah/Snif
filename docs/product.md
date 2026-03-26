# Product

# Problem

AI code review tools fail for predictable reasons. They lack repository-specific
context, so their findings are generic. They have no output discipline, so
developers get noise. They have no rerun stability, so every push creates comment
churn. They have no evaluation harness, so quality regressions ship silently.

The result is the same every time: developers disable the tool within weeks.
Once trust is lost, the tool becomes a tax rather than a multiplier.

This is not primarily a model quality problem. It is a systems problem — weak
context, permissive output, no lifecycle management, and no measurement.


# Solution

Snif owns the full review pipeline end-to-end. The model is a replaceable
execution layer. Snif controls what context goes in, what findings come out, how
they are filtered, and how they are published.

Concretely, Snif owns context assembly (what the model sees), prompt
construction (what the model is asked), output parsing and filtering (what
survives), annotation lifecycle (what gets posted, updated, or resolved), and
evaluation (whether quality is improving or regressing).


# Users

Snif targets engineering teams running CI-driven code review. Developers see
inline review comments on their pull requests. Tech leads configure review
behavior per repository through `.snif.json`. Platform admins add Snif to CI
pipelines and manage credentials.


# Deployment

Snif ships as a single Rust binary. It runs anywhere a CI job runs — GitHub
Actions, GitLab CI, Jenkins, or any system that can execute a binary and pass
environment variables.

The primary trigger is a pull request or merge request event in CI. Snif can
also be invoked by platform webhooks or manually from a developer's terminal
with `snif review`. Configuration lives in the repository as `.snif.json`.
Credentials come from environment variables. No external database or service is
required.


# Phase 1 Scope

Phase 1 delivers one workflow: deterministic change review.

`snif review` reviews a code change. `snif eval` runs the benchmark evaluation
harness. The system uses deterministic, diff-first context assembly, produces
structured findings with confidence, evidence, and impact, publishes annotations
through platform adapters with full lifecycle management, and enforces quality
gates through benchmark fixtures.

Semantic search, vector databases, multi-agent orchestration, chat workflows,
and automated code modification are all out of scope for Phase 1. Deferring
these is a product decision, not a capability gap. The first job is to prove the
baseline reviewer is trustworthy.


# Success Metrics

Snif is measured on trust, not volume. Precision must be at least 80% — most
comments must be correct. Recall must be at least 60% — the reviewer must still
catch meaningful issues. The noise rate must stay under 10% — clean changes
should produce no output. The false positive rate in production must remain below
10%, and over 60% of findings must be directly actionable.

Review time must stay under 120 seconds to fit CI time budgets. Token cost is
tracked per review so cost regressions are visible immediately.

Quality gates block shipping if precision drops below 70% or noise rate exceeds
20%. These are minimum operating thresholds, not aspirational targets.


# Evaluation

Changes to prompts, models, or retrieval are validated against a fixed benchmark
set before shipping. The benchmark is composed of 10 changes with known real
bugs, 10 clean changes with no issues, and 5 changes with style noise that must
not be flagged. Each fixture includes change metadata, a unified diff, file
contents, conventions, and expected findings or expected silence.


# Delivery Plan

Phase 1 delivers the single-agent deterministic reviewer with strict filtering,
platform adapters, and the evaluation harness. This is the foundation everything
else builds on.

Phase 1.5 adds a structural retrieval upgrade — a local import graph and symbol
map — but only if baseline retrieval proves insufficient against the benchmark.

Phase 2 introduces specialized review dimensions such as security, logic, and
conventions as separate review passes. This only happens if measured gaps in
depth or latency justify the added complexity.

Phase 3 expands the command surface with commands like `suggest`, `migrate`, and
`document`, and introduces semantic retrieval as a secondary path alongside the
structural baseline.


# Implementation Order

1. Scaffold the Rust CLI project
2. Configuration loading and validation
3. Platform adapter abstraction and first concrete adapter
4. Deterministic related-file retrieval
5. Context package assembler
6. OpenCode execution integration
7. Prompt templates and output schema
8. Response parsing and filter policies
9. Finding fingerprinting and lifecycle logic
10. Evaluation harness and benchmark fixtures
11. Tune until quality gates pass
12. CI integration and first production deployment
