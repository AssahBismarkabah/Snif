# Configuration

Snif is configured through a `.snif.json` file at the repository root and
environment variables for secrets. All configuration fields have sensible
defaults. A minimal setup requires only an LLM provider endpoint and model
names.


# Configuration File

The `.snif.json` file lives at the root of the repository being reviewed. It
controls the LLM provider, indexing behavior, context retrieval, and output
filtering. All fields are optional and fall back to defaults if omitted.

An example file with all fields and provider examples is at
`.snif.json.example`. Copy it to `.snif.json` and adjust for your setup. The
config file should be committed to the repository — it contains no secrets.


# Quick Start Examples

A minimal `.snif.json` for OpenAI:

```json
{
  "model": {
    "review_model": "gpt-4o",
    "summary_model": "gpt-4o-mini",
    "endpoint": "https://api.openai.com/v1"
  }
}
```

For AWS Bedrock with Claude:

```json
{
  "model": {
    "review_model": "anthropic.claude-sonnet-4-6",
    "summary_model": "anthropic.claude-haiku-4-5-20251001-v1:0",
    "endpoint": "https://bedrock-runtime.us-west-2.amazonaws.com/v1"
  }
}
```

For any OpenAI-compatible endpoint (self-hosted, Azure, third-party):

```json
{
  "model": {
    "review_model": "your-model-name",
    "summary_model": "your-model-name",
    "endpoint": "https://your-provider.com/v1"
  }
}
```

All other fields use sensible defaults when omitted. Set `SNIF_API_KEY` in your
environment with the provider's API key.


# Fields

## model

Controls the LLM provider connection and model selection.

`endpoint` is the base URL of any OpenAI-compatible chat completions API. This
can be OpenAI, AWS Bedrock, Azure, or any self-hosted endpoint. The URL should
point to the API base, not the completions path — Snif appends
`/chat/completions` automatically.

`review_model` is the model used for code review. This should be a capable
reasoning model since review quality depends on it.

`summary_model` is the model used during indexing for generating code
summaries. A cheaper, faster model works well here since summaries are short
and factual.

Defaults: all three fields are empty strings. At least `endpoint` and
`review_model` must be set for `snif review` to work. `summary_model` must be
set for `snif index` to generate summaries.

## platform

`provider` specifies the code hosting platform. Supported values: `github` and
`gitlab`. Default: `github`. In CI, the platform is auto-detected from
environment variables (`GITHUB_REPOSITORY` for GitHub, `CI_PROJECT_PATH` for
GitLab).

`api_base` is the GitLab API base URL for self-hosted instances. Default:
`https://gitlab.com/api/v4`. Not needed for GitHub or gitlab.com. In GitLab
CI, the `CI_API_V4_URL` variable is read automatically.

## index

`db_path` is the path to the SQLite database that stores the repository index.
Default: `.snif/index.db`. Can be overridden by the `SNIF_DB_PATH` environment
variable.

`embedding_dimension` is the vector dimension for the embedding model. Default:
384 (matches the AllMiniLML6V2 model used by fastembed). Do not change this
unless you change the embedding model.

`languages` is the list of languages to parse. Default: `["rust", "typescript",
"python"]`. Files in unsupported languages are skipped.

`exclude_patterns` is the list of directory names to skip during parsing.
Default: `["target", "node_modules", "vendor", ".git"]`.

## context

`max_tokens` is the maximum token budget for the rendered prompt sent to the
LLM during review. The budget is enforced on the fully formatted prompt
including line numbers, markdown, and headers. If the rendered prompt exceeds
this limit, Snif removes the lowest-ranked related files until it fits.
Default: 128000. Token count is estimated at 3 characters per token
(conservative).

`max_files` is the maximum number of related files to include in the context.
Default: 50.

`output_reserve_tokens` is the number of tokens reserved for the model's
response output. The prompt budget is `max_tokens - output_reserve_tokens`.
Default: 4096.

`retrieval_weights` controls how structural, semantic, and keyword retrieval
results are weighted when ranking related files. Default: structural 1.0,
semantic 0.7, keyword 0.3. Higher weight means that retrieval method's results
rank higher in the final list.

## filter

`min_confidence` is the minimum confidence score for a finding to survive
filtering. Findings below this threshold are suppressed. Default: 0.7.

`suppress_style_only` controls whether style-only findings (formatting, naming
preferences) are suppressed. Default: true.

`feedback_min_signals` is the minimum number of developer feedback signals
before the learned feedback filter activates. Until this threshold is reached,
only the static filter runs. Default: 20.

## conventions_paths

A list of file paths (relative to the repository root) containing coding
conventions and review guidelines. These are included in the review context to
help the reviewer understand project-specific rules. Default: empty.

## eval_fixtures_path

Path to the directory containing evaluation benchmark fixtures. Used by
`snif eval`. Default: null.


# Environment Variables

Secrets and deployment-specific settings come from environment variables.
Environment variables override the corresponding `.snif.json` fields where
applicable.

`SNIF_API_KEY` or `OPENAI_API_KEY` is the API key for the configured LLM
provider. Snif checks `SNIF_API_KEY` first, then falls back to `OPENAI_API_KEY`.
Required for `snif index` (summarization) and `snif review`.

`SNIF_ENDPOINT` overrides `model.endpoint` from the config file. Useful for CI
environments where the endpoint varies.

`SNIF_DB_PATH` overrides `index.db_path` from the config file.

`GITHUB_TOKEN` is required when using the GitHub adapter (`--repo` and `--pr`
flags). This is a GitHub personal access token or GitHub App token with read
access to pull requests and write access to review comments.

`GITHUB_REPOSITORY` is automatically set by GitHub Actions in CI. Format:
`owner/repo`. Used by the GitHub adapter when running in CI.

`SNIF_PR_NUMBER` or `GITHUB_PR_NUMBER` is the pull request number. Used by the
GitHub adapter when running in CI.

`SNIF_PLATFORM` overrides platform auto-detection. Set to `github` or `gitlab`.

`GITLAB_TOKEN` is a GitLab personal or project access token with `api` scope.
Required for posting merge request comments. Falls back to `CI_JOB_TOKEN` in
GitLab CI if not set.

`CI_PROJECT_PATH` is automatically set by GitLab CI. Format: `group/project`.
Used for auto-detecting GitLab platform and as the project identifier.

`CI_MERGE_REQUEST_IID` is automatically set by GitLab CI in merge request
pipelines. Used as the merge request identifier.

`CI_API_V4_URL` is automatically set by GitLab CI. Points to the instance's
API base (e.g., `https://gitlab.com/api/v4` or
`https://git.example.com/api/v4`). Used for self-hosted GitLab support.


# CLI Commands

## snif index

Builds the repository index: parses source files, builds the structural graph,
analyzes co-change patterns, generates LLM summaries, and computes vector
embeddings.

```
snif index [--path <PATH>] [--full]
```

`--path` is the repository root. Default: current directory.

`--full` forces a complete rebuild of the index, dropping all existing data.
Without this flag, indexing is incremental — only changed files are reprocessed.

## snif review

Reviews a code change using the indexed repository context.

```
snif review [--path <PATH>] [--diff-file <FILE>] [--repo <OWNER/REPO>] [--pr <NUMBER>]
```

`--path` is the repository root. Default: current directory.

`--diff-file` reads a unified diff from a local file. Findings are printed to
stdout. This mode does not require GitHub credentials.

`--repo` and `--pr` use the GitHub adapter to fetch the diff from a pull
request and post findings as inline review comments. Requires `GITHUB_TOKEN`.

One of `--diff-file` or `--repo`/`--pr` must be provided.

## snif eval

Runs the evaluation harness against benchmark fixtures.

```
snif eval --fixtures <PATH> [--path <PATH>]
```

`--fixtures` is the path to the directory containing JSON fixture files.

`--path` is the repository root for config loading. Default: current directory.

The command exits with code 0 if quality gates pass and code 1 if they fail.
Quality gates: precision >= 70%, noise rate <= 20%.
