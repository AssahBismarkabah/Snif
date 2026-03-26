# Crates

Snif is organized as a Cargo workspace. Each crate owns one responsibility and
has a distinct dependency profile. This keeps compilation isolated — changes to
the parser do not recompile SQLite, and changes to the config do not recompile
tree-sitter.

`snif-cli` is the binary crate. It provides the `snif` command with four
subcommands: `index`, `review`, `clean`, and `eval`. It orchestrates the
pipeline by calling into the library crates below.

`snif-config` loads `.snif.json` from the repository root, merges environment
variables, and validates settings. It exposes the `SnifConfig` struct consumed
by every other crate.

`snif-types` defines the shared domain types used across the workspace:
extraction types (Language, Symbol, Import, Reference), finding types (Finding,
Fingerprint, FindingCategory), and review types (ContextPackage, RetrievalResult,
ChangeMetadata).

`snif-parser` uses tree-sitter to parse source files into ASTs and extract
imports, symbol definitions, and references. It ships with language adapters for
Rust, TypeScript, and Python. Adding a new language means implementing the
`LanguageAdapter` trait.

`snif-graph` takes the parsed extractions and writes the structural relationship
graph to the store. It handles file hashing for incremental indexing.

`snif-store` manages the SQLite database with sqlite-vec extensions. It provides
CRUD operations for files, symbols, imports, references, co-change pairs,
summaries, and vector embeddings.

`snif-cochange` analyzes git history to find files that frequently change
together and stores correlation scores.

`snif-summarizer` generates natural language summaries of code units by calling
the configured LLM provider. It walks the symbol graph bottom-up: functions
first, then files.

`snif-embeddings` computes vector embeddings of summaries using fastembed
(AllMiniLML6V2, 384 dimensions) and stores them in the sqlite-vec table.

`snif-retrieval` queries the index using three methods: structural graph
traversal, semantic vector similarity, and keyword matching. It merges and ranks
results by configurable weights.

`snif-context` assembles the context package sent to the review model. It
enforces the token budget, includes changed files and ranked related files, and
records omissions.

`snif-prompts` renders the system prompt and user prompt from the context
package. It defines the output schema that the model must follow.

`snif-execution` provides the provider-neutral LLM HTTP client. It calls any
OpenAI-compatible chat completions endpoint. Used by both the summarizer and the
review command.

`snif-output` parses the model response into structured findings, applies static
filter rules (confidence, evidence, impact, style suppression, deduplication),
and computes stable fingerprints for annotation lifecycle tracking.

`snif-platform` defines the `PlatformAdapter` trait and implements the GitHub
adapter. The adapter fetches PR diffs and metadata, posts findings as inline
review comments, and resolves stale comments from prior runs.

`snif-feedback` implements the feedback learning system. It collects developer
reactions to findings, stores them with vector embeddings, and uses similarity
matching to suppress findings that resemble past-rejected output.

`snif-eval` runs benchmark fixtures through the review pipeline and computes
precision, recall, and noise rate. It enforces quality gates and exits with a
non-zero code if thresholds are not met.
