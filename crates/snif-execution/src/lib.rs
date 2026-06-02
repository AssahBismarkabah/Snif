use anyhow::{bail, Context, Result};
use serde::{Deserialize, Serialize};
use snif_config::{
    constants,
    constants::timeouts,
    env::{get_api_key, keys},
    ModelConfig,
};
use std::fmt;
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

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LlmRetryFailureKind {
    RateLimited,
    RetryableServerError,
    RequestFailed,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LlmRetryFailure {
    pub kind: LlmRetryFailureKind,
    pub max_retries: u32,
    pub status: Option<u16>,
    pub retry_after: Option<String>,
    pub body: Option<String>,
    pub message: String,
}

impl LlmRetryFailure {
    fn request_failed(max_retries: u32, message: String) -> Self {
        Self {
            kind: LlmRetryFailureKind::RequestFailed,
            max_retries,
            status: None,
            retry_after: None,
            body: None,
            message,
        }
    }

    fn retryable_response(
        max_retries: u32,
        status: u16,
        retry_after: Option<String>,
        body: String,
    ) -> Self {
        let kind = if status == snif_config::constants::http::STATUS_TOO_MANY_REQUESTS {
            LlmRetryFailureKind::RateLimited
        } else {
            LlmRetryFailureKind::RetryableServerError
        };
        let message = if body.trim().is_empty() {
            format!("Server error {}", status)
        } else {
            format!("Server error {}: {}", status, truncate_for_log(&body))
        };

        Self {
            kind,
            max_retries,
            status: Some(status),
            retry_after,
            body: Some(body),
            message,
        }
    }

    pub fn is_rate_limited(&self) -> bool {
        self.kind == LlmRetryFailureKind::RateLimited
    }
}

impl fmt::Display for LlmRetryFailure {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if self.is_rate_limited() {
            write!(
                f,
                "LLM request was rate-limited after {} retries",
                self.max_retries
            )?;
        } else {
            write!(f, "LLM request failed after {} retries", self.max_retries)?;
        }

        if let Some(status) = self.status {
            write!(f, ": status {}", status)?;
        } else if !self.message.is_empty() {
            write!(f, ": {}", self.message)?;
        }

        if let Some(retry_after) = &self.retry_after {
            write!(f, ", retry_after={}", retry_after)?;
        }

        if let Some(body) = &self.body {
            if !body.trim().is_empty() {
                write!(f, ", provider_body={}", truncate_for_log(body))?;
            }
        }

        if self.is_rate_limited() {
            write!(
                f,
                ". Reduce context.max_tokens, lower context.summarizer_concurrency, or retry after provider quota resets."
            )?;
        }

        Ok(())
    }
}

impl std::error::Error for LlmRetryFailure {}

pub fn is_rate_limit_error(error: &anyhow::Error) -> bool {
    error
        .downcast_ref::<LlmRetryFailure>()
        .map(LlmRetryFailure::is_rate_limited)
        .unwrap_or(false)
}

fn truncate_for_log(text: &str) -> String {
    const MAX_LOG_CHARS: usize = 500;
    let trimmed = text.trim();
    if trimmed.chars().count() <= MAX_LOG_CHARS {
        trimmed.to_string()
    } else {
        format!(
            "{}...",
            trimmed.chars().take(MAX_LOG_CHARS).collect::<String>()
        )
    }
}

#[derive(Serialize)]
struct ChatRequest {
    model: String,
    messages: Vec<Message>,
    temperature: f64,
    response_format: ResponseFormat,
    #[serde(skip_serializing_if = "Option::is_none")]
    max_tokens: Option<usize>,
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
    message: ChoiceMessage,
}

#[derive(Deserialize)]
struct ChoiceMessage {
    #[serde(default)]
    content: Option<String>,
    #[serde(default)]
    reasoning_content: Option<String>,
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
        self.chat_completion_with_max_tokens(system_prompt, user_prompt, None)
            .await
    }

    pub async fn chat_completion_with_max_tokens(
        &self,
        system_prompt: &str,
        user_prompt: &str,
        max_tokens: Option<usize>,
    ) -> Result<String> {
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
            max_tokens,
        };

        let max_retries = timeouts::LLM_MAX_RETRIES;
        let mut last_failure: Option<LlmRetryFailure> = None;

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
                .header(
                    "Authorization",
                    format!("{} {}", http::AUTHORIZATION_BEARER, self.api_key),
                )
                .header("Content-Type", http::CONTENT_TYPE_JSON)
                .json(&request)
                .send()
                .await
            {
                Ok(r) => r,
                Err(e) => {
                    last_failure = Some(LlmRetryFailure::request_failed(
                        max_retries,
                        format!("Request failed: {}", e),
                    ));
                    continue;
                }
            };

            let status = response.status();
            if status.is_server_error()
                || status.as_u16() == http::STATUS_TOO_MANY_REQUESTS
                || status.as_u16() == http::STATUS_REQUEST_TIMEOUT
            {
                let retry_after = response
                    .headers()
                    .get("Retry-After")
                    .and_then(|value| value.to_str().ok())
                    .map(str::to_string);
                let body = response.text().await.unwrap_or_default();
                let failure = LlmRetryFailure::retryable_response(
                    max_retries,
                    status.as_u16(),
                    retry_after,
                    body,
                );
                tracing::warn!(
                    status = status.as_u16(),
                    retry_after = ?failure.retry_after,
                    body = failure.body.as_deref().map(truncate_for_log),
                    "LLM provider returned retryable error"
                );
                last_failure = Some(failure);
                continue;
            }

            if !status.is_success() {
                let body = response.text().await.unwrap_or_default();
                bail!("{} {}: {}", http::ERROR_LLM_PROVIDER, status, body);
            }

            let chat_response: ChatResponse =
                response.json().await.context(http::ERROR_PARSE_RESPONSE)?;

            let first_choice = chat_response
                .choices
                .into_iter()
                .next()
                .context("LLM provider returned empty choices array")?;

            let content = first_choice.message.content.clone();
            let reasoning = first_choice.message.reasoning_content.clone();

            // Some models (e.g. GLM) return JSON in reasoning_content
            // instead of the standard content field. Need to check for
            // empty strings, not just None.
            let text = content
                .filter(|s| !s.trim().is_empty())
                .or_else(|| reasoning.filter(|s| !s.trim().is_empty()));

            match text {
                Some(t) => return Ok(t.trim().to_string()),
                None => {
                    tracing::warn!(
                        content = ?first_choice.message.content,
                        reasoning_content_len = first_choice.message.reasoning_content.as_ref().map(|s| s.len()),
                        "LLM returned empty response"
                    );
                    bail!("LLM provider returned empty response (both content and reasoning_content are empty)")
                }
            }
        }

        Err(anyhow::Error::new(last_failure.unwrap_or_else(|| {
            LlmRetryFailure::request_failed(max_retries, "Unknown retry failure".to_string())
        })))
    }
}

pub fn execute_review(
    system_prompt: &str,
    user_prompt: &str,
    config: &ModelConfig,
) -> Result<ExecutionResult> {
    execute_review_with_max_tokens(system_prompt, user_prompt, config, None)
}

pub fn execute_review_with_max_tokens(
    system_prompt: &str,
    user_prompt: &str,
    config: &ModelConfig,
    max_tokens: Option<usize>,
) -> Result<ExecutionResult> {
    let api_key = get_api_key(keys::SNIF_API_KEY, keys::OPENAI_API_KEY)?;

    let client = LlmClient::from_config(config, &api_key, true);

    let rt = tokio::runtime::Runtime::new()?;
    let start = Instant::now();

    let response = rt.block_on(client.chat_completion_with_max_tokens(
        system_prompt,
        user_prompt,
        max_tokens,
    ))?;
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
    repair_review_response_with_max_tokens(raw_response, config, None)
}

pub fn repair_review_response_with_max_tokens(
    raw_response: &str,
    config: &ModelConfig,
    max_tokens: Option<usize>,
) -> Result<ExecutionResult> {
    let repair_system_prompt = constants::prompts::REPAIR_SYSTEM_PROMPT;
    let repair_user_prompt = format!(
        "{}{}",
        constants::prompts::REPAIR_USER_PROMPT_INTRO,
        raw_response
    );

    execute_review_with_max_tokens(
        repair_system_prompt,
        &repair_user_prompt,
        config,
        max_tokens,
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn retry_exhausted_429_is_classified_as_rate_limited() {
        let error = LlmRetryFailure::retryable_response(
            5,
            429,
            Some("60".to_string()),
            "{\"error\":\"quota exceeded\"}".to_string(),
        );
        let anyhow_error = anyhow::Error::new(error.clone());

        assert!(error.is_rate_limited());
        assert!(is_rate_limit_error(&anyhow_error));
        assert_eq!(error.retry_after.as_deref(), Some("60"));
        assert_eq!(
            error.body.as_deref(),
            Some("{\"error\":\"quota exceeded\"}")
        );
    }

    #[test]
    fn retry_exhausted_server_error_is_not_rate_limited() {
        let error = LlmRetryFailure::retryable_response(5, 500, None, "oops".to_string());
        let anyhow_error = anyhow::Error::new(error.clone());

        assert!(!error.is_rate_limited());
        assert!(!is_rate_limit_error(&anyhow_error));
    }

    #[test]
    fn chat_request_serializes_max_tokens_when_present() {
        let request = ChatRequest {
            model: "test-model".to_string(),
            messages: Vec::new(),
            temperature: 0.0,
            response_format: ResponseFormat {
                kind: constants::model::RESPONSE_FORMAT_JSON,
            },
            max_tokens: Some(4096),
        };

        let value = serde_json::to_value(request).unwrap();

        assert_eq!(value["max_tokens"], 4096);
    }

    #[test]
    fn chat_request_omits_max_tokens_when_absent() {
        let request = ChatRequest {
            model: "test-model".to_string(),
            messages: Vec::new(),
            temperature: 0.0,
            response_format: ResponseFormat {
                kind: constants::model::RESPONSE_FORMAT_JSON,
            },
            max_tokens: None,
        };

        let value = serde_json::to_value(request).unwrap();

        assert!(value.get("max_tokens").is_none());
    }
}
