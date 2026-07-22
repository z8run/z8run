# z8run-core

Flow engine, scheduler, executor, and data model for [z8run](https://github.com/z8run/z8run).

## Overview

`z8run-core` is the foundation crate of the z8run visual flow engine. It provides:

- **Flow model** - directed acyclic graphs (DAGs) of nodes connected by typed ports
- **Execution engine** - compiles flows into parallel execution plans using Kahn's algorithm
- **39 built-in nodes** - HTTP, AI/LLM, MQTT, database, webhook, and more (source of truth: [`src/nodes/mod.rs`](src/nodes/mod.rs))
- **Event system** - real-time broadcasting of execution events (node started, completed, error, etc.)

## Architecture

```
FlowEngine
├── Flow (DAG of Nodes + Edges)
│   ├── Node (inputs, outputs, config)
│   └── Edge (source port → target port)
├── ExecutionPlan (topological order, maximized parallelism)
├── NodeExecutor trait (process, configure, validate)
└── EngineEvent (FlowStarted, NodeCompleted, StreamChunk, ...)
```

## Key types

| Type | Description |
|------|-------------|
| `FlowEngine` | Compiles and executes flows with concurrent node scheduling |
| `Flow` | Directed graph with nodes, edges, and DAG validation |
| `Node` | Node with typed input/output ports and config |
| `FlowMessage` | Message flowing between nodes (payload + metadata) |
| `NodeExecutor` | Trait for implementing custom nodes |
| `ExecutionPlan` | Compiled plan with parallel execution steps |
| `EngineEvent` | Events emitted during flow execution |

## Built-in nodes

z8run-core registers 39 built-in nodes (the authoritative list is the
`register_node_type(...)` calls in [`src/nodes/mod.rs`](src/nodes/mod.rs)):

| Category | Nodes |
|----------|-------|
| **Input / Trigger** | HTTP In, Timer, Webhook, Webhook Trigger, Cron Trigger |
| **Process** | Function, JSON Transform, HTTP Request, Filter |
| **Output** | Debug, HTTP Response |
| **Logic** | Switch, Delay |
| **Control flow** | If/Else, Loop |
| **Data** | Database (PostgreSQL/MySQL/SQLite), MQTT |
| **Data engineering** | CSV, Aggregator, Batch |
| **Data shaping** | Mapper |
| **Security** | Sanitize |
| **AI** | LLM, Embeddings, Classifier, Prompt Template, Text Splitter, Vector Store, Structured Output, Summarizer, AI Agent, Image Gen, STT, TTS |
| **Integration** | Twilio (SMS/Call/Lookup), WhatsApp, CRM (HubSpot/Salesforce), Conversation Memory, Human Handoff |

## Usage

```toml
[dependencies]
z8run-core = "0.1"
```

```rust
use z8run_core::{Flow, Node, FlowEngine};

// Create a flow (Flow::new takes a single name argument)
let mut flow = Flow::new("My Flow");
// Node::new(name, node_type) — the node_type must match a registered node
let node = Node::new("Debug", "debug")
    .with_input("in", z8run_core::PortType::Any)
    .with_output("out", z8run_core::PortType::Any);
flow.add_node(node);

// Create and run the engine
let engine = FlowEngine::new();
engine.register_builtin_nodes();
```

## Implementing a custom node

```rust
use z8run_core::engine::NodeExecutor;
use z8run_core::message::FlowMessage;
use async_trait::async_trait;

pub struct MyNode { name: String }

#[async_trait]
impl NodeExecutor for MyNode {
    async fn process(&self, msg: FlowMessage) -> z8run_core::error::Z8Result<Vec<(String, FlowMessage)>> {
        // Process the message and return outputs
        Ok(vec![("out".to_string(), msg)])
    }
    fn node_type(&self) -> &str { "my-node" }
}
```

## License

Apache-2.0 OR MIT
