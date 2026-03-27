# Contributing

Snif is a repository-aware code review agent built in Rust. Contributions are
welcome — whether it's a bug fix, a new language adapter, improved prompts, or
better documentation.


# Getting Started

Clone the repository and build:

```
git clone https://github.com/AssahBismarkabah/Snif.git
cd Snif
cargo build
```

Run the tests:

```
cargo test --all
```

Run clippy and formatting checks:

```
cargo clippy --all-targets -- -D warnings
cargo fmt --all -- --check
```

These checks run in CI on every pull request. Your PR must pass all four
(check, test, clippy, fmt) before it can be merged.


# Making Changes

1. Create a branch from main for your work.
2. Make your changes. Keep commits focused — one logical change per commit.
3. Run `cargo fmt --all` before committing.
4. Run `cargo clippy --all-targets -- -D warnings` and fix any warnings.
5. Update the CHANGELOG.md with a summary of your change under an
   "Unreleased" section at the top.
6. Open a pull request against main.


# Changelog

Every pull request that changes behavior must update `CHANGELOG.md`. Add your
entry under the "Unreleased" section at the top of the file. When a release is
cut, the unreleased entries are moved under the new version heading.

Use this format:

```markdown
## Unreleased

- Fix: description of the bug fix
- Add: description of the new feature
- Change: description of the behavior change
- Remove: description of what was removed
```

Documentation-only changes and CI configuration changes do not require a
changelog entry.


# Code Organization

The codebase is a Cargo workspace under `crates/`. Each crate has one
responsibility. See `crates/README.md` for a description of each crate.

When adding code, put it in the crate that owns that responsibility. Do not
add unrelated logic to a crate. If your change spans multiple crates, that
is fine — but each crate's changes should be cohesive within that crate's
scope.


# Adding a Language Adapter

Snif uses tree-sitter for source code parsing. To add support for a new
language:

1. Add the tree-sitter grammar crate to `crates/snif-parser/Cargo.toml`.
2. Create a new adapter file in `crates/snif-parser/src/adapters/`.
3. Implement the `LanguageAdapter` trait with tree-sitter S-expression
   queries for imports, symbol definitions, and references.
4. Register the adapter in `crates/snif-parser/src/lib.rs` in the
   `all_adapters()` function.
5. Add the language to the default list in
   `crates/snif-config/src/lib.rs`.

See the existing Rust, TypeScript, and Python adapters for the pattern.
Each adapter is typically 100-150 lines.


# Adding Benchmark Fixtures

Fixtures live in `fixtures/` as directories. Each fixture contains:

- `fixture.json` — name, description, expected findings
- `change.patch` — the unified diff
- Real source files in their directory structure

See `fixtures/README.md` for the format. Before committing a fixture, verify
the source code is genuinely correct for clean fixtures and genuinely buggy
for bug fixtures. The eval harness catches fixture inaccuracies because the
model finds real issues the fixture author missed.


# Prompt Changes

Changes to the system or user prompts in `crates/snif-prompts/` directly
affect review quality. Before merging a prompt change:

1. Run the evaluation harness: `cargo run -- eval --fixtures ./fixtures/`
2. Verify quality gates pass (precision >= 70%, noise <= 20%).
3. Document the reasoning for the change in the PR description.

The eval workflow runs automatically on push to main when prompt files change.


# Pull Request Guidelines

- Keep PRs focused. One feature or fix per PR.
- Write a clear title and description. Snif reviews PRs automatically — a
  good description helps it understand the intent of your change.
- Include the changelog entry in the PR.
- Do not force-push after review has started.
- Address review feedback by adding new commits, not amending.


# Running Snif Locally

To test the full pipeline locally:

```
# Set up config
cp .snif.json.example .snif.json
# Edit .snif.json with your LLM provider

# Set API key
export SNIF_API_KEY=your-key

# Index
cargo run -- index --path .

# Review a diff
git diff HEAD~1 > /tmp/test.diff
cargo run -- review --diff-file /tmp/test.diff --path .

# Run evaluation
cargo run -- eval --fixtures ./fixtures/
```

See `docs/testing.md` for more details.
