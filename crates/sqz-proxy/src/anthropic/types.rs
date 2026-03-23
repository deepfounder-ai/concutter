use serde::{Deserialize, Serialize};

// ---------------------------------------------------------------------------
// MessagesRequest
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MessagesRequest {
    pub model: String,
    pub messages: Vec<AnthropicMessage>,
    pub max_tokens: u32,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub system: Option<SystemPrompt>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub stream: Option<bool>,
    #[serde(flatten)]
    pub other: serde_json::Map<String, serde_json::Value>,
}

// ---------------------------------------------------------------------------
// AnthropicMessage
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AnthropicMessage {
    pub role: String,
    pub content: AnthropicContent,
    #[serde(flatten)]
    pub other: serde_json::Map<String, serde_json::Value>,
}

// ---------------------------------------------------------------------------
// AnthropicContent
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum AnthropicContent {
    Text(String),
    Blocks(Vec<ContentBlock>),
}

impl AnthropicContent {
    /// Return mutable references to all text strings within this content.
    ///
    /// Only `Text` variants and `ContentBlock::Text` blocks are returned;
    /// images, tool_use, tool_result, and other blocks are skipped.
    pub fn text_mut(&mut self) -> Vec<&mut String> {
        match self {
            AnthropicContent::Text(ref mut s) => vec![s],
            AnthropicContent::Blocks(ref mut blocks) => {
                let mut refs = Vec::new();
                for block in blocks {
                    if let ContentBlock::Text { ref mut text } = block {
                        refs.push(text);
                    }
                }
                refs
            }
        }
    }
}

// ---------------------------------------------------------------------------
// ContentBlock
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum ContentBlock {
    #[serde(rename = "text")]
    Text { text: String },
    #[serde(rename = "image")]
    Image { source: serde_json::Value },
    #[serde(rename = "tool_use")]
    ToolUse {
        #[serde(flatten)]
        data: serde_json::Map<String, serde_json::Value>,
    },
    #[serde(rename = "tool_result")]
    ToolResult {
        #[serde(flatten)]
        data: serde_json::Map<String, serde_json::Value>,
    },
    #[serde(other)]
    Other,
}

// ---------------------------------------------------------------------------
// SystemPrompt
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum SystemPrompt {
    Text(String),
    Blocks(Vec<SystemBlock>),
}

impl SystemPrompt {
    /// Return mutable references to all text strings within this system prompt.
    pub fn text_mut(&mut self) -> Vec<&mut String> {
        match self {
            SystemPrompt::Text(ref mut s) => vec![s],
            SystemPrompt::Blocks(ref mut blocks) => {
                let mut refs = Vec::new();
                for block in blocks {
                    if let SystemBlock::Text { ref mut text, .. } = block {
                        refs.push(text);
                    }
                }
                refs
            }
        }
    }
}

// ---------------------------------------------------------------------------
// SystemBlock
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum SystemBlock {
    #[serde(rename = "text")]
    Text {
        text: String,
        #[serde(flatten)]
        other: serde_json::Map<String, serde_json::Value>,
    },
    #[serde(other)]
    Other,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_anthropic_content_text_mut() {
        let mut content = AnthropicContent::Text("hello".to_string());
        let refs = content.text_mut();
        assert_eq!(refs.len(), 1);
    }

    #[test]
    fn test_anthropic_content_blocks_text_mut() {
        let mut content = AnthropicContent::Blocks(vec![
            ContentBlock::Text {
                text: "first".to_string(),
            },
            ContentBlock::Image {
                source: serde_json::Value::Null,
            },
            ContentBlock::Text {
                text: "second".to_string(),
            },
        ]);
        let refs = content.text_mut();
        assert_eq!(refs.len(), 2);
    }

    #[test]
    fn test_system_prompt_text_mut() {
        let mut prompt = SystemPrompt::Text("system text".to_string());
        let refs = prompt.text_mut();
        assert_eq!(refs.len(), 1);
    }

    #[test]
    fn test_system_prompt_blocks_text_mut() {
        let mut prompt = SystemPrompt::Blocks(vec![SystemBlock::Text {
            text: "block text".to_string(),
            other: serde_json::Map::new(),
        }]);
        let refs = prompt.text_mut();
        assert_eq!(refs.len(), 1);
    }

    #[test]
    fn test_deserialize_text_content() {
        let json = r#"{"role":"user","content":"hello"}"#;
        let msg: AnthropicMessage = serde_json::from_str(json).unwrap();
        assert!(matches!(msg.content, AnthropicContent::Text(_)));
    }

    #[test]
    fn test_deserialize_blocks_content() {
        let json = r#"{"role":"user","content":[{"type":"text","text":"hello"}]}"#;
        let msg: AnthropicMessage = serde_json::from_str(json).unwrap();
        assert!(matches!(msg.content, AnthropicContent::Blocks(_)));
    }

    #[test]
    fn test_deserialize_system_string() {
        let json = r#"{"model":"claude-3-opus-20240229","messages":[{"role":"user","content":"hi"}],"max_tokens":1024,"system":"Be helpful"}"#;
        let req: MessagesRequest = serde_json::from_str(json).unwrap();
        assert!(matches!(req.system, Some(SystemPrompt::Text(_))));
    }

    #[test]
    fn test_roundtrip() {
        let req = MessagesRequest {
            model: "claude-3-opus-20240229".to_string(),
            messages: vec![AnthropicMessage {
                role: "user".to_string(),
                content: AnthropicContent::Text("hello".to_string()),
                other: serde_json::Map::new(),
            }],
            max_tokens: 1024,
            system: Some(SystemPrompt::Text("Be helpful".to_string())),
            stream: None,
            other: serde_json::Map::new(),
        };
        let serialized = serde_json::to_string(&req).unwrap();
        let deserialized: MessagesRequest = serde_json::from_str(&serialized).unwrap();
        assert_eq!(deserialized.model, "claude-3-opus-20240229");
    }
}
