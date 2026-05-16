use anyhow::{Result, anyhow};
use serde::{Deserialize, Serialize};

#[derive(Clone)]
pub struct OpenAiClient {
    pub api_key: String,
    pub base_url: String,
    pub model: String,
    pub max_tokens: u32,
    pub temperature: f64,
    client: reqwest::Client,
}

impl OpenAiClient {
    pub fn new(api_key: String, base_url: String, model: String) -> Self {
        Self { api_key, base_url, model, max_tokens: 16000, temperature: 0.2, client: reqwest::Client::new() }
    }
    pub fn with_model(mut self, model: &str) -> Self { self.model = model.into(); self }
    pub fn with_max_tokens(mut self, n: u32) -> Self { self.max_tokens = n; self }

    pub async fn chat(&self, system: &str, messages: &[ChatMessage]) -> Result<ChatResponse> {
        let url = format!("{}/chat/completions", self.base_url.trim_end_matches('/'));
        let mut all_msgs = vec![ChatMessage { role: "system".into(), content: system.to_string() }];
        all_msgs.extend_from_slice(messages);
        let body = OpenAiRequest { model: self.model.clone(), messages: all_msgs, max_tokens: self.max_tokens, temperature: self.temperature };
        let resp = self.client.post(&url)
            .header("Authorization", format!("Bearer {}", self.api_key))
            .header("Content-Type", "application/json")
            .json(&body).send().await?;
        if !resp.status().is_success() {
            let status = resp.status();
            let text = resp.text().await.unwrap_or_default();
            return Err(anyhow!("API error {status}: {text}"));
        }
        let response: ChatResponse = resp.json().await?;
        Ok(response)
    }

    pub fn user_message(content: &str) -> ChatMessage { ChatMessage { role: "user".into(), content: content.to_string() } }
    pub fn assistant_message(content: &str) -> ChatMessage { ChatMessage { role: "assistant".into(), content: content.to_string() } }

    pub fn extract_text(response: &ChatResponse) -> String {
        response.choices.iter().filter_map(|c| c.message.as_ref()).map(|m| m.content.clone()).collect::<Vec<_>>().join("\n")
    }
}

#[derive(Debug, Serialize)] struct OpenAiRequest { model: String, messages: Vec<ChatMessage>, max_tokens: u32, temperature: f64 }

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChatMessage { pub role: String, pub content: String }

#[derive(Debug, Deserialize)]
pub struct ChatResponse { pub choices: Vec<Choice>, pub usage: Option<Usage> }

#[derive(Debug, Deserialize)]
pub struct Choice { pub message: Option<ResponseMessage>, pub finish_reason: Option<String> }

#[derive(Debug, Deserialize)]
pub struct ResponseMessage { pub content: String }

#[derive(Debug, Deserialize)]
pub struct Usage { pub prompt_tokens: u32, pub completion_tokens: u32, pub total_tokens: u32 }
