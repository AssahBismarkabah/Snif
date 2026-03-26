# Architecture

## Overview

Snif is a layered modular monolith with ports and adapters at the system boundary. The application core owns workflow orchestration, context assembly, prompt construction, output handling, and evaluation. External systems are accessed through thin integration layers.

This is not a microservice architecture, event-driven pipeline, or multi-agent orchestration system. It is a single binary with explicit module boundaries.

## System Context

Snif sits between four external systems. The **repository platform** (GitLab, GitHub) provides change metadata, diffs, and annotation APIs. The **repository filesystem** provides conventions, file contents, and related code. **OpenCode** handles LLM execution. The **CI or webhook trigger** invokes `snif review` or `snif eval`.

## Modules

### Core Pipeline

`config` loads `.snif.json`, merges environment variables, and validates settings. `commands` handles CLI entry points and argument parsing. `review` orchestrates the end-to-end review pipeline.

### Context Assembly

`context::builder` assembles the final context package from all sources. `context::diff_parser` parses unified diffs into changed files and hunks. `context::related_files` resolves imports, tests, and shared types for changed files. `context::conventions` loads repository review conventions and policies. `context::budget` enforces token and file-count limits and records what was omitted.

### Prompt and Execution

`prompts` renders the system prompt, user prompt, and output schema. `execution::opencode` generates runtime config and invokes OpenCode.

### Output Processing

`output::parser` translates the model response into structured findings. `output::filter` applies confidence, evidence, impact, and suppression rules. `output::fingerprint` generates stable finding identity across reruns. `output::publish` translates findings into platform-neutral publication actions.

### Platform Integration

`platform` defines provider-neutral interfaces for triggers, metadata, diffs, and annotations. `platform::gitlab` and `platform::github` are concrete adapters for their respective platforms. `platform::annotations` handles posting comments, mapping prior bot output, and resolving stale discussions.

### Evaluation

`eval` runs benchmark fixtures, computes quality metrics, and enforces quality gates.

## Data Model

The system revolves around six internal models. A **review request** is the canonical input — platform identity, change identity, and repo location. The **context package** is the assembled payload sent to the prompt layer, containing metadata, diff, file contents, related files, and truncation info. A **finding** is the normalized representation of a model-reported issue with confidence, evidence, impact, and location. A **fingerprint** is the stable identity key used to compare findings across reruns. **Run metadata** captures timing, token usage, omitted context, and outcome. An **evaluation result** records per-fixture and aggregate benchmark outcomes.

## Runtime Flow: `snif review`

```
CLI start
  -> config: load .snif.json + env vars
  -> platform: select adapter, fetch change metadata + diff
  -> context::conventions: load repo review guidance
  -> context::builder: load changed files from working tree
  -> context::related_files: expand via imports, tests, shared types
  -> context::budget: trim to budget, record omissions
  -> prompts: render system prompt + user prompt + output schema
  -> execution::opencode: execute review
  -> output::parser: parse structured findings
  -> output::filter: reject weak findings
  -> output::fingerprint: compute stable identities
  -> output::publish: generate publication actions
  -> platform::annotations: post new findings, resolve stale ones
  -> record run metrics
```

## Runtime Flow: `snif eval`

```
CLI start
  -> eval: load benchmark fixtures
  -> for each fixture:
       run through the same pipeline as `snif review`
       compare actual findings with expected findings
  -> compute precision, recall, noise rate
  -> fail if quality gates are not met
```

## Context Retrieval

Context assembly is deterministic and follows a fixed priority:

1. Change metadata (author, branch, labels)
2. Repository conventions (`.snif.json` review hints)
3. Unified diff
4. Full changed files
5. Direct imports of changed files
6. Corresponding test files
7. Shared types and interfaces

The budget policy prioritizes changed files over related files, preserves the diff even when file bodies are trimmed, and records every omission.

Phase 1 does not use vector databases, embeddings, or semantic search. If retrieval proves insufficient, the first upgrade is a lightweight local structural index (import graph, symbol map, test-to-source mapping) stored as generated JSON or SQLite.

## Output Filtering

Findings are rejected if they are:

- Speculative or hypothetical
- Style-only (unless configured as enforced policy)
- Missing evidence from the provided context
- Missing user-relevant impact
- Duplicates of stronger findings

The system biases toward false negatives over false positives until precision is stable.

## Annotation Lifecycle

- **Idempotent reruns:** same change version produces the same comments
- **Stable fingerprints:** findings match across pushes
- **Stale resolution:** bot findings are resolved when the underlying issue disappears
- **Human preservation:** ongoing developer discussions are not disrupted

## Configuration

Repository-scoped configuration lives in `.snif.json`, committed to the repo. It covers the platform adapter, model settings, context budget, filter thresholds, conventions paths, and evaluation fixture paths. Credentials and provider endpoints come from environment variables.

Validation fails fast on missing or inconsistent settings. No external database required.

## Security

- Repository contents are treated as sensitive
- Credentials come from environment variables only
- Logs do not leak source content
- Only minimum required context is sent to the model runtime

## Observability

Every review run captures:

- Duration and token usage
- Context size and truncation decisions
- Finding counts before and after filtering
- Comment posting and resolution actions
- Evaluation metrics by fixture and aggregate

This makes it possible to answer: *Why did Snif say this?* and *Why did Snif stay quiet?*
