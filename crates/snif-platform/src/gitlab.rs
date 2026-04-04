use crate::{extract_fingerprints, PlatformAdapter, BOT_MARKER};
use anyhow::{bail, Context, Result};
use snif_types::{ChangeMetadata, Finding, Fingerprint};

pub struct GitLabAdapter {
    token: String,
    project_path: String,
    mr_iid: u64,
    api_base: String,
    http: reqwest::blocking::Client,
}

impl GitLabAdapter {
    pub fn new(project_path: &str, mr_iid: u64, api_base: Option<&str>) -> Result<Self> {
        let token = std::env::var("GITLAB_TOKEN")
            .or_else(|_| std::env::var("CI_JOB_TOKEN"))
            .context("GITLAB_TOKEN or CI_JOB_TOKEN must be set")?;

        let encoded_path = project_path.replace('/', "%2F");
        let base = api_base.unwrap_or("https://gitlab.com/api/v4").to_string();

        Ok(Self {
            token,
            project_path: encoded_path,
            mr_iid,
            api_base: base.trim_end_matches('/').to_string(),
            http: reqwest::blocking::Client::new(),
        })
    }

    pub fn from_env() -> Result<Self> {
        let project_path =
            std::env::var("CI_PROJECT_PATH").context("CI_PROJECT_PATH must be set")?;

        let mr_iid: u64 = std::env::var("CI_MERGE_REQUEST_IID")
            .context("CI_MERGE_REQUEST_IID must be set")?
            .parse()
            .context("CI_MERGE_REQUEST_IID must be a valid integer")?;

        let api_base = std::env::var("CI_API_V4_URL").ok();

        Self::new(&project_path, mr_iid, api_base.as_deref())
    }

    fn api_url(&self, path: &str) -> String {
        format!("{}/projects/{}/{}", self.api_base, self.project_path, path)
    }

    fn get(&self, path: &str) -> Result<reqwest::blocking::Response> {
        let url = self.api_url(path);
        let response = self
            .http
            .get(&url)
            .header("PRIVATE-TOKEN", &self.token)
            .header("User-Agent", "snif-review-agent")
            .send()
            .context("Failed to call GitLab API")?;

        if !response.status().is_success() {
            let status = response.status();
            let body = response.text().unwrap_or_default();
            bail!("GitLab API {} returned {}: {}", path, status, body);
        }

        Ok(response)
    }

    fn post(&self, path: &str, body: &serde_json::Value) -> Result<reqwest::blocking::Response> {
        let url = self.api_url(path);
        self.http
            .post(&url)
            .header("PRIVATE-TOKEN", &self.token)
            .header("User-Agent", "snif-review-agent")
            .json(body)
            .send()
            .context("Failed to call GitLab API")
    }

    fn put(&self, path: &str, body: &serde_json::Value) -> Result<reqwest::blocking::Response> {
        let url = self.api_url(path);
        self.http
            .put(&url)
            .header("PRIVATE-TOKEN", &self.token)
            .header("User-Agent", "snif-review-agent")
            .json(body)
            .send()
            .context("Failed to call GitLab API")
    }

    fn get_all_notes(&self) -> Result<Vec<serde_json::Value>> {
        let mut all_notes = Vec::new();
        let mut page = 1u32;
        let max_pages = 100;

        loop {
            if page > max_pages {
                tracing::warn!("Pagination exceeded max pages, stopping");
                break;
            }

            let path = format!(
                "merge_requests/{}/notes?sort=asc&per_page=100&page={}",
                self.mr_iid, page
            );
            let response = self.get(&path)?;

            let next_page = response
                .headers()
                .get("x-next-page")
                .and_then(|v| v.to_str().ok())
                .and_then(|s| s.parse::<u32>().ok());

            let notes: Vec<serde_json::Value> = response.json()?;
            all_notes.extend(notes);

            match next_page {
                Some(p) if p > page => page = p,
                _ => break,
            }
        }

        Ok(all_notes)
    }
}

impl PlatformAdapter for GitLabAdapter {
    fn fetch_diff(&self) -> Result<String> {
        let response = self.get(&format!("merge_requests/{}/changes", self.mr_iid))?;
        let data: serde_json::Value = response.json()?;

        let changes = data
            .get("changes")
            .and_then(serde_json::Value::as_array)
            .context("MR changes response missing 'changes' array")?;

        let mut unified_diff = String::new();
        for change in changes {
            let old_path = change
                .get("old_path")
                .and_then(serde_json::Value::as_str)
                .unwrap_or("unknown");
            let new_path = change
                .get("new_path")
                .and_then(serde_json::Value::as_str)
                .unwrap_or("unknown");
            let diff = change
                .get("diff")
                .and_then(serde_json::Value::as_str)
                .unwrap_or("");

            // Only add headers if not already present in the diff
            if diff.trim_start().starts_with("---") {
                unified_diff.push_str(diff);
                unified_diff.push('\n');
            } else {
                unified_diff.push_str(&format!(
                    "--- a/{}\n+++ b/{}\n{}\n",
                    old_path, new_path, diff
                ));
            }
        }

        Ok(unified_diff)
    }

    fn fetch_changed_paths(&self) -> Result<Vec<String>> {
        let response = self.get(&format!("merge_requests/{}/changes", self.mr_iid))?;
        let data: serde_json::Value = response.json()?;

        let changes = data
            .get("changes")
            .and_then(serde_json::Value::as_array)
            .context("MR changes response missing 'changes' array")?;

        let mut paths = Vec::new();
        for change in changes {
            if let Some(path) = change.get("new_path").and_then(serde_json::Value::as_str) {
                paths.push(path.to_string());
            }
        }

        Ok(paths)
    }

    fn fetch_metadata(&self) -> Result<ChangeMetadata> {
        let response = self.get(&format!("merge_requests/{}", self.mr_iid))?;
        let mr: serde_json::Value = response.json()?;

        let title = mr
            .get("title")
            .and_then(serde_json::Value::as_str)
            .map(String::from);

        let author = mr
            .get("author")
            .and_then(|a: &serde_json::Value| a.get("username"))
            .and_then(serde_json::Value::as_str)
            .map(String::from);

        let base_branch = mr
            .get("target_branch")
            .and_then(serde_json::Value::as_str)
            .map(String::from);

        let description = mr
            .get("description")
            .and_then(serde_json::Value::as_str)
            .filter(|s| !s.is_empty())
            .map(String::from);

        let labels = mr
            .get("labels")
            .and_then(serde_json::Value::as_array)
            .map(|arr| {
                arr.iter()
                    .filter_map(serde_json::Value::as_str)
                    .map(String::from)
                    .collect()
            })
            .unwrap_or_default();

        let commit_messages = match self.get(&format!("merge_requests/{}/commits", self.mr_iid)) {
            Ok(resp) => {
                let commits: Vec<serde_json::Value> = resp.json().unwrap_or_default();
                commits
                    .iter()
                    .filter_map(|c| {
                        c.get("message")
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
        if findings.is_empty() {
            return Ok(());
        }

        // Fetch diff_refs once for all findings
        let diff_refs = self
            .get(&format!("merge_requests/{}", self.mr_iid))
            .ok()
            .and_then(|resp| resp.json::<serde_json::Value>().ok())
            .and_then(|mr| {
                let refs = mr.get("diff_refs")?;
                let base = refs.get("base_sha")?.as_str()?;
                let head = refs.get("head_sha")?.as_str()?;
                let start = refs.get("start_sha")?.as_str()?;
                if base.is_empty() || head.is_empty() || start.is_empty() {
                    return None;
                }
                Some((base.to_string(), head.to_string(), start.to_string()))
            });

        for finding in findings {
            let comment_body = crate::format_finding_body(finding);

            // Try inline discussion first if we have valid diff_refs
            let inline_result = if let Some((ref base, ref head, ref start)) = diff_refs {
                let body = serde_json::json!({
                    "body": comment_body,
                    "position": {
                        "base_sha": base,
                        "head_sha": head,
                        "start_sha": start,
                        "position_type": "text",
                        "new_path": finding.location.file,
                        "old_path": finding.location.file,
                        "new_line": finding.location.start_line,
                    }
                });

                let path = format!("merge_requests/{}/discussions", self.mr_iid);
                self.post(&path, &body).ok()
            } else {
                None
            };

            // Check if inline succeeded, otherwise fall back to regular note
            let posted = match inline_result {
                Some(r) if r.status().is_success() => true,
                _ => {
                    let body = serde_json::json!({
                        "body": comment_body,
                    });
                    let path = format!("merge_requests/{}/notes", self.mr_iid);
                    matches!(self.post(&path, &body), Ok(r) if r.status().is_success())
                }
            };

            if posted {
                tracing::info!(
                    file = %finding.location.file,
                    line = finding.location.start_line,
                    "Posted finding"
                );
            } else {
                tracing::warn!(
                    file = %finding.location.file,
                    "Failed to post finding"
                );
            }
        }

        Ok(())
    }

    fn post_summary(&self, summary: &str) -> Result<()> {
        let body = serde_json::json!({
            "body": format!("{}\n\n{}", BOT_MARKER, summary),
        });

        let path = format!("merge_requests/{}/notes", self.mr_iid);
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
        let notes = self.get_all_notes()?;

        let mut fingerprints = Vec::new();
        for note in &notes {
            let body = match note.get("body").and_then(serde_json::Value::as_str) {
                Some(b) => b,
                None => continue,
            };

            if !body.contains(BOT_MARKER) {
                continue;
            }

            let (content_id, line_id) = extract_fingerprints(body);
            match (content_id, line_id) {
                (Some(cid), Some(lid)) => {
                    fingerprints.push(Fingerprint {
                        id: cid,
                        line_id: lid,
                    });
                }
                (Some(cid), None) => {
                    // Old format: single fingerprint was line-based
                    fingerprints.push(Fingerprint {
                        id: cid.clone(),
                        line_id: cid,
                    });
                }
                _ => {}
            }
        }

        tracing::info!(count = fingerprints.len(), "Fetched prior fingerprints");
        Ok(fingerprints)
    }

    fn resolve_stale(&self, stale: &[Fingerprint]) -> Result<()> {
        if stale.is_empty() {
            return Ok(());
        }

        let stale_content_ids: Vec<&str> = stale.iter().map(|fp| fp.id.as_str()).collect();

        // Fetch discussions to find threads with stale fingerprints
        let response = self.get(&format!("merge_requests/{}/discussions", self.mr_iid))?;
        let discussions: Vec<serde_json::Value> = response.json()?;

        for discussion in &discussions {
            let discussion_id = match discussion.get("id").and_then(serde_json::Value::as_str) {
                Some(id) => id,
                None => continue,
            };

            let notes = match discussion
                .get("notes")
                .and_then(serde_json::Value::as_array)
            {
                Some(n) => n,
                None => continue,
            };

            let is_stale = notes.iter().any(|note| {
                let body = note
                    .get("body")
                    .and_then(serde_json::Value::as_str)
                    .unwrap_or("");

                if !body.contains(BOT_MARKER) {
                    return false;
                }

                let (content_id, _line_id) = extract_fingerprints(body);
                content_id
                    .as_deref()
                    .is_some_and(|id| stale_content_ids.contains(&id))
            });

            if is_stale {
                let body = serde_json::json!({ "resolved": true });
                let path = format!(
                    "merge_requests/{}/discussions/{}",
                    self.mr_iid, discussion_id
                );

                match self.put(&path, &body) {
                    Ok(r) if r.status().is_success() => {
                        tracing::info!(discussion = discussion_id, "Resolved stale discussion");
                    }
                    Ok(r) => {
                        tracing::warn!(
                            discussion = discussion_id,
                            status = %r.status(),
                            "Failed to resolve discussion"
                        );
                    }
                    Err(e) => {
                        tracing::warn!(
                            discussion = discussion_id,
                            error = %e,
                            "Failed to resolve discussion"
                        );
                    }
                }
            }
        }

        Ok(())
    }
}
