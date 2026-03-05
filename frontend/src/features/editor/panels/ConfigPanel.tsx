import { X, Settings, Zap, AlertCircle, CheckCircle2, Loader2, Circle, Plus, Trash2 } from "lucide-react";
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

/** Available switch operators with human-friendly labels */
const SWITCH_OPERATORS: { value: string; label: string }[] = [
  { value: "eq", label: "== (equals)" },
  { value: "neq", label: "!= (not equal)" },
  { value: "gt", label: "> (greater)" },
  { value: "lt", label: "< (less)" },
  { value: "gte", label: ">= (greater or equal)" },
  { value: "lte", label: "<= (less or equal)" },
  { value: "contains", label: "contains" },
  { value: "regex", label: "matches regex" },
  { value: "empty", label: "is empty" },
  { value: "notempty", label: "is not empty" },
  { value: "true", label: "is truthy" },
  { value: "false", label: "is falsy" },
];

interface SwitchRule {
  type: string;
  value?: string;
  port: string;
}

/** Inline rules editor for Switch nodes */
function SwitchRulesEditor({
  rules,
  outputs,
  onChange,
}: {
  rules: SwitchRule[];
  outputs: { id: string; name: string }[];
  onChange: (rules: SwitchRule[]) => void;
}) {
  const updateRule = (index: number, field: keyof SwitchRule, val: string) => {
    const updated = rules.map((r, i) => (i === index ? { ...r, [field]: val } : r));
    onChange(updated);
  };

  const addRule = () => {
    const nextPort = outputs.find(
      (o) => o.id !== "default" && !rules.some((r) => r.port === o.id)
    );
    onChange([
      ...rules,
      { type: "eq", value: "", port: nextPort?.id ?? outputs[0]?.id ?? "out1" },
    ]);
  };

  const removeRule = (index: number) => {
    onChange(rules.filter((_, i) => i !== index));
  };

  // Operators that don't need a value input
  const noValueOps = new Set(["empty", "notempty", "true", "false"]);

  return (
    <div className="space-y-2">
      {rules.map((rule, i) => (
        <div key={i} className="bg-slate-800/60 rounded-md p-2 space-y-1.5 border border-slate-700/50">
          <div className="flex items-center gap-1.5">
            <span className="text-[10px] text-slate-500 shrink-0 w-4">{i + 1}.</span>
            <select
              value={rule.type}
              onChange={(e) => updateRule(i, "type", e.target.value)}
              className="flex-1 bg-slate-800 border border-slate-700 rounded px-2 py-1
                text-[11px] text-slate-200 focus:outline-none focus:border-z8-500"
            >
              {SWITCH_OPERATORS.map((op) => (
                <option key={op.value} value={op.value}>
                  {op.label}
                </option>
              ))}
            </select>
            <button
              type="button"
              onClick={() => removeRule(i)}
              className="p-1 hover:bg-slate-700 rounded transition-colors"
            >
              <Trash2 size={11} className="text-slate-600 hover:text-red-400" />
            </button>
          </div>
          <div className="flex items-center gap-1.5 ml-4">
            {!noValueOps.has(rule.type) && (
              <input
                type="text"
                value={rule.value ?? ""}
                onChange={(e) => updateRule(i, "value", e.target.value)}
                placeholder="value"
                className="flex-1 bg-slate-800 border border-slate-700 rounded px-2 py-1
                  text-[11px] text-slate-200 font-mono focus:outline-none focus:border-z8-500"
              />
            )}
            <span className="text-[10px] text-slate-600 shrink-0">→</span>
            <select
              value={rule.port}
              onChange={(e) => updateRule(i, "port", e.target.value)}
              className="w-24 bg-slate-800 border border-slate-700 rounded px-2 py-1
                text-[11px] text-slate-200 focus:outline-none focus:border-z8-500"
            >
              {outputs
                .filter((o) => o.id !== "default")
                .map((o) => (
                  <option key={o.id} value={o.id}>
                    {o.name}
                  </option>
                ))}
            </select>
          </div>
        </div>
      ))}
      <button
        type="button"
        onClick={addRule}
        className="flex items-center gap-1.5 text-[11px] text-slate-500 hover:text-z8-400
          px-2 py-1 hover:bg-slate-800 rounded transition-colors w-full"
      >
        <Plus size={12} />
        <span>Add rule</span>
      </button>
    </div>
  );
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
              {Object.entries(config).map(([key, value]) => {
                // Switch rules get a dedicated editor
                if (key === "rules" && nodeType === "switch" && Array.isArray(value)) {
                  return (
                    <div key={key}>
                      <label className="text-[10px] font-medium text-slate-400 block mb-1.5">
                        Rules
                      </label>
                      <SwitchRulesEditor
                        rules={value as SwitchRule[]}
                        outputs={outputs}
                        onChange={(newRules) => handleConfigChange("rules", newRules)}
                      />
                    </div>
                  );
                }

                return (
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
                  ) : typeof value === "object" && value !== null ? (
                    <textarea
                      value={JSON.stringify(value, null, 2)}
                      onChange={(e) => {
                        try {
                          handleConfigChange(key, JSON.parse(e.target.value));
                        } catch {
                          // Don't update if invalid JSON — user is still typing
                        }
                      }}
                      rows={4}
                      className="w-full bg-slate-800 border border-slate-700 rounded-md px-3 py-1.5
                        text-xs text-slate-200 font-mono focus:outline-none focus:border-z8-500
                        transition-colors resize-y"
                      spellCheck={false}
                    />
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
                );
              })}
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
