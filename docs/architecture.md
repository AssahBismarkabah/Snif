# Architecture

# Overview

Snif is a layered modular monolith with ports and adapters at the system
boundary. The application core owns workflow orchestration, context assembly,
prompt construction, output handling, and evaluation. External systems are
accessed through thin integration layers.

This is not a microservice architecture, event-driven pipeline, or multi-agent
orchestration system. It is a single binary with explicit module boundaries.


# System Context

Snif sits between four external systems. The repository platform (GitLab,
GitHub) provides change metadata, diffs, and annotation APIs. The repository
filesystem provides conventions, file contents, and related code. OpenCode
handles LLM execution as a thin runtime layer. CI pipelines or platform webhooks
invoke `snif review` or `snif eval`.


# Modules

## Core Pipeline

`config` loads `.snif.json`, merges environment variables, and validates
settings. `commands` handles CLI entry points and argument parsing. `review`
orchestrates the end-to-end review pipeline and collects run metadata.

## Context Assembly

`context::builder` assembles the final context package from all sources.
`context::diff_parser` parses unified diffs into changed files and hunks.
`context::related_files` resolves imports, tests, and shared types for changed
files. `context::conventions` loads repository review conventions and policies
from the working tree. `context::budget` enforces token and file-count limits
and records what was omitted.

## Prompt and Execution

`prompts` renders the system prompt, user prompt, and output schema.
`execution::opencode` generates runtime configuration and invokes OpenCode. The
execution layer contains no product policy — its job is to call the model and
return the response.

## Output Processing

`output::parser` translates the model response into structured findings.
`output::filter` applies confidence, evidence, impact, and suppression rules.
`output::fingerprint` generates stable finding identity across reruns.
`output::publish` translates filtered findings into platform-neutral publication
actions.

## Platform Integration

`platform` defines provider-neutral interfaces for triggers, metadata, diffs,
and annotations. `platform::gitlab` and `platform::github` are concrete adapters
for their respective platforms. `platform::annotations` handles posting inline
comments, mapping prior bot output, and resolving stale discussions.

## Evaluation

`eval` runs benchmark fixtures through the same pipeline used by `snif review`,
computes precision, recall, and noise rate, and fails the run if quality gates
are not met.


# Data Model

The system revolves around six internal models. A review request is the
canonical input for a workflow run — it carries platform identity, change
identity, and repository location. The context package is the assembled payload
sent to the prompt layer, containing metadata, diff, file contents, related
files, and truncation information. A finding is the normalized representation of
a model-reported issue, carrying confidence, evidence, impact, and location. A
fingerprint is the stable identity key used to compare findings across reruns.
Run metadata captures timing, token usage, omitted context, and outcome for
observability. An evaluation result records per-fixture and aggregate benchmark
outcomes used for quality gating.


# Runtime Flow

## `snif review`

The CLI loads `.snif.json` and environment variables, then selects the active
platform adapter and fetches change metadata and the diff. The conventions loader
reads repository review guidance from the working tree. The context builder loads
changed file contents, the related-files resolver expands context through
imports, tests, and shared types, and the budget module trims the result to fit
within deterministic limits while recording omissions.

The prompt layer renders the system prompt, user prompt, and output schema. The
execution adapter invokes OpenCode and returns the raw model response. The parser
translates this into structured findings. The filter rejects anything
speculative, style-only, weakly evidenced, or missing user-relevant impact. The
fingerprint module computes stable identities for surviving findings. The publish
module generates platform-neutral publication actions. The platform annotation
adapter posts new findings as inline comments and resolves stale ones from prior
runs. Finally, run metrics are recorded.

## `snif eval`

The evaluation harness loads the benchmark fixture set and runs each fixture
through the same pipeline used by `snif review`. Actual findings are compared
with expected findings or expected silence. Precision, recall, and noise rate are
computed. The command fails if configured quality gates are not met.


# Context Retrieval

Context assembly is deterministic and follows a fixed priority order: change
metadata first, then repository conventions, the unified diff, full changed
files, direct imports of changed files, corresponding test files, and finally
shared types and interfaces.

The budget policy prioritizes changed files over related files, preserves the
diff even when file bodies must be trimmed, and records every omission. Budgeting
decisions are product behavior — they must be observable and testable.

Phase 1 does not use vector databases, embeddings, or semantic search. If
retrieval proves insufficient, the first upgrade is a lightweight local
structural index covering the import graph, symbol map, and test-to-source
mapping, stored as generated JSON or SQLite.


# Output Filtering

Findings are rejected if they are speculative, style-only unless configured as
enforced policy, unsupported by evidence from the provided context, missing
user-relevant impact, or duplicates of stronger findings. The system biases
toward false negatives over false positives until precision is stable.


# Annotation Lifecycle

Reruns against the same change version are idempotent. Finding fingerprints are
stable across pushes, so the system can match current findings with prior bot
output. When the underlying issue disappears, stale bot findings are resolved
automatically. Ongoing human discussions are preserved — the reviewer behaves as
a disciplined participant in the code review process, not a stream of
disconnected comments.


# Configuration

Repository-scoped configuration lives in `.snif.json`, committed to the
repository. It covers the platform adapter, model settings, context budget,
filter thresholds, conventions paths, and evaluation fixture paths. Credentials
and provider endpoints come from environment variables. Validation fails fast on
missing or inconsistent settings. No external database is required.


# Security

Repository contents are treated as sensitive. Credentials come from environment
variables only. Logs avoid leaking source content. Only the minimum required
context is transmitted to the model runtime.


# Observability

Every review run captures duration, token usage, context size, truncation
decisions, finding counts before and after filtering, comment posting and
resolution actions, and evaluation metrics by fixture and aggregate. This makes
it possible to answer two questions: why did Snif say this, and why did Snif
stay quiet.
