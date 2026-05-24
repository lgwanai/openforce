use anyhow::{Result, anyhow};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use crate::tool::{Tool, ToolCall, ToolChatResponse, ToolResult};

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

    /// Simple chat without tools (backward compatible).
    pub async fn chat(&self, system: &str, messages: &[ChatMessage]) -> Result<ChatResponse> {
        let url = format!("{}/chat/completions", self.base_url.trim_end_matches('/'));
        let mut all_msgs = vec![ChatMessage { role: "system".into(), content: system.to_string(), tool_calls: None, tool_call_id: None }];
        all_msgs.extend_from_slice(messages);
        let body = OpenAiRequest {
            model: self.model.clone(), messages: all_msgs, max_tokens: self.max_tokens,
            temperature: self.temperature, tools: None, tool_choice: None,
        };
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

    /// Chat with native tool calling support.
    pub async fn chat_with_tools(
        &self, system: &str, messages: &[ChatMessage], tools: &[Tool],
    ) -> Result<ToolChatResponse> {
        let url = format!("{}/chat/completions", self.base_url.trim_end_matches('/'));
        let mut all_msgs = vec![ChatMessage {
            role: "system".into(), content: system.to_string(),
            tool_calls: None, tool_call_id: None,
        }];
        all_msgs.extend_from_slice(messages);

        let openai_tools: Vec<Value> = tools.iter().map(|t| {
            serde_json::json!({
                "type": "function",
                "function": {
                    "name": t.name,
                    "description": t.description,
                    "parameters": t.parameters
                }
            })
        }).collect();

        let body = OpenAiRequest {
            model: self.model.clone(), messages: all_msgs, max_tokens: self.max_tokens,
            temperature: self.temperature,
            tools: Some(openai_tools), tool_choice: Some(serde_json::Value::String("auto".into())),
        };

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
        let choice = response.choices.into_iter().next()
            .ok_or_else(|| anyhow!("No choices in response"))?;

        let text = choice.message.as_ref().map(|m| m.content.clone()).unwrap_or_default();
        let finish = choice.finish_reason.clone().unwrap_or_default();
        let tokens = response.usage.as_ref().map_or(0, |u| u.total_tokens);

        let tool_calls: Vec<ToolCall> = choice.message.as_ref()
            .and_then(|m| m.tool_calls.as_ref())
            .map(|tc| tc.iter().map(|c| ToolCall {
                id: c.id.clone().unwrap_or_default(),
                name: c.function.name.clone(),
                arguments: c.function.arguments.clone(),
            }).collect())
            .unwrap_or_default();

        Ok(ToolChatResponse { text, tool_calls, finish_reason: finish, tokens_used: tokens, stop_reason: None })
    }

    pub fn user_message(content: &str) -> ChatMessage {
        ChatMessage { role: "user".into(), content: content.to_string(), tool_calls: None, tool_call_id: None }
    }

    pub fn assistant_message(content: &str) -> ChatMessage {
        ChatMessage { role: "assistant".into(), content: content.to_string(), tool_calls: None, tool_call_id: None }
    }

    /// Build an assistant message that includes tool calls.
    pub fn assistant_tool_call_message(calls: &[ToolCall]) -> ChatMessage {
        let openai_calls: Vec<OpenAiToolCall> = calls.iter().map(|c| {
            OpenAiToolCall {
                id: Some(c.id.clone()), call_type: "function".into(),
                function: OpenAiFunctionCall { name: c.name.clone(), arguments: c.arguments.clone() },
            }
        }).collect();
        ChatMessage {
            role: "assistant".into(), content: String::new(),
            tool_calls: Some(openai_calls), tool_call_id: None,
        }
    }

    /// Build a tool result message for the conversation.
    pub fn tool_result_message(call_id: &str, content: &str) -> ChatMessage {
        ChatMessage {
            role: "tool".into(), content: content.to_string(),
            tool_calls: None, tool_call_id: Some(call_id.into()),
        }
    }

    /// Build multiple tool result messages at once.
    pub fn tool_result_messages(results: &[ToolResult]) -> Vec<ChatMessage> {
        results.iter().map(|r| {
            let label = if r.success { "" } else { "[ERROR] " };
            Self::tool_result_message(&r.call_id, &format!("{label}{}", r.content))
        }).collect()
    }

    pub fn extract_text(response: &ChatResponse) -> String {
        response.choices.iter()
            .filter_map(|c| c.message.as_ref())
            .map(|m| m.content.clone()).collect::<Vec<_>>().join("\n")
    }
}

#[derive(Debug, Serialize)]
struct OpenAiRequest {
    model: String, messages: Vec<ChatMessage>, max_tokens: u32, temperature: f64,
    #[serde(skip_serializing_if = "Option::is_none")] tools: Option<Vec<Value>>,
    #[serde(skip_serializing_if = "Option::is_none")] tool_choice: Option<Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChatMessage {
    pub role: String, pub content: String,
    #[serde(skip_serializing_if = "Option::is_none")] pub tool_calls: Option<Vec<OpenAiToolCall>>,
    #[serde(skip_serializing_if = "Option::is_none")] pub tool_call_id: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OpenAiToolCall {
    #[serde(skip_serializing_if = "Option::is_none")] pub id: Option<String>,
    #[serde(rename = "type")] pub call_type: String,
    pub function: OpenAiFunctionCall,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OpenAiFunctionCall { pub name: String, pub arguments: String }

#[derive(Debug, Deserialize)]
pub struct ChatResponse { pub choices: Vec<Choice>, pub usage: Option<Usage> }

#[derive(Debug, Deserialize)]
pub struct Choice { pub message: Option<ResponseMessage>, pub finish_reason: Option<String> }

#[derive(Debug, Deserialize)]
pub struct ResponseMessage {
    #[serde(default)] pub content: String,
    #[serde(default)] pub tool_calls: Option<Vec<OpenAiToolCall>>,
}

#[derive(Debug, Deserialize)]
pub struct Usage { pub prompt_tokens: u32, pub completion_tokens: u32, pub total_tokens: u32 }
