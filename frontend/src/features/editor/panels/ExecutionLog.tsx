import { useRef, useEffect, useState, useCallback } from "react";
import { useEngineStore } from "@/hooks/useEngineSocket";
import type { EngineEvent, NodeInfo } from "@/hooks/useEngineSocket";
import {
  Trash2, ChevronUp, ChevronDown, ChevronRight,
  Play, CheckCircle2, XCircle, ArrowRight, Zap, AlertTriangle, SkipForward, Globe,
} from "lucide-react";

type ResolveFn = (id: string) => NodeInfo | undefined;

interface EventDisplay {
  icon: React.ComponentType<{ size?: number; className?: string }>;
  color: string;
  label: string;
  detail: (event: EngineEvent, resolve: ResolveFn) => string;
}

/** Extract payload data from an event (if any). */
function getEventPayload(event: EngineEvent): unknown | undefined {
  if (event.type === "node_completed" && event.output != null) return event.output;
  if (event.type === "message_sent" && event.payload != null) return event.payload;
  return undefined;
}

/** Format a JSON value as a compact preview string (single line, max ~120 chars). */
function formatPreview(value: unknown): string {
  if (value == null) return "";
  try {
    const s = JSON.stringify(value);
    return s.length > 120 ? s.substring(0, 117) + "..." : s;
  } catch {
    return String(value);
  }
}

const EVENT_CONFIG: Record<string, EventDisplay> = {
  routes_registered: {
    icon: Globe,
    color: "text-purple-400",
    label: "ROUTES",
    detail: (e, _r) => {
      const routes = (e as Record<string, unknown>).routes as string | undefined;
      return routes ? `Registered endpoints:\n${routes}` : "No routes registered";
    },
  },
  flow_started: {
    icon: Play,
    color: "text-blue-400",
    label: "FLOW START",
    detail: (_e, _r) => "Flow execution started",
  },
  flow_completed: {
    icon: CheckCircle2,
    color: "text-green-400",
    label: "FLOW DONE",
    detail: (e, _r) =>
      `Flow completed successfully${e.duration_ms != null ? ` in ${e.duration_ms}ms` : ""}`,
  },
  flow_error: {
    icon: XCircle,
    color: "text-red-400",
    label: "FLOW ERROR",
    detail: (e, _r) => `Flow failed: ${e.error ?? "Unknown error"}`,
  },
  node_started: {
    icon: Zap,
    color: "text-cyan-400",
    label: "NODE START",
    detail: (e, resolve) => {
      const info = e.node_id ? resolve(e.node_id) : undefined;
      return info
        ? `Executing "${info.label}" (${info.nodeType})`
        : `Executing node ${e.node_id?.substring(0, 8) ?? "?"}`;
    },
  },
  node_completed: {
    icon: CheckCircle2,
    color: "text-emerald-400",
    label: "NODE DONE",
    detail: (e, resolve) => {
      const info = e.node_id ? resolve(e.node_id) : undefined;
      const dur = e.duration_us != null ? ` — ${(e.duration_us / 1000).toFixed(1)}ms` : "";
      return info
        ? `"${info.label}" completed${dur}`
        : `Node ${e.node_id?.substring(0, 8) ?? "?"} completed${dur}`;
    },
  },
  node_skipped: {
    icon: SkipForward,
    color: "text-slate-500",
    label: "SKIPPED",
    detail: (e, resolve) => {
      const info = e.node_id ? resolve(e.node_id) : undefined;
      return info
        ? `"${info.label}" skipped (inactive branch)`
        : `Node ${e.node_id?.substring(0, 8) ?? "?"} skipped`;
    },
  },
  node_error: {
    icon: XCircle,
    color: "text-red-400",
    label: "NODE ERROR",
    detail: (e, resolve) => {
      const info = e.node_id ? resolve(e.node_id) : undefined;
      const name = info ? `"${info.label}"` : `Node ${e.node_id?.substring(0, 8) ?? "?"}`;
      return `${name} failed: ${e.error ?? "Unknown error"}`;
    },
  },
  message_sent: {
    icon: ArrowRight,
    color: "text-slate-400",
    label: "MSG SENT",
    detail: (e, resolve) => {
      const from = e.from_node ? resolve(e.from_node) : undefined;
      const to = e.to_node ? resolve(e.to_node) : undefined;
      const fromName = from?.label ?? e.from_node?.substring(0, 8) ?? "?";
      const toName = to?.label ?? e.to_node?.substring(0, 8) ?? "?";
      return `${fromName} → ${toName}`;
    },
  },
};

const DEFAULT_EVENT: EventDisplay = {
  icon: AlertTriangle,
  color: "text-slate-500",
  label: "EVENT",
  detail: (e) => e.type,
};

/** A single log row — expandable if it has payload data. */
function LogEntry({
  entry,
  resolve,
}: {
  entry: { id: number; timestamp: Date; event: EngineEvent };
  resolve: ResolveFn;
}) {
  const [open, setOpen] = useState(false);
  const cfg = EVENT_CONFIG[entry.event.type] ?? DEFAULT_EVENT;
  const Icon = cfg.icon;
  const detail = cfg.detail(entry.event, resolve);
  const payload = getEventPayload(entry.event);
  const hasPayload = payload != null;
  const time = entry.timestamp.toLocaleTimeString("en-US", {
    hour12: false,
    fractionalSecondDigits: 3,
  });

  return (
    <div className="border-b border-slate-900/30">
      <div
        className={`flex items-start gap-2 px-3 py-1 hover:bg-slate-900/50 ${
          hasPayload ? "cursor-pointer" : ""
        }`}
        onClick={hasPayload ? () => setOpen(!open) : undefined}
      >
        <span className="text-slate-600 shrink-0 w-[72px]">{time}</span>
        {hasPayload ? (
          <span className="shrink-0 mt-0.5 text-slate-600">
            {open ? <ChevronDown size={12} /> : <ChevronRight size={12} />}
          </span>
        ) : (
          <Icon size={12} className={`${cfg.color} shrink-0 mt-0.5`} />
        )}
        <span className={`${cfg.color} shrink-0 w-[88px] font-semibold`}>
          {cfg.label}
        </span>
        <span className="text-slate-300 truncate flex-1">{detail}</span>
        {hasPayload && !open && (
          <span className="text-slate-600 truncate max-w-[300px] text-[10px]">
            {formatPreview(payload)}
          </span>
        )}
      </div>
      {hasPayload && open && (
        <pre className="mx-3 mb-1 ml-[172px] px-2 py-1.5 bg-slate-900 rounded text-[10px] text-amber-300/80 overflow-x-auto max-h-32 whitespace-pre-wrap break-all">
          {JSON.stringify(payload, null, 2)}
        </pre>
      )}
    </div>
  );
}

const MIN_HEIGHT = 80;
const DEFAULT_HEIGHT = 224; // ~14rem
const MAX_HEIGHT = 600;

export function ExecutionLog() {
  const logs = useEngineStore((s) => s.logs);
  const clearLogs = useEngineStore((s) => s.clearLogs);
  const nodeInfoMap = useEngineStore((s) => s.nodeInfoMap);
  const [expanded, setExpanded] = useState(true);
  const [height, setHeight] = useState(DEFAULT_HEIGHT);
  const scrollRef = useRef<HTMLDivElement>(null);
  const dragging = useRef(false);
  const startY = useRef(0);
  const startHeight = useRef(0);

  const resolve = useCallback(
    (uuid: string): NodeInfo | undefined => nodeInfoMap[uuid],
    [nodeInfoMap],
  );

  // Auto-scroll to bottom
  useEffect(() => {
    if (scrollRef.current) {
      scrollRef.current.scrollTop = scrollRef.current.scrollHeight;
    }
  }, [logs]);

  // Drag-to-resize handler
  const onDragStart = useCallback(
    (e: React.MouseEvent) => {
      e.preventDefault();
      dragging.current = true;
      startY.current = e.clientY;
      startHeight.current = height;

      const onMove = (ev: MouseEvent) => {
        if (!dragging.current) return;
        // Dragging up = increasing height (Y decreases)
        const delta = startY.current - ev.clientY;
        setHeight(Math.min(MAX_HEIGHT, Math.max(MIN_HEIGHT, startHeight.current + delta)));
      };
      const onUp = () => {
        dragging.current = false;
        document.removeEventListener("mousemove", onMove);
        document.removeEventListener("mouseup", onUp);
      };
      document.addEventListener("mousemove", onMove);
      document.addEventListener("mouseup", onUp);
    },
    [height],
  );

  if (logs.length === 0) return null;

  return (
    <div className="border-t border-slate-700 bg-slate-950 flex flex-col">
      {/* Drag handle to resize */}
      {expanded && (
        <div
          onMouseDown={onDragStart}
          className="h-1 cursor-ns-resize hover:bg-z8-500/40 transition-colors bg-transparent shrink-0"
          title="Drag to resize"
        />
      )}

      {/* Header */}
      <div className="flex items-center justify-between px-3 py-1.5 bg-slate-900 border-b border-slate-800 shrink-0">
        <button
          type="button"
          onClick={() => setExpanded(!expanded)}
          className="flex items-center gap-2 text-xs text-slate-400 hover:text-slate-200 transition-colors"
        >
          {expanded ? <ChevronDown size={12} /> : <ChevronUp size={12} />}
          <span className="font-medium">Execution Log</span>
          <span className="text-slate-600">({logs.length})</span>
        </button>
        <button
          type="button"
          onClick={clearLogs}
          className="p-1 hover:bg-slate-800 rounded transition-colors"
          title="Clear logs"
        >
          <Trash2 size={12} className="text-slate-500" />
        </button>
      </div>

      {/* Log entries */}
      {expanded && (
        <div
          ref={scrollRef}
          className="overflow-y-auto font-mono text-[11px]"
          style={{ height }}
        >
          {logs.map((entry) => (
            <LogEntry key={entry.id} entry={entry} resolve={resolve} />
          ))}
        </div>
      )}
    </div>
  );
}
