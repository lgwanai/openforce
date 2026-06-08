use crate::anthropic::AnthropicClient;
use crate::openai::OpenAiClient;
use crate::tool::{Tool, ToolChatResponse, ToolResult};
use anyhow::Result;

#[derive(Clone)]
pub enum LlmClient {
    Anthropic(AnthropicClient),
    OpenAI(OpenAiClient),
}

impl LlmClient {
    pub fn anthropic(api_key: String, base_url: Option<String>) -> Self {
        Self::Anthropic(AnthropicClient::new(api_key, base_url))
    }
    pub fn openai(api_key: String, base_url: String, model: String) -> Self {
        Self::OpenAI(OpenAiClient::new(api_key, base_url, model))
    }
    pub fn with_model(self, model: &str) -> Self {
        match self {
            Self::Anthropic(c) => Self::Anthropic(c.with_model(model)),
            Self::OpenAI(c) => Self::OpenAI(c.with_model(model)),
        }
    }
    pub fn with_max_tokens(self, n: u32) -> Self {
        match self {
            Self::Anthropic(c) => Self::Anthropic(c.with_max_tokens(n)),
            Self::OpenAI(c) => Self::OpenAI(c.with_max_tokens(n)),
        }
    }

    pub async fn chat(&self, system: &str, user_message: &str) -> Result<(String, u32)> {
        match self {
            Self::Anthropic(c) => {
                let messages = vec![AnthropicClient::user_message(user_message)];
                let resp = c.chat(system, &messages).await?;
                let tokens = resp.usage.as_ref().map_or(0, |u| u.output_tokens);
                Ok((AnthropicClient::extract_text(&resp), tokens))
            }
            Self::OpenAI(c) => {
                let messages = vec![OpenAiClient::user_message(user_message)];
                let resp = c.chat(system, &messages).await?;
                let tokens = resp.usage.as_ref().map_or(0, |u| u.completion_tokens);
                Ok((OpenAiClient::extract_text(&resp), tokens))
            }
        }
    }

    pub async fn chat_with_tools(
        &self,
        system: &str,
        messages: &[ToolMessage],
        tools: &[Tool],
    ) -> Result<ToolChatResponse> {
        match self {
            Self::Anthropic(c) => {
                c.chat_with_tools(system, &convert_anthropic(messages), tools)
                    .await
            }
            Self::OpenAI(c) => {
                c.chat_with_tools(system, &convert_openai(messages), tools)
                    .await
            }
        }
    }

    pub fn build_tool_results(
        &self,
        _calls: &[crate::tool::ToolCall],
        results: &[ToolResult],
    ) -> Vec<ToolMessage> {
        vec![ToolMessage::tool_results(results)]
    }

    pub fn is_anthropic(&self) -> bool {
        matches!(self, Self::Anthropic(_))
    }
    pub fn is_openai(&self) -> bool {
        matches!(self, Self::OpenAI(_))
    }
}

/// Unified tool-calling message with role tracking for multi-turn conversations.
#[derive(Debug, Clone)]
pub struct ToolMessage {
    pub role: String,
    pub content: String,
    pub tool_calls: Option<Vec<crate::tool::ToolCall>>,
    pub tool_results: Option<Vec<crate::tool::ToolResult>>,
    pub tool_call_id: Option<String>,
}

impl ToolMessage {
    pub fn user(content: &str) -> Self {
        Self {
            role: "user".into(),
            content: content.into(),
            tool_calls: None,
            tool_results: None,
            tool_call_id: None,
        }
    }

    pub fn assistant_with_tools(content: &str, calls: &[crate::tool::ToolCall]) -> Self {
        Self {
            role: "assistant".into(),
            content: content.into(),
            tool_calls: Some(calls.to_vec()),
            tool_results: None,
            tool_call_id: None,
        }
    }

    pub fn tool_results(results: &[crate::tool::ToolResult]) -> Self {
        Self {
            role: "user".into(),
            content: String::new(),
            tool_calls: None,
            tool_results: Some(results.to_vec()),
            tool_call_id: None,
        }
    }
}

fn convert_anthropic(msgs: &[ToolMessage]) -> Vec<crate::anthropic::ChatMessage> {
    let mut out = Vec::new();
    for m in msgs {
        match m.role.as_str() {
            "user" => {
                if let Some(ref results) = m.tool_results {
                    out.extend(AnthropicClient::tool_result_messages(results));
                } else {
                    out.push(AnthropicClient::user_message(&m.content));
                }
            }
            "assistant" => {
                if let Some(ref calls) = m.tool_calls {
                    out.push(AnthropicClient::assistant_tool_call_message(calls));
                } else {
                    out.push(AnthropicClient::assistant_message(&m.content));
                }
            }
            _ => out.push(AnthropicClient::user_message(&m.content)),
        }
    }
    out
}

fn convert_openai(msgs: &[ToolMessage]) -> Vec<crate::openai::ChatMessage> {
    let mut out = Vec::new();
    for m in msgs {
        match m.role.as_str() {
            "user" => {
                if let Some(ref results) = m.tool_results {
                    out.extend(OpenAiClient::tool_result_messages(results));
                } else {
                    out.push(OpenAiClient::user_message(&m.content));
                }
            }
            "assistant" => {
                if let Some(ref calls) = m.tool_calls {
                    out.push(OpenAiClient::assistant_tool_call_message(calls));
                } else {
                    out.push(OpenAiClient::assistant_message(&m.content));
                }
            }
            _ => out.push(OpenAiClient::user_message(&m.content)),
        }
    }
    out
}
