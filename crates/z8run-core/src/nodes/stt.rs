//! Speech-to-Text node: transcribes audio using AI speech recognition APIs.
//!
//! Supports OpenAI Whisper, Google Speech-to-Text, and Deepgram.
//!
//! Outputs:
//!   - "transcript" port: Transcription result with text, confidence, duration, language, and provider
//!   - "error" port: API or validation errors

use crate::configure_fields;
use crate::engine::NodeExecutor;
use crate::error::Z8Result;
use crate::message::FlowMessage;
use crate::node_factory;
use crate::utils::encoding::{base64_decode, base64_encode};
use crate::utils::node_helpers::{error_output, error_output_with_context, require_non_empty};
use tracing::{info, warn};

pub struct SttNode {
    name: String,
    provider: String, // "openai", "google", "deepgram"
    api_key: String,
    model: String,
    language: String, // default ""
    timeout_ms: u64,
}

#[async_trait::async_trait]
impl NodeExecutor for SttNode {
    async fn process(&self, msg: FlowMessage) -> Z8Result<Vec<FlowMessage>> {
        // Extract audio data from payload
        let (audio_data, audio_format) = match extract_audio_data(&msg.payload) {
            Some((data, format)) => (data, format),
            None => {
                return Ok(error_output(&msg, "No audio data found in message. Expected fields: audio, audioData, audio_data, file, or data"));
            }
        };

        // Extract language override from payload (optional)
        let language = extract_language(&msg.payload).unwrap_or_else(|| self.language.clone());

        info!(
            node = %self.name,
            provider = %self.provider,
            model = %self.model,
            audio_format = %audio_format,
            language = %language,
            "STT request"
        );

        let client = reqwest::Client::new();
        let timeout = std::time::Duration::from_millis(self.timeout_ms);

        let result = match self.provider.as_str() {
            "deepgram" => {
                self.call_deepgram(&client, &audio_data, &audio_format, &language, timeout)
                    .await
            }
            "google" => {
                self.call_google(&client, &audio_data, &language, timeout)
                    .await
            }
            _ => {
                self.call_openai(&client, &audio_data, &language, timeout)
                    .await
            } // default to OpenAI
        };

        match result {
            Ok(transcript_data) => {
                info!(
                    node = %self.name,
                    provider = %self.provider,
                    text_len = transcript_data.get("text").and_then(|v| v.as_str()).map(|s| s.len()).unwrap_or(0),
                    "STT transcription completed"
                );
                Ok(vec![msg.derive(
                    msg.source_node,
                    "transcript",
                    transcript_data,
                )])
            }
            Err(e) => {
                warn!(node = %self.name, error = %e, "STT request failed");
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
            "language" => language: str,
            "timeout" => timeout_ms: u64,
        );
        Ok(())
    }

    async fn validate(&self) -> Z8Result<()> {
        require_non_empty(&self.api_key, "STT node requires an API key")?;
        if self.provider != "openai" && self.provider != "google" && self.provider != "deepgram" {
            return Err(crate::error::Z8Error::Internal(format!(
                "Unknown provider: {}. Use 'openai', 'google', or 'deepgram'",
                self.provider
            )));
        }
        Ok(())
    }

    fn node_type(&self) -> &str {
        "stt"
    }
}

impl SttNode {
    /// Call OpenAI Whisper API for transcription
    async fn call_openai(
        &self,
        client: &reqwest::Client,
        audio_data: &[u8],
        language: &str,
        timeout: std::time::Duration,
    ) -> Result<serde_json::Value, String> {
        let url = "https://api.openai.com/v1/audio/transcriptions";

        let model = if self.model.is_empty() {
            "whisper-1".to_string()
        } else {
            self.model.clone()
        };

        // Build multipart form
        let form = reqwest::multipart::Form::new()
            .part(
                "file",
                reqwest::multipart::Part::bytes(audio_data.to_vec()).file_name("audio.wav"),
            )
            .text("model", model.clone());

        let form = if !language.is_empty() {
            form.text("language", language.to_string())
        } else {
            form
        };

        let resp = client
            .post(url)
            .bearer_auth(&self.api_key)
            .timeout(timeout)
            .multipart(form)
            .send()
            .await
            .map_err(|e| format!("OpenAI request failed: {}", e))?;

        let status = resp.status().as_u16();
        let text = resp
            .text()
            .await
            .map_err(|e| format!("Failed to read response: {}", e))?;

        if status != 200 {
            return Err(format!("OpenAI API error ({}): {}", status, text));
        }

        let json: serde_json::Value =
            serde_json::from_str(&text).map_err(|e| format!("Parse error: {}", e))?;

        let transcript = json["text"].as_str().unwrap_or("").to_string();

        if transcript.is_empty() {
            return Err("No transcript in response".to_string());
        }

        Ok(serde_json::json!({
            "text": transcript,
            "language": language,
            "confidence": json.get("confidence").and_then(|v| v.as_f64()).unwrap_or(0.0),
            "duration": json.get("duration").and_then(|v| v.as_f64()).unwrap_or(0.0),
            "provider": self.provider,
            "model": model,
        }))
    }

    /// Call Deepgram API for transcription
    async fn call_deepgram(
        &self,
        client: &reqwest::Client,
        audio_data: &[u8],
        audio_format: &str,
        language: &str,
        timeout: std::time::Duration,
    ) -> Result<serde_json::Value, String> {
        let mut url = format!(
            "https://api.deepgram.com/v1/listen?model={}",
            if self.model.is_empty() {
                "default"
            } else {
                &self.model
            }
        );

        if !language.is_empty() {
            url.push_str(&format!("&language={}", language));
        }

        let resp = client
            .post(&url)
            .header("Authorization", format!("Token {}", self.api_key))
            .header("Content-Type", audio_format)
            .timeout(timeout)
            .body(audio_data.to_vec())
            .send()
            .await
            .map_err(|e| format!("Deepgram request failed: {}", e))?;

        let status = resp.status().as_u16();
        let text = resp
            .text()
            .await
            .map_err(|e| format!("Failed to read response: {}", e))?;

        if status != 200 {
            return Err(format!("Deepgram API error ({}): {}", status, text));
        }

        let json: serde_json::Value =
            serde_json::from_str(&text).map_err(|e| format!("Parse error: {}", e))?;

        let transcript = json["results"]["channels"][0]["alternatives"][0]["transcript"]
            .as_str()
            .unwrap_or("")
            .to_string();

        let confidence = json["results"]["channels"][0]["alternatives"][0]["confidence"]
            .as_f64()
            .unwrap_or(0.0);

        let duration = json["metadata"]["duration"].as_f64().unwrap_or(0.0);

        if transcript.is_empty() {
            return Err("No transcript in response".to_string());
        }

        Ok(serde_json::json!({
            "text": transcript,
            "language": language,
            "confidence": confidence,
            "duration": duration,
            "provider": self.provider,
            "model": self.model,
        }))
    }

    /// Call Google Cloud Speech-to-Text API for transcription
    async fn call_google(
        &self,
        client: &reqwest::Client,
        audio_data: &[u8],
        language: &str,
        timeout: std::time::Duration,
    ) -> Result<serde_json::Value, String> {
        let url = "https://speech.googleapis.com/v1/speech:recognize";

        // Encode audio as base64
        let audio_base64 = base64_encode(audio_data);

        let body = serde_json::json!({
            "config": {
                "encoding": "LINEAR16",
                "languageCode": if language.is_empty() { "en-US" } else { language },
                "model": if self.model.is_empty() { "default" } else { &self.model },
            },
            "audio": {
                "content": audio_base64,
            }
        });

        let resp = client
            .post(url)
            .header("Content-Type", "application/json")
            .header("X-Goog-Api-Key", &self.api_key)
            .timeout(timeout)
            .json(&body)
            .send()
            .await
            .map_err(|e| format!("Google request failed: {}", e))?;

        let status = resp.status().as_u16();
        let text = resp
            .text()
            .await
            .map_err(|e| format!("Failed to read response: {}", e))?;

        if status != 200 {
            return Err(format!("Google API error ({}): {}", status, text));
        }

        let json: serde_json::Value =
            serde_json::from_str(&text).map_err(|e| format!("Parse error: {}", e))?;

        let transcript = json["results"][0]["alternatives"][0]["transcript"]
            .as_str()
            .unwrap_or("")
            .to_string();

        let confidence = json["results"][0]["alternatives"][0]["confidence"]
            .as_f64()
            .unwrap_or(0.0);

        if transcript.is_empty() {
            return Err("No transcript in response".to_string());
        }

        Ok(serde_json::json!({
            "text": transcript,
            "language": language,
            "confidence": confidence,
            "duration": 0.0,
            "provider": self.provider,
            "model": self.model,
        }))
    }
}

/// Extract audio data from payload. Returns (audio_bytes, audio_format).
fn extract_audio_data(payload: &serde_json::Value) -> Option<(Vec<u8>, String)> {
    let mut audio_bytes = Vec::new();
    let mut found = false;

    // Try common audio field names
    for key in &["audio", "audioData", "audio_data", "file", "data"] {
        if let Some(v) = payload.get(key) {
            if let Some(s) = v.as_str() {
                // Try base64 decode
                if let Ok(bytes) = base64_decode(s) {
                    audio_bytes = bytes;
                    found = true;
                    break;
                }
            }
        }
    }

    // If not found at top level, try nested in req.body
    if !found {
        if let Some(body) = payload.get("req").and_then(|r| r.get("body")) {
            for key in &["audio", "audioData", "audio_data", "file", "data"] {
                if let Some(v) = body.get(key) {
                    if let Some(s) = v.as_str() {
                        if let Ok(bytes) = base64_decode(s) {
                            audio_bytes = bytes;
                            found = true;
                            break;
                        }
                    }
                }
            }
        }
    }

    if !found || audio_bytes.is_empty() {
        return None;
    }

    // Extract audio format
    let audio_format = extract_audio_format(payload).unwrap_or_else(|| "audio/wav".to_string());

    Some((audio_bytes, audio_format))
}

/// Extract audio format from payload (default "audio/wav")
fn extract_audio_format(payload: &serde_json::Value) -> Option<String> {
    for key in &["format", "audioFormat", "content_type"] {
        if let Some(s) = payload.get(key).and_then(|v| v.as_str()) {
            return Some(s.to_string());
        }
    }

    // Try nested in req.body
    if let Some(body) = payload.get("req").and_then(|r| r.get("body")) {
        for key in &["format", "audioFormat", "content_type"] {
            if let Some(s) = body.get(key).and_then(|v| v.as_str()) {
                return Some(s.to_string());
            }
        }
    }

    None
}

/// Extract language override from payload (optional)
fn extract_language(payload: &serde_json::Value) -> Option<String> {
    for key in &["language", "lang"] {
        if let Some(s) = payload.get(key).and_then(|v| v.as_str()) {
            if !s.is_empty() {
                return Some(s.to_string());
            }
        }
    }

    // Try nested in req.body
    if let Some(body) = payload.get("req").and_then(|r| r.get("body")) {
        for key in &["language", "lang"] {
            if let Some(s) = body.get(key).and_then(|v| v.as_str()) {
                if !s.is_empty() {
                    return Some(s.to_string());
                }
            }
        }
    }

    None
}

node_factory!(SttNodeFactory, SttNode, "stt", {
    name: "STT".to_string(),
    provider: "openai".to_string(),
    api_key: String::new(),
    model: "whisper-1".to_string(),
    language: String::new(),
    timeout_ms: 30000
});

#[cfg(test)]
mod tests {
    use super::*;
    use crate::engine::NodeExecutorFactory;

    #[test]
    fn test_base64_encode_decode() {
        let original = b"Hello, World!";
        let encoded = base64_encode(original);
        let decoded = base64_decode(&encoded).unwrap();
        assert_eq!(original.to_vec(), decoded);
    }

    #[test]
    fn test_base64_encode() {
        let data = b"test";
        let encoded = base64_encode(data);
        assert_eq!(encoded, "dGVzdA==");
    }

    #[test]
    fn test_base64_decode() {
        let encoded = "dGVzdA==";
        let decoded = base64_decode(encoded).unwrap();
        assert_eq!(decoded, b"test");
    }

    #[test]
    fn test_extract_audio_data_with_base64() {
        let payload = serde_json::json!({
            "audio": "dGVzdA==",
            "format": "audio/wav"
        });

        let result = extract_audio_data(&payload);
        assert!(result.is_some());
        let (bytes, format) = result.unwrap();
        assert_eq!(bytes, b"test");
        assert_eq!(format, "audio/wav");
    }

    #[test]
    fn test_extract_audio_data_nested() {
        let payload = serde_json::json!({
            "req": {
                "body": {
                    "audioData": "dGVzdA==",
                    "format": "audio/mp3"
                }
            }
        });

        let result = extract_audio_data(&payload);
        assert!(result.is_some());
        let (bytes, format) = result.unwrap();
        assert_eq!(bytes, b"test");
        assert_eq!(format, "audio/mp3");
    }

    #[test]
    fn test_extract_audio_format_default() {
        let payload = serde_json::json!({
            "audio": "dGVzdA=="
        });

        let format = extract_audio_format(&payload);
        assert_eq!(format, None);
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
    fn test_stt_node_creation() {
        let _config = serde_json::json!({
            "name": "STT Node",
            "provider": "openai",
            "apiKey": "test-key",
            "model": "whisper-1",
            "language": "en",
            "timeout": 60000
        });

        let node = SttNode {
            name: "STT Node".to_string(),
            provider: "openai".to_string(),
            api_key: "test-key".to_string(),
            model: "whisper-1".to_string(),
            language: "en".to_string(),
            timeout_ms: 60000,
        };

        assert_eq!(node.name, "STT Node");
        assert_eq!(node.provider, "openai");
        assert_eq!(node.api_key, "test-key");
        assert_eq!(node.model, "whisper-1");
        assert_eq!(node.language, "en");
        assert_eq!(node.timeout_ms, 60000);
        assert_eq!(node.node_type(), "stt");
    }

    #[test]
    fn test_stt_node_validation() {
        let node = SttNode {
            name: "STT Node".to_string(),
            provider: "openai".to_string(),
            api_key: String::new(),
            model: "whisper-1".to_string(),
            language: String::new(),
            timeout_ms: 30000,
        };

        let result = tokio::runtime::Runtime::new()
            .unwrap()
            .block_on(node.validate());
        assert!(result.is_err());
    }

    #[test]
    fn test_stt_node_validation_invalid_provider() {
        let node = SttNode {
            name: "STT Node".to_string(),
            provider: "invalid".to_string(),
            api_key: "test-key".to_string(),
            model: "whisper-1".to_string(),
            language: String::new(),
            timeout_ms: 30000,
        };

        let result = tokio::runtime::Runtime::new()
            .unwrap()
            .block_on(node.validate());
        assert!(result.is_err());
    }

    #[test]
    fn test_stt_node_factory() {
        let factory = SttNodeFactory;
        assert_eq!(factory.node_type(), "stt");
    }
}
