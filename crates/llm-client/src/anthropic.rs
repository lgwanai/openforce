use serde::{Deserialize, Serialize};
use anyhow::{Result, anyhow};

const ANTHROPIC_API_URL: &str = "https://api.anthropic.com/v1/messages";

#[derive(Clone)]
pub struct AnthropicClient {
    pub api_key: String,
    pub model: String,
    pub max_tokens: u32,
    client: reqwest::Client,
}

impl AnthropicClient {
    pub fn new(api_key: String) -> Self {
        Self {
            api_key,
            model: "claude-sonnet-4-6".into(),
            max_tokens: 16000,
            client: reqwest::Client::new(),
        }
    }

    pub fn with_model(mut self, model: &str) -> Self {
        self.model = model.into();
        self
    }

    pub async fn chat(&self, system: &str, messages: &[ChatMessage]) -> Result<ChatResponse> {
        let body = AnthropicRequest {
            model: self.model.clone(),
            max_tokens: self.max_tokens,
            system: system.to_string(),
            messages: messages.to_vec(),
        };

        let resp = self.client
            .post(ANTHROPIC_API_URL)
            .header("x-api-key", &self.api_key)
            .header("anthropic-version", "2023-06-01")
            .header("content-type", "application/json")
            .json(&body)
            .send()
            .await?;

        if !resp.status().is_success() {
            let status = resp.status();
            let text = resp.text().await.unwrap_or_default();
            return Err(anyhow!("Anthropic API error {}: {}", status, text));
        }

        let response: ChatResponse = resp.json().await?;
        Ok(response)
    }

    pub fn user_message(content: &str) -> ChatMessage {
        ChatMessage {
            role: "user".into(),
            content: vec![ContentBlock {
                content_type: "text".into(),
                text: content.to_string(),
            }],
        }
    }

    pub fn assistant_message(content: &str) -> ChatMessage {
        ChatMessage {
            role: "assistant".into(),
            content: vec![ContentBlock {
                content_type: "text".into(),
                text: content.to_string(),
            }],
        }
    }

    pub fn extract_text(response: &ChatResponse) -> String {
        response.content.iter()
            .filter_map(|b| {
                if b.content_type == "text" { Some(b.text.clone()) }
                else { None }
            })
            .collect::<Vec<_>>()
            .join("\n")
    }
}

#[derive(Debug, Serialize)]
struct AnthropicRequest {
    model: String,
    max_tokens: u32,
    system: String,
    messages: Vec<ChatMessage>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChatMessage {
    pub role: String,
    pub content: Vec<ContentBlock>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ContentBlock {
    #[serde(rename = "type")]
    pub content_type: String,
    pub text: String,
}

#[derive(Debug, Deserialize)]
pub struct ChatResponse {
    pub id: String,
    pub content: Vec<ResponseContent>,
    pub role: String,
    pub model: String,
    pub stop_reason: Option<String>,
    pub usage: Option<Usage>,
}

#[derive(Debug, Deserialize)]
pub struct ResponseContent {
    #[serde(rename = "type")]
    pub content_type: String,
    pub text: String,
}

#[derive(Debug, Deserialize)]
pub struct Usage {
    pub input_tokens: u32,
    pub output_tokens: u32,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_anthropic_chat() {
        let api_key = std::env::var("ANTHROPIC_API_KEY").unwrap_or_default();
        if api_key.is_empty() {
            eprintln!("SKIP: ANTHROPIC_API_KEY not set");
            return;
        }
        let client = AnthropicClient::new(api_key);
        let messages = vec![
            AnthropicClient::user_message("Say 'hello' in exactly one word, no punctuation."),
        ];
        let resp = client.chat("You are a helpful assistant.", &messages).await.unwrap();
        let text = AnthropicClient::extract_text(&resp);
        eprintln!("LLM response: {}", text);
        assert!(!text.is_empty());
    }
}
