use crate::{PlatformAdapter, BOT_MARKER, FINGERPRINT_MARKER};
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
        let token = resolve_token()?;

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

    fn auth_header(&self) -> String {
        format!("Bearer {}", self.token)
    }

    fn get(&self, path: &str) -> Result<reqwest::blocking::Response> {
        let url = self.api_url(path);
        let response = self
            .http
            .get(&url)
            .header("Authorization", self.auth_header())
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

    fn post(&self, path: &str, body: &serde_json::Value) -> Result<reqwest::blocking::Response> {
        let url = self.api_url(path);
        self.http
            .post(&url)
            .header("Authorization", self.auth_header())
            .header("Accept", "application/vnd.github.v3+json")
            .header("User-Agent", "snif-review-agent")
            .json(body)
            .send()
            .context("Failed to call GitHub API")
    }
}

/// Resolve a GitHub API token. Tries GitHub App auth first, falls back to GITHUB_TOKEN.
fn resolve_token() -> Result<String> {
    // Try GitHub App authentication
    if let (Ok(app_id), Ok(private_key), Ok(installation_id)) = (
        std::env::var("SNIF_APP_ID"),
        std::env::var("SNIF_APP_PRIVATE_KEY"),
        std::env::var("SNIF_APP_INSTALLATION_ID"),
    ) {
        tracing::info!("Authenticating as GitHub App");
        return get_installation_token(&app_id, &private_key, &installation_id);
    }

    // Fall back to GITHUB_TOKEN
    std::env::var("GITHUB_TOKEN").context(
        "No GitHub credentials found. Set GITHUB_TOKEN or SNIF_APP_ID + SNIF_APP_PRIVATE_KEY + SNIF_APP_INSTALLATION_ID",
    )
}

fn get_installation_token(
    app_id: &str,
    private_key: &str,
    installation_id: &str,
) -> Result<String> {
    let jwt = generate_jwt(app_id, private_key)?;

    let url = format!(
        "https://api.github.com/app/installations/{}/access_tokens",
        installation_id
    );

    let client = reqwest::blocking::Client::new();
    let response = client
        .post(&url)
        .header("Authorization", format!("Bearer {}", jwt))
        .header("Accept", "application/vnd.github.v3+json")
        .header("User-Agent", "snif-review-agent")
        .send()
        .context("Failed to get installation token")?;

    if !response.status().is_success() {
        let status = response.status();
        let body = response.text().unwrap_or_default();
        bail!("Failed to get installation token: {} {}", status, body);
    }

    let body: serde_json::Value = response.json()?;
    body.get("token")
        .and_then(serde_json::Value::as_str)
        .map(String::from)
        .context("Installation token response missing 'token' field")
}

fn generate_jwt(app_id: &str, private_key: &str) -> Result<String> {
    use jsonwebtoken::{encode, Algorithm, EncodingKey, Header};
    use serde::Serialize;

    #[derive(Serialize)]
    struct Claims {
        iat: u64,
        exp: u64,
        iss: String,
    }

    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs();

    let claims = Claims {
        iat: now - 60,        // 60 seconds in the past to account for clock drift
        exp: now + (10 * 60), // 10 minutes
        iss: app_id.to_string(),
    };

    let key = EncodingKey::from_rsa_pem(private_key.as_bytes())
        .context("Failed to parse GitHub App private key")?;

    encode(&Header::new(Algorithm::RS256), &claims, &key).context("Failed to sign JWT")
}

impl PlatformAdapter for GitHubAdapter {
    fn fetch_diff(&self) -> Result<String> {
        let url = self.api_url(&format!("pulls/{}", self.pr_number));
        let response = self
            .http
            .get(&url)
            .header("Authorization", self.auth_header())
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

        let title = pr
            .get("title")
            .and_then(serde_json::Value::as_str)
            .map(String::from);

        let author = pr
            .get("user")
            .and_then(|u: &serde_json::Value| u.get("login"))
            .and_then(serde_json::Value::as_str)
            .map(String::from);

        let base_branch = pr
            .get("base")
            .and_then(|b: &serde_json::Value| b.get("ref"))
            .and_then(serde_json::Value::as_str)
            .map(String::from);

        let description = pr
            .get("body")
            .and_then(serde_json::Value::as_str)
            .filter(|s| !s.is_empty())
            .map(String::from);

        let labels = pr
            .get("labels")
            .and_then(serde_json::Value::as_array)
            .map(|arr| {
                arr.iter()
                    .filter_map(|l| l.get("name").and_then(serde_json::Value::as_str))
                    .map(String::from)
                    .collect()
            })
            .unwrap_or_default();

        let commit_messages = match self.get(&format!("pulls/{}/commits", self.pr_number)) {
            Ok(resp) => {
                let commits: Vec<serde_json::Value> = resp.json().unwrap_or_default();
                commits
                    .iter()
                    .filter_map(|c| {
                        c.get("commit")
                            .and_then(|cm: &serde_json::Value| cm.get("message"))
                            .and_then(serde_json::Value::as_str)
                            .map(|s| s.lines().next().unwrap_or(s).to_string())
                    })
                    .collect()
            }
            Err(_) => vec![],
        };

        Ok(ChangeMetadata {
            title,
            author,
            base_branch,
            description,
            labels,
            commit_messages,
        })
    }

    fn post_findings(&self, findings: &[Finding]) -> Result<()> {
        for finding in findings {
            let comment_body = crate::format_finding_body(finding);

            let body = serde_json::json!({
                "body": comment_body,
                "commit_id": serde_json::Value::Null,
                "path": finding.location.file,
                "line": finding.location.start_line,
                "side": "RIGHT",
            });

            let path = format!("pulls/{}/comments", self.pr_number);
            match self.post(&path, &body) {
                Ok(r) if r.status().is_success() => {
                    tracing::info!(
                        file = %finding.location.file,
                        line = finding.location.start_line,
                        "Posted finding"
                    );
                }
                Ok(r) => {
                    tracing::warn!(
                        file = %finding.location.file,
                        status = %r.status(),
                        "Failed to post finding"
                    );
                }
                Err(e) => {
                    tracing::warn!(file = %finding.location.file, error = %e, "Failed to post finding");
                }
            }
        }

        Ok(())
    }

    fn post_summary(&self, summary: &str) -> Result<()> {
        let body = serde_json::json!({
            "body": format!("{}\n\n{}", BOT_MARKER, summary),
        });

        let path = format!("issues/{}/comments", self.pr_number);
        match self.post(&path, &body) {
            Ok(r) if r.status().is_success() => {
                tracing::info!("Posted review summary");
            }
            Ok(r) => {
                tracing::warn!(status = %r.status(), "Failed to post summary");
            }
            Err(e) => {
                tracing::warn!(error = %e, "Failed to post summary");
            }
        }

        Ok(())
    }

    fn get_prior_fingerprints(&self) -> Result<Vec<Fingerprint>> {
        let response = self.get(&format!("pulls/{}/comments", self.pr_number))?;
        let comments: Vec<serde_json::Value> = response.json()?;

        let mut fingerprints = Vec::new();
        for comment in &comments {
            let body = match comment.get("body").and_then(serde_json::Value::as_str) {
                Some(b) => b,
                None => continue,
            };

            if !body.contains(BOT_MARKER) {
                continue;
            }

            if let Some(start) = body.find(FINGERPRINT_MARKER) {
                let after = &body[start + FINGERPRINT_MARKER.len()..];
                if let Some(end) = after.find(" -->") {
                    let fp_id = after[..end].trim().to_string();
                    if !fp_id.is_empty() {
                        fingerprints.push(Fingerprint { id: fp_id });
                    }
                }
            }
        }

        tracing::info!(count = fingerprints.len(), "Fetched prior fingerprints");
        Ok(fingerprints)
    }

    fn resolve_stale(&self, stale: &[Fingerprint]) -> Result<()> {
        if stale.is_empty() {
            return Ok(());
        }

        let response = self.get(&format!("pulls/{}/comments", self.pr_number))?;
        let comments: Vec<serde_json::Value> = response.json()?;

        let stale_ids: Vec<&str> = stale.iter().map(|fp| fp.id.as_str()).collect();

        for comment in &comments {
            let body = match comment.get("body").and_then(serde_json::Value::as_str) {
                Some(b) => b,
                None => continue,
            };

            if !body.contains(BOT_MARKER) {
                continue;
            }

            let is_stale = if let Some(start) = body.find(FINGERPRINT_MARKER) {
                let after = &body[start + FINGERPRINT_MARKER.len()..];
                if let Some(end) = after.find(" -->") {
                    let fp_id = after[..end].trim();
                    stale_ids.contains(&fp_id)
                } else {
                    false
                }
            } else {
                false
            };

            if is_stale {
                if let Some(comment_id) = comment.get("id").and_then(serde_json::Value::as_i64) {
                    let reply_body = serde_json::json!({
                        "body": format!(
                            "{}\n\n**Resolved** — this issue is no longer present in the current change.",
                            BOT_MARKER
                        ),
                    });

                    let path = format!("pulls/{}/comments/{}/replies", self.pr_number, comment_id);

                    match self.post(&path, &reply_body) {
                        Ok(r) if r.status().is_success() => {
                            tracing::info!(comment_id, "Resolved stale finding");
                        }
                        Ok(r) => {
                            tracing::warn!(
                                comment_id,
                                status = %r.status(),
                                "Failed to resolve stale finding"
                            );
                        }
                        Err(e) => {
                            tracing::warn!(comment_id, error = %e, "Failed to resolve stale finding");
                        }
                    }
                }
            }
        }

        Ok(())
    }
}
