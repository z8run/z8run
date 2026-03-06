//! MQTT node: publish and subscribe to MQTT brokers.
//!
//! Supports two modes:
//! - **subscribe**: Connect to broker, subscribe to topic, wait for one message
//! - **publish**: Receive message, publish payload to MQTT topic
//!
//! Outputs:
//!   - "message" port: Received MQTT message (subscribe mode)
//!   - "published" port: Publish confirmation (publish mode)
//!   - "error" port: Connection or operation errors

use crate::engine::{NodeExecutor, NodeExecutorFactory};
use crate::error::Z8Result;
use crate::message::FlowMessage;
use rumqttc::{AsyncClient, Event, MqttOptions, Packet, QoS};
use std::time::Duration;
use tracing::{info, warn};

pub struct MqttNode {
    name: String,
    action: String, // "subscribe" or "publish"
    broker: String,
    port: u16,
    topic: String,
    qos: u8,
    client_id: String,
    username: String,
    password: String,
    use_tls: bool,
    keep_alive: u64,
    timeout_ms: u64,
}

#[async_trait::async_trait]
impl NodeExecutor for MqttNode {
    async fn process(&self, msg: FlowMessage) -> Z8Result<Vec<FlowMessage>> {
        match self.action.as_str() {
            "publish" => self.handle_publish(msg).await,
            "subscribe" => self.handle_subscribe(msg).await,
            _ => {
                let err_payload = serde_json::json!({
                    "error": format!("Unknown MQTT action: {}. Expected 'publish' or 'subscribe'", self.action),
                });
                Ok(vec![msg.derive(msg.source_node, "error", err_payload)])
            }
        }
    }

    async fn configure(&mut self, config: serde_json::Value) -> Z8Result<()> {
        if let Some(v) = config.get("name").and_then(|v| v.as_str()) {
            self.name = v.to_string();
        }
        if let Some(v) = config.get("action").and_then(|v| v.as_str()) {
            self.action = v.to_string();
        }
        if let Some(v) = config.get("broker").and_then(|v| v.as_str()) {
            self.broker = v.to_string();
        }
        if let Some(v) = config.get("port").and_then(|v| v.as_u64()) {
            self.port = v as u16;
        }
        if let Some(v) = config.get("topic").and_then(|v| v.as_str()) {
            self.topic = v.to_string();
        }
        if let Some(v) = config.get("qos").and_then(|v| v.as_u64()) {
            self.qos = (v as u8).min(2);
        }
        if let Some(v) = config.get("clientId").and_then(|v| v.as_str()) {
            if !v.is_empty() {
                self.client_id = v.to_string();
            }
        }
        if let Some(v) = config.get("username").and_then(|v| v.as_str()) {
            self.username = v.to_string();
        }
        if let Some(v) = config.get("password").and_then(|v| v.as_str()) {
            self.password = v.to_string();
        }
        if let Some(v) = config.get("useTls").and_then(|v| v.as_bool()) {
            self.use_tls = v;
        }
        if let Some(v) = config.get("keepAlive").and_then(|v| v.as_u64()) {
            self.keep_alive = v;
        }
        if let Some(v) = config.get("timeout").and_then(|v| v.as_u64()) {
            self.timeout_ms = v;
        }
        Ok(())
    }

    async fn validate(&self) -> Z8Result<()> {
        if self.action != "subscribe" && self.action != "publish" {
            return Err(crate::error::Z8Error::Internal(format!(
                "MQTT action must be 'subscribe' or 'publish', got: {}",
                self.action
            )));
        }
        if self.broker.is_empty() {
            return Err(crate::error::Z8Error::Internal(
                "MQTT broker cannot be empty".to_string(),
            ));
        }
        if self.topic.is_empty() {
            return Err(crate::error::Z8Error::Internal(
                "MQTT topic cannot be empty".to_string(),
            ));
        }
        Ok(())
    }

    fn node_type(&self) -> &str {
        "mqtt"
    }
}

impl MqttNode {
    /// Publish mode: extract payload from message and publish to MQTT topic
    async fn handle_publish(&self, msg: FlowMessage) -> Z8Result<Vec<FlowMessage>> {
        info!(
            node = %self.name,
            broker = %self.broker,
            topic = %self.topic,
            "MQTT publish request"
        );

        // Extract payload as string
        let payload = extract_payload(&msg.payload);

        if payload.is_empty() {
            let err_payload = serde_json::json!({
                "error": "No payload found in message. Expected string payload or 'payload'/'body'/'text' field",
            });
            return Ok(vec![msg.derive(msg.source_node, "error", err_payload)]);
        }

        // Build MQTT options
        let mut opts = MqttOptions::new(&self.client_id, &self.broker, self.port);
        opts.set_keep_alive(Duration::from_secs(self.keep_alive));

        if !self.username.is_empty() {
            opts.set_credentials(&self.username, &self.password);
        }

        if self.use_tls {
            opts.set_transport(rumqttc::Transport::tls_with_default_config());
        }

        // Create async client
        let (client, mut eventloop) = AsyncClient::new(opts, 10);

        // Spawn eventloop polling in background
        let handle = tokio::spawn(async move {
            loop {
                match eventloop.poll().await {
                    Ok(_) => continue,
                    Err(_) => break,
                }
            }
        });

        // Publish message
        let qos = match self.qos {
            0 => QoS::AtMostOnce,
            1 => QoS::AtLeastOnce,
            _ => QoS::ExactlyOnce,
        };

        let result = client
            .publish(&self.topic, qos, false, payload.as_bytes())
            .await;

        // Cleanup
        let _ = client.disconnect().await;
        handle.abort();

        match result {
            Ok(_) => {
                info!(
                    node = %self.name,
                    topic = %self.topic,
                    bytes = payload.len(),
                    "MQTT publish successful"
                );
                let resp_payload = serde_json::json!({
                    "topic": self.topic,
                    "qos": self.qos,
                    "payload_size": payload.len(),
                });
                Ok(vec![msg.derive(msg.source_node, "published", resp_payload)])
            }
            Err(e) => {
                warn!(node = %self.name, error = %e, "MQTT publish failed");
                let err_payload = serde_json::json!({
                    "error": format!("MQTT publish failed: {}", e),
                    "topic": self.topic,
                });
                Ok(vec![msg.derive(msg.source_node, "error", err_payload)])
            }
        }
    }

    /// Subscribe mode: connect, subscribe to topic, wait for one message
    async fn handle_subscribe(&self, msg: FlowMessage) -> Z8Result<Vec<FlowMessage>> {
        info!(
            node = %self.name,
            broker = %self.broker,
            topic = %self.topic,
            "MQTT subscribe request"
        );

        // Build MQTT options
        let mut opts = MqttOptions::new(&self.client_id, &self.broker, self.port);
        opts.set_keep_alive(Duration::from_secs(self.keep_alive));

        if !self.username.is_empty() {
            opts.set_credentials(&self.username, &self.password);
        }

        if self.use_tls {
            opts.set_transport(rumqttc::Transport::tls_with_default_config());
        }

        // Create async client
        let (client, mut eventloop) = AsyncClient::new(opts, 10);

        // Subscribe to topic
        let qos = match self.qos {
            0 => QoS::AtMostOnce,
            1 => QoS::AtLeastOnce,
            _ => QoS::ExactlyOnce,
        };

        match client.subscribe(&self.topic, qos).await {
            Ok(_) => {
                info!(node = %self.name, topic = %self.topic, "MQTT subscribed");
            }
            Err(e) => {
                warn!(node = %self.name, error = %e, "MQTT subscribe failed");
                let err_payload = serde_json::json!({
                    "error": format!("MQTT subscribe failed: {}", e),
                    "topic": self.topic,
                });
                return Ok(vec![msg.derive(msg.source_node, "error", err_payload)]);
            }
        }

        // Wait for one message with timeout
        let timeout = Duration::from_millis(self.timeout_ms);
        let wait_result =
            tokio::time::timeout(timeout, self.poll_for_message(&mut eventloop)).await;

        match wait_result {
            Ok(Ok(Some((publish_payload, retain)))) => {
                info!(
                    node = %self.name,
                    topic = %self.topic,
                    bytes = publish_payload.len(),
                    "MQTT message received"
                );
                let payload_str = String::from_utf8_lossy(&publish_payload).to_string();
                let resp_payload = serde_json::json!({
                    "topic": self.topic,
                    "payload": payload_str,
                    "qos": self.qos,
                    "retain": retain,
                });
                Ok(vec![msg.derive(msg.source_node, "message", resp_payload)])
            }
            Ok(Ok(None)) => {
                warn!(node = %self.name, "MQTT poll failed: unexpected None");
                let err_payload = serde_json::json!({
                    "error": "MQTT polling ended unexpectedly",
                    "topic": self.topic,
                });
                Ok(vec![msg.derive(msg.source_node, "error", err_payload)])
            }
            Ok(Err(e)) => {
                warn!(node = %self.name, error = %e, "MQTT poll error");
                let err_payload = serde_json::json!({
                    "error": format!("MQTT poll error: {}", e),
                    "topic": self.topic,
                });
                Ok(vec![msg.derive(msg.source_node, "error", err_payload)])
            }
            Err(_) => {
                warn!(
                    node = %self.name,
                    timeout_ms = self.timeout_ms,
                    "MQTT receive timeout"
                );
                let err_payload = serde_json::json!({
                    "error": format!("MQTT receive timeout after {}ms", self.timeout_ms),
                    "topic": self.topic,
                });
                Ok(vec![msg.derive(msg.source_node, "error", err_payload)])
            }
        }
    }

    /// Poll eventloop until a Publish packet is received
    async fn poll_for_message(
        &self,
        eventloop: &mut rumqttc::EventLoop,
    ) -> Result<Option<(Vec<u8>, bool)>, String> {
        loop {
            match eventloop.poll().await {
                Ok(Event::Incoming(Packet::Publish(p))) => {
                    return Ok(Some((p.payload.to_vec(), p.retain)));
                }
                Ok(Event::Incoming(_)) => {
                    // Skip other packet types, keep polling
                    continue;
                }
                Ok(Event::Outgoing(_)) => {
                    // Skip outgoing events
                    continue;
                }
                Err(e) => {
                    return Err(format!("EventLoop error: {}", e));
                }
            }
        }
    }
}

/// Extract payload from message (similar to extract_prompt in LLM node)
fn extract_payload(payload: &serde_json::Value) -> String {
    // If payload is a string directly
    if let Some(s) = payload.as_str() {
        return s.to_string();
    }
    // Try common field names
    for key in &["payload", "text", "body", "message", "content", "input"] {
        if let Some(s) = payload.get(key).and_then(|v| v.as_str()) {
            return s.to_string();
        }
    }
    // Try nested: req.body.payload, req.body.text, etc.
    if let Some(body) = payload.get("req").and_then(|r| r.get("body")) {
        for key in &["payload", "text", "message", "content", "input"] {
            if let Some(s) = body.get(key).and_then(|v| v.as_str()) {
                return s.to_string();
            }
        }
        // If body is a string
        if let Some(s) = body.as_str() {
            return s.to_string();
        }
    }
    String::new()
}

pub struct MqttNodeFactory;

#[async_trait::async_trait]
impl NodeExecutorFactory for MqttNodeFactory {
    async fn create(&self, config: serde_json::Value) -> Z8Result<Box<dyn NodeExecutor>> {
        let mut node = MqttNode {
            name: "MQTT".to_string(),
            action: "publish".to_string(),
            broker: "localhost".to_string(),
            port: 1883,
            topic: "z8run/default".to_string(),
            qos: 0,
            client_id: format!(
                "z8run-{}",
                uuid::Uuid::new_v4().to_string()[..8].to_string()
            ),
            username: String::new(),
            password: String::new(),
            use_tls: false,
            keep_alive: 30,
            timeout_ms: 30000,
        };
        node.configure(config).await?;
        Ok(Box::new(node))
    }

    fn node_type(&self) -> &str {
        "mqtt"
    }
}
