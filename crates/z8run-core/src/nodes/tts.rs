//! Text-to-Speech node: converts text to audio using AI TTS APIs.
//!
//! Supports OpenAI, ElevenLabs, and Google Cloud Text-to-Speech providers.
//!
//! Outputs:
//!   - "audio" port: Generated audio data in base64 with metadata
//!   - "error" port: API or validation errors

use crate::configure_fields;
use crate::engine::{NodeExecutor, NodeExecutorFactory};
use crate::error::Z8Result;
use crate::message::FlowMessage;
use crate::utils::extract::TEXT_FIELDS;
use crate::utils::node_helpers::{error_output, error_output_with_context, require_non_empty};
use tracing::{info, warn};

pub struct TtsNode {
    name: String,
    provider: String, // "openai", "elevenlabs", "google"
    api_key: String,
    model: String,
    voice: String,
    language: String, // for Google provider
    timeout_ms: u64,
}

#[async_trait::async_trait]
impl NodeExecutor for TtsNode {
    async fn process(&self, msg: FlowMessage) -> Z8Result<Vec<FlowMessage>> {
        // Extract text from payload
        let text = crate::utils::extract::extract_text(&msg.payload, TEXT_FIELDS);

        if text.is_empty() {
            return Ok(error_output(&msg, "No text found in message. Expected string payload or fields: text, message, body, content, input"));
        }

        // Extract voice override from payload (optional)
        let voice = extract_voice(&msg.payload).unwrap_or_else(|| self.voice.clone());

        info!(
            node = %self.name,
            provider = %self.provider,
            model = %self.model,
            voice = %voice,
            text_len = text.len(),
            "TTS request"
        );

        let client = reqwest::Client::new();
        let timeout = std::time::Duration::from_millis(self.timeout_ms);

        let result = match self.provider.as_str() {
            "elevenlabs" => self.call_elevenlabs(&client, &text, &voice, timeout).await,
            "google" => self.call_google(&client, &text, &voice, timeout).await,
            _ => self.call_openai(&client, &text, &voice, timeout).await, // default to OpenAI
        };

        match result {
            Ok(audio_data) => {
                info!(
                    node = %self.name,
                    provider = %self.provider,
                    audio_len = audio_data.get("audio").and_then(|v| v.as_str()).map(|s| s.len()).unwrap_or(0),
                    "TTS audio generated successfully"
                );
                Ok(vec![msg.derive(msg.source_node, "audio", audio_data)])
            }
            Err(e) => {
                warn!(node = %self.name, error = %e, "TTS request failed");
                return Ok(error_output_with_context(
                    &msg,
                    &e,
                    serde_json::json!({
                        "provider": self.provider,
                        "model": self.model,
                    }),
                ));
            }
        }
    }

    async fn configure(&mut self, config: serde_json::Value) -> Z8Result<()> {
        configure_fields!(config, self,
            "name" => name: str,
            "provider" => provider: str_lower,
            "apiKey" => api_key: str,
            "model" => model: str,
            "voice" => voice: str,
            "language" => language: str,
            "timeout" => timeout_ms: u64,
        );
        Ok(())
    }

    async fn validate(&self) -> Z8Result<()> {
        require_non_empty(&self.api_key, "TTS node requires an API key")?;
        if self.provider != "openai" && self.provider != "elevenlabs" && self.provider != "google" {
            return Err(crate::error::Z8Error::Internal(format!(
                "Unknown provider: {}. Use 'openai', 'elevenlabs', or 'google'",
                self.provider
            )));
        }
        Ok(())
    }

    fn node_type(&self) -> &str {
        "tts"
    }
}

impl TtsNode {
    /// Call OpenAI Text-to-Speech API
    async fn call_openai(
        &self,
        client: &reqwest::Client,
        text: &str,
        voice: &str,
        timeout: std::time::Duration,
    ) -> Result<serde_json::Value, String> {
        let url = "https://api.openai.com/v1/audio/speech";

        let model = if self.model.is_empty() {
            "tts-1".to_string()
        } else {
            self.model.clone()
        };

        let voice_name = if voice.is_empty() {
            "alloy".to_string()
        } else {
            voice.to_string()
        };

        let body = serde_json::json!({
            "model": model,
            "input": text,
            "voice": voice_name,
        });

        let resp = client
            .post(url)
            .bearer_auth(&self.api_key)
            .header("Content-Type", "application/json")
            .timeout(timeout)
            .json(&body)
            .send()
            .await
            .map_err(|e| format!("OpenAI TTS request failed: {}", e))?;

        let status = resp.status().as_u16();

        if status != 200 {
            let text = resp
                .text()
                .await
                .unwrap_or_else(|_| "Unknown error".to_string());
            return Err(format!("OpenAI TTS API error ({}): {}", status, text));
        }

        let audio_bytes = resp
            .bytes()
            .await
            .map_err(|e| format!("Failed to read audio response: {}", e))?;

        let audio_base64 = base64_encode(&audio_bytes);

        Ok(serde_json::json!({
            "audio": audio_base64,
            "format": "mp3",
            "provider": "openai",
            "voice": voice_name,
            "model": model,
            "text_length": text.len(),
        }))
    }

    /// Call ElevenLabs Text-to-Speech API
    async fn call_elevenlabs(
        &self,
        client: &reqwest::Client,
        text: &str,
        voice: &str,
        timeout: std::time::Duration,
    ) -> Result<serde_json::Value, String> {
        let voice_id = if voice.is_empty() {
            "21m00Tcm4TlvDq8ikWAM".to_string() // Rachel
        } else {
            voice.to_string()
        };

        let model = if self.model.is_empty() {
            "eleven_monolingual_v1".to_string()
        } else {
            self.model.clone()
        };

        let url = format!("https://api.elevenlabs.io/v1/text-to-speech/{}", voice_id);

        let body = serde_json::json!({
            "text": text,
            "model_id": model,
        });

        let resp = client
            .post(&url)
            .header("xi-api-key", &self.api_key)
            .header("Content-Type", "application/json")
            .timeout(timeout)
            .json(&body)
            .send()
            .await
            .map_err(|e| format!("ElevenLabs TTS request failed: {}", e))?;

        let status = resp.status().as_u16();

        if status != 200 {
            let text = resp
                .text()
                .await
                .unwrap_or_else(|_| "Unknown error".to_string());
            return Err(format!("ElevenLabs TTS API error ({}): {}", status, text));
        }

        let audio_bytes = resp
            .bytes()
            .await
            .map_err(|e| format!("Failed to read audio response: {}", e))?;

        let audio_base64 = base64_encode(&audio_bytes);

        Ok(serde_json::json!({
            "audio": audio_base64,
            "format": "mp3",
            "provider": "elevenlabs",
            "voice": voice_id,
            "model": model,
            "text_length": text.len(),
        }))
    }

    /// Call Google Cloud Text-to-Speech API
    async fn call_google(
        &self,
        client: &reqwest::Client,
        text: &str,
        voice: &str,
        timeout: std::time::Duration,
    ) -> Result<serde_json::Value, String> {
        let voice_name = if voice.is_empty() {
            "en-US-Standard-A".to_string()
        } else {
            voice.to_string()
        };

        let language = if self.language.is_empty() {
            "en-US".to_string()
        } else {
            self.language.clone()
        };

        let url = format!(
            "https://texttospeech.googleapis.com/v1/text:synthesize?key={}",
            self.api_key
        );

        let body = serde_json::json!({
            "input": {
                "text": text
            },
            "voice": {
                "languageCode": language,
                "name": voice_name
            },
            "audioConfig": {
                "audioEncoding": "MP3"
            }
        });

        let resp = client
            .post(&url)
            .header("Content-Type", "application/json")
            .timeout(timeout)
            .json(&body)
            .send()
            .await
            .map_err(|e| format!("Google TTS request failed: {}", e))?;

        let status = resp.status().as_u16();
        let response_text = resp
            .text()
            .await
            .map_err(|e| format!("Failed to read response: {}", e))?;

        if status != 200 {
            return Err(format!(
                "Google TTS API error ({}): {}",
                status, response_text
            ));
        }

        let json: serde_json::Value =
            serde_json::from_str(&response_text).map_err(|e| format!("Parse error: {}", e))?;

        let audio_base64 = json["audioContent"]
            .as_str()
            .ok_or("No audioContent in response")?
            .to_string();

        Ok(serde_json::json!({
            "audio": audio_base64,
            "format": "mp3",
            "provider": "google",
            "voice": voice_name,
            "language": language,
            "text_length": text.len(),
        }))
    }
}

/// Extract voice override from payload
fn extract_voice(payload: &serde_json::Value) -> Option<String> {
    payload
        .get("voice")
        .and_then(|v| v.as_str())
        .map(|s| s.to_string())
}

/// Extract language from payload for Google provider
#[allow(dead_code)]
fn extract_language(payload: &serde_json::Value) -> Option<String> {
    for key in &["language", "lang"] {
        if let Some(s) = payload.get(key).and_then(|v| v.as_str()) {
            return Some(s.to_string());
        }
    }
    None
}

/// Encode bytes to base64 string
fn base64_encode(data: &[u8]) -> String {
    use base64::{engine::general_purpose, Engine as _};
    general_purpose::STANDARD.encode(data)
}

pub struct TtsNodeFactory;

#[async_trait::async_trait]
impl NodeExecutorFactory for TtsNodeFactory {
    async fn create(&self, config: serde_json::Value) -> Z8Result<Box<dyn NodeExecutor>> {
        let mut node = TtsNode {
            name: "Tts".to_string(),
            provider: "openai".to_string(),
            api_key: String::new(),
            model: String::new(),
            voice: String::new(),
            language: String::new(),
            timeout_ms: 30000,
        };
        node.configure(config).await?;
        Ok(Box::new(node))
    }

    fn node_type(&self) -> &str {
        "tts"
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_extract_text_string_payload() {
        let payload = serde_json::json!("Hello, world!");
        let text = crate::utils::extract::extract_text(&payload, TEXT_FIELDS);
        assert_eq!(text, "Hello, world!");
    }

    #[test]
    fn test_extract_text_from_field() {
        let payload = serde_json::json!({
            "text": "Hello, world!"
        });
        let text = crate::utils::extract::extract_text(&payload, TEXT_FIELDS);
        assert_eq!(text, "Hello, world!");
    }

    #[test]
    fn test_extract_text_from_message_field() {
        let payload = serde_json::json!({
            "message": "Hello from message"
        });
        let text = crate::utils::extract::extract_text(&payload, TEXT_FIELDS);
        assert_eq!(text, "Hello from message");
    }

    #[test]
    fn test_extract_text_from_body_field() {
        let payload = serde_json::json!({
            "body": "Hello from body"
        });
        let text = crate::utils::extract::extract_text(&payload, TEXT_FIELDS);
        assert_eq!(text, "Hello from body");
    }

    #[test]
    fn test_extract_text_from_content_field() {
        let payload = serde_json::json!({
            "content": "Hello from content"
        });
        let text = crate::utils::extract::extract_text(&payload, TEXT_FIELDS);
        assert_eq!(text, "Hello from content");
    }

    #[test]
    fn test_extract_text_from_input_field() {
        let payload = serde_json::json!({
            "input": "Hello from input"
        });
        let text = crate::utils::extract::extract_text(&payload, TEXT_FIELDS);
        assert_eq!(text, "Hello from input");
    }

    #[test]
    fn test_extract_text_from_nested_body() {
        let payload = serde_json::json!({
            "req": {
                "body": {
                    "text": "Hello from nested"
                }
            }
        });
        let text = crate::utils::extract::extract_text(&payload, TEXT_FIELDS);
        assert_eq!(text, "Hello from nested");
    }

    #[test]
    fn test_extract_text_empty() {
        let payload = serde_json::json!({});
        let text = crate::utils::extract::extract_text(&payload, TEXT_FIELDS);
        assert_eq!(text, "");
    }

    #[test]
    fn test_extract_voice() {
        let payload = serde_json::json!({
            "voice": "nova"
        });
        let voice = extract_voice(&payload);
        assert_eq!(voice, Some("nova".to_string()));
    }

    #[test]
    fn test_extract_voice_none() {
        let payload = serde_json::json!({
            "text": "Hello"
        });
        let voice = extract_voice(&payload);
        assert_eq!(voice, None);
    }

    #[test]
    fn test_extract_language() {
        let payload = serde_json::json!({
            "language": "en-US"
        });
        let lang = extract_language(&payload);
        assert_eq!(lang, Some("en-US".to_string()));
    }

    #[test]
    fn test_extract_language_lang_alias() {
        let payload = serde_json::json!({
            "lang": "fr-FR"
        });
        let lang = extract_language(&payload);
        assert_eq!(lang, Some("fr-FR".to_string()));
    }

    #[test]
    fn test_extract_language_none() {
        let payload = serde_json::json!({
            "text": "Hello"
        });
        let lang = extract_language(&payload);
        assert_eq!(lang, None);
    }

    #[test]
    fn test_tts_node_creation() {
        let node = TtsNode {
            name: "TTS Node".to_string(),
            provider: "openai".to_string(),
            api_key: "test-key".to_string(),
            model: "tts-1".to_string(),
            voice: "alloy".to_string(),
            language: "en-US".to_string(),
            timeout_ms: 30000,
        };

        assert_eq!(node.name, "TTS Node");
        assert_eq!(node.provider, "openai");
        assert_eq!(node.api_key, "test-key");
        assert_eq!(node.model, "tts-1");
        assert_eq!(node.voice, "alloy");
        assert_eq!(node.language, "en-US");
        assert_eq!(node.timeout_ms, 30000);
        assert_eq!(node.node_type(), "tts");
    }

    #[test]
    fn test_tts_node_creation_elevenlabs() {
        let node = TtsNode {
            name: "TTS ElevenLabs".to_string(),
            provider: "elevenlabs".to_string(),
            api_key: "test-key".to_string(),
            model: "eleven_monolingual_v1".to_string(),
            voice: "21m00Tcm4TlvDq8ikWAM".to_string(),
            language: String::new(),
            timeout_ms: 30000,
        };

        assert_eq!(node.provider, "elevenlabs");
        assert_eq!(node.voice, "21m00Tcm4TlvDq8ikWAM");
    }

    #[test]
    fn test_tts_node_creation_google() {
        let node = TtsNode {
            name: "TTS Google".to_string(),
            provider: "google".to_string(),
            api_key: "test-key".to_string(),
            model: String::new(),
            voice: "en-US-Standard-A".to_string(),
            language: "en-US".to_string(),
            timeout_ms: 30000,
        };

        assert_eq!(node.provider, "google");
        assert_eq!(node.language, "en-US");
    }

    #[test]
    fn test_tts_node_validation() {
        let node = TtsNode {
            name: "TTS Node".to_string(),
            provider: "openai".to_string(),
            api_key: String::new(),
            model: "tts-1".to_string(),
            voice: "alloy".to_string(),
            language: String::new(),
            timeout_ms: 30000,
        };

        let result = tokio::runtime::Runtime::new()
            .unwrap()
            .block_on(node.validate());
        assert!(result.is_err());
    }

    #[test]
    fn test_tts_node_validation_invalid_provider() {
        let node = TtsNode {
            name: "TTS Node".to_string(),
            provider: "invalid".to_string(),
            api_key: "test-key".to_string(),
            model: "tts-1".to_string(),
            voice: "alloy".to_string(),
            language: String::new(),
            timeout_ms: 30000,
        };

        let result = tokio::runtime::Runtime::new()
            .unwrap()
            .block_on(node.validate());
        assert!(result.is_err());
    }

    #[test]
    fn test_tts_node_validation_success() {
        let node = TtsNode {
            name: "TTS Node".to_string(),
            provider: "openai".to_string(),
            api_key: "test-key".to_string(),
            model: "tts-1".to_string(),
            voice: "alloy".to_string(),
            language: String::new(),
            timeout_ms: 30000,
        };

        let result = tokio::runtime::Runtime::new()
            .unwrap()
            .block_on(node.validate());
        assert!(result.is_ok());
    }

    #[test]
    fn test_tts_node_factory() {
        let factory = TtsNodeFactory;
        assert_eq!(factory.node_type(), "tts");
    }

    #[test]
    fn test_base64_encode() {
        let data = b"Hello, world!";
        let encoded = base64_encode(data);
        assert_eq!(encoded, "SGVsbG8sIHdvcmxkIQ==");
    }
}
