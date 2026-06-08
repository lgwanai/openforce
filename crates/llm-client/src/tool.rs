use serde::{Deserialize, Serialize};
use serde_json::Value;

/// Tool definition compatible with OpenAI/Anthropic function calling APIs.
#[derive(Debug, Clone, Serialize)]
pub struct Tool {
    pub name: String,
    pub description: String,
    pub parameters: Value, // JSON Schema for the tool's input
}

/// A tool call made by the LLM that needs to be executed.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct ToolCall {
    /// Unique identifier for this specific call instance.
    pub id: String,
    /// The tool name the LLM wants to invoke.
    pub name: String,
    /// JSON-encoded arguments for the tool.
    pub arguments: String,
}

/// Result of executing a tool call, to be sent back to the LLM.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolResult {
    /// Must match the ToolCall.id this result is for.
    pub call_id: String,
    /// The tool's output as a string.
    pub content: String,
    /// Whether the tool execution was successful.
    #[serde(default = "default_success")]
    pub success: bool,
}

fn default_success() -> bool {
    true
}

/// Structured response from a tool-capable chat call.
#[derive(Debug, Clone)]
pub struct ToolChatResponse {
    /// Text content from the LLM (may be empty if only tool calls).
    pub text: String,
    /// Tool calls the LLM requested (empty if it's a final text response).
    pub tool_calls: Vec<ToolCall>,
    /// Finish reason from the API.
    pub finish_reason: String,
    /// Token usage from this response.
    pub tokens_used: u32,
    /// Raw stop reason for additional decision-making.
    pub stop_reason: Option<String>,
}

impl Tool {
    pub fn new(name: &str, description: &str, parameters: Value) -> Self {
        Self {
            name: name.into(),
            description: description.into(),
            parameters,
        }
    }

    /// Build a simple tool with a single required string parameter.
    pub fn simple(name: &str, description: &str, param_name: &str, param_desc: &str) -> Self {
        let parameters = serde_json::json!({
            "type": "object",
            "properties": {
                param_name: {
                    "type": "string",
                    "description": param_desc
                }
            },
            "required": [param_name]
        });
        Self::new(name, description, parameters)
    }

    /// Build a tool with only description (no parameters).
    pub fn bare(name: &str, description: &str) -> Self {
        let parameters = serde_json::json!({
            "type": "object",
            "properties": {}
        });
        Self::new(name, description, parameters)
    }
}

impl ToolResult {
    pub fn success(call_id: &str, content: &str) -> Self {
        Self {
            call_id: call_id.into(),
            content: content.into(),
            success: true,
        }
    }

    pub fn error(call_id: &str, content: &str) -> Self {
        Self {
            call_id: call_id.into(),
            content: content.into(),
            success: false,
        }
    }
}

impl ToolChatResponse {
    /// Whether the LLM wants to call tools.
    pub fn has_tool_calls(&self) -> bool {
        !self.tool_calls.is_empty()
    }

    /// Whether the LLM is finished (not requesting tool calls).
    pub fn is_finished(&self) -> bool {
        let fr = self.finish_reason.to_lowercase();
        self.tool_calls.is_empty() && (fr == "stop" || fr == "end_turn" || fr == "completed")
    }
}
