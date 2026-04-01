//! Twilio node: send SMS, initiate calls, and lookup phone numbers via Twilio API.
//!
//! Supports three modes:
//! - **sms**: Send SMS via Twilio REST API
//! - **call**: Initiate voice call via Twilio REST API
//! - **lookup**: Lookup phone number info via Twilio Lookups API
//!
//! Outputs:
//!   - "sent" port: successful operation (SMS sent, call initiated, or lookup completed)
//!   - "error" port: API or validation errors

use crate::configure_fields;
use crate::engine::NodeExecutor;
use crate::error::Z8Result;
use crate::message::FlowMessage;
use crate::node_factory;
use crate::utils::extract::extract_field;
use crate::utils::node_helpers::{error_output, error_output_with_context, require_non_empty};
use serde_json::Value;
use tracing::{info, warn};

pub struct TwilioNode {
    name: String,
    account_sid: String,
    auth_token: String,
    from_number: String,
    action: String, // "sms", "call", or "lookup"
    timeout_ms: u64,
}

#[async_trait::async_trait]
impl NodeExecutor for TwilioNode {
    async fn process(&self, msg: FlowMessage) -> Z8Result<Vec<FlowMessage>> {
        match self.action.as_str() {
            "sms" => self.handle_sms(msg).await,
            "call" => self.handle_call(msg).await,
            "lookup" => self.handle_lookup(msg).await,
            _ => {
                return Ok(error_output(
                    &msg,
                    &format!(
                        "Unknown Twilio action: {}. Expected 'sms', 'call', or 'lookup'",
                        self.action
                    ),
                ));
            }
        }
    }

    async fn configure(&mut self, config: Value) -> Z8Result<()> {
        configure_fields!(config, self,
            "name" => name: str,
            "accountSid" => account_sid: str,
            "authToken" => auth_token: str,
            "fromNumber" => from_number: str,
            "action" => action: str,
            "timeout" => timeout_ms: u64,
        );
        Ok(())
    }

    async fn validate(&self) -> Z8Result<()> {
        require_non_empty(&self.account_sid, "Twilio node requires accountSid")?;
        require_non_empty(&self.auth_token, "Twilio node requires authToken")?;
        match self.action.as_str() {
            "sms" | "call" => {
                require_non_empty(
                    &self.from_number,
                    "Twilio node requires fromNumber for SMS and call actions",
                )?;
            }
            "lookup" => {}
            _ => {
                return Err(crate::error::Z8Error::Internal(format!(
                    "Unknown Twilio action: {}",
                    self.action
                )))
            }
        }
        Ok(())
    }

    async fn shutdown(&self) -> Z8Result<()> {
        Ok(())
    }

    fn node_type(&self) -> &str {
        "twilio"
    }
}

impl TwilioNode {
    /// Send SMS via Twilio REST API
    async fn handle_sms(&self, msg: FlowMessage) -> Z8Result<Vec<FlowMessage>> {
        let to = extract_field(&msg.payload, &["to", "phone", "phoneNumber", "number"]);
        let body = extract_field(&msg.payload, &["body", "message", "text", "content"]);

        if to.is_empty() {
            return Ok(error_output(&msg, "No phone number found in message. Expected 'to', 'phone', 'phoneNumber', or 'number' field"));
        }

        if body.is_empty() {
            return Ok(error_output(
                &msg,
                "No message body found. Expected 'body', 'message', 'text', or 'content' field",
            ));
        }

        info!(
            node = %self.name,
            to = %to,
            "Twilio SMS send request"
        );

        let url = format!(
            "https://api.twilio.com/2010-04-01/Accounts/{}/Messages.json",
            self.account_sid
        );

        if !url.starts_with("https://") {
            return Ok(error_output(&msg, "URL must use HTTPS"));
        }

        let client = reqwest::Client::builder()
            .https_only(true)
            .build()
            .map_err(|e| crate::error::Z8Error::Internal(format!("HTTP client error: {}", e)))?;
        let params = [
            ("From", self.from_number.as_str()),
            ("To", &to),
            ("Body", &body),
        ];

        match client
            .post(&url)
            .basic_auth(&self.account_sid, Some(&self.auth_token))
            .form(&params)
            .timeout(std::time::Duration::from_millis(self.timeout_ms))
            .send()
            .await
        {
            Ok(response) => {
                let status = response.status().as_u16();
                let body_text = response.text().await.unwrap_or_default();

                if (200..300).contains(&status) {
                    info!(
                        node = %self.name,
                        to = %to,
                        status = status,
                        "Twilio SMS sent successfully"
                    );

                    let resp_payload = serde_json::json!({
                        "status": status,
                        "to": to,
                        "from": self.from_number,
                        "message": "SMS sent successfully",
                    });

                    Ok(vec![msg.derive(msg.source_node, "sent", resp_payload)])
                } else {
                    warn!(
                        node = %self.name,
                        to = %to,
                        status = status,
                        body = %body_text,
                        "Twilio SMS failed"
                    );

                    Ok(error_output_with_context(
                        &msg,
                        &format!("Twilio SMS failed with status {}: {}", status, body_text),
                        serde_json::json!({"status": status, "to": to}),
                    ))
                }
            }
            Err(e) => {
                warn!(
                    node = %self.name,
                    to = %to,
                    error = %e,
                    "Twilio SMS request failed"
                );

                Ok(error_output_with_context(
                    &msg,
                    &format!("Twilio SMS request failed: {}", e),
                    serde_json::json!({"to": to}),
                ))
            }
        }
    }

    /// Initiate voice call via Twilio REST API
    async fn handle_call(&self, msg: FlowMessage) -> Z8Result<Vec<FlowMessage>> {
        let to = extract_field(&msg.payload, &["to", "phone", "phoneNumber", "number"]);
        let twiml_url = extract_twiml_url(&msg.payload);

        if to.is_empty() {
            return Ok(error_output(&msg, "No phone number found in message. Expected 'to', 'phone', 'phoneNumber', or 'number' field"));
        }

        if twiml_url.is_empty() {
            return Ok(error_output(
                &msg,
                "No TwiML URL found. Expected 'twimlUrl' in message payload",
            ));
        }

        info!(
            node = %self.name,
            to = %to,
            twiml_url = %twiml_url,
            "Twilio call initiation request"
        );

        let url = format!(
            "https://api.twilio.com/2010-04-01/Accounts/{}/Calls.json",
            self.account_sid
        );

        if !url.starts_with("https://") {
            return Ok(error_output(&msg, "URL must use HTTPS"));
        }

        let client = reqwest::Client::builder()
            .https_only(true)
            .build()
            .map_err(|e| crate::error::Z8Error::Internal(format!("HTTP client error: {}", e)))?;
        let params = [
            ("From", self.from_number.as_str()),
            ("To", &to),
            ("Url", &twiml_url),
        ];

        match client
            .post(&url)
            .basic_auth(&self.account_sid, Some(&self.auth_token))
            .form(&params)
            .timeout(std::time::Duration::from_millis(self.timeout_ms))
            .send()
            .await
        {
            Ok(response) => {
                let status = response.status().as_u16();
                let body_text = response.text().await.unwrap_or_default();

                if (200..300).contains(&status) {
                    info!(
                        node = %self.name,
                        to = %to,
                        status = status,
                        "Twilio call initiated successfully"
                    );

                    let resp_payload = serde_json::json!({
                        "status": status,
                        "to": to,
                        "from": self.from_number,
                        "message": "Call initiated successfully",
                    });

                    Ok(vec![msg.derive(msg.source_node, "sent", resp_payload)])
                } else {
                    warn!(
                        node = %self.name,
                        to = %to,
                        status = status,
                        body = %body_text,
                        "Twilio call initiation failed"
                    );

                    Ok(error_output_with_context(
                        &msg,
                        &format!(
                            "Twilio call initiation failed with status {}: {}",
                            status, body_text
                        ),
                        serde_json::json!({"status": status, "to": to}),
                    ))
                }
            }
            Err(e) => {
                warn!(
                    node = %self.name,
                    to = %to,
                    error = %e,
                    "Twilio call request failed"
                );

                Ok(error_output_with_context(
                    &msg,
                    &format!("Twilio call request failed: {}", e),
                    serde_json::json!({"to": to}),
                ))
            }
        }
    }

    /// Lookup phone number info via Twilio Lookups API
    async fn handle_lookup(&self, msg: FlowMessage) -> Z8Result<Vec<FlowMessage>> {
        let number = extract_field(&msg.payload, &["to", "phone", "phoneNumber", "number"]);

        if number.is_empty() {
            return Ok(error_output(&msg, "No phone number found in message. Expected 'to', 'phone', 'phoneNumber', or 'number' field"));
        }

        info!(
            node = %self.name,
            number = %number,
            "Twilio phone number lookup request"
        );

        let url = format!("https://lookups.twilio.com/v1/PhoneNumbers/{}", number);

        if !url.starts_with("https://") {
            return Ok(error_output(&msg, "URL must use HTTPS"));
        }

        let client = reqwest::Client::builder()
            .https_only(true)
            .build()
            .map_err(|e| crate::error::Z8Error::Internal(format!("HTTP client error: {}", e)))?;

        match client
            .get(&url)
            .basic_auth(&self.account_sid, Some(&self.auth_token))
            .timeout(std::time::Duration::from_millis(self.timeout_ms))
            .send()
            .await
        {
            Ok(response) => {
                let status = response.status().as_u16();
                let body_text = response.text().await.unwrap_or_default();

                if (200..300).contains(&status) {
                    let lookup_data: serde_json::Value =
                        serde_json::from_str(&body_text).unwrap_or(serde_json::json!({}));

                    info!(
                        node = %self.name,
                        number = %number,
                        status = status,
                        "Twilio phone lookup successful"
                    );

                    let resp_payload = serde_json::json!({
                        "status": status,
                        "number": number,
                        "data": lookup_data,
                    });

                    Ok(vec![msg.derive(msg.source_node, "sent", resp_payload)])
                } else {
                    warn!(
                        node = %self.name,
                        number = %number,
                        status = status,
                        body = %body_text,
                        "Twilio phone lookup failed"
                    );

                    Ok(error_output_with_context(
                        &msg,
                        &format!("Twilio lookup failed with status {}: {}", status, body_text),
                        serde_json::json!({"status": status, "number": number}),
                    ))
                }
            }
            Err(e) => {
                warn!(
                    node = %self.name,
                    number = %number,
                    error = %e,
                    "Twilio lookup request failed"
                );

                Ok(error_output_with_context(
                    &msg,
                    &format!("Twilio lookup request failed: {}", e),
                    serde_json::json!({"number": number}),
                ))
            }
        }
    }
}

/// Extract TwiML URL from payload or default to empty
fn extract_twiml_url(payload: &Value) -> String {
    if let Some(s) = payload.get("twimlUrl").and_then(|v| v.as_str()) {
        return s.to_string();
    }
    String::new()
}

node_factory!(TwilioNodeFactory, TwilioNode, "twilio", {
    name: "Twilio".to_string(),
    account_sid: String::new(),
    auth_token: String::new(),
    from_number: String::new(),
    action: "sms".to_string(),
    timeout_ms: 10000
});

#[cfg(test)]
mod tests {
    use super::*;
    use crate::engine::NodeExecutorFactory;

    #[test]
    fn test_extract_phone_number_first_field() {
        let payload = serde_json::json!({
            "to": "+1234567890",
            "phone": "+0987654321"
        });
        let number = extract_field(&payload, &["to", "phone"]);
        assert_eq!(number, "+1234567890");
    }

    #[test]
    fn test_extract_phone_number_fallback() {
        let payload = serde_json::json!({
            "phone": "+1234567890",
        });
        let number = extract_field(&payload, &["to", "phone", "phoneNumber"]);
        assert_eq!(number, "+1234567890");
    }

    #[test]
    fn test_extract_phone_number_not_found() {
        let payload = serde_json::json!({
            "other": "+1234567890"
        });
        let number = extract_field(&payload, &["to", "phone", "phoneNumber"]);
        assert_eq!(number, "");
    }

    #[test]
    fn test_extract_text_first_field() {
        let payload = serde_json::json!({
            "body": "Hello World",
            "message": "Goodbye"
        });
        let text = extract_field(&payload, &["body", "message"]);
        assert_eq!(text, "Hello World");
    }

    #[test]
    fn test_extract_text_fallback() {
        let payload = serde_json::json!({
            "text": "Test message"
        });
        let text = extract_field(&payload, &["body", "message", "text"]);
        assert_eq!(text, "Test message");
    }

    #[test]
    fn test_extract_text_not_found() {
        let payload = serde_json::json!({
            "other": "Some text"
        });
        let text = extract_field(&payload, &["body", "message", "text"]);
        assert_eq!(text, "");
    }

    #[test]
    fn test_extract_twiml_url_found() {
        let payload = serde_json::json!({
            "twimlUrl": "https://example.com/twiml.xml"
        });
        let url = extract_twiml_url(&payload);
        assert_eq!(url, "https://example.com/twiml.xml");
    }

    #[test]
    fn test_extract_twiml_url_not_found() {
        let payload = serde_json::json!({
            "other": "value"
        });
        let url = extract_twiml_url(&payload);
        assert_eq!(url, "");
    }

    #[tokio::test]
    async fn test_twilio_node_factory_creates_default() {
        let factory = TwilioNodeFactory;
        let config = serde_json::json!({
            "accountSid": "ACtest123",
            "authToken": "token123",
            "fromNumber": "+1234567890",
            "action": "sms"
        });
        let node = factory.create(config).await;
        assert!(node.is_ok());
        let node = node.unwrap();
        assert_eq!(node.node_type(), "twilio");
    }

    #[tokio::test]
    async fn test_twilio_validation_missing_account_sid() {
        let node = TwilioNode {
            name: "test".to_string(),
            account_sid: String::new(),
            auth_token: "token".to_string(),
            from_number: "+1234567890".to_string(),
            action: "sms".to_string(),
            timeout_ms: 5000,
        };
        let result = node.validate().await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_twilio_validation_missing_auth_token() {
        let node = TwilioNode {
            name: "test".to_string(),
            account_sid: "ACtest".to_string(),
            auth_token: String::new(),
            from_number: "+1234567890".to_string(),
            action: "sms".to_string(),
            timeout_ms: 5000,
        };
        let result = node.validate().await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_twilio_validation_sms_missing_from_number() {
        let node = TwilioNode {
            name: "test".to_string(),
            account_sid: "ACtest".to_string(),
            auth_token: "token".to_string(),
            from_number: String::new(),
            action: "sms".to_string(),
            timeout_ms: 5000,
        };
        let result = node.validate().await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_twilio_validation_lookup_no_from_number_required() {
        let node = TwilioNode {
            name: "test".to_string(),
            account_sid: "ACtest".to_string(),
            auth_token: "token".to_string(),
            from_number: String::new(),
            action: "lookup".to_string(),
            timeout_ms: 5000,
        };
        let result = node.validate().await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_twilio_process_unknown_action() {
        let node = TwilioNode {
            name: "test".to_string(),
            account_sid: "ACtest".to_string(),
            auth_token: "token".to_string(),
            from_number: "+1234567890".to_string(),
            action: "unknown".to_string(),
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
            .contains("Unknown Twilio action"));
    }

    #[tokio::test]
    async fn test_twilio_sms_missing_phone_number() {
        let node = TwilioNode {
            name: "test".to_string(),
            account_sid: "ACtest".to_string(),
            auth_token: "token".to_string(),
            from_number: "+1234567890".to_string(),
            action: "sms".to_string(),
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
    async fn test_twilio_sms_missing_body() {
        let node = TwilioNode {
            name: "test".to_string(),
            account_sid: "ACtest".to_string(),
            auth_token: "token".to_string(),
            from_number: "+1234567890".to_string(),
            action: "sms".to_string(),
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
            .contains("No message body found"));
    }

    #[tokio::test]
    async fn test_twilio_call_missing_twiml_url() {
        let node = TwilioNode {
            name: "test".to_string(),
            account_sid: "ACtest".to_string(),
            auth_token: "token".to_string(),
            from_number: "+1234567890".to_string(),
            action: "call".to_string(),
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
            .contains("No TwiML URL found"));
    }

    #[tokio::test]
    async fn test_twilio_lookup_missing_phone_number() {
        let node = TwilioNode {
            name: "test".to_string(),
            account_sid: "ACtest".to_string(),
            auth_token: "token".to_string(),
            from_number: String::new(),
            action: "lookup".to_string(),
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
            .contains("No phone number found"));
    }
}
