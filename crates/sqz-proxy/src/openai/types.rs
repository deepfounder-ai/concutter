use serde::{Deserialize, Serialize};

// ---------------------------------------------------------------------------
// ChatCompletionRequest
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChatCompletionRequest {
    pub model: String,
    pub messages: Vec<Message>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub stream: Option<bool>,
    #[serde(flatten)]
    pub other: serde_json::Map<String, serde_json::Value>,
}

// ---------------------------------------------------------------------------
// Message
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Message {
    pub role: String,
    #[serde(default)]
    pub content: MessageContent,
    #[serde(flatten)]
    pub other: serde_json::Map<String, serde_json::Value>,
}

// ---------------------------------------------------------------------------
// MessageContent
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(untagged)]
pub enum MessageContent {
    Text(String),
    Parts(Vec<ContentPart>),
    #[default]
    Null,
}

impl MessageContent {
    /// Return mutable references to all text strings within this content.
    ///
    /// This is used by the compression layer to modify text in-place without
    /// needing to reconstruct the entire message structure.
    pub fn text_mut(&mut self) -> Vec<&mut String> {
        match self {
            MessageContent::Text(ref mut s) => vec![s],
            MessageContent::Parts(ref mut parts) => {
                let mut refs = Vec::new();
                for part in parts {
                    if let ContentPart::Text { ref mut text } = part {
                        refs.push(text);
                    }
                }
                refs
            }
            MessageContent::Null => vec![],
        }
    }
}

// ---------------------------------------------------------------------------
// ContentPart
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum ContentPart {
    #[serde(rename = "text")]
    Text { text: String },
    #[serde(rename = "image_url")]
    ImageUrl { image_url: serde_json::Value },
    #[serde(other)]
    Other,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_text_mut_string() {
        let mut content = MessageContent::Text("hello".to_string());
        let refs = content.text_mut();
        assert_eq!(refs.len(), 1);
        assert_eq!(*refs[0], "hello");
    }

    #[test]
    fn test_text_mut_parts() {
        let mut content = MessageContent::Parts(vec![
            ContentPart::Text {
                text: "first".to_string(),
            },
            ContentPart::Other,
            ContentPart::Text {
                text: "second".to_string(),
            },
        ]);
        let refs = content.text_mut();
        assert_eq!(refs.len(), 2);
    }

    #[test]
    fn test_text_mut_null() {
        let mut content = MessageContent::Null;
        let refs = content.text_mut();
        assert!(refs.is_empty());
    }

    #[test]
    fn test_deserialize_text_content() {
        let json = r#"{"role":"user","content":"hello"}"#;
        let msg: Message = serde_json::from_str(json).unwrap();
        assert!(matches!(msg.content, MessageContent::Text(_)));
    }

    #[test]
    fn test_deserialize_parts_content() {
        let json = r#"{"role":"user","content":[{"type":"text","text":"hello"}]}"#;
        let msg: Message = serde_json::from_str(json).unwrap();
        assert!(matches!(msg.content, MessageContent::Parts(_)));
    }

    #[test]
    fn test_deserialize_null_content() {
        let json = r#"{"role":"assistant","content":null}"#;
        let msg: Message = serde_json::from_str(json).unwrap();
        assert!(matches!(msg.content, MessageContent::Null));
    }

    #[test]
    fn test_roundtrip() {
        let req = ChatCompletionRequest {
            model: "gpt-4".to_string(),
            messages: vec![Message {
                role: "user".to_string(),
                content: MessageContent::Text("hello".to_string()),
                other: serde_json::Map::new(),
            }],
            stream: Some(false),
            other: serde_json::Map::new(),
        };
        let serialized = serde_json::to_string(&req).unwrap();
        let deserialized: ChatCompletionRequest = serde_json::from_str(&serialized).unwrap();
        assert_eq!(deserialized.model, "gpt-4");
    }
}
