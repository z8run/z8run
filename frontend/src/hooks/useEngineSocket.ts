import { useEffect } from "react";
import { create } from "zustand";
import { useFlowStore } from "@/stores/flowStore";

/** Engine event received from the WebSocket. */
export interface EngineEvent {
  type: string;
  flow_id?: string;
  trace_id?: string;
  node_id?: string;
  from_node?: string;
  to_node?: string;
  message_id?: string;
  duration_us?: number;
  duration_ms?: number;
  error?: string;
  /** Payload preview for message_sent events */
  payload?: unknown;
  /** Output preview for node_completed events */
  output?: unknown;
}

interface EngineLogEntry {
  id: number;
  timestamp: Date;
  event: EngineEvent;
}

/** Maps core UUID → canvas node ID for visual feedback */
type NodeMap = Record<string, string>;

/** Info about a canvas node, resolved for log display */
export interface NodeInfo {
  canvasId: string;
  label: string;
  nodeType: string;
}

/** Maps core UUID → node info for descriptive logs */
type NodeInfoMap = Record<string, NodeInfo>;

interface EngineStore {
  connected: boolean;
  running: boolean;
  logs: EngineLogEntry[];
  /** Reverse map: core UUID → canvas node ID */
  nodeMap: NodeMap;
  /** Reverse map: core UUID → {canvasId, label, nodeType} for log display */
  nodeInfoMap: NodeInfoMap;
  setConnected: (v: boolean) => void;
  setRunning: (v: boolean) => void;
  addLog: (event: EngineEvent) => void;
  clearLogs: () => void;
  /** Store the node_map returned by start_flow (canvas_id → UUID), reversed for lookup */
  setNodeMap: (map: Record<string, string>) => void;
}

let logCounter = 0;
let resetTimer: ReturnType<typeof setTimeout> | null = null;
/** Queue of node events that arrived before node_map was available */
let pendingNodeEvents: EngineEvent[] = [];

/** Apply a node event to the canvas (update node visual status) */
function applyNodeEvent(event: EngineEvent, nodeMap: NodeMap) {
  if (!event.node_id) return;
  const canvasId = nodeMap[event.node_id];
  if (!canvasId) return;
  const { setNodeStatus } = useFlowStore.getState();
  if (event.type === "node_started") {
    setNodeStatus(canvasId, "running");
  } else if (event.type === "node_completed") {
    setNodeStatus(canvasId, "success");
  } else if (event.type === "node_error") {
    setNodeStatus(canvasId, "error");
  }
}

export const useEngineStore = create<EngineStore>((set, get) => ({
  connected: false,
  running: false,
  logs: [],
  nodeMap: {},
  nodeInfoMap: {},

  setConnected: (v) => set({ connected: v }),
  setRunning: (v) => set({ running: v }),

  setNodeMap: (forwardMap) => {
    // Reverse: canvas_id → uuid  →  uuid → canvas_id
    const reversed: NodeMap = {};
    const infoMap: NodeInfoMap = {};

    // Get canvas nodes from flowStore to resolve names
    const canvasNodes = useFlowStore.getState().nodes;
    const nodeDataById: Record<string, { label: string; nodeType: string }> = {};
    for (const n of canvasNodes) {
      const d = n.data as Record<string, unknown>;
      nodeDataById[n.id] = {
        label: String(d.label ?? "Node"),
        nodeType: String(d.type ?? d.nodeType ?? "unknown"),
      };
    }

    for (const [canvasId, uuid] of Object.entries(forwardMap)) {
      reversed[uuid] = canvasId;
      const meta = nodeDataById[canvasId];
      infoMap[uuid] = {
        canvasId,
        label: meta?.label ?? canvasId,
        nodeType: meta?.nodeType ?? "unknown",
      };
    }

    set({ nodeMap: reversed, nodeInfoMap: infoMap });

    // Replay any events that arrived before the map was ready
    if (pendingNodeEvents.length > 0) {
      const queued = [...pendingNodeEvents];
      pendingNodeEvents = [];
      for (const evt of queued) {
        applyNodeEvent(evt, reversed);
      }
    }
  },

  addLog: (event) => {
    logCounter++;
    const entry: EngineLogEntry = {
      id: logCounter,
      timestamp: new Date(),
      event,
    };
    const logs = [...get().logs, entry].slice(-200);
    set({ logs });

    // Update running state based on event type
    if (event.type === "flow_started") {
      set({ running: true });
      pendingNodeEvents = []; // clear any stale queue
      useFlowStore.getState().resetAllNodeStatus();
    } else if (
      event.type === "flow_completed" ||
      event.type === "flow_error"
    ) {
      set({ running: false });
      // Keep success/error visible for 4 seconds, then reset to idle
      if (resetTimer) clearTimeout(resetTimer);
      resetTimer = setTimeout(() => {
        useFlowStore.getState().resetAllNodeStatus();
        resetTimer = null;
      }, 4000);
    }

    // Update individual node visual status
    const { nodeMap } = get();
    if (event.node_id) {
      if (Object.keys(nodeMap).length > 0) {
        // Map is ready — apply immediately
        applyNodeEvent(event, nodeMap);
      } else {
        // Map not ready yet (HTTP response still in flight) — queue for replay
        pendingNodeEvents.push(event);
      }
    }
  },

  clearLogs: () => set({ logs: [] }),
}));

// ─── Singleton WebSocket manager (outside React lifecycle) ───────────
//
// Strategy: the actual connect() is debounced by 150ms so that React
// Strict Mode's mount→unmount→mount cycle (which happens synchronously
// within a single microtask flush) resolves before we ever open a socket.
// This guarantees exactly ONE connection regardless of double-mounts.

let ws: WebSocket | null = null;
let reconnectTimer: ReturnType<typeof setTimeout> | null = null;
let connectTimer: ReturnType<typeof setTimeout> | null = null;
let refCount = 0;

function getWsUrl(): string {
  const protocol = window.location.protocol === "https:" ? "wss:" : "ws:";
  const backendHost = import.meta.env.DEV
    ? "localhost:7700"
    : window.location.host;
  return `${protocol}//${backendHost}/ws/engine`;
}

function doConnect() {
  // Already connected or connecting — nothing to do
  if (
    ws?.readyState === WebSocket.OPEN ||
    ws?.readyState === WebSocket.CONNECTING
  ) {
    return;
  }

  // Nobody wants the connection — don't connect
  if (refCount <= 0) return;

  const url = getWsUrl();
  const socket = new WebSocket(url);
  ws = socket;

  socket.onopen = () => {
    if (ws !== socket) return;
    useEngineStore.getState().setConnected(true);
  };

  socket.onmessage = (e) => {
    if (ws !== socket) return;
    try {
      const event: EngineEvent = JSON.parse(e.data);
      useEngineStore.getState().addLog(event);
    } catch {
      // ignore malformed messages
    }
  };

  socket.onclose = () => {
    if (ws !== socket) return;
    ws = null;
    useEngineStore.getState().setConnected(false);

    // Reconnect if there are still consumers
    if (refCount > 0) {
      reconnectTimer = setTimeout(scheduleConnect, 3000);
    }
  };

  socket.onerror = () => {
    // onclose will fire after onerror — no action needed
  };
}

/** Debounced connect — waits 150ms so Strict Mode cleanup can cancel it. */
function scheduleConnect() {
  if (connectTimer) clearTimeout(connectTimer);
  connectTimer = setTimeout(() => {
    connectTimer = null;
    doConnect();
  }, 150);
}

function cancelScheduledConnect() {
  if (connectTimer) {
    clearTimeout(connectTimer);
    connectTimer = null;
  }
}

function addRef() {
  refCount++;
  if (refCount === 1) {
    scheduleConnect();
  }
}

function removeRef() {
  refCount--;
  if (refCount <= 0) {
    refCount = 0;
    // Cancel any pending connect
    cancelScheduledConnect();
    if (reconnectTimer) {
      clearTimeout(reconnectTimer);
      reconnectTimer = null;
    }
    if (ws) {
      ws.onclose = null; // prevent reconnect from the close handler
      ws.close();
      ws = null;
      useEngineStore.getState().setConnected(false);
    }
  }
}

/**
 * Hook that manages the WebSocket connection to /ws/engine.
 *
 * Uses a module-level singleton with debounced connect so React Strict
 * Mode double-mounts, HMR re-renders, and multiple components calling
 * this hook all share a single WebSocket connection.
 */
export function useEngineSocket() {
  useEffect(() => {
    addRef();
    return () => removeRef();
  }, []);
}
