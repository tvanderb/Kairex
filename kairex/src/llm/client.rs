use std::path::Path;
use std::time::Duration;

use tracing::{debug, instrument, warn};

use crate::config::LlmConfig;
use crate::llm::api_types::{
    ApiErrorResponse, ContentBlock, Message, MessagesRequest, MessagesResponse, Tool, ToolChoice,
};
use crate::llm::schemas::{AlertReport, EveningReport, MiddayReport, MorningReport, WeeklyReport};
use crate::llm::{LlmError, LlmProvider, Provider, ReportType};

const ANTHROPIC_VERSION: &str = "2023-06-01";

/// Raw output from a successful LLM call.
pub struct LlmResponse {
    /// Structured report JSON extracted from the tool use block.
    pub output: serde_json::Value,
    pub input_tokens: u32,
    pub output_tokens: u32,
    pub model: String,
}

pub struct LlmClient {
    http: reqwest::Client,
    config: LlmConfig,
    provider: Provider,
    api_key: String,
}

impl LlmClient {
    /// Create a new client. Reads the API key from the environment variable
    /// corresponding to the configured provider.
    pub fn new(config: LlmConfig) -> Result<Self, LlmError> {
        let provider = Provider::parse(&config.provider).ok_or_else(|| {
            LlmError::Config(format!(
                "unknown LLM provider '{}' (expected 'anthropic' or 'openrouter')",
                config.provider
            ))
        })?;

        let api_key = std::env::var(provider.env_var()).map_err(|_| {
            LlmError::Config(format!(
                "{} environment variable not set",
                provider.env_var()
            ))
        })?;

        let http = reqwest::Client::builder()
            .timeout(Duration::from_secs(config.timeout_seconds))
            .build()?;

        Ok(Self {
            http,
            config,
            provider,
            api_key,
        })
    }
}

#[async_trait::async_trait]
impl LlmProvider for LlmClient {
    /// Generate a report for the given type and context.
    #[instrument(name = "llm.generate", skip(self, context, project_root), fields(report_type = ?report_type))]
    async fn generate(
        &self,
        report_type: ReportType,
        context: &serde_json::Value,
        project_root: &Path,
    ) -> Result<LlmResponse, LlmError> {
        let prompts_dir = project_root.join(&self.config.prompts_dir);

        // Read prompts from disk (hot-swappable)
        let identity = std::fs::read_to_string(prompts_dir.join("identity.md"))
            .map_err(|e| LlmError::Config(format!("failed to read identity.md: {e}")))?;
        let system_prompt = std::fs::read_to_string(prompts_dir.join("system.md"))
            .map_err(|e| LlmError::Config(format!("failed to read system.md: {e}")))?;

        // Read schema from disk
        let schema_path = prompts_dir.join(report_type.schema_path());
        let schema_json = std::fs::read_to_string(&schema_path).map_err(|e| {
            LlmError::Config(format!(
                "failed to read schema {}: {e}",
                schema_path.display()
            ))
        })?;
        let schema: serde_json::Value = serde_json::from_str(&schema_json)?;

        // Build the tool from the schema file (Anthropic tool use format)
        let tool = Tool {
            name: schema["name"]
                .as_str()
                .unwrap_or(report_type.tool_name())
                .to_string(),
            description: schema["description"].as_str().unwrap_or("").to_string(),
            input_schema: schema["input_schema"].clone(),
        };

        // Assemble system prompt: identity + system concatenated
        let full_system = format!("{identity}\n\n---\n\n{system_prompt}");

        // User message is the context JSON
        let user_content = serde_json::to_string_pretty(context)?;

        let request = MessagesRequest {
            model: self.config.model.clone(),
            max_tokens: self.config.max_tokens,
            temperature: self.config.temperature,
            system: full_system,
            messages: vec![Message {
                role: "user".to_string(),
                content: user_content,
            }],
            tools: vec![tool],
            tool_choice: ToolChoice {
                choice_type: "tool".to_string(),
                name: report_type.tool_name().to_string(),
            },
        };

        let api_url = self.provider.api_url();
        let response = self.execute_with_retry(&request, api_url).await?;
        let output = Self::extract_tool_output(&response)?;

        Ok(LlmResponse {
            output,
            input_tokens: response.usage.input_tokens,
            output_tokens: response.usage.output_tokens,
            model: response.model,
        })
    }
}

impl LlmClient {
    /// Execute a request with retry on 429/5xx/timeout.
    async fn execute_with_retry(
        &self,
        request: &MessagesRequest,
        api_url: &str,
    ) -> Result<MessagesResponse, LlmError> {
        let max_retries = self.config.retry.max_retries;
        let base_delay = self.config.retry.base_delay_ms;
        let max_delay = self.config.retry.max_delay_ms;

        let mut last_error = None;

        for attempt in 0..=max_retries {
            if attempt > 0 {
                debug!(attempt, max_retries, "retrying LLM request");
            }

            let mut req = self
                .http
                .post(api_url)
                .header("content-type", "application/json")
                .json(request);

            req = match self.provider {
                Provider::Anthropic => req
                    .header("x-api-key", &self.api_key)
                    .header("anthropic-version", ANTHROPIC_VERSION),
                Provider::OpenRouter => {
                    req.header("Authorization", format!("Bearer {}", self.api_key))
                }
            };

            let result = req.send().await;

            match result {
                Ok(response) => {
                    let status = response.status().as_u16();

                    if status == 200 {
                        let body = response.text().await?;
                        let parsed: MessagesResponse = serde_json::from_str(&body)?;
                        return Ok(parsed);
                    }

                    let body = response.text().await.unwrap_or_default();

                    // 429 — rate limited, use retry-after if available
                    if status == 429 {
                        let retry_after_ms = parse_retry_after(&body)
                            .unwrap_or_else(|| compute_backoff(attempt, base_delay, max_delay));
                        warn!(
                            status,
                            retry_after_ms, attempt, "rate limited, sleeping before retry"
                        );
                        last_error = Some(LlmError::RateLimited { retry_after_ms });
                        if attempt < max_retries {
                            tokio::time::sleep(Duration::from_millis(retry_after_ms)).await;
                            continue;
                        }
                    }
                    // 5xx — server error, retry with backoff
                    else if status >= 500 {
                        let delay = compute_backoff(attempt, base_delay, max_delay);
                        let message =
                            parse_error_message(&body).unwrap_or_else(|| format!("HTTP {status}"));
                        warn!(
                            status,
                            delay_ms = delay,
                            attempt,
                            %message,
                            "server error, sleeping before retry"
                        );
                        last_error = Some(LlmError::Api {
                            status,
                            message: message.clone(),
                        });
                        if attempt < max_retries {
                            tokio::time::sleep(Duration::from_millis(delay)).await;
                            continue;
                        }
                    }
                    // 4xx (not 429) — client error, fail immediately
                    else {
                        let message =
                            parse_error_message(&body).unwrap_or_else(|| format!("HTTP {status}"));
                        return Err(LlmError::Api { status, message });
                    }
                }
                Err(e) => {
                    // Timeout or connection error — retry
                    let delay = compute_backoff(attempt, base_delay, max_delay);
                    warn!(
                        error = %e,
                        delay_ms = delay,
                        attempt,
                        "request failed, sleeping before retry"
                    );
                    last_error = Some(LlmError::Http(e));
                    if attempt < max_retries {
                        tokio::time::sleep(Duration::from_millis(delay)).await;
                        continue;
                    }
                }
            }
        }

        // All retries exhausted
        match last_error {
            Some(err) => Err(LlmError::RetriesExhausted {
                attempts: max_retries + 1,
                message: err.to_string(),
            }),
            None => Err(LlmError::RetriesExhausted {
                attempts: max_retries + 1,
                message: "unknown error".into(),
            }),
        }
    }

    /// Extract the tool use output from the response content blocks.
    fn extract_tool_output(response: &MessagesResponse) -> Result<serde_json::Value, LlmError> {
        for block in &response.content {
            if let ContentBlock::ToolUse { input, .. } = block {
                return Ok(input.clone());
            }
        }
        Err(LlmError::SchemaValidation(
            "no tool_use block in response".into(),
        ))
    }

    // --- Typed convenience methods ---

    pub async fn generate_morning(
        &self,
        context: &serde_json::Value,
        root: &Path,
    ) -> Result<MorningReport, LlmError> {
        let resp = self.generate(ReportType::Morning, context, root).await?;
        Ok(serde_json::from_value(resp.output)?)
    }

    pub async fn generate_midday(
        &self,
        context: &serde_json::Value,
        root: &Path,
    ) -> Result<MiddayReport, LlmError> {
        let resp = self.generate(ReportType::Midday, context, root).await?;
        Ok(serde_json::from_value(resp.output)?)
    }

    pub async fn generate_evening(
        &self,
        context: &serde_json::Value,
        root: &Path,
    ) -> Result<EveningReport, LlmError> {
        let resp = self.generate(ReportType::Evening, context, root).await?;
        Ok(serde_json::from_value(resp.output)?)
    }

    pub async fn generate_alert(
        &self,
        context: &serde_json::Value,
        root: &Path,
    ) -> Result<AlertReport, LlmError> {
        let resp = self.generate(ReportType::Alert, context, root).await?;
        Ok(serde_json::from_value(resp.output)?)
    }

    pub async fn generate_weekly(
        &self,
        context: &serde_json::Value,
        root: &Path,
    ) -> Result<WeeklyReport, LlmError> {
        let resp = self.generate(ReportType::Weekly, context, root).await?;
        Ok(serde_json::from_value(resp.output)?)
    }
}

/// Compute exponential backoff with jitter.
fn compute_backoff(attempt: u32, base_delay_ms: u64, max_delay_ms: u64) -> u64 {
    let exp_delay = base_delay_ms.saturating_mul(1u64.wrapping_shl(attempt));
    let capped = exp_delay.min(max_delay_ms);
    // Simple jitter: 50-100% of computed delay
    let jitter = capped / 2 + (capped / 2).wrapping_mul((attempt as u64 * 7 + 3) % 10) / 10;
    jitter.max(base_delay_ms)
}

/// Try to parse retry-after from Anthropic error response headers embedded in body.
fn parse_retry_after(body: &str) -> Option<u64> {
    // Anthropic may include retry_after in the error response
    let parsed: serde_json::Value = serde_json::from_str(body).ok()?;
    parsed
        .get("error")?
        .get("retry_after")?
        .as_f64()
        .map(|s| (s * 1000.0) as u64)
}

/// Extract human-readable error message from API error response.
fn parse_error_message(body: &str) -> Option<String> {
    let parsed: ApiErrorResponse = serde_json::from_str(body).ok()?;
    Some(parsed.error.message)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::llm::ReportType;

    #[test]
    fn report_type_from_str() {
        assert_eq!(ReportType::parse("morning"), Some(ReportType::Morning));
        assert_eq!(ReportType::parse("midday"), Some(ReportType::Midday));
        assert_eq!(ReportType::parse("evening"), Some(ReportType::Evening));
        assert_eq!(ReportType::parse("alert"), Some(ReportType::Alert));
        assert_eq!(ReportType::parse("weekly"), Some(ReportType::Weekly));
        assert_eq!(ReportType::parse("unknown"), None);
        assert_eq!(ReportType::parse("Morning"), None);
        assert_eq!(ReportType::parse(""), None);
    }

    #[test]
    fn report_type_tool_name() {
        assert_eq!(ReportType::Morning.tool_name(), "morning_report");
        assert_eq!(ReportType::Midday.tool_name(), "midday_report");
        assert_eq!(ReportType::Evening.tool_name(), "evening_report");
        assert_eq!(ReportType::Alert.tool_name(), "alert_report");
        assert_eq!(ReportType::Weekly.tool_name(), "weekly_report");
    }

    #[test]
    fn report_type_schema_path() {
        assert_eq!(ReportType::Morning.schema_path(), "schemas/morning.json");
        assert_eq!(ReportType::Midday.schema_path(), "schemas/midday.json");
        assert_eq!(ReportType::Evening.schema_path(), "schemas/evening.json");
        assert_eq!(ReportType::Alert.schema_path(), "schemas/alert.json");
        assert_eq!(ReportType::Weekly.schema_path(), "schemas/weekly.json");
    }

    #[test]
    fn extract_tool_output_success() {
        let fixture = load_fixture("anthropic_response.json");
        let response: MessagesResponse = serde_json::from_str(&fixture).unwrap();
        let output = LlmClient::extract_tool_output(&response).unwrap();
        assert_eq!(output["regime_status"], "range_bound");
        assert_eq!(output["assets"].as_array().unwrap().len(), 5);
    }

    #[test]
    fn extract_tool_output_missing() {
        let response = MessagesResponse {
            id: "msg_test".into(),
            content: vec![ContentBlock::Text {
                text: "no tool use".into(),
            }],
            model: "test".into(),
            stop_reason: "end_turn".into(),
            usage: crate::llm::api_types::Usage {
                input_tokens: 10,
                output_tokens: 5,
            },
        };
        let err = LlmClient::extract_tool_output(&response).unwrap_err();
        assert!(matches!(err, LlmError::SchemaValidation(_)));
    }

    #[test]
    fn compute_backoff_values() {
        // First attempt: base delay
        let b0 = compute_backoff(0, 1000, 30000);
        assert!(b0 >= 1000);

        // Second attempt: grows
        let b1 = compute_backoff(1, 1000, 30000);
        assert!(b1 >= 1000);

        // High attempt: capped at max
        let b10 = compute_backoff(10, 1000, 30000);
        assert!(b10 <= 30000);
    }

    #[test]
    fn parse_error_message_valid() {
        let body = r#"{"type":"error","error":{"type":"invalid_request_error","message":"max_tokens too high"}}"#;
        assert_eq!(
            parse_error_message(body),
            Some("max_tokens too high".into())
        );
    }

    #[test]
    fn parse_error_message_invalid_json() {
        assert_eq!(parse_error_message("not json"), None);
    }

    fn load_fixture(name: &str) -> String {
        let path = format!(
            "{}/tests/fixtures/llm/{name}",
            env!("CARGO_MANIFEST_DIR").trim_end_matches("/kairex")
        );
        std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("failed to read {path}: {e}"))
    }

    fn test_config() -> LlmConfig {
        LlmConfig {
            provider: "anthropic".into(),
            model: "claude-opus-4-20250514".into(),
            max_tokens: 8192,
            temperature: 0.3,
            timeout_seconds: 10,
            prompts_dir: "prompts".into(),
            retry: crate::config::LlmRetryConfig {
                max_retries: 2,
                base_delay_ms: 100,
                max_delay_ms: 1000,
            },
        }
    }

    // --- Integration tests using wiremock ---

    #[tokio::test]
    async fn generate_success() {
        let mock_server = wiremock::MockServer::start().await;

        let fixture = load_fixture("anthropic_response.json");

        wiremock::Mock::given(wiremock::matchers::method("POST"))
            .and(wiremock::matchers::path("/v1/messages"))
            .and(wiremock::matchers::header("x-api-key", "test-key"))
            .and(wiremock::matchers::header(
                "anthropic-version",
                ANTHROPIC_VERSION,
            ))
            .respond_with(wiremock::ResponseTemplate::new(200).set_body_string(&fixture))
            .expect(1)
            .mount(&mock_server)
            .await;

        let config = test_config();
        let client = LlmClient {
            http: reqwest::Client::new(),
            config,
            provider: Provider::Anthropic,
            api_key: "test-key".into(),
        };

        // Build a minimal request
        let request = MessagesRequest {
            model: "claude-opus-4-20250514".into(),
            max_tokens: 8192,
            temperature: 0.3,
            system: "test system".into(),
            messages: vec![Message {
                role: "user".into(),
                content: "{}".into(),
            }],
            tools: vec![Tool {
                name: "morning_report".into(),
                description: "test".into(),
                input_schema: serde_json::json!({"type": "object"}),
            }],
            tool_choice: ToolChoice {
                choice_type: "tool".into(),
                name: "morning_report".into(),
            },
        };

        let api_url = format!("{}/v1/messages", mock_server.uri());
        let response = client.execute_with_retry(&request, &api_url).await.unwrap();
        let output = LlmClient::extract_tool_output(&response).unwrap();

        // Verify it deserializes to MorningReport
        let report: MorningReport = serde_json::from_value(output).unwrap();
        assert_eq!(report.regime_status, "range_bound");
        assert_eq!(report.assets.len(), 5);
        assert_eq!(report.setups.len(), 2);
        assert_eq!(response.usage.input_tokens, 12500);
        assert_eq!(response.usage.output_tokens, 3200);
    }

    #[tokio::test]
    async fn generate_retries_on_429() {
        let mock_server = wiremock::MockServer::start().await;

        let fixture = load_fixture("anthropic_response.json");

        // First call returns 429, second returns 200
        wiremock::Mock::given(wiremock::matchers::method("POST"))
            .and(wiremock::matchers::path("/v1/messages"))
            .respond_with(wiremock::ResponseTemplate::new(429).set_body_string(
                r#"{"type":"error","error":{"type":"rate_limit_error","message":"rate limited"}}"#,
            ))
            .expect(1)
            .up_to_n_times(1)
            .mount(&mock_server)
            .await;

        wiremock::Mock::given(wiremock::matchers::method("POST"))
            .and(wiremock::matchers::path("/v1/messages"))
            .respond_with(wiremock::ResponseTemplate::new(200).set_body_string(&fixture))
            .expect(1)
            .mount(&mock_server)
            .await;

        let config = test_config();
        let client = LlmClient {
            http: reqwest::Client::new(),
            config,
            provider: Provider::Anthropic,
            api_key: "test-key".into(),
        };

        let request = MessagesRequest {
            model: "claude-opus-4-20250514".into(),
            max_tokens: 8192,
            temperature: 0.3,
            system: "test".into(),
            messages: vec![Message {
                role: "user".into(),
                content: "{}".into(),
            }],
            tools: vec![Tool {
                name: "morning_report".into(),
                description: "test".into(),
                input_schema: serde_json::json!({"type": "object"}),
            }],
            tool_choice: ToolChoice {
                choice_type: "tool".into(),
                name: "morning_report".into(),
            },
        };

        let api_url = format!("{}/v1/messages", mock_server.uri());
        let response = client.execute_with_retry(&request, &api_url).await.unwrap();
        assert_eq!(response.stop_reason, "tool_use");
    }

    #[tokio::test]
    async fn generate_fails_on_400() {
        let mock_server = wiremock::MockServer::start().await;

        wiremock::Mock::given(wiremock::matchers::method("POST"))
            .and(wiremock::matchers::path("/v1/messages"))
            .respond_with(wiremock::ResponseTemplate::new(400).set_body_string(
                r#"{"type":"error","error":{"type":"invalid_request_error","message":"bad request"}}"#,
            ))
            .expect(1)
            .mount(&mock_server)
            .await;

        let config = test_config();
        let client = LlmClient {
            http: reqwest::Client::new(),
            config,
            provider: Provider::Anthropic,
            api_key: "test-key".into(),
        };

        let request = MessagesRequest {
            model: "claude-opus-4-20250514".into(),
            max_tokens: 8192,
            temperature: 0.3,
            system: "test".into(),
            messages: vec![Message {
                role: "user".into(),
                content: "{}".into(),
            }],
            tools: vec![Tool {
                name: "morning_report".into(),
                description: "test".into(),
                input_schema: serde_json::json!({"type": "object"}),
            }],
            tool_choice: ToolChoice {
                choice_type: "tool".into(),
                name: "morning_report".into(),
            },
        };

        let api_url = format!("{}/v1/messages", mock_server.uri());
        let err = client
            .execute_with_retry(&request, &api_url)
            .await
            .unwrap_err();

        // Should fail immediately (no retry) with Api error
        match err {
            LlmError::Api { status, message } => {
                assert_eq!(status, 400);
                assert_eq!(message, "bad request");
            }
            other => panic!("expected LlmError::Api, got: {other:?}"),
        }
    }

    #[tokio::test]
    async fn generate_retries_on_500() {
        let mock_server = wiremock::MockServer::start().await;

        let fixture = load_fixture("anthropic_response.json");

        // First call returns 500, second returns 200
        wiremock::Mock::given(wiremock::matchers::method("POST"))
            .and(wiremock::matchers::path("/v1/messages"))
            .respond_with(wiremock::ResponseTemplate::new(500).set_body_string(
                r#"{"type":"error","error":{"type":"api_error","message":"internal server error"}}"#,
            ))
            .expect(1)
            .up_to_n_times(1)
            .mount(&mock_server)
            .await;

        wiremock::Mock::given(wiremock::matchers::method("POST"))
            .and(wiremock::matchers::path("/v1/messages"))
            .respond_with(wiremock::ResponseTemplate::new(200).set_body_string(&fixture))
            .expect(1)
            .mount(&mock_server)
            .await;

        let config = test_config();
        let client = LlmClient {
            http: reqwest::Client::new(),
            config,
            provider: Provider::Anthropic,
            api_key: "test-key".into(),
        };

        let request = MessagesRequest {
            model: "claude-opus-4-20250514".into(),
            max_tokens: 8192,
            temperature: 0.3,
            system: "test".into(),
            messages: vec![Message {
                role: "user".into(),
                content: "{}".into(),
            }],
            tools: vec![Tool {
                name: "morning_report".into(),
                description: "test".into(),
                input_schema: serde_json::json!({"type": "object"}),
            }],
            tool_choice: ToolChoice {
                choice_type: "tool".into(),
                name: "morning_report".into(),
            },
        };

        let api_url = format!("{}/v1/messages", mock_server.uri());
        let response = client.execute_with_retry(&request, &api_url).await.unwrap();
        assert_eq!(response.stop_reason, "tool_use");
    }

    #[tokio::test]
    async fn openrouter_sends_bearer_auth() {
        let mock_server = wiremock::MockServer::start().await;

        let fixture = load_fixture("anthropic_response.json");

        // Expect Bearer auth header (OpenRouter uses standard Authorization header)
        wiremock::Mock::given(wiremock::matchers::method("POST"))
            .and(wiremock::matchers::path("/v1/messages"))
            .and(wiremock::matchers::header(
                "Authorization",
                "Bearer or-test-key",
            ))
            .respond_with(wiremock::ResponseTemplate::new(200).set_body_string(&fixture))
            .expect(1)
            .mount(&mock_server)
            .await;

        let mut config = test_config();
        config.provider = "openrouter".into();
        let client = LlmClient {
            http: reqwest::Client::new(),
            config,
            provider: Provider::OpenRouter,
            api_key: "or-test-key".into(),
        };

        let request = MessagesRequest {
            model: "claude-opus-4-20250514".into(),
            max_tokens: 8192,
            temperature: 0.3,
            system: "test".into(),
            messages: vec![Message {
                role: "user".into(),
                content: "{}".into(),
            }],
            tools: vec![Tool {
                name: "morning_report".into(),
                description: "test".into(),
                input_schema: serde_json::json!({"type": "object"}),
            }],
            tool_choice: ToolChoice {
                choice_type: "tool".into(),
                name: "morning_report".into(),
            },
        };

        let api_url = format!("{}/v1/messages", mock_server.uri());
        let response = client.execute_with_retry(&request, &api_url).await.unwrap();
        assert_eq!(response.stop_reason, "tool_use");
    }
}
