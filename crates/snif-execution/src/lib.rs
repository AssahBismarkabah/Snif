use anyhow::{bail, Context, Result};
use serde::{Deserialize, Serialize};
use snif_config::{constants::timeouts, constants, env::{keys, get_api_key}, ModelConfig};
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
    response_format: ResponseFormat,
}

#[derive(Serialize)]
struct ResponseFormat {
    #[serde(rename = "type")]
    kind: &'static str,
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
            .timeout(std::time::Duration::from_secs(
                timeouts::LLM_REQUEST_TIMEOUT_SECS,
            ))
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
        use snif_config::constants::http;

        let url = format!("{}{}", self.endpoint, http::OPENAI_CHAT_COMPLETIONS);

        let request = ChatRequest {
            model: self.model.clone(),
            messages: vec![
                Message {
                    role: http::ROLE_SYSTEM.to_string(),
                    content: system_prompt.to_string(),
                },
                Message {
                    role: http::ROLE_USER.to_string(),
                    content: user_prompt.to_string(),
                },
            ],
            temperature: constants::model::DEFAULT_TEMPERATURE,
            response_format: ResponseFormat {
                kind: constants::model::RESPONSE_FORMAT_JSON,
            },
        };

        let max_retries = timeouts::LLM_MAX_RETRIES;
        let mut last_error = String::new();

        for attempt in 0..=max_retries {
            if attempt > 0 {
                let delay = std::time::Duration::from_secs(
                    timeouts::LLM_RETRY_BASE_DELAY_SECS.pow(attempt),
                );
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
                .header("Authorization", format!("{} {}", http::AUTHORIZATION_BEARER, self.api_key))
                .header("Content-Type", http::CONTENT_TYPE_JSON)
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
            if status.is_server_error() || status.as_u16() == http::STATUS_TOO_MANY_REQUESTS || status.as_u16() == http::STATUS_REQUEST_TIMEOUT {
                last_error = format!("Server error {}", status);
                continue;
            }

            if !status.is_success() {
                let body = response.text().await.unwrap_or_default();
                bail!("{} {}: {}", http::ERROR_LLM_PROVIDER, status, body);
            }

            let chat_response: ChatResponse = response
                .json()
                .await
                .context(http::ERROR_PARSE_RESPONSE)?;

            return chat_response
                .choices
                .into_iter()
                .next()
                .map(|c| c.message.content.trim().to_string())
                .context(http::ERROR_NO_CHOICES);
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
    let api_key = get_api_key(keys::SNIF_API_KEY, keys::OPENAI_API_KEY)?;

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

pub fn repair_review_response(raw_response: &str, config: &ModelConfig) -> Result<ExecutionResult> {
    let repair_system_prompt = constants::prompts::REPAIR_SYSTEM_PROMPT;
    let repair_user_prompt = format!(
        "{}{}",
        constants::prompts::REPAIR_USER_PROMPT_INTRO,
        raw_response
    );

    execute_review(repair_system_prompt, &repair_user_prompt, config)
}
