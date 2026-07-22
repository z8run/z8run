//! Background recorder that persists flow execution history (FUNC-008).
//!
//! The engine broadcasts execution events regardless of trigger source (API or
//! hooks). This task subscribes to that stream and writes each run's start and
//! completion into the `executions` table via [`ExecutionRepository`], so the
//! last-run status of a flow survives after it leaves the in-memory active set.

use std::collections::HashMap;
use std::sync::Arc;

use tokio::sync::broadcast::error::RecvError;
use tokio::sync::broadcast::Receiver;
use tracing::{debug, warn};
use uuid::Uuid;
use z8run_core::engine::EngineEvent;
use z8run_storage::repository::ExecutionRepository;

/// Spawns the recorder task. It runs for the lifetime of the process.
pub fn spawn(mut events: Receiver<EngineEvent>, executions: Arc<dyn ExecutionRepository>) {
    tokio::spawn(async move {
        // trace_id -> execution row id, for correlating start with completion.
        let mut in_flight: HashMap<Uuid, Uuid> = HashMap::new();

        loop {
            match events.recv().await {
                Ok(EngineEvent::FlowStarted { flow_id, trace_id }) => {
                    match executions.record_start(flow_id, trace_id).await {
                        Ok(execution_id) => {
                            in_flight.insert(trace_id, execution_id);
                        }
                        Err(e) => warn!(error = %e, %flow_id, "Failed to record execution start"),
                    }
                }
                Ok(EngineEvent::FlowCompleted {
                    trace_id,
                    duration_ms,
                    ..
                }) => {
                    if let Some(execution_id) = in_flight.remove(&trace_id) {
                        if let Err(e) = executions
                            .record_completion(execution_id, "completed", duration_ms, None)
                            .await
                        {
                            warn!(error = %e, "Failed to record execution completion");
                        }
                    }
                }
                Ok(EngineEvent::FlowError {
                    trace_id, error, ..
                }) => {
                    if let Some(execution_id) = in_flight.remove(&trace_id) {
                        if let Err(e) = executions
                            .record_completion(execution_id, "error", 0, Some(&error))
                            .await
                        {
                            warn!(error = %e, "Failed to record execution error");
                        }
                    }
                }
                // Node-level events are not persisted here.
                Ok(_) => {}
                Err(RecvError::Lagged(n)) => {
                    warn!(
                        missed = n,
                        "Execution recorder lagged; some runs may be unrecorded"
                    );
                }
                Err(RecvError::Closed) => {
                    debug!("Engine event stream closed; execution recorder stopping");
                    break;
                }
            }
        }
    });
}
