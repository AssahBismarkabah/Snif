# Spike Results

This document records the findings from the technical validation spikes run
before Snif's main implementation. Each spike tested a core technical bet that
the architecture depends on.


# Spike 1: SQLite + sqlite-vec

## What Was Tested

Whether rusqlite + sqlite-vec can serve as the single local store for both the
structural relationship graph (relational tables) and vector embeddings (vec0
virtual tables), with acceptable KNN query performance and hybrid query support.

## Setup

sqlite-vec v0.1.7 registered via `sqlite3_auto_extension` with rusqlite 0.32.
Schema includes relational tables for files, symbols, imports, references, and
co-change data alongside vec0 virtual tables for embeddings. Synthetic data
generated at three scales: 500 files / 5k embeddings (small), 2,500 files / 25k
embeddings (medium), and 5,000 files / 50k embeddings (large). All vectors are
384-dimensional normalized float32. Benchmarks run in debug mode (unoptimized).

## Results

Bootstrap passed. Vectors round-trip correctly through sqlite-vec.

At the large scale (50k embeddings, 384-dim), KNN queries average 103ms with a
p95 of 132ms in debug mode. Release mode is expected to be 5-10x faster based
on typical Rust debug-to-release ratios, putting real-world KNN latency in the
10-25ms range. This is negligible inside the 120-second review time budget.

The hybrid query approach works via two steps in application code: a structural
query to collect candidate file IDs, then a KNN query with app-side filtering.
This averaged 112ms at the large scale in debug mode — acceptable overhead over
a standalone KNN query.

Insert throughput is roughly 4,000 vectors per second in debug mode across all
scales. For incremental indexing where only changed files are re-embedded, this
is more than sufficient.

Database file sizes scale linearly and are small: 9MB for 5k embeddings, 44MB
for 25k, 87MB for 50k, all at 384 dimensions. At 768 dimensions the DB doubles
to 82MB for 25k embeddings, and at 1536 dimensions it reaches 157MB. KNN latency
scales roughly linearly with dimension: 51ms avg at 384-dim, 100ms at 768-dim,
201ms at 1536-dim (debug mode, 25k vectors).

## Decisions

384 dimensions is the target for Phase 1. It offers the best trade-off between
query latency, storage size, and embedding quality for the AllMiniLML6V2 model
that produces 384-dim vectors. Higher dimensions can be revisited if embedding
quality evaluation in Spike 3 shows a need.

The two-step hybrid query approach (structural query in SQL, then KNN with
app-side filtering) is the recommended pattern. sqlite-vec's vec0 virtual table
does not support arbitrary WHERE clauses alongside MATCH, so filtering must
happen in application code after the KNN results are returned.

## Verdict

Go. sqlite-vec meets all performance requirements for the repository index and
feedback store. No external vector database is needed.


# Spike 2: Tree-sitter Extraction

## What Was Tested

Whether tree-sitter can extract imports, symbol definitions (functions, structs,
classes, interfaces, enums, traits, type aliases), and call references from real
source files in Rust, TypeScript, and Python, and how much per-language adapter
code is needed.

## Setup

A generic `LanguageAdapter` trait defines the interface. Each language provides
tree-sitter S-expression queries for imports, symbol definitions, and
references. A shared parser orchestrator runs the queries and collects results.
Testing was done against hand-written source code snippets covering the common
patterns in each language.

## Results

All three adapters extract accurately.

The Rust adapter correctly parses `use` declarations including nested paths
(`use std::collections::{HashMap, HashSet}`), extracts function, struct, enum,
trait, type alias, module, and constant definitions, and captures call-site
references including scoped and field-expression calls.

The TypeScript adapter correctly parses ES module imports from string literals,
extracts function declarations, class declarations, interface declarations, type
alias declarations, enum declarations, and arrow functions assigned to
variables. Both `.ts` and `.tsx` grammars are supported.

The Python adapter correctly parses `import` and `from ... import` statements,
extracts function and class definitions including nested methods within classes,
and captures function call references including attribute-style calls.

Each adapter is approximately 100-150 lines of code, mostly consisting of the
tree-sitter query S-expressions and match-to-struct mapping logic. Adding a new
language requires writing a new adapter implementing the trait — no changes to
the orchestrator or shared types.

## What Was Not Tested

The adapters were tested against clean, representative code snippets. They were
not tested against large real-world repositories with macros, code generation,
unusual syntax patterns, or files with syntax errors. The error recovery path
(tree-sitter producing partial results for broken files) was implemented but not
exercised in this spike. These should be validated when Spike 3 runs the
extraction pipeline against a real repository.

## Decisions

The `LanguageAdapter` trait pattern is the right abstraction. It keeps
language-specific knowledge isolated in small adapter modules while sharing all
orchestration, error handling, and output formatting.

Tree-sitter query S-expressions are the right level for extraction. They are
declarative, readable, and composable. Raw AST walking would be more powerful
but significantly more verbose and harder to maintain.

## Verdict

Go. Tree-sitter extraction works for the three target languages. Adapter
complexity is low and the pattern scales to additional languages.


# Spike 3: LLM Summarization + Embedding

## What Was Tested

Whether LLM-generated summaries of code units produce vector embeddings that
are useful for finding semantically related code. The full pipeline was tested:
extract code units from a real repository, generate natural language summaries
via OpenCode (Claude Haiku through a configured provider), embed the summaries using fastembed-rs
(AllMiniLML6V2, 384-dim), store in sqlite-vec, and query for similar code units.

## Setup

Test repository: axum (Tokio web framework, 291 Rust files). 20 code units
extracted and summarized using Claude Haiku via OpenCode subprocess invocation.
Summaries embedded locally using fastembed AllMiniLML6V2 (384 dimensions, ONNX
runtime). Embeddings stored in sqlite-vec and queried with KNN.

## Results

All 20 summaries generated successfully. The summaries are accurate and describe
purpose rather than implementation. For example, `into_response` in
`append_headers.rs` was summarized as: "converts an AppendHeaders instance into
an HTTP response by delegating to the IntoResponseParts trait implementation,
enabling AppendHeaders to be used as a responder in Axum route handlers." This
is the kind of grounded, purpose-oriented description that embeds well.

Embedding quality is positive. Similarity queries return semantically related
code units. `into_response` from `append_headers.rs` correctly finds
`into_response_parts`, `headers`, and `extensions` from the same module as its
nearest neighbors. Related functions cluster together with distances in the
0.3-0.4 range, indicating meaningful semantic similarity.

The embedding step itself is fast: 20 summaries embedded in 1.5 seconds locally.
The fastembed model (22MB ONNX file) loaded in ~52 seconds on first use and
would be cached for subsequent runs.

## Cost

At Claude Haiku pricing (the provider used in this spike was AWS Bedrock, but
any OpenAI-compatible provider works), the 20 summaries cost approximately
$0.13. Extrapolated:

- 1,000 code units: ~$6.57
- 5,000 code units: ~$33
- 10,000 code units: ~$66

For per-commit incremental indexing where only changed code units are
re-summarized, these costs are acceptable.

## Performance

The main bottleneck is summarization time. Each OpenCode subprocess call takes
~16 seconds on average, dominated by process startup overhead. At this rate,
1,000 code units would take roughly 4.5 hours serially.

This is not acceptable for production use. The OpenCode subprocess approach
adds significant per-call overhead that does not scale for batch indexing. The
recommended approach for the main implementation is direct API calls to the
configured provider from Rust using an HTTP client, which eliminates process
startup overhead and enables concurrent requests.

The embedding step is not a bottleneck. Local embedding of 20 summaries
completed in 1.5 seconds. Even at 10,000 summaries, this would take under two
minutes.

## Issues Found

The OpenCode JSON output format requires more robust parsing. The current
implementation leaks raw JSON event envelopes into stored summaries. This is a
parsing issue, not a fundamental problem — the text content is present in the
output and just needs cleaner extraction. For the main implementation, direct
API calls would return cleaner response structures.

## Decisions

Use fastembed AllMiniLML6V2 (384-dim, local ONNX) for embedding. It produces
meaningful similarity results, runs locally with no API dependency, and the
384-dim output aligns with the sqlite-vec performance sweet spot validated in
Spike 1.

Do not use OpenCode subprocess for batch summarization in the main
implementation. Use direct API calls to the configured provider via an HTTP
client from Rust. This eliminates process startup overhead and enables
concurrent summarization of multiple code units. The provider is configurable
in `.snif.json` — AWS Bedrock, OpenAI, Azure, or any OpenAI-compatible endpoint.

A smaller, cheaper model (Haiku-class) is sufficient for summary generation.
The summaries are accurate and descriptive. Larger models can be reserved for
the review step where reasoning quality matters more.

## Verdict

Go with caveats. The semantic pipeline produces useful summaries and meaningful
embeddings. Similarity search finds related code. The OpenCode subprocess
approach must be replaced with direct API calls for production indexing
performance.


# Overall Assessment

All three spikes validate the core architecture.

SQLite with sqlite-vec handles both relational graph data and vector embeddings
in a single local database with acceptable performance. 384-dim embeddings at
50k scale query in ~100ms debug mode with 87MB database size.

Tree-sitter extracts imports, symbols, and references accurately across Rust,
TypeScript, and Python with ~100-150 lines of adapter code per language.

LLM-generated summaries produce meaningful vector embeddings that find
semantically related code. The cost is manageable for incremental indexing. The
main implementation should use direct API calls rather than OpenCode subprocess
for batch summarization performance.

The architecture proceeds as designed. The key implementation change from the
spike findings is replacing OpenCode subprocess invocation with direct provider
API calls for the summarization step in `snif index`. The provider is not
hard-coded — Snif supports any OpenAI-compatible endpoint configured in
`.snif.json`.
