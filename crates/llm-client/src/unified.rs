use anyhow::Result;
use crate::anthropic::AnthropicClient;
use crate::openai::OpenAiClient;

#[derive(Clone)]
pub enum LlmClient {
    Anthropic(AnthropicClient),
    OpenAI(OpenAiClient),
}

impl LlmClient {
    pub fn anthropic(api_key: String) -> Self { Self::Anthropic(AnthropicClient::new(api_key)) }
    pub fn openai(api_key: String, base_url: String, model: String) -> Self {
        Self::OpenAI(OpenAiClient::new(api_key, base_url, model))
    }
    pub fn with_model(self, model: &str) -> Self {
        match self { Self::Anthropic(c) => Self::Anthropic(c.with_model(model)), Self::OpenAI(c) => Self::OpenAI(c.with_model(model)) }
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
}
