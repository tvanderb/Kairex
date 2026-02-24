use serde::{Deserialize, Serialize};

// --- Request types ---

#[derive(Debug, Serialize)]
pub struct MessagesRequest {
    pub model: String,
    pub max_tokens: u32,
    pub temperature: f64,
    pub system: String,
    pub messages: Vec<Message>,
    pub tools: Vec<Tool>,
    pub tool_choice: ToolChoice,
}

#[derive(Debug, Serialize)]
pub struct Message {
    pub role: String,
    pub content: String,
}

#[derive(Debug, Serialize)]
pub struct Tool {
    pub name: String,
    pub description: String,
    pub input_schema: serde_json::Value,
}

#[derive(Debug, Serialize)]
pub struct ToolChoice {
    #[serde(rename = "type")]
    pub choice_type: String,
    pub name: String,
}

// --- Response types ---

#[derive(Debug, Deserialize)]
pub struct MessagesResponse {
    pub id: String,
    pub content: Vec<ContentBlock>,
    pub model: String,
    pub stop_reason: String,
    pub usage: Usage,
}

#[derive(Debug, Deserialize)]
#[serde(tag = "type")]
pub enum ContentBlock {
    #[serde(rename = "tool_use")]
    ToolUse {
        id: String,
        name: String,
        input: serde_json::Value,
    },
    #[serde(rename = "text")]
    Text { text: String },
}

#[derive(Debug, Deserialize)]
pub struct Usage {
    pub input_tokens: u32,
    pub output_tokens: u32,
}

// --- Error response ---

#[derive(Debug, Deserialize)]
pub struct ApiErrorResponse {
    #[serde(rename = "type")]
    pub error_type: String,
    pub error: ApiErrorDetail,
}

#[derive(Debug, Deserialize)]
pub struct ApiErrorDetail {
    #[serde(rename = "type")]
    pub error_type: String,
    pub message: String,
}

#[cfg(test)]
mod tests {
    use super::*;

    fn load_fixture(name: &str) -> String {
        let path = format!(
            "{}/tests/fixtures/llm/{name}",
            env!("CARGO_MANIFEST_DIR").trim_end_matches("/kairex")
        );
        std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("failed to read {path}: {e}"))
    }

    #[test]
    fn deserialize_success_response() {
        let json = load_fixture("anthropic_response.json");
        let response: MessagesResponse = serde_json::from_str(&json).unwrap();
        assert_eq!(response.id, "msg_01XFDUDYJgAACzvnptvVoYEL");
        assert_eq!(response.model, "claude-opus-4-20250514");
        assert_eq!(response.stop_reason, "tool_use");
        assert_eq!(response.usage.input_tokens, 12500);
        assert_eq!(response.usage.output_tokens, 3200);
        assert!(!response.content.is_empty());
    }

    #[test]
    fn deserialize_error_response() {
        let json = r#"{
            "type": "error",
            "error": {
                "type": "invalid_request_error",
                "message": "max_tokens: 8192 > 4096, which is the maximum"
            }
        }"#;
        let err: ApiErrorResponse = serde_json::from_str(json).unwrap();
        assert_eq!(err.error_type, "error");
        assert_eq!(err.error.error_type, "invalid_request_error");
        assert!(err.error.message.contains("max_tokens"));
    }

    #[test]
    fn extract_tool_use_from_response() {
        let json = load_fixture("anthropic_response.json");
        let response: MessagesResponse = serde_json::from_str(&json).unwrap();

        let tool_use = response.content.iter().find_map(|block| match block {
            ContentBlock::ToolUse { input, name, .. } => Some((name.as_str(), input)),
            _ => None,
        });

        let (name, input) = tool_use.expect("should have tool_use block");
        assert_eq!(name, "morning_report");
        assert!(input.get("regime_status").is_some());
        assert!(input.get("assets").is_some());
        assert!(input.get("setups").is_some());
    }

    #[test]
    fn extract_tool_use_missing() {
        let json = r#"{
            "id": "msg_text_only",
            "content": [{"type": "text", "text": "I cannot generate a report."}],
            "model": "claude-opus-4-20250514",
            "stop_reason": "end_turn",
            "usage": {"input_tokens": 100, "output_tokens": 20}
        }"#;
        let response: MessagesResponse = serde_json::from_str(json).unwrap();

        let tool_use = response.content.iter().find_map(|block| match block {
            ContentBlock::ToolUse { input, .. } => Some(input),
            _ => None,
        });

        assert!(tool_use.is_none());
    }
}
