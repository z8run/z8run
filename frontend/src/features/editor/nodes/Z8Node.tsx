import { useUIStore } from "@/stores/uiStore";
import type { Z8NodeData } from "@/types/flow";
import { CATEGORY_COLORS, PORT_COLORS } from "@/types/flow";
import { Handle, type NodeProps, Position, useReactFlow } from "@xyflow/react";
import {
  AlignLeft,
  Bot,
  Braces,
  Brain,
  Bug,
  Clock,
  Code,
  Database,
  FileText,
  Filter,
  Fingerprint,
  GitBranch,
  Globe,
  Image,
  Radio,
  Scissors,
  Send,
  Tags,
  Timer,
  Webhook,
  X,
} from "lucide-react";
import { memo, useCallback } from "react";

const ICON_MAP: Record<string, React.ComponentType<{ size?: number }>> = {
  Globe,
  Clock,
  Webhook,
  Code,
  Braces,
  Filter,
  Bug,
  Send,
  GitBranch,
  Timer,
  Database,
  Radio,
  Brain,
  Fingerprint,
  Tags,
  FileText,
  Scissors,
  AlignLeft,
  Bot,
  Image,
};

const STATUS_STYLES: Record<string, string> = {
  idle: "border-slate-600",
  running: "border-blue-400 shadow-blue-500/30 shadow-lg animate-pulse",
  success: "border-green-400 shadow-green-500/20 shadow-md",
  error: "border-red-500 shadow-red-500/20 shadow-md",
  disabled: "border-slate-700 opacity-50",
};

/*
 * Layout (px):
 *  Header  40px  |  Type label 22px  |  Port rows 22px each  |  Pad 6px
 *  Handle top = 40 + 22 + i*22 + 11
 */
const HEADER_H = 40;
const TYPE_H = 22;
const ROW_H = 22;
const PAD_BOTTOM = 6;

function getPortColor(portId: string, portType: string): string {
  if (portId === "error" || portId === "reject") return "#EF4444";
  return (PORT_COLORS as Record<string, string>)[portType] ?? "#94A3B8";
}

function Z8NodeComponent({ id, data, selected }: NodeProps) {
  try {
    const nodeData = data as unknown as Z8NodeData;
    const openConfigPanel = useUIStore((s) => s.openConfigPanel);
    const { deleteElements } = useReactFlow();

    const icon = nodeData?.icon ?? "Code";
    const category = nodeData?.category ?? "process";
    const label = nodeData?.label ?? nodeData?.type ?? "Node";

    const Icon = ICON_MAP[icon];
    const categoryColor =
      (CATEGORY_COLORS as Record<string, string>)[category] ?? "#6366f1";
    const status = nodeData?.status ?? "idle";
    const statusStyle = STATUS_STYLES[status] || STATUS_STYLES.idle;

    // Defensive: ensure arrays
    const inputs = Array.isArray(nodeData?.inputs) ? nodeData.inputs : [];
    const outputs = Array.isArray(nodeData?.outputs) ? nodeData.outputs : [];
    const maxPorts = Math.max(inputs.length, outputs.length);

    const handleDelete = useCallback(
      (e: React.MouseEvent) => {
        e.stopPropagation();
        deleteElements({ nodes: [{ id }] });
      },
      [id, deleteElements],
    );

    return (
      <div
        className={`
          relative bg-slate-800 rounded-lg border-2 min-w-[180px] cursor-pointer
          transition-all duration-150 group/node
          ${statusStyle}
          ${selected ? "ring-2 ring-z8-500 ring-offset-1 ring-offset-slate-950" : ""}
        `}
        onDoubleClick={() => openConfigPanel(id)}
      >
        {/* Delete button */}
        <button
          type="button"
          onClick={handleDelete}
          className="absolute -top-2 -right-2 w-5 h-5 rounded-full bg-red-600 hover:bg-red-500
            flex items-center justify-center opacity-0 group-hover/node:opacity-100
            transition-opacity z-10 shadow-md"
        >
          <X size={12} className="text-white" />
        </button>

        {/* Header */}
        <div
          className="flex items-center gap-2 px-3 rounded-t-md"
          style={{ height: HEADER_H, backgroundColor: `${categoryColor}15` }}
        >
          <div
            className="w-6 h-6 rounded flex items-center justify-center shrink-0"
            style={{ backgroundColor: `${categoryColor}30` }}
          >
            {Icon ? <Icon size={14} /> : null}
          </div>
          <span className="text-xs font-medium text-slate-200 truncate">
            {label}
          </span>
        </div>

        {/* Body */}
        <div style={{ paddingBottom: PAD_BOTTOM }}>
          {/* Type label */}
          <div className="px-3 flex items-center" style={{ height: TYPE_H }}>
            <span className="text-[10px] text-slate-500 font-mono">
              {nodeData?.type ?? "node"}
            </span>
          </div>

          {/* Port rows */}
          {Array.from({ length: maxPorts }, (_, i) => {
            const inp = inputs[i];
            const out = outputs[i];
            return (
              <div
                key={inp?.id ?? out?.id ?? `port-${i}`}
                className="flex items-center justify-between px-3 gap-4"
                style={{ height: ROW_H }}
              >
                <span
                  className="text-[9px] font-mono truncate"
                  style={{
                    color: inp ? getPortColor(inp.id, inp.type) : "transparent",
                  }}
                >
                  {inp?.name ?? ""}
                </span>
                <span
                  className="text-[9px] font-mono truncate text-right"
                  style={{
                    color: out ? getPortColor(out.id, out.type) : "transparent",
                  }}
                >
                  {out?.name ?? ""}
                </span>
              </div>
            );
          })}
        </div>

        {/* Input handles */}
        {inputs.map((port, i) => (
          <Handle
            key={`in-${port.id}`}
            type="target"
            position={Position.Left}
            id={port.id}
            title={port.name}
            style={{
              top: HEADER_H + TYPE_H + i * ROW_H + ROW_H / 2,
              background: getPortColor(port.id, port.type),
              width: 10,
              height: 10,
              border: "2px solid #1e293b",
            }}
          />
        ))}

        {/* Output handles */}
        {outputs.map((port, i) => (
          <Handle
            key={`out-${port.id}`}
            type="source"
            position={Position.Right}
            id={port.id}
            title={port.name}
            style={{
              top: HEADER_H + TYPE_H + i * ROW_H + ROW_H / 2,
              background: getPortColor(port.id, port.type),
              width: 10,
              height: 10,
              border: "2px solid #1e293b",
            }}
          />
        ))}
      </div>
    );
  } catch (err) {
    // Fallback: render a basic box so the node doesn't disappear
    return (
      <div className="bg-slate-800 rounded-lg border-2 border-red-500 p-3 min-w-[140px]">
        <span className="text-xs text-red-400">Error rendering node</span>
        <Handle
          type="target"
          position={Position.Left}
          id="input"
          style={{
            top: "50%",
            background: "#94A3B8",
            width: 10,
            height: 10,
            border: "2px solid #1e293b",
          }}
        />
        <Handle
          type="source"
          position={Position.Right}
          id="output"
          style={{
            top: "50%",
            background: "#94A3B8",
            width: 10,
            height: 10,
            border: "2px solid #1e293b",
          }}
        />
      </div>
    );
  }
}

export const Z8Node = memo(Z8NodeComponent);
