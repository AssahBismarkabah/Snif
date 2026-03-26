# Spikes

This directory contains technical validation spikes run before Snif's main
implementation. Each spike tests a core technical bet that the architecture
depends on. The goal is to produce concrete numbers — performance, cost,
quality — and make a go/no-go decision for each component before committing
to the full build.

Results are documented in [RESULTS.md](./RESULTS.md).


# Structure

The spikes are organized as a Cargo workspace with a shared types crate.

`common` defines the shared data types used across spikes: `FileExtraction`,
`Import`, `Symbol`, `Reference`, and `ParseError`. These types represent the
output of tree-sitter extraction and are consumed by both the extraction and
embedding spikes.

`spike-sqlite-vec` validates SQLite with the sqlite-vec extension as a unified
local store for structural graph data and vector embeddings. It benchmarks KNN
query latency, insert throughput, hybrid queries, and dimension trade-offs at
multiple scales.

`spike-treesitter` validates tree-sitter for extracting imports, symbol
definitions, and references from source code. It implements language adapters
for Rust, TypeScript, and Python and tests them against both built-in snippets
and real repositories.

`spike-llm-embed` validates the end-to-end semantic pipeline: extracting code
units from a real repository, generating natural language summaries via an LLM
provider, embedding the summaries locally with fastembed, storing them in
sqlite-vec, and evaluating whether vector similarity finds related code.


# Running the Spikes

All commands are run from the `spikes/` directory.

## Spike 1: SQLite + sqlite-vec

```
cargo run -p spike-sqlite-vec
```

Runs the full benchmark suite: bootstrap verification, structural data
population, KNN query benchmarks at three scales (5k, 25k, 50k embeddings),
hybrid query benchmarks, insert throughput, and dimension comparison (384, 768,
1536). No external dependencies required.

## Spike 2: Tree-sitter Extraction

```
cargo run -p spike-treesitter
```

Runs built-in extraction tests against Rust, TypeScript, and Python code
snippets. Prints imports, symbols, and references found in each.

To parse a real repository:

```
cargo run -p spike-treesitter -- --repo /path/to/repo
```

Add `--json` to write the full extraction output to
`/tmp/snif_spike_treesitter_output.json`.

## Spike 3: LLM Summarization + Embedding

```
cargo run -p spike-llm-embed -- --repo /path/to/rust-repo --limit 20
```

Requires a configured LLM provider accessible through OpenCode. The default
model is Claude Haiku via Bedrock. Override with `--model provider/model-id`.

The spike clones axum as a default test repository if no `--repo` is specified.
To set up the default test repo:

```
git clone --depth 1 https://github.com/tokio-rs/axum.git /tmp/snif-test-repo-axum
cargo run -p spike-llm-embed
```

This will summarize code units, embed them locally, store them in sqlite-vec,
print a cost report, and evaluate embedding quality by querying for similar
code units.


# Dependencies

The spikes use the following key crates:

- `rusqlite` 0.32 with `sqlite-vec` 0.1.6 for vector-capable local storage
- `tree-sitter` 0.24 with language grammars for Rust, TypeScript, and Python
- `fastembed` 4 for local ONNX-based text embedding (AllMiniLML6V2, 384-dim)
- `zerocopy` 0.7 for efficient vector byte serialization to sqlite-vec

The fastembed model (~22MB) is downloaded automatically on first run and cached
in `.fastembed_cache/`.
