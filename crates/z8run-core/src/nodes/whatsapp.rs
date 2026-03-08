//! WhatsApp node: send messages via WhatsApp Business Cloud API (Meta Graph API).
//!
//! Supports three message types:
//! - **send_text**: Send plain text messages
//! - **send_template**: Send templated messages with language code
//! - **send_media**: Send media (images, documents, audio, video)
//!
//! Outputs:
//!   - "sent" port: successful API call (message accepted by WhatsApp)
//!   - "error" port: API or validation errors

use crate::engine::{NodeExecutor, NodeExecutorFactory};
use crate::error::Z8Result;
use crate::message::FlowMessage;
use serde_json::Value;
use tracing::{info, warn};

pub struct WhatsAppNode {
    name: String,
    phone_number_id: String,
    access_token: String,
    action: String, // "send_text", "send_template", or "send_media"
    api_version: String,
    timeout_ms: u64,
}

#[async_trait::async_trait]
impl NodeExecutor for WhatsAppNode {
    async fn process(&self, msg: FlowMessage) -> Z8Result<Vec<FlowMessage>> {
        match self.action.as_str() {
            "send_text" => self.handle_send_text(msg).await,
            "send_template" => self.handle_send_template(msg).await,
            "send_media" => self.handle_send_media(msg).await,
            _ => {
                let err_payload = serde_json::json!({
                    "error": format!("Unknown WhatsApp action: {}. Expected 'send_text', 'send_template', or 'send_media'", self.action),
                });
                Ok(vec![msg.derive(msg.source_node, "error", err_payload)])
            }
        }
    }

    async fn configure(&mut self, config: Value) -> Z8Result<()> {
        if let Some(v) = config.get("name").and_then(|v| v.as_str()) {
            self.name = v.to_string();
        }
        if let Some(v) = config.get("phoneNumberId").and_then(|v| v.as_str()) {
            self.phone_number_id = v.to_string();
        }
        if let Some(v) = config.get("accessToken").and_then(|v| v.as_str()) {
            self.access_token = v.to_string();
        }
        if let Some(v) = config.get("action").and_then(|v| v.as_str()) {
            self.action = v.to_string();
        }
        if let Some(v) = config.get("apiVersion").and_then(|v| v.as_str()) {
            self.api_version = v.to_string();
        }
        if let Some(v) = config.get("timeout").and_then(|v| v.as_u64()) {
            self.timeout_ms = v;
        }
        Ok(())
    }

    async fn validate(&self) -> Z8Result<()> {
        if self.phone_number_id.is_empty() {
            return Err(crate::error::Z8Error::Internal(
                "WhatsApp node requires phoneNumberId".to_string(),
            ));
        }
        if self.access_token.is_empty() {
            return Err(crate::error::Z8Error::Internal(
                "WhatsApp node requires accessToken".to_string(),
            ));
        }
        match self.action.as_str() {
            "send_text" | "send_template" | "send_media" => {}
            _ => {
                return Err(crate::error::Z8Error::Internal(
                    format!("Unknown WhatsApp action: {}", self.action),
                ))
            }
        }
        Ok(())
    }

    async fn shutdown(&self) -> Z8Result<()> {
        Ok(())
    }

    fn node_type(&self) -> &str {
        "whatsapp"
    }
}

impl WhatsAppNode {
    /// Send plain text message via WhatsApp Business Cloud API
    async fn handle_send_text(&self, msg: FlowMessage) -> Z8Result<Vec<FlowMessage>> {
        let to = extract_phone_number(&msg.payload, &["to", "phone", "phoneNumber", "number"]);
        let text = extract_text(&msg.payload, &["body", "message", "text", "content"]);

        if to.is_empty() {
            let err_payload = serde_json::json!({
                "error": "No phone number found in message. Expected 'to', 'phone', 'phoneNumber', or 'number' field",
            });
            return Ok(vec![msg.derive(msg.source_node, "error", err_payload)]);
        }

        if text.is_empty() {
            let err_payload = serde_json::json!({
                "error": "No message text found. Expected 'body', 'message', 'text', or 'content' field",
            });
            return Ok(vec![msg.derive(msg.source_node, "error", err_payload)]);
        }

        info!(
            node = %self.name,
            to = %to,
            "WhatsApp text message send request"
        );

        let url = format!(
            "https://graph.facebook.com/{}/{}/messages",
            self.api_version, self.phone_number_id
        );

        let body = serde_json::json!({
            "messaging_product": "whatsapp",
            "to": to,
            "type": "text",
            "text": {
                "body": text
            }
        });

        let client = reqwest::Client::new();

        match client
            .post(&url)
            .bearer_auth(&self.access_token)
            .header("Content-Type", "application/json")
            .json(&body)
            .timeout(std::time::Duration::from_millis(self.timeout_ms))
            .send()
            .await
        {
            Ok(response) => {
                let status = response.status().as_u16();
                let body_text = response.text().await.unwrap_or_default();

                if status >= 200 && status < 300 {
                    info!(
                        node = %self.name,
                        to = %to,
                        status = status,
                        "WhatsApp text message sent successfully"
                    );

                    let resp_payload = serde_json::json!({
                        "status": status,
                        "to": to,
                        "message": "WhatsApp text message sent successfully",
                    });

                    Ok(vec![msg.derive(msg.source_node, "sent", resp_payload)])
                } else {
                    warn!(
                        node = %self.name,
                        to = %to,
                        status = status,
                        body = %body_text,
                        "WhatsApp text message send failed"
                    );

                    let err_payload = serde_json::json!({
                        "error": format!("WhatsApp API failed with status {}: {}", status, body_text),
                        "status": status,
                        "to": to,
                    });

                    Ok(vec![msg.derive(msg.source_node, "error", err_payload)])
                }
            }
            Err(e) => {
                warn!(
                    node = %self.name,
                    to = %to,
                    error = %e,
                    "WhatsApp text message request failed"
                );

                let err_payload = serde_json::json!({
                    "error": format!("WhatsApp request failed: {}", e),
                    "to": to,
                });

                Ok(vec![msg.derive(msg.source_node, "error", err_payload)])
            }
        }
    }

    /// Send templated message via WhatsApp Business Cloud API
    async fn handle_send_template(&self, msg: FlowMessage) -> Z8Result<Vec<FlowMessage>> {
        let to = extract_phone_number(&msg.payload, &["to", "phone", "phoneNumber", "number"]);
        let template_name = extract_template_name(&msg.payload);
        let language_code = extract_language_code(&msg.payload);

        if to.is_empty() {
            let err_payload = serde_json::json!({
                "error": "No phone number found in message. Expected 'to', 'phone', 'phoneNumber', or 'number' field",
            });
            return Ok(vec![msg.derive(msg.source_node, "error", err_payload)]);
        }

        if template_name.is_empty() {
            let err_payload = serde_json::json!({
                "error": "No template name found. Expected 'templateName' or 'template_name' field",
            });
            return Ok(vec![msg.derive(msg.source_node, "error", err_payload)]);
        }

        info!(
            node = %self.name,
            to = %to,
            template = %template_name,
            "WhatsApp template message send request"
        );

        let url = format!(
            "https://graph.facebook.com/{}/{}/messages",
            self.api_version, self.phone_number_id
        );

        let body = serde_json::json!({
            "messaging_product": "whatsapp",
            "to": to,
            "type": "template",
            "template": {
                "name": template_name,
                "language": {
                    "code": language_code
                }
            }
        });

        let client = reqwest::Client::new();

        match client
            .post(&url)
            .bearer_auth(&self.access_token)
            .header("Content-Type", "application/json")
            .json(&body)
            .timeout(std::time::Duration::from_millis(self.timeout_ms))
            .send()
            .await
        {
            Ok(response) => {
                let status = response.status().as_u16();
                let body_text = response.text().await.unwrap_or_default();

                if status >= 200 && status < 300 {
                    info!(
                        node = %self.name,
                        to = %to,
                        template = %template_name,
                        status = status,
                        "WhatsApp template message sent successfully"
                    );

                    let resp_payload = serde_json::json!({
                        "status": status,
                        "to": to,
                        "template": template_name,
                        "message": "WhatsApp template message sent successfully",
                    });

                    Ok(vec![msg.derive(msg.source_node, "sent", resp_payload)])
                } else {
                    warn!(
                        node = %self.name,
                        to = %to,
                        template = %template_name,
                        status = status,
                        body = %body_text,
                        "WhatsApp template message send failed"
                    );

                    let err_payload = serde_json::json!({
                        "error": format!("WhatsApp API failed with status {}: {}", status, body_text),
                        "status": status,
                        "to": to,
                        "template": template_name,
                    });

                    Ok(vec![msg.derive(msg.source_node, "error", err_payload)])
                }
            }
            Err(e) => {
                warn!(
                    node = %self.name,
                    to = %to,
                    template = %template_name,
                    error = %e,
                    "WhatsApp template message request failed"
                );

                let err_payload = serde_json::json!({
                    "error": format!("WhatsApp request failed: {}", e),
                    "to": to,
                    "template": template_name,
                });

                Ok(vec![msg.derive(msg.source_node, "error", err_payload)])
            }
        }
    }

    /// Send media message via WhatsApp Business Cloud API
    async fn handle_send_media(&self, msg: FlowMessage) -> Z8Result<Vec<FlowMessage>> {
        let to = extract_phone_number(&msg.payload, &["to", "phone", "phoneNumber", "number"]);
        let media_url = extract_media_url(&msg.payload);

        if to.is_empty() {
            let err_payload = serde_json::json!({
                "error": "No phone number found in message. Expected 'to', 'phone', 'phoneNumber', or 'number' field",
            });
            return Ok(vec![msg.derive(msg.source_node, "error", err_payload)]);
        }

        if media_url.is_empty() {
            let err_payload = serde_json::json!({
                "error": "No media URL found. Expected 'mediaUrl', 'media_url', 'image', or 'imageUrl' field",
            });
            return Ok(vec![msg.derive(msg.source_node, "error", err_payload)]);
        }

        info!(
            node = %self.name,
            to = %to,
            media_url = %media_url,
            "WhatsApp media message send request"
        );

        let url = format!(
            "https://graph.facebook.com/{}/{}/messages",
            self.api_version, self.phone_number_id
        );

        let body = serde_json::json!({
            "messaging_product": "whatsapp",
            "to": to,
            "type": "image",
            "image": {
                "link": media_url
            }
        });

        let client = reqwest::Client::new();

        match client
            .post(&url)
            .bearer_auth(&self.access_token)
            .header("Content-Type", "application/json")
            .json(&body)
            .timeout(std::time::Duration::from_millis(self.timeout_ms))
            .send()
            .await
        {
            Ok(response) => {
                let status = response.status().as_u16();
                let body_text = response.text().await.unwrap_or_default();

                if status >= 200 && status < 300 {
                    info!(
                        node = %self.name,
                        to = %to,
                        media_url = %media_url,
                        status = status,
                        "WhatsApp media message sent successfully"
                    );

                    let resp_payload = serde_json::json!({
                        "status": status,
                        "to": to,
                        "media_url": media_url,
                        "message": "WhatsApp media message sent successfully",
                    });

                    Ok(vec![msg.derive(msg.source_node, "sent", resp_payload)])
                } else {
                    warn!(
                        node = %self.name,
                        to = %to,
                        media_url = %media_url,
                        status = status,
                        body = %body_text,
                        "WhatsApp media message send failed"
                    );

                    let err_payload = serde_json::json!({
                        "error": format!("WhatsApp API failed with status {}: {}", status, body_text),
                        "status": status,
                        "to": to,
                        "media_url": media_url,
                    });

                    Ok(vec![msg.derive(msg.source_node, "error", err_payload)])
                }
            }
            Err(e) => {
                warn!(
                    node = %self.name,
                    to = %to,
                    media_url = %media_url,
                    error = %e,
                    "WhatsApp media message request failed"
                );

                let err_payload = serde_json::json!({
                    "error": format!("WhatsApp request failed: {}", e),
                    "to": to,
                    "media_url": media_url,
                });

                Ok(vec![msg.derive(msg.source_node, "error", err_payload)])
            }
        }
    }
}

/// Extract phone number from payload using multiple field name variations
fn extract_phone_number(payload: &Value, field_names: &[&str]) -> String {
    for field_name in field_names {
        if let Some(s) = payload.get(field_name).and_then(|v| v.as_str()) {
            if !s.is_empty() {
                return s.to_string();
            }
        }
    }
    String::new()
}

/// Extract text content from payload using multiple field name variations
fn extract_text(payload: &Value, field_names: &[&str]) -> String {
    for field_name in field_names {
        if let Some(s) = payload.get(field_name).and_then(|v| v.as_str()) {
            if !s.is_empty() {
                return s.to_string();
            }
        }
    }
    String::new()
}

/// Extract template name from payload
fn extract_template_name(payload: &Value) -> String {
    if let Some(s) = payload.get("templateName").and_then(|v| v.as_str()) {
        if !s.is_empty() {
            return s.to_string();
        }
    }
    if let Some(s) = payload.get("template_name").and_then(|v| v.as_str()) {
        if !s.is_empty() {
            return s.to_string();
        }
    }
    String::new()
}

/// Extract language code from payload with default "en"
fn extract_language_code(payload: &Value) -> String {
    if let Some(s) = payload.get("languageCode").and_then(|v| v.as_str()) {
        if !s.is_empty() {
            return s.to_string();
        }
    }
    if let Some(s) = payload.get("language_code").and_then(|v| v.as_str()) {
        if !s.is_empty() {
            return s.to_string();
        }
    }
    "en".to_string()
}

/// Extract media URL from payload using multiple field name variations
fn extract_media_url(payload: &Value) -> String {
    let field_names = vec!["mediaUrl", "media_url", "image", "imageUrl"];
    for field_name in field_names {
        if let Some(s) = payload.get(field_name).and_then(|v| v.as_str()) {
            if !s.is_empty() {
                return s.to_string();
            }
        }
    }
    String::new()
}

pub struct WhatsAppNodeFactory;

#[async_trait::async_trait]
impl NodeExecutorFactory for WhatsAppNodeFactory {
    async fn create(&self, config: Value) -> Z8Result<Box<dyn NodeExecutor>> {
        let mut node = WhatsAppNode {
            name: "WhatsApp".to_string(),
            phone_number_id: String::new(),
            access_token: String::new(),
            action: "send_text".to_string(),
            api_version: "v18.0".to_string(),
            timeout_ms: 10000,
        };
        node.configure(config).await?;
        Ok(Box::new(node))
    }

    fn node_type(&self) -> &str {
        "whatsapp"
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_extract_phone_number_first_field() {
        let payload = serde_json::json!({
            "to": "+1234567890",
            "phone": "+0987654321"
        });
        let number = extract_phone_number(&payload, &["to", "phone"]);
        assert_eq!(number, "+1234567890");
    }

    #[test]
    fn test_extract_phone_number_fallback() {
        let payload = serde_json::json!({
            "phone": "+1234567890",
        });
        let number = extract_phone_number(&payload, &["to", "phone", "phoneNumber"]);
        assert_eq!(number, "+1234567890");
    }

    #[test]
    fn test_extract_phone_number_not_found() {
        let payload = serde_json::json!({
            "other": "+1234567890"
        });
        let number = extract_phone_number(&payload, &["to", "phone", "phoneNumber"]);
        assert_eq!(number, "");
    }

    #[test]
    fn test_extract_text_first_field() {
        let payload = serde_json::json!({
            "body": "Hello World",
            "message": "Goodbye"
        });
        let text = extract_text(&payload, &["body", "message"]);
        assert_eq!(text, "Hello World");
    }

    #[test]
    fn test_extract_text_fallback() {
        let payload = serde_json::json!({
            "text": "Test message"
        });
        let text = extract_text(&payload, &["body", "message", "text"]);
        assert_eq!(text, "Test message");
    }

    #[test]
    fn test_extract_text_not_found() {
        let payload = serde_json::json!({
            "other": "Some text"
        });
        let text = extract_text(&payload, &["body", "message", "text"]);
        assert_eq!(text, "");
    }

    #[test]
    fn test_extract_template_name_first_field() {
        let payload = serde_json::json!({
            "templateName": "greeting",
            "template_name": "farewell"
        });
        let name = extract_template_name(&payload);
        assert_eq!(name, "greeting");
    }

    #[test]
    fn test_extract_template_name_second_field() {
        let payload = serde_json::json!({
            "template_name": "greeting"
        });
        let name = extract_template_name(&payload);
        assert_eq!(name, "greeting");
    }

    #[test]
    fn test_extract_template_name_not_found() {
        let payload = serde_json::json!({
            "other": "value"
        });
        let name = extract_template_name(&payload);
        assert_eq!(name, "");
    }

    #[test]
    fn test_extract_language_code_default() {
        let payload = serde_json::json!({
            "other": "value"
        });
        let code = extract_language_code(&payload);
        assert_eq!(code, "en");
    }

    #[test]
    fn test_extract_language_code_explicit() {
        let payload = serde_json::json!({
            "languageCode": "es_ES"
        });
        let code = extract_language_code(&payload);
        assert_eq!(code, "es_ES");
    }

    #[test]
    fn test_extract_language_code_underscore() {
        let payload = serde_json::json!({
            "language_code": "fr_FR"
        });
        let code = extract_language_code(&payload);
        assert_eq!(code, "fr_FR");
    }

    #[test]
    fn test_extract_media_url_first_field() {
        let payload = serde_json::json!({
            "mediaUrl": "https://example.com/image.jpg",
            "image": "https://example.com/photo.jpg"
        });
        let url = extract_media_url(&payload);
        assert_eq!(url, "https://example.com/image.jpg");
    }

    #[test]
    fn test_extract_media_url_fallback() {
        let payload = serde_json::json!({
            "imageUrl": "https://example.com/image.jpg"
        });
        let url = extract_media_url(&payload);
        assert_eq!(url, "https://example.com/image.jpg");
    }

    #[test]
    fn test_extract_media_url_not_found() {
        let payload = serde_json::json!({
            "other": "value"
        });
        let url = extract_media_url(&payload);
        assert_eq!(url, "");
    }

    #[tokio::test]
    async fn test_whatsapp_node_factory_creates_default() {
        let factory = WhatsAppNodeFactory;
        let config = serde_json::json!({
            "phoneNumberId": "1234567890",
            "accessToken": "token123",
            "action": "send_text"
        });
        let node = factory.create(config).await;
        assert!(node.is_ok());
        let node = node.unwrap();
        assert_eq!(node.node_type(), "whatsapp");
    }

    #[tokio::test]
    async fn test_whatsapp_validation_missing_phone_number_id() {
        let mut node = WhatsAppNode {
            name: "test".to_string(),
            phone_number_id: String::new(),
            access_token: "token".to_string(),
            action: "send_text".to_string(),
            api_version: "v18.0".to_string(),
            timeout_ms: 5000,
        };
        let result = node.validate().await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_whatsapp_validation_missing_access_token() {
        let mut node = WhatsAppNode {
            name: "test".to_string(),
            phone_number_id: "123".to_string(),
            access_token: String::new(),
            action: "send_text".to_string(),
            api_version: "v18.0".to_string(),
            timeout_ms: 5000,
        };
        let result = node.validate().await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_whatsapp_validation_invalid_action() {
        let node = WhatsAppNode {
            name: "test".to_string(),
            phone_number_id: "123".to_string(),
            access_token: "token".to_string(),
            action: "invalid_action".to_string(),
            api_version: "v18.0".to_string(),
            timeout_ms: 5000,
        };
        let result = node.validate().await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_whatsapp_process_unknown_action() {
        let node = WhatsAppNode {
            name: "test".to_string(),
            phone_number_id: "123".to_string(),
            access_token: "token".to_string(),
            action: "unknown".to_string(),
            api_version: "v18.0".to_string(),
            timeout_ms: 5000,
        };

        let msg = FlowMessage::new(
            uuid::Uuid::now_v7(),
            "input",
            serde_json::json!({}),
            uuid::Uuid::now_v7(),
        );
        let results = node.process(msg).await.unwrap();
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].source_port, "error");
        assert!(results[0].payload["error"]
            .as_str()
            .unwrap()
            .contains("Unknown WhatsApp action"));
    }

    #[tokio::test]
    async fn test_whatsapp_send_text_missing_phone_number() {
        let node = WhatsAppNode {
            name: "test".to_string(),
            phone_number_id: "123".to_string(),
            access_token: "token".to_string(),
            action: "send_text".to_string(),
            api_version: "v18.0".to_string(),
            timeout_ms: 5000,
        };

        let msg = FlowMessage::new(
            uuid::Uuid::now_v7(),
            "input",
            serde_json::json!({ "body": "test message" }),
            uuid::Uuid::now_v7(),
        );
        let results = node.process(msg).await.unwrap();
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].source_port, "error");
        assert!(results[0].payload["error"]
            .as_str()
            .unwrap()
            .contains("No phone number found"));
    }

    #[tokio::test]
    async fn test_whatsapp_send_text_missing_message() {
        let node = WhatsAppNode {
            name: "test".to_string(),
            phone_number_id: "123".to_string(),
            access_token: "token".to_string(),
            action: "send_text".to_string(),
            api_version: "v18.0".to_string(),
            timeout_ms: 5000,
        };

        let msg = FlowMessage::new(
            uuid::Uuid::now_v7(),
            "input",
            serde_json::json!({ "to": "+1234567890" }),
            uuid::Uuid::now_v7(),
        );
        let results = node.process(msg).await.unwrap();
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].source_port, "error");
        assert!(results[0].payload["error"]
            .as_str()
            .unwrap()
            .contains("No message text found"));
    }

    #[tokio::test]
    async fn test_whatsapp_send_template_missing_template_name() {
        let node = WhatsAppNode {
            name: "test".to_string(),
            phone_number_id: "123".to_string(),
            access_token: "token".to_string(),
            action: "send_template".to_string(),
            api_version: "v18.0".to_string(),
            timeout_ms: 5000,
        };

        let msg = FlowMessage::new(
            uuid::Uuid::now_v7(),
            "input",
            serde_json::json!({ "to": "+1234567890" }),
            uuid::Uuid::now_v7(),
        );
        let results = node.process(msg).await.unwrap();
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].source_port, "error");
        assert!(results[0].payload["error"]
            .as_str()
            .unwrap()
            .contains("No template name found"));
    }

    #[tokio::test]
    async fn test_whatsapp_send_media_missing_media_url() {
        let node = WhatsAppNode {
            name: "test".to_string(),
            phone_number_id: "123".to_string(),
            access_token: "token".to_string(),
            action: "send_media".to_string(),
            api_version: "v18.0".to_string(),
            timeout_ms: 5000,
        };

        let msg = FlowMessage::new(
            uuid::Uuid::now_v7(),
            "input",
            serde_json::json!({ "to": "+1234567890" }),
            uuid::Uuid::now_v7(),
        );
        let results = node.process(msg).await.unwrap();
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].source_port, "error");
        assert!(results[0].payload["error"]
            .as_str()
            .unwrap()
            .contains("No media URL found"));
    }

    #[tokio::test]
    async fn test_whatsapp_send_template_missing_phone_number() {
        let node = WhatsAppNode {
            name: "test".to_string(),
            phone_number_id: "123".to_string(),
            access_token: "token".to_string(),
            action: "send_template".to_string(),
            api_version: "v18.0".to_string(),
            timeout_ms: 5000,
        };

        let msg = FlowMessage::new(
            uuid::Uuid::now_v7(),
            "input",
            serde_json::json!({ "templateName": "greeting" }),
            uuid::Uuid::now_v7(),
        );
        let results = node.process(msg).await.unwrap();
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].source_port, "error");
        assert!(results[0].payload["error"]
            .as_str()
            .unwrap()
            .contains("No phone number found"));
    }

    #[tokio::test]
    async fn test_whatsapp_send_media_missing_phone_number() {
        let node = WhatsAppNode {
            name: "test".to_string(),
            phone_number_id: "123".to_string(),
            access_token: "token".to_string(),
            action: "send_media".to_string(),
            api_version: "v18.0".to_string(),
            timeout_ms: 5000,
        };

        let msg = FlowMessage::new(
            uuid::Uuid::now_v7(),
            "input",
            serde_json::json!({ "mediaUrl": "https://example.com/image.jpg" }),
            uuid::Uuid::now_v7(),
        );
        let results = node.process(msg).await.unwrap();
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].source_port, "error");
        assert!(results[0].payload["error"]
            .as_str()
            .unwrap()
            .contains("No phone number found"));
    }

    #[tokio::test]
    async fn test_whatsapp_configuration() {
        let mut node = WhatsAppNode {
            name: "default".to_string(),
            phone_number_id: String::new(),
            access_token: String::new(),
            action: "send_text".to_string(),
            api_version: "v18.0".to_string(),
            timeout_ms: 10000,
        };

        let config = serde_json::json!({
            "name": "MyWhatsApp",
            "phoneNumberId": "9876543210",
            "accessToken": "mytoken123",
            "action": "send_template",
            "apiVersion": "v19.0",
            "timeout": 15000
        });

        node.configure(config).await.unwrap();

        assert_eq!(node.name, "MyWhatsApp");
        assert_eq!(node.phone_number_id, "9876543210");
        assert_eq!(node.access_token, "mytoken123");
        assert_eq!(node.action, "send_template");
        assert_eq!(node.api_version, "v19.0");
        assert_eq!(node.timeout_ms, 15000);
    }
}
