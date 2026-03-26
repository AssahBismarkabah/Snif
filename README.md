# Snif

Snif is a repository-aware code review agent. It builds a deep understanding of
your codebase — structural graph, semantic summaries, and vector embeddings —
and uses that understanding to review code changes with the full context needed
to produce specific, evidenced, and actionable findings.

Most AI reviewers treat every codebase the same way. They read a narrow slice of
context, apply a generic prompt, and return output that is noisy, obvious, or
weakly justified. Developers learn to ignore that output quickly. Snif solves
this by owning the full pipeline: codebase indexing, multi-method context
retrieval, output filtering, feedback learning, and annotation lifecycle.

Snif ships as a single Rust binary designed to run inside CI pipelines. It
indexes the repository once per commit — parsing the AST, building a
relationship graph, generating LLM summaries of every code unit, and embedding
those summaries for semantic search. When a pull request arrives, Snif retrieves
context using three methods in parallel: structural graph traversal for blast
radius, semantic similarity for pattern matching, and keyword search for exact
references. Findings are filtered aggressively by static rules and by learned
team preferences, then posted as inline comments with stale findings resolved
automatically.


# Getting Started

## Prerequisites

Rust toolchain (1.70+). An OpenAI-compatible LLM provider endpoint and API key.

## Build

```
cargo build --release
```

The binary is at `target/release/snif`.

## Configure

Copy the example configuration to your repository root:

```
cp .snif.json.example .snif.json
```

Edit `.snif.json` to set your LLM provider. At minimum you need the endpoint
and model names:

```json
{
  "model": {
    "review_model": "gpt-4o",
    "summary_model": "gpt-4o-mini",
    "endpoint": "https://api.openai.com/v1"
  }
}
```

Set the API key as an environment variable:

```
export SNIF_API_KEY=your-api-key
```

See [Configuration](./docs/configuration.md) for all options, provider
examples, and environment variables.

## Index

Build the repository index. This parses source files, builds the structural
graph, generates LLM summaries, and computes vector embeddings.

```
snif index --path /path/to/repo
```

Use `--full` for a clean rebuild. Without it, indexing is incremental.

## Review

Review a code change using a local diff file:

```
git diff main > /tmp/change.diff
snif review --path /path/to/repo --diff-file /tmp/change.diff
```

Review a GitHub pull request directly:

```
GITHUB_TOKEN=your-token snif review --repo owner/repo --pr 123
```

Findings are printed to stdout as JSON. When using `--repo` and `--pr`,
findings are also posted as inline review comments on the pull request.

## Evaluate

Run the evaluation harness against benchmark fixtures:

```
snif eval --fixtures ./fixtures/
```

The command exits with code 0 if quality gates pass and code 1 if they fail.

## Clean

Remove all local runtime data (index database, embedding cache, feedback store):

```
snif clean
```

This does not touch your source code or configuration — only Snif's generated
data.


# Documentation

- [Configuration](./docs/configuration.md) — setup, config format, CLI reference
- [Testing](./docs/testing.md) — local testing and evaluation
- [Product](./docs/product.md) — scope, delivery plan, success criteria
- [Architecture](./docs/architecture.md) — system design, modules, data flows
