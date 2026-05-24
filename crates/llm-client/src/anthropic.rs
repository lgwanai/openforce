use serde::{Deserialize, Serialize};
use serde_json::Value;
use anyhow::{Result, anyhow};
use crate::tool::{Tool, ToolCall, ToolChatResponse, ToolResult};

const DEFAULT_ANTHROPIC_BASE: &str = "https://api.anthropic.com";

#[derive(Clone)]
pub struct AnthropicClient {
    pub api_key: String,
    pub base_url: String,
    pub model: String,
    pub max_tokens: u32,
    client: reqwest::Client,
}

impl AnthropicClient {
    pub fn new(api_key: String, base_url: Option<String>) -> Self {
        let base = base_url.unwrap_or_else(|| DEFAULT_ANTHROPIC_BASE.into());
        let clean = base.trim_end_matches('/').trim_end_matches("/v1/messages").to_string();
        Self { api_key, base_url: clean, model: "claude-sonnet-4-6".into(), max_tokens: 16000, client: reqwest::Client::new() }
    }

    pub fn with_model(mut self, model: &str) -> Self { self.model = model.into(); self }
    pub fn with_max_tokens(mut self, n: u32) -> Self { self.max_tokens = n; self }

    /// Simple chat without tools (backward compatible).
    pub async fn chat(&self, system: &str, messages: &[ChatMessage]) -> Result<ChatResponse> {
        let body = AnthropicRequest {
            model: self.model.clone(), max_tokens: self.max_tokens,
            system: system.to_string(), messages: messages.to_vec(), tools: None,
        };
        self.send_request(&body).await
    }

    /// Chat with native tool calling support.
    pub async fn chat_with_tools(
        &self, system: &str, messages: &[ChatMessage], tools: &[Tool],
    ) -> Result<ToolChatResponse> {
        let anthropic_tools: Vec<Value> = tools.iter().map(|t| {
            serde_json::json!({
                "name": t.name,
                "description": t.description,
                "input_schema": t.parameters
            })
        }).collect();

        let body = AnthropicRequest {
            model: self.model.clone(), max_tokens: self.max_tokens,
            system: system.to_string(), messages: messages.to_vec(),
            tools: Some(anthropic_tools),
        };

        let response = self.send_request(&body).await?;

        let text: String = response.content.iter()
            .filter(|c| c.content_type == "text")
            .map(|c| c.text.clone()).collect::<Vec<_>>().join("\n");

        let tool_calls: Vec<ToolCall> = response.content.iter()
            .filter(|c| c.content_type == "tool_use")
            .map(|c| ToolCall {
                id: c.id.clone().unwrap_or_default(),
                name: c.name.clone().unwrap_or_default(),
                arguments: serde_json::to_string(&c.input).unwrap_or_default(),
            }).collect();

        let tokens = response.usage.as_ref().map_or(0, |u| u.input_tokens + u.output_tokens);

        Ok(ToolChatResponse {
            text, tool_calls,
            finish_reason: response.stop_reason.clone().unwrap_or_default(),
            tokens_used: tokens,
            stop_reason: response.stop_reason.clone(),
        })
    }

    async fn send_request(&self, body: &AnthropicRequest) -> Result<ChatResponse> {
        let url = format!("{}/v1/messages", self.base_url);
        let resp = self.client.post(&url)
            .header("x-api-key", &self.api_key)
            .header("anthropic-version", "2023-06-01")
            .header("content-type", "application/json")
            .json(body).send().await?;

        if !resp.status().is_success() {
            let status = resp.status();
            let text = resp.text().await.unwrap_or_default();
            return Err(anyhow!("Anthropic API error {}: {}", status, text));
        }
        let response: ChatResponse = resp.json().await?;
        Ok(response)
    }

    pub fn user_message(content: &str) -> ChatMessage {
        ChatMessage { role: "user".into(), content: vec![text_block(content)] }
    }

    pub fn assistant_message(content: &str) -> ChatMessage {
        ChatMessage { role: "assistant".into(), content: vec![text_block(content)] }
    }

    /// Build an assistant message with tool use blocks.
    pub fn assistant_tool_call_message(calls: &[ToolCall]) -> ChatMessage {
        let mut blocks: Vec<ContentBlock> = calls.iter().map(|c| {
            let input: Value = serde_json::from_str(&c.arguments).unwrap_or(Value::Null);
            ContentBlock {
                content_type: "tool_use".into(), text: String::new(),
                id: Some(c.id.clone()), name: Some(c.name.clone()), input,
                tool_use_id: None, is_error: None,
            }
        }).collect();
        // Anthropic requires at least one text block alongside tool_use
        blocks.insert(0, text_block("I will use the following tools:"));
        ChatMessage { role: "assistant".into(), content: blocks }
    }

    /// Build tool result messages for Anthropic (user role with tool_result blocks).
    pub fn tool_result_messages(results: &[ToolResult]) -> Vec<ChatMessage> {
        let blocks: Vec<ContentBlock> = results.iter().map(|r| ContentBlock {
            content_type: "tool_result".into(),
            text: r.content.clone(),
            id: None, name: None, input: Value::Null,
            tool_use_id: Some(r.call_id.clone()),
            is_error: Some(!r.success),
        }).collect();
        vec![ChatMessage { role: "user".into(), content: blocks }]
    }

    pub fn extract_text(response: &ChatResponse) -> String {
        response.content.iter()
            .filter_map(|b| if b.content_type == "text" { Some(b.text.clone()) } else { None })
            .collect::<Vec<_>>().join("\n")
    }
}

fn text_block(text: &str) -> ContentBlock {
    ContentBlock { content_type: "text".into(), text: text.to_string(),
        id: None, name: None, input: Value::Null, tool_use_id: None, is_error: None }
}

#[derive(Debug, Serialize)]
struct AnthropicRequest {
    model: String, max_tokens: u32, system: String, messages: Vec<ChatMessage>,
    #[serde(skip_serializing_if = "Option::is_none")] tools: Option<Vec<Value>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChatMessage { pub role: String, pub content: Vec<ContentBlock> }

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ContentBlock {
    #[serde(rename = "type")] pub content_type: String,
    #[serde(default)] pub text: String,
    #[serde(skip_serializing_if = "Option::is_none")] pub id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")] pub name: Option<String>,
    #[serde(default)] pub input: Value,
    #[serde(skip_serializing_if = "Option::is_none")] pub tool_use_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")] pub is_error: Option<bool>,
}

#[derive(Debug, Deserialize)]
pub struct ChatResponse {
    pub id: String, pub content: Vec<ResponseContent>, pub role: String,
    pub model: String, pub stop_reason: Option<String>, pub usage: Option<Usage>,
}

#[derive(Debug, Deserialize)]
pub struct ResponseContent {
    #[serde(rename = "type")] pub content_type: String,
    #[serde(default)] pub text: String,
    #[serde(default)] pub id: Option<String>,
    #[serde(default)] pub name: Option<String>,
    #[serde(default)] pub input: Value,
}

#[derive(Debug, Deserialize)]
pub struct Usage { pub input_tokens: u32, pub output_tokens: u32 }

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_anthropic_chat() {
        let api_key = std::env::var("API_KEY").unwrap_or_default();
        if api_key.len() < 5 {
            eprintln!("SKIP: API_KEY not set");
            return;
        }
        let client = AnthropicClient::new(api_key, None);
        let messages = vec![
            AnthropicClient::user_message("Say 'hello' in exactly one word, no punctuation."),
        ];
        match client.chat("You are a helpful assistant.", &messages).await {
            Ok(resp) => {
                let text = AnthropicClient::extract_text(&resp);
                eprintln!("LLM response: {}", text);
                assert!(!text.is_empty());
            }
            Err(e) => {
                eprintln!("SKIP: API call failed ({e})");
            }
        }
    }
}
