use anyhow::{bail, Context, Result};
use serde::{Deserialize, Serialize};
use snif_config::ModelConfig;
use std::time::Instant;

pub struct LlmClient {
    http: reqwest::Client,
    endpoint: String,
    model: String,
    api_key: String,
}

pub struct ExecutionResult {
    pub response: String,
    pub duration: std::time::Duration,
}

#[derive(Serialize)]
struct ChatRequest {
    model: String,
    messages: Vec<Message>,
    temperature: f64,
}

#[derive(Serialize, Deserialize)]
struct Message {
    role: String,
    content: String,
}

#[derive(Deserialize)]
struct ChatResponse {
    choices: Vec<Choice>,
}

#[derive(Deserialize)]
struct Choice {
    message: Message,
}

impl LlmClient {
    pub fn new(endpoint: &str, model: &str, api_key: &str) -> Self {
        let http = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(300))
            .build()
            .unwrap_or_default();

        Self {
            http,
            endpoint: endpoint.trim_end_matches('/').to_string(),
            model: model.to_string(),
            api_key: api_key.to_string(),
        }
    }

    pub fn from_config(config: &ModelConfig, api_key: &str, use_review_model: bool) -> Self {
        let model = if use_review_model {
            &config.review_model
        } else {
            &config.summary_model
        };
        Self::new(&config.endpoint, model, api_key)
    }

    pub async fn chat_completion(&self, system_prompt: &str, user_prompt: &str) -> Result<String> {
        let url = format!("{}/chat/completions", self.endpoint);

        let request = ChatRequest {
            model: self.model.clone(),
            messages: vec![
                Message {
                    role: "system".to_string(),
                    content: system_prompt.to_string(),
                },
                Message {
                    role: "user".to_string(),
                    content: user_prompt.to_string(),
                },
            ],
            temperature: 0.0,
        };

        let max_retries = 3;
        let mut last_error = String::new();

        for attempt in 0..=max_retries {
            if attempt > 0 {
                let delay = std::time::Duration::from_secs(2u64.pow(attempt as u32));
                tracing::warn!(
                    attempt,
                    delay_secs = delay.as_secs(),
                    "Retrying LLM request after server error"
                );
                tokio::time::sleep(delay).await;
            }

            let response = match self
                .http
                .post(&url)
                .header("Authorization", format!("Bearer {}", self.api_key))
                .header("Content-Type", "application/json")
                .json(&request)
                .send()
                .await
            {
                Ok(r) => r,
                Err(e) => {
                    last_error = format!("Request failed: {}", e);
                    continue;
                }
            };

            let status = response.status();
            if status.is_server_error() || status.as_u16() == 429 || status.as_u16() == 408 {
                last_error = format!("Server error {}", status);
                continue;
            }

            if !status.is_success() {
                let body = response.text().await.unwrap_or_default();
                bail!("LLM provider returned {}: {}", status, body);
            }

            let chat_response: ChatResponse = response
                .json()
                .await
                .context("Failed to parse LLM provider response")?;

            return chat_response
                .choices
                .into_iter()
                .next()
                .map(|c| c.message.content.trim().to_string())
                .context("LLM provider returned no choices");
        }

        bail!(
            "LLM request failed after {} retries: {}",
            max_retries,
            last_error
        )
    }
}

pub fn execute_review(
    system_prompt: &str,
    user_prompt: &str,
    config: &ModelConfig,
) -> Result<ExecutionResult> {
    let api_key = std::env::var("SNIF_API_KEY")
        .or_else(|_| std::env::var("OPENAI_API_KEY"))
        .context("SNIF_API_KEY or OPENAI_API_KEY must be set")?;

    let client = LlmClient::from_config(config, &api_key, true);

    let rt = tokio::runtime::Runtime::new()?;
    let start = Instant::now();

    let response = rt.block_on(client.chat_completion(system_prompt, user_prompt))?;
    let duration = start.elapsed();

    tracing::info!(
        model = %config.review_model,
        duration = ?duration,
        response_len = response.len(),
        "Review execution complete"
    );

    Ok(ExecutionResult { response, duration })
}
