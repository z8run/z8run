//! Human Handoff node: manages escalation from AI agent to human agent.
//!
//! Handles ticket creation, status tracking, assignment, and resolution
//! for call center and support flow escalations. Integrates with external
//! systems via optional webhooks.
//!
//! Features:
//! - Ticket escalation with automatic ID generation
//! - Ticket status tracking (pending, assigned, resolved)
//! - Agent assignment management
//! - Webhook notifications for external system integration
//! - Priority-based queuing (low, medium, high, urgent)
//!
//! Config example:
//! ```json
//! {
//!   "action": "escalate",
//!   "webhookUrl": "https://crm.example.com/webhooks/escalations",
//!   "priority": "high",
//!   "timeoutMs": 300000
//! }
//! ```

use crate::engine::{NodeExecutor, NodeExecutorFactory};
use crate::error::Z8Result;
use crate::message::FlowMessage;
use serde_json::{json, Value};
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;
use tracing::{debug, info, warn};

/// Ticket data structure for tracking support/escalation tickets.
#[derive(Debug, Clone)]
pub struct TicketData {
    /// Unique ticket identifier.
    pub id: String,
    /// Associated conversation or session ID.
    pub conversation_id: String,
    /// Reason for escalation or ticket creation.
    pub reason: String,
    /// Priority level (low, medium, high, urgent).
    pub priority: String,
    /// Current ticket status (pending, assigned, resolved).
    pub status: String,
    /// Agent assigned to handle this ticket, if any.
    pub assigned_to: Option<String>,
    /// Unix timestamp of ticket creation.
    pub created_at: u64,
    /// Unix timestamp of last update.
    pub updated_at: u64,
    /// Additional metadata (context, customer info, etc.).
    pub metadata: Value,
    /// Message history for this ticket.
    pub messages: Vec<Value>,
}

impl TicketData {
    /// Serializes the ticket to a JSON value.
    pub fn to_json(&self) -> Value {
        json!({
            "id": self.id,
            "conversationId": self.conversation_id,
            "reason": self.reason,
            "priority": self.priority,
            "status": self.status,
            "assignedTo": self.assigned_to,
            "createdAt": self.created_at,
            "updatedAt": self.updated_at,
            "metadata": self.metadata,
            "messages": self.messages,
        })
    }
}

/// Human Handoff node for managing escalations.
pub struct HumanHandoffNode {
    /// Node name.
    name: String,
    /// Action type: "escalate", "check_status", "resolve", or "assign".
    action: String,
    /// Optional webhook URL to notify external systems.
    webhook_url: Option<String>,
    /// Default priority level for new tickets.
    priority: String,
    /// Timeout in milliseconds for the escalation.
    timeout_ms: u64,
    /// Queue storing all tickets by ID.
    queue: Arc<RwLock<HashMap<String, TicketData>>>,
}

impl HumanHandoffNode {
    /// Creates a new HumanHandoffNode.
    pub fn new(name: String) -> Self {
        Self {
            name,
            action: "escalate".to_string(),
            webhook_url: None,
            priority: "medium".to_string(),
            timeout_ms: 300000,
            queue: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    /// Generates a unique ticket ID using timestamp and hash.
    fn generate_ticket_id() -> String {
        use std::collections::hash_map::DefaultHasher;
        use std::hash::{Hash, Hasher};

        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default();

        let timestamp = now.as_millis();
        let nonce = now.as_nanos();

        let mut hasher = DefaultHasher::new();
        nonce.hash(&mut hasher);
        let hash = hasher.finish();
        let hash_hex = format!("{:04x}", hash & 0xFFFF);

        format!("tkt-{}-{}", timestamp, hash_hex)
    }

    /// Extracts a value from payload using multiple possible field names.
    fn extract_field(payload: &Value, field_names: &[&str]) -> Option<String> {
        for name in field_names {
            if let Some(val) = payload.get(name) {
                if let Some(s) = val.as_str() {
                    return Some(s.to_string());
                }
                return Some(val.to_string());
            }
        }
        None
    }

    /// Handles the "escalate" action: creates a new ticket.
    async fn handle_escalate(&self, msg: FlowMessage) -> Z8Result<Vec<FlowMessage>> {
        let payload = &msg.payload;

        // Extract fields with fallback names
        let conversation_id = Self::extract_field(payload, &["conversationId", "conversation_id", "sessionId"])
            .unwrap_or_else(|| uuid::Uuid::now_v7().to_string());

        let reason = Self::extract_field(payload, &["reason", "escalationReason", "issue", "description"])
            .unwrap_or_else(|| "No reason provided".to_string());

        let priority = Self::extract_field(payload, &["priority"])
            .unwrap_or_else(|| self.priority.clone());

        let customer = Self::extract_field(payload, &["customer", "customerName", "customer_name"]);

        let metadata = payload.get("metadata")
            .or_else(|| payload.get("context"))
            .cloned()
            .unwrap_or(Value::Null);

        // Create new ticket
        let ticket_id = Self::generate_ticket_id();
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();

        let mut ticket = TicketData {
            id: ticket_id.clone(),
            conversation_id,
            reason,
            priority: priority.clone(),
            status: "pending".to_string(),
            assigned_to: None,
            created_at: now,
            updated_at: now,
            metadata,
            messages: vec![],
        };

        if let Some(cust) = customer {
            ticket.metadata["customer"] = json!(cust);
        }

        // Store in queue
        {
            let mut queue = self.queue.write().await;
            queue.insert(ticket_id.clone(), ticket.clone());
        }

        info!(node = %self.name, ticket_id = %ticket_id, "Escalation ticket created");

        // Send webhook notification if configured
        if let Some(webhook_url) = &self.webhook_url {
            self.notify_webhook("escalated", &ticket, webhook_url).await;
        }

        // Output on "escalated" port
        let out = msg.derive(msg.source_node, "escalated", ticket.to_json());
        Ok(vec![out])
    }

    /// Handles the "check_status" action: looks up ticket status.
    async fn handle_check_status(&self, msg: FlowMessage) -> Z8Result<Vec<FlowMessage>> {
        let payload = &msg.payload;

        let ticket_id = Self::extract_field(payload, &["ticketId", "ticket_id", "id"])
            .unwrap_or_default();

        if ticket_id.is_empty() {
            let err = json!({
                "error": "No ticket ID provided",
                "received": payload,
            });
            let out = msg.derive(msg.source_node, "error", err);
            return Ok(vec![out]);
        }

        let queue = self.queue.read().await;
        match queue.get(&ticket_id) {
            Some(ticket) => {
                debug!(node = %self.name, ticket_id = %ticket_id, "Ticket status retrieved");
                let out = msg.derive(msg.source_node, "status", ticket.to_json());
                Ok(vec![out])
            }
            None => {
                warn!(node = %self.name, ticket_id = %ticket_id, "Ticket not found");
                let err = json!({
                    "error": "Ticket not found",
                    "ticketId": ticket_id,
                });
                let out = msg.derive(msg.source_node, "error", err);
                Ok(vec![out])
            }
        }
    }

    /// Handles the "resolve" action: marks a ticket as resolved.
    async fn handle_resolve(&self, msg: FlowMessage) -> Z8Result<Vec<FlowMessage>> {
        let payload = &msg.payload;

        let ticket_id = Self::extract_field(payload, &["ticketId", "ticket_id", "id"])
            .unwrap_or_default();

        if ticket_id.is_empty() {
            let err = json!({
                "error": "No ticket ID provided",
            });
            let out = msg.derive(msg.source_node, "error", err);
            return Ok(vec![out]);
        }

        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();

        let mut queue = self.queue.write().await;
        match queue.get_mut(&ticket_id) {
            Some(ticket) => {
                ticket.status = "resolved".to_string();
                ticket.updated_at = now;
                let ticket_copy = ticket.clone();
                drop(queue);

                info!(node = %self.name, ticket_id = %ticket_id, "Ticket resolved");

                // Send webhook notification if configured
                if let Some(webhook_url) = &self.webhook_url {
                    self.notify_webhook("resolved", &ticket_copy, webhook_url).await;
                }

                let out = msg.derive(msg.source_node, "resolved", ticket_copy.to_json());
                Ok(vec![out])
            }
            None => {
                warn!(node = %self.name, ticket_id = %ticket_id, "Ticket not found for resolution");
                let err = json!({
                    "error": "Ticket not found",
                    "ticketId": ticket_id,
                });
                let out = msg.derive(msg.source_node, "error", err);
                Ok(vec![out])
            }
        }
    }

    /// Handles the "assign" action: assigns a ticket to an agent.
    async fn handle_assign(&self, msg: FlowMessage) -> Z8Result<Vec<FlowMessage>> {
        let payload = &msg.payload;

        let ticket_id = Self::extract_field(payload, &["ticketId", "ticket_id", "id"])
            .unwrap_or_default();

        let agent = Self::extract_field(payload, &["agentId", "agent_id", "agentName", "agent_name", "assignee"])
            .unwrap_or_default();

        if ticket_id.is_empty() {
            let err = json!({
                "error": "No ticket ID provided",
            });
            let out = msg.derive(msg.source_node, "error", err);
            return Ok(vec![out]);
        }

        if agent.is_empty() {
            let err = json!({
                "error": "No agent ID provided",
            });
            let out = msg.derive(msg.source_node, "error", err);
            return Ok(vec![out]);
        }

        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();

        let mut queue = self.queue.write().await;
        match queue.get_mut(&ticket_id) {
            Some(ticket) => {
                ticket.assigned_to = Some(agent.clone());
                ticket.status = "assigned".to_string();
                ticket.updated_at = now;
                let ticket_copy = ticket.clone();
                drop(queue);

                info!(node = %self.name, ticket_id = %ticket_id, agent = %agent, "Ticket assigned");

                // Send webhook notification if configured
                if let Some(webhook_url) = &self.webhook_url {
                    self.notify_webhook("assigned", &ticket_copy, webhook_url).await;
                }

                let out = msg.derive(msg.source_node, "assigned", ticket_copy.to_json());
                Ok(vec![out])
            }
            None => {
                warn!(node = %self.name, ticket_id = %ticket_id, "Ticket not found for assignment");
                let err = json!({
                    "error": "Ticket not found",
                    "ticketId": ticket_id,
                });
                let out = msg.derive(msg.source_node, "error", err);
                Ok(vec![out])
            }
        }
    }

    /// Sends a webhook notification to an external system.
    async fn notify_webhook(&self, event: &str, ticket: &TicketData, webhook_url: &str) {
        let payload = json!({
            "event": event,
            "ticket": ticket.to_json(),
            "timestamp": chrono::Utc::now().to_rfc3339(),
        });

        let client = reqwest::Client::new();
        match client
            .post(webhook_url)
            .json(&payload)
            .timeout(std::time::Duration::from_millis(5000))
            .send()
            .await
        {
            Ok(resp) => {
                if resp.status().is_success() {
                    debug!(node = %self.name, event = event, "Webhook notification sent successfully");
                } else {
                    warn!(node = %self.name, event = event, status = %resp.status(), "Webhook returned non-success status");
                }
            }
            Err(e) => {
                warn!(node = %self.name, event = event, error = %e, "Failed to send webhook notification");
            }
        }
    }
}

#[async_trait::async_trait]
impl NodeExecutor for HumanHandoffNode {
    async fn process(&self, msg: FlowMessage) -> Z8Result<Vec<FlowMessage>> {
        debug!(node = %self.name, action = %self.action, "Processing human handoff request");

        match self.action.as_str() {
            "escalate" => self.handle_escalate(msg).await,
            "check_status" => self.handle_check_status(msg).await,
            "resolve" => self.handle_resolve(msg).await,
            "assign" => self.handle_assign(msg).await,
            other => {
                warn!(node = %self.name, action = other, "Unknown action");
                let err = json!({
                    "error": format!("Unknown action: {}", other),
                });
                let out = msg.derive(msg.source_node, "error", err);
                Ok(vec![out])
            }
        }
    }

    async fn configure(&mut self, config: Value) -> Z8Result<()> {
        if let Some(name) = config.get("name").and_then(|v| v.as_str()) {
            self.name = name.to_string();
        }
        if let Some(action) = config.get("action").and_then(|v| v.as_str()) {
            self.action = action.to_string();
        }
        if let Some(webhook) = config.get("webhookUrl").and_then(|v| v.as_str()) {
            self.webhook_url = Some(webhook.to_string());
        }
        if let Some(priority) = config.get("priority").and_then(|v| v.as_str()) {
            self.priority = priority.to_string();
        }
        if let Some(timeout) = config.get("timeoutMs").and_then(|v| v.as_u64()) {
            self.timeout_ms = timeout;
        }
        Ok(())
    }

    async fn validate(&self) -> Z8Result<()> {
        match self.action.as_str() {
            "escalate" | "check_status" | "resolve" | "assign" => Ok(()),
            other => Err(crate::error::Z8Error::Internal(
                format!("Invalid action '{}'. Must be one of: escalate, check_status, resolve, assign", other),
            )),
        }
    }

    fn node_type(&self) -> &str {
        "human-handoff"
    }
}

pub struct HumanHandoffNodeFactory;

#[async_trait::async_trait]
impl NodeExecutorFactory for HumanHandoffNodeFactory {
    async fn create(&self, config: Value) -> Z8Result<Box<dyn NodeExecutor>> {
        let mut node = HumanHandoffNode::new("HumanHandoff".to_string());
        node.configure(config).await?;
        Ok(Box::new(node))
    }

    fn node_type(&self) -> &str {
        "human-handoff"
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_generate_ticket_id() {
        let id1 = HumanHandoffNode::generate_ticket_id();
        let id2 = HumanHandoffNode::generate_ticket_id();

        assert!(id1.starts_with("tkt-"));
        assert!(id2.starts_with("tkt-"));
        assert_ne!(id1, id2); // IDs should be unique
    }

    #[test]
    fn test_extract_field_with_fallbacks() {
        let payload = json!({
            "conversationId": "conv-123"
        });
        let result = HumanHandoffNode::extract_field(&payload, &["conversationId", "conversation_id", "sessionId"]);
        assert_eq!(result, Some("conv-123".to_string()));
    }

    #[test]
    fn test_extract_field_fallback_to_second() {
        let payload = json!({
            "conversation_id": "conv-456"
        });
        let result = HumanHandoffNode::extract_field(&payload, &["conversationId", "conversation_id"]);
        assert_eq!(result, Some("conv-456".to_string()));
    }

    #[test]
    fn test_extract_field_not_found() {
        let payload = json!({
            "other": "value"
        });
        let result = HumanHandoffNode::extract_field(&payload, &["conversationId", "conversation_id"]);
        assert_eq!(result, None);
    }

    #[test]
    fn test_ticket_data_to_json() {
        let ticket = TicketData {
            id: "tkt-12345".to_string(),
            conversation_id: "conv-789".to_string(),
            reason: "Billing issue".to_string(),
            priority: "high".to_string(),
            status: "pending".to_string(),
            assigned_to: None,
            created_at: 1000,
            updated_at: 1000,
            metadata: json!({"customer": "John Doe"}),
            messages: vec![],
        };

        let json_val = ticket.to_json();
        assert_eq!(json_val["id"], "tkt-12345");
        assert_eq!(json_val["conversationId"], "conv-789");
        assert_eq!(json_val["reason"], "Billing issue");
        assert_eq!(json_val["priority"], "high");
        assert_eq!(json_val["status"], "pending");
    }

    #[tokio::test]
    async fn test_escalate_creates_ticket() {
        let node = HumanHandoffNode::new("test".to_string());

        let msg = FlowMessage::new(
            uuid::Uuid::now_v7(),
            "input",
            json!({
                "conversationId": "conv-123",
                "reason": "Customer angry",
                "priority": "urgent",
            }),
            uuid::Uuid::now_v7(),
        );

        let results = node.process(msg.clone()).await.unwrap();
        assert_eq!(results[0].source_port, "escalated");
        assert_eq!(results[0].payload["conversationId"], "conv-123");
        assert_eq!(results[0].payload["reason"], "Customer angry");
        assert_eq!(results[0].payload["status"], "pending");

        // Verify ticket was stored in queue
        let queue = node.queue.read().await;
        let ticket_id = results[0].payload["id"].as_str().unwrap();
        assert!(queue.contains_key(ticket_id));
    }

    #[tokio::test]
    async fn test_check_status_finds_ticket() {
        let node = HumanHandoffNode::new("test".to_string());

        // First, create a ticket
        let create_msg = FlowMessage::new(
            uuid::Uuid::now_v7(),
            "input",
            json!({
                "conversationId": "conv-123",
                "reason": "Test escalation",
            }),
            uuid::Uuid::now_v7(),
        );

        let create_results = node.process(create_msg).await.unwrap();
        let ticket_id = create_results[0].payload["id"].as_str().unwrap().to_string();

        // Now check status
        let status_msg = FlowMessage::new(
            uuid::Uuid::now_v7(),
            "input",
            json!({
                "ticketId": ticket_id.clone(),
            }),
            uuid::Uuid::now_v7(),
        );

        // Reconfigure node for check_status action
        let mut check_node = HumanHandoffNode::new("test".to_string());
        check_node.action = "check_status".to_string();
        check_node.queue = node.queue.clone();

        let results = check_node.process(status_msg).await.unwrap();
        assert_eq!(results[0].source_port, "status");
        assert_eq!(results[0].payload["id"], ticket_id);
        assert_eq!(results[0].payload["status"], "pending");
    }

    #[tokio::test]
    async fn test_check_status_not_found() {
        let node = HumanHandoffNode::new("test".to_string());
        let mut check_node = HumanHandoffNode::new("test".to_string());
        check_node.action = "check_status".to_string();

        let msg = FlowMessage::new(
            uuid::Uuid::now_v7(),
            "input",
            json!({
                "ticketId": "nonexistent-ticket",
            }),
            uuid::Uuid::now_v7(),
        );

        let results = check_node.process(msg).await.unwrap();
        assert_eq!(results[0].source_port, "error");
        assert!(results[0].payload["error"].as_str().unwrap().contains("not found"));
    }

    #[tokio::test]
    async fn test_assign_ticket() {
        let node = HumanHandoffNode::new("test".to_string());

        // Create a ticket
        let create_msg = FlowMessage::new(
            uuid::Uuid::now_v7(),
            "input",
            json!({
                "conversationId": "conv-123",
                "reason": "Support needed",
            }),
            uuid::Uuid::now_v7(),
        );

        let create_results = node.process(create_msg).await.unwrap();
        let ticket_id = create_results[0].payload["id"].as_str().unwrap().to_string();

        // Assign the ticket
        let assign_msg = FlowMessage::new(
            uuid::Uuid::now_v7(),
            "input",
            json!({
                "ticketId": ticket_id.clone(),
                "agentId": "agent-001",
            }),
            uuid::Uuid::now_v7(),
        );

        let mut assign_node = HumanHandoffNode::new("test".to_string());
        assign_node.action = "assign".to_string();
        assign_node.queue = node.queue.clone();

        let results = assign_node.process(assign_msg).await.unwrap();
        assert_eq!(results[0].source_port, "assigned");
        assert_eq!(results[0].payload["assignedTo"], "agent-001");
        assert_eq!(results[0].payload["status"], "assigned");
    }

    #[tokio::test]
    async fn test_resolve_ticket() {
        let node = HumanHandoffNode::new("test".to_string());

        // Create a ticket
        let create_msg = FlowMessage::new(
            uuid::Uuid::now_v7(),
            "input",
            json!({
                "conversationId": "conv-123",
                "reason": "Issue resolved",
            }),
            uuid::Uuid::now_v7(),
        );

        let create_results = node.process(create_msg).await.unwrap();
        let ticket_id = create_results[0].payload["id"].as_str().unwrap().to_string();

        // Resolve the ticket
        let resolve_msg = FlowMessage::new(
            uuid::Uuid::now_v7(),
            "input",
            json!({
                "ticketId": ticket_id,
            }),
            uuid::Uuid::now_v7(),
        );

        let mut resolve_node = HumanHandoffNode::new("test".to_string());
        resolve_node.action = "resolve".to_string();
        resolve_node.queue = node.queue.clone();

        let results = resolve_node.process(resolve_msg).await.unwrap();
        assert_eq!(results[0].source_port, "resolved");
        assert_eq!(results[0].payload["status"], "resolved");
    }

    #[tokio::test]
    async fn test_validate_action() {
        let mut node = HumanHandoffNode::new("test".to_string());
        node.action = "escalate".to_string();
        assert!(node.validate().await.is_ok());

        node.action = "invalid_action".to_string();
        assert!(node.validate().await.is_err());
    }

    #[tokio::test]
    async fn test_configure_node() {
        let mut node = HumanHandoffNode::new("test".to_string());
        let config = json!({
            "name": "MyHandoff",
            "action": "assign",
            "priority": "high",
            "timeoutMs": 600000,
            "webhookUrl": "https://example.com/webhook"
        });

        node.configure(config).await.unwrap();
        assert_eq!(node.name, "MyHandoff");
        assert_eq!(node.action, "assign");
        assert_eq!(node.priority, "high");
        assert_eq!(node.timeout_ms, 600000);
        assert_eq!(node.webhook_url, Some("https://example.com/webhook".to_string()));
    }

    #[test]
    fn test_node_type() {
        let node = HumanHandoffNode::new("test".to_string());
        assert_eq!(node.node_type(), "human-handoff");
    }

    #[tokio::test]
    async fn test_factory_creates_node() {
        let factory = HumanHandoffNodeFactory;
        let config = json!({
            "name": "MyHandoff",
            "action": "escalate",
        });

        let node = factory.create(config).await.unwrap();
        assert_eq!(node.node_type(), "human-handoff");
    }
}
