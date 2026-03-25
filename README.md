# Snif

Snif is a repository-aware engineering workflow agent. It is designed to execute repository-specific workflows with strong contextual understanding, high-signal output, and strict operational control.

The core thesis behind the project is simple: review quality is determined more by context quality, filtering discipline, and operational control than by prompt cleverness alone. Snif therefore treats context assembly, output policy, and evaluation as first-class engineering concerns.

## Status

This repository is currently documentation-first. The product direction, delivery scope, and target architecture are defined here; implementation has not started yet.

Phase 1 is intended to ship as a Rust CLI with one primary workflow:

- `snif review`

The initial release will focus on deterministic change review. The review workflow should be platform-agnostic at the core and attach to concrete surfaces such as GitLab, GitHub, local CLI, or CI through adapters. Platform is an integration detail, not the product boundary.

## Product Goals

Snif is being built to:

- understand repository conventions and surrounding code context
- stay quiet on clean changes
- emit findings that are specific, evidenced, and actionable
- support repeatable reruns without comment churn
- measure quality with a fixed evaluation harness

## Phase 1 Scope

In scope:

- change review as the first workflow
- single-agent review execution
- deterministic, diff-first context retrieval
- structured findings with confidence and evidence requirements
- platform-specific comment or annotation lifecycle management
- benchmark-driven evaluation and quality gates

Out of scope:

- vector databases and embeddings as core infrastructure
- multi-agent workflow orchestration
- generalized chat workflows
- broader code transformation commands such as `migrate` or `document`

## Repository Guide

- [Product and Delivery](./docs/product-and-delivery.md): product intent, scope, success criteria, rollout plan, and implementation priorities
- [Architecture Overview](./docs/architecture.md): documentation entry point for the technical design
- [Detailed Architecture](./docs/05_dev_agent_architecture.adoc): system architecture, module boundaries, runtime flow, and operational model

## Planned System Shape

The target system is a layered modular monolith with ports and adapters at the edges. Snif will own workflow orchestration, context assembly, prompt construction, output parsing, filtering, lifecycle handling, and evaluation. Model execution will be delegated to OpenCode through a thin adapter layer.

At a high level, a review run will:

1. load configuration and credentials
2. fetch change metadata and diff data from the active platform adapter
3. assemble deterministic context from the repository and related files
4. execute a single structured model review
5. parse and aggressively filter findings
6. publish new findings through the active platform adapter and resolve stale ones
7. record metrics for tuning and evaluation

## Quality Bar

Snif is only useful if developers trust it. The project is therefore biased toward precision over output volume. A workflow run that emits nothing on a clean change is a success, not a miss.

The target quality bar for Phase 1 is:

- precision >= 80%
- recall >= 60%
- noise rate <= 10%

Any change to prompts, models, or retrieval behavior should be validated against the benchmark harness before it is shipped.
