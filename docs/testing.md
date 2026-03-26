# Testing

This document describes how to test Snif locally and how to set up the
evaluation harness.


# Local Testing

Local testing runs the full Snif pipeline against a repository on your machine.
No GitHub API access is needed — the diff is read from a local file and
findings are printed to stdout.

## Prerequisites

Build the binary:

```
cargo build
```

Create a `.snif.json` at the root of the repository you want to test. At
minimum, configure the LLM provider:

```json
{
  "model": {
    "review_model": "your-review-model",
    "summary_model": "your-summary-model",
    "endpoint": "https://your-provider.com/v1"
  }
}
```

Set the API key:

```
export SNIF_API_KEY=your-api-key
```

## Step 1: Index the Repository

Build the repository index. This parses all source files, builds the structural
graph, generates LLM summaries, and computes vector embeddings.

```
RUST_LOG=info cargo run -- index --path /path/to/repo
```

On the first run, use `--full` to build a clean index:

```
RUST_LOG=info cargo run -- index --path /path/to/repo --full
```

The index is stored at `.snif/index.db` relative to the repository root. You
can inspect it directly:

```
sqlite3 .snif/index.db "SELECT COUNT(*) FROM files;"
sqlite3 .snif/index.db "SELECT COUNT(*) FROM symbols;"
sqlite3 .snif/index.db "SELECT COUNT(*) FROM summaries;"
sqlite3 .snif/index.db "SELECT summary FROM summaries LIMIT 3;"
```

If no API key is set, the structural index is built but LLM summaries and
embeddings are skipped. The review command still works with reduced context
quality.

## Step 2: Generate a Test Diff

Create a diff from a recent commit:

```
cd /path/to/repo
git diff HEAD~1 > /tmp/test.diff
```

Or diff between branches:

```
git diff main..feature-branch > /tmp/test.diff
```

## Step 3: Run the Review

```
RUST_LOG=info cargo run -- review --path /path/to/repo --diff-file /tmp/test.diff
```

Findings are printed to stdout as a JSON array. Each finding includes the file
path, line range, category, confidence, evidence, explanation, impact, and
optional suggestion.

If no issues are found, the output is an empty array and the log shows "No
findings — change looks clean".

## Step 4: Iterate

Modify the prompts in `crates/snif-prompts/src/lib.rs` and re-run the review.
No re-indexing is needed — the index is reused across review runs. Only re-index
when the codebase changes significantly or when you want to update summaries.


# Evaluation

The evaluation harness runs benchmark fixtures through the full review pipeline
and measures precision, recall, and noise rate.

## Fixture Format

Each fixture is a JSON file in the fixtures directory:

```json
{
  "name": "missing-null-check",
  "description": "Function dereferences nullable pointer without check",
  "diff": "--- a/src/handler.rs\n+++ b/src/handler.rs\n@@ -40,6 +40,8 @@\n ...",
  "files": {
    "src/handler.rs": "full file content here"
  },
  "conventions": "",
  "expected_findings": [
    {
      "file": "src/handler.rs",
      "start_line": 42,
      "category": "logic",
      "description": "Null pointer dereference"
    }
  ]
}
```

For clean changes that should produce no findings, set `expected_findings` to
an empty array. For style-noise changes that must not be flagged, also set it
to an empty array.

## Running the Harness

```
RUST_LOG=info cargo run -- eval --fixtures ./fixtures/
```

The harness loads each fixture, runs the review pipeline, compares actual
findings against expected findings, and computes aggregate metrics.

## Quality Gates

The evaluation command exits with code 0 if gates pass and code 1 if they fail.

The minimum thresholds are:

- Precision >= 70% (at least 70% of findings must be correct)
- Noise rate <= 20% (at most 20% of findings can be false positives)

The aspirational targets for Phase 1 are:

- Precision >= 80%
- Recall >= 60%
- Noise rate <= 10%

## Creating Fixtures

Start with a small set of 3-5 fixtures:

1. A change with a known bug (e.g., off-by-one error, missing error handling)
2. A clean change with no issues
3. A change with style noise that should not be flagged

Extract diffs and file contents from real repository history. Expected findings
should be concrete and verifiable — not hypothetical issues.

Expand the fixture set over time as the reviewer's behavior is observed in
production.
