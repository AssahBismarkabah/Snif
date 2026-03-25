# Product and Delivery

This document defines the product intent, scope, quality bar, and delivery plan for Snif.

Technical architecture is documented separately in [05_dev_agent_architecture.adoc](./05_dev_agent_architecture.adoc).

## Executive Summary

Snif is a repository-aware engineering workflow agent. Its job is not to generate generic coding commentary. Its job is to execute concrete workflows against a specific repository with enough context to produce credible, relevant, and actionable output.

The product is intentionally narrow in its first phase. It will prove one workflow well before it broadens:

- review code changes

The project prioritizes precision, predictability, and operational control over feature breadth. In practice, that means deterministic context retrieval, strict filtering, disciplined annotation lifecycle handling, and benchmark-driven tuning.

Platform is not the product boundary. Snif should be able to attach to different code-hosting and workflow surfaces through adapters, even if Phase 1 starts with only one concrete integration.

## Problem Statement

Most AI workflow tools treat every codebase the same way. They read a narrow slice of context, apply a generic prompt, and return output that is often noisy, obvious, or weakly justified. Developers quickly learn to ignore that feedback. Once trust is lost, workflow automation becomes a tax rather than a multiplier.

The failure mode is not just model quality. It is usually a systems problem:

- the tool lacks repository-specific context
- the output policy is too permissive
- reruns cause comment churn
- there is no stable evaluation harness
- operational ownership is delegated to a black-box runtime

Snif is meant to solve that systems problem.

## Product Vision

Snif should feel less like a brainstorming assistant and more like a disciplined engineering agent:

- quiet when the change is sound
- specific when a real issue exists
- consistent across reruns
- configurable per repository
- measurable in quality, speed, and cost

Over time, the product may expand into adjacent engineering workflows. Phase 1 does not depend on that expansion.

## Product Goals

Phase 1 is successful if Snif can replace a generic AI change-review workflow for a narrow class of repositories without reducing developer trust.

The goals are:

- understand repository conventions and surrounding code context
- review entire changes rather than isolated lines in a vacuum
- produce findings with evidence, violated invariant, and likely impact
- avoid preference-based or speculative comments
- support idempotent reruns and stable finding identity
- provide a measurable path for prompt and retrieval iteration

## Non-Goals

The following are explicitly out of scope for Phase 1:

- vector databases and embeddings as required infrastructure
- multi-agent workflow orchestration
- generalized chat or ask-the-codebase workflows
- automated migration, refactor, or documentation-generation commands
- deep repository indexing services that require separate operational ownership

Deferring these items is a product decision, not a capability gap. The first job is to prove that the baseline reviewer is trustworthy.

## Product Principles

### We own the intelligence layer

Snif owns context loading, prompt construction, output parsing, filtering, annotation lifecycle, and evaluation. The model runtime is replaceable.

### Context quality matters more than prompt cleverness

A diff alone is not enough. The workflow engine needs change metadata, repository conventions, changed files, and structurally related code.

### Silence is a feature

A clean change should usually produce no comments. Review quality is judged by precision and actionability, not by output volume.

### Start narrow, then earn expansion

The product should prove a high-trust single-agent baseline before adding more workflows, retrieval layers, or orchestration complexity.

### Feedback is part of the product

Resolved, dismissed, and ignored findings are operational signals. They are not incidental byproducts.

## Primary User Workflow

The primary Phase 1 workflow is:

1. a developer opens or updates a code change on a supported platform
2. Snif is triggered by that platform, by CI, or by a local CLI run
3. Snif assembles deterministic review context
4. Snif runs a single structured review through OpenCode
5. Snif posts only findings that survive policy filtering
6. on subsequent runs, Snif updates comments without duplicating or churning discussions

The output should be useful inside a normal review loop without requiring a separate UI or human interpretation layer.

## Phase 1 Scope

### In Scope

- code change review as the first workflow
- one primary CLI command: `review`
- optional evaluation command for benchmark execution
- single-agent review flow
- deterministic, diff-first context assembly
- repository conventions loading
- structured findings and schema-based output
- confidence and evidence-based filtering
- platform adapter abstraction for annotations and lifecycle handling
- benchmark fixtures and quality gates

### Out of Scope

- semantic retrieval as the primary retrieval mechanism
- autonomous code modification
- agent-to-agent coordination
- organization-wide analytics platforms
- full platform parity in the initial release

## User Value

The first release should provide three immediate forms of value:

- fewer comments than generic AI reviewers, but higher average usefulness
- comments that are concrete enough to act on without a follow-up conversation
- predictable behavior across reruns and change updates

Snif should not attempt to be impressive. It should attempt to be dependable.

## Product Requirements

The Phase 1 reviewer must:

- understand the target change as a cross-file unit of work
- use repository-specific conventions when available
- prefer deterministic retrieval over opaque retrieval heuristics
- return structured findings rather than free-form review prose
- suppress findings that are stylistic, speculative, or weakly evidenced
- maintain stable identity for findings across reruns
- expose enough metrics to debug quality, latency, and cost
- keep workflow logic separate from platform-specific transport and annotation concerns

## Model Strategy

### Phase 1

Use a single model call per review through OpenCode. Candidate models should be selected based on measured precision and false-positive behavior against real review fixtures, not subjective prompt quality.

### Later Phases

If the product later introduces orchestration or specialization:

- use cheaper models for coordination where possible
- reserve stronger models for work that clearly benefits from them
- keep cost and latency visible as first-class metrics

## Success Metrics

| Metric | Target | Why it matters |
|---|---|---|
| Precision | >= 80% | Most comments must be correct |
| Recall | >= 60% | The reviewer must still catch meaningful issues |
| Noise rate | <= 10% | Clean changes should usually stay quiet |
| False positive rate in production | < 10% | Trust erodes quickly if comments are often wrong |
| Actionable rate in production | > 60% | Findings must translate into useful developer action |
| Review time | < 120s | The tool must fit naturally into CI and local workflows |
| Token cost per review | Track continuously | Cost regressions must be observable |

## Quality Gates

Prompt, model, or retrieval changes must not ship if:

- precision drops below 70%
- noise rate exceeds 20%

These are minimum operating thresholds, not aspirational targets. The benchmark harness is the enforcement mechanism.

## Evaluation Strategy

Snif should be tuned against a fixed benchmark set composed of real workflow scenarios.

Recommended initial benchmark composition:

- 10 changes with known real bugs
- 10 clean changes with no meaningful issues
- 5 changes containing style or preference noise that should not be flagged

Each fixture should contain:

- change metadata
- unified diff
- changed file contents
- repository conventions
- expected findings or expected silence

The benchmark should be run for:

- prompt changes
- model changes
- retrieval changes
- filtering policy changes

## Rollout Plan

1. Build the core review pipeline.
2. Build the evaluation harness.
3. Curate the benchmark set from real repository history.
4. Tune retrieval and filtering until quality gates pass.
5. Run Snif in parallel with the current review process.
6. Replace the generic AI reviewer only after measured validation.

This rollout is deliberately conservative. Developer trust is harder to regain than it is to delay.

## Delivery Strategy

### Phase 1: Deterministic Change Review

Deliver a reliable change-review workflow with:

- deterministic context assembly
- strict filtering
- at least one platform adapter
- stable annotation lifecycle handling
- benchmark-backed quality gates

Success condition:

- the review workflow is good enough to replace the current generic AI review path for the target repositories

### Phase 1.5: Structural Retrieval Upgrade

If the baseline reviewer needs more depth, add a lightweight structural index to improve related-file retrieval while keeping the system local, explainable, and operationally simple.

### Phase 2: Specialized Review Dimensions

Only after the baseline is proven should the product consider specialized review dimensions such as:

- security
- logic
- conventions

This phase should be justified by measured gaps in depth or latency, not by architectural preference.

### Phase 3: Product Expansion

Once the initial review workflow is stable and trusted, the product may expand to additional commands such as:

- `suggest`
- `migrate`
- `document`

Semantic retrieval may also be introduced at this stage as a secondary capability.

## Implementation Priorities

1. Scaffold the Rust CLI project.
2. Add configuration loading and validation.
3. Build the platform adapter abstraction and first concrete adapter.
4. Build deterministic related-file retrieval.
5. Build the context package assembler.
6. Integrate OpenCode execution.
7. Implement prompt templates and output schema.
8. Implement response parsing and filter policies.
9. Implement finding fingerprinting and lifecycle logic.
10. Build the evaluation harness.
11. Build the benchmark fixtures.
12. Tune prompts and retrieval until the quality gates pass.
13. Integrate the workflow into CI and the first supported platform trigger.

## Risks and Mitigations

| Risk | Product Impact | Response |
|---|---|---|
| Too many false positives | Developers ignore the reviewer | Tight filtering, evidence requirements, and benchmark gates |
| Weak benchmark set | The team overestimates product quality | Curate fixtures from real history and maintain them carefully |
| Premature infrastructure complexity | Delivery slows and operations become expensive | Keep Phase 1 deterministic and simple |
| Annotation churn across pushes | Developers lose confidence in reruns | Stable fingerprints and stale-thread resolution |
| Model drift | Review quality regresses silently | Re-run evaluation on every relevant change |

## Current Recommendation

Proceed with Snif as a controlled, deterministic review product first.

The product path should remain:

1. prove the single-agent baseline
2. improve retrieval structurally before semantically
3. expand only after measured success
