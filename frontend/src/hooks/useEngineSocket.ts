import { useEffect } from "react";
import { create } from "zustand";

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
}

interface EngineLogEntry {
  id: number;
  timestamp: Date;
  event: EngineEvent;
}

interface EngineStore {
  connected: boolean;
  running: boolean;
  logs: EngineLogEntry[];
  setConnected: (v: boolean) => void;
  setRunning: (v: boolean) => void;
  addLog: (event: EngineEvent) => void;
  clearLogs: () => void;
}

let logCounter = 0;

export const useEngineStore = create<EngineStore>((set, get) => ({
  connected: false,
  running: false,
  logs: [],

  setConnected: (v) => set({ connected: v }),
  setRunning: (v) => set({ running: v }),

  addLog: (event) => {
    logCounter++;
    const entry: EngineLogEntry = {
      id: logCounter,
      timestamp: new Date(),
      event,
    };
    // Keep last 200 entries
    const logs = [...get().logs, entry].slice(-200);
    set({ logs });

    // Update running state based on event type
    if (event.type === "flow_started") {
      set({ running: true });
    } else if (
      event.type === "flow_completed" ||
      event.type === "flow_error"
    ) {
      set({ running: false });
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
