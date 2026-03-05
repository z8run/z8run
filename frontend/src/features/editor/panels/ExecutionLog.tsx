import { useRef, useEffect, useState } from "react";
import { useEngineStore } from "@/hooks/useEngineSocket";
import { Trash2, ChevronUp, ChevronDown } from "lucide-react";

const EVENT_COLORS: Record<string, string> = {
  flow_started: "text-blue-400",
  flow_completed: "text-green-400",
  flow_error: "text-red-400",
  node_started: "text-cyan-400",
  node_completed: "text-emerald-400",
  node_error: "text-red-400",
  message_sent: "text-slate-400",
};

const EVENT_LABELS: Record<string, string> = {
  flow_started: "FLOW START",
  flow_completed: "FLOW DONE",
  flow_error: "FLOW ERROR",
  node_started: "NODE START",
  node_completed: "NODE DONE",
  node_error: "NODE ERROR",
  message_sent: "MSG SENT",
};

export function ExecutionLog() {
  const logs = useEngineStore((s) => s.logs);
  const clearLogs = useEngineStore((s) => s.clearLogs);
  const [expanded, setExpanded] = useState(true);
  const scrollRef = useRef<HTMLDivElement>(null);

  // Auto-scroll to bottom
  useEffect(() => {
    if (scrollRef.current) {
      scrollRef.current.scrollTop = scrollRef.current.scrollHeight;
    }
  }, [logs]);

  if (logs.length === 0) return null;

  return (
    <div className="border-t border-slate-700 bg-slate-950">
      {/* Header */}
      <div className="flex items-center justify-between px-3 py-1.5 bg-slate-900 border-b border-slate-800">
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
        <div ref={scrollRef} className="max-h-48 overflow-y-auto font-mono text-[11px]">
          {logs.map((entry) => {
            const color = EVENT_COLORS[entry.event.type] || "text-slate-500";
            const label = EVENT_LABELS[entry.event.type] || entry.event.type;
            const time = entry.timestamp.toLocaleTimeString("en-US", {
              hour12: false,
              fractionalSecondDigits: 3,
            });

            let detail = "";
            if (entry.event.node_id) {
              detail += ` node=${entry.event.node_id.substring(0, 8)}`;
            }
            if (entry.event.duration_us) {
              detail += ` ${(entry.event.duration_us / 1000).toFixed(1)}ms`;
            }
            if (entry.event.duration_ms) {
              detail += ` ${entry.event.duration_ms}ms`;
            }
            if (entry.event.error) {
              detail += ` ${entry.event.error}`;
            }
            if (entry.event.from_node && entry.event.to_node) {
              detail += ` ${entry.event.from_node.substring(0, 8)} → ${entry.event.to_node.substring(0, 8)}`;
            }

            return (
              <div
                key={entry.id}
                className="flex items-start px-3 py-0.5 hover:bg-slate-900/50"
              >
                <span className="text-slate-600 w-20 shrink-0">{time}</span>
                <span className={`${color} w-24 shrink-0 font-semibold`}>
                  {label}
                </span>
                <span className="text-slate-400 truncate">{detail}</span>
              </div>
            );
          })}
        </div>
      )}
    </div>
  );
}
