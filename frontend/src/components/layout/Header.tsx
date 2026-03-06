import { flowsApi } from "@/api/flows";
import { useEngineStore } from "@/hooks/useEngineSocket";
import { useFlowStore } from "@/stores/flowStore";
import { useReactFlow } from "@xyflow/react";
import { Check, ChevronLeft, Loader2, Play, Save, Square } from "lucide-react";
import { useCallback, useEffect, useRef, useState } from "react";
import { Link } from "react-router-dom";

export function Header() {
  const flowName = useFlowStore((s) => s.flowName);
  const flowId = useFlowStore((s) => s.flowId);
  const saving = useFlowStore((s) => s.saving);
  const dirty = useFlowStore((s) => s.dirty);
  const saveFlow = useFlowStore((s) => s.saveFlow);
  const setFlowName = useFlowStore((s) => s.setFlowName);
  const running = useEngineStore((s) => s.running);
  const wsConnected = useEngineStore((s) => s.connected);
  const reactFlow = useReactFlow();

  const [editing, setEditing] = useState(false);
  const [editValue, setEditValue] = useState(flowName);
  const inputRef = useRef<HTMLInputElement>(null);

  useEffect(() => {
    if (editing && inputRef.current) {
      inputRef.current.focus();
      inputRef.current.select();
    }
  }, [editing]);

  const handleSave = useCallback(() => {
    const viewport = reactFlow.getViewport();
    saveFlow(viewport);
  }, [reactFlow, saveFlow]);

  const commitRename = () => {
    const trimmed = editValue.trim();
    if (trimmed && trimmed !== flowName) {
      setFlowName(trimmed);
    }
    setEditing(false);
  };

  return (
    <header className="h-12 bg-slate-900 border-b border-slate-700 flex items-center justify-between px-4">
      {/* Left: nav + flow name */}
      <div className="flex items-center gap-3">
        <Link
          to="/"
          className="p-1 hover:bg-slate-700 rounded transition-colors"
        >
          <ChevronLeft size={18} className="text-slate-400" />
        </Link>
        <div className="flex items-center gap-2">
          <div className="w-5 h-5 rounded bg-z8-600 flex items-center justify-center">
            <span className="text-[8px] font-bold text-white">z8</span>
          </div>
          {editing ? (
            <input
              ref={inputRef}
              value={editValue}
              onChange={(e) => setEditValue(e.target.value)}
              onBlur={commitRename}
              onKeyDown={(e) => {
                if (e.key === "Enter") commitRename();
                if (e.key === "Escape") {
                  setEditValue(flowName);
                  setEditing(false);
                }
              }}
              className="text-sm font-medium text-slate-200 bg-slate-800 border border-slate-600
                rounded px-2 py-0.5 outline-none focus:border-z8-500 w-48"
            />
          ) : (
            <span
              className="text-sm font-medium text-slate-200 cursor-pointer hover:text-white
                px-2 py-0.5 rounded hover:bg-slate-800 transition-colors"
              onDoubleClick={() => {
                setEditValue(flowName);
                setEditing(true);
              }}
              title="Double-click to rename"
            >
              {flowName}
            </span>
          )}
          {dirty && (
            <span className="text-[10px] text-amber-400 ml-1">(unsaved)</span>
          )}
        </div>

        {/* Connection status */}
        <div className="flex items-center gap-1.5 ml-4">
          <div
            className={`w-1.5 h-1.5 rounded-full ${wsConnected ? "bg-green-500" : "bg-red-500"}`}
          />
          <span className="text-[10px] text-slate-500">
            {wsConnected ? "Connected" : "Disconnected"}
          </span>
        </div>
      </div>

      {/* Right: actions */}
      <div className="flex items-center gap-2">
        <button
          type="button"
          onClick={handleSave}
          disabled={saving || !dirty}
          className={`flex items-center gap-1.5 px-3 py-1.5 text-xs rounded-md transition-colors ${
            dirty
              ? "text-white bg-blue-600 hover:bg-blue-700"
              : "text-slate-500 hover:bg-slate-700"
          } ${saving ? "opacity-60 cursor-wait" : ""}`}
        >
          {saving ? (
            <Loader2 size={14} className="animate-spin" />
          ) : !dirty ? (
            <Check size={14} />
          ) : (
            <Save size={14} />
          )}
          <span>{saving ? "Saving..." : dirty ? "Save" : "Saved"}</span>
        </button>
        <button
          type="button"
          disabled={running || !flowId}
          onClick={async () => {
            if (!flowId) return;
            // Auto-save before deploy
            if (dirty) {
              await saveFlow(reactFlow.getViewport());
            }
            try {
              // Clear old mapping so new events get queued until fresh map arrives
              useEngineStore.getState().setNodeMap({});
              const res = await flowsApi.start(flowId);
              if (res.node_map) {
                useEngineStore.getState().setNodeMap(res.node_map);
              }
              // Log registered routes for visibility
              if (res.routes && res.routes.length > 0) {
                const port =
                  window.location.port === "5173"
                    ? "7700"
                    : window.location.port;
                const base = `${window.location.protocol}//${window.location.hostname}:${port}`;
                const routeList = res.routes
                  .map(
                    (r: { method: string; path: string }) =>
                      `  ${r.method} ${base}${r.path}`,
                  )
                  .join("\n");
                useEngineStore.getState().addLog({
                  type: "routes_registered",
                  flow_id: flowId,
                  routes: routeList,
                } as never);
              }
            } catch (err) {
              console.error("Deploy failed:", err);
            }
          }}
          className={`flex items-center gap-1.5 px-3 py-1.5 text-xs rounded-md transition-colors ${
            running
              ? "bg-green-800 text-green-300 cursor-wait"
              : "bg-green-600 hover:bg-green-700 text-white"
          }`}
        >
          {running ? (
            <Loader2 size={14} className="animate-spin" />
          ) : (
            <Play size={14} />
          )}
          <span>{running ? "Running..." : "Deploy"}</span>
        </button>
        <button
          type="button"
          disabled={!running || !flowId}
          onClick={async () => {
            if (!flowId) return;
            try {
              await flowsApi.stop(flowId);
            } catch (err) {
              console.error("Stop failed:", err);
            }
          }}
          className={`flex items-center gap-1.5 px-3 py-1.5 text-xs rounded-md transition-colors ${
            running
              ? "text-red-300 hover:bg-red-900/30"
              : "text-slate-600 cursor-not-allowed"
          }`}
        >
          <Square size={14} />
          <span>Stop</span>
        </button>
      </div>
    </header>
  );
}
