# z8run-protocol

Binary WebSocket protocol for real-time communication between the z8run frontend editor and backend engine.

## Overview

`z8run-protocol` defines a compact binary frame format for low-latency, bidirectional messaging over WebSockets. It handles:

- **Binary frame encoding/decoding** with an 11-byte fixed header
- **Request/response correlation** via 4-byte correlation IDs
- **Typed messages** for flow commands, execution events, editor sync, and debugging
- **Bincode serialization** for minimal payload size (with JSON debug mode)

## Frame format

```
┌─────────┬──────────┬────────────────┬──────────────┐
│ Version │ MsgType  │ CorrelationID  │ PayloadLen   │
│ 1 byte  │ 2 bytes  │ 4 bytes        │ 4 bytes      │
└─────────┴──────────┴────────────────┴──────────────┘
│                  Payload (bincode)                  │
└────────────────────────────────────────────────────┘
```

## Message types

| Range | Category | Messages |
|-------|----------|----------|
| `0x00xx` | Control | Ping, Pong, Authenticate, AuthResult |
| `0x01xx` | Flow | Create, Update, Delete, Start, Stop, List, Get |
| `0x02xx` | Execution | Started, NodeStarted, NodeCompleted, NodeError, Completed |
| `0x03xx` | Debug | SetBreakpoint, RemoveBreakpoint, Inspect, InspectResult |
| `0x04xx` | Editor | NodeMoved, ZoomChanged, SelectionChanged, CursorPosition |
| `0x0Fxx` | Response | Ok, Error |

## Usage

```toml
[dependencies]
z8run-protocol = "0.1"
```

```rust
use z8run_protocol::{Z8Codec, ProtocolMessage};

let codec = Z8Codec::new();

// Encode a message
let frame = codec.encode(&ProtocolMessage::Ping)?;
let bytes = frame.to_bytes();

// Decode a message
let frame = z8run_protocol::Frame::from_bytes(&bytes)?;
let message = codec.decode(&frame)?;
```

## License

Apache-2.0 OR MIT
