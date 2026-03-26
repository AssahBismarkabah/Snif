# Architecture

## Overview

Snif is a layered modular monolith with ports and adapters at the system boundary. The application core owns workflow orchestration, context assembly, prompt construction, output handling, and evaluation. External systems are accessed through thin integration layers.

This is not a microservice architecture, event-driven pipeline, or multi-agent orchestration system. It is a single binary with explicit module boundaries.

## System Context

| External System | Role |
|---|---|
| Repository platform (GitLab, GitHub) | Source of change metadata, diffs, and annotation APIs |
| Repository filesystem | Source of conventions, file contents, and related code |
| OpenCode | LLM execution runtime |
| CI / webhook trigger | Invokes `snif review` or `snif eval` |

## Modules

### Core Pipeline

| Module | Responsibility |
|---|---|
| `config` | Parse `.dev-agent.json`, merge env vars, validate settings |
| `commands` | CLI entry points and argument handling |
| `review` | End-to-end review pipeline orchestration |

### Context Assembly

| Module | Responsibility |
|---|---|
| `context::builder` | Assemble the final context package from all sources |
| `context::diff_parser` | Parse unified diffs into changed files and hunks |
| `context::related_files` | Resolve imports, tests, and shared types for changed files |
| `context::conventions` | Load repository review conventions and policies |
| `context::budget` | Enforce token and file-count limits; record omissions |

### Prompt and Execution

| Module | Responsibility |
|---|---|
| `prompts` | Render system prompt, user prompt, and output schema |
| `execution::opencode` | Generate runtime config and invoke OpenCode |

### Output Processing

| Module | Responsibility |
|---|---|
| `output::parser` | Translate model response into structured findings |
| `output::filter` | Apply confidence, evidence, impact, and suppression rules |
| `output::fingerprint` | Generate stable finding identity across reruns |
| `output::publish` | Translate findings into platform-neutral publication actions |

### Platform Integration

| Module | Responsibility |
|---|---|
| `platform` | Provider-neutral interfaces for triggers, metadata, diffs, annotations |
| `platform::gitlab` | GitLab merge request and discussion adapter |
| `platform::github` | GitHub pull request and review comment adapter |
| `platform::annotations` | Post comments, map prior output, resolve stale discussions |

### Evaluation

| Module | Responsibility |
|---|---|
| `eval` | Run benchmark fixtures, compute metrics, enforce quality gates |

## Data Model

| Model | Purpose |
|---|---|
| Review request | Input for a run: platform identity, change identity, repo location |
| Context package | Assembled payload: metadata, diff, files, related files, truncation info |
| Finding | Normalized issue with confidence, evidence, impact, and location |
| Fingerprint | Stable identity key for cross-rerun comparison |
| Run metadata | Timing, token usage, omitted context, outcome |
| Evaluation result | Per-fixture and aggregate benchmark outcomes |

## Runtime Flow: `snif review`

```
CLI start
  -> config: load .dev-agent.json + env vars
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
2. Repository conventions (`.dev-agent.json` review hints)
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

| Source | Content |
|---|---|
| `.dev-agent.json` | Platform adapter, model settings, context budget, filter thresholds, conventions paths, eval fixture paths |
| Environment variables | Credentials, provider endpoints |

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
