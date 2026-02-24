use serde::{Deserialize, Serialize};
use tracing::{debug, instrument, warn};

use super::error::{DeliveryError, Result};

const DEFAULT_API_BASE: &str = "https://api.telegram.org";
const TELEGRAM_MAX_LENGTH: usize = 4096;

pub struct TelegramClient {
    http: reqwest::Client,
    bot_token: String,
    premium_channel_id: String,
    free_channel_id: String,
    operator_chat_id: String,
    api_base: String,
}

#[derive(Serialize)]
struct SendMessageRequest<'a> {
    chat_id: &'a str,
    text: &'a str,
    parse_mode: &'a str,
    disable_web_page_preview: bool,
}

#[derive(Deserialize)]
struct TelegramResponse {
    ok: bool,
    #[serde(default)]
    description: Option<String>,
}

impl TelegramClient {
    pub fn from_env() -> Result<Self> {
        Self::from_env_with_base(DEFAULT_API_BASE.to_string())
    }

    pub fn from_env_with_base(api_base: String) -> Result<Self> {
        let bot_token = read_env("TELEGRAM_BOT_TOKEN")?;
        let premium_channel_id = read_env("TELEGRAM_PREMIUM_CHANNEL_ID")?;
        let free_channel_id = read_env("TELEGRAM_FREE_CHANNEL_ID")?;
        let operator_chat_id = read_env("TELEGRAM_OPERATOR_CHAT_ID")?;

        Ok(Self {
            http: reqwest::Client::new(),
            bot_token,
            premium_channel_id,
            free_channel_id,
            operator_chat_id,
            api_base,
        })
    }

    pub async fn send_premium(&self, html: &str) -> Result<()> {
        self.send_message(&self.premium_channel_id, html).await
    }

    pub async fn send_free(&self, html: &str) -> Result<()> {
        self.send_message(&self.free_channel_id, html).await
    }

    pub async fn send_operator(&self, html: &str) -> Result<()> {
        self.send_message(&self.operator_chat_id, html).await
    }

    #[instrument(name = "delivery.telegram.send", skip(self, html), fields(chat_id = %chat_id))]
    async fn send_message(&self, chat_id: &str, html: &str) -> Result<()> {
        let fragments = split_message(html);
        debug!(
            chat_id,
            fragments = fragments.len(),
            total_len = html.len(),
            "sending Telegram message"
        );

        for fragment in &fragments {
            self.send_fragment(chat_id, fragment).await?;
        }
        Ok(())
    }

    async fn send_fragment(&self, chat_id: &str, text: &str) -> Result<()> {
        let url = format!("{}/bot{}/sendMessage", self.api_base, self.bot_token);

        let body = SendMessageRequest {
            chat_id,
            text,
            parse_mode: "HTML",
            disable_web_page_preview: true,
        };

        let resp = self.http.post(&url).json(&body).send().await?;
        let status = resp.status().as_u16();
        let telegram_resp: TelegramResponse = resp.json().await?;

        if !telegram_resp.ok {
            let description = telegram_resp
                .description
                .unwrap_or_else(|| "unknown error".to_string());
            warn!(status, %description, "Telegram API error");
            return Err(DeliveryError::TelegramApi {
                status,
                description,
            });
        }

        Ok(())
    }
}

/// Split a message into fragments that fit within the Telegram 4096-char limit.
///
/// Splits at paragraph boundaries (`\n\n`) first, then line boundaries (`\n`).
fn split_message(text: &str) -> Vec<&str> {
    if text.len() <= TELEGRAM_MAX_LENGTH {
        return vec![text];
    }

    let mut fragments = Vec::new();
    let mut remaining = text;

    while remaining.len() > TELEGRAM_MAX_LENGTH {
        let boundary = &remaining[..TELEGRAM_MAX_LENGTH];

        // Try paragraph break first
        let split_at = if let Some(pos) = boundary.rfind("\n\n") {
            pos
        } else if let Some(pos) = boundary.rfind('\n') {
            pos
        } else {
            // Hard split at limit as last resort
            TELEGRAM_MAX_LENGTH
        };

        let (fragment, rest) = remaining.split_at(split_at);
        fragments.push(fragment.trim_end());

        // Skip the delimiter
        remaining = rest.trim_start_matches('\n');
    }

    if !remaining.is_empty() {
        fragments.push(remaining);
    }

    fragments
}

fn read_env(key: &str) -> Result<String> {
    std::env::var(key).map_err(|_| DeliveryError::MissingEnvVar(key.to_string()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn split_short_message_unchanged() {
        let text = "Hello, world!";
        let fragments = split_message(text);
        assert_eq!(fragments, vec!["Hello, world!"]);
    }

    #[test]
    fn split_at_paragraph_boundary() {
        // Create a message just over 4096 chars with a paragraph break
        let para1 = "A".repeat(3000);
        let para2 = "B".repeat(2000);
        let text = format!("{}\n\n{}", para1, para2);

        let fragments = split_message(&text);
        assert_eq!(fragments.len(), 2);
        assert_eq!(fragments[0], para1.as_str());
        assert_eq!(fragments[1], para2.as_str());
    }

    #[test]
    fn split_at_line_boundary_when_no_paragraph_break() {
        let line1 = "A".repeat(3000);
        let line2 = "B".repeat(2000);
        let text = format!("{}\n{}", line1, line2);

        let fragments = split_message(&text);
        assert_eq!(fragments.len(), 2);
        assert_eq!(fragments[0], line1.as_str());
        assert_eq!(fragments[1], line2.as_str());
    }

    #[test]
    fn missing_env_var_returns_error() {
        // Ensure the var doesn't exist
        std::env::remove_var("TELEGRAM_BOT_TOKEN_TEST_NONEXISTENT");
        let result = read_env("TELEGRAM_BOT_TOKEN_TEST_NONEXISTENT");
        assert!(matches!(result, Err(DeliveryError::MissingEnvVar(_))));
    }

    #[tokio::test]
    async fn send_success() {
        let server = wiremock::MockServer::start().await;

        wiremock::Mock::given(wiremock::matchers::method("POST"))
            .and(wiremock::matchers::path_regex("/bot.*/sendMessage"))
            .respond_with(
                wiremock::ResponseTemplate::new(200).set_body_json(serde_json::json!({"ok": true})),
            )
            .expect(1)
            .mount(&server)
            .await;

        std::env::set_var("TELEGRAM_BOT_TOKEN", "test_token");
        std::env::set_var("TELEGRAM_PREMIUM_CHANNEL_ID", "@test_premium");
        std::env::set_var("TELEGRAM_FREE_CHANNEL_ID", "@test_free");
        std::env::set_var("TELEGRAM_OPERATOR_CHAT_ID", "12345");

        let client = TelegramClient::from_env_with_base(server.uri()).unwrap();
        let result = client.send_premium("<b>Test</b>").await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn send_api_error() {
        let server = wiremock::MockServer::start().await;

        wiremock::Mock::given(wiremock::matchers::method("POST"))
            .and(wiremock::matchers::path_regex("/bot.*/sendMessage"))
            .respond_with(wiremock::ResponseTemplate::new(400).set_body_json(
                serde_json::json!({"ok": false, "description": "Bad Request: chat not found"}),
            ))
            .expect(1)
            .mount(&server)
            .await;

        std::env::set_var("TELEGRAM_BOT_TOKEN", "test_token");
        std::env::set_var("TELEGRAM_PREMIUM_CHANNEL_ID", "@test_premium");
        std::env::set_var("TELEGRAM_FREE_CHANNEL_ID", "@test_free");
        std::env::set_var("TELEGRAM_OPERATOR_CHAT_ID", "12345");

        let client = TelegramClient::from_env_with_base(server.uri()).unwrap();
        let result = client.send_premium("<b>Test</b>").await;

        match result {
            Err(DeliveryError::TelegramApi {
                status,
                description,
            }) => {
                assert_eq!(status, 400);
                assert!(description.contains("chat not found"));
            }
            other => panic!("expected TelegramApi error, got: {other:?}"),
        }
    }

    #[tokio::test]
    async fn long_message_splits_into_multiple_sends() {
        let server = wiremock::MockServer::start().await;

        wiremock::Mock::given(wiremock::matchers::method("POST"))
            .and(wiremock::matchers::path_regex("/bot.*/sendMessage"))
            .respond_with(
                wiremock::ResponseTemplate::new(200).set_body_json(serde_json::json!({"ok": true})),
            )
            .expect(2)
            .mount(&server)
            .await;

        std::env::set_var("TELEGRAM_BOT_TOKEN", "test_token");
        std::env::set_var("TELEGRAM_PREMIUM_CHANNEL_ID", "@test_premium");
        std::env::set_var("TELEGRAM_FREE_CHANNEL_ID", "@test_free");
        std::env::set_var("TELEGRAM_OPERATOR_CHAT_ID", "12345");

        let client = TelegramClient::from_env_with_base(server.uri()).unwrap();

        // Build a 5000+ char message with paragraph breaks
        let para1 = "A".repeat(3000);
        let para2 = "B".repeat(2000);
        let message = format!("{}\n\n{}", para1, para2);

        let result = client.send_premium(&message).await;
        assert!(result.is_ok());
        // wiremock .expect(2) verifies exactly 2 sendMessage calls
    }
}
