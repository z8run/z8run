//! Flow execution engine.
//!
//! Receives a flow, compiles it into an execution plan,
//! and orchestrates concurrent node execution using Tokio.

use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::{broadcast, mpsc, RwLock};
use tracing::{debug, error, info, instrument, warn};
use uuid::Uuid;

use crate::error::{Z8Error, Z8Result};
use crate::flow::{Flow, FlowStatus};
use crate::message::FlowMessage;
use crate::scheduler::ExecutionPlan;

/// Event emitted by the engine during execution.
#[derive(Debug, Clone)]
pub enum EngineEvent {
    /// A flow started executing.
    FlowStarted { flow_id: Uuid, trace_id: Uuid },
    /// A node started processing.
    NodeStarted { flow_id: Uuid, node_id: Uuid },
    /// A node finished processing.
    NodeCompleted {
        flow_id: Uuid,
        node_id: Uuid,
        duration_us: u64,
        /// Truncated output payload for UI display (first output message).
        output_preview: Option<serde_json::Value>,
    },
    /// A node was skipped (received no message in a conditional branch).
    NodeSkipped { flow_id: Uuid, node_id: Uuid },
    /// A node failed.
    NodeError {
        flow_id: Uuid,
        node_id: Uuid,
        error: String,
    },
    /// A message was sent between nodes.
    MessageSent {
        flow_id: Uuid,
        from_node: Uuid,
        to_node: Uuid,
        message_id: Uuid,
        /// Truncated payload for UI display.
        payload_preview: Option<serde_json::Value>,
    },
    /// A streaming chunk from a node (e.g., LLM token).
    StreamChunk {
        flow_id: Uuid,
        node_id: Uuid,
        chunk: String,
        /// Whether this is the final chunk.
        done: bool,
    },
    /// A flow completed execution.
    FlowCompleted {
        flow_id: Uuid,
        trace_id: Uuid,
        duration_ms: u64,
    },
    /// A flow failed.
    FlowError {
        flow_id: Uuid,
        trace_id: Uuid,
        error: String,
    },
}

/// Trait implemented by all executable nodes.
/// Native nodes implement it directly;
/// WASM nodes implement it via the z8run-runtime.
#[async_trait::async_trait]
pub trait NodeExecutor: Send + Sync {
    /// Processes a message and returns zero or more output messages.
    async fn process(&self, msg: FlowMessage) -> Z8Result<Vec<FlowMessage>>;

    /// Initializes the node with its configuration.
    async fn configure(&mut self, config: serde_json::Value) -> Z8Result<()>;

    /// Validates the configuration before execution.
    async fn validate(&self) -> Z8Result<()>;

    /// Cleanup when stopping the node.
    async fn shutdown(&self) -> Z8Result<()> {
        Ok(())
    }

    /// Optionally provides an event emitter for streaming.
    /// Default implementation does nothing.
    fn set_event_emitter(&mut self, _tx: broadcast::Sender<EngineEvent>) {}

    /// Returns the name of the node type.
    fn node_type(&self) -> &str;
}

/// Execution state of an active flow.
struct ActiveFlow {
    _flow: Flow,
    _plan: ExecutionPlan,
    status: FlowStatus,
    _trace_id: Uuid,
}

/// z8run flow execution engine.
pub struct FlowEngine {
    /// Active flows currently executing.
    active_flows: Arc<RwLock<HashMap<Uuid, ActiveFlow>>>,
    /// Broadcast channel to emit engine events.
    event_tx: broadcast::Sender<EngineEvent>,
    /// Registry of node executors by type.
    node_registry: Arc<RwLock<HashMap<String, Arc<dyn NodeExecutorFactory>>>>,
    /// Channel buffer size between nodes.
    default_buffer_size: usize,
}

/// Factory that creates NodeExecutor instances for a node type.
#[async_trait::async_trait]
pub trait NodeExecutorFactory: Send + Sync {
    /// Creates a new executor instance with the given configuration.
    async fn create(&self, config: serde_json::Value) -> Z8Result<Box<dyn NodeExecutor>>;
    /// Returns the node type that this factory produces.
    fn node_type(&self) -> &str;
}

/// Truncate a JSON payload for UI preview (max ~500 chars).
/// Deeply nested objects get replaced with a summary.
fn truncate_payload(value: &serde_json::Value) -> serde_json::Value {
    let s = value.to_string();
    if s.len() <= 500 {
        return value.clone();
    }
    // For large payloads, show top-level keys with truncated values
    if let serde_json::Value::Object(map) = value {
        let mut preview = serde_json::Map::new();
        for (k, v) in map.iter().take(10) {
            let vs = v.to_string();
            if vs.len() > 100 {
                preview.insert(
                    k.clone(),
                    serde_json::Value::String(format!("{}...", &vs[..97])),
                );
            } else {
                preview.insert(k.clone(), v.clone());
            }
        }
        if map.len() > 10 {
            preview.insert(
                "_truncated".to_string(),
                serde_json::Value::String(format!("...and {} more keys", map.len() - 10)),
            );
        }
        serde_json::Value::Object(preview)
    } else {
        // For non-objects, just truncate the string
        serde_json::Value::String(format!("{}...", &s[..497]))
    }
}

impl FlowEngine {
    /// Creates a new flow engine.
    pub fn new() -> Self {
        let (event_tx, _) = broadcast::channel(1024);
        Self {
            active_flows: Arc::new(RwLock::new(HashMap::new())),
            event_tx,
            node_registry: Arc::new(RwLock::new(HashMap::new())),
            default_buffer_size: 256,
        }
    }

    /// Configures the backpressure buffer size.
    pub fn with_buffer_size(mut self, size: usize) -> Self {
        self.default_buffer_size = size;
        self
    }

    /// Registers a node factory for a specific type.
    pub async fn register_node_type(&self, factory: Arc<dyn NodeExecutorFactory>) {
        let node_type = factory.node_type().to_string();
        info!(node_type = %node_type, "Registering node type");
        self.node_registry.write().await.insert(node_type, factory);
    }

    /// Subscribes to engine events.
    pub fn subscribe_events(&self) -> broadcast::Receiver<EngineEvent> {
        self.event_tx.subscribe()
    }

    /// Compiles and executes a flow.
    #[instrument(skip(self, flow), fields(flow_id = %flow.id, flow_name = %flow.name))]
    pub async fn execute(&self, flow: Flow) -> Z8Result<Uuid> {
        self.execute_with_trigger(flow, None).await
    }

    /// Compiles and executes a flow with an optional trigger message.
    /// When `trigger_msg` is provided, root nodes receive it instead of generating a default one.
    #[instrument(skip(self, flow, trigger_msg), fields(flow_id = %flow.id, flow_name = %flow.name))]
    pub async fn execute_with_trigger(
        &self,
        flow: Flow,
        trigger_msg: Option<FlowMessage>,
    ) -> Z8Result<Uuid> {
        let trace_id = trigger_msg
            .as_ref()
            .map(|m| m.trace_id)
            .unwrap_or_else(Uuid::now_v7);
        let flow_id = flow.id;

        info!("Compiling execution plan");
        let plan = ExecutionPlan::compile(&flow)?;
        info!(
            steps = plan.depth(),
            parallelism = plan.max_parallelism(),
            nodes = plan.total_nodes,
            "Execution plan compiled"
        );

        // Register flow as active
        {
            let mut active = self.active_flows.write().await;
            active.insert(
                flow_id,
                ActiveFlow {
                    _flow: flow.clone(),
                    _plan: plan.clone(),
                    status: FlowStatus::Running,
                    _trace_id: trace_id,
                },
            );
        }

        // Emit startup event
        let _ = self
            .event_tx
            .send(EngineEvent::FlowStarted { flow_id, trace_id });

        let engine = self.clone_refs();
        let flow_clone = flow.clone();

        // Execute in background
        tokio::spawn(async move {
            let start = std::time::Instant::now();

            match engine
                .execute_plan(&flow_clone, &plan, trace_id, trigger_msg.as_ref())
                .await
            {
                Ok(()) => {
                    let duration_ms = start.elapsed().as_millis() as u64;
                    info!(duration_ms, "Flow completed successfully");
                    let _ = engine.event_tx.send(EngineEvent::FlowCompleted {
                        flow_id,
                        trace_id,
                        duration_ms,
                    });
                    engine.set_flow_status(flow_id, FlowStatus::Completed).await;
                }
                Err(e) => {
                    error!(error = %e, "Flow failed");
                    let _ = engine.event_tx.send(EngineEvent::FlowError {
                        flow_id,
                        trace_id,
                        error: e.to_string(),
                    });
                    engine.set_flow_status(flow_id, FlowStatus::Error).await;
                }
            }
        });

        Ok(trace_id)
    }

    /// Executes the plan step by step.
    async fn execute_plan(
        &self,
        flow: &Flow,
        plan: &ExecutionPlan,
        trace_id: Uuid,
        trigger_msg: Option<&FlowMessage>,
    ) -> Z8Result<()> {
        // Communication channels between nodes: node_id -> sender
        let mut channels: HashMap<Uuid, mpsc::Sender<FlowMessage>> = HashMap::new();
        let mut receivers: HashMap<Uuid, mpsc::Receiver<FlowMessage>> = HashMap::new();

        // Determine which nodes have incoming edges (non-root nodes)
        let nodes_with_incoming: std::collections::HashSet<Uuid> =
            flow.edges.iter().map(|e| e.to_node).collect();

        // Create channels ONLY for nodes that have incoming edges.
        // Root nodes (no incoming edges) won't get a receiver,
        // so they'll take the "generate trigger message" path.
        for node in &flow.nodes {
            if node.enabled && nodes_with_incoming.contains(&node.id) {
                let (tx, rx) = mpsc::channel(flow.config.buffer_size.max(self.default_buffer_size));
                channels.insert(node.id, tx);
                receivers.insert(node.id, rx);
            }
        }

        // Execute each step of the plan
        for step in &plan.steps {
            debug!(
                step = step.step,
                nodes = step.node_ids.len(),
                "Executing step"
            );

            let mut handles = Vec::new();

            for &node_id in &step.node_ids {
                let node = flow.find_node(node_id).ok_or(Z8Error::NodeNotFound {
                    flow_id: flow.id,
                    node_id,
                })?;

                let flow_id = flow.id;
                let event_tx = self.event_tx.clone();
                let node_type_str = node.node_type.0.clone();
                let node_config = node.config.clone();

                // Get the receiver for this node
                let mut rx = receivers.remove(&node_id);

                // Get the senders for target nodes
                let outgoing = flow.outgoing_edges(node_id);
                let out_channels: Vec<(String, Uuid, mpsc::Sender<FlowMessage>)> = outgoing
                    .iter()
                    .filter_map(|edge| {
                        channels
                            .get(&edge.to_node)
                            .map(|tx| (edge.from_port.clone(), edge.to_node, tx.clone()))
                    })
                    .collect();

                let registry = self.node_registry.clone();
                let trigger_clone = trigger_msg.cloned();

                let handle = tokio::spawn(async move {
                    let start = std::time::Instant::now();

                    // If it is a root node (no receiver), generate an initial message
                    let messages = if let Some(ref mut receiver) = rx {
                        // Receive a message from the channel
                        match receiver.recv().await {
                            Some(msg) => {
                                // Node will process — emit NodeStarted
                                let _ =
                                    event_tx.send(EngineEvent::NodeStarted { flow_id, node_id });

                                let reg = registry.read().await;
                                let factory = reg.get(&node_type_str).ok_or_else(|| {
                                    Z8Error::Internal(format!(
                                        "No executor registered for type '{}'",
                                        node_type_str
                                    ))
                                })?;
                                let mut executor = factory.create(node_config).await?;
                                executor.set_event_emitter(event_tx.clone());
                                executor.process(msg).await?
                            }
                            None => {
                                // Channel closed — this node is on an inactive branch.
                                // Emit NodeSkipped instead of NodeStarted + NodeCompleted.
                                debug!(node_id = %node_id, "Node skipped (no message received)");
                                let _ =
                                    event_tx.send(EngineEvent::NodeSkipped { flow_id, node_id });
                                return Ok(());
                            }
                        }
                    } else {
                        // Root node: always processes
                        let _ = event_tx.send(EngineEvent::NodeStarted { flow_id, node_id });

                        let reg = registry.read().await;
                        let factory = reg.get(&node_type_str).ok_or_else(|| {
                            Z8Error::Internal(format!(
                                "No executor registered for type '{}'",
                                node_type_str
                            ))
                        })?;
                        let mut executor = factory.create(node_config).await?;
                        executor.set_event_emitter(event_tx.clone());

                        let root_msg = if let Some(ref tmsg) = trigger_clone {
                            let mut m = tmsg.clone();
                            m.source_node = node_id;
                            m
                        } else {
                            FlowMessage::new(
                                node_id,
                                "trigger",
                                serde_json::json!({"triggered": true}),
                                trace_id,
                            )
                        };
                        executor.process(root_msg).await?
                    };

                    // Capture the first output payload for UI preview
                    let output_preview = messages.first().map(|m| truncate_payload(&m.payload));

                    // Send messages to target nodes
                    for msg in &messages {
                        for (port, to_node, tx) in &out_channels {
                            if msg.source_port == *port || out_channels.len() == 1 {
                                let preview = truncate_payload(&msg.payload);
                                let _ = event_tx.send(EngineEvent::MessageSent {
                                    flow_id,
                                    from_node: node_id,
                                    to_node: *to_node,
                                    message_id: msg.id,
                                    payload_preview: Some(preview),
                                });
                                if tx.send(msg.clone()).await.is_err() {
                                    warn!("Channel closed when sending to node {}", to_node);
                                }
                            }
                        }
                    }

                    let duration_us = start.elapsed().as_micros() as u64;
                    let _ = event_tx.send(EngineEvent::NodeCompleted {
                        flow_id,
                        node_id,
                        duration_us,
                        output_preview,
                    });

                    Ok::<(), Z8Error>(())
                });

                handles.push(handle);
            }

            // Wait for all nodes in the step to complete
            for handle in handles {
                match handle.await {
                    Ok(Ok(())) => {}
                    Ok(Err(e)) => return Err(e),
                    Err(e) => {
                        return Err(Z8Error::Internal(format!("Task panicked: {}", e)));
                    }
                }
            }

            // Drop the original senders for this step's outgoing targets.
            // The spawned tasks already cloned what they needed; those clones
            // are now dropped too (tasks finished). By removing the originals
            // here, downstream nodes that received NO message will see their
            // channel close → recv() returns None → they complete gracefully.
            // This is critical for conditional routing (switch/filter) where
            // only one branch receives a message.
            for &node_id in &step.node_ids {
                let outgoing = flow.outgoing_edges(node_id);
                for edge in &outgoing {
                    channels.remove(&edge.to_node);
                }
            }
        }

        Ok(())
    }

    /// Stops the execution of a flow.
    pub async fn stop(&self, flow_id: Uuid) -> Z8Result<()> {
        self.set_flow_status(flow_id, FlowStatus::Stopped).await;
        info!(flow_id = %flow_id, "Flow stopped");
        Ok(())
    }

    /// Returns the state of an active flow.
    pub async fn flow_status(&self, flow_id: Uuid) -> Option<FlowStatus> {
        self.active_flows
            .read()
            .await
            .get(&flow_id)
            .map(|af| af.status.clone())
    }

    /// Returns the IDs of all active flows.
    pub async fn active_flow_ids(&self) -> Vec<Uuid> {
        self.active_flows.read().await.keys().cloned().collect()
    }

    async fn set_flow_status(&self, flow_id: Uuid, status: FlowStatus) {
        if let Some(af) = self.active_flows.write().await.get_mut(&flow_id) {
            af.status = status;
        }
    }

    fn clone_refs(&self) -> Self {
        Self {
            active_flows: Arc::clone(&self.active_flows),
            event_tx: self.event_tx.clone(),
            node_registry: Arc::clone(&self.node_registry),
            default_buffer_size: self.default_buffer_size,
        }
    }
}

impl Default for FlowEngine {
    fn default() -> Self {
        Self::new()
    }
}
