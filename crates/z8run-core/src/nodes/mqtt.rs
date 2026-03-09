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

use crate::configure_fields;
use crate::engine::{NodeExecutor, NodeExecutorFactory};
use crate::error::Z8Result;
use crate::message::FlowMessage;
use crate::utils::node_helpers::{
    error_output, error_output_with_context, require_non_empty, require_one_of,
};
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
                return Ok(error_output(
                    &msg,
                    &format!(
                        "Unknown MQTT action: {}. Expected 'publish' or 'subscribe'",
                        self.action
                    ),
                ));
            }
        }
    }

    async fn configure(&mut self, config: serde_json::Value) -> Z8Result<()> {
        configure_fields!(config, self,
            "name" => name: str,
            "action" => action: str,
            "broker" => broker: str,
            "topic" => topic: str,
            "clientId" => client_id: str,
            "username" => username: str,
            "password" => password: str,
            "useTls" => use_tls: bool,
            "keepAlive" => keep_alive: u64,
            "timeout" => timeout_ms: u64,
        );

        if let Some(v) = config.get("port").and_then(|v| v.as_u64()) {
            self.port = v as u16;
        }
        if let Some(v) = config.get("qos").and_then(|v| v.as_u64()) {
            self.qos = (v as u8).min(2);
        }
        Ok(())
    }

    async fn validate(&self) -> Z8Result<()> {
        require_one_of(
            &self.action,
            &["subscribe", "publish"],
            "Invalid MQTT action",
        )?;
        require_non_empty(&self.broker, "MQTT broker cannot be empty")?;
        require_non_empty(&self.topic, "MQTT topic cannot be empty")?;
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
            return Ok(error_output(&msg, "No payload found in message. Expected string payload or 'payload'/'body'/'text' field"));
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
        let handle = tokio::spawn(async move { while eventloop.poll().await.is_ok() {} });

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
                Ok(error_output_with_context(
                    &msg,
                    &format!("MQTT publish failed: {}", e),
                    serde_json::json!({"topic": self.topic}),
                ))
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
                return Ok(error_output_with_context(
                    &msg,
                    &format!("MQTT subscribe failed: {}", e),
                    serde_json::json!({"topic": self.topic}),
                ));
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
                Ok(error_output_with_context(
                    &msg,
                    "MQTT polling ended unexpectedly",
                    serde_json::json!({"topic": self.topic}),
                ))
            }
            Ok(Err(e)) => {
                warn!(node = %self.name, error = %e, "MQTT poll error");
                Ok(error_output_with_context(
                    &msg,
                    &format!("MQTT poll error: {}", e),
                    serde_json::json!({"topic": self.topic}),
                ))
            }
            Err(_) => {
                warn!(
                    node = %self.name,
                    timeout_ms = self.timeout_ms,
                    "MQTT receive timeout"
                );
                Ok(error_output_with_context(
                    &msg,
                    &format!("MQTT receive timeout after {}ms", self.timeout_ms),
                    serde_json::json!({"topic": self.topic}),
                ))
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
            client_id: format!("z8run-{}", &uuid::Uuid::new_v4().to_string()[..8]),
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
