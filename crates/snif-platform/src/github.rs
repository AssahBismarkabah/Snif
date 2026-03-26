use crate::PlatformAdapter;
use anyhow::{bail, Context, Result};
use snif_types::{ChangeMetadata, Finding, Fingerprint};

pub struct GitHubAdapter {
    token: String,
    owner: String,
    repo: String,
    pr_number: u64,
    http: reqwest::blocking::Client,
}

impl GitHubAdapter {
    pub fn new(owner: &str, repo: &str, pr_number: u64) -> Result<Self> {
        let token = std::env::var("GITHUB_TOKEN")
            .context("GITHUB_TOKEN environment variable must be set")?;

        Ok(Self {
            token,
            owner: owner.to_string(),
            repo: repo.to_string(),
            pr_number,
            http: reqwest::blocking::Client::new(),
        })
    }

    pub fn from_env() -> Result<Self> {
        let repo_full = std::env::var("GITHUB_REPOSITORY")
            .context("GITHUB_REPOSITORY not set (expected owner/repo)")?;
        let parts: Vec<&str> = repo_full.splitn(2, '/').collect();
        if parts.len() != 2 {
            bail!("GITHUB_REPOSITORY must be in owner/repo format");
        }

        let pr_number: u64 = std::env::var("SNIF_PR_NUMBER")
            .or_else(|_| std::env::var("GITHUB_PR_NUMBER"))
            .context("SNIF_PR_NUMBER or GITHUB_PR_NUMBER must be set")?
            .parse()
            .context("PR number must be a valid integer")?;

        Self::new(parts[0], parts[1], pr_number)
    }

    fn api_url(&self, path: &str) -> String {
        format!(
            "https://api.github.com/repos/{}/{}/{}",
            self.owner, self.repo, path
        )
    }

    fn get(&self, path: &str) -> Result<reqwest::blocking::Response> {
        let url = self.api_url(path);
        let response = self
            .http
            .get(&url)
            .header("Authorization", format!("Bearer {}", self.token))
            .header("Accept", "application/vnd.github.v3+json")
            .header("User-Agent", "snif-review-agent")
            .send()
            .context("Failed to call GitHub API")?;

        if !response.status().is_success() {
            let status = response.status();
            let body = response.text().unwrap_or_default();
            bail!("GitHub API {} returned {}: {}", path, status, body);
        }

        Ok(response)
    }
}

impl PlatformAdapter for GitHubAdapter {
    fn fetch_diff(&self) -> Result<String> {
        let url = self.api_url(&format!("pulls/{}", self.pr_number));
        let response = self
            .http
            .get(&url)
            .header("Authorization", format!("Bearer {}", self.token))
            .header("Accept", "application/vnd.github.v3.diff")
            .header("User-Agent", "snif-review-agent")
            .send()
            .context("Failed to fetch PR diff")?;

        if !response.status().is_success() {
            let status = response.status();
            let body = response.text().unwrap_or_default();
            bail!("GitHub diff API returned {}: {}", status, body);
        }

        response.text().context("Failed to read diff body")
    }

    fn fetch_changed_paths(&self) -> Result<Vec<String>> {
        let response = self.get(&format!("pulls/{}/files", self.pr_number))?;
        let files: Vec<serde_json::Value> = response.json()?;

        let mut paths = Vec::new();
        for f in &files {
            if let Some(name) = f.get("filename").and_then(serde_json::Value::as_str) {
                paths.push(name.to_string());
            }
        }

        Ok(paths)
    }

    fn fetch_metadata(&self) -> Result<ChangeMetadata> {
        let response = self.get(&format!("pulls/{}", self.pr_number))?;
        let pr: serde_json::Value = response.json()?;

        let title = pr.get("title")
            .and_then(serde_json::Value::as_str)
            .map(String::from);

        let author = pr.get("user")
            .and_then(|u: &serde_json::Value| u.get("login"))
            .and_then(serde_json::Value::as_str)
            .map(String::from);

        let base_branch = pr.get("base")
            .and_then(|b: &serde_json::Value| b.get("ref"))
            .and_then(serde_json::Value::as_str)
            .map(String::from);

        Ok(ChangeMetadata { title, author, base_branch })
    }

    fn post_findings(&self, findings: &[Finding]) -> Result<()> {
        for finding in findings {
            let body = serde_json::json!({
                "body": format!(
                    "**[{}]** {} (confidence: {:.0}%)\n\n{}\n\n**Impact:** {}\n\n**Evidence:**\n```\n{}\n```{}",
                    finding.category,
                    finding.explanation,
                    finding.confidence * 100.0,
                    finding.explanation,
                    finding.impact,
                    finding.evidence,
                    finding.suggestion.as_ref().map_or(String::new(), |s| format!("\n\n**Suggestion:** {}", s))
                ),
                "commit_id": serde_json::Value::Null,
                "path": finding.location.path,
                "line": finding.location.start_line,
                "side": "RIGHT",
            });

            let url = self.api_url(&format!("pulls/{}/comments", self.pr_number));
            let response = self
                .http
                .post(&url)
                .header("Authorization", format!("Bearer {}", self.token))
                .header("Accept", "application/vnd.github.v3+json")
                .header("User-Agent", "snif-review-agent")
                .json(&body)
                .send()?;

            if response.status().is_success() {
                tracing::info!(
                    file = %finding.location.path,
                    line = finding.location.start_line,
                    "Posted finding"
                );
            } else {
                tracing::warn!(
                    file = %finding.location.path,
                    status = %response.status(),
                    "Failed to post finding"
                );
            }
        }

        Ok(())
    }

    fn get_prior_fingerprints(&self) -> Result<Vec<Fingerprint>> {
        // TODO: fetch prior bot comments and extract fingerprints
        Ok(vec![])
    }

    fn resolve_stale(&self, _stale: &[Fingerprint]) -> Result<()> {
        // TODO: resolve stale bot comments
        Ok(())
    }
}
