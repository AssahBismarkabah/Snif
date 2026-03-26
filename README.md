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

The quality targets for Phase 1 are at least 80% precision, at least 60% recall,
a noise rate under 10%, and review completion within 120 seconds. Changes to
prompts, models, or retrieval must pass a fixed evaluation harness before
shipping.


# Status

This repository is in the documentation phase. Implementation has not started.
See the [Product](./docs/product.md) document for scope, delivery plan, and
success criteria. See the [Architecture](./docs/architecture.md) document for
the technical design.
