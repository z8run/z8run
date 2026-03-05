import { X, Settings, Zap, AlertCircle, CheckCircle2, Loader2, Circle } from "lucide-react";
import { useUIStore } from "@/stores/uiStore";
import { useFlowStore } from "@/stores/flowStore";
import type { Z8NodeData, NodeStatus } from "@/types/flow";
import { CATEGORY_COLORS, PORT_COLORS } from "@/types/flow";

const STATUS_BADGES: Record<NodeStatus, { label: string; color: string; Icon: React.ComponentType<{ size?: number; className?: string }> }> = {
  idle: { label: "Idle", color: "text-slate-500", Icon: Circle },
  running: { label: "Running", color: "text-blue-400", Icon: Loader2 },
  success: { label: "Success", color: "text-green-400", Icon: CheckCircle2 },
  error: { label: "Error", color: "text-red-400", Icon: AlertCircle },
  disabled: { label: "Disabled", color: "text-slate-600", Icon: Circle },
};

/** Convert camelCase or snake_case key to a human-friendly label */
function humanizeKey(key: string): string {
  return key
    .replace(/([a-z])([A-Z])/g, "$1 $2")
    .replace(/_/g, " ")
    .replace(/\b\w/g, (c) => c.toUpperCase());
}

export function ConfigPanel() {
  const selectedNodeId = useUIStore((s) => s.selectedNodeId);
  const configPanelOpen = useUIStore((s) => s.configPanelOpen);
  const closeConfigPanel = useUIStore((s) => s.closeConfigPanel);
  const nodes = useFlowStore((s) => s.nodes);
  const updateNodeData = useFlowStore((s) => s.updateNodeData);

  if (!configPanelOpen || !selectedNodeId) return null;

  const node = nodes.find((n) => n.id === selectedNodeId);
  if (!node) return null;

  const data = node.data as unknown as Z8NodeData;
  const color = CATEGORY_COLORS[data.category] ?? "#6366f1";
  const config = data.config ?? {};
  const inputs = data.inputs ?? [];
  const outputs = data.outputs ?? [];
  const status: NodeStatus = data.status ?? "idle";
  const statusBadge = STATUS_BADGES[status] ?? STATUS_BADGES.idle;
  const nodeType = String(data.nodeType ?? data.type ?? "unknown");

  const handleConfigChange = (key: string, value: unknown) => {
    updateNodeData(selectedNodeId, {
      config: { ...config, [key]: value },
    } as Partial<Z8NodeData>);
  };

  return (
    <div className="w-[340px] bg-slate-900 border-l border-slate-700 flex flex-col h-full">
      {/* Header with color accent */}
      <div className="border-b border-slate-700">
        <div
          className="h-1 w-full"
          style={{ backgroundColor: color }}
        />
        <div className="flex items-center justify-between p-3">
          <div className="flex items-center gap-2.5">
            <div
              className="w-8 h-8 rounded-lg flex items-center justify-center"
              style={{ backgroundColor: `${color}20`, border: `1px solid ${color}40` }}
            >
              <Zap size={16} style={{ color }} />
            </div>
            <div>
              <div className="text-sm font-medium text-slate-200">{data.label}</div>
              <div className="text-[10px] text-slate-500 font-mono">{nodeType}</div>
            </div>
          </div>
          <button
            type="button"
            onClick={closeConfigPanel}
            className="p-1.5 hover:bg-slate-700 rounded-lg transition-colors"
          >
            <X size={16} className="text-slate-400" />
          </button>
        </div>

        {/* Status badge */}
        <div className="px-3 pb-2.5 flex items-center gap-2">
          <div className={`flex items-center gap-1.5 text-[11px] ${statusBadge.color}`}>
            <statusBadge.Icon size={12} className={status === "running" ? "animate-spin" : ""} />
            <span>{statusBadge.label}</span>
          </div>
        </div>
      </div>

      {/* Config form */}
      <div className="flex-1 overflow-y-auto p-3 space-y-4">
        {/* Node Name */}
        <div>
          <label className="text-[10px] font-semibold text-slate-500 uppercase tracking-wider block mb-1">
            Node Name
          </label>
          <input
            type="text"
            value={data.label}
            onChange={(e) =>
              updateNodeData(selectedNodeId, { label: e.target.value } as Partial<Z8NodeData>)
            }
            className="w-full bg-slate-800 border border-slate-700 rounded-md px-3 py-1.5
              text-xs text-slate-200 focus:outline-none focus:border-z8-500 transition-colors"
          />
        </div>

        {/* Configuration section */}
        {Object.keys(config).length > 0 && (
          <div>
            <div className="flex items-center gap-2 mb-2">
              <Settings size={12} className="text-slate-500" />
              <span className="text-[10px] font-semibold text-slate-500 uppercase tracking-wider">
                Configuration
              </span>
            </div>
            <div className="space-y-2.5">
              {Object.entries(config).map(([key, value]) => (
                <div key={key}>
                  <label className="text-[10px] font-medium text-slate-400 block mb-1">
                    {humanizeKey(key)}
                  </label>
                  {typeof value === "string" && (key === "code" || value.length > 50) ? (
                    <textarea
                      value={String(value)}
                      onChange={(e) => handleConfigChange(key, e.target.value)}
                      rows={key === "code" ? 6 : 3}
                      className="w-full bg-slate-800 border border-slate-700 rounded-md px-3 py-1.5
                        text-xs text-slate-200 font-mono focus:outline-none focus:border-z8-500
                        transition-colors resize-y"
                      spellCheck={false}
                    />
                  ) : typeof value === "number" ? (
                    <input
                      type="number"
                      value={Number(value)}
                      onChange={(e) => handleConfigChange(key, Number(e.target.value))}
                      className="w-full bg-slate-800 border border-slate-700 rounded-md px-3 py-1.5
                        text-xs text-slate-200 focus:outline-none focus:border-z8-500 transition-colors"
                    />
                  ) : typeof value === "boolean" ? (
                    <div className="flex items-center gap-2">
                      <button
                        type="button"
                        onClick={() => handleConfigChange(key, !value)}
                        className={`
                          w-9 h-5 rounded-full transition-colors relative shrink-0
                          ${value ? "bg-z8-600" : "bg-slate-700"}
                        `}
                      >
                        <div
                          className={`
                            w-3.5 h-3.5 bg-white rounded-full absolute top-[3px] transition-transform
                            ${value ? "translate-x-[19px]" : "translate-x-[3px]"}
                          `}
                        />
                      </button>
                      <span className="text-[10px] text-slate-500">
                        {value ? "Enabled" : "Disabled"}
                      </span>
                    </div>
                  ) : (
                    <input
                      type="text"
                      value={String(value)}
                      onChange={(e) => handleConfigChange(key, e.target.value)}
                      className="w-full bg-slate-800 border border-slate-700 rounded-md px-3 py-1.5
                        text-xs text-slate-200 font-mono focus:outline-none focus:border-z8-500
                        transition-colors"
                    />
                  )}
                </div>
              ))}
            </div>
          </div>
        )}

        {/* Ports section */}
        {(inputs.length > 0 || outputs.length > 0) && (
          <div>
            <div className="flex items-center gap-2 mb-2">
              <Zap size={12} className="text-slate-500" />
              <span className="text-[10px] font-semibold text-slate-500 uppercase tracking-wider">
                Ports
              </span>
            </div>

            {inputs.length > 0 && (
              <div className="mb-2">
                <div className="text-[10px] text-slate-600 mb-1">Inputs</div>
                <div className="space-y-1">
                  {inputs.map((p) => (
                    <div
                      key={p.id}
                      className="flex items-center gap-2 px-2 py-1 bg-slate-800/50 rounded text-xs"
                    >
                      <div
                        className="w-2 h-2 rounded-full shrink-0"
                        style={{ backgroundColor: PORT_COLORS[p.type] ?? "#94A3B8" }}
                      />
                      <span className="text-slate-300">{p.name}</span>
                      <span className="text-slate-600 font-mono text-[10px] ml-auto">{p.type}</span>
                    </div>
                  ))}
                </div>
              </div>
            )}

            {outputs.length > 0 && (
              <div>
                <div className="text-[10px] text-slate-600 mb-1">Outputs</div>
                <div className="space-y-1">
                  {outputs.map((p) => (
                    <div
                      key={p.id}
                      className="flex items-center gap-2 px-2 py-1 bg-slate-800/50 rounded text-xs"
                    >
                      <div
                        className="w-2 h-2 rounded-full shrink-0"
                        style={{ backgroundColor: PORT_COLORS[p.type] ?? "#94A3B8" }}
                      />
                      <span className="text-slate-300">{p.name}</span>
                      <span className="text-slate-600 font-mono text-[10px] ml-auto">{p.type}</span>
                    </div>
                  ))}
                </div>
              </div>
            )}
          </div>
        )}
      </div>

      {/* Footer */}
      <div className="p-3 border-t border-slate-700">
        <div className="text-[10px] text-slate-600 font-mono truncate">
          ID: {selectedNodeId}
        </div>
      </div>
    </div>
  );
}
