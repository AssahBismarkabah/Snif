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
invoke `snif review`, `snif index`, or `snif eval`.


# Modules

## Core Pipeline

`config` loads `.snif.json`, merges environment variables, and validates
settings. `commands` handles CLI entry points and argument parsing. `review`
orchestrates the end-to-end review pipeline and collects run metadata.

## Repository Index

`index::parser` uses tree-sitter grammars to parse source files into ASTs and
extract structural information — imports, symbol definitions, symbol references,
and exports. `index::graph` builds and maintains the relationship graph from
parsed data: import edges, call sites, type references, and test-to-source
links. `index::cochange` analyzes git history to compute co-change correlations
between files. `index::summarizer` generates natural language summaries of code
units — functions, classes, files, and directories — through the LLM execution
layer. `index::embeddings` computes vector embeddings of those summaries and
stores them alongside the structural data. `index::store` manages the local
SQLite database with vector extensions, handles incremental updates, and exposes
query interfaces for the retrieval layer.

## Context Assembly

`context::builder` assembles the final context package from all sources.
`context::diff_parser` parses unified diffs into changed files and hunks.
`context::retrieval` queries the repository index using all three retrieval
methods — structural graph traversal, semantic vector search, and keyword
matching — then merges and ranks the results. `context::conventions` loads
repository review conventions and policies from the working tree.
`context::budget` enforces token and file-count limits, selects from ranked
candidates, and records what was omitted.

## Prompt and Execution

`prompts` renders the system prompt, user prompt, and output schema.
`execution::opencode` generates runtime configuration and invokes OpenCode. The
execution layer contains no product policy — its job is to call the model and
return the response.

## Output Processing

`output::parser` translates the model response into structured findings.
`output::filter` applies confidence, evidence, impact, and suppression rules,
including feedback-learned suppressions from the team signal store.
`output::fingerprint` generates stable finding identity across reruns.
`output::publish` translates filtered findings into platform-neutral publication
actions.

## Feedback Learning

`feedback::collector` captures signals from the platform — developer reactions,
comment resolutions, ignored findings, and human reviewer comments on the same
PR. `feedback::store` persists these signals per team with vector embeddings of
the associated findings. `feedback::filter` uses similarity matching against
the signal store to suppress findings that resemble past-rejected output and
boost findings that resemble past-accepted output.

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

The system revolves around a small set of internal models. A review request is
the canonical input for a workflow run — it carries platform identity, change
identity, and repository location. The context package is the assembled payload
sent to the prompt layer, containing metadata, diff, file contents, related
files, and truncation information. A finding is the normalized representation of
a model-reported issue, carrying confidence, evidence, impact, and location. A
fingerprint is the stable identity key used to compare findings across reruns.
Run metadata captures timing, token usage, omitted context, and outcome for
observability. An evaluation result records per-fixture and aggregate benchmark
outcomes used for quality gating. A feedback signal records a developer action
on a finding — accepted, dismissed, or ignored — along with the embedded
representation of that finding for similarity matching.


# Runtime Flow

## `snif index`

The indexer analyzes the repository in two passes. The first pass is structural.
Tree-sitter parses every source file into an AST. The parser extracts imports,
symbol definitions, symbol references, and exports. The graph builder assembles
these into a relationship graph — import edges, call sites, type references,
and test-to-source links. The co-change analyzer reads git history and computes
file-pair correlation scores.

The second pass is semantic. The summarizer walks the graph bottom-up and
generates natural language summaries for each code unit — functions first, then
classes, then files, then directories. Each summary is generated by the LLM
through the same execution layer used by `snif review`. The embedding module
computes vector embeddings of these summaries and writes them to the SQLite
vector store.

The index is incremental. On subsequent runs, it reads the git diff since the
last indexed commit and re-processes only the affected files. Summaries and
embeddings are regenerated only for code units that changed or whose
dependencies changed. A full rebuild is available but should rarely be necessary.

The index runs as a separate command — `snif index` — and can be triggered
manually, by a git hook, or as a CI setup step before `snif review`.

## `snif review`

The CLI loads `.snif.json` and environment variables, then selects the active
platform adapter and fetches change metadata and the diff. The conventions loader
reads repository review guidance from the working tree. The context builder loads
changed file contents, then the retrieval module expands context using three
methods in parallel.

Structural retrieval queries the relationship graph for direct imports,
consumers, test files, co-change correlates, and symbol references of the
changed code. Semantic retrieval takes the changed code and its summaries,
searches the vector store for code units with similar summaries elsewhere in the
repository, and returns matches that are not already covered by the structural
results. Keyword retrieval searches for exact identifiers, type names, and error
strings referenced in the diff.

The results from all three methods are merged and deduplicated. Each candidate
file is scored by a weighted combination of relationship type, retrieval method,
and match strength. The budget module selects from the ranked candidates until
the token limit is reached and records omissions.

The prompt layer renders the system prompt, user prompt, and output schema. The
execution adapter invokes OpenCode and returns the raw model response. The parser
translates this into structured findings. The filter rejects anything
speculative, style-only, weakly evidenced, or missing user-relevant impact. The
feedback filter then checks surviving findings against the team's signal store —
findings that closely resemble past-rejected output are suppressed, and findings
that resemble past-accepted output are boosted in confidence. The fingerprint
module computes stable identities for the final set. The publish module generates
platform-neutral publication actions. The platform annotation adapter posts new
findings as inline comments and resolves stale ones from prior runs. Finally, run
metrics are recorded.

## `snif eval`

The evaluation harness loads the benchmark fixture set and runs each fixture
through the same pipeline used by `snif review`. Actual findings are compared
with expected findings or expected silence. Precision, recall, and noise rate are
computed. The command fails if configured quality gates are not met.


# Repository Index

Context quality determines review quality. A model reviewing a diff in isolation
will produce shallow, generic findings. A model reviewing a diff alongside the
full changed files, the code those files depend on, the tests that cover them,
and the conventions the team has agreed on will produce findings that are
specific and grounded. Snif's repository index exists to close that gap.

## What the Index Contains

The index has two layers: structural and semantic.

The structural layer is built by static analysis. The import graph maps every
file to the files it imports and the files that import it. The symbol index
records where types, functions, traits, and interfaces are defined and where
they are used across the repository. The test map links source files to their
corresponding test files by tracking which test files import which source files,
not just by naming convention. The co-change graph is derived from git history —
files that frequently change together across commits have a statistical
relationship that often reflects a real dependency the import graph misses.

The semantic layer is built by the LLM. The summarizer generates a natural
language description of every function, class, file, and directory in the
repository. These summaries describe what the code does, not how it's written.
The embedding module converts these summaries into vector embeddings stored in
the same SQLite database using vector extensions.

The structural layer answers "what is directly connected to this code." The
semantic layer answers "what else in this repository does something similar or
related to this code." Both are needed. Structural retrieval catches the blast
radius — the code that will break if this change is wrong. Semantic retrieval
catches the pattern radius — code that follows the same conventions, handles
the same concerns, or implements the same contract elsewhere in the repository.

## Building the Index

The index is built by `snif index`. Tree-sitter grammars handle the structural
parsing — they extract imports, symbol definitions, and symbol references from
source files without executing the code. Adding support for a new language means
adding a tree-sitter grammar and a small extraction adapter.

The semantic pass walks the structural graph bottom-up. Functions are summarized
first, then classes that contain them, then the files, then directories. Each
summary is generated by the LLM through OpenCode. This is the expensive part of
indexing, but it runs per-commit rather than per-review, and the incremental
update logic ensures that only changed code units and their dependents are
re-summarized.

Everything is stored in a single local SQLite database with vector extensions
for embedding storage and similarity search. No external vector database, no
hosted service, no infrastructure beyond the binary and the database file.

## Querying the Index

When the context builder needs to expand beyond the changed files, it runs three
retrieval methods in parallel.

Structural retrieval queries the graph for direct imports, direct consumers, test
files, co-change correlates, and symbol references. This is deterministic — the
same change against the same graph always returns the same results.

Semantic retrieval takes the summaries of the changed code units, searches the
vector store for similar summaries elsewhere in the repository, and returns
matches not already covered by structural results. This finds code related by
purpose rather than by import path — other implementations of the same
interface, other handlers following the same pattern, other modules touching the
same domain.

Keyword retrieval searches for exact identifiers, type names, and string
literals referenced in the diff. This catches references that the structural
parser missed and provides a fast fallback for languages without full tree-sitter
support.

Results from all three methods are merged, deduplicated, and scored. Structural
matches rank highest because they represent verified dependencies. Semantic
matches rank next because they represent likely relevance. Keyword matches rank
last as supplementary context. The budget module selects from this ranked list
until the token limit is reached.

## Budgeting

Context must fit within a deterministic token budget. The diff is always included
in full. Changed files are included next. Related files from the retrieval layer
fill the remaining budget in ranked order. When the budget is exhausted,
remaining candidates are recorded as omissions in the run metadata so the
decision is observable.

The budget is configured in `.snif.json` and can be tuned per repository. A
small, focused repository might allow a generous budget. A large monorepo might
need a tighter limit to keep review times under 120 seconds.


# Feedback Learning

Filtering based on static rules — confidence thresholds, evidence requirements,
impact assessment — gets Snif to a usable baseline. But every engineering team
has its own standards, conventions, and tolerance for certain kinds of feedback.
Static rules cannot learn what a specific team cares about. The feedback learning
system closes that gap.

## How It Works

When Snif posts a finding, the platform adapter tracks what happens to it. A
developer might react with a thumbs-up, resolve the comment after fixing the
issue, leave it unresolved, or reply with disagreement. These signals are
collected by the feedback module and stored per team.

Each signal is paired with a vector embedding of the finding that produced it.
Over time, this builds a semantic map of what the team considers valuable versus
noisy. When Snif generates a new finding, it computes the embedding and checks
it against the team's signal store. If the new finding has high similarity to
multiple past-rejected findings, it is suppressed. If it has high similarity to
multiple past-accepted findings, its confidence is boosted.

This is not prompt tuning or fine-tuning. The model itself does not change. The
filter learns what to let through and what to hold back based on observed team
behavior.

## What It Stores

The feedback store is a local SQLite database, separate from the repository
index. It contains the finding text, the embedding vector, the team identifier,
the signal type (accepted, dismissed, ignored), and a timestamp. The store is
scoped per team so that different teams working on the same repository can have
different learned preferences.

## Cold Start

A new team starts with no feedback data. The static filter rules carry the full
filtering load until enough signals accumulate to be meaningful. The system
requires a configurable minimum number of signals before the feedback filter
activates — this prevents early noise from poisoning the learned preferences.


# Output Filtering

Output filtering has two stages. The static filter rejects findings that are
speculative, style-only unless configured as enforced policy, unsupported by
evidence from the provided context, missing user-relevant impact, or duplicates
of stronger findings. The system biases toward false negatives over false
positives.

The feedback filter runs second. It checks surviving findings against the team's
signal store for similarity to past-rejected and past-accepted findings and
adjusts confidence scores accordingly. Findings that drop below the confidence
threshold after feedback adjustment are suppressed.

Together, the static filter enforces a universal quality floor and the feedback
filter tunes the output to what the specific team actually values.


# Annotation Lifecycle

Reruns against the same change version are idempotent. Finding fingerprints are
stable across pushes, so the system can match current findings with prior bot
output. When the underlying issue disappears, stale bot findings are resolved
automatically. Ongoing human discussions are preserved — the reviewer behaves as
a disciplined participant in the code review process, not a stream of
disconnected comments.

Developer reactions and comment resolutions on Snif's findings are captured as
feedback signals and fed into the learning system.


# Configuration

Repository-scoped configuration lives in `.snif.json`, committed to the
repository. It covers the platform adapter, model settings, context budget,
retrieval weights, filter thresholds, conventions paths, and evaluation fixture
paths. Credentials and provider endpoints come from environment variables.
Validation fails fast on missing or inconsistent settings.

The feedback store and repository index are local SQLite databases stored in the
repository's `.snif/` directory or a configurable location. They do not require
external services.


# Security

Repository contents are treated as sensitive. Credentials come from environment
variables only. Logs avoid leaking source content. Only the minimum required
context is transmitted to the model runtime. The LLM-generated summaries stored
in the index describe code behavior in natural language — they do not contain
raw source code, but they do reflect the repository's logic and should be treated
with the same access controls as the source itself.


# Observability

Every review run captures duration, token usage, context size, truncation
decisions, retrieval method breakdown (how many files came from structural,
semantic, and keyword retrieval), finding counts before and after each filtering
stage, feedback filter actions, comment posting and resolution actions, and
evaluation metrics by fixture and aggregate. This makes it possible to answer
three questions: why did Snif say this, why did Snif stay quiet, and what did
Snif learn from this team's feedback.
