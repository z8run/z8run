//! Flow execution engine.
//!
//! Receives a flow, compiles it into an execution plan,
//! and orchestrates concurrent node execution using Tokio.

use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use tokio::sync::{broadcast, mpsc, Notify, RwLock};
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

impl EngineEvent {
    /// Returns the id of the flow this event belongs to.
    ///
    /// Used to route events to the right client (e.g. per-user WebSocket
    /// filtering) without matching every variant at the call site.
    pub fn flow_id(&self) -> Uuid {
        match self {
            EngineEvent::FlowStarted { flow_id, .. }
            | EngineEvent::NodeStarted { flow_id, .. }
            | EngineEvent::NodeCompleted { flow_id, .. }
            | EngineEvent::NodeSkipped { flow_id, .. }
            | EngineEvent::NodeError { flow_id, .. }
            | EngineEvent::MessageSent { flow_id, .. }
            | EngineEvent::StreamChunk { flow_id, .. }
            | EngineEvent::FlowCompleted { flow_id, .. }
            | EngineEvent::FlowError { flow_id, .. } => *flow_id,
        }
    }
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

/// Cooperative cancellation handle for a running flow.
///
/// Combines an atomic flag (for cheap polling and to cover subscribers that
/// register after cancellation) with a [`Notify`] (to wake an awaiting driver
/// immediately when cancellation fires). `tokio-util`'s `CancellationToken` is
/// not a dependency of this crate, so we implement the equivalent semantics
/// with the primitives already available in `tokio`.
#[derive(Clone)]
struct CancelHandle {
    flag: Arc<AtomicBool>,
    notify: Arc<Notify>,
}

impl CancelHandle {
    fn new() -> Self {
        Self {
            flag: Arc::new(AtomicBool::new(false)),
            notify: Arc::new(Notify::new()),
        }
    }

    /// Requests cancellation and wakes any driver awaiting [`Self::cancelled`].
    fn cancel(&self) {
        self.flag.store(true, Ordering::SeqCst);
        self.notify.notify_waiters();
    }

    /// Returns whether cancellation has been requested.
    fn is_cancelled(&self) -> bool {
        self.flag.load(Ordering::SeqCst)
    }

    /// Resolves as soon as cancellation has been requested.
    async fn cancelled(&self) {
        loop {
            // Register interest BEFORE checking the flag so a `cancel()` that
            // races between the check and the await cannot be missed
            // (`notify_waiters` does not store a permit for future waiters).
            let notified = self.notify.notified();
            if self.is_cancelled() {
                return;
            }
            notified.await;
            if self.is_cancelled() {
                return;
            }
        }
    }
}

/// Execution state of an active flow.
struct ActiveFlow {
    _flow: Flow,
    _plan: ExecutionPlan,
    status: FlowStatus,
    _trace_id: Uuid,
    /// Handle used by [`FlowEngine::stop`] to cancel the running driver task.
    cancel: CancelHandle,
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

/// Routes a node's output messages to its downstream channels, emitting a
/// `MessageSent` event per delivery. A message goes to a target when its
/// `source_port` matches the edge's port, or when the node has a single
/// outgoing channel (the common pass-through case).
async fn dispatch_outputs(
    messages: &[FlowMessage],
    out_channels: &[(String, Uuid, mpsc::Sender<FlowMessage>)],
    event_tx: &broadcast::Sender<EngineEvent>,
    flow_id: Uuid,
    node_id: Uuid,
) {
    for msg in messages {
        for (port, to_node, tx) in out_channels {
            if msg.source_port == *port || out_channels.len() == 1 {
                let _ = event_tx.send(EngineEvent::MessageSent {
                    flow_id,
                    from_node: node_id,
                    to_node: *to_node,
                    message_id: msg.id,
                    payload_preview: Some(truncate_payload(&msg.payload)),
                });
                if tx.send(msg.clone()).await.is_err() {
                    warn!("Channel closed when sending to node {}", to_node);
                }
            }
        }
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

        // Cancellation handle shared between the registered flow and the driver
        // task; `stop()` triggers it to cancel execution (FUNC-004).
        let cancel = CancelHandle::new();

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
                    cancel: cancel.clone(),
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

            let result = engine
                .execute_plan(&flow_clone, &plan, trace_id, trigger_msg.as_ref(), &cancel)
                .await;

            if cancel.is_cancelled() {
                // The flow was stopped by the user. `stop()` already set the
                // status to `Stopped`; do NOT override it with Completed/Error
                // and do NOT emit a terminal completion/error event. The entry
                // is still removed below (FUNC-005).
                info!(flow_id = %flow_id, "Flow execution cancelled by stop()");
            } else {
                match result {
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
            }

            // FUNC-005: the flow has reached a terminal state (completed,
            // errored, or cancelled) and all terminal events have already been
            // broadcast. Remove it from the active set so the map does not grow
            // unbounded and `active_flow_ids()` reports only genuinely-active
            // flows.
            engine.remove_flow(flow_id).await;
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
        cancel: &CancelHandle,
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
            // FUNC-004: stop scheduling further nodes once cancellation is
            // requested. Returning early leaves the flow un-completed; the
            // driver task observes `cancel.is_cancelled()` and keeps the
            // user-set `Stopped` status.
            if cancel.is_cancelled() {
                warn!(
                    flow_id = %flow.id,
                    step = step.step,
                    "Flow cancelled; not scheduling further steps"
                );
                return Ok(());
            }

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

                    // First output payload, captured for the UI preview.
                    let mut output_preview = None;

                    if let Some(ref mut receiver) = rx {
                        // FUNC-007: process EVERY message this node received, not
                        // just the first. Because nodes run in topological steps,
                        // all upstream senders are already dropped by the time this
                        // node runs, so the channel drains and closes without
                        // blocking. A node with several incoming edges therefore
                        // fires once per message instead of silently discarding all
                        // but the first.
                        let Some(first_msg) = receiver.recv().await else {
                            // Channel closed with no message: inactive branch.
                            debug!(node_id = %node_id, "Node skipped (no message received)");
                            let _ = event_tx.send(EngineEvent::NodeSkipped { flow_id, node_id });
                            return Ok(());
                        };

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

                        let mut next = Some(first_msg);
                        while let Some(msg) = next {
                            let outputs = executor.process(msg).await?;
                            if output_preview.is_none() {
                                output_preview =
                                    outputs.first().map(|m| truncate_payload(&m.payload));
                            }
                            dispatch_outputs(&outputs, &out_channels, &event_tx, flow_id, node_id)
                                .await;
                            next = receiver.recv().await;
                        }
                    } else {
                        // Root node: always processes a single trigger message.
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
                        let outputs = executor.process(root_msg).await?;
                        output_preview = outputs.first().map(|m| truncate_payload(&m.payload));
                        dispatch_outputs(&outputs, &out_channels, &event_tx, flow_id, node_id)
                            .await;
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

            // Wait for all nodes in the step to complete, honoring cancellation.
            // If `stop()` cancels mid-step, abort the in-flight node tasks so
            // their external calls / DB queries are not driven any further and
            // return promptly without scheduling the remaining steps.
            let mut idx = 0;
            while idx < handles.len() {
                let awaited = tokio::select! {
                    biased;
                    _ = cancel.cancelled() => None,
                    res = &mut handles[idx] => Some(res),
                };
                match awaited {
                    // Cancellation requested: abort every node task in this step
                    // (including any not yet awaited) and stop driving the flow.
                    None => {
                        for h in &handles {
                            h.abort();
                        }
                        warn!(
                            flow_id = %flow.id,
                            "Flow cancelled; aborting in-flight node tasks"
                        );
                        return Ok(());
                    }
                    Some(Ok(Ok(()))) => idx += 1,
                    Some(Ok(Err(e))) => return Err(e),
                    Some(Err(e)) => {
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
    ///
    /// In addition to marking the flow `Stopped`, this triggers the flow's
    /// cancellation handle so the running driver task stops scheduling new
    /// nodes and aborts any in-flight ones (FUNC-004). The driver task then
    /// removes the flow from the active set (FUNC-005).
    pub async fn stop(&self, flow_id: Uuid) -> Z8Result<()> {
        // Trigger cancellation while holding only a read lock; `cancel()` just
        // flips an atomic flag and notifies, so it cannot deadlock against the
        // driver's removal (which takes a write lock afterwards).
        if let Some(af) = self.active_flows.read().await.get(&flow_id) {
            af.cancel.cancel();
        }
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

    /// Removes a flow from the active set once it has reached a terminal state
    /// (FUNC-005). Called by the driver task after all terminal events have
    /// been emitted.
    async fn remove_flow(&self, flow_id: Uuid) {
        self.active_flows.write().await.remove(&flow_id);
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::node::{Node, PortType};
    use std::time::Duration;

    /// A trivial single-node flow whose root "debug" node runs to completion.
    fn trivial_flow() -> Flow {
        let mut flow = Flow::new("Cleanup Test Flow");
        let debug = Node::new("Debug", "debug").with_input("input", PortType::Any);
        flow.add_node(debug);
        flow
    }

    /// FUNC-005: once a flow reaches a terminal state it must be removed from
    /// the active set, so `active_flow_ids()` never reports historical runs.
    #[tokio::test]
    async fn completed_flow_is_removed_from_active_set() {
        let engine = FlowEngine::new();
        engine
            .register_node_type(Arc::new(crate::nodes::debug::DebugNodeFactory))
            .await;

        let flow = trivial_flow();
        let flow_id = flow.id;
        let mut events = engine.subscribe_events();

        engine.execute(flow).await.unwrap();

        // Wait until the flow reaches a terminal event.
        tokio::time::timeout(Duration::from_secs(5), async {
            loop {
                match events.recv().await {
                    Ok(EngineEvent::FlowCompleted { flow_id: fid, .. })
                    | Ok(EngineEvent::FlowError { flow_id: fid, .. })
                        if fid == flow_id =>
                    {
                        break
                    }
                    _ => continue,
                }
            }
        })
        .await
        .expect("flow did not reach a terminal state in time");

        // The terminal event is emitted just before the entry is removed, so
        // allow the driver task a brief moment to run the removal.
        let removed = tokio::time::timeout(Duration::from_secs(5), async {
            while engine.active_flow_ids().await.contains(&flow_id) {
                tokio::time::sleep(Duration::from_millis(5)).await;
            }
        })
        .await;

        assert!(
            removed.is_ok(),
            "completed flow was not removed from active_flows"
        );
        assert!(
            !engine.active_flow_ids().await.contains(&flow_id),
            "active_flow_ids still reports a completed flow"
        );
    }

    /// FUNC-004: the cancellation primitive must wake a driver already awaiting
    /// `cancelled()`, even when `cancel()` races with the awaiter registering.
    /// (A deterministic test of the primitive; end-to-end cancellation of an
    /// in-flight external call is timing-dependent and intentionally not
    /// asserted here.)
    #[tokio::test]
    async fn cancel_handle_wakes_awaiter() {
        let handle = CancelHandle::new();
        assert!(!handle.is_cancelled());

        let awaiter = {
            let h = handle.clone();
            tokio::spawn(async move { h.cancelled().await })
        };

        // Give the awaiter a chance to register, then request cancellation.
        tokio::task::yield_now().await;
        handle.cancel();
        assert!(handle.is_cancelled());

        tokio::time::timeout(Duration::from_secs(1), awaiter)
            .await
            .expect("cancelled() did not resolve after cancel()")
            .expect("awaiter task panicked");
    }

    /// FUNC-007: a node with several incoming edges must process EVERY message,
    /// not just the first. Two root nodes fan into `C`, which forwards to `D`;
    /// `C` must therefore emit two outgoing messages (one per input), proving
    /// both inputs were consumed rather than one silently dropped.
    #[tokio::test]
    async fn multi_input_node_processes_every_message() {
        use crate::flow::Edge;

        let engine = FlowEngine::new();
        engine
            .register_node_type(Arc::new(crate::nodes::debug::DebugNodeFactory))
            .await;

        let mut flow = Flow::new("Fan-in Flow");
        let a = Node::new("A", "debug").with_output("output", PortType::Any);
        let b = Node::new("B", "debug").with_output("output", PortType::Any);
        let c = Node::new("C", "debug")
            .with_input("input", PortType::Any)
            .with_output("output", PortType::Any);
        let d = Node::new("D", "debug").with_input("input", PortType::Any);
        let (a_id, b_id, c_id, d_id) = (a.id, b.id, c.id, d.id);
        flow.add_node(a);
        flow.add_node(b);
        flow.add_node(c);
        flow.add_node(d);
        flow.edges.push(Edge::new(a_id, "output", c_id, "input"));
        flow.edges.push(Edge::new(b_id, "output", c_id, "input"));
        flow.edges.push(Edge::new(c_id, "output", d_id, "input"));

        let flow_id = flow.id;
        let mut events = engine.subscribe_events();
        engine.execute(flow).await.unwrap();

        // Count the messages C forwarded to D, up to flow completion.
        let from_c = tokio::time::timeout(Duration::from_secs(5), async {
            let mut count = 0;
            loop {
                match events.recv().await {
                    Ok(EngineEvent::MessageSent { from_node, .. }) if from_node == c_id => {
                        count += 1;
                    }
                    Ok(EngineEvent::FlowCompleted { flow_id: fid, .. })
                    | Ok(EngineEvent::FlowError { flow_id: fid, .. })
                        if fid == flow_id =>
                    {
                        break count;
                    }
                    _ => continue,
                }
            }
        })
        .await
        .expect("flow did not complete in time");

        assert_eq!(
            from_c, 2,
            "fan-in node forwarded {from_c} of 2 inputs (must process every message)"
        );
    }
}
