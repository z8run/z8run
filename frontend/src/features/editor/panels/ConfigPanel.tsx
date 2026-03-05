import { X, Settings } from "lucide-react";
import { useUIStore } from "@/stores/uiStore";
import { useFlowStore } from "@/stores/flowStore";
import type { Z8NodeData } from "@/types/flow";
import { CATEGORY_COLORS } from "@/types/flow";

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
  const color = CATEGORY_COLORS[data.category];

  const handleConfigChange = (key: string, value: unknown) => {
    updateNodeData(selectedNodeId, {
      config: { ...data.config, [key]: value },
    } as Partial<Z8NodeData>);
  };

  return (
    <div className="w-[340px] bg-slate-900 border-l border-slate-700 flex flex-col h-full">
      {/* Header */}
      <div className="flex items-center justify-between p-3 border-b border-slate-700">
        <div className="flex items-center gap-2">
          <div
            className="w-6 h-6 rounded flex items-center justify-center"
            style={{ backgroundColor: `${color}30` }}
          >
            <Settings size={12} className="text-slate-300" />
          </div>
          <div>
            <div className="text-sm font-medium text-slate-200">{data.label}</div>
            <div className="text-[10px] text-slate-500 font-mono">{data.type}</div>
          </div>
        </div>
        <button
          type="button"
          onClick={closeConfigPanel}
          className="p-1 hover:bg-slate-700 rounded transition-colors"
        >
          <X size={16} className="text-slate-400" />
        </button>
      </div>

      {/* Config form */}
      <div className="flex-1 overflow-y-auto p-3 space-y-3">
        <div>
          <label className="text-[10px] font-semibold text-slate-500 uppercase tracking-wider">
            Node Name
          </label>
          <input
            type="text"
            value={data.label}
            onChange={(e) =>
              updateNodeData(selectedNodeId, { label: e.target.value } as Partial<Z8NodeData>)
            }
            className="w-full mt-1 bg-slate-800 border border-slate-700 rounded-md px-3 py-1.5
              text-xs text-slate-200 focus:outline-none focus:border-z8-500 transition-colors"
          />
        </div>

        {/* Dynamic config fields */}
        {Object.entries(data.config).map(([key, value]) => (
          <div key={key}>
            <label className="text-[10px] font-semibold text-slate-500 uppercase tracking-wider">
              {key}
            </label>
            {typeof value === "string" && value.length > 50 ? (
              <textarea
                value={String(value)}
                onChange={(e) => handleConfigChange(key, e.target.value)}
                rows={4}
                className="w-full mt-1 bg-slate-800 border border-slate-700 rounded-md px-3 py-1.5
                  text-xs text-slate-200 font-mono focus:outline-none focus:border-z8-500
                  transition-colors resize-y"
              />
            ) : typeof value === "number" ? (
              <input
                type="number"
                value={Number(value)}
                onChange={(e) => handleConfigChange(key, Number(e.target.value))}
                className="w-full mt-1 bg-slate-800 border border-slate-700 rounded-md px-3 py-1.5
                  text-xs text-slate-200 focus:outline-none focus:border-z8-500 transition-colors"
              />
            ) : typeof value === "boolean" ? (
              <div className="mt-1">
                <button
                  type="button"
                  onClick={() => handleConfigChange(key, !value)}
                  className={`
                    w-10 h-5 rounded-full transition-colors relative
                    ${value ? "bg-z8-600" : "bg-slate-700"}
                  `}
                >
                  <div
                    className={`
                      w-3.5 h-3.5 bg-white rounded-full absolute top-[3px] transition-transform
                      ${value ? "translate-x-[22px]" : "translate-x-[3px]"}
                    `}
                  />
                </button>
              </div>
            ) : (
              <input
                type="text"
                value={String(value)}
                onChange={(e) => handleConfigChange(key, e.target.value)}
                className="w-full mt-1 bg-slate-800 border border-slate-700 rounded-md px-3 py-1.5
                  text-xs text-slate-200 font-mono focus:outline-none focus:border-z8-500
                  transition-colors"
              />
            )}
          </div>
        ))}

        {/* Ports info */}
        {data.inputs.length > 0 && (
          <div>
            <label className="text-[10px] font-semibold text-slate-500 uppercase tracking-wider">
              Inputs
            </label>
            <div className="mt-1 space-y-1">
              {data.inputs.map((p) => (
                <div
                  key={p.id}
                  className="flex items-center gap-2 text-xs text-slate-400"
                >
                  <div
                    className="w-2 h-2 rounded-full"
                    style={{ backgroundColor: `var(--port-${p.type}, #94A3B8)` }}
                  />
                  <span>{p.name}</span>
                  <span className="text-slate-600 font-mono">{p.type}</span>
                </div>
              ))}
            </div>
          </div>
        )}

        {data.outputs.length > 0 && (
          <div>
            <label className="text-[10px] font-semibold text-slate-500 uppercase tracking-wider">
              Outputs
            </label>
            <div className="mt-1 space-y-1">
              {data.outputs.map((p) => (
                <div
                  key={p.id}
                  className="flex items-center gap-2 text-xs text-slate-400"
                >
                  <div
                    className="w-2 h-2 rounded-full"
                    style={{ backgroundColor: `var(--port-${p.type}, #94A3B8)` }}
                  />
                  <span>{p.name}</span>
                  <span className="text-slate-600 font-mono">{p.type}</span>
                </div>
              ))}
            </div>
          </div>
        )}
      </div>

      {/* Footer */}
      <div className="p-3 border-t border-slate-700">
        <div className="text-[10px] text-slate-600 font-mono">
          ID: {selectedNodeId}
        </div>
      </div>
    </div>
  );
}
