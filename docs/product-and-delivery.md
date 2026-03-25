# Product and Delivery

This document defines the product intent, scope, success criteria, and delivery plan for Snif.

Technical architecture is documented separately in [05_dev_agent_architecture.adoc](./05_dev_agent_architecture.adoc).

## Problem

Generic AI review tools treat every codebase the same. They run one-size-fits-all prompts against a diff and produce noisy output. Developers learn to ignore the feedback because most of it is irrelevant, obvious, or wrong. That is worse than no review at all.

## Product Goal

Snif is a repository-aware merge request review agent that understands project conventions, architecture, and surrounding code context.

The product goal is to deliver a reviewer that:

- stays quiet when there is nothing meaningful to say
- speaks clearly when there is a real issue
- is configurable per project
- is measurable in quality, speed, and cost
- can expand later into adjacent engineering workflows

Phase 1 focuses on one capability only:

- merge request review

## Product Principles

### We control the intelligence layer

We own the context loading, prompt construction, output parsing, filtering, lifecycle handling, and evaluation. The model runtime is replaceable.

### Context quality matters more than prompt cleverness

A diff alone is not enough. The reviewer needs project conventions, changed files, and structurally related code.

### Silence is a feature

A clean merge request should produce zero comments. Review quality is judged by precision, not by output volume.

### Start narrow

Phase 1 does one job well before the product expands to more commands or more complex review modes.

### Feedback closes the loop

Comment outcomes are product signals. Resolved, dismissed, and ignored findings are part of the tuning process.

## Scope

### In Scope for Phase 1

- GitLab merge request review
- single-agent review flow
- deterministic context building
- structured findings
- confidence and evidence-based filtering
- inline GitLab comments
- comment lifecycle handling
- evaluation harness and quality gates

### Out of Scope for Phase 1

- vector database
- embeddings pipeline
- persistent knowledge graph service
- multi-agent review
- migration/refactor/document commands
- chat interface

## User Value

The first product value is simple:

- fewer noisy comments than generic AI reviewers
- comments that are specific enough to act on
- predictable behavior across reruns and MR updates

Snif should feel closer to a disciplined reviewer than a brainstorming assistant.

## Delivery Strategy

### Phase 1: Single-Agent MR Review

Deliver a reliable merge request reviewer with deterministic context assembly, strict filtering, GitLab integration, and evaluation gates.

Success condition:

- the product is good enough to replace the current generic AI review workflow

### Phase 1.5: Structural Retrieval Upgrade

If Phase 1 needs more depth, add a lightweight structural repository index to improve related-file retrieval while keeping the system deterministic and operationally simple.

### Phase 2: Smarter Review

Only if the single-agent baseline is proven and measured should the product consider specialized parallel review dimensions such as:

- security
- logic
- conventions

This phase is justified only by measured bottlenecks in wall-clock time or review depth.

### Phase 3: Product Expansion

Once the core review product is stable, expand to additional commands such as:

- `suggest`
- `migrate`
- `document`

Semantic retrieval may also be introduced at this stage as a secondary retrieval capability.

## Model Strategy

### Phase 1

Use a single model call per review through OpenCode. Test candidate models against real MR fixtures and choose based on measured false positive performance rather than intuition.

### Later Phases

If multi-agent orchestration is added later:

- use cheaper models for orchestration
- use stronger models for specialized review work

Cost should remain observable throughout.

## Success Metrics

| Metric | Target | Why it matters |
|---|---|---|
| Precision | >= 80% | Most comments must be correct |
| Recall | >= 60% | The reviewer must still catch meaningful issues |
| Noise rate | <= 10% | Clean MRs should usually stay clean |
| False positive rate in production | < 10% | Trust must hold after rollout |
| Actionable rate in production | > 60% | Comments should lead to useful changes |
| Review time | < 120s | The reviewer must fit naturally into CI |
| Token cost per review | Track continuously | Cost regressions must be visible |

## Quality Gates

Prompt, model, or context changes do not ship if:

- precision drops below 70%
- noise rate exceeds 20%

The evaluation harness is the enforcement mechanism, not manual judgment.

## Evaluation Plan

The product relies on a fixed benchmark set.

Benchmark composition:

- 10 MRs with known real bugs
- 10 clean MRs with no issues
- 5 MRs with intentional style/preference issues that should not be flagged

Each fixture contains:

- diff
- changed files
- conventions
- expected findings or expected silence

This benchmark is used on:

- prompt changes
- model changes
- retrieval changes

## Rollout Plan

1. Build the Phase 1 review pipeline
2. Build the evaluation harness
3. Create the benchmark set from real history
4. Tune until quality gates pass
5. Run against real merge requests in parallel with the current process
6. Replace the generic AI reviewer only after measured validation

## Implementation Order

1. Scaffold the Rust CLI project
2. Add configuration loading and validation
3. Build the GitLab client
4. Build deterministic related-file retrieval
5. Build the context package assembler
6. Integrate OpenCode execution
7. Implement prompt templates and output schema
8. Implement response parsing and filter policies
9. Implement comment fingerprinting and lifecycle logic
10. Build the evaluation harness
11. Build the benchmark fixtures
12. Tune prompts and retrieval until the gates pass
13. Integrate the reviewer into CI

## Product Risks

| Risk | Product Impact | Response |
|---|---|---|
| Too many false positives | Developers ignore the reviewer | Tight filtering and evaluation gates |
| Weak benchmark set | False confidence in product quality | Curate fixtures from real history and maintain them carefully |
| Premature infrastructure complexity | Slower delivery and harder operations | Keep Phase 1 deterministic and simple |
| Comment churn across pushes | Developer frustration and distrust | Stable fingerprints and discussion lifecycle handling |
| Model drift | Review quality regresses silently | Re-run evaluation on every relevant change |

## Current Recommendation

Proceed with Snif as a controlled, deterministic review product first.

The professional path is:

1. prove the single-agent baseline
2. improve retrieval structurally before semantically
3. expand only after measured success
